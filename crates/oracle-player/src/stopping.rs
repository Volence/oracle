//! **The three stopping panels' model** — Breakpoints, Watchpoints, Profiler.
//!
//! One module for three tabs, for the reason [`crate::memory`] is one module for five spaces: they are
//! three views of one question — *what will stop this machine, and what has it already seen?* — and they
//! share a vocabulary that is worth having exactly one copy of.
//!
//! # ⚑ The trap this module exists to close: armed is not derivable from the rows
//!
//! [`Bus::read_instruments`](crate::bus::Bus::read_instruments)' own doc names it:
//!
//! > *the armed flag is not derivable from the accumulator — disarming RETAINS the sample, so rows exist
//! > whether or not anything is still recording, and a panel showing only the rows could not tell the two
//! > apart.*
//!
//! It is true of all three, in three different spellings, and every one of them is a way for a panel to
//! give a **believable wrong answer** rather than a missing one:
//!
//! * **Profiler** — `emulator/set_profiler {enabled:false}` disarms and keeps the sample (§11.16: arming
//!   resets, disarming retains, reading never clears). A grid of hot routines from four minutes ago looks
//!   exactly like a grid of hot routines from now.
//! * **Watchpoints** — `emulator/watchpoint_clear` retires the watch and **deliberately keeps its hits**
//!   (the handler says so: *"a destructive clear would let one client erase another's evidence"*). So a
//!   hit log can outlive every watch that could produce another entry.
//! * **Breakpoints** — `emulator/breakpoint_set_enabled {enabled:false}` carries `hits` across the toggle
//!   (§6: *"a client wanting a fresh count clears and re-adds"*). A disabled row with 12,000 hits is a
//!   breakpoint that fired 12,000 times and will not fire again.
//!
//! [`Live`] is the one answer to all three, and each panel puts it **on screen in words** before it draws
//! a single row. A stale table rendered as a live one is precisely the silent-wrong-answer class this repo
//! keeps paying for, and it is the one thing here that no amount of correct row rendering would fix.
//!
//! # The read/write line (design §4.4), applied
//!
//! Every **read** below takes a shared borrow of the instrument the loop itself feeds — `&Breakpoints`,
//! `&Watchpoints`, `&Profiler` — and never a `Host::call`. That is R2: one breakpoint list, one watch, one
//! profiler, read by the panel and by `emulator/breakpoint_list` alike, with nothing to drift apart from.
//! The borrows are shared, which states the rest in the type: *a panel cannot move a number a `Host::call`
//! is gating on.*
//!
//! Every **gesture** is a `(method, params)` pair built here and dispatched through
//! [`Bus::call`](crate::bus::Bus::call) by the panel, whose whole rendering job is then
//! [`crate::memory::answer_line`]. Nothing in this module or in `ui.rs` composes a refusal: the arming
//! rules, the caps, the handle grammar and the param types are the handlers' and the panels inherit every
//! one of them, including the ones nobody here anticipated.
//!
//! **The params builders are separate functions rather than inline `json!`s at the click sites** so the
//! tests below can check them against the handlers' own parsers without a machine — which is how the two
//! traps in `watchpoint_add` (`write: true` rather than `op: "write"`, and a `len` that must be a JSON
//! *number*) are held closed by something other than memory.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use oracle_aether::breakpoints::Breakpoints;
use oracle_aether::engine::{breakpoint_wire_id, symbol_at, watch_wire_id};
use oracle_aether::hex;
use oracle_core::profiler::{Counts, Profiler};
use oracle_core::symbols::SymbolTable;
use oracle_core::watchpoints::{WatchHit, WatchReport, Watchpoints};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------------------------------
// ⚑ Live — the one distinction all three panels owe a reader
// ---------------------------------------------------------------------------------------------------

/// **Whether what is on screen is being produced right now, or is left over from when it was.**
///
/// Three states rather than a `bool`, because "nothing armed and nothing recorded" and "nothing armed but
/// here is what was" are different facts and collapsing them is how an empty grid comes to mean *no hot
/// code* when it means *never measured*. That is the Objects tab's rule one instrument over: a tab that
/// cannot work says so in words rather than rendering an empty table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Live {
    /// Armed. The rows below are moving, and this instrument can act on the machine.
    Yes,
    /// **Nothing is armed, and what is shown was recorded before it stopped.** The rows are frozen: they
    /// will not grow, and nothing here will stop the machine.
    Retained,
    /// Nothing is armed and nothing was ever recorded. There is no table to draw.
    Never,
}

impl Live {
    /// The sentence a panel puts at the top of itself, given the noun for its own instrument.
    ///
    /// Written here rather than at three call sites so the three tabs cannot come to word this differently
    /// — the distinction is subtle enough that a reader who learns it in one tab should meet the same
    /// words in the next.
    pub fn sentence(self, armed_noun: &str, retained_noun: &str) -> String {
        match self {
            Live::Yes => format!("RECORDING — {armed_noun}"),
            Live::Retained => format!(
                "STOPPED — nothing is armed. {retained_noun} These figures are what was recorded before \
                 it stopped; they will not move."
            ),
            Live::Never => format!("NEVER ARMED — {retained_noun}"),
        }
    }

    /// Whether a panel should draw its table at all. [`Live::Never`] has nothing to draw and an empty grid
    /// in its place would assert a measurement that was never taken.
    pub fn has_rows(self) -> bool {
        !matches!(self, Live::Never)
    }
}

// ---------------------------------------------------------------------------------------------------
// Breakpoints
// ---------------------------------------------------------------------------------------------------

/// One breakpoint, as the panel draws it. The fields are `breakpoint_list`'s own row, read from the set
/// rather than from a JSON page of it.
pub struct BreakRow {
    /// The opaque handle, spelled by [`breakpoint_wire_id`] — **the server's own function**, so the string
    /// a row shows is byte-for-byte the string its ✕ sends back.
    pub handle: String,
    /// `hex::addr`'s spelling, which is the one on the wire — **the only spelling of this address the row
    /// carries.** A raw `u32` beside it would invite a second `{:08X}` at some call site, and two
    /// spellings of one address is how a reader ends up comparing a panel against a tool and seeing a
    /// difference that is not one.
    pub addr_text: String,
    /// `name` and displacement, resolved through the same `symbol_at` the handler uses.
    pub symbol: Option<(String, u32)>,
    pub enabled: bool,
    /// Firings **while enabled**. Carried across a disable and never reset by this surface, which is why
    /// [`BreakView::live`] has to be reported separately from this number.
    pub hits: u64,
    /// The caller's label, verbatim and never interpreted. Empty means none was given.
    pub label: String,
}

impl BreakRow {
    /// The row as one monospace line.
    ///
    /// **The arm state is a word, not a checkbox alone**, and it sits beside `hits` on purpose: those two
    /// columns together are the whole armed-versus-retained distinction at row scale, and `disabled` next
    /// to a five-figure count is the pairing a reader has to be able to see.
    pub fn summary(&self) -> String {
        let sym = match &self.symbol {
            Some((n, 0)) => format!("  {n}"),
            Some((n, d)) => format!("  {n}+0x{d:X}"),
            None => String::new(),
        };
        let label = if self.label.is_empty() {
            String::new()
        } else {
            format!("  ({})", self.label)
        };
        format!(
            "{:<5} {:<10} {:<8} {:>9} hits{sym}{label}",
            self.handle,
            self.addr_text,
            if self.enabled { "ARMED" } else { "disabled" },
            self.hits,
        )
    }
}

/// Everything the Breakpoints tab draws for one repaint.
pub struct BreakView {
    pub rows: Vec<BreakRow>,
    /// How many rows are `enabled` — the count that decides whether anything here can halt the machine.
    pub armed: usize,
    /// Hits held on rows that are **not** armed. Non-zero is the exact case this panel must not render as
    /// though it were live.
    pub retained_hits: u64,
    pub live: Live,
}

/// Read the armed set. **A shared borrow of the `Host`'s own list** — there is no second copy (R2).
pub fn breakpoints(set: &Breakpoints, symbols: Option<&SymbolTable>) -> BreakView {
    let rows: Vec<BreakRow> = set
        .iter()
        .map(|b| BreakRow {
            handle: breakpoint_wire_id(b.id),
            addr_text: hex::addr(b.addr),
            symbol: symbols.and_then(|t| symbol_at(t, b.addr)),
            enabled: b.enabled,
            hits: b.hits,
            label: b.label.clone(),
        })
        .collect();
    let armed = rows.iter().filter(|r| r.enabled).count();
    let retained_hits = rows.iter().filter(|r| !r.enabled).map(|r| r.hits).sum();
    BreakView {
        live: breakpoints_live(set),
        armed,
        retained_hits,
        rows,
    }
}

/// ⚑ **Armed, retained, or never** — for breakpoints, where "retained" means *rows exist and none of them
/// is enabled*, so the list is real and nothing in it will stop the machine.
///
/// `Breakpoints::any_enabled` is the same predicate `Engine::run_sinks` gates the halt sink on, so this
/// panel's headline and the engine's decision to attach a `BreakStop` are one fact rather than two.
pub fn breakpoints_live(set: &Breakpoints) -> Live {
    if set.any_enabled() {
        Live::Yes
    } else if set.is_empty() {
        Live::Never
    } else {
        Live::Retained
    }
}

// ---------------------------------------------------------------------------------------------------
// Watchpoints
// ---------------------------------------------------------------------------------------------------

/// One armed watch, as the panel draws it.
pub struct WatchRow {
    /// The opaque handle, spelled by [`watch_wire_id`] — **the server's own function**, so the string a
    /// row shows is byte-for-byte the string its ✕ sends back. A `format!("w{}", …)` written in the panel
    /// would be a second spelling of one fact, agreeing right up until it did not, in the one place where
    /// being wrong retires somebody else's watch.
    pub handle: String,
    /// `Watchpoints::watch()`'s own read-time snapshot: the configuration and everything it aggregated.
    pub report: WatchReport,
}

/// Everything the Watchpoints tab draws for one repaint: the armed watches, the hit log, and the three
/// aggregate counters that make a *negative* finding readable.
pub struct WatchView {
    /// The armed watches, each paired with the handle its ✕ sends back.
    pub watches: Vec<WatchRow>,
    /// The retained hit log, newest last.
    pub hits: Vec<WatchHit>,
    /// Accesses the instrument saw at all. **`seen > 0, matched == 0` is a genuine negative finding**, and
    /// it is only distinguishable from a silently-dropped watch because both numbers are shown.
    pub seen: u64,
    pub matched: u64,
    /// Hits the ring could not keep. Non-zero means the log below has gaps, and a `seq` gap marks them.
    pub dropped: u64,
    /// The instrument's own caveats about what its numbers mean on this run — its words, not ours.
    pub caveats: Vec<String>,
    pub live: Live,
}

/// Read the watch instrument. Shared borrow of the engine's own — the same one `emulator/watchpoint_list`
/// and `emulator/watchpoint_hits` answer from.
pub fn watches(w: &Watchpoints) -> WatchView {
    WatchView {
        live: watches_live(w),
        watches: w
            .watches()
            .into_iter()
            .map(|report| WatchRow {
                handle: watch_wire_id(report.id),
                report,
            })
            .collect(),
        hits: w.hits().to_vec(),
        seen: w.seen(),
        matched: w.matched(),
        dropped: w.dropped(),
        caveats: w.caveats(),
    }
}

/// ⚑ **Armed, retained, or never** — for watches.
///
/// The retained case here is not hypothetical and not rare: `emulator/watchpoint_clear` retires a watch
/// and **keeps its hits on purpose**, so "no watches, plenty of hits" is the ordinary state one gesture
/// after a capture. A panel that drew the log without saying so would be showing a live trace of a machine
/// nothing is watching.
pub fn watches_live(w: &Watchpoints) -> Live {
    if w.watch_count() > 0 {
        Live::Yes
    } else if w.seen() > 0 || !w.hits().is_empty() {
        Live::Retained
    } else {
        Live::Never
    }
}

// ---------------------------------------------------------------------------------------------------
// Profiler
// ---------------------------------------------------------------------------------------------------

/// One routine's row, undivided. See [`ProfilerView::frames`] for why the panel shows the undivided sample
/// and the frame count rather than the divided view.
///
/// **One spelling of the address, as [`BreakRow::addr_text`] states the rule.** The raw `u32` used to sit
/// here too, and the only thing that ever read it was the sort — which now runs on the bare `(addr,
/// counts)` pairs *before* a row exists ([`top_routines`]), so the field became a second spelling with no
/// reader. Two spellings of one address is how a reader comes to compare a panel against a tool and see a
/// difference that is not one.
pub struct RoutineRow {
    pub addr_text: String,
    pub symbol: Option<(String, u32)>,
    pub counts: Counts,
}

/// Everything the Profiler tab draws for one repaint.
///
/// **The undivided sample, plus the divisor, rather than `Profiler::report()`.** `report()` allocates two
/// fresh `BTreeMap`s and divides every figure; this body repaints at 60 Hz and
/// [`Profiler::sample_routines`] hands back the accumulator by reference. The panel shows
/// `framesRecorded` beside the totals, which is exactly what `get_profiler_frames` divides by, so a reader
/// can do the division the server would have done and a client asking for the divided view still gets it —
/// **from the server, where §11.16 puts it**. Nothing here re-implements the division; it declines to
/// perform it.
pub struct ProfilerView {
    /// `enabled`, from the flag beside the instrument. **Not derivable from anything below it.**
    pub armed: bool,
    /// Whole frames in the sample — `get_profiler`'s `framesRecorded`, and `get_profiler_frames`' divisor.
    pub frames: u64,
    /// Rows the accumulator holds, whether or not anything is still recording.
    pub routine_count: usize,
    /// Frames open on the shadow stack right now.
    pub open_frames: usize,
    pub per_frame_armed: bool,
    pub callers_armed: bool,
    /// The hottest routines by inclusive cycles, longest first, capped at [`TOP_ROUTINES`].
    pub top: Vec<RoutineRow>,
    pub live: Live,
}

/// How many routine rows the panel draws. A cap rather than a scroll over the whole map, because the
/// accumulator can hold thousands and a panel body is on the 60 Hz path; the count it is a subset of is
/// shown beside it ([`ProfilerView::routine_count`]) so a clipped list is never mistaken for a complete
/// one — the mistake `emulator/get_profiler_frames`' own `top` refuses to make by clamping.
pub const TOP_ROUTINES: usize = 24;

/// The panel's row order: descending by inclusive cycles, ties broken by **ascending address** so the
/// order is stable boot to boot — a table whose rows swap places between repaints is unreadable at 60 Hz.
///
/// It is a **total** order on the sample, because the sample is a `BTreeMap` keyed by address and so no
/// two rows can share one. That is what lets [`profiler`] select the top rows without sorting the whole
/// map: a partial selection under a total order picks the same set, in the same order, as a full sort.
fn hotter(a: (u32, Counts), b: (u32, Counts)) -> Ordering {
    b.1.cycles.cmp(&a.1.cycles).then(a.0.cmp(&b.0))
}

/// The [`TOP_ROUTINES`] hottest rows of a sample, in [`hotter`] order.
///
/// ⚑ **Select first, format second — the panel pays for [`TOP_ROUTINES`], not for the sample.** The
/// accumulator holds a row per routine the ROM has entered — thousands, for a ROM that has been running —
/// and this runs on the 60 Hz repaint path. Building a [`RoutineRow`] costs a `hex::addr` allocation and a
/// `symbol_at` lookup apiece, so ranking the bare `(addr, counts)` pairs and *then* formatting the 24 that
/// survive is the difference between paying that per routine **in the map** and paying it per **drawn**
/// row. `select_nth_unstable_by` does the ranking in one linear pass rather than an `n log n` sort.
///
/// **The result is identical to mapping every routine, sorting the lot, and truncating** — which is what
/// this did before, and which `the_top_selection_matches_a_full_sort_even_with_ties` re-derives and
/// compares against on a fixture built to be all ties. That equality is not an accident of the data: it
/// holds because [`hotter`] is a *total* order here, so there is exactly one correct answer for a partial
/// selection to find.
fn top_routines(sample: &BTreeMap<u32, Counts>, symbols: Option<&SymbolTable>) -> Vec<RoutineRow> {
    let mut ranked: Vec<(u32, Counts)> = sample
        .iter()
        .map(|(&addr, &counts)| (addr, counts))
        .collect();
    if ranked.len() > TOP_ROUTINES {
        ranked.select_nth_unstable_by(TOP_ROUTINES - 1, |a, b| hotter(*a, *b));
        ranked.truncate(TOP_ROUTINES);
    }
    ranked.sort_unstable_by(|a, b| hotter(*a, *b));
    ranked
        .into_iter()
        .map(|(addr, counts)| RoutineRow {
            addr_text: hex::addr(addr),
            symbol: symbols.and_then(|t| symbol_at(t, addr)),
            counts,
        })
        .collect()
}

/// Read the profiler. The `armed` flag comes from
/// [`Bus::read_instruments`](crate::bus::Bus::read_instruments)' third element and **nowhere else**.
pub fn profiler(p: &Profiler, armed: bool, symbols: Option<&SymbolTable>) -> ProfilerView {
    let top = top_routines(p.sample_routines(), symbols);
    ProfilerView {
        armed,
        frames: p.frames(),
        routine_count: p.routine_count(),
        open_frames: p.open_frames(),
        per_frame_armed: p.per_frame_armed(),
        callers_armed: p.callers_armed(),
        top,
        live: profiler_live(p, armed),
    }
}

/// ⚑ **Armed, retained, or never** — the case this whole module was written around.
///
/// `armed` is the instrument's own flag and is the **only** input that can produce [`Live::Yes`]: §11.16
/// makes disarming retain the sample, so a full accumulator is evidence about the past and evidence about
/// nothing else. The two other arms are separated on the accumulator, which is what lets the panel refuse
/// to draw a grid it has no measurement for.
pub fn profiler_live(p: &Profiler, armed: bool) -> Live {
    if armed {
        Live::Yes
    } else if p.frames() > 0 || p.routine_count() > 0 {
        Live::Retained
    } else {
        Live::Never
    }
}

// ---------------------------------------------------------------------------------------------------
// The gestures — every one of them a (method, params) pair the handler judges
// ---------------------------------------------------------------------------------------------------

pub const BREAKPOINT_ADD: &str = "emulator/breakpoint_add";
pub const BREAKPOINT_SET_ENABLED: &str = "emulator/breakpoint_set_enabled";
pub const BREAKPOINT_CLEAR: &str = "emulator/breakpoint_clear";
pub const WATCHPOINT_ADD: &str = "emulator/watchpoint_add";
pub const WATCHPOINT_CLEAR: &str = "emulator/watchpoint_clear";
pub const SET_PROFILER: &str = "emulator/set_profiler";

/// The four spaces `emulator/watchpoint_add` accepts, in the handler's own spelling.
///
/// Read from nowhere but this list, and checked against `parse_watch_space`'s accepted set by the test
/// below rather than trusted: a fifth spelling here would produce a selector entry whose only possible
/// outcome is `-32602`.
pub const WATCH_SPACES: [&str; 4] = ["bus", "vram", "cram", "vsram"];

/// **An address box's text, as the `addr`-or-`symbol` pair the handlers take.**
///
/// Hex goes as `addr`; anything else goes as `symbol` and **the server resolves it**. That is deliberate
/// and it is not laziness: `breakpoint_add` answers with the *resolved* address and its symbol, and a
/// name it cannot find is refused in its own words. Resolving here first would mean this panel writing a
/// second "no symbol named …" sentence and keeping it in step with the server's.
///
/// The one refusal this function makes is about input the server would never see.
pub fn target_params(text: &str) -> Result<Value, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("type an address or a symbol name".into());
    }
    let bare = t.strip_prefix("0x").or_else(|| t.strip_prefix('$'));
    // A `0x`/`$` prefix is an unambiguous request for a literal; a bare hex-looking word is one too, on
    // `memory::resolve_address`'s precedent, so the two boxes in this player behave the same way.
    let looks_hex = bare.unwrap_or(t);
    if !looks_hex.is_empty() && looks_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(json!({ "addr": t }));
    }
    Ok(json!({ "symbol": t }))
}

/// `emulator/breakpoint_add` — arm at an address or a symbol, with an optional label.
///
/// `enabled` is left off rather than sent as `true`: the handler's default *is* `true` and it echoes back
/// the arm state it actually gave, so omitting it means the panel is told what it got instead of asserting
/// what it asked for.
pub fn breakpoint_add_params(target: &str, label: &str) -> Result<Value, String> {
    let mut v = target_params(target)?;
    let label = label.trim();
    if !label.is_empty() {
        v["label"] = json!(label);
    }
    Ok(v)
}

/// `emulator/breakpoint_set_enabled` — the one writer of `enabled` on this surface.
///
/// Both keys are always sent. `enabled` is **required** by the handler, deliberately: *"a toggle whose
/// argument may be omitted is a toggle whose caller cannot tell which way it went."*
pub fn breakpoint_enable_params(handle: &str, enabled: bool) -> Value {
    json!({ "breakpoint": handle, "enabled": enabled })
}

/// `emulator/breakpoint_clear` — one handle.
pub fn breakpoint_clear_params(handle: &str) -> Value {
    json!({ "breakpoint": handle })
}

/// `emulator/breakpoint_clear {all:true}` — **every breakpoint on the server, other clients' included.**
///
/// `all` and `breakpoint` are mutually exclusive at the handler, which is why this is its own function
/// rather than an `Option` on the one above: a call carrying both is `-32602`, and the two gestures are
/// different enough that a human should be clicking different buttons for them.
pub fn breakpoint_clear_all_params() -> Value {
    json!({ "all": true })
}

/// **`emulator/watchpoint_add` — the row with the two traps in it.**
///
/// 1. **`write: true`, not `op: "write"`.** The handler resolves the op from the `read`/`write` boolean
///    pair (`parse_watch_op`); there is no `op` *param* at all — `op` is a key of the *reply*, saying what
///    the pair became. A panel sending `{"op":"write"}` would be refused by the params closure.
/// 2. **`len` must be a JSON number.** `hex::parse_count` takes `as_u64()` and refuses a string in as many
///    words: *"must be a non-negative JSON number (D9), not a string"*. So the panel's `len` box is parsed
///    here and sent as a number — a `"0x10"` typed into it is the panel's own refusal, not a `-32602` for
///    a shape the panel chose.
///
/// Both `read` and `write` false is refused **by the handler** and not here: it is a request for a watch
/// that can never match, and the handler's sentence about it is better than one this file would write.
/// They are sent explicitly rather than omitted so the checkboxes mean what they show.
pub fn watch_add_params(
    target: &str,
    len_text: &str,
    space: &str,
    read: bool,
    write: bool,
    stop_after_text: &str,
    label: &str,
) -> Result<Value, String> {
    let mut v = target_params(target)?;
    let len = len_text.trim();
    // Empty is "the handler's default", which is 1, and saying nothing is how a panel asks for a default
    // without knowing what it is.
    if !len.is_empty() {
        let n: u64 = len
            .parse()
            .map_err(|e| format!("len {len:?}: {e} — `len` is a decimal byte count, not hex"))?;
        v["len"] = json!(n);
    }
    v["space"] = json!(space);
    v["read"] = json!(read);
    v["write"] = json!(write);
    let stop = stop_after_text.trim();
    if !stop.is_empty() {
        let n: u64 = stop
            .parse()
            .map_err(|e| format!("stopAfter {stop:?}: {e} — a decimal count of matches"))?;
        v["stopAfter"] = json!(n);
    }
    let label = label.trim();
    if !label.is_empty() {
        v["label"] = json!(label);
    }
    Ok(v)
}

/// `emulator/watchpoint_clear` — one handle. Hits are **not** removed with the watch, by the handler's own
/// design, which is the case [`watches_live`] exists to make legible.
pub fn watch_clear_params(handle: &str) -> Value {
    json!({ "watch": handle })
}

/// `emulator/watchpoint_clear {all:true}`.
pub fn watch_clear_all_params() -> Value {
    json!({ "all": true })
}

/// **`emulator/set_profiler` — and arming RESETS the sample.**
///
/// §11.18: every arming flag resets together, so a second arm starts a fresh sample under exactly the
/// lenses *this* call names. The panel's arm button says so in its own hover text, because a human who
/// ticks `callers` on a running measurement to "add" the lens loses the sample they were watching.
///
/// `perFrame` and `callers` are sent on the disarm too, harmlessly — the handler only reads them under
/// `enabled` — so that one function builds both gestures and there is no second spelling to keep in step.
pub fn set_profiler_params(enabled: bool, per_frame: bool, callers: bool) -> Value {
    json!({ "enabled": enabled, "perFrame": per_frame, "callers": callers })
}

// ---------------------------------------------------------------------------------------------------
// The three panels' own state
// ---------------------------------------------------------------------------------------------------

/// Everything the three stopping tabs remember between repaints: **what is typed in their boxes, and the
/// last answer each of them got.** Nothing else.
///
/// ⚑ **No breakpoint list, no watch list, no profiler sample, and no armed flag.** Those are the `Host`'s,
/// re-read every repaint (R2). A copy here would be the second belief this rule exists to prevent, and it
/// would be the one a human is looking at.
///
/// The three `note` fields hold [`crate::memory::Line`]s, which carry `refused` beside the text so the
/// panel colours on the [`Answer`](crate::bus::Answer) rather than on the shape of the string.
pub struct Panel {
    pub bp_target: String,
    pub bp_label: String,
    pub bp_note: Option<crate::memory::Line>,

    pub w_target: String,
    /// A **decimal** byte count. See [`watch_add_params`] for why the panel parses it rather than passing
    /// the text through.
    pub w_len: String,
    /// Index into [`WATCH_SPACES`].
    pub w_space: usize,
    pub w_read: bool,
    pub w_write: bool,
    pub w_stop_after: String,
    pub w_label: String,
    pub w_note: Option<crate::memory::Line>,

    pub prof_per_frame: bool,
    pub prof_callers: bool,
    pub prof_note: Option<crate::memory::Line>,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            bp_target: String::new(),
            bp_label: String::new(),
            bp_note: None,
            w_target: String::new(),
            // The handler's own default is 1, and the box shows it rather than being blank: a length box
            // whose emptiness silently means "one byte" is a box that lies by omission.
            w_len: "1".into(),
            w_space: 0,
            w_read: false,
            // The handler's documented default op, shown rather than implied — `parse_watch_op` turns
            // *neither flag given* into a write watch, and a checkbox pair that started unticked would be
            // showing a state the server would not honour.
            w_write: true,
            w_stop_after: String::new(),
            w_label: String::new(),
            w_note: None,
            prof_per_frame: false,
            prof_callers: false,
            prof_note: None,
        }
    }
}

// ---------------------------------------------------------------------------------------------------

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::bus::{Answer, Bus};
    use crate::machine::Machine;
    use crate::memory;
    use oracle_aether::host::MachineInfo;

    fn rig() -> (Machine, Bus) {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let bus = Bus::new(machine.system_mut(), MachineInfo::default(), false, None);
        (machine, bus)
    }

    fn ok(a: &Answer) -> &Value {
        match a {
            Answer::Ok(v) => v,
            Answer::Err(e) => panic!("expected a reply, got REFUSED {} {}", e.code, e.message),
        }
    }

    /// **Every method these panels name is one the registry actually carries.**
    ///
    /// A typo produces a button whose only possible outcome is `-32601`, which the panel would render
    /// perfectly correctly and a human would read as "the emulator is broken". Checked against `METHODS`
    /// itself, the same slice `emulator/initialize` advertises from.
    ///
    /// **The alternative green path, ruled out:** an `is_served` that answered `true` unconditionally
    /// would pass the loop and prove nothing, so a name that cannot exist is checked in the same test.
    #[test]
    fn every_gesture_names_a_served_method() {
        for m in [
            BREAKPOINT_ADD,
            BREAKPOINT_SET_ENABLED,
            BREAKPOINT_CLEAR,
            WATCHPOINT_ADD,
            WATCHPOINT_CLEAR,
            SET_PROFILER,
        ] {
            assert!(
                memory::is_served(m),
                "a stopping panel offers {m}, which the METHODS registry does not carry"
            );
        }
        assert!(
            !memory::is_served("emulator/breakpoint_add_but_spelled_wrong"),
            "`is_served` answered true for a method that cannot exist, so the loop above witnesses nothing"
        );
    }

    /// ★ **The `watchpoint_add` traps, held closed by the handler rather than by memory.**
    ///
    /// Both are refusals that come from the handler and appear in no documentation the panel author would
    /// have read. This drives [`watch_add_params`] through a real `Host::call` and asserts the arm lands;
    /// then it asserts, in the same test, that the two shapes the panel might plausibly have sent instead
    /// are **refused** — which is what makes the first half a measurement rather than a coincidence.
    #[test]
    fn a_watch_arms_with_write_true_and_a_numeric_len_and_the_near_misses_are_refused() {
        let (mut machine, mut bus) = rig();

        let params = watch_add_params("0xFF0000", "16", "bus", false, true, "", "hud")
            .expect("a hex target and a decimal len are the panel's own valid input");
        // The shape assertions, before the call, because the call could pass for the wrong reason.
        assert_eq!(
            params.get("write"),
            Some(&json!(true)),
            "the op is carried by the `write` BOOLEAN; there is no `op` param on this row"
        );
        assert!(
            params.get("op").is_none(),
            "`op` is a key of the REPLY, not of the request — sending it is -32602"
        );
        assert!(
            params["len"].is_number(),
            "`len` must be a JSON number: hex::parse_count reads it with as_u64() and refuses a string"
        );

        let a = bus.call(machine.system_mut(), WATCHPOINT_ADD, &params);
        let v = ok(&a);
        assert_eq!(v.get("op"), Some(&json!("write")), "reply: {v}");
        assert_eq!(v.get("len"), Some(&json!(16)), "reply: {v}");
        let handle = v["watch"].as_str().expect("a watch handle").to_owned();

        // ⚑ Trap 1, demonstrated: `op: "write"` in place of the boolean is refused.
        let mut wrong = params.clone();
        wrong.as_object_mut().unwrap().remove("write");
        wrong["op"] = json!("write");
        let a = bus.call(machine.system_mut(), WATCHPOINT_ADD, &wrong);
        assert!(
            a.is_err(),
            "`op: \"write\"` was ACCEPTED, so the `write: true` above may have been unnecessary and this \
             test's first half witnesses nothing"
        );

        // ⚑ Trap 2, demonstrated: a hex-string `len` is refused.
        let mut wrong = params.clone();
        wrong["len"] = json!("0x10");
        let a = bus.call(machine.system_mut(), WATCHPOINT_ADD, &wrong);
        assert!(a.is_err(), "a hex-string `len` was accepted");
        assert!(
            a.reason().is_none() && matches!(&a, Answer::Err(e) if e.code == -32602),
            "a string `len` should be a params refusal from hex::parse_count"
        );

        // And the handle this module spells is the one the server answers to.
        let a = bus.call(
            machine.system_mut(),
            WATCHPOINT_CLEAR,
            &watch_clear_params(&handle),
        );
        assert_eq!(
            ok(&a).get("removed"),
            Some(&json!(1)),
            "watch_clear_params spelled a handle the server did not recognise: {handle:?}"
        );
    }

    /// The space list the selector offers is the one the handler accepts — and a fifth is refused.
    #[test]
    fn every_watch_space_the_selector_offers_is_one_the_handler_takes() {
        let (mut machine, mut bus) = rig();
        for space in WATCH_SPACES {
            let params = watch_add_params("0x0", "1", space, false, true, "", "")
                .expect("valid panel input");
            let a = bus.call(machine.system_mut(), WATCHPOINT_ADD, &params);
            assert!(
                !a.is_err(),
                "the selector offers space {space:?}, which the handler refuses"
            );
        }
        let params =
            watch_add_params("0x0", "1", "sram", false, true, "", "").expect("valid panel input");
        let a = bus.call(machine.system_mut(), WATCHPOINT_ADD, &params);
        assert!(
            a.is_err(),
            "the handler accepted a space not in WATCH_SPACES, so the loop above proves nothing about \
             the list being complete"
        );
    }

    /// ★ **A breakpoint round trip through the handle this module spells.**
    ///
    /// The alternative green path — `breakpoint_wire_id` and `resolve_breakpoint_handle` agreeing on some
    /// spelling that is not the one on the wire — is ruled out by taking the handle from
    /// `breakpoint_add`'s **reply** and asserting it equals the one [`breakpoints`] derived from the set.
    #[test]
    fn a_breakpoint_arms_toggles_and_clears_through_the_spelled_handle() {
        let (mut machine, mut bus) = rig();

        let params = breakpoint_add_params("0x000400", "vint").expect("valid panel input");
        let a = bus.call(machine.system_mut(), BREAKPOINT_ADD, &params);
        let served_handle = ok(&a)["breakpoint"].as_str().expect("a handle").to_owned();

        let view = breakpoints(bus.read_breakpoints(), None);
        assert_eq!(view.rows.len(), 1, "the panel should see the arm it made");
        assert_eq!(
            view.rows[0].handle, served_handle,
            "the panel spells a handle the server does not use"
        );
        assert_eq!(view.rows[0].addr_text, "0x00000400");
        assert!(view.rows[0].enabled);
        assert_eq!(view.live, Live::Yes);

        // Disable it: `hits` survives the toggle, and the panel's headline must change.
        let a = bus.call(
            machine.system_mut(),
            BREAKPOINT_SET_ENABLED,
            &breakpoint_enable_params(&served_handle, false),
        );
        assert!(!a.is_err(), "the toggle was refused");
        let view = breakpoints(bus.read_breakpoints(), None);
        assert_eq!(
            view.live,
            Live::Retained,
            "a set with rows and nothing enabled is RETAINED, not Yes and not Never — the row exists and \
             nothing in it will stop the machine"
        );
        assert_eq!(view.armed, 0);
        assert_eq!(view.rows.len(), 1, "disabling must not remove the row");

        // Clear it: back to Never.
        let a = bus.call(
            machine.system_mut(),
            BREAKPOINT_CLEAR,
            &breakpoint_clear_params(&served_handle),
        );
        assert_eq!(ok(&a).get("removed"), Some(&json!(1)));
        assert_eq!(breakpoints(bus.read_breakpoints(), None).live, Live::Never);
    }

    /// **`clear all` is its own gesture**, and the handler refuses the two mixed together.
    #[test]
    fn clear_all_is_a_separate_gesture_because_the_handler_refuses_the_mixture() {
        let (mut machine, mut bus) = rig();
        for a in ["0x400", "0x500"] {
            let p = breakpoint_add_params(a, "").expect("valid");
            assert!(!bus.call(machine.system_mut(), BREAKPOINT_ADD, &p).is_err());
        }
        assert_eq!(bus.read_breakpoints().len(), 2);

        let mut both = breakpoint_clear_all_params();
        both["breakpoint"] = json!("b0");
        assert!(
            bus.call(machine.system_mut(), BREAKPOINT_CLEAR, &both)
                .is_err(),
            "`all` and `breakpoint` together should be -32602; if they compose, this module could have \
             offered one function instead of two"
        );

        let a = bus.call(
            machine.system_mut(),
            BREAKPOINT_CLEAR,
            &breakpoint_clear_all_params(),
        );
        assert_eq!(ok(&a).get("removed"), Some(&json!(2)));
        assert!(bus.read_breakpoints().is_empty());
    }

    /// ★★ **THE trap: disarming the profiler retains the sample, and `Live` is what tells them apart.**
    ///
    /// This is the assertion the whole module exists for. A panel reading only the accumulator would see
    /// the identical rows on both sides of the disarm.
    ///
    /// **The third assertion, per the anti-vacuity rule.** *If this went green for a reason other than
    /// `Live` discriminating, what would it be?* It would be the sample being **empty** on both sides —
    /// `Retained` and `Never` are both "not armed", and an empty accumulator makes them indistinguishable
    /// while the test still reads as though it proved something. So the sample is asserted non-empty at
    /// the disarm, and the `Never` case is reached separately by a machine that never armed at all.
    #[test]
    fn disarming_the_profiler_keeps_the_rows_and_live_is_what_says_so() {
        let (mut machine, mut bus) = rig();

        // Never armed, nothing recorded.
        let (_, p, armed) = bus.read_instruments();
        assert!(!armed);
        assert_eq!(
            profiler_live(p, armed),
            Live::Never,
            "a fresh profiler has never been armed and has no sample"
        );

        let a = bus.call(
            machine.system_mut(),
            SET_PROFILER,
            &set_profiler_params(true, true, false),
        );
        assert_eq!(ok(&a).get("enabled"), Some(&json!(true)));
        let (_, p, armed) = bus.read_instruments();
        assert!(
            armed,
            "the arm did not reach the instrument the panel reads"
        );
        assert_eq!(profiler_live(p, armed), Live::Yes);

        // Run enough frames for the accountant to commit rows. `emulator/run_frames` is the served way to
        // advance, and it carries the profiler because the engine's own run does. It is **`require_paused`**
        // and the fixture is a free-running bus exactly like the player, so the pause goes through the tool
        // too rather than being arranged behind it.
        assert!(!bus
            .call(machine.system_mut(), "emulator/pause", &json!({}))
            .is_err());
        let a = bus.call(
            machine.system_mut(),
            "emulator/run_frames",
            &json!({"frames": 4}),
        );
        assert!(!a.is_err(), "run_frames was refused: {:?}", ok_or_err(&a));

        let (_, p, armed) = bus.read_instruments();
        // ⚑ The anti-vacuity clause, checked before it is leaned on.
        assert!(
            p.frames() > 0 && p.routine_count() > 0,
            "the sample is EMPTY, so `Retained` and `Never` below are indistinguishable and the disarm \
             assertion witnesses nothing. frames={} routines={}",
            p.frames(),
            p.routine_count()
        );
        let rows_while_armed = p.routine_count();
        let frames_while_armed = p.frames();
        assert!(armed);

        let a = bus.call(
            machine.system_mut(),
            SET_PROFILER,
            &set_profiler_params(false, false, false),
        );
        assert_eq!(ok(&a).get("enabled"), Some(&json!(false)));

        let (_, p, armed) = bus.read_instruments();
        assert!(!armed, "the disarm did not reach the flag the panel reads");
        assert_eq!(
            p.routine_count(),
            rows_while_armed,
            "disarming DROPPED the sample; §11.16 says it retains it, and this panel's whole \
             armed-vs-retained distinction rests on that"
        );
        assert_eq!(p.frames(), frames_while_armed);
        assert_eq!(
            profiler_live(p, armed),
            Live::Retained,
            "the rows are identical on both sides of the disarm — `Live` is the ONLY thing that can tell \
             a reader the difference, and it did not"
        );

        // …and the view a panel draws carries the distinction, not just the helper.
        let view = profiler(p, armed, None);
        assert_eq!(view.live, Live::Retained);
        assert!(!view.armed);
        assert!(
            view.live.has_rows(),
            "a retained sample has rows worth drawing; only Never does not"
        );
        assert!(
            view.live.sentence("x", "y").contains("STOPPED"),
            "the retained sentence must say so in words a human reads: {}",
            view.live.sentence("x", "y")
        );
    }

    /// **The fast selection returns exactly what the slow one did — ties included.**
    ///
    /// [`top_routines`] stopped sorting the whole sample: it selects the [`TOP_ROUTINES`] hottest pairs in
    /// one linear pass and formats only those, because formatting every routine cost 91 % of a frame
    /// budget (design §5.7.1). That is a **pure performance change**, so the rows a human sees must be
    /// byte-for-byte the old ones, and this is what says so. The old algorithm is re-derived here — map
    /// every routine, sort the lot, truncate — rather than a golden list being pasted in, so the reference
    /// stays a *statement of the rule* and not a snapshot of one run of it.
    ///
    /// **The fixture is built to be all ties, on purpose.** A partial selection can only diverge from a
    /// full sort where the comparator declines to separate two rows: with distinct keys, `select_nth` and
    /// `sort` cannot disagree about *which* rows are in the top 24, and any correct-looking result proves
    /// nothing. So the cycle counts take three values across `4 × TOP_ROUTINES` routines, which puts a tie
    /// group of ~32 rows **straddling the 24-row cut** — the exact place a partial selection is free to
    /// pick a different 24 — and that straddle is asserted before the comparison leans on it.
    ///
    /// The all-equal fixture is the same trap at its limit: every row ties every other, so the answer is
    /// decided entirely by the address tie-break, and a selection that dropped that tie-break would return
    /// an arbitrary 24 of 96 and still be "the hottest rows".
    #[test]
    fn the_top_selection_matches_a_full_sort_even_with_ties() {
        /// The old body, verbatim in shape: a row per routine, one sort of the whole vector, then the cut.
        fn reference(sample: &BTreeMap<u32, Counts>) -> Vec<(String, Counts)> {
            let mut all: Vec<(u32, Counts)> = sample.iter().map(|(&a, &c)| (a, c)).collect();
            all.sort_by(|a, b| b.1.cycles.cmp(&a.1.cycles).then(a.0.cmp(&b.0)));
            all.truncate(TOP_ROUTINES);
            all.into_iter().map(|(a, c)| (hex::addr(a), c)).collect()
        }

        fn actual(sample: &BTreeMap<u32, Counts>) -> Vec<(String, Counts)> {
            top_routines(sample, None)
                .into_iter()
                .map(|r| (r.addr_text, r.counts))
                .collect()
        }

        fn counts(cycles: u64) -> Counts {
            Counts {
                cycles,
                self_cycles: cycles / 2,
                stall_cycles: 0,
                calls: 1,
            }
        }

        // Addresses ascend with `i`, so the tie-break's ordering is not accidentally the insertion order
        // of the hot rows: the hottest group is spread across the whole address range.
        let n = 4 * TOP_ROUTINES;
        let strided: BTreeMap<u32, Counts> = (0..n)
            .map(|i| (0x00FF_0000 + (i as u32) * 6, counts((i % 3) as u64)))
            .collect();
        let flat: BTreeMap<u32, Counts> = (0..n)
            .map(|i| (0x00FF_0000 + (i as u32) * 6, counts(7)))
            .collect();

        // ⚑ Anti-vacuity, checked before the comparisons are leaned on. If the fixture were smaller than
        // the cut, or if no tie group straddled it, both implementations would be forced to the same
        // answer and this test would be green for a reason that has nothing to do with the change.
        assert!(
            strided.len() > TOP_ROUTINES,
            "the fixture does not reach the cut, so nothing is selected and nothing is proven"
        );
        let hottest = strided.values().map(|c| c.cycles).max().unwrap();
        let in_hottest = strided.values().filter(|c| c.cycles == hottest).count();
        assert!(
            in_hottest > TOP_ROUTINES,
            "the hottest tie group is {in_hottest} rows and the cut is {TOP_ROUTINES}; the boundary must \
             fall INSIDE a tie group or a partial selection has no freedom to diverge"
        );

        for (name, sample) in [("strided ties", &strided), ("all equal", &flat)] {
            let want = reference(sample);
            let got = actual(sample);
            assert_eq!(
                got.len(),
                TOP_ROUTINES,
                "{name}: the selection returned {} rows, not the {TOP_ROUTINES} the panel draws",
                got.len()
            );
            assert_eq!(
                got, want,
                "{name}: the fast selection picked a different table than a full sort would have. This \
                 is a pure performance change and the drawn rows must be identical."
            );
        }

        // A sample below the cut is returned whole, still in order — the branch that skips `select_nth`.
        let small: BTreeMap<u32, Counts> = (0..TOP_ROUTINES / 2)
            .map(|i| (0x0010_0000 + (i as u32) * 4, counts((i % 2) as u64)))
            .collect();
        assert_eq!(actual(&small), reference(&small), "short sample");
        assert_eq!(actual(&small).len(), TOP_ROUTINES / 2);

        // And the empty sample, which `select_nth_unstable_by` would panic on if the guard were dropped.
        let empty: BTreeMap<u32, Counts> = BTreeMap::new();
        assert!(actual(&empty).is_empty());
    }

    /// **Watch hits outlive the watch, and `Live` says so.** The `watchpoint_clear` handler keeps them on
    /// purpose; a panel drawing the log without the headline would show a live trace of a machine nothing
    /// is watching.
    ///
    /// **Third assertion:** the log is checked non-empty before the clear, because an empty log makes
    /// `Retained` and `Never` the same state and the test would pass on nothing.
    #[test]
    fn watch_hits_outlive_the_watch_and_live_says_so() {
        let (mut machine, mut bus) = rig();

        let (w, _, _) = bus.read_instruments();
        assert_eq!(watches_live(w), Live::Never, "a fresh instrument is Never");

        // A wide write watch over the whole of work RAM, which any running ROM writes to.
        let params = watch_add_params("0xFF0000", "65536", "bus", false, true, "", "ram")
            .expect("valid panel input");
        let a = bus.call(machine.system_mut(), WATCHPOINT_ADD, &params);
        let handle = ok(&a)["watch"].as_str().expect("a handle").to_owned();
        let (w, _, _) = bus.read_instruments();
        assert_eq!(watches_live(w), Live::Yes);

        // `run_frames` is `require_paused`; the pause goes through the tool like every other gesture here.
        assert!(!bus
            .call(machine.system_mut(), "emulator/pause", &json!({}))
            .is_err());
        let a = bus.call(
            machine.system_mut(),
            "emulator/run_frames",
            &json!({"frames": 2}),
        );
        assert!(!a.is_err(), "run_frames refused: {:?}", ok_or_err(&a));

        let (w, _, _) = bus.read_instruments();
        // ⚑ The anti-vacuity clause.
        assert!(
            !w.hits().is_empty(),
            "the watch recorded NOTHING, so `Retained` below would be reached via `seen` alone or not at \
             all, and the outlives-the-watch claim is untested. seen={} matched={}",
            w.seen(),
            w.matched()
        );
        let hits_before = w.hits().len();

        let a = bus.call(
            machine.system_mut(),
            WATCHPOINT_CLEAR,
            &watch_clear_params(&handle),
        );
        assert_eq!(ok(&a).get("removed"), Some(&json!(1)));

        let (w, _, _) = bus.read_instruments();
        assert_eq!(w.watch_count(), 0, "the watch should be gone");
        assert_eq!(
            w.hits().len(),
            hits_before,
            "clearing the watch deleted its hits; the handler says it must not, and this panel's \
             retained-vs-live distinction is built on that"
        );
        assert_eq!(
            watches_live(w),
            Live::Retained,
            "no watch armed and a full hit log is RETAINED — a panel calling that live would be showing a \
             trace of a machine nothing is watching"
        );

        let view = watches(w);
        assert!(view.watches.is_empty());
        assert_eq!(view.hits.len(), hits_before);
        assert_eq!(view.live, Live::Retained);
    }

    /// `Live::Never` is a genuinely reachable third state for all three, and it is **not** the same value
    /// as `Retained`. Without this the two could have been one bool and every assertion above would still
    /// pass.
    #[test]
    fn the_three_live_states_are_three_and_never_is_reachable_for_each() {
        let (_, bus) = rig();
        let (w, p, armed) = bus.read_instruments();
        assert_eq!(breakpoints_live(bus.read_breakpoints()), Live::Never);
        assert_eq!(watches_live(w), Live::Never);
        assert_eq!(profiler_live(p, armed), Live::Never);
        assert_ne!(Live::Never, Live::Retained);
        assert_ne!(Live::Yes, Live::Retained);
        assert!(
            !Live::Never.has_rows(),
            "Never must refuse to draw a table; an empty grid in its place asserts a measurement nobody \
             took"
        );
        assert!(Live::Yes.has_rows() && Live::Retained.has_rows());
    }

    /// The panel's own refusals are about input the server never sees, and everything else is deferred.
    #[test]
    fn the_panel_refuses_only_what_the_server_would_never_receive() {
        assert!(
            target_params("  ").is_err(),
            "an empty box is the panel's own"
        );
        assert_eq!(
            target_params("0xFF0000").expect("hex"),
            json!({"addr": "0xFF0000"})
        );
        assert_eq!(
            target_params("Player_1").expect("a name"),
            json!({"symbol": "Player_1"}),
            "a name goes to the server as `symbol`; the panel does not resolve it and does not write its \
             own not-found sentence"
        );
        // A `len` that is not a decimal count is the panel's, because the panel chose to send a number.
        assert!(watch_add_params("0x0", "0x10", "bus", false, true, "", "").is_err());
        // …and a `read:false, write:false` is NOT the panel's: the handler has a better sentence for it.
        let p = watch_add_params("0x0", "1", "bus", false, false, "", "")
            .expect("the panel must not pre-empt the handler's refusal here");
        assert_eq!(p["read"], json!(false));
        assert_eq!(p["write"], json!(false));
    }

    fn ok_or_err(a: &Answer) -> String {
        match a {
            Answer::Ok(v) => v.to_string(),
            Answer::Err(e) => format!("{} {}", e.code, e.message),
        }
    }
}
