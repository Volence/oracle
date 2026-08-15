//! Classifying a run: what the machine's observable state means, and in what order to believe it.
//!
//! # The one ordering that matters
//!
//! **A trap must be checked before a timeout is ever reported.** A desync presents *exactly* as a hang —
//! the machine is still running, `Logic_Tick` is frozen — so a timeout report that has not first asked
//! `PC == ErrorHandlerBlob` is the single most likely wrong answer this tool can produce
//! (`aeon/docs/superpowers/plans/2026-08-13-replay-net-restamp.md:98`). [`dispose`] encodes that
//! precedence in one pure function so it cannot be re-derived differently at some future call site.
//!
//! # A PASS is never one byte
//!
//! `Replay_Done == $FF` is the *completion* signal, and on its own it says only "the playback path reached
//! an end-of-stream opcode". It does not say **which** end: a truncated or mis-packed stream
//! (`FF 01 <hash> FF 00 …`) passes every header check, compares the ring-0 checkpoint, hits `REPLAY_OP_END`
//! at tick 2 and sets the same `$FF` — having verified 1 of 27 checkpoints. The negative control does not
//! catch that, because it corrupts checkpoint 0, which such a stream *does* compare.
//!
//! So completion is qualified by three corroborations that were already being read and thrown away
//! ([`Probe::shortfalls`]):
//!
//! 1. **`Logic_Tick >= tick_count`.** Safe by construction: playback consumes exactly one tick per
//!    `GameLoop` iteration and the arm lands at `Logic_Tick 1`, so genuine completion is `tick_count + 1`,
//!    and it overshoots further afterwards because the game keeps running on live input.
//! 2. **`Input_Source` self-cleared**, which `replay.emp:204-205` does on the same two instructions that set
//!    `Replay_Done`. A set flag with an uncleared source is not the completion path.
//! 3. **The cursor left the header.** `Replay_Ptr` still inside the 20-byte header is the signature of a bad
//!    arm (design §2.7), and it is the one cell that was previously consulted on the timeout path only.
//!
//! Anything that sets `Replay_Done` without all three is [`Disposition::ShortCompletion`] — a loud failure
//! that names what was short — never a PASS.
//!
//! # The watchdog is a progress check, not a fixed cap
//!
//! `Logic_Tick` is not the frame clock. It increments once per `GameLoop` iteration, *after* `VSync_Wait`
//! (`aeon/engine/system/game_loop.emp:29-30`, explicitly "lag-immune, unlike `Frame_Counter`"), so
//! `ticks ≤ frames` and never the reverse — and the boot phase burns frames with **zero** ticks, because
//! `Level_LoadArt` spins `VSync_Wait` inside a single dispatch (`aeon/engine/level/load_art.emp:124`).
//!
//! So the primary bound is "`Logic_Tick` has not advanced for N frames while armed and not done". That
//! distinguishes wedged from slow, and catches an arm failure at frame ~10 rather than at the end of a
//! budget. A generous absolute frame cap backstops it. Wall-clock is never a budget under any
//! circumstances: recorded sibling runs varied between ~30 fps and ~0.9 fps purely with competing desktop
//! load.

use crate::header::REPLAY_HEADER_LEN;
use std::fmt;

/// `Replay_Done`'s set value. The 68000 `st` instruction writes `$FF`, not 1 (`replay.emp:204`) — reading
/// this as a boolean `1` is a recorded way to never see completion.
pub const REPLAY_DONE: u8 = 0xFF;

/// The work-RAM cells the runner polls between frame chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    /// `Logic_Tick` (u32) — the progress signal.
    pub logic_tick: u32,
    /// `Replay_Done` (u8) — `$FF` when the stream ran to its end.
    pub replay_done: u8,
    /// `Input_Source` (u8) — 1 while playback is armed, self-cleared to 0 on completion.
    pub input_source: u8,
    /// `Replay_Ptr` (u32) — the stream cursor. Still inside the header is the signature of a bad arm.
    pub replay_ptr: u32,
    /// `Replay_Hold` (u8) — remaining ticks of the current RLE run. `None` when the listing does not
    /// declare the symbol.
    pub replay_hold: Option<u8>,
}

impl Probe {
    /// Whether the stream reported completion.
    pub fn done(&self) -> bool {
        self.replay_done == REPLAY_DONE
    }

    /// The independent corroboration of [`done`](Self::done): the playback path clears `Input_Source` on
    /// the same code path that sets `Replay_Done`.
    pub fn input_source_cleared(&self) -> bool {
        self.input_source == 0
    }

    /// How far into the stream the cursor has advanced from the fixture symbol. Values below
    /// [`REPLAY_HEADER_LEN`](crate::header::REPLAY_HEADER_LEN) mean the cursor never left the header.
    pub fn stream_offset(&self, fixture_base: u32) -> i64 {
        self.replay_ptr as i64 - fixture_base as i64
    }

    /// **The bad-arm signature**: the cursor never left the 20-byte header, so the playback path has been
    /// reading a pointer we never successfully poked (design §2.7). Extracted here rather than left inline
    /// in a `println!` so it is one predicate with one test, and so both the timeout report and
    /// [`shortfalls`](Self::shortfalls) ask the same question.
    pub fn stuck_in_header(&self, fixture_base: u32) -> bool {
        self.stream_offset(fixture_base) < i64::from(REPLAY_HEADER_LEN)
    }

    /// Everything that is wrong with a `Replay_Done == $FF` that should not be believed. Empty means the
    /// completion is corroborated three independent ways and a PASS is honest.
    ///
    /// Order is deliberate: the tick shortfall is the one that catches a truncated stream, and it is named
    /// first because it is the finding a reader most needs.
    pub fn shortfalls(&self, expected: &Expected) -> Vec<Shortfall> {
        let mut out = Vec::new();
        if self.logic_tick < expected.tick_count {
            out.push(Shortfall::TicksShort {
                logic_tick: self.logic_tick,
                required: expected.tick_count,
            });
        }
        if !self.input_source_cleared() {
            out.push(Shortfall::InputSourceNotCleared {
                input_source: self.input_source,
            });
        }
        if self.stuck_in_header(expected.fixture_base) {
            out.push(Shortfall::CursorInHeader {
                offset: self.stream_offset(expected.fixture_base),
            });
        }
        out
    }
}

/// What a completion is measured against: the header's own recorded tick count, and the fixture base the
/// cursor is measured from. Both come from the stream that was armed, so this is the stream judging itself
/// — nothing here is a hardcoded property of any particular fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expected {
    /// The header's `tick_count` (offset 6).
    pub tick_count: u32,
    /// The fixture symbol's address — the `A` of `ARP0`.
    pub fixture_base: u32,
}

/// One reason a `Replay_Done == $FF` is not a PASS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortfall {
    /// The stream declared more ticks than the machine ran. **This is the truncated-stream catch**: a
    /// mis-packed `FF 01 <hash> FF 00` ends at tick 2 having compared one checkpoint out of 27.
    TicksShort { logic_tick: u32, required: u32 },
    /// The completion path clears `Input_Source` on the same two instructions that set `Replay_Done`
    /// (`replay.emp:204-205`), so a set flag with a live source did not come from that path.
    InputSourceNotCleared { input_source: u8 },
    /// The cursor never left the header — the arm did not take, whatever else happened.
    CursorInHeader { offset: i64 },
}

impl fmt::Display for Shortfall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TicksShort {
                logic_tick,
                required,
            } => write!(
                f,
                "Logic_Tick is {logic_tick}, but the stream declares {required} ticks — the playback \
                 reached an end-of-stream opcode {} ticks early, so most of the stream was never \
                 replayed and most of its checkpoints were never compared (a truncated or mis-packed \
                 stream looks exactly like this)",
                required.saturating_sub(*logic_tick)
            ),
            Self::InputSourceNotCleared { input_source } => write!(
                f,
                "Input_Source is ${input_source:02X}, not $00 — the completion path clears it on the \
                 same instruction pair that sets Replay_Done, so this flag was not set by that path"
            ),
            Self::CursorInHeader { offset } => write!(
                f,
                "Replay_Ptr is only fixture+{offset}, still inside the {REPLAY_HEADER_LEN}-byte header \
                 — the arm never took"
            ),
        }
    }
}

/// What the runner should do after a frame chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Keep running.
    Continue,
    /// The machine stopped at `ErrorHandlerBlob` — decode the fault.
    Trapped,
    /// `Replay_Done == $FF`, corroborated by every check in [`Probe::shortfalls`].
    Passed,
    /// `Replay_Done == $FF`, but the run does not stand up: something was short. **A failure**, never a
    /// PASS — the caller names the shortfalls from [`Probe::shortfalls`].
    ShortCompletion,
    /// `Logic_Tick` has been frozen for the whole stall budget.
    Stalled,
    /// The absolute frame cap was reached with neither of the above.
    Deadline,
}

/// The classification, in its load-bearing order: trap, then completion, then the watchdog, then the cap.
///
/// `trapped` must be the *exact-equality* `PC == ErrorHandlerBlob` result. Not a range: other blob entry
/// points (`MDDBG__KDebug_Write` at blob+`$D0E`, `MDDBG__Console_Write` at blob+`$B92`) are called during
/// **normal** operation, so a range predicate would fire on every debug print.
///
/// Completion is *qualified* against `expected` — see the module docs. A `Replay_Done` that does not carry
/// its three corroborations is [`Disposition::ShortCompletion`], not [`Disposition::Passed`]; it still
/// outranks the watchdog and the cap, because the machine really did stop replaying and reporting a
/// timeout would name the wrong thing.
pub fn dispose(
    trapped: bool,
    probe: &Probe,
    expected: &Expected,
    stalled: bool,
    deadline: bool,
) -> Disposition {
    if trapped {
        return Disposition::Trapped;
    }
    if probe.done() {
        return if probe.shortfalls(expected).is_empty() {
            Disposition::Passed
        } else {
            Disposition::ShortCompletion
        };
    }
    if stalled {
        return Disposition::Stalled;
    }
    if deadline {
        return Disposition::Deadline;
    }
    Disposition::Continue
}

/// The progress watchdog: how many consecutive frames `Logic_Tick` has failed to advance.
#[derive(Debug, Clone, Copy)]
pub struct Watchdog {
    budget: u64,
    last_tick: Option<u32>,
    frozen_frames: u64,
}

impl Watchdog {
    /// `budget` = how many consecutive frames without a tick constitutes a wedge.
    pub fn new(budget: u64) -> Self {
        Self {
            budget,
            last_tick: None,
            frozen_frames: 0,
        }
    }

    /// Record one frame's `Logic_Tick`. Returns `true` once the tick has been frozen for the whole budget.
    pub fn observe(&mut self, tick: u32) -> bool {
        match self.last_tick {
            Some(prev) if prev == tick => self.frozen_frames += 1,
            _ => self.frozen_frames = 0,
        }
        self.last_tick = Some(tick);
        self.frozen_frames >= self.budget
    }

    /// Consecutive frames with a frozen tick, for the report.
    pub fn frozen_frames(&self) -> u64 {
        self.frozen_frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(tick: u32, done: u8, src: u8, ptr: u32) -> Probe {
        Probe {
            logic_tick: tick,
            replay_done: done,
            input_source: src,
            replay_ptr: ptr,
            replay_hold: None,
        }
    }

    const BASE: u32 = 0x000A_1D80;

    /// The standing fixture's shape: 1721 ticks, based at `BASE`.
    fn expected() -> Expected {
        Expected {
            tick_count: 1721,
            fixture_base: BASE,
        }
    }

    /// A green run's cells: overshot the tick count, source cleared, cursor deep in the stream.
    fn green() -> Probe {
        probe(2423, 0xFF, 0, BASE + 0x100)
    }

    /// `st` writes `$FF`. A runner that tested `== 1` would poll forever on a green run.
    #[test]
    fn completion_is_ff_not_one() {
        assert!(probe(100, 0xFF, 0, 0).done());
        assert!(!probe(100, 1, 1, 0).done());
        assert!(!probe(100, 0, 1, 0).done());
    }

    /// The load-bearing precedence: a desync looks exactly like a hang, so the trap wins over both the
    /// stall watchdog and the absolute cap.
    #[test]
    fn a_trap_outranks_a_stall_and_a_deadline() {
        let p = probe(2, 0, 1, BASE + 26);
        let e = expected();
        assert_eq!(dispose(true, &p, &e, true, true), Disposition::Trapped);
        assert_eq!(dispose(true, &p, &e, false, false), Disposition::Trapped);
    }

    /// …and completion outranks the watchdog too: the last frame of a green run naturally stops ticking.
    #[test]
    fn completion_outranks_a_stall() {
        assert_eq!(
            dispose(false, &green(), &expected(), true, true),
            Disposition::Passed
        );
    }

    /// **The F1 regression, at the unit level.** A truncated stream (`FF 01 <hash> FF 00 …`) parses, arms,
    /// compares checkpoint 0, hits end-of-stream at tick 2 and sets the very same `Replay_Done = $FF` — with
    /// `Input_Source` cleared and a cursor past the header, because it really did take the completion path.
    /// One unqualified byte compare calls that a PASS. It verified 1 of 27 checkpoints.
    #[test]
    fn a_truncated_stream_that_completes_at_tick_two_is_not_a_pass() {
        let p = probe(2, 0xFF, 0, BASE + 26);
        let e = expected();
        assert!(p.done(), "the byte the old PASS rested on is set");
        assert!(p.input_source_cleared(), "…and so is its corroboration");
        assert!(
            !p.stuck_in_header(BASE),
            "…and the cursor did leave the header"
        );
        assert_eq!(
            dispose(false, &p, &e, false, false),
            Disposition::ShortCompletion
        );
        assert_eq!(
            p.shortfalls(&e),
            vec![Shortfall::TicksShort {
                logic_tick: 2,
                required: 1721
            }]
        );
        assert!(
            p.shortfalls(&e)[0].to_string().contains("1719 ticks early"),
            "the failure must name how much was skipped: {}",
            p.shortfalls(&e)[0]
        );
    }

    /// Every corroboration is load-bearing on its own, and a green run trips none of them.
    #[test]
    fn each_corroboration_can_fail_alone() {
        let e = expected();
        assert_eq!(green().shortfalls(&e), vec![]);
        assert_eq!(
            dispose(false, &green(), &e, false, false),
            Disposition::Passed
        );

        // Exactly the declared count is enough — the arm lands at Logic_Tick 1, so genuine completion is
        // tick_count + 1 and this bound can never fail a healthy run.
        let exact = probe(1721, 0xFF, 0, BASE + 0x100);
        assert_eq!(exact.shortfalls(&e), vec![]);

        let source_live = probe(2423, 0xFF, 1, BASE + 0x100);
        assert_eq!(
            source_live.shortfalls(&e),
            vec![Shortfall::InputSourceNotCleared { input_source: 1 }]
        );
        assert_eq!(
            dispose(false, &source_live, &e, false, false),
            Disposition::ShortCompletion
        );

        let bad_arm = probe(2423, 0xFF, 0, BASE + 4);
        assert_eq!(
            bad_arm.shortfalls(&e),
            vec![Shortfall::CursorInHeader { offset: 4 }]
        );
        assert_eq!(
            dispose(false, &bad_arm, &e, false, false),
            Disposition::ShortCompletion
        );
    }

    /// A short completion still outranks the watchdog and the cap: the machine stopped replaying, so a
    /// TIMEOUT report would name the wrong failure.
    #[test]
    fn a_short_completion_outranks_a_stall_and_a_deadline() {
        let p = probe(2, 0xFF, 0, BASE + 26);
        assert_eq!(
            dispose(false, &p, &expected(), true, true),
            Disposition::ShortCompletion
        );
    }

    #[test]
    fn a_stall_outranks_the_absolute_cap_because_it_is_more_specific() {
        let p = probe(2, 0, 1, BASE + 26);
        let e = expected();
        assert_eq!(dispose(false, &p, &e, true, true), Disposition::Stalled);
        assert_eq!(dispose(false, &p, &e, false, true), Disposition::Deadline);
        assert_eq!(dispose(false, &p, &e, false, false), Disposition::Continue);
    }

    #[test]
    fn the_watchdog_fires_only_after_the_whole_budget_of_frozen_frames() {
        let mut w = Watchdog::new(3);
        assert!(!w.observe(0)); // first sample: nothing to compare against
        assert!(!w.observe(0)); // 1 frozen
        assert!(!w.observe(0)); // 2 frozen
        assert!(w.observe(0)); // 3 frozen -> budget reached
        assert_eq!(w.frozen_frames(), 3);
    }

    /// Any advance resets it — a slow run must never be reported as a wedged one.
    #[test]
    fn progress_resets_the_watchdog() {
        let mut w = Watchdog::new(2);
        assert!(!w.observe(5));
        assert!(!w.observe(5));
        assert!(!w.observe(6));
        assert_eq!(w.frozen_frames(), 0);
        assert!(!w.observe(6));
        assert!(w.observe(6));
    }

    /// A budget of 0 would report a wedge on the very first sample; it is the caller's job not to pass
    /// one, but the behaviour is pinned so it cannot surprise anyone.
    #[test]
    fn a_zero_budget_fires_immediately() {
        assert!(Watchdog::new(0).observe(0));
    }

    /// The bad-arm signature: the cursor never left the header.
    #[test]
    fn the_stream_offset_exposes_a_cursor_stuck_in_the_header() {
        let base = BASE;
        assert_eq!(probe(0, 0, 1, base + 4).stream_offset(base), 4);
        assert_eq!(probe(0, 0, 1, 0).stream_offset(base), -(base as i64));
        assert_eq!(probe(0, 0, 1, base + 26).stream_offset(base), 26);
    }

    /// …as a predicate, because it is asked in two places (the timeout report and the PASS
    /// corroborations) and was previously re-derived inline inside a `println!`.
    #[test]
    fn stuck_in_header_is_a_predicate_with_its_boundary_pinned() {
        let base = BASE;
        assert!(probe(0, 0, 1, base).stuck_in_header(base), "offset 0");
        assert!(probe(0, 0, 1, base + 19).stuck_in_header(base), "offset 19");
        // The first body byte is the boundary: `body = base + REPLAY_HEADER_LEN` is out of the header.
        assert!(
            !probe(0, 0, 1, base + 20).stuck_in_header(base),
            "offset 20"
        );
        assert!(!probe(0, 0, 1, base + 26).stuck_in_header(base));
        // An unpoked cell reading zero is *before* the fixture entirely, which is still a bad arm.
        assert!(probe(0, 0, 1, 0).stuck_in_header(base), "a zero cursor");
    }
}
