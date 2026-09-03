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

use crate::device::{Device, WARMUP_CALLBACKS};
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
            "presented frame rate REFUSED — this run rasterises in software (llvmpipe on Xvfb) on the \
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
        None => println!("last picture         NONE — THE RUN NEVER COMPLETED A FRAME; ignore every number above"),
    }

    // The headline, in the two numbers a mean would have hidden.
    if !r.governor.is_paced() {
        println!(
            "\n*** GOVERNOR OFF — this is the CONTROL run, the toolkit spike's arrangement with layer 1 \
             removed. Nothing below is the player's behaviour. ***"
        );
    }
    if r.buckets.period.is_empty() {
        println!("\nFRAME PERIOD         NOT MEASURED — fewer than two frame-owning iterations completed");
    } else {
        println!(
            "\nFRAME PERIOD         median {:.3} ms, WORST {:.3} ms  (target {})",
            r.buckets.period.median(),
            r.buckets.period.max(),
            match r.governor.period() {
                Some(p) => format!("{:.3} ms", p.as_secs_f64() * 1000.0),
                None => "NONE — governor off".to_string(),
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
                "NOT MEASURED — audio was REQUESTED and no usable output device exists here. The pacing \
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
    println!(
        "STARVED callbacks    {starved} total ({:.4}%)",
        if cb > 0 {
            starved as f64 * 100.0 / cb as f64
        } else {
            0.0
        }
    );
    println!(
        "  of which STEADY    {steady}   <-- the pacing verdict (warm-up = first {WARMUP_CALLBACKS} callbacks, excluded)"
    );
    println!(
        "starved samples      {lost} ({:.1} ms of inserted silence)",
        lost as f64 * 500.0 / d.rate().max(1) as f64
    );
    println!(
        "leanest ring         {} samples ({:.1} ms)",
        if minocc == u64::MAX { 0 } else { minocc },
        if minocc == u64::MAX {
            0.0
        } else {
            minocc as f64 * 500.0 / d.rate().max(1) as f64
        }
    );
    println!(
        "producer DROPS       {} samples (ring full)   <-- the OTHER failure direction",
        d.dropped()
    );
    println!("ring at exit         {} samples", d.prod().occupied_len());
}
