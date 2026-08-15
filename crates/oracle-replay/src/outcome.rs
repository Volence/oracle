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
}

/// What the runner should do after a frame chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Keep running.
    Continue,
    /// The machine stopped at `ErrorHandlerBlob` — decode the fault.
    Trapped,
    /// `Replay_Done == $FF`.
    Passed,
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
pub fn dispose(trapped: bool, probe: &Probe, stalled: bool, deadline: bool) -> Disposition {
    if trapped {
        return Disposition::Trapped;
    }
    if probe.done() {
        return Disposition::Passed;
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
        let p = probe(2, 0, 1, 0x100);
        assert_eq!(dispose(true, &p, true, true), Disposition::Trapped);
        assert_eq!(dispose(true, &p, false, false), Disposition::Trapped);
    }

    /// …and completion outranks the watchdog too: the last frame of a green run naturally stops ticking.
    #[test]
    fn completion_outranks_a_stall() {
        let p = probe(1721, 0xFF, 0, 0x200);
        assert_eq!(dispose(false, &p, true, true), Disposition::Passed);
    }

    #[test]
    fn a_stall_outranks_the_absolute_cap_because_it_is_more_specific() {
        let p = probe(2, 0, 1, 0x100);
        assert_eq!(dispose(false, &p, true, true), Disposition::Stalled);
        assert_eq!(dispose(false, &p, false, true), Disposition::Deadline);
        assert_eq!(dispose(false, &p, false, false), Disposition::Continue);
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
        let base = 0x000A_1D80;
        assert_eq!(probe(0, 0, 1, base + 4).stream_offset(base), 4);
        assert_eq!(probe(0, 0, 1, 0).stream_offset(base), -(base as i64));
        assert_eq!(probe(0, 0, 1, base + 26).stream_offset(base), 26);
    }
}
