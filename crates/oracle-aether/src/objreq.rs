//! The game-defined **live-object mailbox** — `protocol.md` §6 rule (5), adopted as §11.32 (CR-J).
//!
//! Eight cells in game RAM through which `emulator/object_spawn`, `emulator/object_move` and
//! `emulator/object_delete` reach the engine. This module owns everything about that mailbox that does
//! **not** need the machine: resolving the cells, asserting their layout, composing the placement word,
//! and turning the engine's one status byte into a typed error. [`crate::engine`] owns the part that
//! does — the writes, the frame advance and the acknowledgement — as one indivisible operation.
//!
//! # Three properties this file exists to make structural
//!
//! 1. **Every cell is resolved by its own name, on every call, and never by an offset from another.**
//!    That is §11.32 J5, and it is a *safety* property rather than a style one: the cells live in an
//!    `if DEBUG == 1 @shape_divergent` block at the RAM tail, so a release build carries none of them
//!    and the resolution *fails*. An offset-based implementation would work beautifully against every
//!    DEBUG build and write fifteen bytes into whatever a release build put at the same addresses,
//!    silently, answering `result`. The offsets are therefore **not written down anywhere in this
//!    file** — not as a constant, not as a fallback, not in a comment as "for reference". A number a
//!    reader can copy is a number a later edit can use.
//!
//! 2. **The flag is written last, and the layout assertion is what keeps that meaningful.** Flag-last
//!    is the engine's entire concurrency control. A build that ever declared a ninth cell *after* the
//!    flag would keep every name and every width, resolve identically, and have silently lost it — so
//!    [`Mailbox::assert_layout`] refuses a mailbox whose cells are not contiguous in the published
//!    order with the flag highest. Aeon runs the equivalent gate in the tree that can change the
//!    layout; this one runs against the ROM a server was handed, which is the only place *this* side
//!    can catch it.
//!
//! 3. **A refusal by the game reaches the client as an error, never as a result field** (§8 item 25).
//!    [`status_error`] is total over the engine's status byte and has no "ok-ish" branch: `0` never
//!    reaches it, and every other value produces an `Err`. `{ "status": 3 }` in a result is a 200 with
//!    a sad face, and the engine made its refusals *silent* precisely so that a refused request could
//!    never fall back to guessing — which is the argument for making them loud here.

use crate::rpc::{code, RpcError};
use oracle_core::symbols::SymbolTable;
use serde_json::{json, Value};

/// One cell of the mailbox: the name the server resolves and the width it writes or reads.
pub struct Cell {
    pub name: &'static str,
    pub width: u32,
}

/// The mailbox's cells **in the engine's published declaration order**, which is also the order
/// [`Mailbox::assert_layout`] requires them to sit in.
///
/// The order is load-bearing twice over: it is the order the assertion checks, and its last entry is
/// the flag, which is the cell written last on every request.
pub const CELLS: &[Cell] = &[
    Cell {
        name: "Obj_Req_Def",
        width: 4,
    },
    Cell {
        name: "Obj_Req_X",
        width: 2,
    },
    Cell {
        name: "Obj_Req_Y",
        width: 2,
    },
    Cell {
        name: "Obj_Req_Slot",
        width: 2,
    },
    Cell {
        name: "Obj_Req_Place",
        width: 2,
    },
    Cell {
        name: "Obj_Req_Op",
        width: 1,
    },
    Cell {
        name: "Obj_Req_Status",
        width: 1,
    },
    Cell {
        name: "Obj_Req_Flag",
        width: 1,
    },
];

/// Index of each cell in [`CELLS`] and in [`Mailbox::addrs`]. Named rather than spelled as literals at
/// the call sites, so "the flag is the last one" is a single fact with a single home.
pub const DEF: usize = 0;
pub const X: usize = 1;
pub const Y: usize = 2;
pub const SLOT: usize = 3;
pub const PLACE: usize = 4;
pub const OP: usize = 5;
pub const STATUS: usize = 6;
pub const FLAG: usize = 7;

/// The engine's op codes. The server writes one of exactly these three and never accepts one from a
/// client, which is why an `ERR_OP` coming back is [`code::INTERNAL_ERROR`] and not the caller's fault.
pub const OP_SPAWN: u8 = 1;
pub const OP_MOVE: u8 = 2;
pub const OP_DELETE: u8 = 3;

/// The engine's status codes, valid exactly when the flag reads `0`.
pub const OK: u8 = 0;
pub const ERR_OP: u8 = 1;
pub const ERR_DEF: u8 = 2;
pub const ERR_FULL: u8 = 3;
pub const ERR_SLOT: u8 = 4;
pub const ERR_OWNED: u8 = 5;

/// The upper rail on a spawn's archetype pointer: the cart address window is `$000000-$3FFFFF`.
///
/// Pre-flighting this one rail (and only this one) is what §11.32 Q1 sanctions — *"the pre-flight
/// cart-window check already refuses the same fault with `-32602` before any write"*. The other three
/// rails (nonzero, even, a nonzero `ObjDef.code_addr`) are left to the engine deliberately: they are
/// facts about the bytes at the pointer, and a server that re-derived them would be a second opinion
/// that can disagree with the machine.
pub const CART_WINDOW_END: u32 = 0x0040_0000;

/// The two derived RAM words that carry **the act's true pixel extent**, and the only two this server
/// will accept as the answer to *is this placement inside the level* (§11.35, CR-L).
///
/// aeon published them for exactly this join, and their declaration states the box: *"the act's TRUE
/// pixel extent — the valid world box is `[0, Level_Width) × [0, Level_Height)`"*. They are written by
/// `Player_BoundsInit` from the values it holds **before** it subtracts its margins, and they exist in
/// both the release and the DEBUG shape — which is why a build with no `Obj_Req_*` mailbox can still
/// answer this question.
///
/// ⚠ **`Player_Bound_Right` / `Player_Bound_Bottom` are NOT these.** They are the *player's* clamp
/// edges, inset by `PBOUND_RIGHT_MARGIN` and `SCREEN_HEIGHT`; objects are deliberately unclamped, so a
/// placement between `Player_Bound_Right` and `Level_Width` is **legal and renders**. A server that
/// read the clamp edges would refuse real placements *and look correct doing it*, because the refusals
/// would cluster at an edge where a refusal is half expected — and it is the symbol a grep for "the
/// bounds" finds first, since the warp path clamps against it. There is no `Player_Bound_Left`/`_Top`
/// at all: the low edge of the box is a literal `0`.
///
/// ⚑ **Resolved by name, per call, independently of each other, never cached** — the rule §11.26 was
/// amended to impose on `Camera_X`. Measured on this box: `Level_Width` is `$FFFFBABE` in `s4.lst` and
/// `$FFFFE95C` in `s4.debug.lst`, ~11 KB apart, so a cached address does not fault in the other build
/// shape — it returns a number, and a number is what this check compares against.
pub const LEVEL_WIDTH_SYMBOL: &str = "Level_Width";
/// See [`LEVEL_WIDTH_SYMBOL`].
pub const LEVEL_HEIGHT_SYMBOL: &str = "Level_Height";

/// **The act's pixel extent, as read out of the machine on this call.** Never a constant: the one act
/// measured on this box reads `$1800 × $1800` (6144), and that is *one act's value*, not the engine's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActExtent {
    /// `Level_Width`, in world pixels. The box is half-open: `width` itself is **outside**.
    pub width: u32,
    /// `Level_Height`, in world pixels.
    pub height: u32,
}

impl ActExtent {
    /// Whether a world pixel is inside `[0, width) × [0, height)` — aeon's own words for the box.
    ///
    /// Half-open, and that is load-bearing at exactly one pixel per axis: `x == width` is the first
    /// column that is not in the act. The low edge needs no test — `x`/`y` arrive as unsigned 16-bit
    /// params and a negative world pixel cannot be represented.
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height
    }

    /// Whether these are the boot-cleared zeroes rather than a measurement.
    ///
    /// aeon's declaration: both words are *"boot-cleared with all Work RAM, so both read 0 until an act
    /// init has run"*. Either axis reading `0` is therefore **not an act of no size** — it is the
    /// absence of an act, and it gets its own sentence, because *"outside the level"* is a confusing
    /// thing to be told on a title screen. (Item 26 names the both-zero case; a half-zero extent is the
    /// same absence with one word already written, and answering `outsideAct` for it would be the same
    /// wrong sentence.)
    pub fn no_act_loaded(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// **The refusal for a placement outside a KNOWN act extent.**
    ///
    /// It says what the engine would have done, because that is the whole reason this refusal exists:
    /// aeon's `RunObjects` culls an out-of-act object on camera distance and does *nothing* — no error,
    /// no refusal, nothing on screen. Before this rail the row acked such a request as placed. A caller
    /// told only "refused" will reasonably assume the server is being fussy; one told the object would
    /// have been silently culled knows the refusal is the useful half.
    pub fn outside(&self, x: u32, y: u32) -> RpcError {
        RpcError::invalid_params(format!(
            "world ({x}, {y}) is outside the loaded act, whose extent is {} x {} pixels — the valid box \
             is [0, {}) x [0, {}), read just now from `{LEVEL_WIDTH_SYMBOL}` and \
             `{LEVEL_HEIGHT_SYMBOL}`. An object placed there is culled by the engine on camera distance \
             with no error and nothing on screen, so this is refused BEFORE anything is written rather \
             than acked and thrown away. Refused and never clamped: a clamp would report success at a \
             position the caller did not choose.",
            self.width, self.height, self.width, self.height
        ))
        .with_data(json!({
            "reason": "outsideAct",
            "x": x,
            "y": y,
            "actWidth": self.width,
            "actHeight": self.height,
        }))
    }

    /// **The refusal for a build whose listing cannot answer where the act ends.**
    ///
    /// The third option, and the one §11.35 adopts: not silently permitting (which is the defect
    /// itself, restored), not silently refusing (the row works perfectly inside a level and a silent
    /// refusal would read as a broken method), but **saying the check could not be made**. A
    /// measurement that cannot be taken is not a measurement of zero — the `arm`-refuses-with-no-
    /// archetypes precedent, applied to an extent.
    ///
    /// Both names or neither: half an extent is not a smaller measurement, and the half that *did*
    /// resolve is the more dangerous of the two, because a check on one axis looks like a check.
    pub fn unmeasurable(missing: &[&str]) -> RpcError {
        RpcError::invalid_params(format!(
            "this server cannot tell whether this placement is inside the act: {} not in the loaded \
             listing, so there is no measurement to check it against. An object placed outside the act \
             is culled by the engine with no error and nothing on screen, and a request sent unchecked \
             would be acked as placed and then vanish — so this refuses rather than treating the act as \
             infinite.",
            match missing {
                [one] => format!("`{one}` is"),
                _ => format!("`{LEVEL_WIDTH_SYMBOL}` and `{LEVEL_HEIGHT_SYMBOL}` are"),
            }
        ))
        .with_data(json!({
            "reason": "actExtentUnknown",
            "missing": missing,
        }))
    }

    /// **The refusal for a boot-cleared extent** — which is the absence of an act, not an act of no
    /// size.
    pub fn no_act() -> RpcError {
        RpcError::invalid_params(format!(
            "`{LEVEL_WIDTH_SYMBOL}` and `{LEVEL_HEIGHT_SYMBOL}` read 0, which is what they hold until an \
             act has initialised — there is no act for an object to be inside of, and anything placed \
             now would be culled the moment objects run. This is deliberately NOT `outsideAct`: there is \
             no level edge to be outside of yet, and saying so would send the caller hunting for one."
        ))
        .with_data(json!({"reason": "noActLoaded"}))
    }
}

/// The resolved mailbox: one address per [`CELLS`] entry, in that order.
#[derive(Debug)]
pub struct Mailbox {
    addrs: [u32; 8],
}

impl Mailbox {
    /// The address of one cell, by its [`DEF`]/[`FLAG`]/… index.
    pub fn at(&self, cell: usize) -> u32 {
        self.addrs[cell]
    }

    /// Every resolved address, in the published order — for `error.data` on a layout refusal.
    pub fn resolved(&self) -> Value {
        Value::Array(
            CELLS
                .iter()
                .zip(self.addrs.iter())
                .map(|(c, a)| json!({"name": c.name, "addr": crate::hex::addr(*a)}))
                .collect(),
        )
    }

    /// **§8.4's layout assertion.** The cells must be contiguous, in the published order, with the flag
    /// highest.
    ///
    /// A build that appended a cell *after* `Obj_Req_Flag` would keep every name and every width — so
    /// nothing above this function would notice, and the flag-last write would stop being the last
    /// write of the request. That is the whole concurrency control, so it is checked rather than
    /// assumed, against the ROM this server was actually handed.
    pub fn assert_layout(&self) -> Result<(), RpcError> {
        let mut want = self.addrs[0];
        for (i, c) in CELLS.iter().enumerate() {
            if self.addrs[i] != want {
                return Err(self.layout_refusal(c.name));
            }
            want = want.wrapping_add(c.width);
        }
        // Redundant with the walk above by construction, and stated anyway: the property the walk exists
        // to protect is *the flag is highest*, and a reader should be able to find that sentence.
        if self.addrs[FLAG] != *self.addrs.iter().max().expect("eight cells") {
            return Err(self.layout_refusal(CELLS[FLAG].name));
        }
        Ok(())
    }

    fn layout_refusal(&self, at: &str) -> RpcError {
        RpcError::invalid_state(
            "mailboxLayoutUnexpected",
            format!(
                "the live-object mailbox cells did not resolve contiguously in the published order with \
                 `{flag}` highest (first divergence at `{at}`). The flag being the last cell written is \
                 this protocol's entire concurrency control, so a layout that moved it is refused rather \
                 than written to.",
                flag = CELLS[FLAG].name,
            ),
            json!({ "resolved": self.resolved() }),
        )
    }
}

/// Resolve all eight cells **by name, individually**, or refuse naming what was absent.
///
/// * No table at all → [`code::NO_SYMBOLS_LOADED`]. *"You forgot `load_symbols`."*
/// * A table that resolves none or some of them → [`code::SYMBOL_NOT_FOUND`], with `data.missing`
///   listing **every** absent name rather than the first. A partial answer to *what is missing* invites
///   a fix-and-retry loop, and against a release ROM the honest answer is *all eight*.
///
/// The `-32013` message says the second thing out loud, because the client's next question is always
/// *why not*, and *this build has no live-object mailbox; it is a DEBUG-shape interface* is the answer.
///
/// ⚑ That answer covers a **release** ROM and not the other reality with the same symptom — a debug
/// build whose mailbox the loaded listing predates. `Engine::objreq_exchange` therefore appends
/// `Engine::with_symbol_freshness`'s clause to what this returns, which separates the two;
/// `F-LOOKUP-MISS-SAYS-NOTHING`. The clause is joined on by the caller rather than composed here so
/// there is exactly one implementation of it across all five `-32013` sites — and because this function
/// takes a table, not the engine that knows where the table came from.
pub fn resolve(table: Option<&SymbolTable>) -> Result<Mailbox, RpcError> {
    let Some(table) = table else {
        return Err(RpcError::new(
            code::NO_SYMBOLS_LOADED,
            "no symbol table is loaded, and this row resolves the live-object mailbox by symbol on every \
             call — call emulator/load_symbols first",
        ));
    };
    let mut addrs = [0u32; 8];
    let mut missing: Vec<&'static str> = Vec::new();
    for (i, c) in CELLS.iter().enumerate() {
        match table.address_of(c.name) {
            Some(a) => addrs[i] = a,
            None => missing.push(c.name),
        }
    }
    if !missing.is_empty() {
        return Err(RpcError::new(
            code::SYMBOL_NOT_FOUND,
            format!(
                "this build has no live-object mailbox: {} of the {} `Obj_Req_*` cells are absent from \
                 the loaded symbol table. The mailbox is a DEBUG-shape interface, so a release ROM \
                 resolves none of it — and nothing is written at a computed address when a name is \
                 missing.",
                missing.len(),
                CELLS.len(),
            ),
        )
        .with_data(json!({ "missing": missing })));
    }
    Ok(Mailbox { addrs })
}

/// Compose `Obj_Req_Place` from the structured params.
///
/// The raw word is deliberately not a param (§11.32 Q4): the engine masks it to `$60FF` **silently**, so
/// a client's stray bit would be a value the caller set and the machine never saw. Composing it here is
/// what makes every accepted bit meaningful, and what lets an out-of-range `subtype` be a `-32602`
/// instead of a truncation.
///
/// The flip bits are `OEF_XFLIP = 13` / `OEF_YFLIP = 14` (aeon `engine/system/constants.emp`), not the
/// architecture doc's unordered *"flips in bits 13/14"*.
pub fn place_word(subtype: u8, flip_h: bool, flip_v: bool) -> u16 {
    let mut w = u16::from(subtype);
    if flip_h {
        w |= 1 << 13;
    }
    if flip_v {
        w |= 1 << 14;
    }
    w
}

/// Context a status refusal needs to say anything useful beyond its code.
#[derive(Default)]
pub struct StatusContext {
    /// The `def` a spawn was refused for, already in its wire spelling.
    pub def: Option<String>,
    /// The handle a move/delete named, already in its wire spelling.
    pub handle: Option<String>,
    /// The dynamic pool's size, when `layout.pools` resolved it.
    pub dynamic_slots: Option<u32>,
}

/// **The engine's one status byte as a typed error.** Total over `1..=255`; `0` is not a refusal and
/// never reaches here.
///
/// `frames_advanced` rides in `data` on every one of them, because rule (5) requires it on every reply,
/// success and failure alike — a caller that is told *no* still has to know where its machine ended up.
pub fn status_error(status: u8, frames_advanced: u64, ctx: &StatusContext) -> RpcError {
    let frames = json!(frames_advanced);
    match status {
        ERR_DEF => {
            // §11.32 Q1: one fault, one code. The engine collapses four rails into this byte, so `data`
            // names all four rather than guessing which one failed — the honest shape of "which rail"
            // being unknowable.
            RpcError::invalid_params(format!(
                "the engine refused the archetype pointer{}: it must be nonzero, EVEN, inside the cart \
                 window ($000000-$3FFFFF) and point at a record whose head word (ObjDef.code_addr) is \
                 nonzero. The engine reports one code for all four, so which rail failed is not knowable \
                 from the machine.",
                ctx.def.as_ref().map(|d| format!(" {d}")).unwrap_or_default(),
            ))
            .with_data(json!({
                "def": ctx.def,
                "rails": ["nonzero", "even", "insideCartWindow", "nonzeroCodeAddr"],
                "framesAdvanced": frames,
            }))
        }
        ERR_FULL => RpcError::invalid_state(
            "objectPoolFull",
            "the dynamic object pool is exhausted, so nothing was spawned — and NOTHING WAS EVICTED: \
             the engine allocates from its own free stack and refuses rather than reclaiming a live \
             slot. Retrying harder will not help; the same request succeeds once a slot frees.",
            json!({"dynamicSlots": ctx.dynamic_slots, "framesAdvanced": frames}),
        ),
        ERR_SLOT => RpcError::invalid_state(
            "unknownSlot",
            "no live dynamic object has that handle. The engine cannot distinguish three realities here \
             and neither can this reply: the slot died, the handle never named a dynamic slot (a player, \
             system or effect handle reaches nothing here), or the handle is malformed.",
            json!({"handle": ctx.handle, "framesAdvanced": frames}),
        ),
        ERR_OWNED => RpcError::invalid_state(
            "slotOwnedByEntityWindow",
            "that slot belongs to the entity window, which clears the entity's loaded bit before it \
             deletes — so a bare delete here would leave the window believing the entity is still \
             spawned. It despawns on its own when its section stops being tracked, and \
             emulator/object_move IS allowed on it.",
            json!({"handle": ctx.handle, "framesAdvanced": frames}),
        ),
        ERR_OP => RpcError::new(
            code::INTERNAL_ERROR,
            "the engine did not recognise the op byte — which this server wrote, so this is our bug and \
             not the caller's. The row is unreachable by construction and is mapped rather than left \
             silently impossible.",
        )
        .with_data(json!({"framesAdvanced": frames})),
        other => RpcError::new(
            code::INTERNAL_ERROR,
            format!(
                "the engine answered status {other}, which this build's mapping does not know. The \
                 request was consumed; what it did is not knowable from here."
            ),
        )
        .with_data(json!({"status": other, "framesAdvanced": frames})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mailbox(addrs: [u32; 8]) -> Mailbox {
        Mailbox { addrs }
    }

    /// The cell table itself, because two other assertions read it as an ordered fact.
    #[test]
    fn the_flag_is_the_last_cell_of_the_published_order() {
        assert_eq!(CELLS.len(), 8);
        assert_eq!(CELLS[FLAG].name, "Obj_Req_Flag");
        assert_eq!(CELLS[DEF].name, "Obj_Req_Def");
        assert_eq!(CELLS.iter().map(|c| c.width).sum::<u32>(), 15);
    }

    /// A contiguous mailbox in the published order passes, and this is the *positive control* the two
    /// refusals below need in order to mean anything.
    #[test]
    fn a_contiguous_mailbox_with_the_flag_highest_passes() {
        let base = 0x00FF_E610;
        let mut a = [0u32; 8];
        let mut at = base;
        for (i, c) in CELLS.iter().enumerate() {
            a[i] = at;
            at += c.width;
        }
        mailbox(a).assert_layout().expect("the published layout");
    }

    /// **The mutation §8.4 exists for.** A ninth cell inserted before the flag keeps every name and
    /// every width and moves the flag off the end of the block.
    #[test]
    fn a_cell_inserted_before_the_flag_is_refused() {
        let base = 0x00FF_E610;
        let mut a = [0u32; 8];
        let mut at = base;
        for (i, c) in CELLS.iter().enumerate() {
            if i == FLAG {
                at += 2; // somebody's new u16, declared just before the flag
            }
            a[i] = at;
            at += c.width;
        }
        let e = mailbox(a).assert_layout().expect_err("must refuse");
        assert_eq!(e.code, code::INVALID_STATE);
        assert_eq!(
            e.data.as_ref().unwrap()["reason"],
            json!("mailboxLayoutUnexpected")
        );
        assert_eq!(
            e.data.as_ref().unwrap()["resolved"]
                .as_array()
                .unwrap()
                .len(),
            8,
            "the refusal names what it resolved, so the reader can see the shape it refused"
        );
    }

    /// The flag moved *below* the block — same names, same widths, the ack no longer last.
    #[test]
    fn a_flag_that_is_not_highest_is_refused() {
        let base = 0x00FF_E610;
        let mut a = [0u32; 8];
        let mut at = base + 1;
        for (i, c) in CELLS.iter().enumerate() {
            a[i] = at;
            at += c.width;
        }
        a[FLAG] = base; // relocated to the front
        let e = mailbox(a).assert_layout().expect_err("must refuse");
        assert_eq!(
            e.data.as_ref().unwrap()["reason"],
            json!("mailboxLayoutUnexpected")
        );
    }

    #[test]
    fn no_symbol_table_is_a_different_answer_from_a_release_rom() {
        let e = resolve(None).expect_err("must refuse");
        assert_eq!(e.code, code::NO_SYMBOLS_LOADED);
    }

    /// `$60FF` is the engine's mask; every bit this composes must survive it, and no other bit may be
    /// set. That is what makes the field TOTAL — the property the structured params buy.
    #[test]
    fn every_composed_placement_bit_survives_the_engines_mask() {
        for subtype in [0u8, 1, 0x7F, 0xFF] {
            for (h, v) in [(false, false), (true, false), (false, true), (true, true)] {
                let w = place_word(subtype, h, v);
                assert_eq!(w & !0x60FF, 0, "composed a bit the engine masks away");
                assert_eq!(w & 0x60FF, w);
                assert_eq!(w & 0x00FF, u16::from(subtype));
                assert_eq!(w & 0x2000 != 0, h);
                assert_eq!(w & 0x4000 != 0, v);
            }
        }
    }

    /// **§8 item 25's second half, at the unit level.** Every non-zero engine status is an `Err`, and
    /// the five that a client can meet carry the codes and discriminants §11.32 names. There is no
    /// branch that can turn one into a `result`.
    #[test]
    fn every_non_zero_engine_status_is_a_typed_error() {
        let ctx = StatusContext {
            def: Some("0x000A1C4".into()),
            handle: Some("0x8E62".into()),
            dynamic_slots: Some(48),
        };
        let cases: &[(u8, i64, Option<&str>)] = &[
            (ERR_OP, code::INTERNAL_ERROR, None),
            (ERR_DEF, code::INVALID_PARAMS, None),
            (ERR_FULL, code::INVALID_STATE, Some("objectPoolFull")),
            (ERR_SLOT, code::INVALID_STATE, Some("unknownSlot")),
            (
                ERR_OWNED,
                code::INVALID_STATE,
                Some("slotOwnedByEntityWindow"),
            ),
            (200, code::INTERNAL_ERROR, None),
        ];
        for (status, want_code, want_reason) in cases {
            let e = status_error(*status, 2, &ctx);
            assert_eq!(e.code, *want_code, "status {status}");
            let data = e.data.as_ref().expect("every one carries data");
            assert_eq!(
                data["framesAdvanced"],
                json!(2),
                "status {status}: framesAdvanced rides every failure too (rule (5))"
            );
            match want_reason {
                Some(r) => assert_eq!(data["reason"], json!(r), "status {status}"),
                None => assert!(
                    data.get("reason").is_none(),
                    "status {status}: only -32005 carries a reason"
                ),
            }
        }
    }

    /// The pool-full message must say *nothing was evicted*, because a client's next instinct is to
    /// retry harder, and the stale-handle message must not read as a malformed request.
    #[test]
    fn the_two_messages_a_user_will_actually_read_say_the_thing() {
        let ctx = StatusContext::default();
        assert!(status_error(ERR_FULL, 1, &ctx)
            .message
            .contains("NOTHING WAS EVICTED"));
        assert!(status_error(ERR_OWNED, 1, &ctx)
            .message
            .contains("object_move"));
    }
}
