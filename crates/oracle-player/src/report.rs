//! The pacing report: everything one measurement run has to say, in one block.
//!
//! # What is reported and what is refused
//!
//! * **Distributions, never a mean alone.** Every cost is quoted as mean/median/p95/p99/max over retained
//!   samples ([`crate::stats::Series`]). The frame rate is quoted as a rate *and* as a period
//!   distribution, because "60 fps with a 40 ms worst frame" and "60 fps flat" are different players.
//! * **Both ends of the audio feedback loop.** Starvations *and* producer drops. The toolkit spike's two
//!   bad runs were the same fault in opposite directions — one starved, one overflowed — and a report that
//!   printed only underruns would have called the 93 fps run healthy.
//! * **Proof the picture is real.** The last frame's non-black pixel count and distinct-colour count. A run
//!   that never got the VDP going would show 0 non-black pixels and every cost above it would be a
//!   measurement of a black screen.
//! * **A refusal, where the instrument cannot reach.** Under `Xvfb` the GL path is `llvmpipe`, a software
//!   rasteriser on the same cores as the emulator. Its presented frame rate is not this machine's, and the
//!   report says so in place of quoting it. See [`Reach`].

use std::sync::atomic::Ordering;

use crate::device::{Device, MIN_OCCUPANCY_UNMEASURED, WARMUP_CALLBACKS};
use crate::machine::Machine;
use crate::pacing::Governor;
use crate::stats::Series;

/// Per-part cost buckets, split the way `docs/2026-09-02-toolkit-spike.md` §3 split them so the two sets
/// of numbers line up column for column.
#[derive(Default)]
pub struct Buckets {
    pub emulate: Series,
    pub audio: Series,
    pub convert: Series,
    pub upload: Series,
    pub ui: Series,
    pub tessellate: Series,
    /// **Parcel 3's own cost, named rather than folded in.** One `Host::set_paused` plus one `Host::pump`
    /// per iteration — the drain that lands a breakpoint halt and a client pause. It is a bucket of its own
    /// because this parcel invalidated parcel 1's measurement and a re-measurement that could not see the
    /// thing that invalidated it would be worth nothing. Recorded on every *frame-owning* iteration, so it
    /// shares its `n` with the columns beside it.
    ///
    /// (The instrument wrappers are the other half of the change, and they are inside `emulate` — see
    /// [`crate::machine::Machine::step`] for why they are timed there rather than here.)
    pub bus: Series,
    pub cpu_total: Series,
    /// Wall time between the starts of consecutive *frame-owning* iterations. This is the frame period,
    /// and its distribution is the stutter answer.
    pub period: Series,
}

/// How far this run's instrument reaches. Printed with the numbers, not buried in a doc.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// No window, no GPU: a bare `egui::Context` driven to the governor's deadline. Every figure is
    /// display-independent by construction, including the frame rate — because the governor, not vsync, is
    /// what sets it.
    DisplayIndependent,
    /// The real winit + wgpu stack under `Xvfb`. The CPU parts are honest; the presented frame rate is
    /// `llvmpipe`'s and is refused.
    SoftwareRasteriser,
}

pub struct Run<'a> {
    pub label: &'a str,
    pub reach: Reach,
    pub elapsed: f64,
    pub iterations: u64,
    /// Iterations that owned a frame (i.e. were not turned away early by the governor).
    pub frame_iterations: u64,
    pub buckets: &'a Buckets,
    pub machine: &'a Machine,
    pub governor: &'a Governor,
    /// Frame-owning iterations by how many emulated frames the **audio ring** asked for: `[0, 1, 2]`.
    ///
    /// This is the fine trim's own workload, and it is the number that says whether the governor and the
    /// device agree. A healthy run is almost all 1s: the governor is holding 60.000 Hz and the device's
    /// true rate is close enough that the ring rarely has to correct. A run full of 2s is a governor
    /// running slow; a run full of 0s is a governor running fast — and it was a run of nothing but 0s,
    /// exhausting `MAX_CONSECUTIVE_SKIPS` over and over, that produced the spike's 5.8 M dropped samples.
    pub frames_per_iter: [u64; 3],
    /// What the toolkit said its screen was, when the run asked. `None` in modes with no toolkit screen.
    pub screen: Option<(f32, f32)>,
    /// Whether audio was *asked for*. Distinguishes "no device here" (a reach limit) from "switched off
    /// for this pass" (a choice).
    pub wanted_audio: bool,
}

pub fn print(r: &Run) {
    println!("\n================ oracle-player pacing report ================");
    println!("run                  {}", r.label);
    if let Some((w, h)) = r.screen {
        println!("toolkit screen       {w}x{h}");
    }
    println!("wall seconds         {:.2}", r.elapsed);
    println!("loop iterations      {}", r.iterations);
    println!(
        "frame iterations     {}  ({:.2}/s)",
        r.frame_iterations,
        r.frame_iterations as f64 / r.elapsed
    );
    println!(
        "emulated frames      {}  ({:.3}/s)   <-- the machine's speed; 60.000 is real time",
        r.machine.frames(),
        r.machine.frames() as f64 / r.elapsed
    );
    println!(
        "pictures completed   {}  ({:.3}/s)",
        r.machine.pictures(),
        r.machine.pictures() as f64 / r.elapsed
    );
    match r.reach {
        Reach::DisplayIndependent => println!(
            "presented frame rate {:.3}/s   <-- set by the GOVERNOR, not by vsync, so this figure is \
             display-independent",
            r.frame_iterations as f64 / r.elapsed
        ),
        Reach::SoftwareRasteriser => println!(
            "presented frame rate REFUSED: this run rasterises in software (llvmpipe on Xvfb) on the \
             same cores as the emulator. Its present cost is not this machine's."
        ),
    }

    // Proof the costs above are for a real picture.
    match r.machine.image() {
        Some(img) => {
            let lit = img
                .pixels
                .iter()
                .filter(|p| p.r() != 0 || p.g() != 0 || p.b() != 0)
                .count();
            let mut c: Vec<[u8; 4]> = img.pixels.iter().map(|p| p.to_array()).collect();
            c.sort_unstable();
            c.dedup();
            println!(
                "last picture         {}x{}, {lit} non-black pixels ({:.1}%), {} distinct colours",
                img.size[0],
                img.size[1],
                lit as f64 * 100.0 / img.pixels.len().max(1) as f64,
                c.len()
            );
        }
        None => println!(
            "last picture         NONE: THE RUN NEVER COMPLETED A FRAME; ignore every number above"
        ),
    }

    // The headline, in the two numbers a mean would have hidden.
    if !r.governor.is_paced() {
        println!(
            "\n*** GOVERNOR OFF: this is the CONTROL run, the toolkit spike's arrangement with layer 1 \
             removed. Nothing below is the player's behaviour. ***"
        );
    }
    if r.buckets.period.is_empty() {
        println!(
            "\nFRAME PERIOD         NOT MEASURED: fewer than two frame-owning iterations completed"
        );
    } else {
        println!(
            "\nFRAME PERIOD         median {:.3} ms, WORST {:.3} ms  (target {})",
            r.buckets.period.median(),
            r.buckets.period.max(),
            match r.governor.period() {
                Some(p) => format!("{:.3} ms", p.as_secs_f64() * 1000.0),
                None => "NONE (governor off)".to_string(),
            }
        );
    }

    println!("\n-- the fine trim: emulated frames the audio ring asked for, per iteration --");
    let total: u64 = r.frames_per_iter.iter().sum();
    for (n, count) in r.frames_per_iter.iter().enumerate() {
        println!(
            "{n} frame(s)            {count:>8}  ({:.3}%)",
            if total > 0 {
                *count as f64 * 100.0 / total as f64
            } else {
                0.0
            }
        );
    }

    println!("\n-- governor (the coarse rate limit) --");
    println!(
        "rebases              {}   <-- iterations that started a whole frame or more late",
        r.governor.rebases()
    );
    println!(
        "early wakes          {}   <-- repaints turned away before their deadline",
        r.governor.early_wakes()
    );
    println!(
        "worst lateness       {:.3} ms",
        r.governor.worst_late().as_secs_f64() * 1000.0
    );

    println!("\n-- per-iteration cost, milliseconds --");
    println!("{}", Series::header());
    for (name, s) in [
        ("emulate", &r.buckets.emulate),
        ("audio", &r.buckets.audio),
        ("convert", &r.buckets.convert),
        ("tex-upload", &r.buckets.upload),
        ("ui-build", &r.buckets.ui),
        ("tessellate", &r.buckets.tessellate),
        ("bus-pump", &r.buckets.bus),
        ("CPU TOTAL", &r.buckets.cpu_total),
        ("period", &r.buckets.period),
    ] {
        println!("{}", s.row(name));
    }

    match r.machine.device() {
        Some(d) => print_audio(d),
        None if r.wanted_audio => {
            println!("\n-- audio --");
            println!(
                "NOT MEASURED: audio was REQUESTED and no usable output device exists here. The pacing \
                 verdict is UNAVAILABLE for this run, not favourable."
            );
        }
        None => {
            println!("\n-- audio --");
            println!("switched off for this pass (--audio off), so nothing paced the emulator but the governor.");
        }
    }
    println!("=============================================================\n");
}

fn print_audio(d: &Device) {
    use ringbuf::traits::Observer;
    let c = d.counters();
    let cb = c.callbacks.load(Ordering::Relaxed);
    let starved = c.starved.load(Ordering::Relaxed);
    let steady = c.starved_steady.load(Ordering::Relaxed);
    let lost = c.starved_samples.load(Ordering::Relaxed);
    let minocc = c.min_occupancy.load(Ordering::Relaxed);

    println!("\n-- audio (real device) --");
    println!("device               {} Hz, {} ch", d.rate(), d.channels());
    println!(
        "ring capacity        {} samples ({} frames), low-water {} frames",
        d.ring_capacity(),
        crate::audio::RING_FRAMES,
        crate::pacing::RENDER_LOW_WATER_FRAMES
    );
    println!("callbacks            {cb}");
    println!("STARVED callbacks    {}", starved_total_line(starved, cb));
    println!("  of which STEADY    {}", steady_verdict_line(steady, cb));
    println!(
        "starved samples      {lost} ({:.1} ms of inserted silence)",
        lost as f64 * 500.0 / d.rate().max(1) as f64
    );
    println!(
        "leanest ring         {}",
        leanest_ring_line(minocc, d.rate())
    );
    println!(
        "producer DROPS       {} samples (ring full)   <-- the OTHER failure direction",
        d.dropped()
    );
    println!("ring at exit         {} samples", d.prod().occupied_len());
}

/// The **total-starvation** row.
///
/// `callbacks == 0` is not a clean run — it is no run at all: the device opened and the host never called
/// back, so there is no denominator. The row used to substitute `0.0` for the missing quotient, printing
/// `0 total (0.0000%)`: the most *favourable* value the row can take, for a pass that measured nothing.
/// [`leanest_ring_line`] had the same fault pointing the other way, and both are the doctrine this crate
/// states two files apart — never render "could not measure" as a number.
fn starved_total_line(starved: u64, callbacks: u64) -> String {
    if callbacks == 0 {
        return "NOT MEASURED: the device opened and never called back, so nothing paced this run and \
                there is no starvation rate to quote"
            .to_string();
    }
    format!(
        "{starved} total ({:.4}%)",
        starved as f64 * 100.0 / callbacks as f64
    )
}

/// The **pacing verdict** row: steady-state starvations, excluding [`WARMUP_CALLBACKS`] of warm-up.
///
/// The window opens on the callback whose 0-based index first reaches [`WARMUP_CALLBACKS`] (see the
/// `index >= WARMUP_CALLBACKS` arms in [`crate::device`]), so it takes **more than** `WARMUP_CALLBACKS`
/// callbacks to produce any verdict at all. Below that the counter reads `0` for want of a window, which
/// is indistinguishable from `0` for want of a starvation — and this row is labelled *the pacing verdict*,
/// so the two must not share a spelling.
fn steady_verdict_line(steady: u64, callbacks: u64) -> String {
    let steady_callbacks = callbacks.saturating_sub(WARMUP_CALLBACKS);
    if steady_callbacks == 0 {
        return format!(
            "NOT MEASURED: {callbacks} callback(s) ran and the first {WARMUP_CALLBACKS} are warm-up, so \
             the steady-state window never opened. This run has NO pacing verdict, not a favourable one"
        );
    }
    format!(
        "{steady}   <-- the pacing verdict, over {steady_callbacks} steady callback(s) \
         (warm-up = first {WARMUP_CALLBACKS}, excluded)"
    )
}

/// The **leanest steady-state ring occupancy** row.
///
/// [`MIN_OCCUPANCY_UNMEASURED`] means no steady-state callback has run, and this row used to render it as
/// `0 samples (0.0 ms)` — "the ring hit rock bottom", the most alarming reading the statistic can produce,
/// for a run that never took the reading. Any pass shorter than the warm-up window ended here, with every
/// neighbouring figure guarding loudly.
///
/// A **genuine** `0` still prints as `0 samples`: the ring really can empty, and that reading is the one
/// this row exists for.
fn leanest_ring_line(minocc: u64, rate: u32) -> String {
    if minocc == MIN_OCCUPANCY_UNMEASURED {
        return format!(
            "NOT MEASURED: no steady-state callback ran (the first {WARMUP_CALLBACKS} are warm-up), so \
             the ring's low-water mark is UNKNOWN, not zero"
        );
    }
    format!(
        "{minocc} samples ({:.1} ms)",
        minocc as f64 * 500.0 / rate.max(1) as f64
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H11. The three audio rows whose "never measured" state had a numeric spelling, each checked
    /// against **both** a real reading and the sentinel — the sentinel arm alone would pass on a row that
    /// had stopped reporting anything at all.
    #[test]
    fn the_unmeasured_audio_rows_say_so_instead_of_quoting_a_number() {
        // (a) leanest ring: the sentinel is an ABSENCE, and it used to print as `0 samples (0.0 ms)`.
        let unmeasured = leanest_ring_line(MIN_OCCUPANCY_UNMEASURED, 48_000);
        assert!(
            unmeasured.contains("NOT MEASURED"),
            "the sentinel must announce itself, got {unmeasured:?}"
        );
        assert!(
            !unmeasured.contains("0 samples"),
            "and must not be spelled as the worst real reading, got {unmeasured:?}"
        );

        // The control: a ring that genuinely emptied still reads as empty, so the fix is the sentinel
        // branch and not the row.
        assert_eq!(leanest_ring_line(0, 48_000), "0 samples (0.0 ms)");
        assert_eq!(leanest_ring_line(4_800, 48_000), "4800 samples (50.0 ms)");

        // (b) the pacing verdict. The boundary is DERIVED from `device`'s own `index >= WARMUP_CALLBACKS`
        // on a 0-based index: `WARMUP_CALLBACKS` callbacks leave the window shut, one more opens it.
        let shut = steady_verdict_line(0, WARMUP_CALLBACKS);
        assert!(
            shut.contains("NOT MEASURED") && shut.contains("NO pacing verdict"),
            "a run that never left warm-up has no verdict, got {shut:?}"
        );
        assert!(
            steady_verdict_line(0, 0).contains("NOT MEASURED"),
            "and neither does a run with no callbacks at all"
        );
        let open = steady_verdict_line(0, WARMUP_CALLBACKS + 1);
        assert!(
            !open.contains("NOT MEASURED") && open.contains("the pacing verdict"),
            "one steady callback IS a verdict, got {open:?}"
        );
        assert!(
            open.contains("over 1 steady callback"),
            "and it says how thin it is, got {open:?}"
        );

        // (c) the starvation rate, whose missing denominator used to print as the most FAVOURABLE value.
        let none = starved_total_line(0, 0);
        assert!(
            none.contains("NOT MEASURED"),
            "no callbacks is no rate, got {none:?}"
        );
        assert!(
            !none.contains('%'),
            "and must not quote a percentage of nothing, got {none:?}"
        );
        assert_eq!(starved_total_line(3, 1_000), "3 total (0.3000%)");
        assert_eq!(
            starved_total_line(0, 1_000),
            "0 total (0.0000%)",
            "a measured zero is still a zero"
        );
    }
}
