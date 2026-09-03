//! The emulator, wrapped as **one self-contained step** that takes a pad and returns a picture.
//!
//! Nothing in this module's public surface mentions egui, eframe, winit or wgpu. That is deliberate and it
//! is the seam named in [`crate::pacing`]'s docs: if a later parcel's debug panel is ever measured to stall
//! the UI thread, [`Machine`] moves to its own thread behind a frame channel, and only `main.rs` and
//! `ui.rs` change. Keeping the toolkit out of here is the cheap half of that option; taking it is the
//! expensive half, and it should not be paid before something measures that it is needed.

use oracle_core::bus::Fanout;
use oracle_core::io::Pad;
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;
use std::time::{Duration, Instant};

use crate::device::Device;

/// Active display height, matching `crates/oracle-frontend/src/main.rs`.
pub const HEIGHT: usize = 224;

/// The ceiling on the capture's per-delivery line log (~215 KB per emulated second, unbounded by design),
/// for runs that keep ending mid-frame. Same value as `oracle-frontend/src/main.rs`'s, same reason.
const MAX_CAPTURE_LINES: usize = 8 * HEIGHT;

/// What one iteration cost, in milliseconds, split the way the toolkit spike split it so the two sets of
/// numbers can be read side by side.
#[derive(Clone, Copy, Debug, Default)]
pub struct StepCost {
    pub emulate: f64,
    pub audio: f64,
    pub convert: f64,
    /// Emulated frames this iteration actually ran (0, 1 or 2 — the audio ring decides).
    pub frames: usize,
}

/// The emulator plus its pixel path and its audio path.
pub struct Machine {
    sys: System,
    cap: ScanlineCapture,
    device: Option<Device>,
    /// The last completed picture, retained so an iteration that emulates nothing (a governor early-wake,
    /// or the ring asking for a skip) re-presents it instead of flashing black. Exactly what the minifb
    /// player does with its `buf`.
    image: Option<egui::ColorImage>,
    /// Consecutive iterations the ring has answered 0 for — the [`crate::audio::MAX_CONSECUTIVE_SKIPS`]
    /// safety valve's counter.
    skips: usize,
    frames: u64,
    pictures: u64,
}

impl Machine {
    pub fn new(rom: Vec<u8>, device: Option<Device>) -> Self {
        let mut sys = System::new(0x5EED);
        sys.load_rom(rom);
        sys.reset();
        Self {
            sys,
            cap: ScanlineCapture::new(Retain::LastFrame),
            device,
            image: None,
            skips: 0,
            frames: 0,
            pictures: 0,
        }
    }

    /// Run this iteration's emulated frames — however many the **audio ring** asks for — with `human` on
    /// the two pads, drain the synth into the ring, and convert the completed picture.
    ///
    /// `set_pad` is called once per iteration, before the run, exactly as the minifb player does: the pad
    /// is the state of the keys at the top of the frame and is held for every frame the iteration runs.
    ///
    /// # ⚑ The pads are the human's OR the client's, and the merge happens HERE
    ///
    /// `human` is what the keyboard says. What reaches `System::set_pad` is that OR-ed, per button, with
    /// the set a client is holding through `emulator/hold`
    /// ([`Bus::merge_held`](crate::bus::Bus::merge_held)), and the human's own pads are published to the
    /// bus first ([`Bus::set_live_pads`](crate::bus::Bus::set_live_pads)) so the engine's own pad writes
    /// compose with them instead of erasing them. Neither side can suppress the other; that is the OR.
    ///
    /// **It is here rather than in `Loop::iterate`, where `oracle-frontend` does it, for two reasons.**
    /// These two lines are the *only* place this crate writes a pad into the `System`, so no path can grow
    /// that bypasses the merge — and `iterate` needs a window, so a merge placed there is a merge no test
    /// in this crate could ever execute. A seam that can only be exercised behind a GUI is a seam that is
    /// asserted rather than shown.
    ///
    /// Port 1 is no longer hardcoded to [`Pad::default`] for the same reason: `Host::held(1)` is real, and
    /// a merge that dropped it would show a held set in the status strip that never reached the machine.
    ///
    /// # ⚑ The bus rides every emulated frame (parcel 3)
    ///
    /// `bus` is not decoration and it is not optional. Three of its four contributions are invisible until
    /// something is armed, and the fourth changes what a frame *is*:
    ///
    /// * [`Bus::run_sinks`](crate::bus::Bus::run_sinks) lends the run the engine's own watch and profiler,
    ///   `Observe`-wrapped, plus the breakpoint sink bare. Unarmed, all three come back `None`, whose sink
    ///   impl wants nothing and does nothing — which is why there is no "is anything armed" branch here.
    /// * The breakpoint sink can **end the run mid-frame**. That is the whole point, and it is why the
    ///   capture-lifecycle check below already tolerates a run that did not complete a frame.
    /// * [`break_observed`](crate::bus::break_observed) consumes the sink — releasing its borrow of `bus` —
    ///   and [`record_break`](crate::bus::Bus::record_break) latches the halt for the loop's drain.
    /// * [`publish`](crate::bus::Bus::publish) hands over the completed frame *before* the capture is
    ///   cleared, because that clear drops the pixels along with the line log.
    ///
    /// **All of it is inside the `emulate` bucket.** `run_sinks` and `record_break` are new per-frame work
    /// on the emulation path, and timing only `run_frames_with_sink` would have made this parcel's own
    /// re-measurement structurally unable to see the thing it was retaken to price.
    pub fn step(&mut self, human: [Pad; 2], bus: &mut crate::bus::Bus) -> StepCost {
        bus.set_live_pads(human);
        let pads = bus.merge_held(human);
        self.sys.set_pad(0, pads[0]);
        self.sys.set_pad(1, pads[1]);

        let n = match self.device.as_ref() {
            Some(d) => crate::pacing::frames_to_run_for(d.prod(), d.frame_samples(), self.skips),
            // No device — nothing to pace against, so run at the governor's rate, exactly as the minifb
            // player does in a no-audio build.
            None => 1,
        };
        self.skips = if n == 0 { self.skips + 1 } else { 0 };

        let mut cost = StepCost {
            frames: n,
            ..Default::default()
        };
        let mut emulate = Duration::ZERO;
        let mut audio_t = Duration::ZERO;
        let mut convert = Duration::ZERO;

        for _ in 0..n {
            let t0 = Instant::now();
            // Read *before* the run: the breakpoint sink suppresses a re-fire at the PC the run started
            // on until one instruction retires, without which a machine halted at a breakpoint could
            // never be resumed past it. The engine cannot read this for itself — outside a `pump` drain
            // it holds a placeholder `System` whose PC is 0.
            let resume_pc = self.sys.cpu_regs().pc;
            let (watch, prof, mut brk) = bus.run_sinks(resume_pc);
            let instruments = Fanout::new(watch, prof);
            // The breakpoint sink rides in the OUTER `Fanout`, beside the capture, in both arms: the stop
            // signal is then composed by a plain `Fanout` in every build variant and cannot be dropped by
            // an intervening combinator.
            match self.device.as_mut() {
                Some(d) => {
                    let mut sink = Fanout::new(
                        &mut self.cap,
                        Fanout::new(&mut brk, Fanout::new(d.sink_mut(), instruments)),
                    );
                    self.sys.run_frames_with_sink(1, &mut sink);
                }
                None => {
                    let mut sink = Fanout::new(&mut self.cap, Fanout::new(&mut brk, instruments));
                    self.sys.run_frames_with_sink(1, &mut sink);
                }
            }
            // Consuming the sink is what releases its borrow of `bus`, which is why this is a free
            // function and not a method (see `bus::break_observed`).
            if let Some(addr) = crate::bus::break_observed(brk) {
                bus.record_break(addr);
            }
            emulate += t0.elapsed();
            self.frames += 1;

            if let Some(d) = self.device.as_mut() {
                let t1 = Instant::now();
                d.push_frame();
                audio_t += t1.elapsed();
            }

            let t2 = Instant::now();
            if let Some(img) = capture_to_image(&self.cap) {
                self.image = Some(img);
                self.pictures += 1;
                // Published on exactly the frames that produced a picture, and *before* the clear below,
                // which drops the retained pixels along with the line log.
                bus.publish(&self.cap);
            }
            convert += t2.elapsed();

            // The normal case is a run that ended cleanly on the frame boundary. **Parcel 3 makes the other
            // case reachable for the first time**: with the breakpoint sink attached a run can end
            // mid-frame, leaving real pixels buffered for a frame that has not completed, which must be
            // left to finish. Before this parcel `run_frames_with_sink(1, ..)` carried nothing that could
            // stop it, so this branch could not be taken and the player needed no bound; it needs one now.
            // Copied from `oracle-frontend/src/main.rs`, constant and all, because it is the same hazard.
            let ended_on_a_frame_boundary =
                self.cap.frames_completed() >= 1 && self.cap.lines().len() == HEIGHT;
            if ended_on_a_frame_boundary || self.cap.lines().len() >= MAX_CAPTURE_LINES {
                self.cap.clear();
            }
        }

        cost.emulate = ms(emulate);
        cost.audio = ms(audio_t);
        cost.convert = ms(convert);
        cost
    }

    /// The last completed picture, or `None` before the first frame finishes.
    pub fn image(&self) -> Option<&egui::ColorImage> {
        self.image.as_ref()
    }

    pub fn cpu_regs(&self) -> &oracle_core::m68000::Registers {
        self.sys.cpu_regs()
    }

    /// The machine itself, read-only — what a panel derives from when it needs more than one accessor's
    /// worth (`crate::ui::StatusStrip` reads the ROM length and the scheduler clock through it, by the
    /// same expressions `Engine::status` uses).
    ///
    /// Shared: what a panel body reads when it needs more than one accessor's worth.
    pub fn system(&self) -> &System {
        &self.sys
    }

    /// The machine, mutably — **for `Host::call` and `Host::pump` and nothing else** (parcel 2b).
    ///
    /// Parcel 2a said this was "deliberately not here, because this parcel does not host a `Host` in the
    /// running player". It hosts one now, and both entry points take `&mut System` because they *swap*
    /// the caller's machine into the engine for the duration of a dispatch and swap it straight back —
    /// the machine is lent, not moved, and the mutability is the swap's, not a handler's licence to run
    /// the CPU.
    ///
    /// It is still not a licence for a panel to advance the machine: nothing in `ui.rs` reaches
    /// `run_frames`, and the run-control methods that could are refused with `-32005 machineRunning`
    /// while the player's loop owns the clock — which is what the pause mirror in [`crate::bus`] is for.
    pub fn system_mut(&mut self) -> &mut System {
        &mut self.sys
    }

    pub fn device(&self) -> Option<&Device> {
        self.device.as_ref()
    }

    /// Emulated frames since reset.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Completed pictures since reset. Below [`Machine::frames`] only when a run ended mid-frame; above
    /// the *presented* count whenever an iteration ran two frames and the first was dropped from the
    /// display, which is the ordinary cost of audio-mastered pacing.
    pub fn pictures(&self) -> u64 {
        self.pictures
    }
}

/// The completed frame as egui pixels, or `None` if the run did not finish one.
///
/// This mirrors `crates/oracle-frontend/src/main.rs::blit_capture` — including its two non-obvious rules,
/// which are load-bearing and are restated because getting them wrong is silent:
///
/// * The completed frame is the **last `HEIGHT` deliveries**, and the sum check is what proves it: a run
///   that ended mid-frame leaves a *previous* frame in `pixels()` whose lines are no longer the tail of
///   the log.
/// * A frame is **not guaranteed rectangular** — a game can switch H32↔H40 part-way down, and S3K does on
///   the first frame after a soft reset. The display width is the width the frame *ended* on (what the VDP
///   is scanning out by V-Blank) and shorter lines are padded with black; rejecting such frames would
///   blank the window for as long as a game kept switching.
fn capture_to_image(cap: &ScanlineCapture) -> Option<egui::ColorImage> {
    let px = cap.pixels();
    let log = cap.lines();
    if px.is_empty() || log.len() < HEIGHT {
        return None;
    }
    let widths = &log[log.len() - HEIGHT..];
    if widths.iter().map(|&(_, w)| w).sum::<usize>() != px.len() {
        return None;
    }
    let width = widths[HEIGHT - 1].1;
    if width == 0 {
        return None;
    }
    let mut pixels = Vec::with_capacity(width * HEIGHT);
    let mut at = 0;
    for &(_, line_width) in widths {
        let line = &px[at..at + line_width];
        at += line_width;
        for x in 0..width {
            let (r, g, b) = line.get(x).copied().unwrap_or((0, 0, 0));
            pixels.push(egui::Color32::from_rgb(r, g, b));
        }
    }
    Some(egui::ColorImage {
        size: [width, HEIGHT],
        source_size: egui::vec2(width as f32, HEIGHT as f32),
        pixels,
    })
}

pub fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
