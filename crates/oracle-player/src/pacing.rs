//! # The pacing design: *audio is the clock, the deadline is the governor, the display is a slave.*
//!
//! This module is the whole reason parcel 1 exists. The toolkit spike
//! (`docs/2026-09-02-toolkit-spike.md`) established that egui + egui_dock cost **0.217 ms of a 16.667 ms
//! frame** — about 1 % — so drawing was never the risk. What the spike also did, twice, was run the same
//! binary at **92.87 fps** and at **22.71 fps**. Those two runs are the same fault pointing in opposite
//! directions, and this module is the fix.
//!
//! ## What actually went wrong in the spike
//!
//! The spike's `eframe` loop called `ctx.request_repaint()` unconditionally and had **no rate limit of its
//! own**. Its only feedback was `audio::frames_to_run`, which answers 0, 1 or 2 emulated frames per
//! iteration from ring occupancy. That is a *trim*, not a *governor*: it can correct the ~0.62 %/s drift
//! between a nominal-60 Hz host loop and a real 44 100 Hz device, and it cannot correct "the loop is
//! iterating at 93 Hz" or "the loop is iterating at 23 Hz". Concretely:
//!
//! * **Too fast (5.8 M producer drops, 93 fps).** With nothing limiting the iteration rate, the loop spins
//!   as fast as the backend returns. `frames_to_run` answers 0 while the ring is near full, but only
//!   [`audio::MAX_CONSECUTIVE_SKIPS`] times in a row — a deliberate safety valve so a wedged audio device
//!   cannot freeze the game — after which it runs a frame regardless and [`audio::push_frame`] discards
//!   what will not fit. Back-pressure with a bounded skip run cannot hold back an unbounded producer.
//! * **Too slow (4 122 starvations, 23 fps).** The render path stalled 26–40 ms per present under
//!   llvmpipe. `frames_to_run` compensated correctly on the *emulation* side (the spike measured 59.69
//!   emulated fps from 4 141 iterations), but a ring whose low-water mark is **one** frame has only ~17 ms
//!   of margin, and a 40 ms stall drains it before the loop gets another turn.
//!
//! The minifb player never shows either failure, and the reason is instructive: `minifb`'s
//! `set_target_fps(60)` limiter is a **coarse governor** that the audio ring then **finely trims**. Two
//! layers. `eframe` has no equivalent turned on by default, so the spike shipped one layer and got a
//! one-layer result. **The toolkit did not remove the fix; it removed the thing that was doing half of it.**
//!
//! ## The design
//!
//! Three layers, in order of authority:
//!
//! 1. **Governor (coarse, this module's [`Governor`]).** A monotonic 60.00 Hz deadline. The loop asks egui
//!    to repaint at the next deadline and refuses to emulate when it is woken early. This bounds the
//!    iteration rate *from above* no matter what the display does, and it is display-independent by
//!    construction — which is why the measurement this parcel owes can be taken without a real GPU.
//! 2. **Clock (fine, [`frames_to_run`] below, delegating to the player's own policy).** The audio device
//!    remains the master clock. Ring occupancy decides 0, 1 or 2 emulated frames per iteration. Nothing
//!    about that is changed: a host's "60 Hz" is never the device's 44 100/735, and only the consumer
//!    knows the truth.
//! 3. **Display (slave).** Whatever the compositor does. If present blocks — vsync on a 60 Hz panel — the
//!    governor's wait is simply already satisfied and it costs nothing. If present blocks *longer* than a
//!    period (a 50 Hz panel, a compositor hiccup, a shader recompile), the loop falls behind, the governor
//!    **rebases instead of sprinting**, and layer 2 runs the extra emulated frames to keep audio fed. If
//!    present does not block at all (no vsync, a 144 Hz panel), layer 1 is the only thing standing between
//!    the loop and the spike's run-2 overflow.
//!
//! ### Why the audio device stays the master clock
//!
//! The incumbent design (`crates/oracle-frontend/src/audio.rs`, `frames_to_run`) is right and is adopted
//! deliberately, not inherited. The argument, restated for this loop:
//!
//! * The audio device is the only clock in the system that **cannot be made to wait**. A dropped video
//!   frame is invisible at 60 Hz; a starved audio callback is an audible click. Whichever clock is not the
//!   master is the one that absorbs the error, and video is the one that can absorb it silently.
//! * It is the only clock whose true rate is **knowable at runtime**. `sample_rate` is nominal; the actual
//!   crystal is not 44 100.000 Hz and no API reports what it is. Ring occupancy measures it directly.
//! * It is the clock the core already produces against — the synth emits exactly `sample_rate / 60` pairs
//!   per *emulated* frame, so pacing on anything else creates a permanent one-directional deficit. That
//!   deficit is measured in `audio.rs`: 0.62 %/s, which pins the ring at empty and silence-fills 8–16 % of
//!   callbacks. A bigger ring does not fix a deficit.
//!
//! The alternative — vsync as master, emulate one frame per present — was considered and rejected: it is
//! only correct when the panel is exactly 60 Hz, it makes the emulator's speed a property of the user's
//! monitor, and it is precisely what produced the 92.87 fps run.
//!
//! ### The one departure: the low-water dial
//!
//! [`RENDER_LOW_WATER_FRAMES`] is **2**, against the minifb player's [`audio::LOW_WATER_FRAMES`] of 1.
//! That is the change `audio.rs` itself names for this case:
//!
//! > A machine that stalls its render loop for tens of milliseconds at a time wants 2 or 3 here; nothing
//! > else has to change. — `audio::LOW_WATER_FRAMES`
//!
//! A toolkit present *can* stall for tens of milliseconds (the spike measured 26–40 ms under llvmpipe, and
//! a resize or a shader recompile does it on real hardware too). Two frames costs ~17 ms of added audio
//! latency and buys ~32 ms of margin, and `audio.rs`'s own table says 1, 2 and 3 all give zero underruns
//! in the steady state — so the latency is the only thing being spent.
//!
//! It is implemented as a *parameter* here rather than as an edit to `audio.rs`, because changing that
//! constant would change the minifb player's behaviour and this parcel does not touch the minifb player.
//! [`frames_to_run`] with `low_water == audio::LOW_WATER_FRAMES` is proven identical to
//! `audio::frames_to_run` over a swept grid in the tests below, so the two policies cannot drift apart
//! without a red test.
//!
//! ### What this design does NOT do, and when that will matter
//!
//! Emulation, UI layout and present all run on **one thread**. That is a real limit: a panel expensive
//! enough to stall the UI thread stalls the emulator with it, and no ring depth fixes an emulator that has
//! stopped. It is chosen for parcel 1 on the numbers — emulation is 2.76 ms of a 16.67 ms budget and the
//! whole toolkit is 0.22 ms, so there is 6× headroom and the stall risk is in *present*, not in compute,
//! and present-stall is exactly what a deeper ring absorbs. The boundary is drawn so a later parcel can
//! move it: [`Machine::step`](crate::machine::Machine::step) is a self-contained unit that takes a pad and
//! returns a picture, with no toolkit types in its signature. **If a debug panel is ever measured to stall
//! the UI thread, the fix is to put `Machine` behind a frame channel on its own thread — not to raise the
//! low-water mark again.**

use std::time::{Duration, Instant};

use crate::audio;

/// The nominal NTSC video period, 60.0 Hz. The emulator's *true* rate is set by the audio ring (layer 2);
/// this is only the governor's target, and being slightly wrong here is harmless by design — that is the
/// point of having a trim.
pub const FRAME_PERIOD: Duration = Duration::from_nanos(16_666_667);

/// Ring occupancy, in video frames, below which an extra emulated frame is run — the render-path value.
///
/// **2, against the minifb player's 1.** See this module's docs, "The one departure".
pub const RENDER_LOW_WATER_FRAMES: usize = 2;

/// How early a repaint may arrive and still be treated as this frame's repaint.
///
/// `request_repaint_after` is a *no later than*, not an *exactly at*: an input event, a window expose or
/// the backend's own bookkeeping can wake the loop before the deadline. Without a tolerance the governor
/// would refuse a repaint that missed by a microsecond and hand back a zero-length wait, busy-spinning.
/// One millisecond is above every timer's granularity here and far below a frame.
pub const EARLY_TOLERANCE: Duration = Duration::from_millis(1);

/// What the governor says about this iteration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tick {
    /// Whether this iteration is the one that owns the frame. `false` = woken early; re-present the
    /// retained picture, emulate nothing, and wait out [`Tick::wait`].
    pub run: bool,
    /// How long to ask the toolkit to wait before the next repaint. Zero means "immediately" — the loop is
    /// behind and should not sleep.
    pub wait: Duration,
    /// How far past its deadline this iteration started. Zero when on time or early.
    pub late_by: Duration,
    /// `true` when the deadline had fallen so far behind that it was moved to `now + period` rather than
    /// advanced by one period. See [`Governor::tick`].
    pub rebased: bool,
}

/// The coarse rate limiter: a monotonic deadline that bounds the loop's iteration rate from above.
///
/// It is deliberately **not** a catch-up scheduler. See [`Governor::tick`].
#[derive(Debug)]
pub struct Governor {
    /// `None` = **the governor is off**: every repaint owns a frame and nothing ever waits. That is the
    /// spike's arrangement, and it exists here only so the bench can measure the design against its own
    /// absence — see [`Governor::unpaced`]. Nothing in the player ever constructs it.
    period: Option<Duration>,
    /// When the next frame is due.
    next: Instant,
    /// Iterations that started late enough to force a rebase (see [`Governor::tick`]).
    rebases: u64,
    /// Iterations that were woken before their deadline and therefore emulated nothing.
    early_wakes: u64,
    /// The worst lateness ever observed at the top of an iteration.
    worst_late: Duration,
}

impl Governor {
    /// Start a governor whose first frame is due immediately.
    pub fn start(now: Instant, period: Duration) -> Self {
        Self {
            period: Some(period),
            next: now,
            rebases: 0,
            early_wakes: 0,
            worst_late: Duration::ZERO,
        }
    }

    /// **The control, not a mode of the player.** A governor with layer 1 removed: every repaint owns a
    /// frame, nothing ever waits, and the audio ring's trim is the only pacing left. That is exactly the
    /// arrangement the toolkit spike measured at 92.87 fps and 22.71 fps.
    ///
    /// It exists so the bench can measure the design against its own absence rather than only argue for
    /// it. An absence has to have a control, or the green run witnesses nothing. `--target-fps 0` selects
    /// it, and the report labels the run GOVERNOR OFF so a number from it can never be mistaken for the
    /// player's.
    pub fn unpaced(now: Instant) -> Self {
        Self {
            period: None,
            next: now,
            rebases: 0,
            early_wakes: 0,
            worst_late: Duration::ZERO,
        }
    }

    /// Whether layer 1 is switched on. False only in the control.
    pub fn is_paced(&self) -> bool {
        self.period.is_some()
    }

    /// The deadline this governor is actually holding, or `None` in the control. The report prints *this*
    /// rather than [`FRAME_PERIOD`], so a run made with `--target-fps` cannot silently be compared against
    /// a target it was never given.
    pub fn period(&self) -> Option<Duration> {
        self.period
    }

    /// Decide what this iteration does.
    ///
    /// **The rule that matters is "rebase, never sprint".** When an iteration starts more than a period
    /// late — a stalled present, a scheduler preemption, a resize — the naive fix is to advance the
    /// deadline by one period and let the backlog work itself off. That converts one stall into a *burst*
    /// of unpaced iterations, and a burst is exactly what overflowed the ring in the spike's run 2. So the
    /// deadline never trails `now`: a frame that is late costs a zero-length wait (one immediate
    /// iteration), and the *audio ring* — not the governor — decides whether the lost emulated frames are
    /// made up. That is the correct division of labour, because only the ring knows whether they need to
    /// be.
    pub fn tick(&mut self, now: Instant) -> Tick {
        let Some(period) = self.period else {
            // The control: no deadline, so nothing is early, nothing is late, and nothing waits.
            return Tick {
                run: true,
                wait: Duration::ZERO,
                late_by: Duration::ZERO,
                rebased: false,
            };
        };
        if self.next > now + EARLY_TOLERANCE {
            self.early_wakes += 1;
            return Tick {
                run: false,
                wait: self.next - now,
                late_by: Duration::ZERO,
                rebased: false,
            };
        }

        let late_by = now.saturating_duration_since(self.next);
        self.worst_late = self.worst_late.max(late_by);

        self.next += period;
        let rebased = self.next <= now;
        if rebased {
            self.rebases += 1;
            self.next = now + period;
        }

        Tick {
            run: true,
            wait: self.next.saturating_duration_since(now),
            late_by,
            rebased,
        }
    }

    /// Iterations that had to move the deadline forward rather than advance it — i.e. stalls of a whole
    /// frame or more. Reported by the bench; a healthy run has none.
    pub fn rebases(&self) -> u64 {
        self.rebases
    }

    /// Repaints that arrived before their deadline and were turned away.
    pub fn early_wakes(&self) -> u64 {
        self.early_wakes
    }

    /// The worst lateness observed at the top of an iteration.
    pub fn worst_late(&self) -> Duration {
        self.worst_late
    }
}

/// How many emulated frames this iteration should run, from ring occupancy — the player's own policy, with
/// the low-water mark lifted out as a parameter.
///
/// `low_water == audio::LOW_WATER_FRAMES` reproduces `audio::frames_to_run` exactly; the test
/// `low_water_of_one_is_the_players_policy` sweeps a grid to hold that true. Everything else about the
/// policy — the two-frame burst, the high-water skip, the [`audio::MAX_CONSECUTIVE_SKIPS`] safety valve,
/// the too-small-ring escape — is the player's and is not restated here.
pub fn frames_to_run(
    occupied: usize,
    capacity: usize,
    frame_samples: usize,
    skips: usize,
    low_water: usize,
) -> usize {
    // A ring too small to hold a low band and a high band cannot be steered — run at the nominal rate and
    // let `push_frame` drop the surplus. Same escape as the player's, widened to the parameterised mark.
    if frame_samples == 0 || capacity < (low_water + 2) * frame_samples {
        return 1;
    }
    if occupied < low_water * frame_samples {
        return audio::MAX_FRAMES_PER_ITER;
    }
    if occupied > capacity - frame_samples && skips < audio::MAX_CONSECUTIVE_SKIPS {
        return 0;
    }
    1
}

/// [`frames_to_run`] against a live ring producer, at [`RENDER_LOW_WATER_FRAMES`].
pub fn frames_to_run_for(prod: &audio::AudioProd, frame_samples: usize, skips: usize) -> usize {
    use ringbuf::traits::Observer;
    frames_to_run(
        prod.occupied_len(),
        audio::ring_capacity(prod),
        frame_samples,
        skips,
        RENDER_LOW_WATER_FRAMES,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the governor -------------------------------------------------------------------------------

    /// The plain case: on-time iterations advance the deadline by exactly one period, so the loop's
    /// long-run rate is the period and does not drift with per-iteration work.
    #[test]
    fn on_time_iterations_hold_the_period() {
        let t0 = Instant::now();
        let mut g = Governor::start(t0, FRAME_PERIOD);
        let mut at = t0;
        for i in 0..100 {
            let t = g.tick(at);
            assert!(t.run, "iteration {i} should own its frame");
            assert!(!t.rebased, "iteration {i} rebased with no stall");
            at += t.wait; // the loop waits exactly as told
        }
        assert_eq!(g.rebases(), 0);
        // 100 periods of drift-free scheduling, to the nanosecond.
        assert_eq!(at - t0, FRAME_PERIOD * 100);
    }

    /// **The run-2 fix.** A stall must not be followed by a burst of zero-wait iterations working off a
    /// debt; it must cost exactly one immediate iteration and then resume the period.
    #[test]
    fn a_stall_rebases_and_does_not_sprint() {
        let t0 = Instant::now();
        let mut g = Governor::start(t0, FRAME_PERIOD);
        assert!(g.tick(t0).run);

        // A 100 ms present stall — six periods.
        let stalled = t0 + Duration::from_millis(100);
        let t = g.tick(stalled);
        assert!(t.run);
        assert!(t.rebased, "a six-period stall must rebase");
        // Exactly: the stall, minus the one period the first tick had already scheduled. (`FRAME_PERIOD *
        // 5` is 2 ns above this, which is how precise the accounting is.)
        assert_eq!(t.late_by, Duration::from_millis(100) - FRAME_PERIOD);
        assert_eq!(
            t.wait, FRAME_PERIOD,
            "after a rebase the next frame is a full period away, NOT immediately — a zero wait here \
             is the burst that overflowed the ring in the spike's run 2"
        );

        // And exactly one rebase was charged, not one per lost period.
        assert_eq!(g.rebases(), 1);
        assert_eq!(g.worst_late(), t.late_by);
    }

    /// A single period of lateness is absorbed without a rebase: the deadline moves by one period and the
    /// wait is zero, so the loop takes one immediate iteration to regain phase.
    #[test]
    fn one_period_late_costs_one_immediate_iteration() {
        let t0 = Instant::now();
        let mut g = Governor::start(t0, FRAME_PERIOD);
        assert!(g.tick(t0).run);

        let late = t0 + FRAME_PERIOD + Duration::from_micros(500);
        let t = g.tick(late);
        assert!(t.run);
        assert!(!t.rebased, "half a period over is not a stall");
        assert_eq!(t.wait, FRAME_PERIOD - Duration::from_micros(500));
    }

    /// **The busy-spin guard.** A repaint that arrives early emulates nothing and is handed the remaining
    /// wait, so an input-driven wake cannot advance the emulator off-cadence.
    #[test]
    fn an_early_wake_does_not_run_a_frame() {
        let t0 = Instant::now();
        let mut g = Governor::start(t0, FRAME_PERIOD);
        assert!(g.tick(t0).run);

        let early = t0 + Duration::from_millis(4);
        let t = g.tick(early);
        assert!(
            !t.run,
            "a repaint 12 ms before the deadline must not emulate"
        );
        assert_eq!(t.wait, FRAME_PERIOD - Duration::from_millis(4));
        assert_eq!(g.early_wakes(), 1);

        // ...and the frame it turned away is still owed, at the original deadline.
        let on_time = t0 + FRAME_PERIOD;
        assert!(g.tick(on_time).run);
    }

    /// Within [`EARLY_TOLERANCE`] the repaint counts as this frame's, rather than being turned away for a
    /// timer's rounding.
    #[test]
    fn a_repaint_inside_the_tolerance_counts_as_on_time() {
        let t0 = Instant::now();
        let mut g = Governor::start(t0, FRAME_PERIOD);
        assert!(g.tick(t0).run);
        let nearly = t0 + FRAME_PERIOD - Duration::from_micros(900);
        assert!(g.tick(nearly).run, "900 us early is inside the tolerance");
        assert_eq!(g.early_wakes(), 0);
    }

    /// **The control's own contract.** With layer 1 off, every repaint owns a frame and nothing waits —
    /// the spike's arrangement exactly. If this ever started waiting, the "governor off" bench run would
    /// quietly be measuring a governor, and its numbers would witness nothing.
    #[test]
    fn the_unpaced_control_never_waits_and_never_turns_a_repaint_away() {
        let t0 = Instant::now();
        let mut g = Governor::unpaced(t0);
        assert!(!g.is_paced());
        let mut at = t0;
        for i in 0..1000 {
            let t = g.tick(at);
            assert!(t.run, "iteration {i}: the control must run every repaint");
            assert_eq!(
                t.wait,
                Duration::ZERO,
                "iteration {i}: the control must never wait"
            );
            assert!(!t.rebased);
            // Free-running: the caller comes straight back.
            at += Duration::from_micros(200);
        }
        assert_eq!(g.rebases(), 0);
        assert_eq!(g.early_wakes(), 0);
        assert_eq!(g.worst_late(), Duration::ZERO);
        // ...and a paced governor over the same 1000 free-running repaints turns nearly all of them away,
        // which is the difference the bench is there to measure.
        let mut p = Governor::start(t0, FRAME_PERIOD);
        let mut at = t0;
        let mut ran = 0;
        for _ in 0..1000 {
            if p.tick(at).run {
                ran += 1;
            }
            at += Duration::from_micros(200);
        }
        assert!(
            ran <= 14,
            "a paced governor let {ran} of 1000 free-running repaints through"
        );
        assert!(p.early_wakes() >= 985);
    }

    // ---- the frame policy ---------------------------------------------------------------------------

    /// The equivalence that stops the two policies drifting: at the player's own low-water mark this
    /// function *is* the player's function, over a swept grid of every branch.
    #[test]
    fn low_water_of_one_is_the_players_policy() {
        let f = audio::frame_samples(44_100); // 1470
        let mut checked = 0usize;
        for ring_frames in [0usize, 1, 2, 3, 4, 8, 16] {
            let capacity = ring_frames * f;
            // Sweep occupancy across every band boundary, plus the ends.
            for occ_frac in 0..=(ring_frames * 4).max(1) {
                let occupied = (occ_frac * f / 4).min(capacity);
                for skips in 0..=audio::MAX_CONSECUTIVE_SKIPS + 1 {
                    let ours = frames_to_run(occupied, capacity, f, skips, audio::LOW_WATER_FRAMES);
                    let theirs = audio::frames_to_run(occupied, capacity, f, skips);
                    assert_eq!(
                        ours, theirs,
                        "diverged at occupied={occupied} capacity={capacity} skips={skips}"
                    );
                    checked += 1;
                }
            }
        }
        // A grid that silently swept nothing would pass vacuously.
        assert!(checked > 200, "grid only covered {checked} cells");
        // And a zero-frame-samples ring, which the loop hits before the device is open.
        assert_eq!(frames_to_run(0, 0, 0, 0, audio::LOW_WATER_FRAMES), 1);
        assert_eq!(audio::frames_to_run(0, 0, 0, 0), 1);
    }

    /// The departure itself: in the band between one and two frames of audio, the render path runs an
    /// extra frame where the minifb player would not. This is the ~32 ms of stall margin being bought.
    #[test]
    fn the_render_low_water_is_deeper_than_the_players() {
        let f = audio::frame_samples(44_100);
        let capacity = audio::RING_FRAMES * f;
        // Exactly one and a half frames in the ring.
        let occupied = f + f / 2;

        assert_eq!(
            audio::frames_to_run(occupied, capacity, f, 0),
            1,
            "the minifb player is content at 1.5 frames"
        );
        assert_eq!(
            frames_to_run(occupied, capacity, f, 0, RENDER_LOW_WATER_FRAMES),
            audio::MAX_FRAMES_PER_ITER,
            "the render path refills at 1.5 frames, because its present can stall"
        );
        const {
            assert!(
                RENDER_LOW_WATER_FRAMES > audio::LOW_WATER_FRAMES,
                "this test is only meaningful while the render mark is the deeper one"
            )
        };
        // Above the render mark the two agree again.
        let deep = 3 * f;
        assert_eq!(
            frames_to_run(deep, capacity, f, 0, RENDER_LOW_WATER_FRAMES),
            audio::frames_to_run(deep, capacity, f, 0)
        );
    }

    /// The safety valve survives the deeper mark: a device that stops consuming pins the ring full, and
    /// the loop must give up skipping rather than freeze the game.
    #[test]
    fn a_wedged_device_still_cannot_freeze_the_game() {
        let f = audio::frame_samples(44_100);
        let capacity = audio::RING_FRAMES * f;
        let full = capacity;
        for skips in 0..audio::MAX_CONSECUTIVE_SKIPS {
            assert_eq!(
                frames_to_run(full, capacity, f, skips, RENDER_LOW_WATER_FRAMES),
                0,
                "skip {skips} should still hold the emulator back"
            );
        }
        assert_eq!(
            frames_to_run(
                full,
                capacity,
                f,
                audio::MAX_CONSECUTIVE_SKIPS,
                RENDER_LOW_WATER_FRAMES
            ),
            1,
            "after MAX_CONSECUTIVE_SKIPS the loop runs anyway"
        );
    }

    /// A ring too small for the deeper mark's bands falls back to the nominal rate rather than oscillating
    /// between the two-frame burst and the skip.
    #[test]
    fn a_ring_too_small_for_the_deeper_mark_runs_nominally() {
        let f = audio::frame_samples(44_100);
        // (RENDER_LOW_WATER_FRAMES + 2) = 4 frames needed; give it 3.
        let capacity = 3 * f;
        for occ in [0, f, 2 * f, 3 * f] {
            assert_eq!(
                frames_to_run(occ, capacity, f, 0, RENDER_LOW_WATER_FRAMES),
                1,
                "occ={occ}"
            );
        }
        // The player's shallower mark can steer that same ring, which is why the escape is parameterised.
        assert_eq!(
            audio::frames_to_run(0, capacity, f, 0),
            audio::MAX_FRAMES_PER_ITER
        );
        // ...and the real ring is big enough for both.
        const {
            assert!(
                audio::RING_FRAMES >= RENDER_LOW_WATER_FRAMES + 2,
                "the real ring must be big enough for the deeper mark's bands"
            )
        };
    }
}
