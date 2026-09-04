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
use oracle_aether::engine::{breakpoint_wire_id, symbol_at, watch_wire_id, LastBreak};
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
// ⚑ Halting — what can stop the GAME, whether it just did, and the way out (ARMED-STATE-VISIBLE)
// ---------------------------------------------------------------------------------------------------

/// **The one derivation behind every surface that says "something is armed to stop this window".**
///
/// # The incident this exists for
///
/// A breakpoint was left armed. It halted the machine, the machine stayed halted, and **nothing on the
/// glass said so or offered a way out** — so the window read as a dead one and had to be released by hand
/// by somebody who happened to know what a breakpoint was. The requirement that came out of it is three
/// clauses long and this type owes all three: *anything armed that can halt the game has to say it is
/// armed, say it just halted, and give you the way out.*
///
/// # ⚑ In THIS window, a breakpoint is the only thing that can halt the running game
///
/// That is not an assumption, it is what
/// [`Engine::run_sinks`](oracle_aether::engine::Engine::run_sinks) does, and it is worth stating because
/// the obvious guess is wrong:
///
/// * The **breakpoint** sink is lent to the player's per-frame run **bare**, so its stop reaches
///   `run_frames_with_sink` and ends the run mid-frame. It halts the game.
/// * The **watch** and the **profiler** are lent wrapped in `Observe`, *deliberately*, because a watch's
///   `stopAfter` is a level (`matched >= n` stays true forever) and a bare one would end every 1-frame run
///   the window makes. `Observe` drops the halt and keeps the observations. **A watch cannot freeze this
///   window.**
///
/// So the transport bar's existing armed summary — watches and the profiler, and no breakpoints — names
/// the two things that *cannot* stop the running game and omits the one that can. [`Halting`] is the
/// other half, and [`stopping_watches`](Self::stopping_watches) keeps the watches honest rather than
/// silent: they still end a **commanded** run (`emulator/step`, a client's `run_frames`), which is a real
/// surprise and a different sentence.
///
/// # Why not [`Live`]
///
/// [`Live`] answers *are the rows on this table being produced right now, or left over from when they
/// were* — a question about a **table's freshness**, and the right question for the three tabs. This one
/// is about the **machine**: can it be stopped, was it, and how do I start it again. They agree on
/// exactly one bit ([`Live::Yes`] for breakpoints is `any_enabled()`, which is what
/// [`armed`](Self::armed) counts, and `breakpoints_live_agrees_with_halting` pins that), and they differ
/// on everything a reader of a frozen window actually needs. Collapsing the two would have put "how do I
/// get out" inside a type whose other two variants are both *"the table is stale"*.
pub struct Halting {
    /// Breakpoints **enabled** — `Breakpoints::any_enabled`'s predicate, counted. This is the exact
    /// condition `Engine::run_sinks` attaches the halting sink on, so a non-zero here means the sink is
    /// really riding the player's frames.
    pub armed: usize,
    /// Every armed breakpoint's handle, complete and uncapped, in set order. **The way out is built from
    /// this** ([`release_gestures`]), so it must not be the capped display list.
    pub armed_handles: Vec<String>,
    /// The armed addresses as [`hex::addr`] spells them — deduped, in set order, capped at
    /// [`NAMED_ADDRS`]. Display only.
    pub armed_at: Vec<String>,
    /// Armed addresses [`armed_at`](Self::armed_at) could not name. Shown as a count rather than dropped:
    /// a clipped list that does not say it is clipped is a list a reader will read as complete.
    pub more_addrs: usize,
    /// Whether the machine is stopped right now — [`crate::bus::Bus::is_paused`], which is the *truthful*
    /// reading (it consults `pending_free_run`, which a `Host::call` does not apply).
    pub stopped: bool,
    /// The engine's own record of the last halt it performed. `None` means this window has never halted
    /// on a breakpoint — a different fact from "it halted and I have forgotten".
    pub last: Option<LastBreak>,
    /// Emulated frames between the last halt and now. `Some(0)` means **the machine has not completed a
    /// frame since it halted**, which is what makes "you are stopped *because of* that breakpoint" a
    /// derivation rather than a guess: a manual pause two hundred frames later reads `Some(200)` and gets
    /// a different sentence.
    pub frames_since: Option<u64>,
    /// The listing's name for the last halt's `pc`, through the same `symbol_at` the handlers use.
    pub last_symbol: Option<(String, u32)>,
    /// Watches carrying `stopAfter`. **They cannot halt the running game in this window** (see the type
    /// note) but they do end a commanded run, so they are counted here rather than left to surprise
    /// somebody who clicks `step`.
    pub stopping_watches: usize,
}

/// How many armed addresses [`Halting::armed_at`] names inline before it starts counting instead. Four,
/// because this line is drawn in a single-row top bar beside five other things.
pub const NAMED_ADDRS: usize = 4;

/// **The label on the halting row / bar segment.** A constant because two surfaces draw it and the tests
/// derive their expectations from it rather than retyping it.
pub const HALTING_LABEL: &str = "armed to halt";

impl Halting {
    /// Derive it. Every input is a shared borrow of something the loop itself feeds (R2) — the same
    /// `Breakpoints` `emulator/breakpoint_list` pages, the same `Watchpoints`
    /// `emulator/watchpoint_list` answers from, and the engine's own halt record.
    ///
    /// `now_frame` is `mclk / MCLK_PER_FRAME`, the same expression the D11 stamp and
    /// [`LastBreak::frame`] use — the comparison in [`frames_since`](Self::frames_since) is only
    /// meaningful because both sides are that one derivation.
    pub fn of(
        set: &Breakpoints,
        last: Option<LastBreak>,
        watches: &Watchpoints,
        stopped: bool,
        now_frame: u64,
        symbols: Option<&SymbolTable>,
    ) -> Self {
        let armed_handles: Vec<String> = set
            .iter()
            .filter(|b| b.enabled)
            .map(|b| breakpoint_wire_id(b.id))
            .collect();
        let mut addrs: Vec<String> = Vec::new();
        for b in set.iter().filter(|b| b.enabled) {
            let text = hex::addr(b.addr);
            if !addrs.contains(&text) {
                addrs.push(text);
            }
        }
        let more_addrs = addrs.len().saturating_sub(NAMED_ADDRS);
        addrs.truncate(NAMED_ADDRS);
        Halting {
            armed: armed_handles.len(),
            armed_handles,
            armed_at: addrs,
            more_addrs,
            stopped,
            last,
            // `saturating_sub` rather than a subtraction: a client's `emulator/restore` can wind the
            // machine's clock *backwards* under a halt record taken on the timeline before it, and a
            // wrapped `u64` would render as "halted 18 quintillion frames ago".
            frames_since: last.map(|b| now_frame.saturating_sub(b.frame)),
            last_symbol: last.and_then(|b| symbols.and_then(|t| symbol_at(t, b.pc))),
            stopping_watches: watches
                .watches()
                .iter()
                .filter(|w| w.stop_after.is_some())
                .count(),
        }
    }

    /// Whether anything armed can end the player's own frame run. The predicate every surface gates its
    /// alarm on.
    pub fn can_halt(&self) -> bool {
        self.armed > 0
    }

    /// **Whether the machine is stopped AND a breakpoint is why**, as far as this can be derived.
    ///
    /// `frames_since == Some(0)` says the machine has not completed a frame since the halt, so nothing
    /// has run that could have stopped it for another reason. A stopped machine whose last halt was
    /// frames ago was stopped by something else — a human on the pause button, or a client — and saying
    /// "halted by a breakpoint" there would be the confident wrong sentence this whole surface exists to
    /// avoid.
    pub fn halted_here(&self) -> bool {
        self.stopped && self.frames_since == Some(0)
    }

    /// **The line the top bar and the status strip both put on the glass**, or `None` when there is
    /// genuinely nothing to say.
    ///
    /// `None` rather than `"nothing armed"`, on [`crate::ui::StatusStrip::held_row`]'s precedent: a
    /// permanent row that reads *all clear* is a row every reader learns to skip, which is how the one
    /// day in a hundred that it says something else gets skipped too. The absence is only ever taken when
    /// nothing is armed, nothing is stopping the machine, and nothing ever halted it — a state in which
    /// there is no armed lens to state persistently.
    pub fn headline(&self) -> Option<String> {
        let halts = self.last.map_or(0, |b| b.ordinal);
        let where_ = match (&self.last, &self.last_symbol) {
            (Some(b), Some((n, 0))) => format!("{} ({n})", hex::addr(b.pc)),
            (Some(b), Some((n, d))) => format!("{} ({n}+0x{d:X})", hex::addr(b.pc)),
            (Some(b), None) => hex::addr(b.pc),
            (None, _) => String::new(),
        };
        // Matched on `last` *filtered by* `halted_here` rather than on the two bools plus `last`: the
        // combination `halted_here() && last.is_none()` cannot occur (`halted_here` requires
        // `frames_since == Some(0)`, which requires a record), and a `match` written to spell it out
        // would need an arm for a state that has no honest sentence.
        match (self.last.filter(|_| self.halted_here()), self.can_halt()) {
            // ⚑ The incident, exactly. Stopped, a breakpoint is why, and it is still armed — so the next
            // resume stops again. `halts` is what keeps this readable in the REPEATING case: an edge
            // ("it just halted") would flicker and then go quiet, and a window that has gone quiet is the
            // one that gets read as broken.
            (Some(b), true) => Some(format!(
                "⏹ HALTED BY BREAKPOINT {} at {where_} — {halts} halt{}; {} still armed",
                breakpoint_wire_id(b.id),
                if halts == 1 { "" } else { "s" },
                self.armed,
            )),
            // Stopped at a breakpoint, and it has since been disarmed or cleared. Resume will run.
            (Some(b), false) => Some(format!(
                "⏹ stopped at breakpoint {} at {where_} — {halts} halt{}; nothing is armed now, so \
                 resume will run",
                breakpoint_wire_id(b.id),
                if halts == 1 { "" } else { "s" },
            )),
            // Armed and running (or stopped for some other reason — the `frames_since` clause says which
            // rather than claiming the breakpoint did it).
            (None, true) => Some(format!(
                "⚠ ARMED TO HALT — {} breakpoint{} at {}{}{}",
                self.armed,
                if self.armed == 1 { "" } else { "s" },
                if self.armed_at.is_empty() {
                    "—".to_owned()
                } else {
                    self.armed_at.join(", ")
                },
                if self.more_addrs > 0 {
                    format!(" +{} more", self.more_addrs)
                } else {
                    String::new()
                },
                match (self.stopped, self.frames_since) {
                    (true, Some(n)) => format!(
                        " · the machine is stopped, but its last breakpoint halt was {n} frame{} ago, \
                         so something else stopped it",
                        if n == 1 { "" } else { "s" }
                    ),
                    (true, None) => " · the machine is stopped, and no breakpoint has ever halted it, \
                                     so something else stopped it"
                        .to_owned(),
                    (false, _) => format!(
                        " · running; {halts} halt{} so far",
                        if halts == 1 { "" } else { "s" }
                    ),
                }
            )),
            // Nothing armed and the machine is running: whatever halted it before is history and cannot
            // recur. No alarm.
            (None, false) => None,
        }
    }

    /// **The way out, in words, naming the exact calls.**
    ///
    /// `None` exactly when [`headline`](Self::headline) is `None`. §9.4's rule, one instrument over: *the
    /// remedy is one call, but you have to know to make it* — so the surface is where you learn it. The
    /// watch clause rides here rather than in the headline because it is a *different* surprise (it ends
    /// a commanded run, not the game) and the headline is one row on a shared bar.
    pub fn advice(&self) -> Option<String> {
        self.headline()?;
        let mut s = if self.can_halt() {
            let (label, _) = self.release_label();
            format!(
                "the {} button disarms {} ({} for each armed handle){}, or untick them one at a time in \
                 the Breakpoints tab",
                label,
                if self.armed == 1 {
                    "it".to_owned()
                } else {
                    format!("all {}", self.armed)
                },
                BREAKPOINT_SET_ENABLED,
                if self.halted_here() {
                    format!(" and then issues {}", crate::ui::RESUME)
                } else {
                    String::new()
                },
            )
        } else {
            format!(
                "nothing is armed; {} runs the machine on",
                crate::ui::RESUME
            )
        };
        if self.stopping_watches > 0 {
            s.push_str(&format!(
                " · {} watch{} carr{} stopAfter: those cannot freeze the running game here (the halt is \
                 dropped by Observe) but they DO end a step or a client's run",
                self.stopping_watches,
                if self.stopping_watches == 1 { "" } else { "es" },
                if self.stopping_watches == 1 { "ies" } else { "y" },
            ));
        }
        Some(s)
    }

    /// **The release control's label and whether it resumes**, as one decision.
    ///
    /// A function rather than two expressions at the click site for [`crate::ui::Transport::toggle`]'s
    /// reason: `emulator/screen_text` reports the label, and a button that said *release* while issuing
    /// only a disarm is a defect a readback of the bar could not tell from a correct window.
    ///
    /// It resumes **only** when the machine is stopped and a breakpoint is why
    /// ([`halted_here`](Self::halted_here)). Resuming a machine a human deliberately paused, because they
    /// clicked a button about breakpoints, would be this surface taking a run-control decision nobody
    /// asked it for.
    pub fn release_label(&self) -> (&'static str, bool) {
        if self.halted_here() {
            (RELEASE_LABEL, true)
        } else {
            (DISARM_LABEL, false)
        }
    }
}

/// The release control's two labels. Constants for [`crate::ui::PAUSE_LABEL`]'s reason — `screen_text`
/// reports them.
pub const RELEASE_LABEL: &str = "⏏ release";
pub const DISARM_LABEL: &str = "⏏ disarm";

/// **The way out, as the sequence of `(method, params)` pairs it really is.**
///
/// One `emulator/breakpoint_set_enabled {enabled:false}` per armed handle, then — only when the machine
/// is stopped *because* of a breakpoint — `emulator/resume`. Built here rather than inlined at the click
/// site so it can be checked against the handlers without a window, exactly as every other gesture in
/// this module is.
///
/// # ⚑ Why disable and not `breakpoint_clear {all:true}`
///
/// `clear {all:true}` is already offered by the Breakpoints tab and is the *destructive* gesture: it
/// removes every breakpoint on this server **including ones another client armed**, and it takes their
/// `hits` with them. This button is pressed by somebody who does not yet know why their window is frozen,
/// which is the worst possible moment to destroy another client's evidence. Disabling stops the halting —
/// which is the whole complaint — retains every row and every count (§6 carries `hits` across the toggle),
/// and is undone by re-ticking the box.
///
/// # ⚑ Why a sequence and not one call
///
/// There is no `disable_all` method, and inventing one *inside the panel* — a loop that decides for itself
/// that N successes mean success — is exactly the "a panel composes its own answer" shape this module
/// refuses. Each pair here is judged by the handler on its own, and the caller shows whichever refusal
/// arrives first, in the server's words.
pub fn release_gestures(h: &Halting) -> Vec<(&'static str, Value)> {
    let mut g: Vec<(&'static str, Value)> = h
        .armed_handles
        .iter()
        .map(|handle| {
            (
                BREAKPOINT_SET_ENABLED,
                breakpoint_enable_params(handle, false),
            )
        })
        .collect();
    if h.release_label().1 {
        g.push((crate::ui::RESUME, json!({})));
    }
    g
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

    // -----------------------------------------------------------------------------------------------
    // ARMED-STATE-VISIBLE — what can halt the game, that it did, and the way out
    // -----------------------------------------------------------------------------------------------

    /// The address the fixture ROM's inner loop runs on every pass (`testrom`'s `INNER`, `move.w (A0),
    /// D0`). A breakpoint here halts within the first emulated frame and **halts again on every resume**,
    /// which is the incident's own shape rather than a convenient one-shot.
    const HOT: &str = "0x20E";

    /// **One iteration of the player's real loop**, in `Loop::iterate`'s order: adopt the bus's run state
    /// at the top, run a frame only if not paused, drain at the bottom.
    ///
    /// Written out rather than approximated with a bare `Host::call`, because the ordering *is* the halt
    /// path: `Machine::step` latches the observation through `Bus::record_break` and only the drain
    /// applies it. A test that armed a breakpoint and then called `emulator/run_frames` would exercise
    /// the engine's own run driver and never touch the arrangement the window actually uses.
    fn iterate(machine: &mut Machine, bus: &mut Bus, paused: &mut bool) {
        *paused = bus.is_paused();
        if !*paused {
            machine.step([oracle_core::io::Pad::default(); 2], bus);
        }
        let mut symbols = None;
        let mut rom_path = String::new();
        crate::bus::drain(machine, bus, &mut symbols, &mut rom_path, *paused);
    }

    /// Run the player's loop until the machine stops, or give up. Returns whether it stopped.
    fn run_until_halted(machine: &mut Machine, bus: &mut Bus, paused: &mut bool) -> bool {
        for _ in 0..8 {
            iterate(machine, bus, paused);
            if bus.is_paused() {
                return true;
            }
        }
        false
    }

    fn halting_now(machine: &Machine, bus: &Bus) -> Halting {
        let (watch, _, _) = bus.read_instruments();
        Halting::of(
            bus.read_breakpoints(),
            bus.last_break(),
            watch,
            bus.is_paused(),
            machine.system().scheduler().now() / oracle_core::system::MCLK_PER_FRAME,
            None,
        )
    }

    /// ★ **The incident, reproduced and then read back off the surface that was silent for it.**
    ///
    /// A breakpoint is armed at an address the ROM runs constantly, the *player's own loop* is driven —
    /// not `emulator/run_frames`, which is a different run driver — and the window is asked what it can
    /// say. It must say all three things the queue row demands: that something is armed, that it just
    /// halted, and how to get out. Then the machine is resumed and halted **again**, because the failure
    /// was a repeating halt and a surface that reports the first one and goes quiet reproduces it.
    ///
    /// **Alternative green paths ruled out, in order:**
    ///
    /// 1. *Nothing actually halted and the sentences are decoration.* Ruled out by asserting the bus is
    ///    stopped and that `LastBreak` exists before a single string is looked at.
    /// 2. *The headline is a constant.* Ruled out by comparing the halted headline against the armed-and-
    ///    running headline taken from the same fixture, and requiring them to differ.
    /// 3. *The count is the same number twice.* Ruled out by requiring `ordinal` to have RISEN across the
    ///    second halt and the headline to carry the new figure.
    #[test]
    fn a_breakpoint_that_halts_the_players_own_loop_is_said_out_loud_and_offers_the_way_out() {
        let (mut machine, mut bus) = rig();
        let mut paused = false;

        // Armed and running: the alarm is already up, before anything has halted. This is the state a
        // reader needs in order NOT to be surprised later, and it is also alternative-green-path 2's
        // control.
        let a = bus.call(
            machine.system_mut(),
            BREAKPOINT_ADD,
            &breakpoint_add_params(HOT, "hot").expect("a hex target"),
        );
        let handle = ok(&a)["breakpoint"].as_str().expect("a handle").to_owned();
        let armed_only = halting_now(&machine, &bus);
        assert_eq!(armed_only.armed, 1);
        assert!(armed_only.last.is_none(), "nothing has halted yet");
        let running_head = armed_only
            .headline()
            .expect("an armed breakpoint must be stated while the machine is still running");
        assert!(
            running_head.contains("ARMED") && running_head.contains("0x0000020E"),
            "the running-and-armed line must name what is armed and where: {running_head}"
        );

        // 1: the halt is real, through the window's own loop.
        assert!(
            run_until_halted(&mut machine, &mut bus, &mut paused),
            "an armed breakpoint on the fixture ROM's inner loop must halt the player's own frame run"
        );
        let h = halting_now(&machine, &bus);
        let first = h
            .last
            .expect("the engine must have recorded the halt it performed");
        assert_eq!(first.ordinal, 1, "the first halt is the first halt");
        assert!(
            h.halted_here(),
            "the machine is stopped and has not completed a frame since the halt, so this IS why"
        );

        let head = h.headline().expect("a halted window must say so");
        let advice = h.advice().expect("…and must say how to get out");
        assert!(
            head.contains("HALTED") && head.contains(&handle) && head.contains("0x0000020E"),
            "the headline must name the state, the handle and the address: {head}"
        );
        assert!(
            head.contains("1 halt") && head.contains("still armed"),
            "…and that it is still armed, or a reader does not know resuming will stop again: {head}"
        );
        assert!(
            advice.contains(BREAKPOINT_SET_ENABLED) && advice.contains(crate::ui::RESUME),
            "the way out must name the calls it makes: {advice}"
        );
        // 2: not a constant.
        assert_ne!(
            head, running_head,
            "the halted headline is the same string as the armed-and-running one, so the sentence is \
             two copies of one untouched value and says nothing about the halt"
        );

        // 3: THE REPEATING CASE. Resume, and it halts again — the surface must report the second one.
        let a = bus.call(machine.system_mut(), crate::ui::RESUME, &json!({}));
        assert!(!a.is_err(), "resume: {}", ok_or_err(&a));
        assert!(
            run_until_halted(&mut machine, &mut bus, &mut paused),
            "the breakpoint is still armed, so the resumed machine must halt again — if it did not, the \
             repeating case this parcel exists for is not being exercised"
        );
        let h2 = halting_now(&machine, &bus);
        let second = h2.last.expect("a second halt");
        assert_eq!(
            second.ordinal, 2,
            "the halt count must RISE; a surface that latched the first halt and stopped counting goes \
             quiet exactly when the window is re-freezing every resume"
        );
        let head2 = h2.headline().expect("still halted, still said");
        assert!(head2.contains("2 halts"), "{head2}");
        assert_ne!(
            head2, head,
            "the second halt rendered identically to the first: the count on the glass is not moving, \
             which is the failure mode that reads as a dead window"
        );
    }

    /// ★ **The way out actually works, through the server, and destroys nothing.**
    ///
    /// [`release_gestures`] is driven through real `Host::call`s from a genuinely halted machine, and the
    /// machine must then run again. The gentleness is asserted too: disabling is not clearing, so the row
    /// and its `hits` have to survive — this button is pressed by somebody who does not yet know what is
    /// wrong, and losing another client's breakpoints for them would be a second incident.
    ///
    /// **Alternative green paths ruled out:**
    ///
    /// 1. *Nothing was armed, so "nothing is armed afterwards" is vacuous.* Ruled out by asserting
    ///    `any_enabled()` before the release.
    /// 2. *The release cleared everything, which also disarms.* Ruled out by asserting the row survives
    ///    with its `hits` intact.
    /// 3. *The resume did not actually restart the machine.* Ruled out by running another iteration and
    ///    requiring the emulated clock to have moved.
    #[test]
    fn the_release_disarms_through_the_server_resumes_and_keeps_every_row() {
        let (mut machine, mut bus) = rig();
        let mut paused = false;
        let a = bus.call(
            machine.system_mut(),
            BREAKPOINT_ADD,
            &breakpoint_add_params(HOT, "hot").expect("a hex target"),
        );
        let handle = ok(&a)["breakpoint"].as_str().expect("a handle").to_owned();
        assert!(run_until_halted(&mut machine, &mut bus, &mut paused));

        let h = halting_now(&machine, &bus);
        // 1: there is something to disarm.
        assert!(
            bus.read_breakpoints().any_enabled(),
            "nothing is armed, so the assertions below would pass over an empty world"
        );
        let hits_before: u64 = bus.read_breakpoints().iter().map(|b| b.hits).sum();
        assert!(hits_before > 0, "the halt must have been counted");

        let gestures = release_gestures(&h);
        assert_eq!(
            gestures.len(),
            2,
            "one disarm per armed handle, then the resume: {gestures:?}"
        );
        assert_eq!(gestures[0].0, BREAKPOINT_SET_ENABLED);
        assert_eq!(
            gestures[0].1,
            json!({"breakpoint": handle, "enabled": false})
        );
        assert_eq!(gestures[1].0, crate::ui::RESUME);
        for (method, params) in &gestures {
            let a = bus.call(machine.system_mut(), method, params);
            assert!(!a.is_err(), "{method} refused: {}", ok_or_err(&a));
        }

        assert!(
            !bus.read_breakpoints().any_enabled(),
            "the release left something armed, so the window would halt again on the next frame"
        );
        // 2: gentle, not destructive.
        assert_eq!(
            bus.read_breakpoints().len(),
            1,
            "the release CLEARED the breakpoint. It must disable it — the person pressing this does not \
             know what is wrong yet, and another client's rows are not theirs to destroy"
        );
        assert_eq!(
            bus.read_breakpoints().iter().map(|b| b.hits).sum::<u64>(),
            hits_before,
            "§6 carries `hits` across the toggle, so a disarm must not move the count"
        );

        // 3: the machine really runs again.
        let mclk_before = machine.system().scheduler().now();
        iterate(&mut machine, &mut bus, &mut paused);
        assert!(
            machine.system().scheduler().now() > mclk_before,
            "the release reported success and the machine is still frozen"
        );
        assert!(
            !halting_now(&machine, &bus).can_halt(),
            "nothing is armed any more, so the alarm must stand down"
        );
    }

    /// **Release resumes only when a breakpoint is why the machine stopped**, and says which button it is.
    ///
    /// A human who paused the window and then pressed a button about breakpoints did not ask to be
    /// resumed. The label and the behaviour are one decision ([`Halting::release_label`]) for
    /// [`crate::ui::Transport::toggle`]'s reason — `emulator/screen_text` reports the label, so a button
    /// that read *release* while issuing only a disarm would be wrong in public.
    ///
    /// **⚑ The third assertion.** The two rows are agreement, not correctness: a `release_label` that
    /// ignored its argument satisfies both. So the two states are asserted to produce **different**
    /// answers.
    #[test]
    fn release_resumes_only_when_a_breakpoint_is_why_the_machine_is_stopped() {
        let (mut machine, mut bus) = rig();
        let mut paused = false;
        bus.call(
            machine.system_mut(),
            BREAKPOINT_ADD,
            &breakpoint_add_params(HOT, "").expect("a hex target"),
        );
        assert!(run_until_halted(&mut machine, &mut bus, &mut paused));

        let halted = halting_now(&machine, &bus);
        assert_eq!(halted.release_label(), (RELEASE_LABEL, true));
        assert!(release_gestures(&halted)
            .iter()
            .any(|(m, _)| *m == crate::ui::RESUME));

        // The same set, the same halt record — but the machine has since advanced past it, which is what
        // a human pausing later looks like. Built by hand rather than by driving the loop, so the ONLY
        // thing that differs is the fact under test.
        let paused_by_hand = Halting {
            frames_since: Some(200),
            ..halting_now(&machine, &bus)
        };
        assert_eq!(paused_by_hand.release_label(), (DISARM_LABEL, false));
        assert!(
            !release_gestures(&paused_by_hand)
                .iter()
                .any(|(m, _)| *m == crate::ui::RESUME),
            "a button about breakpoints must not resume a machine a human deliberately paused"
        );
        assert_ne!(
            halted.release_label(),
            paused_by_hand.release_label(),
            "the agreement above is two copies of one untouched value: `release_label` ignored its own \
             state and would offer the same button in both"
        );
        // …and the headline says which, rather than claiming the breakpoint stopped it.
        let head = paused_by_hand.headline().expect("still armed, still said");
        assert!(
            head.contains("something else stopped it"),
            "a stop the breakpoint cannot account for must be named as one: {head}"
        );
    }

    /// **Every call the way out makes is a method the registry carries**, checked against `METHODS`.
    ///
    /// A typo here is a button whose only outcome is `-32601` on a window that is already frozen. The
    /// alternative green path — an `is_served` that answers `true` for everything — is ruled out in the
    /// same test.
    #[test]
    fn every_release_gesture_names_a_served_method() {
        let (mut machine, mut bus) = rig();
        bus.call(
            machine.system_mut(),
            BREAKPOINT_ADD,
            &breakpoint_add_params(HOT, "").expect("a hex target"),
        );
        let h = Halting {
            frames_since: Some(0),
            stopped: true,
            last: Some(oracle_aether::engine::LastBreak {
                id: oracle_aether::breakpoints::BreakpointId(1),
                pc: 0x20E,
                frame: 0,
                ordinal: 1,
            }),
            ..halting_now(&machine, &bus)
        };
        let gestures = release_gestures(&h);
        assert!(!gestures.is_empty(), "an empty sequence names nothing");
        for (m, _) in &gestures {
            assert!(
                memory::is_served(m),
                "the release sequence offers {m}, which the METHODS registry does not carry"
            );
        }
        assert!(
            !memory::is_served("emulator/breakpoint_set_enabled_but_spelled_wrong"),
            "`is_served` answered true for a method that cannot exist, so the loop witnesses nothing"
        );
    }

    /// ★ **The panel's armed set and `emulator/breakpoint_list` are one answer** (§4.4 R2/R3), and
    /// [`Halting`] really filters rather than echoing.
    ///
    /// The fixture holds **three** breakpoints of which **two** are armed, which is what makes the third
    /// assertion possible: a `Halting` that simply copied the list would carry three handles and agree
    /// with `breakpoint_list`'s row count perfectly.
    ///
    /// **Alternative green paths ruled out:**
    ///
    /// 1. *Both sides are empty.* Ruled out by requiring a non-empty armed set.
    /// 2. *The derivation did nothing.* Ruled out by the `assert_ne!` against the served list's own
    ///    handles — the raw input — with the disabled row named in the failure message.
    /// 3. *The handles are spelled by this panel rather than by the server.* Ruled out by taking the
    ///    expected handles out of the **reply**, never from `breakpoint_wire_id` a second time.
    #[test]
    fn the_armed_set_the_window_names_is_the_one_breakpoint_list_serves() {
        let (mut machine, mut bus) = rig();
        for (target, enabled) in [("0x20E", true), ("0x204", true), ("0x218", false)] {
            let a = bus.call(
                machine.system_mut(),
                BREAKPOINT_ADD,
                &breakpoint_add_params(target, "").expect("a hex target"),
            );
            let handle = ok(&a)["breakpoint"].as_str().expect("a handle").to_owned();
            if !enabled {
                let a = bus.call(
                    machine.system_mut(),
                    BREAKPOINT_SET_ENABLED,
                    &breakpoint_enable_params(&handle, false),
                );
                assert!(!a.is_err(), "{}", ok_or_err(&a));
            }
        }

        // The tool's own answer, in process, through `Host::call` — D15's "a consumer of the same
        // registry, not a second server".
        let a = bus.call(machine.system_mut(), "emulator/breakpoint_list", &json!({}));
        let reply = ok(&a).clone();
        let rows = reply["breakpoints"]
            .as_array()
            .expect("breakpoint_list serves an array");
        let served_armed: Vec<String> = rows
            .iter()
            .filter(|r| r["enabled"] == json!(true))
            .map(|r| r["breakpoint"].as_str().expect("a handle").to_owned())
            .collect();
        let served_all: Vec<String> = rows
            .iter()
            .map(|r| r["breakpoint"].as_str().expect("a handle").to_owned())
            .collect();

        let h = halting_now(&machine, &bus);
        // 1: there is something to compare.
        assert_eq!(served_armed.len(), 2, "the fixture must arm two: {reply}");
        assert_eq!(
            h.armed_handles, served_armed,
            "the window's armed set and `emulator/breakpoint_list`'s have DRIFTED"
        );
        assert_eq!(h.armed, served_armed.len());
        // 2 — ⚑ the third assertion: the shared derivation actually did something. A `Halting` that
        // echoed the list would satisfy every row above.
        assert_ne!(
            h.armed_handles, served_all,
            "the agreement above is two copies of one untouched value: `Halting` carried every \
             breakpoint the server holds, including the DISABLED one, so nothing filtered on `enabled` \
             and the window would raise an alarm about a breakpoint that cannot halt anything"
        );
        assert_eq!(
            served_all.len(),
            3,
            "…and there really is a third row: {reply}"
        );
    }

    /// **[`Live`] and [`Halting`] agree on the one bit they share, and on nothing else.**
    ///
    /// `breakpoints_live` is `any_enabled()` — the predicate `Engine::run_sinks` attaches the halting sink
    /// on — and [`Halting::can_halt`] must be the same fact or one of the two surfaces is lying about
    /// whether the machine can be stopped. What they do *not* share is asserted too: `Live` cannot
    /// distinguish a stopped machine from a running one, which is why this parcel did not reuse it.
    #[test]
    fn breakpoints_live_agrees_with_halting_on_armed_and_answers_a_different_question_otherwise() {
        let (mut machine, mut bus) = rig();
        let empty = halting_now(&machine, &bus);
        assert_eq!(breakpoints_live(bus.read_breakpoints()), Live::Never);
        assert!(!empty.can_halt());

        let a = bus.call(
            machine.system_mut(),
            BREAKPOINT_ADD,
            &breakpoint_add_params(HOT, "").expect("a hex target"),
        );
        let handle = ok(&a)["breakpoint"].as_str().expect("a handle").to_owned();
        assert_eq!(breakpoints_live(bus.read_breakpoints()), Live::Yes);
        assert!(halting_now(&machine, &bus).can_halt());

        let a = bus.call(
            machine.system_mut(),
            BREAKPOINT_SET_ENABLED,
            &breakpoint_enable_params(&handle, false),
        );
        assert!(!a.is_err(), "{}", ok_or_err(&a));
        assert_eq!(breakpoints_live(bus.read_breakpoints()), Live::Retained);
        assert!(!halting_now(&machine, &bus).can_halt());

        // ⚑ The third assertion, and the argument for a second type. `Live` is identical for a running
        // machine and a halted one — it is a statement about the TABLE — so the two states below share a
        // `Live` and must not share a headline.
        let running = Halting {
            stopped: false,
            ..halting_now(&machine, &bus)
        };
        let stopped = Halting {
            stopped: true,
            frames_since: Some(0),
            last: Some(oracle_aether::engine::LastBreak {
                id: oracle_aether::breakpoints::BreakpointId(1),
                pc: 0x20E,
                frame: 0,
                ordinal: 9,
            }),
            ..halting_now(&machine, &bus)
        };
        assert_ne!(
            running.headline(),
            stopped.headline(),
            "a running window and a halted one produced the same sentence, which is exactly what `Live` \
             would have done and the reason this is a separate derivation"
        );
        assert!(
            stopped
                .headline()
                .expect("halted says so")
                .contains("9 halts"),
            "the halt count must reach the glass"
        );
    }

    /// **Watches are counted honestly and are NOT reported as things that freeze this window.**
    ///
    /// `Engine::run_sinks` lends the watch wrapped in `Observe`, so a `stopAfter` cannot end one of the
    /// player's 1-frame runs. It *does* end a commanded run, which is a real surprise and gets its own
    /// clause — but a watch alone must never raise the halting alarm, because the reader would then go
    /// looking at the wrong instrument, which is what happened.
    #[test]
    fn a_stopafter_watch_is_named_as_a_commanded_run_hazard_and_never_as_a_frozen_window() {
        let (mut machine, mut bus) = rig();
        let params = watch_add_params("0xFF0000", "2", "bus", false, true, "1", "trap")
            .expect("valid panel input");
        let a = bus.call(machine.system_mut(), WATCHPOINT_ADD, &params);
        assert!(!a.is_err(), "{}", ok_or_err(&a));

        let h = halting_now(&machine, &bus);
        assert_eq!(h.stopping_watches, 1, "the watch carries stopAfter");
        assert!(
            !h.can_halt(),
            "a watch cannot halt the player's own frame run — `Observe` drops its stop"
        );
        assert_eq!(
            h.headline(),
            None,
            "a watch alone must NOT raise the halting alarm: it sends a reader hunting an instrument \
             that is not stopping anything"
        );

        // …but once a breakpoint is armed, the watch's own hazard is spelled out beside it.
        bus.call(
            machine.system_mut(),
            BREAKPOINT_ADD,
            &breakpoint_add_params(HOT, "").expect("a hex target"),
        );
        let h = halting_now(&machine, &bus);
        let advice = h.advice().expect("armed, so there is advice");
        assert!(
            advice.contains("stopAfter") && advice.contains("step"),
            "the watch's real hazard — it ends a COMMANDED run — must be named: {advice}"
        );
    }

    /// **A clipped list of armed addresses says it is clipped**, and the way out is built from the
    /// complete one.
    ///
    /// The display cap and the gesture list are different fields on purpose: a `release` that disarmed
    /// only the four breakpoints the bar had room to name would leave the window halting on the fifth,
    /// which is the same silent-partial-success this module refuses everywhere else.
    #[test]
    fn the_named_addresses_are_capped_for_display_and_the_way_out_is_not() {
        let (mut machine, mut bus) = rig();
        let n = NAMED_ADDRS + 2;
        for i in 0..n {
            let a = bus.call(
                machine.system_mut(),
                BREAKPOINT_ADD,
                &breakpoint_add_params(&format!("0x{:X}", 0x200 + i * 2), "")
                    .expect("a hex target"),
            );
            assert!(!a.is_err(), "{}", ok_or_err(&a));
        }
        let h = halting_now(&machine, &bus);
        assert_eq!(h.armed, n);
        assert_eq!(h.armed_at.len(), NAMED_ADDRS);
        assert_eq!(h.more_addrs, n - NAMED_ADDRS);
        assert_eq!(
            h.armed_handles.len(),
            n,
            "the way out must cover every armed handle, not the ones the bar had room for"
        );
        let head = h.headline().expect("armed");
        assert!(
            head.contains(&format!("+{} more", n - NAMED_ADDRS)),
            "a clipped list that does not say it is clipped reads as complete: {head}"
        );
        assert_eq!(
            release_gestures(&h)
                .iter()
                .filter(|(m, _)| *m == BREAKPOINT_SET_ENABLED)
                .count(),
            n
        );
    }

    fn ok_or_err(a: &Answer) -> String {
        match a {
            Answer::Ok(v) => v.to_string(),
            Answer::Err(e) => format!("{} {}", e.code, e.message),
        }
    }
}
