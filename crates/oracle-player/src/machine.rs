//! The emulator, wrapped as **one self-contained step** that takes a pad and returns a picture.
//!
//! Nothing in this module's public surface mentions egui, eframe, winit or wgpu. That is deliberate and it
//! is the seam named in [`crate::pacing`]'s docs: if a later parcel's debug panel is ever measured to stall
//! the UI thread, [`Machine`] moves to its own thread behind a frame channel, and only `main.rs` and
//! `ui.rs` change. Keeping the toolkit out of here is the cheap half of that option; taking it is the
//! expensive half, and it should not be paid before something measures that it is needed.

use oracle_core::bus::Fanout;
use oracle_core::io::Pad;
use oracle_core::render::LayerMask;
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
    /// ⚑ **The display mask [`image`](Machine::image) was produced under** — `None` before the first
    /// picture exists.
    ///
    /// Carried beside the picture rather than derived from the bus, because it is a fact about *the pixels
    /// that are there*, and the bus's mask is a fact about what the machine has been told. They agree on
    /// every ordinary frame and they are two different things: the whole of S2a is closing the gap between
    /// them, and the residual gap — a mask changed after this picture was made — is what
    /// [`crate::screen_pick`] must refuse on rather than describe.
    ///
    /// Every write to `image` writes this in the same statement, which is what keeps it from becoming a
    /// second opinion about the picture.
    image_mask: Option<LayerMask>,
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
            image_mask: None,
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
                // The captured frame was composited line by line by `Vdp::render_scanline`, which takes no
                // mask and must never gain one — so what came out is the unmasked picture, whatever the
                // engine's mask happens to say. [`crate::bus::drain`] re-derives it under the mask
                // afterwards when one is set; this records what is actually here until it does.
                self.image_mask = Some(LayerMask::ALL);
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

    /// **The machine this wraps was replaced under the player** — a client's `emulator/restore`,
    /// `emulator/reload_rom` or `emulator/reset`, applied inside [`crate::bus::Bus::mirror_pause`]'s drain
    /// between two of this window's own frames. Put back in step everything this struct derives from a
    /// machine rather than reads back out of one.
    ///
    /// Two things qualify, and the list is short **because most of what this window shows is already read
    /// live**: the status strip's `frame (emulated)` is `mclk / MCLK_PER_FRAME` off the machine itself, and
    /// every panel asks the bus in the draw pass it renders. Neither can go stale, so neither is here.
    ///
    /// 1. **The scanline capture.** Normally empty at this point — [`Machine::step`] clears it on every
    ///    frame boundary — but a run that a breakpoint ended *mid-frame* leaves real lines buffered for a
    ///    frame that never completed. Those lines belong to the machine that has just been thrown away, and
    ///    kept here they would be spliced onto the replacement's first lines and handed to
    ///    [`capture_to_image`] as one frame: a picture made of two timelines, which is a believable wrong
    ///    answer rather than a visible fault.
    /// 2. **The audio ring and its clock** ([`crate::device::Device::resync`]) — the severe one, and the one
    ///    a caller that resynchronised for `restore` and not for `reset` would miss. See that method.
    ///
    /// ⚑ **`frames` is deliberately NOT advanced here, and the frontend's equivalent is.** `oracle-frontend`
    /// adds `PumpReport::frames_advanced()` to its `draws` tally, because that tally is the only frame
    /// coordinate its title bar shows. This window shows two rows and they are already right: `frame
    /// (emulated)` is derived live from the machine's own clock (`crate::ui::StatusStrip::of`), so a
    /// client-driven run is on the glass with no help from here — and the other row is labelled *"frames run
    /// (player)"*, which is what [`Machine::frames`] means and what `crate::report` divides by elapsed
    /// seconds to state this loop's throughput. Adding a client's frames to it would falsify the label and
    /// corrupt the measurement to fix a coordinate that was never wrong. [`Machine::pictures`] is left alone
    /// for the same reason: it counts conversions this loop performed.
    pub fn resync_after_replacement(&mut self) {
        self.cap.clear();
        if let Some(d) = self.device.as_mut() {
            d.resync();
        }
    }

    /// **Take a whole machine in place of this one, and put the window back in step in the same
    /// statement** — the save-state load (S3, [`crate::states`]).
    ///
    /// One method rather than a `system_mut()` assignment beside a
    /// [`resync_after_replacement`](Machine::resync_after_replacement) call, because those two lines are
    /// separable and the separation is silent: a restore that skipped the resync would leave the audio
    /// sink's frame clock sitting *above* the restored one, which renders nothing at all until the machine
    /// catches back up — indefinite silence, from a missing line, with the picture still moving. There is
    /// no order in which one of these happens without the other because there is only one call.
    ///
    /// The retained picture is deliberately **kept**. `oracle-frontend` says why: the capture is cleared
    /// here, so the very next completed frame is unambiguously the restored one, and until it arrives the
    /// old image staying up is exactly what an iteration that emulated nothing does. Blanking instead
    /// would put one frame of black on the glass for no reason a person could name.
    ///
    /// [`image_mask`](Machine::image_mask) is left with the picture it describes, for the same reason: it
    /// is a fact about *those pixels*, and the pixels have not changed.
    pub fn adopt_system(&mut self, sys: System) {
        self.sys = sys;
        self.resync_after_replacement();
    }

    /// **Take the picture a client's own run drew**, from the bus's latched frame, and put it on the glass.
    ///
    /// This is what [`crate::bus::Bus::mirror_pause`]'s `screen_changed` is for. The frames a client runs
    /// through `emulator/run_frames` (or `step`, or `run_to`) are run by the *engine*, against the engine's
    /// own scanline capture — this crate's `cap` is not attached to them — so the completed frame exists
    /// only there. Without this the window keeps showing the last picture its own loop drew, which for a
    /// **paused** player (and a client must pause the player to run anything at all: §6's run-control state
    /// rule) is every frame the client will ever ask for.
    ///
    /// Returns whether a picture was taken. `false` covers the case the `PumpReport` doc names explicitly —
    /// the drain *invalidated* the picture rather than redrawing it (a restore, a ROM reload), leaving
    /// nothing to present — and there the retained image stays up exactly as it does for an iteration that
    /// emulated nothing.
    ///
    /// The height is taken from the data (`rgb.len() / width`) rather than assumed to be [`HEIGHT`]: the
    /// engine builds this frame with its own `ACTIVE_LINES`, and a mismatched constant here would be a
    /// silently sheared picture rather than a failure.
    pub fn adopt_frame(&mut self, width: usize, rgb: &[oracle_aether::engine::Rgb]) -> bool {
        if width == 0 || rgb.is_empty() || !rgb.len().is_multiple_of(width) {
            return false;
        }
        let height = rgb.len() / width;
        self.image = Some(egui::ColorImage {
            size: [width, height],
            source_size: egui::vec2(width as f32, height as f32),
            pixels: rgb
                .iter()
                .map(|&(r, g, b)| egui::Color32::from_rgb(r, g, b))
                .collect(),
        });
        // Unmasked: `Host::framebuffer` hands over the engine's *latched* frame, which the engine composed
        // with `render_scanline` during the client's own run. [`crate::bus::drain`] re-derives under the
        // mask immediately after this call when one is set, so the two pixel sources end up under one rule
        // — which is the split `Bus::framebuffer`'s doc used to have to warn about.
        self.image_mask = Some(LayerMask::ALL);
        true
    }

    /// ⚑ **Re-derive the picture under a display mask** — S2a, and the player's equivalent of
    /// `oracle-frontend`'s `blit_masked`. Returns whether a picture came out.
    ///
    /// # Why a masked picture cannot be made out of the captured one
    ///
    /// The capture's rows were composited line by line during the run by
    /// [`Vdp::render_scanline`](oracle_core::vdp::Vdp::render_scanline) — **the one render that commits the
    /// sprite-overflow and collision latches and the R10 carry, which is why it takes no mask and has no
    /// masked twin** (`docs/OVERSEER.md`'s LAYER-MASK entry: *"a display mask cannot perturb emulation" is
    /// enforced by the type system*). This slice adds no mask parameter to it and none may ever be added.
    ///
    /// What the capture leaves behind is decoded colours with the losing layers **already discarded**, so
    /// "mask" applied to those bytes could only mean "paint over" — and painting the backdrop over dots
    /// plane B was visible at is the believable-wrong-answer this whole surface exists to avoid. So a masked
    /// picture is re-derived from VDP state through
    /// [`render_line_masked`](oracle_core::vdp::Vdp::render_line_masked), exactly as `emulator/screenshot`
    /// does under a mask and exactly as the minifb window does.
    ///
    /// # What it costs, stated because it is visible on the glass
    ///
    /// This is a post-hoc read of whatever CRAM holds *now*, so every mid-frame palette effect the capture
    /// exists to preserve (S3K's underwater split is the loud one) is gone for as long as a mask is set: the
    /// water renders in the above-water palette. That is the same trade the bus makes and announces as
    /// `source: "stateRender"`, and it is the second reason the window must **say** a mask is on rather than
    /// let it be inferred — the picture changes in a way the toggle did not ask for. Clearing the mask puts
    /// the captured frame back on the next completed frame; nothing is discarded.
    pub fn render_masked(&mut self, mask: LayerMask) -> bool {
        let vdp = self.sys.vdp();
        // Line 0 is rendered twice rather than held: `render_line_masked` is `&self` and pure, the cost is
        // one line out of 224, and threading the first row through as a special case is how an off-by-one
        // between the width probe and the render gets written. `oracle-frontend`'s `blit_masked` makes the
        // same call for the same reason.
        let width = vdp.render_line_masked(0, mask).len();
        if width == 0 {
            // Loud-on-unmeasurable's floor: no picture rather than a black rectangle presented as one. The
            // retained image and its mask are both left alone, so the caller can see that the glass and the
            // bus disagree instead of being handed a fabricated agreement.
            return false;
        }
        let mut pixels = Vec::with_capacity(width * HEIGHT);
        for line in 0..HEIGHT as u16 {
            let row = vdp.render_line_masked(line, mask);
            for x in 0..width {
                let (r, g, b) = row.get(x).copied().unwrap_or((0, 0, 0));
                pixels.push(egui::Color32::from_rgb(r, g, b));
            }
        }
        self.image = Some(egui::ColorImage {
            size: [width, HEIGHT],
            source_size: egui::vec2(width as f32, HEIGHT as f32),
            pixels,
        });
        self.image_mask = Some(mask);
        true
    }

    /// The last completed picture, or `None` before the first frame finishes.
    pub fn image(&self) -> Option<&egui::ColorImage> {
        self.image.as_ref()
    }

    /// **The mask [`image`](Machine::image) was actually drawn under**, or `None` before there is one.
    ///
    /// Read by the uploader so the Screen tab can compare *what is on the glass* against what the bus says
    /// the mask is. Those are the same on every ordinary frame; where they are not, the panel says so and
    /// refuses rather than describing a picture that is not there.
    pub fn image_mask(&self) -> Option<LayerMask> {
        self.image_mask
    }

    /// Lines the scanline capture is holding right now — **for the tests that prove
    /// [`resync_after_replacement`] cleared it**, and for nothing else.
    ///
    /// A run that ends on a frame boundary leaves this at 0 all by itself, so a test that only asserted
    /// "0 afterwards" would be green against a `resync_after_replacement` that did nothing whatsoever. The
    /// tests read it *before* as well, and that reading is what makes the one after mean something.
    #[cfg(test)]
    pub fn capture_lines(&self) -> usize {
        self.cap.lines().len()
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
