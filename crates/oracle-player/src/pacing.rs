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

use oracle_aether::engine::{FrameTimes, PacingAudio, PacingFacts};

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

/// The span [`Presents::facts`] measures `fps` over once the process has been up that long.
///
/// One second, because that is the figure a person means by "frame rate" and the one the owner will
/// quote back. It is **reported on the wire beside the value** (§11.42 M2), so this constant is a
/// default the reader can see rather than a hidden convention — and a reader who wants a different
/// window cannot ask for one: §8 item 22 refuses a client-chosen `windowMs`.
pub const FPS_WINDOW: Duration = Duration::from_secs(1);

/// The span the frame-time percentiles summarise, at most.
///
/// Ten seconds rather than the one second `fps` uses, and the difference is the point: a p99 needs
/// enough samples that the 99th percentile is not simply the maximum. At [`FRAME_PERIOD`] that is
/// [`FRAME_TIME_SAMPLES`] samples, and the sample count is served beside the pair so a reader never has
/// to take this constant on trust.
pub const FRAME_TIME_WINDOW: Duration = Duration::from_secs(10);

/// How many inter-present gaps the percentiles are taken over — **derived** from
/// [`FRAME_TIME_WINDOW`] and [`FRAME_PERIOD`] rather than picked, so the two cannot drift.
///
/// It bounds the ring for the reason `crate::stats::Series` does not bound its own: `Series` is a bench
/// artefact that lives for one measured run, and this one lives for as long as the owner leaves his
/// window open. At 599 `f64`s it is ~4.7 KB, and the sort behind a percentile is a few microseconds of a
/// 16.667 ms frame.
pub const FRAME_TIME_SAMPLES: usize =
    (FRAME_TIME_WINDOW.as_nanos() / FRAME_PERIOD.as_nanos()) as usize;

/// **The pacing meter: presents in, [`PacingFacts`] out.**
///
/// This is the single derivation §11.42 is served from, and the one the Pacing tab draws. See
/// [`PacingFacts`] for why there is exactly one and what would be wrong with two.
///
/// It counts **presents** — pictures this window actually put on the glass — which is neither of the two
/// frame tallies the player already had. [`crate::machine::Machine::frames`] is emulated frames, and
/// [`crate::machine::Machine::pictures`] is *completed pictures*, which its own doc records as running
/// **above** the presented count whenever an iteration emulated two frames and only the second reached
/// the screen. §11.42 M1 asks for the third quantity by name, so the third quantity is counted here
/// rather than one of the first two being relabelled.
#[derive(Debug)]
pub struct Presents {
    /// Every present since [`Presents::start`], never windowed. `presented` on the wire.
    total: u64,
    /// The instants of the presents inside [`FPS_WINDOW`], oldest first.
    window: std::collections::VecDeque<Instant>,
    /// The last [`FRAME_TIME_SAMPLES`] gaps between consecutive presents, in milliseconds, oldest first.
    gaps: std::collections::VecDeque<f64>,
    /// The previous present, for the next gap. `None` before the first one — and one present is zero
    /// gaps, which is `samples: 0` and no percentiles rather than a fabricated pair.
    last: Option<Instant>,
    /// When the meter started, so a window shorter than [`FPS_WINDOW`] is reported as the span it
    /// actually covers instead of a second that has not elapsed.
    started: Instant,
}

impl Presents {
    pub fn start(now: Instant) -> Self {
        Self {
            total: 0,
            window: std::collections::VecDeque::new(),
            gaps: std::collections::VecDeque::new(),
            last: None,
            started: now,
        }
    }

    /// **One picture reached the glass.** Called from the one place that uploads a texture, so the tally
    /// is presents and not repaints: an iteration woken early re-presents the picture already bound and
    /// deliberately does not count (see `Loop::iterate`).
    pub fn note(&mut self, now: Instant) {
        self.total += 1;
        if let Some(prev) = self.last {
            self.gaps
                .push_back((now.saturating_duration_since(prev)).as_secs_f64() * 1000.0);
            while self.gaps.len() > FRAME_TIME_SAMPLES {
                self.gaps.pop_front();
            }
        }
        self.last = Some(now);
        self.window.push_back(now);
        self.evict(now);
    }

    /// Drop presents that have fallen out of [`FPS_WINDOW`].
    fn evict(&mut self, now: Instant) {
        let Some(cut) = now.checked_sub(FPS_WINDOW) else {
            return;
        };
        while self.window.front().is_some_and(|t| *t <= cut) {
            self.window.pop_front();
        }
    }

    /// **The derivation.** Everything `emulator/pacing` serves and everything the Pacing tab draws about
    /// rate, in one value computed once.
    ///
    /// # `fps_value` and `fps_window_ms` are arithmetically consistent, on purpose
    ///
    /// The window is the elapsed span, clamped to [`FPS_WINDOW`] and to at least the 1 ms the schema's
    /// `minimum` allows, and the value is the presents inside *that* span scaled to a second. So
    /// `value * windowMs / 1000` is exactly the count that was measured, and a reader can invert the
    /// pair — which is the whole reason §11.42 M2 refuses a bare number. A nominal 1000 here would make
    /// a window three frames old report 3 fps and call it a second.
    pub fn facts(&self, now: Instant, governor: &Governor, audio: PacingAudio) -> PacingFacts {
        let span = now.saturating_duration_since(self.started).min(FPS_WINDOW);
        // `max(1)` for the schema's `minimum: 1`, and the same integer is then used as the divisor, so
        // the reported window is the window the value was computed against rather than a rounding of it.
        let window_ms = (span.as_secs_f64() * 1000.0).round().max(1.0) as u32;
        let counted = match now.checked_sub(Duration::from_millis(u64::from(window_ms))) {
            Some(cut) => self.window.iter().filter(|t| **t > cut).count(),
            None => self.window.len(),
        };
        let mut sorted: Vec<f64> = self.gaps.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        // ⚑ Both percentiles come from `nearest_rank`, which answers `None` on an empty slice. The
        // `Unsampled` arm is therefore reached by the estimator's own refusal to invent a figure, not by
        // a length test written beside it that could disagree.
        let frame_time = match (
            crate::stats::nearest_rank(&sorted, 0.50),
            crate::stats::nearest_rank(&sorted, 0.99),
        ) {
            (Some(p50_ms), Some(p99_ms)) => FrameTimes::Sampled {
                samples: sorted.len() as u32,
                p50_ms,
                p99_ms,
            },
            _ => FrameTimes::Unsampled,
        };
        PacingFacts {
            presented: self.total,
            fps_value: counted as f64 * 1000.0 / f64::from(window_ms),
            fps_window_ms: window_ms,
            frame_time,
            audio,
            target_fps: target_fps(governor),
        }
    }
}

/// The governor's target as whole frames per second, `0` when it is switched off.
///
/// **A whole number because the schema's `targetFps` is an integer**, and the rounding is stated rather
/// than silent: a `--target-fps 59.94` run reports `60`. What the field exists to answer is *"is a
/// governor holding this rate at all"* (§11.42 M1: a paced 60 and a free-running 60 are different
/// facts), and the exact period is on the panel beside it in milliseconds.
pub fn target_fps(governor: &Governor) -> u32 {
    match governor.period() {
        // `is_paced()` is false exactly when `period()` is `None` (see `Governor::unpaced`), so the
        // `0` here is the governor being OFF and never a target that rounded to nothing.
        None => 0,
        Some(p) => (1.0 / p.as_secs_f64()).round().max(0.0) as u32,
    }
}

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

// ═══════════════════════════════════════════════════════════════════════════════════════════════════════
// ⚑ The Pacing tab's readout
//
// **The exemplar for `docs/2026-09-05-debug-window-audit.md`.** Everything below is the *projection*: the
// facts this tab shows, named, grouped, and carrying their own health. There is no `egui` type in any of
// it, which is the structural half of the lesson: a panel whose facts are a value can be tested without a
// window, and this window cannot be opened from an agent seat.
//
// # What it replaces
//
// `Panels::pacing` was thirteen `ui.monospace(format!(..))` lines with the label column spelled as literal
// spaces inside each string:
//
// ```text
// frames emulated   12345
// governor rebases  0   <- stalls of a whole frame or more
// device            NONE — pacing is unmeasured, not fine
// ```
//
// That is the style page's **P2** violation in its least visible form. P2's stated check is "no
// width-padded format specifier", and `grep -cE '\{:[<>^][0-9]+'` over the Pacing tab returned **zero**:
// the padding was in the *literal*, not in the specifier, so the tab passed the rule's own check while
// being the worst offender the owner photographed. **The check was narrower than the rule.** It also broke
// **P3** (every one of those lines is prose in the monospace face; not one is an address or a register) and
// **P10** (an em dash in the device-absent line).
//
// # The three shapes, and which fact gets which
//
// 1. **A headline stat** for the number a person opens this tab to read. Big, in the section face, with
//    its unit beside it and its label small and recessed underneath. This is what "pops" means where there
//    is no graph to draw: size and colour, since egui has no weight axis.
// 2. **A labelled fact** for supporting numbers, in the two-column grid the Objects tab already uses.
// 3. **A meter** for the one fact here that genuinely has *shape*: ring occupancy is a fraction of a
//    capacity with a threshold on it, and a fraction is a bar. It ships with a legend sentence, because
//    the failure this repo already paid for was a correct lens nobody could read ("what are the purple
//    boxes").
//
// # Health is carried, never sniffed
//
// [`Health`] is decided here, from the number, and the renderer only picks a colour from it. That is P5's
// principle ("the colour comes from the reply, not from the text") generalised off refusals: a renderer
// that decided "rebases is red when it is not `0`" would be a second copy of a judgement that belongs with
// the fact.
// ═══════════════════════════════════════════════════════════════════════════════════════════════════════

/// How a number is doing, decided beside the number and never inferred from its text downstream.
///
/// Deliberately three states and not two. **[`Health::Unmeasured`] is the whole point**: style page P6
/// says an absent fact is a stated line and never a zero, and a counter that nothing is counting is
/// exactly the case where a green `0` would be a lie. "No audio device is open" and "no audio has been
/// dropped" are different findings and must not render alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// The number is what a healthy run looks like.
    Good,
    /// The number is not fatal and is not nothing. Rebases, starvations and drops are all "zero in a
    /// healthy run", so any of them above zero is worth the reader's eye without being an error.
    Watch,
    /// **Nothing measured this.** Not a zero, not a success.
    Unmeasured,
}

/// One number the tab shows big: the value, its unit, what it is, and how it is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    /// What the number is, in the reader's words. Lower case, no padding: the layout is the layout's job.
    pub label: &'static str,
    /// The number, already formatted. A `String` because a count and a millisecond figure are formatted
    /// differently and the choice belongs here rather than in a format string at the draw site.
    pub value: String,
    /// The unit, drawn small beside the value. `None` for a bare count, where "12345 frames" would put the
    /// label in two places.
    pub unit: Option<&'static str>,
    pub health: Health,
    /// The sentence behind the number, for the hover. Every stat has one: a number whose meaning is only
    /// obvious to the person who wrote the counter is a number the tab may as well not show.
    pub hover: &'static str,
}

/// One supporting fact: a label and a value, for the two-column grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub label: &'static str,
    pub value: String,
    /// Drawn in the monospace face. **P3**: true only for a machine number a reader lines up in a column,
    /// false for everything a person reads as words.
    pub mono: bool,
    pub health: Health,
}

/// The ring, as a bar: how full it is, where the mark that steers it sits, and the sentence that says what
/// the reader is looking at.
#[derive(Clone, Debug, PartialEq)]
pub struct Meter {
    /// Occupancy as a fraction of capacity, clamped to `0.0 ..= 1.0`.
    pub fill: f32,
    /// [`RENDER_LOW_WATER_FRAMES`] as a fraction of capacity: below this the loop runs an extra emulated
    /// frame. Clamped the same way, and `None` when the device's frame size is unknown so nothing draws a
    /// mark at zero and calls it a threshold.
    pub mark: Option<f32>,
    /// What the bar is, in one sentence. **Not optional.** A bar without one is the "purple boxes"
    /// failure, which is a wall of monospace arriving from the other side.
    pub legend: String,
}

/// The audio device, or the stated reason there is none.
///
/// An enum rather than an `Option` of a struct with zeroes in it, so the absent case cannot be rendered as
/// a table of zeroes by anybody downstream. That is P6 made unrepresentable rather than merely required.
#[derive(Clone, Debug, PartialEq)]
pub enum Audio {
    /// No device was opened. `why` is a sentence, not a blank.
    Absent {
        why: &'static str,
    },
    Open(Box<AudioReadout>),
}

/// A live device's numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioReadout {
    /// The starvation and drop counts, which are the two numbers that decide whether pacing is working.
    pub stats: Vec<Stat>,
    /// Rate, channels, ring occupancy: the supporting detail.
    pub facts: Vec<Fact>,
    pub meter: Meter,
}

/// The whole Pacing tab as facts.
#[derive(Clone, Debug, PartialEq)]
pub struct Readout {
    /// The numbers the tab exists to answer: how much was emulated, how much reached the glass, how fast
    /// that is happening, and the worst the governor ever ran late.
    pub headline: Vec<Stat>,
    /// The governor's supporting facts, including the target period it is actually holding.
    pub governor: Vec<Fact>,
    /// The frame-time distribution, or **one stated line** saying nothing has been sampled yet. Never a
    /// pair of zeroes: §11.42 M3's rule is the same rule P6 already imposed on this tab, and this field
    /// and the bus row are two readings of one [`PacingFacts`].
    pub frame_time: Vec<Fact>,
    pub audio: Audio,
    /// The loop's own status line, verbatim. **P8**: it is not rewritten here.
    pub status: String,
}

/// What the tab says when no audio device opened.
///
/// It says *unmeasured*, not *fine*. The original line said the same thing and said it with an em dash;
/// this is the same finding under the owner's 2026-09-05 ruling.
pub const NO_DEVICE: &str = "No audio device opened, so the ring is not being drained and pacing here is \
                             unmeasured rather than healthy. The governor's numbers above are still real.";

/// A count that is zero in a healthy run: [`Health::Good`] at zero, [`Health::Watch`] above it.
///
/// One function so the three counters that share that shape cannot end up with three opinions.
fn zero_is_healthy(n: u64) -> Health {
    if n == 0 {
        Health::Good
    } else {
        Health::Watch
    }
}

impl Readout {
    /// Project the tab.
    ///
    /// The arguments are the raw sources rather than the `Machine` and `Device` themselves, so this
    /// function can be tested against numbers a test chooses. Wiring it to the real ones is
    /// `Panels::pacing`'s one job.
    ///
    /// # ⚑ `facts` is the same value `emulator/pacing` serves, and this function must not re-derive it
    ///
    /// The owner's 2026-09-03 ruling is that a panel shows the same answer a tool gets, and that this is
    /// made true by **one derivation with two readers** rather than by routing the panel through a
    /// transport. [`Presents::facts`] is that derivation; this is one reader and
    /// `oracle_aether::engine::Engine::pacing` is the other. Every rate figure below is *formatted* from
    /// `facts` and none is computed here — a second `presented / elapsed` on this side would be exactly
    /// the drift the arrangement exists to make impossible, and a parity test over two independent
    /// computations would only prove they agreed on the day it was written.
    #[allow(clippy::too_many_arguments)]
    pub fn of(
        frames: u64,
        facts: &PacingFacts,
        governor: &Governor,
        device: Option<DeviceFacts>,
        status: &str,
    ) -> Self {
        let worst = governor.worst_late();
        let headline = vec![
            Stat {
                label: "frames emulated",
                value: frames.to_string(),
                unit: None,
                health: Health::Good,
                hover: "Emulated frames since the window opened. The audio ring decides this number, not \
                        the display: at a steady 60 Hz it climbs by 60 a second whatever the panel is \
                        doing.",
            },
            Stat {
                label: "frames presented",
                value: facts.presented.to_string(),
                unit: None,
                health: Health::Good,
                hover: "Pictures this window actually put on the glass. It trails frames emulated \
                        whenever an iteration ran two emulated frames and presented one, and that is \
                        audio-mastered pacing working rather than a fault. This is the number \
                        `emulator/pacing` serves as `presented`.",
            },
            Stat {
                label: "presented fps",
                value: format!("{:.2}", facts.fps_value),
                unit: Some("fps"),
                // Never coloured against 60: a run launched with `--target-fps 0` is *meant* to be off
                // 60, and a "wrong" colour there would be this tab disagreeing with the flag the
                // operator passed. The judgement a reader wants is the one on `worst late` and
                // `rebases`, which are about the governor holding whatever target it was given.
                health: Health::Good,
                hover: "Presented frames per second, over the window named below it. The window is part \
                        of the figure: a rate over 200 ms and a rate over a second differ most exactly \
                        when something is wrong.",
            },
            Stat {
                label: "worst late",
                value: format!("{:.2}", worst.as_secs_f64() * 1000.0),
                unit: Some("ms"),
                health: if worst >= FRAME_PERIOD {
                    Health::Watch
                } else {
                    Health::Good
                },
                hover: "The furthest past its deadline any iteration has ever started. Under one frame \
                        period the loop absorbed it; over one, the deadline had to be moved and the \
                        rebase count below went up.",
            },
        ];

        let mut gov = vec![Fact {
            label: "fps window",
            // The measured span, said in the same breath as the rate above it. Not a nominal one second:
            // see `Presents::facts`.
            value: format!("{} ms", facts.fps_window_ms),
            mono: true,
            health: Health::Good,
        }];
        gov.push(Fact {
            label: "target period",
            value: match governor.period() {
                Some(p) => format!("{:.3} ms", p.as_secs_f64() * 1000.0),
                // The control, selected by `--target-fps 0`. Said in words, because a blank here would
                // read as "60 Hz" to anybody who did not launch this process.
                None => "none, the governor is switched off for this run".to_owned(),
            },
            mono: governor.period().is_some(),
            health: if governor.is_paced() {
                Health::Good
            } else {
                Health::Unmeasured
            },
        });
        gov.push(Fact {
            label: "rebases",
            value: governor.rebases().to_string(),
            mono: true,
            health: zero_is_healthy(governor.rebases()),
        });
        gov.push(Fact {
            label: "early wakes",
            value: governor.early_wakes().to_string(),
            mono: true,
            // Early wakes are the governor *doing its job*: a repaint arrived before its deadline and was
            // turned away. Unlike the other counters, a large number here is health rather than a
            // symptom, so it is never Watch and the hover says why.
            health: Health::Good,
        });

        // ⚑ **The unmeasured arm is a stated line, never a table of zeroes** — P6, and §11.42 M3 one
        // surface over. `Health::Unmeasured` is what stops it being drawn in the same colour as a
        // measured figure, exactly as the absent-device sentence is.
        let frame_time = match facts.frame_time {
            FrameTimes::Unsampled => vec![Fact {
                label: "samples",
                value:
                    "0. Nothing has been sampled yet, so there is no median and no worst case to \
                        show."
                        .to_owned(),
                mono: false,
                health: Health::Unmeasured,
            }],
            FrameTimes::Sampled {
                samples,
                p50_ms,
                p99_ms,
            } => vec![
                Fact {
                    label: "median",
                    value: format!("{p50_ms:.2} ms"),
                    mono: true,
                    health: Health::Good,
                },
                Fact {
                    label: "p99",
                    value: format!("{p99_ms:.2} ms"),
                    mono: true,
                    // A 99th percentile past a whole frame period is a visible stutter once every
                    // hundred frames. Judged against `FRAME_PERIOD` — the constant the governor holds —
                    // rather than against a number typed here.
                    health: if p99_ms >= FRAME_PERIOD.as_secs_f64() * 1000.0 {
                        Health::Watch
                    } else {
                        Health::Good
                    },
                },
                Fact {
                    label: "samples",
                    value: samples.to_string(),
                    mono: true,
                    health: Health::Good,
                },
            ],
        };

        Self {
            headline,
            governor: gov,
            frame_time,
            // ⚑ The arm is chosen from `facts.audio`, not from `device.is_some()`. They agree today —
            // `Presents::facts` is handed the one derived from the other — and choosing here would be a
            // second opinion about whether a device exists, which is precisely the pair §11.42 M3 says
            // must never be able to disagree.
            audio: match (facts.audio, device) {
                (PacingAudio::Unmeasured, _) | (_, None) => Audio::Absent { why: NO_DEVICE },
                (PacingAudio::Measured { underruns }, Some(d)) => {
                    Audio::Open(Box::new(d.into_readout(underruns)))
                }
            },
            status: status.to_owned(),
        }
    }
}

/// The audio half of [`PacingFacts`], from the device facts the window already reads.
///
/// **One function, because it is the place "no device" becomes a wire shape.** The panel and the bus row
/// both come off the [`PacingFacts`] this feeds, so there is no second site where `starved_steady` could
/// be read into a zero that looks measured.
pub fn audio_facts(device: Option<DeviceFacts>) -> PacingAudio {
    match device {
        None => PacingAudio::Unmeasured,
        Some(d) => PacingAudio::Measured {
            // `starved_steady` and not the raw starvation count: `crate::device` calls the steady-state
            // figure "the pacing verdict", the warm-up ones being the device filling its first buffers.
            underruns: d.starved_steady,
        },
    }
}

/// The raw device numbers, lifted out of `crate::device::Device` so [`Readout::of`] takes values a test can
/// choose rather than a live CPAL stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceFacts {
    pub rate_hz: u32,
    pub channels: usize,
    pub occupied: usize,
    pub capacity: usize,
    pub starved_steady: u64,
    pub dropped: u64,
}

impl DeviceFacts {
    /// `underruns` is passed in rather than read off `self.starved_steady`: it is the number
    /// `emulator/pacing` serves, and taking it from the served value is what makes the tab and the wire
    /// one reading. See [`Readout::of`].
    fn into_readout(self, underruns: u64) -> AudioReadout {
        let frame_samples = if self.rate_hz == 0 {
            0
        } else {
            audio::frame_samples(self.rate_hz)
        };
        // Milliseconds of audio held, from the sample count and the rate. `500.0` rather than `1000.0`
        // because the ring is interleaved stereo, so a sample slot is half a frame of one channel.
        let latency_ms = if self.rate_hz == 0 {
            0.0
        } else {
            self.occupied as f64 * 500.0 / self.rate_hz as f64
        };
        let fraction = |n: usize| {
            if self.capacity == 0 {
                0.0
            } else {
                (n as f32 / self.capacity as f32).clamp(0.0, 1.0)
            }
        };
        let low_water = RENDER_LOW_WATER_FRAMES * frame_samples;
        let stats = vec![
            Stat {
                label: "starved",
                value: underruns.to_string(),
                unit: None,
                health: zero_is_healthy(underruns),
                hover: "Callbacks that found the ring empty once the run had settled, each one an \
                        audible click. Zero is the only healthy value.",
            },
            Stat {
                label: "producer drops",
                value: self.dropped.to_string(),
                unit: None,
                health: zero_is_healthy(self.dropped),
                hover: "Samples the emulator produced that would not fit in the ring. Above zero means \
                        the loop is running ahead of the device, which is the failure the governor was \
                        added to stop.",
            },
        ];
        let facts = vec![
            Fact {
                label: "device",
                value: format!("{} Hz, {} channels", self.rate_hz, self.channels),
                mono: false,
                health: Health::Good,
            },
            Fact {
                label: "ring",
                value: format!("{} of {} samples", self.occupied, self.capacity),
                mono: true,
                health: Health::Good,
            },
            Fact {
                label: "buffered audio",
                value: format!("{latency_ms:.1} ms"),
                mono: true,
                health: Health::Good,
            },
        ];
        AudioReadout {
            stats,
            meter: Meter {
                fill: fraction(self.occupied),
                // No mark rather than a mark at zero: a threshold drawn at the left edge would read as
                // "the ring is always above the mark", which is the opposite of what an unknown means.
                mark: if low_water == 0 || self.capacity == 0 {
                    None
                } else {
                    Some(fraction(low_water))
                },
                legend: format!(
                    "How much audio is waiting to be played, out of the {} samples the ring holds. The \
                     tick is the low mark: below it the loop runs an extra emulated frame to catch up.",
                    self.capacity
                ),
            },
            facts,
        }
    }
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

    // ---- the readout ---------------------------------------------------------------------------------
    //
    // These are the audit page's exemplar gates. They are assertions about the FACTS the Pacing tab shows,
    // which is the whole reason [`Readout`] is an egui-free value: this window cannot be opened from an
    // agent seat, so a panel whose correctness lives in its draw calls is a panel nothing can check.

    /// A device at 44 100 Hz with a healthy ring, for the cases that want one.
    fn healthy_device() -> DeviceFacts {
        let f = audio::frame_samples(44_100);
        DeviceFacts {
            rate_hz: 44_100,
            channels: 2,
            occupied: 3 * f,
            capacity: audio::RING_FRAMES * f,
            starved_steady: 0,
            dropped: 0,
        }
    }

    /// **A measurement, taken from the meter — never a hand-written `PacingFacts` literal.**
    ///
    /// A test that assembles the struct itself is testing a value nothing produces, and would keep
    /// passing after `Presents::facts` started producing a different one. So the presents are fed in at
    /// one frame period apart, which is what the caller's `presented` count means here.
    fn facts_for(presented: u64, governor: &Governor, device: Option<DeviceFacts>) -> PacingFacts {
        let t0 = Instant::now();
        let mut p = Presents::start(t0);
        for i in 1..=presented {
            p.note(t0 + FRAME_PERIOD * u32::try_from(i).expect("test counts fit in u32"));
        }
        let now = t0 + FRAME_PERIOD * u32::try_from(presented + 1).expect("fits");
        p.facts(now, governor, audio_facts(device))
    }

    fn readout(device: Option<DeviceFacts>) -> Readout {
        let g = Governor::start(Instant::now(), FRAME_PERIOD);
        Readout::of(
            1234,
            &facts_for(1230, &g, device),
            &g,
            device,
            "governor on · 1234 frames · 0 rebases",
        )
    }

    /// Every string the tab can draw, in one place, so a rule about text can be asserted over all of it
    /// rather than over whichever field the test author remembered.
    fn every_string(r: &Readout) -> Vec<String> {
        let mut out = vec![r.status.clone()];
        let push_stats = |s: &[Stat], out: &mut Vec<String>| {
            for s in s {
                out.push(s.label.to_owned());
                out.push(s.value.clone());
                out.push(s.hover.to_owned());
                if let Some(u) = s.unit {
                    out.push(u.to_owned());
                }
            }
        };
        let push_facts = |f: &[Fact], out: &mut Vec<String>| {
            for f in f {
                out.push(f.label.to_owned());
                out.push(f.value.clone());
            }
        };
        push_stats(&r.headline, &mut out);
        push_facts(&r.governor, &mut out);
        match &r.audio {
            Audio::Absent { why } => out.push((*why).to_owned()),
            Audio::Open(a) => {
                push_stats(&a.stats, &mut out);
                push_facts(&a.facts, &mut out);
                out.push(a.meter.legend.clone());
            }
        }
        out
    }

    /// **P6, and the reason [`Audio`] is an enum.** With no device open the tab states that pacing is
    /// unmeasured. It does not show a starvation count of zero, which would be a measurement nobody took.
    #[test]
    fn an_absent_device_is_a_stated_line_and_never_a_row_of_zeroes() {
        let r = readout(None);
        let Audio::Absent { why } = &r.audio else {
            panic!("a readout built with no device claims to have one");
        };
        assert!(why.contains("unmeasured"), "{why}");
        assert!(
            !why.contains("fine") || why.contains("not fine"),
            "the absent line must not read as health: {why}"
        );
        // The governor's own numbers survive the device's absence, because the governor measured them.
        assert_eq!(r.headline.len(), 4);
        assert_eq!(r.headline[0].value, "1234");
        // ⚑ And the *frame-time* section survives it too, which is the same rule one instrument over:
        // frame times are wall clock between presents and owe nothing to an audio device, so an absent
        // device must not blank them. A real distribution is here because `readout` fed the meter 1230
        // presents; the unsampled-is-not-zero half is `nothing_sampled_is_a_stated_line_not_a_zero`.
        assert!(
            r.frame_time.iter().any(|f| f.label == "p99"),
            "an absent audio device blanked the frame-time percentiles: {:?}",
            r.frame_time
        );
    }

    // ---- the two readers, and the guarantee that they cannot disagree ------------------------------

    /// Look a `Fact` up by label, or say which labels there were. A `None` here is the failure mode this
    /// whole test is about, so it must not read as "nothing to compare".
    fn fact<'a>(facts: &'a [Fact], label: &str) -> &'a str {
        facts
            .iter()
            .find(|f| f.label == label)
            .map(|f| f.value.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "no `{label}` fact; the panel drew {:?}",
                    facts.iter().map(|f| f.label).collect::<Vec<_>>()
                )
            })
    }

    fn stat<'a>(stats: &'a [Stat], label: &str) -> &'a str {
        stats
            .iter()
            .find(|s| s.label == label)
            .map(|s| s.value.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "no `{label}` stat; the panel drew {:?}",
                    stats.iter().map(|s| s.label).collect::<Vec<_>>()
                )
            })
    }

    /// **THE parity gate: the Pacing tab and `emulator/pacing` cannot show different numbers.**
    ///
    /// The owner's 2026-09-03 ruling is that a panel shows the same answer a tool gets. This lane makes
    /// that true by **one derivation with two readers**, not by a transport — contract D15 has an
    /// in-process GUI read the registry directly and never open a socket to itself, and `host.rs` says
    /// panels draw from the same instruments the bus serves.
    ///
    /// So this test takes ONE `PacingFacts` out of the meter and hands it to both readers: to
    /// [`Readout::of`], which is what the tab draws, and to `Host::set_pacing` + `Host::call`, which is
    /// the served row down to the handler. It then compares the panel's own strings against the served
    /// numbers.
    ///
    /// ⚑ **It compares STRINGS the panel actually drew, not fields it was handed.** A test that read
    /// `facts.presented` on both sides would pass on any panel whatsoever, including one that formats a
    /// different field into the box. What is asserted is the rendered value beside the rendered label —
    /// which is the thing a person reads off the glass.
    ///
    /// **What it does NOT claim**: that two independent computations agree. There is only one, on
    /// purpose. A parity test over two derivations proves they agreed on the day it was written; this one
    /// proves the readers are readers.
    ///
    /// # Red-first evidence, 2026-09-06, with the parameter varied
    ///
    /// Four mutations were applied to the shipped source, each proven on disk before the run and each
    /// restored from the committed baseline afterwards:
    ///
    /// * the panel recomputes its own fps (`facts.presented / 60.0`) — **red here**. This is the defect
    ///   the whole arrangement exists to prevent, planted.
    /// * the panel's fps FORMAT alone changes, `{:.2}` to `{:.1}`, with the same value behind it —
    ///   **red here**, which is what proves this gate compares the string the tab drew rather than the
    ///   field it was handed. A test that read `facts.fps_value` on both sides would have stayed green.
    /// * `crate::stats::nearest_rank` answers `Some(0.0)` on an empty slice — red on
    ///   `nothing_sampled_is_a_stated_line_on_the_tab_and_an_absent_pair_on_the_wire`, which is M3's
    ///   loud-on-unmeasurable rule catching the single most common percentile bug there is.
    /// * `fps_window_ms` becomes a nominal `1000` — red on
    ///   `the_rate_and_its_window_invert_to_the_presents_that_were_counted`.
    ///
    /// ⚑ **The first sweep reported all four as "still green" and they were not.** The runner matched
    /// `"test <name> ... FAILED"`, which is the harness's spelling for an *integration* test; a unit test
    /// inside a bin is named `pacing::tests::<name>` and never matched. Recorded because it is the
    /// six-instance lesson in this repo wearing a new hat: a matcher that cannot see a failure and a gate
    /// that did not fire produce the same artifact.
    #[test]
    fn the_panel_and_the_served_row_cannot_show_different_numbers() {
        // A measurement with a real distribution and a real device, so every conditional arm is
        // populated: an unsampled/unmeasured fixture would compare four fields and skip the rest.
        let t0 = Instant::now();
        let g = Governor::start(t0, FRAME_PERIOD);
        let device = healthy_device();
        let mut p = Presents::start(t0);
        // Deliberately uneven gaps: a constant period makes p50 and p99 the same number, and a test
        // whose two percentiles are equal cannot tell them apart.
        let mut at = t0;
        for i in 0..90u32 {
            at += FRAME_PERIOD + Duration::from_micros(u64::from(i) * 40);
            p.note(at);
        }
        let facts = p.facts(at, &g, audio_facts(Some(device)));

        // Reader 1: the panel.
        let r = Readout::of(4321, &facts, &g, Some(device), "governor on");

        // Reader 2: the bus, through the real handler on a real (in-process) host.
        let mut sys = oracle_core::system::System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        let mut host = oracle_aether::host::Host::new(oracle_aether::host::HostConfig {
            engine: oracle_aether::engine::EngineConfig {
                presents_frames: true,
                ..oracle_aether::host::HostConfig::default().engine
            },
            ..oracle_aether::host::HostConfig::default()
        });
        host.set_pacing(facts);
        let (reply, _) = host.call(&mut sys, "emulator/pacing", &serde_json::json!({}));
        let wire = reply.expect("a presenting host that published serves the row");

        // --- the comparison, field by field, panel string against served number ---------------------

        assert_eq!(
            stat(&r.headline, "frames presented"),
            wire["presented"].as_u64().expect("presented").to_string(),
            "the tab's presented count and the served `presented` are different numbers"
        );
        assert_eq!(
            stat(&r.headline, "presented fps"),
            format!("{:.2}", wire["fps"]["value"].as_f64().expect("fps.value")),
            "the tab's fps and the served `fps.value` are different numbers"
        );
        assert_eq!(
            fact(&r.governor, "fps window"),
            format!(
                "{} ms",
                wire["fps"]["windowMs"].as_u64().expect("fps.windowMs")
            ),
            "the tab's fps is drawn over a different window than the one served with it"
        );
        assert_eq!(
            fact(&r.frame_time, "samples"),
            wire["frameTimeMs"]["samples"]
                .as_u64()
                .expect("samples")
                .to_string()
        );
        assert_eq!(
            fact(&r.frame_time, "median"),
            format!(
                "{:.2} ms",
                wire["frameTimeMs"]["p50"].as_f64().expect("p50")
            )
        );
        assert_eq!(
            fact(&r.frame_time, "p99"),
            format!(
                "{:.2} ms",
                wire["frameTimeMs"]["p99"].as_f64().expect("p99")
            )
        );
        let Audio::Open(a) = &r.audio else {
            panic!("the fixture has a device and the tab must say so");
        };
        assert_eq!(
            wire["audio"]["unmeasured"],
            serde_json::json!(false),
            "the tab drew a device and the wire said there was none"
        );
        assert_eq!(
            stat(&a.stats, "starved"),
            wire["audio"]["underruns"]
                .as_u64()
                .expect("underruns")
                .to_string(),
            "the tab's starvation count and the served `underruns` are different numbers"
        );

        // The percentiles are distinguishable, so the two rows above compared two different values
        // rather than one number twice. Without this the gaps could have been constant and the test
        // would pass with `p50` and `p99` swapped.
        assert_ne!(
            wire["frameTimeMs"]["p50"], wire["frameTimeMs"]["p99"],
            "the fixture produced a flat distribution, so the p50/p99 rows above prove nothing"
        );
    }

    /// The other arm of the same parity, because the arm above populates every conditional and therefore
    /// cannot see a reader that mishandles an ABSENT one: with nothing sampled and no device, the tab
    /// says so in words and the wire says so in its own shape, and **neither says zero**.
    #[test]
    fn nothing_sampled_is_a_stated_line_on_the_tab_and_an_absent_pair_on_the_wire() {
        let t0 = Instant::now();
        let g = Governor::start(t0, FRAME_PERIOD);
        let facts = Presents::start(t0).facts(t0, &g, audio_facts(None));
        assert_eq!(facts.frame_time, FrameTimes::Unsampled);

        let r = Readout::of(0, &facts, &g, None, "starting");
        let line = fact(&r.frame_time, "samples");
        assert!(
            line.contains("Nothing has been sampled"),
            "the tab must SAY nothing was sampled, not print a zero beside a median: {line:?}"
        );
        assert!(
            !r.frame_time.iter().any(|f| f.label == "median"),
            "a median over zero samples is a number nobody measured"
        );
        assert_eq!(
            r.frame_time[0].health,
            Health::Unmeasured,
            "the unsampled line must be drawn as unmeasured, never as a healthy figure"
        );
        let Audio::Absent { .. } = &r.audio else {
            panic!("no device, so the tab states it");
        };

        let mut sys = oracle_core::system::System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        let mut host = oracle_aether::host::Host::new(oracle_aether::host::HostConfig {
            engine: oracle_aether::engine::EngineConfig {
                presents_frames: true,
                ..oracle_aether::host::HostConfig::default().engine
            },
            ..oracle_aether::host::HostConfig::default()
        });
        host.set_pacing(facts);
        let (reply, _) = host.call(&mut sys, "emulator/pacing", &serde_json::json!({}));
        let wire = reply.expect("served");
        assert_eq!(wire["frameTimeMs"]["samples"], serde_json::json!(0));
        assert!(wire["frameTimeMs"].get("p50").is_none());
        assert_eq!(wire["audio"]["unmeasured"], serde_json::json!(true));
        assert!(wire["audio"].get("underruns").is_none());
    }

    /// **`fps.value` and `fps.windowMs` are one quantity, and the pair inverts.**
    ///
    /// Derived from the meter's own inputs rather than pinned: the presents fed in are counted, and the
    /// served rate scaled back by the served window must return that count. This is what §11.42 M2 buys
    /// — a bare number could not be checked this way by anyone.
    #[test]
    fn the_rate_and_its_window_invert_to_the_presents_that_were_counted() {
        let t0 = Instant::now();
        let g = Governor::start(t0, FRAME_PERIOD);
        let mut p = Presents::start(t0);
        // Half a second of presents at one frame period, all inside the one-second window.
        let n = 30u32;
        let mut at = t0;
        for _ in 0..n {
            at += FRAME_PERIOD;
            p.note(at);
        }
        let f = p.facts(at, &g, audio_facts(None));
        let counted = f.fps_value * f64::from(f.fps_window_ms) / 1000.0;
        assert!(
            (counted - f64::from(n)).abs() < 0.5,
            "the pair does not invert: {} fps over {} ms is {counted} presents, and {n} were fed in",
            f.fps_value,
            f.fps_window_ms
        );
        // The window is the ELAPSED span, not a nominal second: this meter is half a second old.
        assert!(
            f.fps_window_ms < 1000,
            "a meter {:?} old reported a {} ms window, which is a second that has not happened",
            at - t0,
            f.fps_window_ms
        );
    }

    /// The governor's target reaches the wire as whole frames per second, and **off is 0**, derived from
    /// the constants rather than typed: `FRAME_PERIOD` is the player's own rate.
    #[test]
    fn the_target_is_the_governors_own_rate_and_zero_means_off() {
        let now = Instant::now();
        let paced = Governor::start(now, FRAME_PERIOD);
        assert_eq!(
            target_fps(&paced),
            (1.0 / FRAME_PERIOD.as_secs_f64()).round() as u32
        );
        assert_eq!(
            target_fps(&Governor::unpaced(now)),
            0,
            "`--target-fps 0` is reported as 0, which is why the field is required"
        );
    }

    /// A present is counted once, and an iteration that presented nothing does not inflate the rate.
    #[test]
    fn only_a_present_is_counted() {
        let t0 = Instant::now();
        let g = Governor::start(t0, FRAME_PERIOD);
        let mut p = Presents::start(t0);
        for i in 1..=5u32 {
            p.note(t0 + FRAME_PERIOD * i);
        }
        let at = t0 + FRAME_PERIOD * 6;
        assert_eq!(p.facts(at, &g, audio_facts(None)).presented, 5);
        // Five presents are four gaps: the first present has nothing to be measured against, and a
        // fifth sample here would be a gap the meter invented.
        let FrameTimes::Sampled { samples, .. } = p.facts(at, &g, audio_facts(None)).frame_time
        else {
            panic!("five presents produce a distribution");
        };
        assert_eq!(samples, 4);
    }

    /// **P5's principle off refusals.** Health is decided beside the number. A renderer that decided
    /// "rebases is a warning when it is not zero" would be a second copy of this judgement, and the two
    /// would drift.
    #[test]
    fn a_counter_that_should_be_zero_carries_its_own_health() {
        let find = |r: &Readout, label: &str| -> Health {
            r.governor
                .iter()
                .find(|f| f.label == label)
                .unwrap_or_else(|| panic!("no `{label}` fact"))
                .health
        };
        let now = Instant::now();

        let quiet = Governor::start(now, FRAME_PERIOD);
        let r = Readout::of(0, &facts_for(0, &quiet, None), &quiet, None, "");
        assert_eq!(find(&r, "rebases"), Health::Good);

        // Force one real rebase rather than poking the field: a 100 ms stall, which is what
        // `a_stall_rebases_and_does_not_sprint` above proves costs exactly one.
        let mut stalled = Governor::start(now, FRAME_PERIOD);
        stalled.tick(now);
        stalled.tick(now + Duration::from_millis(100));
        assert_eq!(stalled.rebases(), 1, "the stall did not produce a rebase");
        let r = Readout::of(0, &facts_for(0, &stalled, None), &stalled, None, "");
        assert_eq!(find(&r, "rebases"), Health::Watch);
        // ...and `worst late` went with it, because a rebase is by definition a whole period over.
        // Found by label rather than by index: the headline gained a fourth stat when the pacing readout
        // landed, and an index here would have moved silently onto a different number.
        let worst = r
            .headline
            .iter()
            .find(|s| s.label == "worst late")
            .expect("a `worst late` stat");
        assert_eq!(worst.health, Health::Watch);

        // An early wake is the governor WORKING, so it is never a warning however large it gets.
        let mut early = Governor::start(now, FRAME_PERIOD);
        early.tick(now);
        for i in 1..50 {
            early.tick(now + Duration::from_micros(i * 10));
        }
        assert!(early.early_wakes() > 10);
        let r = Readout::of(0, &facts_for(0, &early, None), &early, None, "");
        assert_eq!(find(&r, "early wakes"), Health::Good);

        // The device's two counters carry the same rule.
        let mut sick = healthy_device();
        sick.starved_steady = 3;
        sick.dropped = 5_800_000;
        let Audio::Open(a) = readout(Some(sick)).audio else {
            panic!("no device");
        };
        assert!(a.stats.iter().all(|s| s.health == Health::Watch));
        let Audio::Open(a) = readout(Some(healthy_device())).audio else {
            panic!("no device");
        };
        assert!(a.stats.iter().all(|s| s.health == Health::Good));
    }

    /// **The strengthened P2 check.**
    ///
    /// The style page states P2's test as "no width-padded format specifier", and
    /// `grep -cE '\{:[<>^][0-9]+'` over the old Pacing tab returned **zero** while every one of its lines
    /// was a hand-spaced pseudo-table: the padding lived in the string literal, not in the specifier. The
    /// rule was right and its check was narrower than the rule. This is the wider check, and it is
    /// asserted on the value rather than on the source, so no spelling of the padding can slip past it.
    #[test]
    fn no_string_the_tab_draws_pads_itself_into_a_column() {
        for r in [readout(None), readout(Some(healthy_device()))] {
            for s in every_string(&r) {
                assert!(
                    !s.contains("  "),
                    "a run of spaces is a column being drawn inside a string, which is the pseudo-table \
                     P2 outlaws. The grid draws the columns: {s:?}"
                );
                assert!(
                    !s.contains('\t'),
                    "a tab is the same defect with a different character: {s:?}"
                );
            }
        }
    }

    /// **P10.** The owner's ruling, over every string this tab can draw, in both device arms.
    ///
    /// The line this replaced was `device            NONE — pacing is unmeasured, not fine`.
    #[test]
    fn nothing_the_tab_draws_carries_an_em_or_en_dash() {
        for r in [readout(None), readout(Some(healthy_device()))] {
            for s in every_string(&r) {
                for bad in ['\u{2014}', '\u{2013}'] {
                    assert!(
                        !s.contains(bad),
                        "user-facing text carries {bad:?}, which the owner's 2026-09-05 ruling bars: {s:?}"
                    );
                }
            }
        }
    }

    /// **The meter's mark is the policy's, derived rather than copied.**
    ///
    /// A bar with a threshold drawn on it is a claim about behaviour, and the way that claim goes stale is
    /// somebody retuning [`RENDER_LOW_WATER_FRAMES`] and leaving a line painted at the old fraction. So
    /// this does not compare the mark to a number: it asks [`frames_to_run`] what it actually does either
    /// side of the mark, and requires the drawn line to be the place the answer changes.
    #[test]
    fn the_meters_mark_is_where_the_policy_actually_changes_its_mind() {
        let d = healthy_device();
        let f = audio::frame_samples(d.rate_hz);
        let Audio::Open(a) = readout(Some(d)).audio else {
            panic!("no device");
        };
        let mark = a.meter.mark.expect("a device with a known rate has a mark");

        // The sample counts either side of the drawn line.
        let below = (mark * d.capacity as f32) as usize - 1;
        let above = (mark * d.capacity as f32) as usize + 1;
        assert_eq!(
            frames_to_run(below, d.capacity, f, 0, RENDER_LOW_WATER_FRAMES),
            audio::MAX_FRAMES_PER_ITER,
            "the bar's tick is drawn above the point the loop starts catching up, so a reader watching \
             the bar cross it would see nothing happen"
        );
        assert_eq!(
            frames_to_run(above, d.capacity, f, 0, RENDER_LOW_WATER_FRAMES),
            1,
            "the bar's tick is drawn below the point the loop stops catching up"
        );
        // And it is somewhere a person can see, rather than pinned to an edge.
        assert!((0.05..0.95).contains(&mark), "mark at {mark}");
    }

    /// **An unknown is not a threshold at zero.** A ring with no capacity yet, which is what the loop
    /// holds before the device opens, marks nothing rather than drawing a line at the left edge that would
    /// read as "always above the mark".
    #[test]
    fn a_ring_with_nothing_known_about_it_marks_no_threshold() {
        let empty = DeviceFacts {
            rate_hz: 0,
            channels: 0,
            occupied: 0,
            capacity: 0,
            starved_steady: 0,
            dropped: 0,
        };
        let Audio::Open(a) = readout(Some(empty)).audio else {
            panic!("no device");
        };
        assert_eq!(a.meter.mark, None);
        assert_eq!(a.meter.fill, 0.0);
        assert!(
            !a.meter.legend.is_empty(),
            "a bar with no legend is the `what are the purple boxes` failure"
        );
    }

    /// The fill is a fraction and stays one, however the two numbers arrive. A ring reported as fuller
    /// than its capacity is a bug somewhere else, and a bar drawn past its own end is that bug arriving on
    /// the owner's screen as a rendering artefact instead of a number.
    #[test]
    fn the_fill_is_always_a_fraction() {
        let f = audio::frame_samples(44_100);
        for (occ, cap) in [(0, 8 * f), (8 * f, 8 * f), (99 * f, 8 * f), (0, 0)] {
            let d = DeviceFacts {
                rate_hz: 44_100,
                channels: 2,
                occupied: occ,
                capacity: cap,
                starved_steady: 0,
                dropped: 0,
            };
            let Audio::Open(a) = readout(Some(d)).audio else {
                panic!("no device");
            };
            assert!(
                (0.0..=1.0).contains(&a.meter.fill),
                "occ={occ} cap={cap} gave fill {}",
                a.meter.fill
            );
        }
    }

    /// **P3.** Only machine numbers take the monospace face. The tab's prose and its labels do not, and
    /// neither does the line that says the governor is switched off, which is a sentence.
    #[test]
    fn only_the_machine_numbers_are_monospace() {
        let unpaced = Governor::unpaced(Instant::now());
        let d = Some(healthy_device());
        let r = Readout::of(0, &facts_for(0, &unpaced, d), &unpaced, d, "");
        let period = r
            .governor
            .iter()
            .find(|f| f.label == "target period")
            .expect("period fact");
        assert!(
            !period.mono,
            "the governor-off sentence is prose and must not be drawn in the register face"
        );
        assert_eq!(period.health, Health::Unmeasured);

        // With a governor running it is a duration, which lines up in a column and keeps the face.
        let paced = Governor::start(Instant::now(), FRAME_PERIOD);
        let r = Readout::of(0, &facts_for(0, &paced, d), &paced, d, "");
        let period = r
            .governor
            .iter()
            .find(|f| f.label == "target period")
            .expect("period fact");
        assert!(period.mono);
        assert_eq!(period.health, Health::Good);

        // The device line is words and a number ("44100 Hz, 2 channels"); the ring is two counts a reader
        // compares, so it keeps the face.
        let Audio::Open(a) = r.audio else {
            panic!("no device")
        };
        let mono: Vec<&str> = a.facts.iter().filter(|f| f.mono).map(|f| f.label).collect();
        assert_eq!(mono, vec!["ring", "buffered audio"]);
    }

    /// Every stat carries the sentence behind it. A number whose meaning is obvious only to whoever wrote
    /// the counter is a number the tab may as well not show, and this is the assertion that stops the next
    /// stat being added without one.
    #[test]
    fn every_headline_number_says_what_it_means() {
        let r = readout(Some(healthy_device()));
        let Audio::Open(a) = &r.audio else {
            panic!("no device")
        };
        for s in r.headline.iter().chain(a.stats.iter()) {
            assert!(
                s.hover.len() > 40,
                "`{}` has no sentence behind it: {:?}",
                s.label,
                s.hover
            );
            assert!(!s.label.is_empty());
            assert!(!s.value.is_empty());
        }
    }
}
