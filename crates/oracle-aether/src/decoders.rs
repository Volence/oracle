//! **Object-record decoders** — `protocol.md` §6's ⚙ group, schematized 2026-08-26 by §11.25 (CR-D).
//!
//! Three rows share one machine: `emulator/object_list`, `emulator/player_state` and
//! `emulator/object_slot`. This module owns the part that is *not* the wire — how a layout is derived, and
//! how one record's bytes become the contract's keys. The handlers in [`crate::engine`] stay thin because
//! everything engine-shaped lives here.
//!
//! # The shape the contract fixes, and the shape it deliberately does not
//!
//! §11.25's resolution is a **closed envelope over an open payload**: the contract owns `slot`, `addr`,
//! `x`, `y`, `code`, `name`, `nameDisp`, `bytes` and `layout`, and it declares `fields` a *typed-open map*
//! — the value types are pinned, the key set is a function of the loaded game. So the field catalogue
//! below is data keyed by `layout.engine`, never a Rust struct with one field per game concept, and a
//! request for a name this engine does not declare is `-32602` **before any decode**.
//!
//! # Nothing about the pool is written down here
//!
//! The base address, the record stride, the pool bounds and the slot count all come from **symbols**. This
//! is not a stylistic preference. `sst.emp`'s own comment dates the current `$50` record to a 2026-08-05
//! fold that *"shrink[ed] the record `$52` -> `$50`"*, and a committed fixture in aeon's tree
//! (`Dynamic_Slots : FFFF8DC2`) cannot be reconciled with the demand doc's `Player_1 = $FF8DB0` — one of
//! them describes an older ROM. **Any address in this family is a fact about a build**, so a decoder
//! carrying one is carrying a fact about a build it is not looking at. When the symbols are absent the
//! answer is a refusal ([`no_layout`]), never a guess: §11.25 hardens against the legacy server's
//! hardcoded fallback base by name.
//!
//! The **field offsets** are the one thing symbols cannot give, and they are exactly what `layout.engine`
//! discriminates. They are still not blindly trusted: the table declares the record size it was written
//! against, the symbols say what the record actually is, and a disagreement is a refusal rather than a
//! mis-decode ([`EngineLayout::slot_bytes`]). That is the `$52` -> `$50` fold caught the next time it
//! happens instead of silently reading `anim` out of `subtype`.

use crate::hex;
use crate::rpc::{code, RpcError};
use oracle_core::symbols::SymbolTable;
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------------------------------
// The field catalogue — data keyed by engine, never a struct
// ---------------------------------------------------------------------------------------------------

/// How one field's bytes reach the wire, under D9's two categories.
///
/// D9 category 1 (*address-shaped*) is [`Repr::Hex`]; category 2 (*counts and scalars*) is [`Repr::U`] or
/// [`Repr::I`]. The split is per-field and comes from the layout's own typing, exactly as §11.25's
/// `fields` obligation requires — a ROM pointer is a hex string, an animation index is a number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Repr {
    /// Unsigned integer.
    U,
    /// Two's-complement signed integer of the field's width.
    I,
    /// `0x` + `width * 2` hex digits, verbatim.
    Hex,
}

/// One named field of a record: where it is, how wide, and how it is spelled on the wire.
///
/// **Opaque outside this module**: every member is private, so a caller can hold a resolved field and
/// hand it back to be decoded but cannot read an offset out of it and go read the bus itself. That keeps
/// the one place a field's bytes are interpreted the same place its type is declared.
#[derive(Clone, Copy, Debug)]
pub struct FieldSpec {
    name: &'static str,
    offset: u32,
    width: u32,
    repr: Repr,
}

/// Shorthand so the catalogue below reads as a table of rows rather than as 29 six-line records —
/// which is what it is, and what a reader checking it against `sst.emp` needs it to look like.
const fn f(name: &'static str, offset: u32, width: u32, repr: Repr) -> FieldSpec {
    FieldSpec {
        name,
        offset,
        width,
        repr,
    }
}

/// The **aeon SST** field catalogue, read from `engine/objects/sst.emp` at aeon `f4896139` via the CR's
/// §2.4 transcription (which the demand doc's own table agrees with row for row).
///
/// **`sst_custom` (`$30-$4F`) is deliberately absent, and its absence is normative.** That window is an
/// overlay whose meaning is chosen by whichever routine owns the slot — the CR measured **ten** declared
/// interpretations of it in one build, five of which put a different type on the single word at `$30`
/// (`ground_speed: i16`, `player: u16` *an SST pointer*, `steps_remaining`, `timer`, `half_height`).
/// §11.25's rule (3) forbids reporting a key that is addressable but not *live* for the slot's current
/// occupant, so declaring any overlay name here would be exactly the uninitialised-byte-as-datum defect.
/// A client that knows which routine owns the slot reads `addr` with `emulator/read`.
///
/// The engine-owned tail word at `$4E` (`SST_interact`, `sizeof(Sst) - 2`) is likewise not a struct field
/// — `sst.emp` says it *"can't be reached by field name"* — so it is not named here either.
const AEON_SST_FIELDS: &[FieldSpec] = &[
    // The dispatch word. Named because the layout names it, even though it is also hoisted to `code`: the
    // obligation is "MUST NOT emit a key the layout does not name", not "MUST NOT name it twice".
    f("code_addr", 0x00, 2, Repr::Hex),
    // 16.16 fixed point. The *raw* word pair, category 2 — the integer half is hoisted to `x`/`y`, and this
    // is how a caller reaches the sub-pixel half the contract deliberately does not carry.
    f("x_pos", 0x02, 4, Repr::U),
    f("y_pos", 0x06, 4, Repr::U),
    // 8.8 fixed point, signed: a velocity that cannot be negative is not a velocity.
    f("x_vel", 0x0A, 2, Repr::I),
    f("y_vel", 0x0C, 2, Repr::I),
    // RAW, with no decoded bit names — §11.25 refuses those on every row, on the measured ground that a
    // set-bits list carries strictly LESS than the byte beside it (it cannot express a clear bit).
    f("render_flags", 0x0E, 1, Repr::U),
    f("collision_resp", 0x0F, 1, Repr::U),
    // ROM pointers: address-shaped, so D9 category 1.
    f("mappings", 0x10, 4, Repr::Hex),
    f("art_tile", 0x14, 2, Repr::U),
    f("width_pixels", 0x16, 1, Repr::U),
    f("height_pixels", 0x17, 1, Repr::U),
    f("anim", 0x18, 1, Repr::U),
    f("subtype", 0x19, 1, Repr::U),
    f("anim_table", 0x1A, 4, Repr::Hex),
    f("status", 0x1E, 1, Repr::U),
    f("angle", 0x1F, 1, Repr::U),
    f("prev_anim", 0x20, 1, Repr::U),
    f("anim_frame", 0x21, 1, Repr::U),
    f("anim_timer", 0x22, 1, Repr::U),
    f("mapping_frame", 0x23, 1, Repr::U),
    f("prev_frame", 0x24, 1, Repr::U),
    f("sprite_piece_count", 0x25, 1, Repr::U),
    // SST pointers — a RAM word naming another slot, so category 1.
    f("parent_ptr", 0x26, 2, Repr::Hex),
    f("sibling_ptr", 0x28, 2, Repr::Hex),
    f("slot_tag", 0x2A, 1, Repr::U),
    f("entity_section_id", 0x2B, 1, Repr::U),
    f("entity_list_index", 0x2C, 1, Repr::U),
    f("layer", 0x2D, 1, Repr::U),
    f("frame_off", 0x2E, 2, Repr::U),
];

/// One engine's record shape and the symbols that locate its pool.
struct EngineLayout {
    /// `layout.engine`. A free string by contract — §11.18 makes an emitted enum unwidenable — whose job
    /// is to grow one entry per supported game.
    engine: &'static str,
    /// **The record size this field table was written against.** Not the answer: [`derive`] computes the
    /// real stride from `Player_2 - Player_1` and reports *that*. This is the cross-check — if the two
    /// disagree the build is not the one the offsets describe, and the decode is refused.
    table_slot_bytes: u32,
    /// Candidates for slot 0's address, in preference order. The first that resolves both locates the pool
    /// and becomes `layout.detectedFrom`.
    base_symbols: &'static [&'static str],
    /// Two adjacent records; their difference **is** `sizeof(Sst)`. This is why nothing hardcodes `$50`.
    stride_pair: (&'static str, &'static str),
    /// One past the last record.
    end_symbol: &'static str,
    /// `(pool name, symbol at its first slot)`. The first pool starts at the base, so it carries `None`.
    pools: &'static [(&'static str, Option<&'static str>)],
    /// Base of the object code bank. `code_addr` is an offset from it (`objcodebase.emp`: *"Every object
    /// routine's code_addr is `label - ObjCodeBase`"*), so without this symbol no `name` can be resolved —
    /// and the key is then **omitted**, never faked.
    code_base_symbol: &'static str,
    fields: &'static [FieldSpec],
    /// Offset and width of the identity datum hoisted to `code`.
    code_offset: u32,
    code_width: u32,
    /// Offsets of the **integer halves** of the 16.16 positions — the world pixels `x`/`y` carry.
    x_pixel_offset: u32,
    y_pixel_offset: u32,
}

/// The only engine this build decodes. `ram.emp:612-618` declares the RAM order once
/// (`Object_RAM, Player_1, Player_2, Dynamic_Slots, System_Slots, Effect_Slots, Object_RAM_End`) and
/// `core.emp:35` pins the adjacency with a link-time `ensure`, so every quantity below is a symbol
/// difference rather than a number.
const AEON_SST: EngineLayout = EngineLayout {
    engine: "aeon-sst",
    table_slot_bytes: 0x50,
    // `Object_RAM` is a `mark` and may or may not reach the listing; `Player_1` is a label and does. Both
    // name the same address, so either locates the pool and whichever answered is reported.
    base_symbols: &["Object_RAM", "Player_1"],
    stride_pair: ("Player_1", "Player_2"),
    end_symbol: "Object_RAM_End",
    pools: &[
        ("player", None),
        ("dynamic", Some("Dynamic_Slots")),
        ("system", Some("System_Slots")),
        ("effect", Some("Effect_Slots")),
    ],
    code_base_symbol: "ObjCodeBase",
    fields: AEON_SST_FIELDS,
    // `code_addr @ $00`, word. `sst.emp`: *"The first word IS the dispatch … (0 = empty slot, the bank's
    // safety rts)"* — which is also the activity test, and the reason `active` is never a guess.
    code_offset: 0x00,
    code_width: 2,
    // The high word of each 16.16 coordinate. §11.25 carries world **pixels** because pixels are the one
    // value comparable across layouts; the sub-pixel half stays reachable through `fields`.
    x_pixel_offset: 0x02,
    y_pixel_offset: 0x06,
};

// ---------------------------------------------------------------------------------------------------
// The derived layout
// ---------------------------------------------------------------------------------------------------

/// The pool `emulator/player_state` reports on. Named once, here, because two spellings of it would be
/// two answers to "which slots are players".
const PLAYER_POOL: &str = "player";

/// One pool of the object table, as `layout.pools[]` carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pool {
    pub name: &'static str,
    pub first_slot: u32,
    pub slot_count: u32,
}

/// What the server decoded against — the value of every reply's REQUIRED `layout` key.
///
/// **Derived per reply, never cached from the handshake.** `emulator/load_symbols` may be called at any
/// point in a session and the detect branches on whether a symbol resolves, so a handshake-time value is
/// stale by construction — which is also why `capabilities.objectDecoders` reports whether this *build*
/// has the handlers and never whether a layout was found.
pub struct ObjectLayout {
    spec: &'static EngineLayout,
    /// Which of [`EngineLayout::base_symbols`] actually answered.
    detected_from: &'static str,
    /// **Measured**, from `Player_2 - Player_1`.
    slot_bytes: u32,
    /// **Measured**, from `(Object_RAM_End - base) / slot_bytes`.
    slot_count: u32,
    base_addr: u32,
    /// `None` when the pool-boundary symbols did not all resolve, or when what they describe is not a
    /// partition of this table. The key is optional by contract, and its absence is the honest answer
    /// rather than a guessed partition.
    pools: Option<Vec<Pool>>,
    /// Boundary symbols that were absent from the listing. Empty beside `pools: None` means they all
    /// resolved and the geometry was rejected — a different finding, kept distinguishable.
    missing_pool_symbols: Vec<&'static str>,
    /// `ObjCodeBase`, when it resolved. Gates `name`/`nameDisp`.
    code_base: Option<u32>,
}

impl ObjectLayout {
    pub fn slot_count(&self) -> u32 {
        self.slot_count
    }

    pub fn slot_bytes(&self) -> u32 {
        self.slot_bytes
    }

    pub fn engine(&self) -> &'static str {
        self.spec.engine
    }

    /// Address of slot `n`. The client can reproduce it from `layout` alone with one multiplication, which
    /// is P1: a decoder reply must be checkable against another instrument on the same bus.
    pub fn slot_addr(&self, slot: u32) -> u32 {
        self.base_addr
            .wrapping_add(slot.wrapping_mul(self.slot_bytes))
    }

    /// The pool named `name`, if the boundaries resolved.
    pub fn pool(&self, name: &str) -> Option<&Pool> {
        self.pools.as_ref()?.iter().find(|p| p.name == name)
    }

    /// The player pool, or the refusal `emulator/player_state` owes when the partition is unavailable.
    ///
    /// Same call as [`derive`]'s about the base address, one level down: the row cannot say which slots
    /// are players, so it says so rather than guessing that the first two are.
    pub fn player_pool(&self) -> Result<&Pool, RpcError> {
        self.pool(PLAYER_POOL).ok_or_else(|| {
            RpcError::new(
                code::NO_SYMBOLS_LOADED,
                format!(
                    "the loaded symbol table does not partition the object table into pools, so the \
                     `{PLAYER_POOL}` slots cannot be identified — refusing rather than guessing which \
                     slots hold players (protocol.md §11.25)."
                ),
            )
            // Which boundary symbols were absent, and **not** the whole list dressed as the missing one:
            // an empty array here is itself the finding — every symbol resolved and the geometry they
            // describe is not a partition of this table.
            .with_data(json!({
                "engine": self.spec.engine,
                "missingSymbols": self.missing_pool_symbols,
            }))
        })
    }

    /// The `layout` object exactly as the fragment declares it.
    pub fn to_json(&self) -> Value {
        let mut out = Map::new();
        out.insert("engine".into(), json!(self.spec.engine));
        // Always `"symbol"` here: this server has no configured-base path and refuses rather than falling
        // back, so the other two registered values (`configured`, `fallback`) are unreachable — and
        // `fallback`'s mandatory `caveat` with them.
        out.insert("detectedBy".into(), json!("symbol"));
        out.insert("detectedFrom".into(), json!(self.detected_from));
        out.insert("slotBytes".into(), json!(self.slot_bytes));
        out.insert("slotCount".into(), json!(self.slot_count));
        out.insert("baseAddr".into(), json!(hex::addr(self.base_addr)));
        if let Some(pools) = &self.pools {
            out.insert(
                "pools".into(),
                Value::Array(
                    pools
                        .iter()
                        .map(|p| {
                            json!({
                                "name": p.name,
                                "firstSlot": p.first_slot,
                                "slotCount": p.slot_count,
                            })
                        })
                        .collect(),
                ),
            );
        }
        Value::Object(out)
    }

    /// Resolve a requested `fields` list against this engine's catalogue.
    ///
    /// **The refusal precedes any decode** (§11.25 obligation 4, §2.5's *"the refusal precedes any
    /// effect"* one level down), so a refused request has read nothing — and *every* unknown name is
    /// reported, not just the first, because a caller fixing one at a time is a caller making N round
    /// trips to learn one list.
    pub fn resolve_fields(
        &self,
        requested: &[String],
    ) -> Result<Vec<&'static FieldSpec>, RpcError> {
        let mut out = Vec::with_capacity(requested.len());
        let mut unknown = Vec::new();
        for want in requested {
            match self.spec.fields.iter().find(|f| f.name == want) {
                Some(f) => out.push(f),
                None => unknown.push(want.clone()),
            }
        }
        if !unknown.is_empty() {
            return Err(RpcError::invalid_params(format!(
                "layout `{}` does not name {} — a `fields` name must be one this layout declares, and \
                 an unknown one is refused before any decode, so nothing was read",
                self.spec.engine,
                unknown
                    .iter()
                    .map(|u| format!("`{u}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .with_data(json!({ "unknownFields": unknown })));
        }
        Ok(out)
    }
}

/// Every symbol [`derive`] needs before it will decode anything, in the order it looks for them.
fn required_symbols(spec: &EngineLayout) -> Vec<&'static str> {
    let mut v = vec![spec.stride_pair.0, spec.stride_pair.1, spec.end_symbol];
    v.extend_from_slice(spec.base_symbols);
    v
}

/// **Derive the layout from the loaded symbol table, or refuse.**
///
/// There is no third branch. §11.25's first hardening against the only shipping implementation is
/// *"No symbols, no decode"* — the legacy server falls back to a hardcoded base at two call sites, which
/// is `-32004`'s confidently-wrong shape with no `binding` field to reveal it — so this answers `-32012`
/// on `write_memory`'s ground that *"relaxing a refusal later is additive (D5); introducing one is not"*.
pub fn derive(table: Option<&SymbolTable>) -> Result<ObjectLayout, RpcError> {
    let Some(table) = table else {
        return Err(RpcError::new(
            code::NO_SYMBOLS_LOADED,
            "no symbol table is loaded, so no object layout can be derived — call \
             emulator/load_symbols first. This row refuses rather than decoding from a guessed base \
             address (protocol.md §11.25).",
        ));
    };
    let spec = &AEON_SST;

    // 1. Slot 0. `Object_RAM` (a `mark`) is preferred over `Player_1` (a label) only because it names the
    //    region rather than its first occupant; both are the same address and whichever answered is what
    //    `detectedFrom` reports, so the reply never claims a symbol it did not read.
    let Some((detected_from, base_addr)) = spec
        .base_symbols
        .iter()
        .find_map(|s| table.address_of(s).map(|a| (*s, a)))
    else {
        return Err(no_layout(
            spec,
            table,
            "no symbol names the base of the object table",
        ));
    };

    // 2. The stride, MEASURED. Two adjacent `Sst` records; their difference is `sizeof(Sst)`. Hardcoding
    //    it would have been wrong three weeks ago, when the 2026-08-05 fold took the record $52 -> $50.
    let (Some(p1), Some(p2)) = (
        table.address_of(spec.stride_pair.0),
        table.address_of(spec.stride_pair.1),
    ) else {
        return Err(no_layout(
            spec,
            table,
            "the record stride is measured from two adjacent slots and both symbols must resolve",
        ));
    };
    let Some(slot_bytes) = p2.checked_sub(p1).filter(|d| *d > 0) else {
        return Err(no_layout(
            spec,
            table,
            &format!(
                "the two stride symbols do not describe two ascending adjacent records \
                 ({} = {}, {} = {})",
                spec.stride_pair.0,
                hex::addr(p1),
                spec.stride_pair.1,
                hex::addr(p2)
            ),
        ));
    };

    // 3. **The cross-check the field table owes.** Offsets are the one thing symbols cannot supply, so the
    //    catalogue declares the record size it was written against and a disagreement refuses. Without
    //    this, the next fold reads `anim` out of `subtype` and reports it as a datum.
    if slot_bytes != spec.table_slot_bytes {
        return Err(no_layout(
            spec,
            table,
            &format!(
                "the loaded build's record is ${slot_bytes:X} bytes but this server's `{}` field \
                 catalogue was written for ${:X} — the field offsets would not describe this build, so \
                 the decode is refused rather than answered wrongly",
                spec.engine, spec.table_slot_bytes
            ),
        ));
    }

    // 4. The count, MEASURED, from the region's own end mark.
    let Some(end) = table.address_of(spec.end_symbol) else {
        return Err(no_layout(
            spec,
            table,
            "the slot count is measured from the object table's end mark",
        ));
    };
    let Some(span) = end.checked_sub(base_addr).filter(|s| *s > 0) else {
        return Err(no_layout(
            spec,
            table,
            &format!(
                "{} ({}) does not lie above the table base ({})",
                spec.end_symbol,
                hex::addr(end),
                hex::addr(base_addr)
            ),
        ));
    };
    if span % slot_bytes != 0 {
        return Err(no_layout(
            spec,
            table,
            &format!(
                "the object table spans ${span:X} bytes, which is not a whole number of \
                 ${slot_bytes:X}-byte records"
            ),
        ));
    }
    let slot_count = span / slot_bytes;

    // 5. The pools, OPTIONAL — and optional in the direction that matters. `pools` is data rather than an
    //    enum precisely so one engine's pool structure is not frozen into the bus; if the boundary symbols
    //    are not all in the listing the key is omitted, which the fragment allows, rather than guessed.
    let pools = derive_pools(spec, table, base_addr, slot_bytes, slot_count);
    let missing_pool_symbols = spec
        .pools
        .iter()
        .filter_map(|(_, s)| *s)
        .filter(|s| table.address_of(s).is_none())
        .collect();

    Ok(ObjectLayout {
        spec,
        detected_from,
        slot_bytes,
        slot_count,
        base_addr,
        pools,
        missing_pool_symbols,
        code_base: table.address_of(spec.code_base_symbol),
    })
}

/// The pool partition, or `None` if it cannot be built **whole**.
///
/// Every property the fragment states is checked rather than assumed: ascending `firstSlot`, contiguous,
/// slot-aligned, and covering `[0, slotCount)` exactly. A partition that fails any of them is not
/// downgraded to a partial one — a client reading `pools` is told which slots are players, and a table
/// that is wrong about that is worse than one that is absent.
fn derive_pools(
    spec: &EngineLayout,
    table: &SymbolTable,
    base_addr: u32,
    slot_bytes: u32,
    slot_count: u32,
) -> Option<Vec<Pool>> {
    let mut starts: Vec<(&'static str, u32)> = Vec::with_capacity(spec.pools.len());
    for (name, sym) in spec.pools {
        let addr = match sym {
            None => base_addr,
            Some(s) => table.address_of(s)?,
        };
        let off = addr.checked_sub(base_addr)?;
        if off % slot_bytes != 0 {
            return None;
        }
        starts.push((name, off / slot_bytes));
    }
    let mut out = Vec::with_capacity(starts.len());
    for (i, (name, first)) in starts.iter().enumerate() {
        let next = starts.get(i + 1).map(|(_, f)| *f).unwrap_or(slot_count);
        // Ascending and contiguous: `next < first` is a wrong partition, and `next == first` is an empty
        // pool, which is legal (`slotCount: 0` is explicitly `minimum: 0` in the fragment).
        let count = next.checked_sub(*first)?;
        out.push(Pool {
            name,
            first_slot: *first,
            slot_count: count,
        });
    }
    // Covering exactly: the first pool starts at 0 and the last ends at slotCount. Both fall out of the
    // construction above unless a boundary symbol lies past the table's end.
    if out.first()?.first_slot != 0 {
        return None;
    }
    let last = out.last()?;
    if last.first_slot + last.slot_count != slot_count {
        return None;
    }
    Some(out)
}

/// `-32012` with the symbols that did not resolve.
///
/// The code is `NO_SYMBOLS_LOADED` for both the no-table case and this one, because §11.25 names one
/// refusal for the family — *"A server with no symbol table refuses with `-32012` rather than decoding
/// from a guessed base"* — and from a client's side "there is no table" and "the table does not describe
/// an object pool" are the same actionable fact: load a listing for this build. `error.data` carries which
/// names were missing so the caller need not bisect.
fn no_layout(spec: &EngineLayout, table: &SymbolTable, why: &str) -> RpcError {
    let missing: Vec<&str> = required_symbols(spec)
        .into_iter()
        .filter(|s| table.address_of(s).is_none())
        .collect();
    RpcError::new(
        code::NO_SYMBOLS_LOADED,
        format!(
            "the loaded symbol table does not describe an object layout — {why}. Refusing rather than \
             decoding from a guessed base (protocol.md §11.25)."
        ),
    )
    .with_data(json!({
        "engine": spec.engine,
        "missingSymbols": missing,
    }))
}

// ---------------------------------------------------------------------------------------------------
// Decoding one record
// ---------------------------------------------------------------------------------------------------

/// Big-endian read of `width` bytes at `offset`. The 68000 is big-endian and every width here is 1, 2 or 4.
fn be(record: &[u8], offset: u32, width: u32) -> u32 {
    let mut v = 0u32;
    for i in 0..width {
        v = (v << 8) | u32::from(record[(offset + i) as usize]);
    }
    v
}

/// Sign-extend a `width`-byte two's-complement value.
fn signed(raw: u32, width: u32) -> i64 {
    let bits = width * 8;
    let sign = 1u32 << (bits - 1);
    if raw & sign != 0 {
        i64::from(raw) - (1i64 << bits)
    } else {
        i64::from(raw)
    }
}

/// What one slot's bytes say, before any of it reaches the wire.
pub struct DecodedRecord<'a> {
    layout: &'a ObjectLayout,
    slot: u32,
    addr: u32,
    bytes: Vec<u8>,
}

impl<'a> DecodedRecord<'a> {
    pub fn new(layout: &'a ObjectLayout, slot: u32, addr: u32, bytes: Vec<u8>) -> Self {
        Self {
            layout,
            slot,
            addr,
            bytes,
        }
    }

    /// **Whether the slot holds a live object.** `sst.emp` on `code_addr`: *"the first word IS the
    /// dispatch … 0 = empty slot, the bank's safety rts"*. So this is the engine's own sentinel read at
    /// the offset the layout names, not a heuristic over the record.
    pub fn active(&self) -> bool {
        be(
            &self.bytes,
            self.layout.spec.code_offset,
            self.layout.spec.code_width,
        ) != 0
    }

    /// The keys the contract owns, as a map.
    ///
    /// # The conditional is enforced here, not only in the schema
    ///
    /// When `active` is false the decoded keys — `x`, `y`, `code`, `name`, `nameDisp`, `fields`, `bytes` —
    /// are **omitted**, never zeroed. An empty slot's record is bytes the game never wrote, so emitting
    /// `x: 0` reports a value that has no source; §11.25's rule (3) forbids exactly that one level down
    /// for `fields`, and both fragments that carry `active` spell it as an `if`/`then`/`else` that refuses
    /// the reply outright. `bytes` is in the forbidden set on purpose: an empty slot's bytes *are* the
    /// unwritten record, so `includeBytes: true` against one answers `active: false` and no `bytes`.
    pub fn to_json(
        &self,
        include_slot_facts: bool,
        fields: Option<&[&FieldSpec]>,
        include_bytes: bool,
    ) -> Map<String, Value> {
        let mut out = Map::new();
        if include_slot_facts {
            out.insert("slot".into(), json!(self.slot));
            out.insert("addr".into(), json!(hex::addr(self.addr)));
        }
        if !self.active() {
            return out;
        }
        let spec = self.layout.spec;
        let code = be(&self.bytes, spec.code_offset, spec.code_width);
        out.insert(
            "x".into(),
            json!(signed(be(&self.bytes, spec.x_pixel_offset, 2), 2)),
        );
        out.insert(
            "y".into(),
            json!(signed(be(&self.bytes, spec.y_pixel_offset, 2), 2)),
        );
        out.insert("code".into(), json!(hex_of(code, spec.code_width)));
        if let Some(f) = fields {
            let mut m = Map::new();
            for f in f {
                m.insert(f.name.into(), self.field_value(f));
            }
            out.insert("fields".into(), Value::Object(m));
        }
        if include_bytes {
            out.insert("bytes".into(), json!(hex::bytes(&self.bytes)));
        }
        out
    }

    /// The record's world position in pixels, **decoded whether or not the slot is active**.
    ///
    /// [`to_json`](Self::to_json) omits `x`/`y` on an inactive slot and must go on doing so: there, an
    /// omission is the honest answer to *what is in this slot*. This accessor exists for the one caller
    /// that is answering a different question — `emulator/object_spawn`/`_move` re-read the record they
    /// just wrote, know the game wrote those bytes, and owe the fragment a REQUIRED `x`/`y` even in the
    /// rare case where the object removed itself inside the advanced frame. It is the same decode, at
    /// the same layout-owned offsets, so the two cannot drift.
    pub fn position(&self) -> (i64, i64) {
        let spec = self.layout.spec;
        (
            signed(be(&self.bytes, spec.x_pixel_offset, 2), 2),
            signed(be(&self.bytes, spec.y_pixel_offset, 2), 2),
        )
    }

    /// The absolute address `code` names, when the layout resolved the code bank.
    ///
    /// `code_addr` is an **offset**, not an address (`objcodebase.emp`: *"Every object routine's code_addr
    /// is `label - ObjCodeBase`"*), so nothing outside aeon can turn one into an address without
    /// `ObjCodeBase`. When that symbol is absent this is `None` and `name`/`nameDisp` are omitted.
    pub fn code_target(&self) -> Option<u32> {
        if !self.active() {
            return None;
        }
        let spec = self.layout.spec;
        let code = be(&self.bytes, spec.code_offset, spec.code_width);
        self.layout.code_base.map(|b| b.wrapping_add(code))
    }

    fn field_value(&self, f: &FieldSpec) -> Value {
        let raw = be(&self.bytes, f.offset, f.width);
        match f.repr {
            Repr::U => json!(raw),
            Repr::I => json!(signed(raw, f.width)),
            Repr::Hex => json!(hex_of(raw, f.width)),
        }
    }
}

/// `0x` + exactly `width * 2` uppercase hex digits — `$defs/hex`'s spelling at the field's own width, so a
/// two-byte datum does not arrive looking like a four-byte one.
fn hex_of(v: u32, width: u32) -> String {
    format!("0x{:0>width$X}", v, width = (width * 2) as usize)
}

/// The slot's own label, when the listing names it **unambiguously**.
///
/// `role` is the server's label for the slot, from the layout, and it is OPTIONAL. This server derives it
/// the only way it can without inventing a vocabulary: the symbol that names the slot's base address. That
/// is aeon's own name for the slot (`Player_2`), it round-trips through `emulator/lookup_symbol` (§4), and
/// it is a fact about the **slot** rather than its occupant — which is why the delta ruling's M5 lets it
/// survive `active: false`.
///
/// **Ambiguity is an omission, never a pick.** A `mark` and a label may share one address (`Object_RAM`
/// and `Player_1` do), and choosing between them would be a guess dressed as an answer. Zero or several
/// symbols at the address means no `role`.
pub fn slot_role(table: &SymbolTable, addr: u32) -> Option<String> {
    let at = table.symbols_at(addr);
    match at.as_slice() {
        [one] => Some(one.name.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_endian_widths_and_sign_extension() {
        let rec = [0x80u8, 0x01, 0xFF, 0xFE];
        assert_eq!(be(&rec, 0, 1), 0x80);
        assert_eq!(be(&rec, 0, 2), 0x8001);
        assert_eq!(be(&rec, 0, 4), 0x8001_FFFE);
        // A negative pixel coordinate is the whole reason `x` is signed: an object one pixel left of the
        // level origin must not read as 65535.
        assert_eq!(signed(0xFFFF, 2), -1);
        assert_eq!(signed(0x8000, 2), -32768);
        assert_eq!(signed(0x7FFF, 2), 32767);
        assert_eq!(signed(0xFF, 1), -1);
    }

    #[test]
    fn hex_of_pads_to_the_fields_own_width() {
        assert_eq!(hex_of(0x2A18, 2), "0x2A18");
        assert_eq!(hex_of(0x18, 2), "0x0018");
        assert_eq!(hex_of(0x18, 1), "0x18");
        assert_eq!(hex_of(0xA1C4, 4), "0x0000A1C4");
    }

    /// The catalogue must not name a byte of the overlay window, and must not run off the record.
    #[test]
    fn the_field_catalogue_stays_inside_the_declared_struct_fields() {
        // `sst_custom` opens at $30 and runs to the end of the record. §11.25 rule (3) forbids reporting a
        // key that is addressable but not live for the slot's occupant, and every byte of that window is
        // exactly that.
        const CUSTOM_WINDOW_START: u32 = 0x30;
        for f in AEON_SST_FIELDS {
            assert!(
                f.offset + f.width <= CUSTOM_WINDOW_START,
                "{} at ${:X}+{} reaches into the sst_custom overlay window",
                f.name,
                f.offset,
                f.width
            );
            assert!(
                matches!(f.width, 1 | 2 | 4),
                "{}: width {} is not a 68000 operand size",
                f.name,
                f.width
            );
        }
        // No two fields claim the same name — a duplicate would make `resolve_fields` answer the first and
        // silently drop the second.
        let mut names: Vec<&str> = AEON_SST_FIELDS.iter().map(|f| f.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate field name in the catalogue");
    }
}
