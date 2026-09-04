//! **The Objects panel** — the live object pool, the player slots, and one addressed slot, in one tab.
//!
//! Three served rows (`emulator/object_list`, `emulator/player_state`, `emulator/object_slot`) and **one
//! tab**, per design §2.1: `player_state` is a *section* of it, because a separate tab would be the same
//! table with a different filter, and `object_slot` is a *row expansion*, because that is what a row click
//! shows.
//!
//! # This is R1 in its purest form, and that is also its one real hazard
//!
//! `oracle_aether::decoders` is public and the three handlers use it. The functions below call **the same
//! [`decoders::derive`] over the same [`engine::debug_read`] bytes**, wrap them in the same
//! [`decoders::DecodedRecord`], and end each item with the same [`engine::attach_code_name`]. There is no
//! second decoder to drift from, and no wire between them.
//!
//! ⚑ **Which is exactly why a parity test alone would prove nothing here.** A pair held together by one
//! shared derivation is *structurally blind* to a defect in the thing it shares: break the decoder and
//! both sides move together, agreeing perfectly and both wrong. `mod bus_parity` below therefore carries a
//! third clause that compares the decode against **values the test itself wrote into the record**, so a
//! decoder reduced to a pass-through, to a constant, or to the raw bytes fails it while the agreement
//! legs stay green. See [`bus_parity::the_decode_is_a_projection_and_not_a_pass_through`].
//!
//! # Reads go direct, and none of these rows is gated
//!
//! Design §4.4 route (a): a panel body repainting at 60 Hz reads the shared derivation directly rather
//! than through [`crate::bus::Bus::call`]. Checked rather than assumed — fifteen served methods refuse a
//! running machine, and `object_list` / `player_state` / `object_slot` are **not** among them (the three
//! that hide behind `objreq_exchange` are `object_spawn`/`_move`/`_delete`, which this tab does not have:
//! per the owner's ruling this is a thing you LOOK AT). So the panel repaints while the game plays, which
//! is the state in which a live object pool is worth looking at at all.
//!
//! # ⚑ The refusal is a feature of this tab, not an error path
//!
//! [`decoders::derive(None)`](decoders::derive) refuses outright: *"no symbol table is loaded, so no
//! object layout can be derived"*. So a player launched with no `.lst` **has no Objects content**, and
//! [`Objects::Refused`] is what the tab renders — the server's own `-32012`, verbatim, plus what to do
//! about it. Never an empty table: an empty table asserts *this game has no objects*, which is a
//! different claim and a false one. It is the same call `emulator/object_list` makes, and it is the path
//! a user hits first.
//!
//! An empty pool that *did* derive is a third thing again, and is rendered as its own sentence — the
//! handler's `total: 0` beside `truncated: false` is "zero objects" as a stated fact, and so is this.

use oracle_aether::decoders::{self, DecodedRecord, ObjectLayout};
use oracle_aether::engine;
use oracle_aether::hex;
use oracle_aether::rpc::{code, RpcError};
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------------------------------
// One row — the handler's own item, built by the handler's own steps
// ---------------------------------------------------------------------------------------------------

/// One slot as the panel shows it, carrying **the map a served reply carries for that slot**.
///
/// The map rather than a struct of parsed-out fields, deliberately. The contract calls this shape a
/// *closed envelope over an open payload*: `slot`/`addr`/`x`/`y`/`code`/`name`/`nameDisp`/`bytes` are
/// fixed and `fields` is a typed-open map whose key set is a function of the loaded game. A struct here
/// would have to close what the contract deliberately left open, and would then quietly stop showing a
/// field the next engine declares.
#[derive(Debug)]
pub struct Row {
    pub slot: u32,
    pub addr: u32,
    /// The engine's own empty-slot sentinel, read at the offset the layout names — never a heuristic.
    pub active: bool,
    /// Exactly the item `emulator/object_list` / `player_state` / `object_slot` serve for this slot.
    pub item: Map<String, Value>,
}

impl Row {
    /// A contract key as a display string, or `"—"`. **Never blank**: a blank cell in a table of numbers
    /// is indistinguishable from a zero that failed to render, and an inactive slot's omitted keys are an
    /// answer ("the game never wrote this record") rather than a gap.
    pub fn cell(&self, key: &str) -> String {
        match self.item.get(key) {
            None => "—".into(),
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
        }
    }

    /// The row's own headline: what a human scans a pool table for.
    pub fn summary(&self) -> String {
        format!(
            "{:>3}  {:<10}  {:<8}  {:>7} {:>7}  {}",
            self.slot,
            self.cell("addr"),
            self.cell("code"),
            self.cell("x"),
            self.cell("y"),
            match self.item.get("name") {
                Some(Value::String(n)) => match self.item.get("nameDisp").and_then(Value::as_u64) {
                    Some(0) | None => n.clone(),
                    Some(d) => format!("{n}+${d:X}"),
                },
                // `ObjCodeBase` absent, or nothing resolves at the target. Said, not blanked — the two
                // reasons a name is missing are both facts about the listing, not about the object.
                _ => "(no name — ObjCodeBase absent, or nothing resolves there)".into(),
            }
        )
    }
}

/// Read one whole record and decode it, **the identical two steps `Engine::slot_record` performs**: the
/// layout's own address, the layout's own stride, and `debug_read` — the same function `emulator/read`
/// and `emulator/read_memory` resolve through.
fn record<'a>(
    layout: &'a ObjectLayout,
    sys: &System,
    slot: u32,
) -> Result<DecodedRecord<'a>, RpcError> {
    let addr = layout.slot_addr(slot);
    let (bytes, _) = engine::debug_read(sys, addr, layout.slot_bytes() as usize)?;
    Ok(DecodedRecord::new(layout, slot, addr, bytes))
}

/// Which of the optional pieces a row carries. The three served rows differ **only** in these, which is
/// why they are one struct rather than four positional booleans: `object_list` omits `active` because
/// presence is activity there, `player_state` carries `active` and `role`, and `object_slot` carries
/// `active`, every declared field and the raw record.
#[derive(Clone, Copy, Default)]
struct RowOpts<'f> {
    fields: Option<&'f [&'f decoders::FieldSpec]>,
    include_bytes: bool,
    include_active: bool,
    include_role: bool,
}

/// Turn a decoded record into the row a reply carries, with the same optional pieces the handlers attach.
fn row(
    layout: &ObjectLayout,
    symbols: Option<&SymbolTable>,
    rec: &DecodedRecord<'_>,
    slot: u32,
    opts: RowOpts<'_>,
) -> Row {
    let RowOpts {
        fields,
        include_bytes,
        include_active,
        include_role,
    } = opts;
    let mut item = rec.to_json(true, fields, include_bytes);
    if include_active {
        // REQUIRED where it appears, `false` included: false is the answer, not the absence of one.
        item.insert("active".into(), json!(rec.active()));
    }
    if rec.active() {
        engine::attach_code_name(&mut item, symbols, rec);
    }
    if include_role {
        // **Survives inactivity** — the label is the slot's, not the occupant's.
        if let Some(table) = symbols {
            if let Some(r) = decoders::slot_role(table, layout.slot_addr(slot)) {
                item.insert("role".into(), json!(r));
            }
        }
    }
    Row {
        slot,
        addr: layout.slot_addr(slot),
        active: rec.active(),
        item,
    }
}

// ---------------------------------------------------------------------------------------------------
// The three views — one per served row
// ---------------------------------------------------------------------------------------------------

/// `emulator/object_list`'s answer, panel-side: **the active slots only**, because presence *is* activity
/// and an empty slot is not an item. Slot numbers are therefore sparse, exactly as the reply's are.
pub struct ListView {
    pub layout: ObjectLayout,
    /// Active objects in the whole pool. Equal to `objects.len()` here — the panel imposes no `limit`,
    /// so nothing is truncated — and kept as its own number because the reply keeps it as its own key.
    pub total: usize,
    pub objects: Vec<Row>,
}

/// `emulator/player_state`'s answer: the player pool **including inactive slots**, because "player 2 is
/// not present" is the answer to the question asked and not something to infer from a shorter list.
pub struct PlayerView {
    pub layout: ObjectLayout,
    pub players: Vec<Row>,
}

/// `emulator/object_slot`'s answer: one addressed slot, with every field the layout declares and the raw
/// record beside them.
pub struct SlotView {
    pub layout: ObjectLayout,
    pub row: Row,
}

/// The live pool, decoded — or the refusal that stands in for the whole tab.
pub fn object_list(symbols: Option<&SymbolTable>, sys: &System) -> Result<ListView, RpcError> {
    let layout = decoders::derive(symbols)?;
    let mut objects = Vec::new();
    for slot in 0..layout.slot_count() {
        let rec = record(&layout, sys, slot)?;
        if !rec.active() {
            continue;
        }
        objects.push(row(&layout, symbols, &rec, slot, RowOpts::default()));
    }
    Ok(ListView {
        total: objects.len(),
        layout,
        objects,
    })
}

/// The player pool. Refuses when the listing does not partition the table — the same call
/// [`decoders::derive`] makes about the base address, one level down, and the reason this section can be
/// refused while the pool table above still renders.
pub fn player_state(symbols: Option<&SymbolTable>, sys: &System) -> Result<PlayerView, RpcError> {
    let layout = decoders::derive(symbols)?;
    let (first, count) = {
        let pool = layout.player_pool()?;
        (pool.first_slot, pool.slot_count)
    };
    let mut players = Vec::with_capacity(count as usize);
    for slot in first..first + count {
        let rec = record(&layout, sys, slot)?;
        players.push(row(
            &layout,
            symbols,
            &rec,
            slot,
            RowOpts {
                include_active: true,
                include_role: true,
                ..RowOpts::default()
            },
        ));
    }
    Ok(PlayerView { layout, players })
}

/// One addressed slot, with **every field the layout declares** and the raw record.
///
/// The field list comes from [`ObjectLayout::field_names`] — enumerated from the catalogue the handler
/// validates against, never written down here. A copy of those names would be a fact about a build this
/// crate is not looking at, which is the one thing this whole decoder family refuses to hold.
pub fn object_slot(
    symbols: Option<&SymbolTable>,
    sys: &System,
    slot: u32,
) -> Result<SlotView, RpcError> {
    let layout = decoders::derive(symbols)?;
    let names = layout.field_names();
    // Round-tripped through the layout's own resolver rather than used directly, so a name the catalogue
    // publishes but will not resolve is a loud failure here instead of a silently missing column.
    let owned: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
    let specs = layout.resolve_fields(&owned)?;
    // The bound is the pool's, refused rather than clamped — the same shape `emulator/object_slot`
    // answers. Unreachable from a row click (the expansion is opened by a row that exists) and kept
    // because a slot number is an *address*, and an address a caller chose must be checked by the callee.
    if slot >= layout.slot_count() {
        return Err(RpcError::invalid_params(format!(
            "slot {slot} is past the end of the object pool — this build has {} slots (0..={}), and \
             the bound is refused rather than clamped",
            layout.slot_count(),
            layout.slot_count().saturating_sub(1)
        )));
    }
    let rec = record(&layout, sys, slot)?;
    let r = row(
        &layout,
        symbols,
        &rec,
        slot,
        RowOpts {
            fields: Some(&specs),
            include_bytes: true,
            include_active: true,
            include_role: false,
        },
    );
    Ok(SlotView { layout, row: r })
}

// ---------------------------------------------------------------------------------------------------
// Rings — the question this tab kept provoking, answered where it is asked
// ---------------------------------------------------------------------------------------------------

/// The two symbols this derivation reads, named once each.
///
/// A **symbol name** is not a hardcoded address: it is the same kind of fact `AEON_SST.base_symbols`
/// holds when it names `Object_RAM`. Every *number* below is measured from the listing at run time.
const RING_BUFFER: &str = "Ring_Buffer";
const RING_COUNT: &str = "Ring_Count";

/// **Why the ring buffer's capacity is not on this panel**, stated in full because an absent number
/// invites the reader to supply one.
///
/// The span is derivable and is shown. Turning a span in *bytes* into a ceiling in *rings* needs the
/// entry size, and `RING_BUFFER_ENTRY_SIZE` is published by the listing only in its `Equate Table` —
/// a section [`SymbolTable::parse`] recognises and deliberately consumes **without keeping the values**
/// (`F-EQUATES-NAMESPACE`, an unruled decision). So the divisor is not a number this process can read,
/// and the one thing worse than the gap would be closing it with a constant typed in here: an entry size
/// written into this crate is a fact about one build, which is exactly what the rest of this file refuses
/// to hold.
///
/// [`rings_ceiling_is_unknown_because_equate_values_are_not_ingested`] pins the *reason* rather than the
/// sentence, so the day equates become readable this goes red and asks for the division to be finished.
pub const CEILING_UNKNOWN: &str =
    "ceiling unknown — the span above is measured, but converting it to a number of rings needs \
     RING_BUFFER_ENTRY_SIZE, which the listing publishes only as an equate and which the symbol table \
     deliberately does not ingest (F-EQUATES-NAMESPACE). No entry size is guessed here.";

/// The one sentence this panel owes the reader about rings, beside the object count.
pub const RINGS_WHY: &str =
    "Rings never occupy an object slot — they live in their own buffer, so a full ring buffer does not \
     consume the pool and a ring is never one of the objects counted above.";

/// What the panel can say about rings, measured the same way everything else on this tab is.
pub struct RingsView {
    /// `Ring_Buffer`, where the buffer starts.
    pub buffer_addr: u32,
    /// `Ring_Count`, the live entry count — and, being the next symbol in memory, the buffer's end.
    pub count_addr: u32,
    /// `Ring_Count − Ring_Buffer`: the buffer's whole span in bytes.
    ///
    /// ⚑ **This is only the span because the two symbols are adjacent**, and that adjacency is a fact
    /// about the loaded listing rather than an assumption — a third symbol between them would make the
    /// subtraction meaningless and put a confident wrong number on screen. Checked at derive time by
    /// [`rings`], not asserted here.
    pub span_bytes: u32,
    /// The width of `Ring_Count` itself, **measured** as the gap to the next symbol above it — the same
    /// technique `derive` uses for the record stride, for the same reason.
    pub count_width: u32,
    /// The live value read out of `Ring_Count`.
    pub count: u32,
}

impl RingsView {
    /// The rings line as the header shows it — **a string, so it can be asserted on.**
    ///
    /// Same reason [`Row::summary`] is a method here rather than a `format!` in the renderer: a panel
    /// this crate cannot screenshot is only checkable at the seam where it becomes text.
    pub fn summary(&self) -> String {
        format!(
            "rings   {} live   buffer {}..{} (${:X} bytes)   Ring_Count is {} byte{}",
            self.count,
            hex::addr(self.buffer_addr),
            hex::addr(self.count_addr),
            self.span_bytes,
            // Shown because it is what makes the count readable at all: the width was MEASURED from the
            // next symbol, not assumed, and reading two bytes where the listing declares one would fold
            // `Ring_HighWater` into the count and report a plausible number.
            self.count_width,
            if self.count_width == 1 { "" } else { "s" },
        )
    }
}

/// The smallest symbol address strictly above `addr`, i.e. where whatever lives at `addr` must end.
///
/// [`SymbolTable::symbols`] is address-sorted, so the first hit is the nearest.
fn next_symbol_above(table: &SymbolTable, addr: u32) -> Option<u32> {
    table.symbols().iter().map(|s| s.addr).find(|a| *a > addr)
}

/// **The ring buffer, read out of the listing** — or the sentence saying which symbol did not answer.
///
/// Its own `Result`, like the player section's, because a listing can locate the object table and say
/// nothing about rings; that is a gap in one line of this panel and not a reason to lose the tab.
pub fn rings(symbols: Option<&SymbolTable>, sys: &System) -> Result<RingsView, RpcError> {
    let Some(table) = symbols else {
        // The same refusal `decoders::derive` opens with, and unreachable from the panel for the same
        // reason: no listing is already a whole-tab state above this point.
        return Err(RpcError::new(
            code::NO_SYMBOLS_LOADED,
            "no symbol table is loaded, so the ring buffer cannot be located",
        ));
    };
    let (Some(buffer_addr), Some(count_addr)) =
        (table.address_of(RING_BUFFER), table.address_of(RING_COUNT))
    else {
        return Err(RpcError::new(
            code::NO_SYMBOLS_LOADED,
            format!(
                "the loaded listing does not name both `{RING_BUFFER}` and `{RING_COUNT}`, so the ring \
                 buffer cannot be measured — refusing rather than reporting a ring count from a guessed \
                 address"
            ),
        ));
    };

    // ⚑ **The adjacency check, and the whole reason `span_bytes` means anything.** `Ring_Count` is the
    // buffer's end ONLY while nothing else lives between the two. If some third symbol is in there, the
    // subtraction is measuring an unrelated region and the honest answer is to refuse it.
    let Some(span_bytes) = count_addr.checked_sub(buffer_addr).filter(|s| *s > 0) else {
        return Err(RpcError::new(
            code::NO_SYMBOLS_LOADED,
            format!(
                "`{RING_COUNT}` ({}) does not lie above `{RING_BUFFER}` ({}), so the span between them \
                 is not the ring buffer",
                hex::addr(count_addr),
                hex::addr(buffer_addr)
            ),
        ));
    };
    match next_symbol_above(table, buffer_addr) {
        Some(next) if next == count_addr => {}
        other => {
            return Err(RpcError::new(
                code::NO_SYMBOLS_LOADED,
                format!(
                    "`{RING_COUNT}` is not the next symbol after `{RING_BUFFER}` in this listing — {} \
                     comes first — so `{RING_COUNT} − {RING_BUFFER}` spans more than the ring buffer \
                     and is not reported as its size",
                    other.map_or_else(
                        || "nothing at all".to_string(),
                        |a| format!("a symbol at {}", hex::addr(a))
                    ),
                ),
            ));
        }
    }

    // The count's own width, measured the same way. Refused rather than assumed: reading two bytes where
    // the listing declares one would fold `Ring_HighWater` into the count and report a plausible number.
    let count_width = match next_symbol_above(table, count_addr)
        .and_then(|n| n.checked_sub(count_addr))
    {
        Some(w @ (1 | 2 | 4)) => w,
        other => {
            return Err(RpcError::new(
                code::NO_SYMBOLS_LOADED,
                format!(
                    "`{RING_COUNT}`'s width is measured as the gap to the next symbol above it, and \
                     this listing makes that {} — not a 1-, 2- or 4-byte scalar, so the value is not \
                     read",
                    other.map_or("unbounded".to_string(), |w| format!("{w} bytes")),
                ),
            ));
        }
    };
    // `debug_read` — the same function `emulator/read_memory` resolves through, and the same one every
    // object record on this tab is read with.
    let (bytes, _) = engine::debug_read(sys, count_addr, count_width as usize)?;
    let count = bytes.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b));

    Ok(RingsView {
        buffer_addr,
        count_addr,
        span_bytes,
        count_width,
        count,
    })
}

// ---------------------------------------------------------------------------------------------------
// What the tab draws for one repaint
// ---------------------------------------------------------------------------------------------------

/// The tab's whole content for one repaint: either the refusal, or the pool.
///
/// **An enum rather than a struct with an `error` beside empty vectors**, so "there is no layout" cannot
/// be rendered as a table with no rows by any renderer, however careless — the empty table is not
/// reachable from this type. That distinction is the tab's central correctness claim: no symbols is not
/// the same fact as no objects.
pub enum Objects {
    /// [`decoders::derive`] refused. The server's own `-32012` and its own sentence.
    Refused(RpcError),
    /// Boxed: [`Pool`] grew past the refusal variant when it took the layout's fields and the ring
    /// buffer, and an enum sized to its largest arm would make every refusal carry the pool's footprint.
    /// One allocation per repaint, which is nothing beside the pool read that produced it.
    Pool(Box<Pool>),
}

/// A derived layout and everything under it.
///
/// ⚑ **The `layout` object's own fields, not the object.** This used to carry `layout` as a
/// [`serde_json::Value`] and the header rendered it with `{}`, which put a line of raw JSON —
/// `{"baseAddr":"0x00FF8000","detectedBy":"symbol",…}` — across the top of the tab. Every fact in it was
/// already worth showing; none of them was readable in that spelling. So the facts are carried as facts
/// and the header spells them, which is the same information and not a subset: `detectedBy` is the one
/// key dropped, and it is `"symbol"` unconditionally on this server (there is no configured-base path),
/// so rendering it would have been a constant dressed as a finding.
pub struct Pool {
    pub engine: &'static str,
    pub slot_count: u32,
    pub slot_bytes: u32,
    /// `layout.baseAddr` — where the table starts.
    pub base_addr: u32,
    /// `layout.detectedFrom` — **which symbol answered** for that base.
    pub detected_from: &'static str,
    /// `layout.pools[]`, or `None` when the listing does not partition the table. `None` here is the
    /// same fact `players` refuses on, and the header says so rather than omitting the line.
    pub partition: Option<Vec<decoders::Pool>>,
    pub total: usize,
    pub objects: Vec<Row>,
    /// The player section, or **its own refusal**: a listing can locate the table and still not partition
    /// it, and "which slots are players" is then unanswerable while the pool table above is fine.
    pub players: Result<PlayerView, RpcError>,
    /// The ring buffer, or its own refusal — for the same reason, one line further down: a listing that
    /// names the object table need not name the ring buffer, and that is a missing line rather than a
    /// missing tab.
    pub rings: Result<RingsView, RpcError>,
}

impl Pool {
    /// The two layout lines the header shows, **as strings rather than as a `Value`**.
    ///
    /// This is the readable half of what used to be one `ui.monospace` line ending in a whole JSON
    /// object. Returned as text so a test can assert both halves of the fix: that the facts are all still
    /// here, and that no `{"key":` survives into the header.
    pub fn layout_lines(&self) -> Vec<String> {
        vec![
            format!(
                "engine {}   table {}   {} slots × ${:X} bytes   base from {}",
                self.engine,
                hex::addr(self.base_addr),
                self.slot_count,
                self.slot_bytes,
                self.detected_from,
            ),
            match &self.partition {
                Some(ps) => format!(
                    "pools    {}",
                    ps.iter()
                        .map(|p| format!(
                            "{} {}..{}",
                            p.name,
                            p.first_slot,
                            p.first_slot + p.slot_count
                        ))
                        .collect::<Vec<_>>()
                        .join("   ")
                ),
                // Said, not omitted: this is the same missing partition the player section refuses on,
                // and a header that skipped the line would leave that refusal looking like a bug.
                None => "pools    — this listing does not partition the table".to_string(),
            },
        ]
    }
}

impl Objects {
    /// Read the whole tab. One derive per served row, as the handlers do — the panel does not cache a
    /// layout across a repaint, for `ObjectLayout`'s own stated reason: `emulator/load_symbols` may be
    /// called at any point in a session, so a cached layout is stale by construction.
    pub fn of(symbols: Option<&SymbolTable>, sys: &System) -> Objects {
        let list = match object_list(symbols, sys) {
            Err(e) => return Objects::Refused(e),
            Ok(v) => v,
        };
        let players = player_state(symbols, sys);
        let rings = rings(symbols, sys);
        Objects::Pool(Box::new(Pool {
            rings,
            engine: list.layout.engine(),
            slot_count: list.layout.slot_count(),
            slot_bytes: list.layout.slot_bytes(),
            base_addr: list.layout.base_addr(),
            detected_from: list.layout.detected_from(),
            partition: list.layout.pools().map(<[_]>::to_vec),
            total: list.total,
            objects: list.objects,
            players,
        }))
    }
}

/// The sentence the tab shows in place of its content when no layout could be derived.
///
/// The server's code and message come first and **verbatim** — the panel composes no refusal of its own
/// about a server it is living inside — and the player's own two remedies follow, because `-32012` names
/// `emulator/load_symbols`, which is not a thing a human at this window can call.
pub fn refusal_text(e: &RpcError) -> String {
    format!(
        "NO OBJECTS TO SHOW — {} {}\n\nThis tab decodes the game's object records, and every address in \
         them is a fact about the build that is loaded, so without a listing there is nothing to decode \
         and nothing is guessed. Relaunch with `--symbols PATH`, or put the matching `.lst` beside the \
         ROM.\n\nThis is not an empty pool: an empty table would say this game has no objects, which is \
         a different claim.",
        e.code, e.message
    )
}

/// Everything the Objects tab remembers between repaints.
#[derive(Default)]
pub struct ObjectsPanel {
    /// The expanded row, i.e. which slot `object_slot` is being asked about. `None` is the collapsed
    /// table.
    pub selected: Option<u32>,
}

// ---------------------------------------------------------------------------------------------------
// The parity invariants — design §4.4 R3, plus the clause a parity pair cannot supply
// ---------------------------------------------------------------------------------------------------

/// **This panel and the three ⚙ decoder rows must never disagree** — and, separately, **the derivation
/// they share must actually decode something**.
///
/// The second half is the one this module owes more than any panel before it. 2a's Registers panel and
/// 2b's Memory panel share a *function* with their handlers; this one shares the entire decoder. A
/// mutation to `DecodedRecord::to_json` moves the panel and the reply together, so every agreement
/// assertion below stays green while both sides report the wrong thing. That is why
/// [`the_decode_is_a_projection_and_not_a_pass_through`] compares against values written into the record
/// by this test rather than against the bus.
///
/// `oracle-aether` is `#![cfg(unix)]`, so this module is too.
#[cfg(all(test, unix))]
mod bus_parity {
    use super::*;
    use crate::bus::{Answer, Bus};
    use oracle_aether::host::MachineInfo;

    // -----------------------------------------------------------------------------------------------
    // The fixture — a layout DERIVED from named sources, and a pool that is actually populated
    // -----------------------------------------------------------------------------------------------

    /// `sst.emp` at aeon `f4896139`: `pub struct Sst (size: $50)`. The server does not hold this number —
    /// it measures the stride from two adjacent slot symbols — so the test is the side that must.
    const SST: u32 = 0x50;
    /// `engine/system/constants.emp:78-90`.
    const NUM_PLAYERS: u32 = 2;
    const NUM_DYNAMIC: u32 = 40;
    const NUM_SYSTEM: u32 = 8;
    const NUM_EFFECTS: u32 = 16;
    const NUM_TOTAL: u32 = NUM_PLAYERS + NUM_DYNAMIC + NUM_SYSTEM + NUM_EFFECTS;
    /// Work RAM, and low enough that the whole `NUM_TOTAL * SST` span fits inside the window
    /// `debug_read` will serve.
    const BASE: u32 = 0x00FF_8000;
    /// `objcodebase.emp:4-6`: *"The object code bank starts at `$10000` (ObjCodeBase)"*. Inside the
    /// fixture ROM, so a planted symbol there is an address the machine really has.
    const OBJ_CODE_BASE: u32 = 0x0001_0000;

    /// `ram.emp:612-618`'s declaration order, as listing rows. Addresses **computed** from base and
    /// stride, never listed: a table of literals would let this test agree with a server holding the
    /// same literals, which is the property under test.
    fn pool_rows(base: u32, stride: u32) -> Vec<(String, u32)> {
        let dynamic = base + NUM_PLAYERS * stride;
        let system = dynamic + NUM_DYNAMIC * stride;
        let effect = system + NUM_SYSTEM * stride;
        vec![
            ("Object_RAM".into(), base),
            ("Player_1".into(), base),
            ("Player_2".into(), base + stride),
            ("Dynamic_Slots".into(), dynamic),
            ("System_Slots".into(), system),
            ("Effect_Slots".into(), effect),
            ("Object_RAM_End".into(), effect + NUM_EFFECTS * stride),
            ("ObjCodeBase".into(), OBJ_CODE_BASE),
        ]
    }

    /// An AS-dialect listing, in the spelling `SymbolTable::parse` takes.
    fn listing(rows: &[(String, u32)]) -> String {
        let mut s = String::from("  Symbol Table (* = unused):\n\n");
        for (name, addr) in rows {
            s.push_str(&format!(" {name} : {addr:X} C |\n"));
        }
        s.push_str(&format!("\n{:>4} symbols\n", rows.len()));
        s
    }

    fn table_with(extra: &[(String, u32)]) -> SymbolTable {
        let mut rows = pool_rows(BASE, SST);
        rows.extend_from_slice(extra);
        SymbolTable::parse(&listing(&rows)).expect("a parsable listing")
    }

    fn booted() -> System {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        sys.run_frames(7);
        sys
    }

    fn ok(a: Answer) -> Value {
        match a {
            Answer::Ok(v) => v,
            Answer::Err(e) => panic!("expected an answer, got {} {}", e.code, e.message),
        }
    }

    /// Zero the whole table, so "active" means "this test wrote a code word here" and nothing else.
    ///
    /// **Not decoration.** `System::new(seed)` does not promise zeroed work RAM, and a slot that reads
    /// active because of a fill pattern would make every count below a number nobody chose.
    fn zero_pool(bus: &mut Bus, sys: &mut System) {
        let span = (NUM_TOTAL * SST) as usize;
        let chunk = 4096usize; // `limits.maxWriteLen`
        let mut off = 0usize;
        while off < span {
            let n = chunk.min(span - off);
            ok(bus.call(
                sys,
                "emulator/write_memory",
                &json!({
                    "addr": format!("0x{:06X}", BASE + off as u32),
                    "bytes": format!("0x{}", "00".repeat(n)),
                }),
            ));
            off += n;
        }
    }

    /// One live object: a non-zero `code_addr` at `$00` (the engine's own activity sentinel) plus **both
    /// 16.16 position words in full**, laid down as one ten-byte payload through the bus.
    ///
    /// The sub-pixel halves are seeded with real values rather than left zero, and that is load-bearing
    /// rather than thorough: the contract carries world **pixels**, i.e. the high word only, and a decode
    /// reading all four bytes is indistinguishable from the right one whenever the low word is `0000`.
    fn poke_object(bus: &mut Bus, sys: &mut System, slot: u32, code: u16, x: u32, y: u32) {
        ok(bus.call(
            sys,
            "emulator/write_memory",
            &json!({
                "addr": format!("0x{:06X}", BASE + slot * SST),
                "bytes": format!("0x{code:04X}{x:08X}{y:08X}"),
            }),
        ));
    }

    /// The populated fixture every agreement leg runs against: a booted machine, a paused bus carrying
    /// the listing, and **five live objects in three different pools** with distinct codes and positions.
    ///
    /// Paused because `emulator/write_memory` is one of the fifteen paused-only rows; the panel's own
    /// reads are not gated, which is why they still work here and would still work running.
    fn populated(sys: &mut System, table: SymbolTable) -> Bus {
        let mut bus = Bus::new(
            sys,
            MachineInfo {
                rom_path: Some("testrom".into()),
                symbols: Some(table),
                symbols_path: Some("testrom.lst".into()),
            },
            true,
            None,
        );
        zero_pool(&mut bus, sys);
        // Slots 0 and 1 are the player pool; 2 and 3 are dynamic; 50 is a system slot. Every code, x and
        // y is distinct, so a decoder that returned one record for every slot fails the not-all-alike
        // assertion below rather than passing on five identical rows.
        for (slot, code, x, y) in LIVE {
            poke_object(&mut bus, sys, slot, code, x, y);
        }
        bus
    }

    /// `(slot, code_addr, x_pos, y_pos)` — the positions are the **whole 16.16 words**, sub-pixel half
    /// included.
    ///
    /// Slot 3 carries the two values the projection check turns on: a non-zero sub-pixel half (so a
    /// four-byte read of `x` cannot pass for the two-byte one) and a `y` whose high word is negative once
    /// sign-extended (so an unsigned read answers a believable 65533 instead of `-3`).
    const LIVE: [(u32, u16, u32, u32); 5] = [
        (0, 0x1234, 0x0100_0000, 0x0080_0000),
        (1, 0x2468, 0x0140_0000, 0x0080_0000),
        (2, 0x00A2, 0x02A0_0000, 0x01F0_0000),
        (3, 0x1000, 0x0300_BEEF, 0xFFFD_1234),
        (50, 0x0002, 0x0001_0000, 0x0002_0000),
    ];

    /// Every served item, keyed by slot, from one reply's array.
    fn served_items(v: &Value, key: &str) -> Vec<Map<String, Value>> {
        v.get(key)
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("the reply carries a `{key}` array, got {v}"))
            .iter()
            .map(|i| match i {
                Value::Object(m) => m.clone(),
                other => panic!("every item is an object, got {other}"),
            })
            .collect()
    }

    // -----------------------------------------------------------------------------------------------
    // 1. Agreement — the panel against each of the three rows
    // -----------------------------------------------------------------------------------------------

    /// **`object_list`: the panel's pool table against the bus's own reply, item for item.**
    ///
    /// ⚑ **The pool is populated first, and asserted to be populated.** At reset — and seven frames into
    /// the fixture ROM — every slot's `code_addr` is whatever work RAM happens to hold, and after
    /// [`zero_pool`] it is nothing at all: an unpopulated run of this test would compare an empty array
    /// against an empty array and pass forever, while a panel reading entirely the wrong address did the
    /// same. So the count is asserted non-zero, asserted to be the count this test wrote, and the rows
    /// are asserted not to be all alike.
    #[test]
    fn the_objects_panel_lists_what_emulator_object_list_lists() {
        let mut sys = booted();
        let table = table_with(&[]);
        let mut bus = populated(&mut sys, table.clone());

        let reply = ok(bus.call(&mut sys, "emulator/object_list", &json!({})));
        let served = served_items(&reply, "objects");
        let panel = object_list(Some(&table), &sys).expect("the layout derives");

        // --- the anti-vacuity guard, before any comparison ---
        assert_eq!(
            panel.total,
            LIVE.len(),
            "the pool is not populated as this test intended, so every comparison below would be \
             empty-against-empty and would pass whatever the panel read. Served: {reply}"
        );
        assert_eq!(
            reply["total"],
            json!(LIVE.len()),
            "the bus disagrees about how many objects are live"
        );
        let codes: std::collections::BTreeSet<String> =
            panel.objects.iter().map(|r| r.cell("code")).collect();
        assert_eq!(
            codes.len(),
            LIVE.len(),
            "the decoded rows are not all distinct ({codes:?}) — a decoder returning one record for \
             every slot would satisfy the comparison below and this is what says so"
        );

        assert_eq!(
            served.len(),
            panel.objects.len(),
            "the panel shows {} objects and the bus serves {}",
            panel.objects.len(),
            served.len()
        );
        for (s, p) in served.iter().zip(panel.objects.iter()) {
            assert_eq!(
                &p.item, s,
                "slot {} — the panel's row and `emulator/object_list`'s item have DRIFTED",
                p.slot
            );
        }
        // …and the envelope's own facts, which a client reads beside the items.
        assert_eq!(reply["layout"], panel.layout.to_json());
        assert_eq!(reply["truncated"], json!(false));
    }

    /// **`player_state`: including the inactive slots, which is the whole point of that row.**
    ///
    /// Slot 1 is deliberately live and slot 0 too, so the section is not vacuous; the *inactive* case is
    /// covered by [`an_inactive_player_slot_is_reported_as_absent_not_as_zeroes`] below rather than by
    /// hoping this fixture happens to contain one.
    #[test]
    fn the_player_section_shows_what_emulator_player_state_shows() {
        let mut sys = booted();
        let table = table_with(&[]);
        let mut bus = populated(&mut sys, table.clone());

        let reply = ok(bus.call(&mut sys, "emulator/player_state", &json!({})));
        let served = served_items(&reply, "players");
        let panel = player_state(Some(&table), &sys).expect("the player pool resolves");

        assert_eq!(
            panel.players.len(),
            NUM_PLAYERS as usize,
            "the player pool is {NUM_PLAYERS} slots by construction; a section of a different size is \
             not the section under test"
        );
        assert_eq!(served.len(), panel.players.len());
        assert!(
            panel.players.iter().all(|p| p.active),
            "this fixture makes both player slots live, so a comparison here is over decoded records \
             rather than over two `active: false` stubs"
        );
        for (s, p) in served.iter().zip(panel.players.iter()) {
            assert_eq!(
                &p.item, s,
                "slot {} — the panel's player row and `emulator/player_state`'s have DRIFTED",
                p.slot
            );
        }
        // `role` is the slot's own label and the panel must carry it: it is what tells a human which of
        // two identical-looking rows is Player 1.
        assert_eq!(panel.players[1].cell("role"), "Player_2");
        // ⚑ …and slot 0 has NO role, which is the decoder's rule and not a defect here. `Object_RAM` and
        // `Player_1` name one address, and `slot_role` omits rather than picks between them —
        // *"ambiguity is an omission, never a pick"*. Pinned rather than worked around, because the
        // tempting fix (take the first) would make the panel label a slot from a guess while the bus
        // labelled it not at all, which is precisely the drift this test exists to catch. The renderer
        // shows `—` for it, never a blank.
        assert_eq!(
            panel.players[0].cell("role"),
            "—",
            "two symbols name slot 0's address, so the label is omitted by the decoder"
        );
        assert!(!panel.players[0].item.contains_key("role"));
    }

    /// **`object_slot`: the row expansion, with every field the layout declares.**
    ///
    /// The bus side is asked for **the same params the panel used** — the field list and `includeBytes`
    /// — because a comparison of the panel's full expansion against the reply's bare envelope would be
    /// comparing two different questions and would silently drop `fields` from the check entirely.
    #[test]
    fn the_row_expansion_shows_what_emulator_object_slot_shows() {
        let mut sys = booted();
        let table = table_with(&[]);
        let mut bus = populated(&mut sys, table.clone());

        for (slot, ..) in LIVE {
            let panel = object_slot(Some(&table), &sys, slot).expect("an addressed slot decodes");
            let names = panel.layout.field_names();
            let reply = ok(bus.call(
                &mut sys,
                "emulator/object_slot",
                &json!({"slot": slot, "fields": names, "includeBytes": true}),
            ));
            let mut served = match reply.clone() {
                Value::Object(m) => m,
                other => panic!("object_slot answers an object, got {other}"),
            };
            // `layout` is the envelope's, not the item's; the panel carries it separately.
            let layout = served.remove("layout").expect("layout is REQUIRED");
            assert_eq!(layout, panel.layout.to_json());
            assert_eq!(
                panel.row.item, served,
                "slot {slot} — the expansion and `emulator/object_slot` have DRIFTED"
            );
            // Non-vacuity for this leg: an expansion that dropped `fields` would still agree with a
            // reply that dropped them too, and both would look like a tidy little table.
            let fields = panel.row.item["fields"]
                .as_object()
                .expect("the expansion carries the open field map");
            assert_eq!(
                fields.len(),
                names.len(),
                "every declared field must be shown, not a subset"
            );
            assert!(
                names.len() > 20,
                "the catalogue collapsed to {} names — the expansion is no longer the whole record",
                names.len()
            );
        }
    }

    // -----------------------------------------------------------------------------------------------
    // 2. ⚑ The clause a parity pair CANNOT supply
    // -----------------------------------------------------------------------------------------------

    /// **The shared derivation actually decoded something** — checked against values this test wrote,
    /// never against the bus.
    ///
    /// This is the clause the three legs above are structurally blind to. Panel and handler agree by
    /// construction because they run one decoder over one set of bytes; break that decoder and both move
    /// together, agreeing exactly, both wrong. Measured on the parcel before this one: reducing a shared
    /// `absolutise` to a pass-through left the panel and the served field agreeing perfectly on the
    /// un-normalised string.
    ///
    /// **What the right non-identity check is for a decoded object record**, and why each clause is here:
    ///
    /// * `code` is written as a raw word and must come back as a **width-padded hex string** at the
    ///   field's own width — so a decode that returned the number, or the whole record, fails.
    /// * `x` must be the **integer half of a 16.16 fixed-point pair**, i.e. the high word only — so a
    ///   decode reading the full 32 bits fails, and the assertion states that difference rather than just
    ///   comparing to a constant.
    /// * `y` on one slot is **negative once sign-extended**, and a raw big-endian read would give 65533.
    ///   A pass-through cannot produce `-3`.
    /// * `name` must resolve through `code_addr + ObjCodeBase`, so a renderer resolving the raw word — a
    ///   plausible, healthy-looking mistake, since `code_addr` is an offset and not an address — lands on
    ///   a different symbol and fails.
    /// * An **inactive** slot must omit the decoded keys rather than zero them, which no pass-through
    ///   does: the record it would hand back is 80 bytes of zeroes and would render as `x: 0`.
    #[test]
    fn the_decode_is_a_projection_and_not_a_pass_through() {
        let mut sys = booted();
        // A symbol planted at exactly `ObjCodeBase + code` for slot 3, and a decoy at `ObjCodeBase`
        // itself. The decoy is the load-bearing half: without it, a renderer that resolved the raw
        // `code_addr` as an address, or that ignored the offset entirely, could still miss and be
        // reported as "no name" rather than as a wrong name.
        let table = table_with(&[
            ("ObjRing".into(), OBJ_CODE_BASE + 0x1000),
            ("ObjDecoy".into(), OBJ_CODE_BASE),
        ]);
        let _bus = populated(&mut sys, table.clone());

        let panel = object_list(Some(&table), &sys).expect("the layout derives");
        let by_slot = |s: u32| {
            panel
                .objects
                .iter()
                .find(|r| r.slot == s)
                .unwrap_or_else(|| panic!("slot {s} is live in this fixture"))
        };

        // --- slot 3: x_pos = 0x0300BEEF, y_pos = 0xFFFD1234 ---
        let r = by_slot(3);
        assert_eq!(
            r.cell("x"),
            "768",
            "`x` must be the INTEGER half of the 16.16 pair. The record holds 0x0300BEEF across \
             $02..$06, so a decode reading all four bytes answers 50462959 — which is why the \
             sub-pixel half of this fixture is non-zero."
        );
        assert_eq!(
            r.cell("y"),
            "-3",
            "`y` must be SIGN-EXTENDED. The raw word is 0xFFFD; an unsigned read answers 65533, which \
             is a believable coordinate and is what a decode reduced to `be()` would report."
        );
        assert_eq!(
            r.cell("code"),
            "0x1000",
            "`code` is the field's own width in hex digits, not the raw number and not the record"
        );
        assert_eq!(
            r.cell("name"),
            "ObjRing",
            "`name` resolves at `ObjCodeBase + code_addr`. `code_addr` is an OFFSET, so a renderer \
             resolving the raw word would land on the decoy at ObjCodeBase+0 and name it `ObjDecoy` — \
             which is exactly as healthy-looking as the right answer."
        );
        assert_eq!(r.cell("nameDisp"), "0", "landed exactly on the label");

        // --- and the raw bytes are NOT what came back ---
        let raw = engine::debug_read(&sys, BASE + 3 * SST, SST as usize)
            .expect("the record is readable")
            .0;
        assert_eq!(raw.len(), SST as usize, "the whole record was read");
        let raw_hex = oracle_aether::hex::bytes(&raw);
        for key in ["x", "y", "code", "name"] {
            assert_ne!(
                r.cell(key),
                raw_hex,
                "`{key}` is the raw record handed straight back — the decode is the identity"
            );
        }
        // …and it is not a constant either: the same key differs across slots.
        assert_ne!(
            by_slot(3).cell("x"),
            by_slot(2).cell("x"),
            "every slot decodes to the same `x`, so the decode is a constant"
        );

        // --- an inactive slot omits rather than zeroes ---
        let dead = object_slot(Some(&table), &sys, 4).expect("slot 4 decodes");
        assert!(!dead.row.active, "slot 4 was zeroed and not poked");
        assert_eq!(dead.row.item["active"], json!(false));
        for key in ["x", "y", "code", "fields", "bytes"] {
            assert!(
                !dead.row.item.contains_key(key),
                "an empty slot's `{key}` must be OMITTED, never zeroed: those bytes are a record the \
                 game never wrote, and a pass-through decode would report `x: 0` as a position"
            );
        }
        assert_eq!(
            dead.row.cell("x"),
            "—",
            "and the renderer must say so rather than leaving the cell blank"
        );
    }

    /// The inactive-player case, stated on its own because it is a **different rule** from
    /// `object_list`'s: `player_state` returns the slot, with `active: false`, and omits the decoded
    /// keys. "Player 2 is not present" is the answer, not a shorter array.
    #[test]
    fn an_inactive_player_slot_is_reported_as_absent_not_as_zeroes() {
        let mut sys = booted();
        let table = table_with(&[]);
        let mut bus = populated(&mut sys, table.clone());
        // Clear slot 1's code word — the engine's own empty-slot sentinel.
        ok(bus.call(
            &mut sys,
            "emulator/write_memory",
            &json!({"addr": format!("0x{:06X}", BASE + SST), "bytes": "0x0000"}),
        ));

        let panel = player_state(Some(&table), &sys).expect("the player pool resolves");
        assert_eq!(panel.players.len(), 2, "both slots are still REPORTED");
        assert!(panel.players[0].active, "player 1 is still live");
        assert!(!panel.players[1].active);
        assert_eq!(panel.players[1].item["active"], json!(false));
        assert!(
            !panel.players[1].item.contains_key("x"),
            "an absent player's position is omitted, not zeroed"
        );
        // The label survives inactivity: it is the slot's, not the occupant's.
        assert_eq!(panel.players[1].cell("role"), "Player_2");
        // And the bus says the same thing, so this rule is the server's and not the panel's.
        let reply = ok(bus.call(&mut sys, "emulator/player_state", &json!({})));
        assert_eq!(served_items(&reply, "players")[1], panel.players[1].item);
    }

    // -----------------------------------------------------------------------------------------------
    // 3. ⚑ The no-symbols path — the one a user hits first
    // -----------------------------------------------------------------------------------------------

    /// **No listing, no Objects tab — said in those words, and never as an empty table.**
    ///
    /// The refusal is the server's own `-32012` and its own sentence, reached through the same
    /// `decoders::derive(None)` the three handlers reach. Asserted against the bus as well, so "the panel
    /// refuses" cannot pass while the tool answers.
    #[test]
    fn with_no_listing_loaded_the_tab_refuses_in_the_servers_own_words() {
        let mut sys = booted();
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), true, None);

        let view = Objects::of(None, &sys);
        let Objects::Refused(e) = &view else {
            panic!("with no listing the tab must refuse, not render a pool");
        };
        assert_eq!(
            e.code,
            oracle_aether::rpc::code::NO_SYMBOLS_LOADED,
            "the refusal must be the server's -32012, not a sentence this panel wrote"
        );
        assert!(
            e.message.contains("no symbol table is loaded"),
            "the server's own words, verbatim: {:?}",
            e.message
        );

        // The bus refuses identically — the panel is not refusing something the tool would answer.
        let served = match bus.call(&mut sys, "emulator/object_list", &json!({})) {
            Answer::Err(err) => err,
            Answer::Ok(v) => panic!("the bus answered a pool with no listing loaded: {v}"),
        };
        assert_eq!((e.code, &e.message), (served.code, &served.message));

        // …and what the tab actually shows names the fact, the remedy, and the distinction.
        let text = refusal_text(e);
        assert!(
            text.contains("no symbol table is loaded"),
            "the server's sentence must survive into the tab: {text:?}"
        );
        assert!(
            text.contains("--symbols") && text.contains(".lst"),
            "the tab must name the remedy, because -32012 names `emulator/load_symbols`, which is not \
             something a human at this window can call: {text:?}"
        );
        assert!(
            text.contains("empty"),
            "the tab must say that this is NOT an empty pool, which is the false claim an empty table \
             would make: {text:?}"
        );

        // The type itself forbids the empty table: there are no rows to render on this branch.
        assert!(
            object_list(None, &sys).is_err() && player_state(None, &sys).is_err(),
            "every entry point refuses, so no renderer can reach an empty vector by accident"
        );
    }

    /// A listing that locates the table but **does not partition it** refuses the player section only —
    /// the pool table above it is unaffected, and the section says why rather than rendering nothing.
    ///
    /// Two different refusals, and the panel must keep them apart: this one is about the *pools*, the one
    /// above is about the *base address*.
    #[test]
    fn a_listing_with_no_pool_partition_refuses_the_player_section_and_keeps_the_pool_table() {
        let mut sys = booted();
        // Every symbol `derive` requires, and none of the three pool boundary marks.
        let rows: Vec<(String, u32)> = pool_rows(BASE, SST)
            .into_iter()
            .filter(|(n, _)| {
                !matches!(
                    n.as_str(),
                    "Dynamic_Slots" | "System_Slots" | "Effect_Slots"
                )
            })
            .collect();
        let table = SymbolTable::parse(&listing(&rows)).expect("parsable");
        let mut bus = Bus::new(
            &mut sys,
            MachineInfo {
                rom_path: Some("testrom".into()),
                symbols: Some(table.clone()),
                symbols_path: Some("testrom.lst".into()),
            },
            true,
            None,
        );
        zero_pool(&mut bus, &mut sys);
        poke_object(&mut bus, &mut sys, 7, 0x0042, 0x0010_0000, 0x0020_0000);

        let Objects::Pool(pool) = Objects::of(Some(&table), &sys) else {
            panic!("the pool table still derives — only the partition is missing");
        };
        assert_eq!(
            pool.total, 1,
            "the pool table is unaffected and is populated"
        );
        let Err(e) = &pool.players else {
            panic!("the player section must refuse without a partition");
        };
        assert_eq!(e.code, oracle_aether::rpc::code::NO_SYMBOLS_LOADED);
        assert!(
            e.message.contains("pools"),
            "the section's refusal is about the partition, not about the base address: {:?}",
            e.message
        );
        // And it is the bus's refusal, not the panel's.
        let served = match bus.call(&mut sys, "emulator/player_state", &json!({})) {
            Answer::Err(err) => err,
            Answer::Ok(v) => {
                panic!("the bus partitioned a listing that cannot be partitioned: {v}")
            }
        };
        assert_eq!((e.code, &e.message), (served.code, &served.message));
    }

    /// **A derived layout over an empty pool is a third fact**, and is not the refusal.
    ///
    /// `total: 0` beside a real layout means *zero objects are live right now*, which is a true and
    /// useful answer; `-32012` means *nothing can be decoded at all*. Conflating them would put "load a
    /// listing" in front of a human who has one.
    #[test]
    fn an_empty_pool_is_zero_objects_and_not_a_refusal() {
        let mut sys = booted();
        let table = table_with(&[]);
        let mut bus = Bus::new(
            &mut sys,
            MachineInfo {
                rom_path: Some("testrom".into()),
                symbols: Some(table.clone()),
                symbols_path: Some("testrom.lst".into()),
            },
            true,
            None,
        );
        zero_pool(&mut bus, &mut sys);

        let Objects::Pool(pool) = Objects::of(Some(&table), &sys) else {
            panic!("a derivable layout over an empty pool is a pool, not a refusal");
        };
        assert_eq!(pool.total, 0);
        assert!(pool.objects.is_empty());
        assert_eq!(
            pool.slot_count, NUM_TOTAL,
            "the table's size is still known"
        );
        assert_eq!(pool.slot_bytes, SST);
        assert_eq!(pool.engine, "aeon-sst");
        // The players section still answers — two slots, both absent.
        let players = pool.players.expect("the partition resolves");
        assert_eq!(players.players.len(), NUM_PLAYERS as usize);
        assert!(players.players.iter().all(|p| !p.active));
        assert_eq!(players.layout.slot_count(), NUM_TOTAL);
        // And the bus agrees this is zero-objects rather than a refusal.
        assert_eq!(
            ok(bus.call(&mut sys, "emulator/object_list", &json!({})))["total"],
            json!(0)
        );
    }

    // -----------------------------------------------------------------------------------------------
    // 4. The layout is read from the listing, not written into this crate
    // -----------------------------------------------------------------------------------------------

    /// **Move the pool and the panel follows it.** The same ROM, two listings, two answers — which a
    /// panel holding a base address could not produce.
    ///
    /// This is the property the whole decoder family exists for, checked on the *panel's* side: `sst.emp`
    /// dates the current `$50` record to a 2026-08-05 fold from `$52`, so anything here that pinned an
    /// address was wrong three weeks ago.
    #[test]
    fn the_panel_reads_the_pool_out_of_the_listing_and_holds_no_address() {
        let mut sys = booted();
        let a = table_with(&[]);
        let mut bus = populated(&mut sys, a.clone());
        let first = object_list(Some(&a), &sys).expect("derives");
        assert_eq!(
            first.objects[0].addr, BASE,
            "slot 0 is where the listing says"
        );

        // A second listing naming a different base, over the identical machine.
        const MOVED: u32 = BASE + 0x1000;
        let rows = pool_rows(MOVED, SST);
        let b = SymbolTable::parse(&listing(&rows)).expect("parsable");
        // Populate the new location too, so the difference below is a real decode rather than an
        // absence: one live object at the moved slot 0.
        ok(bus.call(
            &mut sys,
            "emulator/write_memory",
            &json!({"addr": format!("0x{MOVED:06X}"), "bytes": "0x5A5A"}),
        ));
        let second = object_list(Some(&b), &sys).expect("derives");
        assert_eq!(
            second.objects[0].addr, MOVED,
            "the panel followed the listing"
        );
        assert_ne!(
            first.objects[0].addr, second.objects[0].addr,
            "the same ROM answered identically under two listings, which a symbol-derived layout \
             cannot do — this panel is holding an address"
        );
        assert_eq!(second.objects[0].cell("code"), "0x5A5A");
    }

    // -----------------------------------------------------------------------------------------------
    // 5. Rings — measured from the listing, and honest about the one number it cannot measure
    // -----------------------------------------------------------------------------------------------

    /// Where the fixture puts the ring buffer. Clear of the object table, which ends at
    /// `BASE + NUM_TOTAL * SST` = `$FF9440`.
    const RING_BASE: u32 = 0x00FF_A000;
    /// The fixture's own span. Not aeon's `$300` — a *different* number on purpose, so a derivation that
    /// had quietly memorised the real listing's span fails here.
    const RING_SPAN: u32 = 0x0180;

    /// `Ring_Buffer`, `Ring_Count`, and the symbol above `Ring_Count` that gives its width.
    fn ring_rows(base: u32, span: u32) -> Vec<(String, u32)> {
        vec![
            ("Ring_Buffer".into(), base),
            ("Ring_Count".into(), base + span),
            ("Ring_HighWater".into(), base + span + 1),
        ]
    }

    /// **The count is read at the MEASURED width, and the width is one byte here.**
    ///
    /// The load-bearing half is `Ring_HighWater = 0xFF`: a read that took two bytes would answer
    /// `0x07FF` = 2047, which is a believable ring count and is exactly what an assumed width produces.
    /// So this is a projection check, not a round-trip — the wrong implementation returns a *different*
    /// number rather than an obviously broken one.
    #[test]
    fn the_ring_count_is_read_at_the_measured_width_and_not_one_byte_wider() {
        let mut sys = booted();
        let table = table_with(&ring_rows(RING_BASE, RING_SPAN));
        let mut bus = populated(&mut sys, table.clone());
        ok(bus.call(
            &mut sys,
            "emulator/write_memory",
            &json!({
                "addr": format!("0x{:06X}", RING_BASE + RING_SPAN),
                // Ring_Count = 0x07, Ring_HighWater = 0xFF.
                "bytes": "0x07FF",
            }),
        ));

        let r = rings(Some(&table), &sys).expect("the ring buffer is measured");
        assert_eq!(r.count_width, 1, "the width is the gap to `Ring_HighWater`");
        assert_eq!(
            r.count, 7,
            "the count must be read at the measured width; a two-byte read answers 2047, which is a \
             believable ring count and is why the next byte is 0xFF in this fixture"
        );
        assert_eq!(r.span_bytes, RING_SPAN);
        assert_eq!(r.buffer_addr, RING_BASE);
        // …and the line a human reads carries the measured numbers, not a remembered pair.
        let line = r.summary();
        assert!(
            line.contains("7 live") && line.contains(&format!("${RING_SPAN:X} bytes")),
            "the rings line must carry the measured count and span: {line:?}"
        );
    }

    /// **The adjacency is re-checked against the loaded listing, and a symbol in the gap refuses.**
    ///
    /// `Ring_Count − Ring_Buffer` is the buffer's span only while nothing lives between them. This is
    /// the assertion that keeps a confident wrong number off the panel: with an intruder in the gap the
    /// subtraction measures an unrelated region, and the honest answer is to refuse it.
    ///
    /// ⚑ The **positive control comes first**: the identical fixture without the intruder must succeed.
    /// Without that, a `rings()` broken in any other way would refuse here and be read as a pass.
    #[test]
    fn a_symbol_between_the_buffer_and_the_count_refuses_instead_of_measuring_the_gap() {
        let sys = booted();

        // --- the control: this fixture derives ---
        let clean = table_with(&ring_rows(RING_BASE, RING_SPAN));
        let ok_view = rings(Some(&clean), &sys)
            .expect("the control fixture must derive, or the refusal below proves nothing");
        assert_eq!(ok_view.span_bytes, RING_SPAN);

        // --- the same listing, plus one symbol inside the buffer ---
        let mut rows = ring_rows(RING_BASE, RING_SPAN);
        rows.push(("Some_Other_Thing".into(), RING_BASE + 0x10));
        let intruded = table_with(&rows);
        let Err(e) = rings(Some(&intruded), &sys) else {
            panic!(
                "a symbol at {} lies between Ring_Buffer and Ring_Count, so the span between them is \
                 NOT the ring buffer and must not be reported as its size",
                hex::addr(RING_BASE + 0x10)
            );
        };
        assert!(
            e.message.contains("not the next symbol after"),
            "the refusal must name the adjacency it failed, so a reader knows the span was not \
             mis-measured but declined: {:?}",
            e.message
        );
    }

    /// **A listing that never names the ring buffer loses the ring line, not the tab** — and the line
    /// says so rather than reporting `0`.
    #[test]
    fn a_listing_with_no_ring_symbols_refuses_the_ring_line_and_keeps_the_tab() {
        let mut sys = booted();
        let table = table_with(&[]); // no ring rows at all
        let _bus = populated(&mut sys, table.clone());

        let Objects::Pool(pool) = Objects::of(Some(&table), &sys) else {
            panic!("the tab still renders — only the ring line is unavailable");
        };
        assert_eq!(pool.total, LIVE.len(), "the pool table is unaffected");
        let Err(e) = &pool.rings else {
            panic!("no Ring_Buffer/Ring_Count in this listing, so there is nothing to measure");
        };
        assert!(
            e.message.contains("Ring_Buffer") && e.message.contains("Ring_Count"),
            "the refusal names the symbols that did not answer: {:?}",
            e.message
        );
    }

    /// **The ceiling is unknown for a stated reason, and this pins the REASON rather than the sentence.**
    ///
    /// Run against `fixtures/aeon/s4.debug.lst` — committed bytes of a real build — because every claim
    /// here is about what that listing does and does not publish:
    ///
    /// 1. `Ring_Buffer` and `Ring_Count` both resolve, and **nothing lies between them**, so the span is
    ///    real. This is the adjacency the whole rings line rests on, checked on the real file.
    /// 2. The listing **does** contain a `RING_BUFFER_ENTRY_SIZE` row — so clause 3 is about ingestion
    ///    and not about absence. Without this clause the test would pass just as well on a listing that
    ///    never mentioned the constant, and would be pinning nothing.
    /// 3. `SymbolTable` nonetheless cannot answer for it, because the `Equate Table`'s values are
    ///    deliberately not ingested (`F-EQUATES-NAMESPACE`). **That** is why no ceiling is shown.
    ///
    /// The day equates become readable, clause 3 goes red and asks whoever ruled it to finish the
    /// division — which is the point of pinning the reason instead of the wording.
    #[test]
    fn the_rings_ceiling_is_unknown_because_equate_values_are_not_ingested() {
        let path = std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/aeon/s4.debug.lst"
        ));
        // Loud, never skipped: these bytes are committed to this repo, so "not there" is a broken
        // checkout and reporting it as a pass would hide the only evidence this test carries.
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the frozen listing must be present at {path:?}: {e}"));
        let table = SymbolTable::parse(&text).expect("the frozen listing parses");

        // 1. Both symbols resolve, and nothing is between them.
        let buffer = table
            .address_of("Ring_Buffer")
            .expect("the real listing names Ring_Buffer");
        let count = table
            .address_of("Ring_Count")
            .expect("the real listing names Ring_Count");
        assert!(count > buffer, "Ring_Count lies above Ring_Buffer");
        let between: Vec<&str> = table
            .symbols()
            .iter()
            .filter(|s| s.addr > buffer && s.addr < count)
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            between.is_empty(),
            "a symbol between Ring_Buffer and Ring_Count would make `Ring_Count − Ring_Buffer` measure \
             an unrelated region, and the span on the panel would be a confident wrong number: {between:?}"
        );
        // The scan is not vacuous: the same filter over a range that must be populated finds symbols.
        assert!(
            table
                .symbols()
                .iter()
                .any(|s| s.addr > buffer && s.addr < count + 0x100),
            "the between-symbols filter matches nothing at all, so its emptiness above proves nothing"
        );

        // 2. The listing DOES publish the entry size — so clause 3 is about ingestion, not absence.
        assert!(
            text.contains("RING_BUFFER_ENTRY_SIZE"),
            "this test's whole claim is that the constant is present but unreadable; if the listing \
             stopped publishing it the claim would need restating rather than silently holding"
        );

        // 3. …and the symbol table still cannot answer for it.
        assert!(
            table.address_of("RING_BUFFER_ENTRY_SIZE").is_none(),
            "equate VALUES are deliberately not ingested (F-EQUATES-NAMESPACE). If this now resolves, \
             the ring ceiling is derivable as (Ring_Count − Ring_Buffer) / RING_BUFFER_ENTRY_SIZE and \
             `CEILING_UNKNOWN` must be replaced with the division rather than left standing."
        );
        assert!(
            CEILING_UNKNOWN.contains("RING_BUFFER_ENTRY_SIZE"),
            "the sentence on the panel must name the constant it is missing"
        );
    }

    // -----------------------------------------------------------------------------------------------
    // 6. The header — readable facts, and no padded table
    // -----------------------------------------------------------------------------------------------

    /// **The header carries the `layout` object's facts and none of its JSON.**
    ///
    /// Both halves are asserted. Absence of `{"` alone would pass on a header that had been emptied, so
    /// every fact the JSON used to carry is checked to still be on the lines.
    #[test]
    fn the_header_carries_the_layouts_facts_and_not_its_json() {
        let mut sys = booted();
        let table = table_with(&ring_rows(RING_BASE, RING_SPAN));
        let _bus = populated(&mut sys, table.clone());
        let Objects::Pool(pool) = Objects::of(Some(&table), &sys) else {
            panic!("the fixture derives");
        };
        let lines = pool.layout_lines();
        let all = lines.join("\n");

        // --- the facts are all still here ---
        for want in [
            "aeon-sst",             // engine
            &hex::addr(BASE),       // baseAddr
            &NUM_TOTAL.to_string(), // slotCount
            &format!("${SST:X}"),   // slotBytes
            "Object_RAM",           // detectedFrom
            "player 0..2",          // pools[]
        ] {
            assert!(
                all.contains(want),
                "the header dropped `{want}` — the readable spelling must carry every fact the JSON \
                 did, not a subset:\n{all}"
            );
        }

        // --- and none of the JSON spelling survived ---
        for bad in ["{\"", "\":", "detectedBy"] {
            assert!(
                !all.contains(bad),
                "`{bad}` is JSON punctuation in the header, which is the defect being fixed:\n{all}"
            );
        }
    }

    /// **The pool table shows the live slots and nothing else — it does not pad to `slotCount`.**
    ///
    /// The queue row this parcel came from asserted that the table "pads with empty rows". It does not,
    /// and never did: `object_list` skips inactive records before pushing, so the vector the renderer
    /// iterates contains only live rows. Pinned here so the property is measured rather than re-argued —
    /// and because a future renderer looping `0..slot_count` to line the table up is a plausible change.
    #[test]
    fn the_pool_table_holds_only_live_slots_and_is_not_padded_to_the_slot_count() {
        let mut sys = booted();
        let table = table_with(&[]);
        let _bus = populated(&mut sys, table.clone());
        let Objects::Pool(pool) = Objects::of(Some(&table), &sys) else {
            panic!("the fixture derives");
        };

        assert_eq!(pool.objects.len(), LIVE.len(), "one row per live object");
        assert_eq!(pool.total, pool.objects.len());
        assert!(
            pool.objects.iter().all(|r| r.active),
            "every row in the pool table is a live slot"
        );
        // The anti-vacuity clause: the pool is much larger than the live set, so "no padding" is a real
        // distinction here and not an accident of a table that happens to be full.
        assert!(
            pool.slot_count > pool.objects.len() as u32 * 2,
            "this fixture must leave most of the table empty ({} live of {} slots) or padding and \
             not-padding would look identical",
            pool.objects.len(),
            pool.slot_count
        );
        // Slot numbers are sparse, which is the visible consequence: slot 50 is present with no rows
        // for 4..50 between it and slot 3.
        let slots: Vec<u32> = pool.objects.iter().map(|r| r.slot).collect();
        assert_eq!(
            slots,
            vec![0, 1, 2, 3, 50],
            "sparse, exactly as the reply's are"
        );
    }

    /// `field_names` publishes the catalogue the handler validates against — every name it hands out
    /// resolves, and the expansion shows every one of them.
    ///
    /// The round-trip matters: a name published but not resolvable would be a column that silently never
    /// appears, and a name resolvable but not published would be a field the expansion never offers.
    #[test]
    fn every_published_field_name_resolves_and_reaches_the_expansion() {
        let mut sys = booted();
        let table = table_with(&[]);
        let _bus = populated(&mut sys, table.clone());
        let view = object_slot(Some(&table), &sys, 0).expect("slot 0 decodes");
        let names = view.layout.field_names();
        assert!(!names.is_empty(), "the catalogue is not empty");
        let owned: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(
            view.layout
                .resolve_fields(&owned)
                .expect("every published name resolves")
                .len(),
            names.len()
        );
        let fields = view.row.item["fields"].as_object().expect("the field map");
        for n in &names {
            assert!(fields.contains_key(*n), "`{n}` is published but not shown");
        }
        assert_eq!(fields.len(), names.len(), "and nothing extra is shown");
        // The overlay window is NOT in the catalogue and must not become a column: §11.25 rule (3)
        // forbids reporting a key that is addressable but not live for the slot's occupant.
        assert!(
            !names.contains(&"sst_custom"),
            "the overlay window must never be offered as a field"
        );
    }
}
