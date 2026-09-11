//! **The Memory panel** — one hex view over five address spaces, with a space selector rather than five
//! tabs (design §2.1: five tabs would be five scroll positions to keep in your head).
//!
//! # The two routes, and where the line falls (design §4.4)
//!
//! * **Reads go direct.** [`view`] calls `oracle_aether::engine`'s free `debug_read` / `vdp_space_read` /
//!   `z80_read_window` — *the same functions the five read handlers call*, made free and public by this
//!   parcel for exactly this reason. The panel does not round-trip a repaint through JSON, and it does not
//!   own a second region decode, a second bound check or a second Z80 mirror fold to drift from theirs.
//! * **Every gesture goes through [`Bus::call`].** The address box's symbol acceptance, the write, and
//!   the hash button are one-per-human-action, so each is answered by `Engine::dispatch` and the panel
//!   renders **the tool's own reply and the tool's own refusal, verbatim**. It composes no error text of
//!   its own about a server it is living inside.
//!
//! # ⚑ The paused-write asymmetry is REFLECTED, not fixed
//!
//! `write_memory`, `write_cram` and `z80_write` refuse a running machine with `-32005 machineRunning`.
//! **`write_vram` does not** — engine.rs's handler carries no `require_paused`, and that is a documented
//! decision rather than an oversight: the row's contract fragment does not name it in §6's run-control
//! rule, and *"relaxing a refusal later is additive (D5); introducing one is not"*, so the server serves
//! the gate it was given and files the argument for one upstream (audit D-16, deviation 1).
//!
//! So a human here can poke VRAM mid-frame and is refused the identical gesture on work RAM. The panel
//! **shows that**, and gating VRAM for tidiness would be the same lie in the opposite direction: a panel
//! that refuses what the tool allows misdescribes the server just as surely as one that allows what the
//! tool refuses. The owner's standing rule is that a panel shows the same answer a tool gets, and the
//! asymmetry is a property of the answer.
//!
//! # How the panel knows, without a table it wrote down
//!
//! [`probe_write_gate`] asks the handler. It dispatches the write method with an **empty params object**
//! and reads the code back: `require_paused` is the first statement in all three gated handlers, so a
//! machine-running refusal arrives *before* any param is parsed and before any byte could land, while an
//! open gate falls through to the handler's own `-32602` about the missing payload. Nothing is written on
//! either path.
//!
//! That is derived rather than copied — no list of "these three are gated" exists in this crate — but it
//! is derived from a **check order**, and a check order can be edited. So it is not trusted on its own:
//! `the_panels_write_gate_agrees_with_what_a_real_write_actually_does` drives a real, well-formed write
//! into every space in both run states and asserts the gate predicted the outcome. If someone ever moves
//! `require_paused` below the param parsing, the probe goes quiet and that test goes red.

use oracle_aether::engine::{self, BusRegion, METHODS};
use oracle_aether::rpc::{code, RpcError};
use oracle_core::system::System;
use oracle_core::watchpoints::WatchSpace;
use serde_json::{json, Value};

use crate::bus::{Answer, Bus};

// ---------------------------------------------------------------------------------------------------
// The five spaces
// ---------------------------------------------------------------------------------------------------

/// One of the five address spaces the Memory panel shows.
///
/// **`bus` and `rom+ram` are one space here, and the design doc's five-way list is wrong about that.**
/// `emulator/read {space:"bus"}` and `emulator/read_memory` both resolve through the *same*
/// `debug_read`, over the same ROM and work-RAM windows, returning the same bytes and the same region
/// label. Two selector entries for one derivation would be the believable wrong answer parcel 2a
/// refused for `A7`/`SP` — a reader takes two rows for two things. The fifth space is `vsram` instead,
/// which `emulator/read` serves and the doc's list dropped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Space {
    /// The 68000 bus: cartridge ROM and the work-RAM window. The only space with a region label and the
    /// only one a symbol can name.
    Bus,
    Vram,
    Cram,
    Vsram,
    Z80,
}

impl Space {
    pub const ALL: [Space; 5] = [
        Space::Bus,
        Space::Vram,
        Space::Cram,
        Space::Vsram,
        Space::Z80,
    ];

    /// The selector's label. The parenthetical on `bus` is what stops a reader hunting for a separate
    /// "rom+ram" entry that would show the identical bytes.
    pub fn label(self) -> &'static str {
        match self {
            Space::Bus => "bus (ROM + work RAM)",
            Space::Vram => "vram",
            Space::Cram => "cram",
            Space::Vsram => "vsram",
            Space::Z80 => "z80",
        }
    }

    /// The served method whose answer this space's hex view reproduces. Named on the panel so a human can
    /// ask the same question from a tool and know which row to reach for.
    pub fn read_method(self) -> &'static str {
        match self {
            // `emulator/read_memory` is the same bytes through the same `debug_read`; `read` is named
            // because it is the row that takes a `space`, which is what this selector is.
            Space::Bus => "emulator/read",
            Space::Vram => "emulator/read_vram",
            Space::Cram => "emulator/read",
            Space::Vsram => "emulator/read",
            Space::Z80 => "emulator/z80_read",
        }
    }

    /// The served method a write in this space would go through, or `None` where the surface has none.
    ///
    /// **`Vsram` is `None` and that is the served surface, not a gap in this panel**: there is no
    /// `emulator/write_vsram` row. Checked, not assumed — [`write_gate`] looks the name up in [`METHODS`]
    /// rather than trusting this table, so a row added upstream turns the cell on by itself.
    pub fn write_method(self) -> Option<&'static str> {
        match self {
            Space::Bus => Some("emulator/write_memory"),
            Space::Vram => Some("emulator/write_vram"),
            Space::Cram => Some("emulator/write_cram"),
            Space::Vsram => None,
            Space::Z80 => Some("emulator/z80_write"),
        }
    }

    /// This space as **`emulator/read`'s `space` param** names it, or `None` for the one selector entry
    /// that method does not serve.
    ///
    /// `Space::Z80` is `None` and that is the served surface, not a gap in this panel: the Z80 bus is
    /// [`Space::read_method`]'s own row (`emulator/z80_read`), and `emulator/read`'s `space` enum has
    /// four members. Returning a [`ReadSpace`] rather than a `&str` is what makes the fifth space
    /// unable to reach a call that cannot carry it — see [`ReadSpace`].
    fn as_read_space(self) -> Option<ReadSpace> {
        match self {
            Space::Bus => Some(ReadSpace::Bus),
            Space::Vram => Some(ReadSpace::Vram),
            Space::Cram => Some(ReadSpace::Cram),
            Space::Vsram => Some(ReadSpace::Vsram),
            Space::Z80 => None,
        }
    }

    /// The space's size **read off the machine**, or `None` for the bus — which has no single end, only
    /// two windows with a hole between them, and whose edges the read itself refuses at.
    pub fn len(self, sys: &System) -> Option<usize> {
        match self {
            Space::Bus => None,
            Space::Vram => Some(sys.vram().len()),
            Space::Cram => Some(sys.vdp().cram().len()),
            Space::Vsram => Some(sys.vdp().vsram().len()),
            // The Z80's *window*, which is twice its RAM: `$2000-$3FFF` mirrors `$0000-$1FFF` and the
            // mirror is the machine (see `z80_read_window`). Showing only 8 KB would hide half the
            // addresses `emulator/z80_read` accepts.
            Space::Z80 => Some(0x4000),
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Reads — route (a), through the handlers' own functions
// ---------------------------------------------------------------------------------------------------

/// `len` bytes of `space` at `addr`, plus the region label on the one space that has one.
///
/// Every branch is a call into `oracle_aether::engine`'s free functions — the identical code
/// `emulator/read`, `emulator/read_memory`, `emulator/read_vram` and `emulator/z80_read` run. The
/// refusals are theirs too, which is why this returns [`RpcError`] rather than a `String`: the panel
/// shows the message a tool would have been given, down to the wording.
pub fn read(
    space: Space,
    sys: &System,
    addr: u32,
    len: usize,
) -> Result<(Vec<u8>, Option<BusRegion>), RpcError> {
    match space {
        Space::Bus => engine::debug_read(sys, addr, len).map(|(b, r)| (b, Some(r))),
        Space::Vram => {
            engine::vdp_space_read(sys, WatchSpace::Vram, addr, len as u64).map(|b| (b, None))
        }
        Space::Cram => {
            engine::vdp_space_read(sys, WatchSpace::Cram, addr, len as u64).map(|b| (b, None))
        }
        Space::Vsram => {
            engine::vdp_space_read(sys, WatchSpace::Vsram, addr, len as u64).map(|b| (b, None))
        }
        Space::Z80 => engine::z80_read_window(sys, addr, len).map(|b| (b, None)),
    }
}

/// One line of the hex view.
pub struct HexRow {
    pub addr: u32,
    pub bytes: Vec<u8>,
}

impl HexRow {
    /// `"00 1A FF …"`, and the ASCII gutter beside it.
    pub fn hex(&self) -> String {
        self.bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn ascii(&self) -> String {
        self.bytes
            .iter()
            .map(|&b| {
                if (0x20..0x7F).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect()
    }
}

/// What the panel draws for one repaint: either a page of rows, or the read's own refusal.
pub struct View {
    pub base: u32,
    /// The bus space's region — the handler's own [`BusRegion`], printed through its one spelling
    /// (`BusRegion::label`), so a remapped window shows `cartridge ROM bank N` here exactly as a tool is told.
    pub region: Option<BusRegion>,
    pub rows: Vec<HexRow>,
    /// The refusal, verbatim, when the requested page does not exist. Never rendered as an empty grid: a
    /// blank hex view and a refused read look identical, and only one of them means "there is nothing
    /// here".
    pub error: Option<RpcError>,
    /// Set when the page was **shortened because the space ends**, so the view says so rather than
    /// silently showing fewer rows than were asked for. A short page that does not announce itself is the
    /// clipped-read failure in a different costume.
    pub truncated_to: Option<usize>,
}

/// A page of `rows × per_row` bytes at `base`, clipped **loudly** to the end of a sized space.
pub fn view(space: Space, sys: &System, base: u32, rows: usize, per_row: usize) -> View {
    let want = rows * per_row;
    let (len, truncated_to) = match space.len(sys) {
        Some(size) => {
            let left = size.saturating_sub(base as usize);
            if left < want {
                (left, Some(left))
            } else {
                (want, None)
            }
        }
        // The bus has no single end; the read refuses at the region edge and that refusal is the answer.
        None => (want, None),
    };
    if len == 0 {
        return View {
            base,
            region: None,
            rows: Vec::new(),
            error: Some(RpcError::new(
                code::ADDRESS_OUT_OF_RANGE,
                format!(
                    "{} is at or past the end of {} ({} bytes)",
                    oracle_aether::hex::addr(base),
                    space.label(),
                    space.len(sys).unwrap_or(0)
                ),
            )),
            truncated_to: None,
        };
    }
    match read(space, sys, base, len) {
        Ok((bytes, region)) => View {
            base,
            region,
            rows: bytes
                .chunks(per_row)
                .enumerate()
                .map(|(i, c)| HexRow {
                    addr: base.wrapping_add((i * per_row) as u32),
                    bytes: c.to_vec(),
                })
                .collect(),
            error: None,
            truncated_to,
        },
        Err(e) => View {
            base,
            region: None,
            rows: Vec::new(),
            error: Some(e),
            truncated_to: None,
        },
    }
}

// ---------------------------------------------------------------------------------------------------
// The write gate — asked of the handler, never written down
// ---------------------------------------------------------------------------------------------------

/// Why a write cell is enabled or disabled, **in the words a tool would have been given**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gate {
    /// The handler would take this gesture right now.
    Open { method: &'static str },
    /// The handler refused for the machine's state. `reason` is the `-32005` discriminant
    /// (`machineRunning`); `message` is the handler's own sentence, shown to the human unchanged.
    Refused {
        method: &'static str,
        code: i64,
        reason: String,
        message: String,
    },
    /// The panel names a method the served surface does not carry.
    Unserved { method: &'static str },
    /// No method writes this space at all. Distinct from [`Unserved`](Gate::Unserved) because the two are
    /// different facts: one is a name that is wrong, the other is a capability that does not exist.
    NoMethod,
}

impl Gate {
    pub fn is_open(&self) -> bool {
        matches!(self, Gate::Open { .. })
    }

    /// The sentence the disabled cell shows. **Never blank and never a bare greyed box** — a control that
    /// does nothing and says nothing is indistinguishable from one that is broken.
    pub fn why(&self) -> String {
        match self {
            Gate::Open { method } => format!("writes go to {method}"),
            Gate::Refused {
                method,
                code,
                reason,
                message,
            } => format!("{method} refuses right now ({code} {reason}): {message}"),
            Gate::Unserved { method } => format!(
                "this build serves no method named {method}, so the panel has nothing to write through"
            ),
            Gate::NoMethod => "no served method writes this space: the bus has read rows for vsram \
                               (emulator/read) and no write row at all, so this is the surface's own \
                               limit and not a missing control"
                .into(),
        }
    }
}

/// Whether the surface carries `name` at all — from the dispatch table itself, which `initialize` also
/// builds its advertised `methods` array from, so the two cannot disagree.
pub fn is_served(name: &str) -> bool {
    METHODS.iter().any(|m| m.name == name)
}

/// **Ask the handler whether it would take a write right now**, without writing anything.
///
/// Dispatches the space's write method with `{}`. In all three gated handlers `require_paused` is the
/// first statement, so:
///
/// * a running machine answers `-32005 machineRunning` before a single param is looked at;
/// * a paused machine (and `write_vram` in either state) falls through to that handler's own `-32602`
///   about the payload it was not given.
///
/// Neither path reaches a `write8`, a `poke_vram`, a `poke_cram` or a `z80_ram_mut`. The probe is a
/// question, and the answer is the handler's.
pub fn probe_write_gate(bus: &mut Bus, sys: &mut System, space: Space) -> Gate {
    let Some(method) = space.write_method() else {
        return Gate::NoMethod;
    };
    if !is_served(method) {
        return Gate::Unserved { method };
    }
    match bus.call(sys, method, &json!({})) {
        Answer::Err(e) if e.code == code::INVALID_STATE => Gate::Refused {
            method,
            code: e.code,
            reason: e
                .data
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or("(the refusal carried no reason discriminant)")
                .to_string(),
            message: e.message,
        },
        // Any other outcome — a params refusal, or (impossible today) a success — means the state gate
        // did not fire, which is exactly what "open" means.
        _ => Gate::Open { method },
    }
}

/// The five gates, in [`Space::ALL`] order.
pub fn probe_all_gates(bus: &mut Bus, sys: &mut System) -> [Gate; 5] {
    [
        probe_write_gate(bus, sys, Space::Bus),
        probe_write_gate(bus, sys, Space::Vram),
        probe_write_gate(bus, sys, Space::Cram),
        probe_write_gate(bus, sys, Space::Vsram),
        probe_write_gate(bus, sys, Space::Z80),
    ]
}

// ---------------------------------------------------------------------------------------------------
// Writes — the params each row actually takes
// ---------------------------------------------------------------------------------------------------

/// Build the params for a write of `payload` (a hex byte string) at `addr` in `space`.
///
/// **CRAM is not addressed in bytes and the panel does not pretend otherwise.** `emulator/write_cram`
/// takes `line` / `index` / `raw`, because a palette entry is a 9-bit word and a byte-wide poke into one
/// is not a thing the chip has. So a CRAM write cell is a *word* cell: the byte address selects the
/// entry (`line = addr / 32`, `index = (addr % 32) / 2` — the inverse of the `cramAddr = entry × 2` the
/// read side publishes as its join key) and the payload is that entry's raw word. An odd address is
/// refused here rather than silently rounded down, which would write a neighbouring colour and report
/// success.
///
/// Errors are the panel's own only where the *panel's* input is malformed; everything the server can
/// judge is left to the server.
pub fn write_params(space: Space, addr: u32, payload: &str) -> Result<Value, String> {
    let clean = payload.trim().trim_start_matches("0x").replace(' ', "");
    if clean.is_empty() {
        return Err("nothing to write: type hex bytes".into());
    }
    if !clean.len().is_multiple_of(2) || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "{payload:?} is not a whole number of hex bytes (two digits each)"
        ));
    }
    match space {
        // The `0x` is not decoration: D9 category 1 makes it required, and `hex::parse_bytes` refuses a
        // bare `"AA"` with *"`bytes` must start with \"0x\" or \"$\""*. A panel that sent the digits
        // alone would show a human a `-32602` about their own perfectly good input.
        Space::Bus | Space::Vram | Space::Z80 => Ok(json!({
            "addr": oracle_aether::hex::addr(addr),
            "bytes": format!("0x{}", clean.to_uppercase()),
        })),
        Space::Cram => {
            if clean.len() != 4 {
                return Err(format!(
                    "a CRAM entry is one 9-bit word: give exactly two bytes (four hex digits), not {}",
                    clean.len() / 2
                ));
            }
            if !addr.is_multiple_of(2) {
                return Err(format!(
                    "{} is an odd byte address and a CRAM entry is two bytes wide, refused rather \
                     than rounded down onto the neighbouring colour",
                    oracle_aether::hex::addr(addr)
                ));
            }
            let raw = u16::from_str_radix(&clean, 16).map_err(|e| e.to_string())?;
            // **All three are JSON numbers, not hex strings.** D9 splits the vocabulary: an *address or
            // payload* is a `"0x…"` string (category 1), a *count, index or bounded value* is a number
            // (category 2), and `line`/`index`/`raw` are all category 2 — `parse_cram_line` refuses a
            // string outright and `raw` answers *"must be a non-negative integer (D9 category 2)"*. The
            // cell above sends `bytes` as a string in the same breath, which is why this is worth
            // stating rather than looking like an inconsistency.
            //
            // `raw` is also refused, never masked, for bits outside the chip's `0x0EEE` — the panel does
            // not pre-mask it, because a colour silently changed on the way to the chip is exactly the
            // wrong answer that refusal exists to make impossible.
            Ok(json!({
                "line": addr / 32,
                "index": (addr % 32) / 2,
                "raw": raw,
            }))
        }
        Space::Vsram => Err(Gate::NoMethod.why()),
    }
}

// ---------------------------------------------------------------------------------------------------
// The address box — which *is* `emulator/lookup_symbol` (design §2.2)
// ---------------------------------------------------------------------------------------------------

/// What the address box did with what was typed.
pub enum Resolved {
    /// A hex literal, parsed here. No dispatch: `"0x1234"` is not a question the server can answer better.
    Hex {
        /// The address to read: **the one the machine puts on the bus**, and the field to compute with.
        addr: u32,
        /// ⚑ **The 68000 listing's own spelling, present only when it differs from `addr`** — i.e. only
        /// when the typed value carried bits above the 24 the 68000 actually drives, and only in
        /// [`Space::Bus`].
        ///
        /// An Aeon listing writes work RAM as `FFFF8CFA`; the machine drives 24 address lines and puts
        /// `$FF8CFA` on the bus. `emulator/lookup_symbol` has always answered **both**
        /// (`{"addr":"0x00FF8000","rawAddr":"0xFFFF8000"}`), so typing the *symbol* `VBlank_Flag` read
        /// fine while typing its address by hand was refused — one field, two behaviours, neither
        /// explained. This carries the same pair for the number, so the panel can say *which* location it
        /// went to and *why* that is the same location, rather than either refusing a spelling a person
        /// read out of the listing in front of them or silently reading somewhere they did not name.
        listing: Option<u32>,
    },
    /// A name the server resolved. Carries the server's own reply so the panel can show the symbol it
    /// actually landed on, its displacement, and any `caveat` the server attached.
    Symbol { addr: u32, reply: Value },
    /// The server's refusal, verbatim.
    Refused(RpcError),
    /// The panel's own refusal, for input the server never sees.
    Rejected(String),
}

/// Resolve what a human typed into the address box, **through the served surface**.
///
/// * A hex literal is taken as one — **in [`Space::Bus`], through the same 24-bit mask the symbol path
///   has always applied**, with the listing spelling kept beside it. See below.
/// * Anything else is a name, and the name goes to the server:
///   * in the **bus** space, to `emulator/lookup_symbol`, which is what the design means by *"the address
///     box **is** this method"*;
///   * in every other space, to `emulator/read {space, symbol}` — which refuses, and refuses **in the
///     server's own words**: *"`symbol` is valid only with space \"bus\" — a VDP-internal byte address
///     has no symbol"*. That refusal is the answer, and asking for it costs one non-mutating read rather
///     than a second sentence this panel would have had to write and keep in step.
///
/// # ⚑ One field, two behaviours — and this is the half that was wrong
///
/// `emulator/lookup_symbol` masks: `VBlank_Flag` answers `{"addr":"0x00FF8000","rawAddr":"0xFFFF8000"}`
/// and reads. Typing `0xFFFF8000` into the *same box* was refused. So the address box did the masking
/// silently for a name and refused it for a number, explaining neither — and the refused spelling is the
/// one an Aeon listing puts in front of a person (`FFFF8CFA` for work RAM; the 68000 drives 24 address
/// lines and puts `$FF8CFA` on them). The panel's own default was that spelling, which is how the Memory
/// tab came to greet a first-time click with a refusal of a value it had pre-filled itself.
///
/// **The mask here is not the wire's rule, and the difference is the point.**
/// `bus-protocol.schema.json`'s `rawAddr` note pins the wire behaviour: passing the listing spelling to
/// `read_memory` is *"refused loudly … so a joining client that takes the wrong field pays a round trip,
/// never gets a wrong byte"*. That is a rule about a **client**, which cannot read an explanation and
/// must not guess. A person at this box is the other case: they read the number off a listing, and there
/// is exactly one location it can mean. So this masks — and then **says so, on screen, naming both
/// spellings** ([`Resolved::Hex::listing`]). Loud, which is what the schema note is actually protecting,
/// rather than refusing. Nothing is silent, and no byte comes from an address the panel did not name.
///
/// **[`Space::Bus`] only.** VRAM, CRAM, VSRAM and Z80 addresses are not 68000 bus addresses; masking one
/// with a 24-bit bus mask would be arithmetic about the wrong machine, and those spaces have their own
/// ranges and refuse out of them in their own words.
pub fn resolve_address(bus: &mut Bus, sys: &mut System, space: Space, text: &str) -> Resolved {
    let t = text.trim();
    if t.is_empty() {
        return Resolved::Rejected("type an address or a symbol name".into());
    }
    let hex = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix('$'))
        .unwrap_or(t);
    if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return match u32::from_str_radix(hex, 16) {
            Ok(a) if space == Space::Bus && a & !oracle_core::symbols::BUS_ADDR_MASK != 0 => {
                // The mask is the core's own constant, not a literal typed here: it is the same
                // `BUS_ADDR_MASK` `SymbolTable` applies on the path that already worked, so the two
                // spellings of one address cannot drift apart in the one box that shows both.
                Resolved::Hex {
                    addr: a & oracle_core::symbols::BUS_ADDR_MASK,
                    listing: Some(a),
                }
            }
            Ok(a) => Resolved::Hex {
                addr: a,
                listing: None,
            },
            Err(e) => Resolved::Rejected(format!("{t:?}: {e}")),
        };
    }
    if space != Space::Bus {
        // H29. The one space `emulator/read` does not name gets the panel's own sentence, because no
        // server refusal here says the right thing: sending `{"space":"z80"}` is refused *about the
        // space* and never mentions symbols. This is the arm `space_wire`'s doc said callers must not
        // reach, and its sole caller reached it on every Z80 gesture; now the type has no variant to
        // reach it with. See [`no_symbol_door`].
        let Some(read_space) = space.as_read_space() else {
            return Resolved::Rejected(no_symbol_door(space));
        };
        // Deliberately a real call. See the doc above.
        return match bus.call(
            sys,
            "emulator/read",
            &json!({"space": read_space.wire(), "symbol": t, "len": 1}),
        ) {
            Answer::Err(e) => Resolved::Refused(e),
            // Unreachable today (the handler refuses a non-bus symbol outright), and if it ever stops
            // being unreachable the honest thing is to use the answer rather than to assert about it.
            Answer::Ok(v) => match v.get("addr").and_then(Value::as_str) {
                Some(s) => match u32::from_str_radix(s.trim_start_matches("0x"), 16) {
                    Ok(a) => Resolved::Symbol { addr: a, reply: v },
                    Err(e) => Resolved::Rejected(format!("{s:?}: {e}")),
                },
                None => Resolved::Rejected(format!("the reply carried no `addr`: {v}")),
            },
        };
    }
    match bus.call(sys, "emulator/lookup_symbol", &json!({"name": t})) {
        Answer::Err(e) => Resolved::Refused(e),
        Answer::Ok(v) => match v.get("addr").and_then(Value::as_str) {
            Some(s) => match u32::from_str_radix(s.trim_start_matches("0x"), 16) {
                Ok(a) => Resolved::Symbol { addr: a, reply: v },
                Err(e) => Resolved::Rejected(format!("{s:?}: {e}")),
            },
            // A prefix search answers `matches`, not `addr` — a real reply shape, not an error, and the
            // panel says which one it got instead of showing nothing.
            None => Resolved::Rejected(format!(
                "{t:?} is not an exact name; the server answered a search instead: {v}"
            )),
        },
    }
}

/// **The four spaces `emulator/read`'s `space` param can name** — a type, and not a fifth arm on
/// [`Space`], because the rule this replaces was one a caller had to remember.
///
/// What was here before was `space_wire(Space) -> &'static str`, carrying the doc *"`Z80` has none …
/// and callers must not reach here with it"* beside a **sole caller that reached there with it on every
/// Z80 gesture**. `Space::Z80` is a live selector entry, so the Memory panel's Z80 space sent
/// `{"space": "z80"}` to `emulator/read` and the human got `-32602 \`space\` must be one of "bus",
/// "vram", "cram", "vsram"` — a refusal *about the space*, where the sentence the doc promised is about
/// the symbol. The comment was right about the rule and the code could not keep it.
///
/// This makes the bad call unrepresentable instead of forbidden: [`ReadSpace::wire`] is **total**, and
/// the one space that has no spelling has no variant, so the only way to reach the call is through
/// [`Space::as_read_space`], which hands back `None` for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ReadSpace {
    Bus,
    Vram,
    Cram,
    Vsram,
}

impl ReadSpace {
    /// The wire spelling of `emulator/read`'s `space` param. Total by construction — there is no
    /// variant here this method cannot name, which is the whole reason the type exists.
    fn wire(self) -> &'static str {
        match self {
            ReadSpace::Bus => "bus",
            ReadSpace::Vram => "vram",
            ReadSpace::Cram => "cram",
            ReadSpace::Vsram => "vsram",
        }
    }
}

/// **Why a name has no door in a space `emulator/read` does not serve** — read off [`METHODS`] rather
/// than typed, on [`hash_gate`]'s pattern and for [`hash_gate`]'s reason: this is a fact about the
/// served surface, so a surface that moves must move the sentence with it.
///
/// This is a [`Resolved::Rejected`], which is *"the panel's own refusal, for input the server never
/// sees"* — and it is the one place in [`resolve_address`] that composes a sentence rather than passing
/// one through, because **no server refusal says the right thing here**. `emulator/read` has no z80
/// space at all, so a real call refuses about the space and never mentions symbols, and
/// `emulator/z80_read` declares no `symbol` key, so a real call there is refused by §2.5's params
/// closure for a key the human never typed. Both are true sentences about the wrong question.
fn no_symbol_door(space: Space) -> String {
    let name = space.read_method();
    let Some(spec) = METHODS.iter().find(|m| m.name == name) else {
        return format!(
            "this build serves no {name}, which is the row that reads the {} space, so there is \
             nothing here to resolve a name against. Type a hex address",
            space.label()
        );
    };
    if spec.params.contains(&"symbol") {
        // **Loud on unmeasurable.** A future revision that adds the key means a name CAN be resolved
        // here and this panel has not been taught how; saying "there is no door" would then be a
        // plausible answer instead of an honest one, which is the failure `hash_gate` names.
        return format!(
            "{name} now declares a `symbol` param (its keys are {:?}), so the {} space has a name door \
             this panel has not been taught to walk through. Type a hex address, and report this line",
            spec.params,
            space.label()
        );
    }
    format!(
        "the {} space is read by {name}, which declares no `symbol` param (its keys are {:?}): only the \
         68000 bus has symbols. Type a hex address, or switch the selector to `bus` to resolve a name",
        space.label(),
        spec.params
    )
}

// ---------------------------------------------------------------------------------------------------
// memory_hash — a read you invoke, for a range you chose (design §2.2)
// ---------------------------------------------------------------------------------------------------

/// Whether `emulator/memory_hash` can hash `space`, **derived from the method's own closed params set**.
///
/// The row declares no `space` key, so it has no way to name anything but the 68000 bus — its target
/// resolves through the same `debug_read` the `bus` selector shows. That is a fact about the served
/// surface, read out of [`MethodSpec::params`], not a rule this panel decided.
pub fn hash_gate(space: Space) -> Result<(), String> {
    let name = "emulator/memory_hash";
    let Some(spec) = METHODS.iter().find(|m| m.name == name) else {
        return Err(format!("this build serves no {name}"));
    };
    if space == Space::Bus {
        return Ok(());
    }
    if spec.params.contains(&"space") {
        // A future revision that adds the key: the button turns on by itself rather than staying dark
        // behind a sentence somebody forgot to delete.
        return Ok(());
    }
    Err(format!(
        "{name} declares no `space` param (its keys are {:?}), so it hashes the 68000 bus only. \
         Switch the selector to `bus` to hash a range",
        spec.params
    ))
}

/// `emulator/memory_hash` over `addr..addr+len`, answered by the handler.
pub fn hash(bus: &mut Bus, sys: &mut System, addr: u32, len: u64) -> Answer {
    bus.call(
        sys,
        "emulator/memory_hash",
        &json!({"addr": oracle_aether::hex::addr(addr), "len": len}),
    )
}

// ---------------------------------------------------------------------------------------------------
// The panel's own state
// ---------------------------------------------------------------------------------------------------

/// Bytes per hex row, and rows per page. Fixed rather than configurable in this parcel: a 16-wide row is
/// what every other hex view in the suite uses, and a knob nobody asked for is a knob to keep working.
pub const PER_ROW: usize = 16;
pub const ROWS: usize = 16;

/// Everything the Memory panel remembers between repaints.
pub struct MemoryPanel {
    pub space: Space,
    /// What is in the address box, verbatim — hex or a symbol name.
    pub addr_text: String,
    /// The resolved base of the page currently shown.
    pub base: u32,
    /// What the last address-box gesture answered, kept on screen until the next one.
    pub addr_note: Option<Line>,
    /// The write cell's payload, and the last write's answer.
    pub write_text: String,
    pub write_note: Option<Line>,
    /// The hash range's length box, and the last hash's answer.
    pub hash_len_text: String,
    pub hash_note: Option<Line>,
    /// The five write gates, and the pause state they were probed under. Re-probed **only** when that
    /// state changes: a gate is a question about the machine's mode, and asking it 60 times a second
    /// would be dispatching inside a repaint for an answer that cannot have moved.
    gates: [Gate; 5],
    gates_at_paused: Option<bool>,
}

/// ⚑ **Where the Memory panel opens** — work RAM's base, as the 68000 puts it on the bus.
///
/// It used to be `0xFFFF0000`, which is the *listing* spelling of the same place and which the read path
/// refused: the panel's first click, with nothing typed, answered `REFUSED -32004: only cartridge ROM
/// ($000000..rom_len) and work RAM ($E00000-$FFFFFF) are readable in this slice` — **a refusal of a value
/// the panel had pre-filled itself.** The hazard was written up in this repo a week earlier
/// (`bus-protocol.schema.json`'s `rawAddr` note, signed *"found by hitting it"*) and then shipped as the
/// default.
///
/// One constant for the box, the page base and the hint text, so the three cannot disagree about where
/// this panel opens — and derived, not typed: it is the bottom of the work-RAM window `engine.rs` decodes
/// and the refusal above names, so a build that moved that window would have to move this too.
pub const DEFAULT_BASE: u32 = 0x00FF_0000;

/// [`DEFAULT_BASE`] as the address box shows it, and as the box's own hint advertises it.
pub fn default_base_text() -> String {
    oracle_aether::hex::addr(DEFAULT_BASE)
}

impl Default for MemoryPanel {
    fn default() -> Self {
        Self {
            space: Space::Bus,
            addr_text: default_base_text(),
            base: DEFAULT_BASE,
            addr_note: None,
            write_text: String::new(),
            write_note: None,
            hash_len_text: "256".into(),
            hash_note: None,
            // Probed before the panel is ever drawn (see `gates_for`), so this initial value is never
            // shown. `NoMethod` rather than `Open`, because a placeholder that reads as permission is
            // the one wrong default here.
            gates: [
                Gate::NoMethod,
                Gate::NoMethod,
                Gate::NoMethod,
                Gate::NoMethod,
                Gate::NoMethod,
            ],
            gates_at_paused: None,
        }
    }
}

impl MemoryPanel {
    /// The gate for the selected space, re-probing if the machine's run state has moved since the last
    /// probe.
    pub fn gates_for(&mut self, bus: &mut Bus, sys: &mut System) -> &Gate {
        let paused = bus.is_paused();
        if self.gates_at_paused != Some(paused) {
            self.gates = probe_all_gates(bus, sys);
            self.gates_at_paused = Some(paused);
        }
        let i = Space::ALL
            .iter()
            .position(|s| *s == self.space)
            .unwrap_or(0);
        &self.gates[i]
    }

    /// The gate the panel is currently showing, without probing. For tests and for the report line.
    pub fn gate_of(&self, space: Space) -> &Gate {
        let i = Space::ALL.iter().position(|s| *s == space).unwrap_or(0);
        &self.gates[i]
    }
}

/// One line of panel text, and **whether it is a refusal**.
///
/// The flag travels beside the text rather than being read back out of it: the panel colours refusals,
/// and a renderer that decided by looking for a `"REFUSED"` prefix would be a second encoding of a fact
/// the [`Answer`] already carries — the kind that agrees until someone rewords the string.
///
/// The text itself is the reply's own JSON, or the refusal's own code, reason and message. Nothing here
/// paraphrases the server.
pub struct Line {
    pub text: String,
    pub refused: bool,
}

pub fn answer_line(a: &Answer) -> Line {
    Line {
        refused: a.is_err(),
        text: match a {
            Answer::Ok(v) => format!("ok: {v}"),
            Answer::Err(e) => match a.reason() {
                Some(r) => format!("REFUSED {} {r}: {}", e.code, e.message),
                None => format!("REFUSED {}: {}", e.code, e.message),
            },
        },
    }
}

impl Line {
    /// A line the **panel** produced about its own input, which never reached the server. Marked as a
    /// refusal so it is coloured like one — it is one — but worded so a reader can tell the two apart:
    /// "the panel cannot send that" is a different fact from "the server said no".
    pub fn from_panel(why: String) -> Self {
        Line {
            text: format!("the panel cannot send that: {why}"),
            refused: true,
        }
    }

    pub fn plain(text: String) -> Self {
        Line {
            text,
            refused: false,
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// The parity invariants — design §4.4 R3
// ---------------------------------------------------------------------------------------------------

/// **This panel and the five read rows must never disagree**, and neither must its write gate and the
/// four write rows.
///
/// The guard lives here rather than in `oracle-aether/tests/` for the structural reason `pick.rs` gives:
/// `oracle-player` is the crate that can see both sides. Every bus side goes through
/// [`crate::bus::Bus`] — the same `Host::call` the shipped panel uses — so a `call` that stopped
/// swapping the machine in would fail these too rather than quietly comparing two placeholders.
#[cfg(test)]
mod bus_parity {
    use super::*;
    use crate::bus::Answer;
    use oracle_aether::host::MachineInfo;
    use oracle_core::system::System;

    fn booted() -> System {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        sys.run_frames(7);
        sys
    }

    fn bus(sys: &mut System, paused: bool) -> Bus {
        Bus::new(sys, MachineInfo::default(), paused, None)
    }

    /// `"0x00AA55"` → the bytes. The bus spells payloads as hex strings (D9 category 1) and the panel
    /// carries them as bytes, so the comparison crosses that boundary explicitly rather than by matching
    /// two strings and calling it agreement.
    fn bytes_of(v: &Value) -> Vec<u8> {
        let s = v
            .get("bytes")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("the reply carries a `bytes` hex string, got {v}"));
        let d = s.trim_start_matches("0x");
        assert!(
            d.len().is_multiple_of(2),
            "an even number of hex digits: {s:?}"
        );
        (0..d.len() / 2)
            .map(|i| u8::from_str_radix(&d[i * 2..i * 2 + 2], 16).expect("hex"))
            .collect()
    }

    fn ok(a: Answer) -> Value {
        match a {
            Answer::Ok(v) => v,
            Answer::Err(e) => panic!("expected an answer, got {} {}", e.code, e.message),
        }
    }

    /// **The read half: every space, the panel's bytes against the bus's own reply.**
    ///
    /// The bus side deliberately uses a *different method per space* where the surface has one
    /// (`emulator/read`, `emulator/read_vram`, `emulator/z80_read`), because that is what a human would
    /// reach for from a tool, and it is the pair that must agree. The panel side calls [`read`], which is
    /// what the shipped hex view calls.
    ///
    /// ⚑ **The fixture is seeded first, and without that most of this test was vacuous.** Measured, not
    /// assumed: seven frames into the fixture ROM the compared pages hold 30 and 31 distinct byte values
    /// in `bus` and `vram` — and **exactly one** in `cram`, `vsram` and `z80`, all of them zero. Three of
    /// the five legs were therefore comparing 32 zeros against 32 zeros, which a panel reading entirely
    /// the wrong array would have passed. So each writable space is poked through the bus with a
    /// distinctive pattern before it is read, and the seeding is asserted to have taken.
    ///
    /// `vsram` cannot be seeded — the surface has no write row for it, which is the same fact
    /// `Space::write_method` returns `None` for — so that leg stays a comparison over a uniformly zero
    /// range. It is kept, and it is kept *named*: the assertion below states the range is constant, so
    /// if the fixture ever gains real VSRAM content this note goes stale loudly instead of quietly.
    #[test]
    fn the_memory_panel_shows_the_same_bytes_the_bus_serves() {
        let mut sys = booted();
        // Paused, because two of the three seeding pokes are paused-only — which is this parcel's
        // subject and is checked in its own right by the gate tests.
        let mut b = bus(&mut sys, true);
        const LEN: usize = 32;

        for (method, params) in [
            (
                "emulator/write_memory",
                json!({"addr": "0x00FF0000", "bytes": "0x0123456789ABCDEF"}),
            ),
            (
                "emulator/write_vram",
                json!({"addr": "0x00000100", "bytes": "0xFEDCBA9876543210"}),
            ),
            (
                "emulator/write_cram",
                json!({"line": 0, "index": 8, "raw": 0x0EEE}),
            ),
            (
                "emulator/z80_write",
                json!({"addr": "0x00000100", "bytes": "0xC3A55A3C"}),
            ),
        ] {
            ok(b.call(&mut sys, method, &params));
        }

        for (space, method, params) in [
            (
                Space::Bus,
                "emulator/read",
                json!({"space": "bus", "addr": "0x00FF0000", "len": LEN}),
            ),
            (
                Space::Vram,
                "emulator/read_vram",
                json!({"addr": "0x00000100", "len": LEN}),
            ),
            (
                Space::Cram,
                "emulator/read",
                json!({"space": "cram", "addr": "0x00000010", "len": LEN}),
            ),
            (
                Space::Vsram,
                "emulator/read",
                json!({"space": "vsram", "addr": "0x00000000", "len": LEN}),
            ),
            (
                Space::Z80,
                "emulator/z80_read",
                json!({"addr": "0x00000100", "len": LEN}),
            ),
        ] {
            let addr = u32::from_str_radix(
                params["addr"].as_str().unwrap().trim_start_matches("0x"),
                16,
            )
            .unwrap();
            let served = bytes_of(&ok(b.call(&mut sys, method, &params)));
            let (panel, _) = read(space, &sys, addr, LEN).expect("the panel reads this range");
            assert_eq!(
                panel,
                served,
                "{}: the panel and {method} have DRIFTED",
                space.label()
            );
            assert_eq!(served.len(), LEN, "{method} answered a short read");

            // The anti-vacuity guard, per leg: two identical runs of one repeated byte agree with each
            // other whatever either side actually read.
            let distinct = served
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            if space == Space::Vsram {
                assert_eq!(
                    distinct, 1,
                    "vsram has gained content — this leg was documented as the one weak comparison \
                     precisely because it could not be seeded, and it can be strengthened now"
                );
            } else {
                assert!(
                    distinct > 1,
                    "{}: the compared page is a single repeated byte ({distinct} distinct value), so \
                     this leg proves nothing — the seeding above did not take",
                    space.label()
                );
            }
        }
    }

    /// The `bus` space's **region label** is the handler's, not a second guess: work RAM and cartridge
    /// ROM are two different windows and mislabelling one would misdescribe what a poke is about to hit.
    #[test]
    fn the_bus_space_reports_the_same_region_the_bus_does() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        for (addr, expect) in [(0x00FF_0000u32, "work RAM"), (0x0000_0100, "cartridge ROM")] {
            let reply = ok(b.call(
                &mut sys,
                "emulator/read",
                &json!({"space": "bus", "addr": oracle_aether::hex::addr(addr), "len": 4}),
            ));
            let (_, region) = read(Space::Bus, &sys, addr, 4).expect("readable");
            assert_eq!(
                region.map(BusRegion::label).as_deref(),
                Some(expect),
                "the fixture moved"
            );
            assert_eq!(
                region.map(BusRegion::label),
                reply["region"].as_str().map(str::to_string),
                "the panel's region label and the bus's have DRIFTED"
            );
        }
    }

    /// ★ **F-DEBUGREAD-BANKED, the panel's third of it.** With mapper window 1 re-pointed at bank 9 through
    /// the real bus write path, the panel's `bus` space shows bank 9's bytes under `cartridge ROM bank 9` —
    /// the answer `emulator/read` gives — AND that answer is the DERIVED one: a panel and a tool that share
    /// one function agree with each other however wrong the function is, so their agreement alone would
    /// have passed the flat read this parcel replaced (`docs/2026-09-11-debugread-banked.md`).
    #[test]
    fn the_panel_shows_the_bank_a_remapped_window_shows() {
        use oracle_core::m68000::bus68k::Bus68k;
        const BANK: usize = 0x8_0000;
        // Each byte names its bank and its in-bank offset, so bank 1 and bank 9 differ at every offset.
        let image: Vec<u8> = (0..10 * BANK)
            .map(|i| ((i / BANK) as u8 ^ 0x5A) ^ ((i % BANK) as u8))
            .collect();
        let off = 9 * BANK + (0x08_0100 & (BANK - 1));
        assert_ne!(
            image[0x08_0100..0x08_0110],
            image[off..off + 16],
            "banks 1 and 9 read alike: the fixture cannot tell a remap from identity"
        );
        let mut sys = System::new(0x5EED);
        sys.load_rom(image.clone());
        sys.reset();
        sys.mega_bus(&mut ()).write8(0xA1_30F3, 5, 9); // window 1 -> bank 9

        let (panel, region) = read(Space::Bus, &sys, 0x08_0100, 16).expect("readable");
        let mut b = bus(&mut sys, false);
        let reply = ok(b.call(
            &mut sys,
            "emulator/read",
            &json!({"space": "bus", "addr": "0x00080100", "len": 16}),
        ));
        assert_eq!(
            panel,
            bytes_of(&reply),
            "the panel and emulator/read have DRIFTED"
        );
        assert_eq!(
            region.map(BusRegion::label),
            reply["region"].as_str().map(str::to_string),
            "the panel's region label and the tool's have DRIFTED"
        );
        assert_eq!(
            panel,
            image[off..off + 16],
            "derived: bank 9 * $80000 + (addr & $7FFFF), not the image's bytes at $080100"
        );
        let v = view(Space::Bus, &sys, 0x08_0100, 1, 16);
        assert_eq!(
            v.region.map(|r| r.to_string()).as_deref(),
            Some("cartridge ROM bank 9"),
            "the panel's `region` line prints the bank"
        );
    }

    /// ⚑ **The write gate against reality — the asymmetry, measured rather than asserted.**
    ///
    /// For every space, in both run states: the gate says whether a write would be taken, then a real,
    /// well-formed write is made and the outcome is compared. Both directions fail loudly, so a gate
    /// that is wrong in either is caught.
    ///
    /// This is what stops [`probe_write_gate`] from being trusted on its own. The probe reads a *check
    /// order* — `require_paused` first — and a check order is editable. If someone moves it below the
    /// param parsing, the probe starts reporting Open for a handler that still refuses, and this test is
    /// what says so.
    #[test]
    fn the_panels_write_gate_agrees_with_what_a_real_write_actually_does() {
        // One well-formed gesture per writable space, in the panel's own spelling — `write_params`, not
        // a hand-built payload, so a params bug in the panel fails here rather than only on a human's
        // screen. (It already caught two: a `bytes` payload missing its D9 `0x`, and a `raw` sent as a
        // hex string where the row wants a number.)
        let gestures = [
            (Space::Bus, 0x00FF_0000u32, "AA"),
            (Space::Vram, 0x0000_0100, "55"),
            (Space::Cram, 0x0000_0020, "0EEE"),
            (Space::Z80, 0x0000_0100, "77"),
        ];

        for paused in [false, true] {
            let mut sys = booted();
            let mut b = bus(&mut sys, paused);
            let gates = probe_all_gates(&mut b, &mut sys);

            for (space, addr, payload) in gestures {
                let i = Space::ALL.iter().position(|s| *s == space).unwrap();
                let gate = &gates[i];
                let params = write_params(space, addr, payload).unwrap_or_else(|e| {
                    panic!("{}: the panel refused its own gesture: {e}", space.label())
                });
                let method = space.write_method().unwrap();
                let answer = b.call(&mut sys, method, &params);

                match (gate.is_open(), &answer) {
                    (true, Answer::Ok(_)) | (false, Answer::Err(_)) => {}
                    (true, Answer::Err(e)) => panic!(
                        "{} (paused={paused}): the gate said {method} would take this write and it \
                         answered {} {}. The panel would have offered a human a control that refuses.",
                        space.label(),
                        e.code,
                        e.message
                    ),
                    (false, Answer::Ok(v)) => panic!(
                        "{} (paused={paused}): the gate said {} but {method} took the write anyway \
                         ({v}). The panel would have greyed out a control that works — and the write \
                         LANDED while the cell claimed it could not.",
                        space.label(),
                        gate.why()
                    ),
                }
                // …and when it refused, it refused for the state, not for the shape. A gate that read a
                // params error as `machineRunning` would be right by accident here.
                if let (false, Answer::Err(e)) = (gate.is_open(), &answer) {
                    assert_eq!(
                        e.code,
                        code::INVALID_STATE,
                        "{}: the closed gate must be the run-state refusal, got {} {}",
                        space.label(),
                        e.code,
                        e.message
                    );
                    assert_eq!(answer.reason(), Some("machineRunning"));
                }
            }
        }
    }

    /// ⚑ **The asymmetry itself, named.** Three writes are paused-only and `write_vram` is not, so on a
    /// running machine the panel offers VRAM and refuses work RAM — and that is the *server's* shape,
    /// which this panel reflects rather than smooths.
    ///
    /// Asserted as an exact partition rather than as "at least one differs", because the interesting
    /// failure is either half moving: a `write_vram` that grew a gate, or one of the three that lost
    /// one. Both would be silent in the UI and both change what a human is allowed to do mid-frame.
    #[test]
    fn on_a_running_machine_vram_is_writable_and_the_other_three_are_not() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        let gates = probe_all_gates(&mut b, &mut sys);
        let open: Vec<&'static str> = Space::ALL
            .iter()
            .zip(gates.iter())
            .filter(|(_, g)| g.is_open())
            .map(|(s, _)| s.label())
            .collect();
        assert_eq!(
            open,
            vec!["vram"],
            "the paused-write asymmetry has MOVED. This panel reflects the server's rule and does not \
             own one, so a change here is a change on the bus (or in the contract's §6 run-control \
             rule) and needs to be understood before this expectation is edited."
        );
        // And vsram is the fifth space precisely because nothing writes it — a different fact from
        // "refused right now", and rendered as a different sentence.
        assert_eq!(gates[3], Gate::NoMethod, "vsram has no write row");
    }

    /// **Every gate says why, in words, including the open one.** A disabled control that explains
    /// nothing is indistinguishable from a broken one, and the requirement is not "non-empty" — the
    /// sentence has to name the thing a human would go looking for.
    #[test]
    fn every_write_cell_states_its_reason_and_names_its_method() {
        for paused in [false, true] {
            let mut sys = booted();
            let mut b = bus(&mut sys, paused);
            for (space, gate) in Space::ALL.iter().zip(probe_all_gates(&mut b, &mut sys)) {
                let why = gate.why();
                assert!(
                    why.len() > 20,
                    "{}: {why:?} is not an explanation",
                    space.label()
                );
                match space.write_method() {
                    Some(m) => assert!(
                        why.contains(m),
                        "{}: the reason must name the method a human would reach for, got {why:?}",
                        space.label()
                    ),
                    None => assert!(
                        why.contains("no served method"),
                        "{}: {why:?}",
                        space.label()
                    ),
                }
                if let Gate::Refused {
                    message, reason, ..
                } = &gate
                {
                    // The handler's own sentence, not a paraphrase of it.
                    assert!(
                        why.contains(message.as_str()) && why.contains(reason.as_str()),
                        "the cell must show the refusal a tool would get, verbatim: {why:?}"
                    );
                    assert!(
                        message.contains("emulator/pause"),
                        "the handler's message names the fix and the panel passes it through: \
                         {message:?}"
                    );
                }
            }
        }
    }

    /// The address box **is** `emulator/lookup_symbol` (design §2.2): it shows the tool's own answer and
    /// the tool's own refusal, and it never invents either.
    #[test]
    fn the_address_box_answers_with_the_tools_own_reply_and_refusal() {
        let mut sys = booted();

        // No listing loaded: the refusal must be the server's `-32012`, not a sentence this panel wrote.
        {
            let mut b = bus(&mut sys, false);
            match resolve_address(&mut b, &mut sys, Space::Bus, "Boot") {
                Resolved::Refused(e) => assert_eq!(
                    e.code,
                    code::NO_SYMBOLS_LOADED,
                    "no table loaded must be the server's own -32012: {} {}",
                    e.code,
                    e.message
                ),
                _ => panic!("a name with no listing loaded must be refused by the server"),
            }
        }

        // With one, the box resolves — and lands on the address the listing names.
        let listing = "  Symbol Table (* = unused):\n\n Boot : 300 C |\n\n   1 symbols\n";
        let table = oracle_core::symbols::SymbolTable::parse(listing).expect("parsable");
        let mut b = Bus::new(
            &mut sys,
            MachineInfo {
                rom_path: Some("testrom".into()),
                symbols: Some(table),
                symbols_path: Some("testrom.lst".into()),
            },
            false,
            None,
        );
        match resolve_address(&mut b, &mut sys, Space::Bus, "Boot") {
            Resolved::Symbol { addr, reply } => {
                assert_eq!(addr, 0x300);
                assert_eq!(reply["name"], json!("Boot"));
            }
            _ => panic!("`Boot` must resolve once the listing is loaded"),
        }
        // A name that is not there is the *other* refusal, and the two must stay tellable apart (§4).
        match resolve_address(&mut b, &mut sys, Space::Bus, "NoSuchThing") {
            Resolved::Refused(e) => assert_eq!(e.code, code::SYMBOL_NOT_FOUND),
            _ => panic!("an absent name must be refused"),
        }
        // Hex never dispatches: it is not a question the server can answer better. `listing` is `None`
        // because a value already inside the 24 bits the 68000 drives has only one spelling.
        assert!(matches!(
            resolve_address(&mut b, &mut sys, Space::Bus, "$FF0000"),
            Resolved::Hex {
                addr: 0x00FF_0000,
                listing: None
            }
        ));

        // ⚑ And a symbol in a non-bus space is refused, in **every** non-bus space — walked from
        // `Space::ALL` rather than sampled.
        //
        // H29: this block used to name `Space::Vram` and assert a sentence the `Z80` arm could not
        // produce. Picking one variant of a live selector is picking the one that passes: `space_wire`
        // carried the doc *"callers must not reach here with `Z80`"* and its sole caller reached there
        // on every Z80 gesture, so the panel sent `{"space":"z80"}` to a method with no z80 space and
        // the human got `-32602 \`space\` must be one of "bus","vram","cram","vsram"` — a refusal about
        // the space where the promised sentence is about the symbol. Walking `ALL` is what makes a
        // sixth selector entry unable to arrive untested.
        for space in Space::ALL {
            if space == Space::Bus {
                continue;
            }
            match resolve_address(&mut b, &mut sys, space, "Boot") {
                // The three spaces `emulator/read` serves: the server's own words, passed through. The
                // panel sends a real call rather than composing a second sentence about a rule it does
                // not own.
                Resolved::Refused(e) => {
                    assert!(
                        space.as_read_space().is_some(),
                        "{space:?} is not a space `emulator/read` names, so a refusal from it is a \
                         refusal about the SPACE and cannot be the sentence about the symbol: {:?}",
                        e.message
                    );
                    assert_eq!(e.code, code::INVALID_PARAMS, "{space:?}");
                    assert!(
                        e.message.contains("symbol") && e.message.contains("bus"),
                        "{space:?}: the server's own rule, passed through: {:?}",
                        e.message
                    );
                }
                // The one space that method does not serve. There is no server refusal that says the
                // right thing here, so the panel says it — `Rejected` is that variant's whole job — and
                // the sentence must name the row that DOES serve the space, or a human is told a name
                // is impossible without being told where to look instead.
                Resolved::Rejected(why) => {
                    assert_eq!(
                        space.as_read_space(),
                        None,
                        "{space:?} IS an `emulator/read` space, so this must be the server's refusal \
                         and not the panel's: {why:?}"
                    );
                    assert!(
                        why.contains(space.read_method()),
                        "{space:?}: the refusal must name the row that reads this space \
                         ({}): {why:?}",
                        space.read_method()
                    );
                    assert!(
                        why.contains("symbol"),
                        "{space:?}: …and must say what is missing, not merely that something is: \
                         {why:?}"
                    );
                }
                Resolved::Hex { addr, .. } => {
                    panic!("{space:?}: `Boot` is not a hex literal, yet it was taken as ${addr:X}")
                }
                Resolved::Symbol { addr, .. } => panic!(
                    "{space:?}: only the 68000 bus has symbols, yet `Boot` resolved to ${addr:X}"
                ),
            }
        }
    }

    /// ★ **The Memory panel opens on an address it can read** — the first click, with nothing typed.
    ///
    /// UX packet finding 4. The panel greeted a newcomer with
    /// `REFUSED -32004: only cartridge ROM ($000000..rom_len) and work RAM ($E00000-$FFFFFF) are readable
    /// in this slice` **under a value it had pre-filled itself** (`0xFFFF0000`, the 68000 *listing*
    /// spelling of work RAM's base). A first contact that refuses its own default teaches a person the
    /// tool is broken before they have asked it anything.
    ///
    /// The row reads the default **off `MemoryPanel::default()`**, not off a constant retyped here: a
    /// test that names the address it expects would pass against a panel that opens somewhere else
    /// entirely. It asserts three things, and the third is the one that stops this becoming vacuous —
    /// a `View` with no rows and no error would satisfy the first two.
    #[test]
    fn the_memory_panel_opens_on_an_address_it_can_actually_read() {
        let sys = booted();
        let p = MemoryPanel::default();
        let v = view(p.space, &sys, p.base, ROWS, PER_ROW);
        assert!(
            v.error.is_none(),
            "the Memory panel's first click refuses its own default ({}): {:?}",
            oracle_aether::hex::addr(p.base),
            v.error
        );
        assert_eq!(
            p.addr_text,
            oracle_aether::hex::addr(p.base),
            "the box and the page it reads must be one value, or the panel shows an address it is not \
             reading"
        );
        assert_eq!(
            v.rows.len(),
            ROWS,
            "a full page, so `error: None` is a read that happened and not an empty grid"
        );
        assert_eq!(
            v.region.map(BusRegion::label).as_deref(),
            Some("work RAM"),
            "the panel opens in work RAM, which is where a person debugging a game is looking"
        );
    }

    /// ★ **One box, one behaviour, for a name and for a number.**
    ///
    /// The other half of finding 4, and the part fixing the default alone would have left standing: the
    /// address box did the 24-bit masking **silently for a symbol** (`emulator/lookup_symbol` answers
    /// `addr` masked and `rawAddr` unmasked, and the panel read it fine) and **refused it for a number**,
    /// explaining neither. The refused spelling is the one an Aeon listing puts in front of a person.
    ///
    /// Both premises are asserted before the claim, because each is a way this row could pass while
    /// measuring nothing: an unmasked value that happens to be readable, or a masked one that is not.
    #[test]
    fn a_listing_address_and_its_bus_address_reach_the_same_page_and_the_panel_says_so() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);

        let listing_spelling = DEFAULT_BASE | !oracle_core::symbols::BUS_ADDR_MASK;
        assert_ne!(
            listing_spelling, DEFAULT_BASE,
            "premise: the two spellings differ, so there is something for the mask to do"
        );
        assert!(
            view(Space::Bus, &sys, listing_spelling, 1, PER_ROW)
                .error
                .is_some(),
            "premise: the unmasked spelling is genuinely unreadable, which is what made this a refusal \
             and not a preference"
        );

        match resolve_address(
            &mut b,
            &mut sys,
            Space::Bus,
            &oracle_aether::hex::addr(listing_spelling),
        ) {
            Resolved::Hex { addr, listing } => {
                assert_eq!(
                    addr, DEFAULT_BASE,
                    "the listing spelling must land on the address the machine puts on the bus"
                );
                assert_eq!(
                    listing,
                    Some(listing_spelling),
                    "the spelling the person typed must be CARRIED, so the panel can name both numbers \
                     rather than moving them somewhere silently"
                );
            }
            _ => panic!("a hex literal must not dispatch, whichever spelling it is written in"),
        }

        // …and the same mask is NOT applied to a space that is not the 68000 bus, where it would be
        // arithmetic about the wrong machine.
        for space in [Space::Vram, Space::Cram, Space::Vsram, Space::Z80] {
            match resolve_address(
                &mut b,
                &mut sys,
                space,
                &oracle_aether::hex::addr(listing_spelling),
            ) {
                Resolved::Hex { addr, listing } => {
                    assert_eq!(
                        (addr, listing),
                        (listing_spelling, None),
                        "{space:?}: a 68000 bus mask was applied to an address that is not on the \
                         68000 bus"
                    );
                }
                _ => panic!("{space:?}: a hex literal must not dispatch"),
            }
        }
    }

    /// The hash button hands the tool a range and shows the tool's answer.
    #[test]
    fn the_hash_button_hashes_what_emulator_memory_hash_hashes() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        let a = hash(&mut b, &mut sys, 0x00FF_0000, 256);
        let v = ok(a);
        assert_eq!(v["len"], json!(256));
        assert_eq!(v["region"], json!("work RAM"));
        assert!(v["fnv1a64"].is_string() && v["crc32"].is_string());
        // The button is offered only where the row can name the space, and the reason is derived from
        // the row's own closed params set rather than written down here.
        assert!(hash_gate(Space::Bus).is_ok());
        let why = hash_gate(Space::Vram).expect_err("vram cannot be hashed by this row");
        assert!(
            why.contains("no `space` param"),
            "the reason must be the derived one: {why:?}"
        );
        assert!(
            !METHODS
                .iter()
                .find(|m| m.name == "emulator/memory_hash")
                .expect("served")
                .params
                .contains(&"space"),
            "if this row ever grows a `space` key, `hash_gate` opens by itself and this expectation is \
             the thing that must be revisited"
        );
    }

    /// A page that runs off the end of a sized space is **shortened loudly**, never quietly. A hex view
    /// showing four rows where sixteen were asked for, with nothing said, is a clipped read wearing a
    /// UI.
    #[test]
    fn a_page_that_runs_past_the_end_of_a_space_says_so() {
        let sys = booted();
        // CRAM is 128 bytes; a 16×16 page from $60 cannot be whole.
        let v = view(Space::Cram, &sys, 0x60, ROWS, PER_ROW);
        assert!(v.error.is_none(), "the readable part is still shown");
        assert_eq!(
            v.truncated_to,
            Some(sys.vdp().cram().len() - 0x60),
            "the shortfall must be reported, not absorbed"
        );
        assert_eq!(v.rows.len(), 2, "$60..$80 is two 16-byte rows");
        // …and past the end entirely is a refusal with a reason, never an empty grid.
        let past = view(Space::Cram, &sys, 0x400, ROWS, PER_ROW);
        assert!(past.rows.is_empty());
        let e = past.error.expect("past the end must refuse");
        assert_eq!(e.code, code::ADDRESS_OUT_OF_RANGE);
        assert!(
            e.message.contains("cram"),
            "the message names the space: {}",
            e.message
        );
    }

    /// A read the *bus* refuses is refused identically here, with the handler's own message — the panel
    /// does not invent a bound of its own for the space that has no single end.
    #[test]
    fn a_bus_read_off_the_end_of_a_region_refuses_exactly_as_the_tool_does() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        let addr = 0x00FF_FFF0u32;
        let panel = read(Space::Bus, &sys, addr, 32).expect_err("this runs past work RAM");
        let served = match b.call(
            &mut sys,
            "emulator/read",
            &json!({"space": "bus", "addr": oracle_aether::hex::addr(addr), "len": 32}),
        ) {
            Answer::Err(e) => e,
            Answer::Ok(v) => panic!("the bus accepted a read past the end of work RAM: {v}"),
        };
        assert_eq!((panel.code, panel.message), (served.code, served.message));
        assert_eq!(panel.code, code::ADDRESS_OUT_OF_RANGE);
    }

    /// The Z80 window's `$2000-$3FFF` mirror is the machine's, and the panel folds it through the same
    /// function the handler does rather than through a second copy of the mask.
    #[test]
    fn the_z80_mirror_folds_the_same_way_for_the_panel_and_the_tool() {
        let mut sys = booted();
        let mut b = bus(&mut sys, true);
        ok(b.call(
            &mut sys,
            "emulator/z80_write",
            &json!({"addr": "0x00000123", "bytes": "0xC3"}),
        ));
        let (low, _) = read(Space::Z80, &sys, 0x0123, 1).expect("in window");
        let (mirrored, _) = read(Space::Z80, &sys, 0x2123, 1).expect("in window");
        assert_eq!(low, vec![0xC3]);
        assert_eq!(mirrored, low, "$2123 mirrors $0123 — that is the machine");
        // …and the window is bounded at BOTH ends, refused whole rather than wrapped.
        let e = read(Space::Z80, &sys, 0x3FFF, 2).expect_err("this runs past the window");
        assert_eq!(e.code, code::ADDRESS_OUT_OF_RANGE);
    }
}
