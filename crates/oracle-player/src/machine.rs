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

    /// Run this iteration's emulated frames — however many the **audio ring** asks for — with `pad` on
    /// port 1, drain the synth into the ring, and convert the completed picture.
    ///
    /// `set_pad` is called once per iteration, before the run, exactly as the minifb player does: the pad
    /// is the state of the keys at the top of the frame and is held for every frame the iteration runs.
    pub fn step(&mut self, pad: Pad) -> StepCost {
        self.sys.set_pad(0, pad);
        self.sys.set_pad(1, Pad::default());

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
            match self.device.as_mut() {
                Some(d) => {
                    let mut sink = Fanout::new(&mut self.cap, d.sink_mut());
                    self.sys.run_frames_with_sink(1, &mut sink);
                }
                None => {
                    self.sys.run_frames_with_sink(1, &mut self.cap);
                }
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
            }
            convert += t2.elapsed();

            if self.cap.frames_completed() >= 1 && self.cap.lines().len() == HEIGHT {
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
