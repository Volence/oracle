//! THROWAWAY MEASUREMENT SPIKE — prices a player rebuild on egui + egui_dock + wgpu.
//!
//! It is **not** a player and must never become one. It exists to answer three questions with numbers, and
//! to be deleted afterwards:
//!
//! 1. What does one frame cost on the **CPU** under this toolkit, broken into emulate / audio / convert /
//!    texture-upload / UI-build / tessellate?
//! 2. What is the frame-time *distribution* (median and worst, not the mean — a mean hides a stutter)?
//! 3. Does the audio ring hold pace, and how many underruns / drops does it take?
//!
//! # The instrument problem
//!
//! This runs on a machine whose owner is using it. Two rules follow, and they shape the modes below.
//!
//! * **No window on his screen.** `--mode eframe` must be run against an `Xvfb` display the operator owns.
//!   It therefore *asks the toolkit for its own screen size* on the first frame and compares it against
//!   `--expect-screen WxH`; a mismatch means the process attached to the real compositor and it exits(2)
//!   without drawing. Set the expected geometry to something no real monitor is (see `run.sh`).
//! * **No sound on his speakers.** `--audio on` opens the real default device through the real
//!   `crates/oracle-frontend/src/audio.rs` path — the ring, the feedback loop, `fill_output` — but pushes
//!   every frame at **gain 0.0**. The ring dynamics, the pacing and the underrun counts are therefore the
//!   real ones; only the amplitude is zero.
//!
//! # Why there are three modes
//!
//! Under `Xvfb` the GL/Vulkan path is **llvmpipe**, a software rasteriser. It is not the machine's GPU, and
//! a presented-fps figure from it is not the number — worse, it burns CPU on the same cores as the emulator
//! and inflates every other measurement. So:
//!
//! * `--mode cpu` — **the answer to question 1.** No window, no GPU, no eframe: a bare `egui::Context`
//!   driven by hand at a 60 Hz deadline, running the identical per-frame pipeline (emulate → convert →
//!   `TextureHandle::set` → `DockArea::show_inside` → `Context::tessellate`). Every cost here is
//!   display-independent by construction.
//! * `--mode cpu-unpaced` — the same pipeline with the deadline removed, to read off headroom: how many
//!   such frames a second this machine can produce flat out.
//! * `--mode eframe` — the whole real stack (winit + wgpu) under `Xvfb`. Its per-part CPU numbers are
//!   contaminated by llvmpipe and its presented fps is **not** the answer; it is here to prove the stack
//!   assembles and runs, and to bound the CPU parts from above.

// The player's audio substrate, included verbatim rather than re-implemented, so "the same cpal path" is
// literally the same file. It is a binary crate over there (no lib target), so a `#[path]` module is the
// only way to share it. Its `#[cfg(test)] mod tests` is never compiled here.
#[allow(dead_code)]
#[path = "../../oracle-frontend/src/audio.rs"]
mod audio;

mod stats;

use oracle_core::bus::Fanout;
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use stats::Series;

/// Active display height, matching `crates/oracle-frontend/src/main.rs`.
const HEIGHT: usize = 224;
/// One 60 Hz frame period.
const FRAME_PERIOD: Duration = Duration::from_nanos(16_666_667);
/// Device callbacks treated as warm-up (the pre-roll reservoir filling), excluded from the steady-state
/// starvation count and from the leanest-ring figure. ~1.5 s at a 40 Hz callback rate.
const WARMUP_CALLBACKS: u64 = 60;

// ---------------------------------------------------------------------------------------------------
// Audio: the player's ring and callback, with the amplitude zeroed and the underruns counted.
// ---------------------------------------------------------------------------------------------------

/// Counters the real-time callback raises and the report reads. The callback may not allocate, lock or
/// print, so everything it says it says through these.
#[derive(Default)]
struct AudioCounters {
    /// Callbacks the device made.
    callbacks: AtomicU64,
    /// Callbacks that found the ring short of what they needed — i.e. that zero-filled a tail.
    starved_callbacks: AtomicU64,
    /// The same, counted only from [`WARMUP_CALLBACKS`] onward. The pre-roll is two frames of silence and
    /// the device's *first* callback can ask for a quantum several times that, so a starve at the very
    /// start is the reservoir filling, not the loop failing to keep pace. Reporting one number for both
    /// would be the believable wrong answer.
    starved_steady: AtomicU64,
    /// Total ring samples missing across all starved callbacks.
    starved_samples: AtomicU64,
    /// Ring occupancy, in samples, at the moment of the leanest callback. `u64::MAX` = no callback yet.
    min_occupancy: AtomicU64,
}

struct AudioState {
    sink: oracle_core::synth::AudioSink,
    prod: audio::AudioProd,
    frame_samples: usize,
    skips: usize,
    counters: Arc<AudioCounters>,
    /// Ring samples the producer could not push because the ring was full (the other end of the feedback
    /// loop from a starve).
    dropped: u64,
    rate: u32,
    channels: usize,
    _stream: cpal::Stream,
}

/// Open the real default output device and start a stream whose callback is the player's own
/// [`audio::fill_output`], wrapped only to count what it could not serve. Returns `None` — and says why —
/// on any failure, exactly as the player does.
fn build_audio() -> Option<AudioState> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use ringbuf::traits::Observer;

    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            eprintln!("audio: no default output device — audio measurement NOT POSSIBLE here");
            return None;
        }
    };
    let default_cfg = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "audio: no default output config ({e}) — audio measurement NOT POSSIBLE here"
            );
            return None;
        }
    };
    if default_cfg.sample_format() != cpal::SampleFormat::F32 {
        eprintln!(
            "audio: device sample format {:?} is not f32 — audio measurement NOT POSSIBLE here",
            default_cfg.sample_format()
        );
        return None;
    }
    let rate = default_cfg.sample_rate().0;
    let channels = default_cfg.channels() as usize;
    let config: cpal::StreamConfig = default_cfg.config();

    let sink = oracle_core::synth::AudioSink::new(rate);
    let (mut prod, mut cons) = audio::make_ring(rate);
    let frame_samples = audio::frame_samples(rate);
    audio::preroll_silence(&mut prod, frame_samples);

    let counters = Arc::new(AudioCounters::default());
    counters.min_occupancy.store(u64::MAX, Ordering::Relaxed);
    let cb_counters = Arc::clone(&counters);
    let flush = Arc::new(AtomicBool::new(false));

    let data_cb = move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
        // Ring samples this callback is about to want, given the device's channel count. `fill_output`
        // itself reports nothing, so the shortfall has to be read off the ring before it runs.
        let need = match channels {
            2 => out.len(),
            1 => out.len() * 2,
            ch => (out.len() / ch.max(1)) * 2,
        };
        let have = cons.occupied_len();
        audio::fill_output(&mut cons, out, channels, &flush);
        let index = cb_counters.callbacks.fetch_add(1, Ordering::Relaxed);
        if index >= WARMUP_CALLBACKS {
            cb_counters
                .min_occupancy
                .fetch_min(have as u64, Ordering::Relaxed);
        }
        if have < need {
            cb_counters
                .starved_callbacks
                .fetch_add(1, Ordering::Relaxed);
            cb_counters
                .starved_samples
                .fetch_add((need - have) as u64, Ordering::Relaxed);
            if index >= WARMUP_CALLBACKS {
                cb_counters.starved_steady.fetch_add(1, Ordering::Relaxed);
            }
        }
    };

    let stream = match device.build_output_stream::<f32, _, _>(
        &config,
        data_cb,
        |e| eprintln!("audio stream error: {e}"),
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "audio: failed to build output stream ({e}) — audio measurement NOT POSSIBLE here"
            );
            return None;
        }
    };
    if let Err(e) = stream.play() {
        eprintln!(
            "audio: failed to start output stream ({e}) — audio measurement NOT POSSIBLE here"
        );
        return None;
    }
    eprintln!(
        "audio: real device open at {rate} Hz / {channels} ch, PUSHED AT GAIN 0.0 (silent by construction)"
    );
    Some(AudioState {
        sink,
        prod,
        frame_samples,
        skips: 0,
        counters,
        dropped: 0,
        rate,
        channels,
        _stream: stream,
    })
}

// ---------------------------------------------------------------------------------------------------
// The docked panels
// ---------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Tab {
    /// The emulator picture — the one panel that carries the uploaded texture.
    Screen,
    /// A placeholder registers panel: a realistic amount of formatted text laid out per frame.
    Registers,
    /// A placeholder live-timings panel.
    Timings,
}

/// Everything the tab bodies read. Held separately from the `DockState` so both can be borrowed at once.
struct Panels<'a> {
    tex: Option<&'a egui::TextureHandle>,
    regs: &'a oracle_core::m68000::Registers,
    line: &'a str,
}

impl egui_dock::TabViewer for Panels<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(match tab {
            Tab::Screen => "screen",
            Tab::Registers => "registers",
            Tab::Timings => "timings",
        })
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            Tab::Screen => "Screen",
            Tab::Registers => "Registers",
            Tab::Timings => "Timings",
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Screen => {
                if let Some(tex) = self.tex {
                    let avail = ui.available_size();
                    let src = tex.size_vec2();
                    // Integer-ish aspect fit, the same job `present::dest_rect` does today.
                    let scale = (avail.x / src.x).min(avail.y / src.y).max(0.01);
                    ui.add(egui::Image::new(tex).fit_to_exact_size(src * scale));
                } else {
                    ui.label("no frame yet");
                }
            }
            Tab::Registers => {
                // A realistic register panel: 8 D, 8 A, PC/SR/USP/SSP — 20 monospace rows of formatted
                // text, rebuilt every frame. This is the shape of per-frame UI work a debug panel adds.
                egui::Grid::new("regs").num_columns(2).show(ui, |ui| {
                    for i in 0..8 {
                        ui.monospace(format!("D{i}"));
                        ui.monospace(format!("{:08X}", self.regs.d[i]));
                        ui.end_row();
                    }
                    // A0–A6; A7 lives in usp/ssp on this core.
                    for i in 0..7 {
                        ui.monospace(format!("A{i}"));
                        ui.monospace(format!("{:08X}", self.regs.a[i]));
                        ui.end_row();
                    }
                    ui.monospace("USP");
                    ui.monospace(format!("{:08X}", self.regs.usp));
                    ui.end_row();
                    ui.monospace("SSP");
                    ui.monospace(format!("{:08X}", self.regs.ssp));
                    ui.end_row();
                    ui.monospace("PC");
                    ui.monospace(format!("{:08X}", self.regs.pc));
                    ui.end_row();
                    ui.monospace("SR");
                    ui.monospace(format!("{:04X}", self.regs.sr));
                    ui.end_row();
                });
            }
            Tab::Timings => {
                ui.monospace(self.line);
                ui.separator();
                for i in 0..12 {
                    ui.monospace(format!("placeholder row {i:>3}  ................"));
                }
            }
        }
    }
}

fn initial_dock() -> egui_dock::DockState<Tab> {
    let mut dock = egui_dock::DockState::new(vec![Tab::Screen]);
    let surface = dock.main_surface_mut();
    let [_, right] = surface.split_right(egui_dock::NodeIndex::root(), 0.68, vec![Tab::Registers]);
    surface.split_below(right, 0.5, vec![Tab::Timings]);
    dock
}

// ---------------------------------------------------------------------------------------------------
// The engine: one frame's worth of work, timed part by part.
// ---------------------------------------------------------------------------------------------------

#[derive(Default)]
struct Buckets {
    emulate: Series,
    audio: Series,
    convert: Series,
    upload: Series,
    ui: Series,
    tessellate: Series,
    cpu_total: Series,
    period: Series,
}

struct Engine {
    sys: System,
    cap: ScanlineCapture,
    audio: Option<AudioState>,
    /// The last completed picture, as egui pixels. Kept so a 0-frame iteration re-presents it.
    image: Option<egui::ColorImage>,
    dock: egui_dock::DockState<Tab>,
    b: Buckets,
    emulated_frames: u64,
    drawn_frames: u64,
    iterations: u64,
    last_iter: Option<Instant>,
    status: String,
}

impl Engine {
    fn new(rom: Vec<u8>, want_audio: bool) -> Self {
        let mut sys = System::new(0x5EED);
        sys.load_rom(rom);
        sys.reset();
        Self {
            sys,
            cap: ScanlineCapture::new(Retain::LastFrame),
            audio: if want_audio { build_audio() } else { None },
            image: None,
            dock: initial_dock(),
            b: Buckets::default(),
            emulated_frames: 0,
            drawn_frames: 0,
            iterations: 0,
            last_iter: None,
            status: String::from("starting"),
        }
    }

    /// Emulate the frames this iteration owes (the player's own audio-ring feedback decides how many),
    /// drain the PCM into the ring, and convert the completed picture into a `ColorImage`.
    ///
    /// Returns `(emulate_ns, audio_ns, convert_ns)`.
    fn emulate_and_convert(&mut self) -> (f64, f64, f64) {
        let mut emulate = Duration::ZERO;
        let mut audio_t = Duration::ZERO;
        let mut convert = Duration::ZERO;

        let n = match self.audio.as_ref() {
            Some(a) => audio::frames_to_run_for(&a.prod, a.frame_samples, a.skips),
            None => 1,
        };
        if let Some(a) = self.audio.as_mut() {
            if n == 0 {
                a.skips += 1;
            } else {
                a.skips = 0;
            }
        }

        for _ in 0..n {
            let t0 = Instant::now();
            match self.audio.as_mut() {
                Some(a) => {
                    let mut sink = Fanout::new(&mut self.cap, &mut a.sink);
                    self.sys.run_frames_with_sink(1, &mut sink);
                }
                None => {
                    self.sys.run_frames_with_sink(1, &mut self.cap);
                }
            }
            emulate += t0.elapsed();
            self.emulated_frames += 1;

            if let Some(a) = self.audio.as_mut() {
                let t1 = Instant::now();
                let pcm = a.sink.drain();
                // Gain 0.0 — the ring, the feedback loop and the callback are the real ones; the sound is
                // not. See the module docs.
                a.dropped += audio::push_frame(&mut a.prod, &pcm, 0.0) as u64;
                audio_t += t1.elapsed();
            }

            let t2 = Instant::now();
            if let Some(img) = capture_to_image(&self.cap) {
                self.image = Some(img);
                self.drawn_frames += 1;
            }
            convert += t2.elapsed();

            if self.cap.frames_completed() >= 1 && self.cap.lines().len() == HEIGHT {
                self.cap.clear();
            }
        }
        (ms(emulate), ms(audio_t), ms(convert))
    }

    /// Upload the current picture into `tex` and draw the docked UI into `ctx`. Returns
    /// `(upload_ms, ui_ms, shapes)`; the caller tessellates so `--mode cpu` can time that separately and
    /// `--mode eframe` can leave it to the backend.
    fn upload(&mut self, ctx: &egui::Context, tex: &mut Option<egui::TextureHandle>) -> f64 {
        let Some(img) = self.image.as_ref() else {
            return 0.0;
        };
        let t = Instant::now();
        match tex {
            Some(h) => h.set(img.clone(), egui::TextureOptions::NEAREST),
            None => {
                *tex = Some(ctx.load_texture("screen", img.clone(), egui::TextureOptions::NEAREST))
            }
        }
        ms(t.elapsed())
    }

    /// Lay out the whole docked UI into `ui`, returning the milliseconds it took.
    ///
    /// **egui 0.36 is `Ui`-first, not `Context`-first.** `Context::run_ui` hands the closure a `&mut Ui`,
    /// panels take `show(ui, …)` rather than `show(ctx, …)`, `TopBottomPanel` is now `Panel::top`, and
    /// `eframe::App`'s per-frame method is `ui(&mut self, &mut Ui, &mut Frame)` rather than `update(…,
    /// &Context, …)`. That churn is itself a pricing input; it is recorded in the write-up.
    fn build_ui(&mut self, ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>) -> f64 {
        // Disjoint field borrows: `sys` immutably for the registers, `dock` mutably for the layout.
        let regs: &oracle_core::m68000::Registers = self.sys.cpu_regs();
        let line = self.status.clone();
        let dock = &mut self.dock;
        let t = Instant::now();
        egui::Panel::top("bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("oracle-panels-spike");
                ui.separator();
                ui.monospace(&line);
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let mut panels = Panels {
                    tex,
                    regs,
                    line: &line,
                };
                egui_dock::DockArea::new(dock)
                    .style(egui_dock::Style::from_egui(ui.style().as_ref()))
                    .show_inside(ui, &mut panels);
            });
        ms(t.elapsed())
    }
}

/// The completed frame as egui pixels, or `None` if the run did not finish one. Mirrors
/// `crates/oracle-frontend/src/main.rs::blit_capture`, but lands in `Color32` instead of `u32` ARGB —
/// which is exactly the conversion a toolkit rebuild would have to pay.
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

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

// ---------------------------------------------------------------------------------------------------
// Modes
// ---------------------------------------------------------------------------------------------------

struct Args {
    rom: String,
    secs: f64,
    mode: String,
    audio: bool,
    expect_screen: Option<(u32, u32)>,
}

fn parse_args() -> Args {
    let mut a = Args {
        rom: String::new(),
        secs: 60.0,
        mode: "cpu".into(),
        audio: true,
        expect_screen: None,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--rom" => {
                a.rom = argv[i + 1].clone();
                i += 2;
            }
            "--secs" => {
                a.secs = argv[i + 1].parse().expect("--secs");
                i += 2;
            }
            "--mode" => {
                a.mode = argv[i + 1].clone();
                i += 2;
            }
            "--audio" => {
                a.audio = argv[i + 1] == "on";
                i += 2;
            }
            "--expect-screen" => {
                let (w, h) = argv[i + 1].split_once('x').expect("--expect-screen WxH");
                a.expect_screen = Some((w.parse().unwrap(), h.parse().unwrap()));
                i += 2;
            }
            other => panic!("unknown flag {other}"),
        }
    }
    assert!(!a.rom.is_empty(), "--rom is required");
    a
}

fn main() {
    let args = parse_args();
    let rom = std::fs::read(&args.rom).expect("read rom");
    eprintln!(
        "spike: mode={} rom={} ({} bytes) secs={} audio={}",
        args.mode,
        args.rom,
        rom.len(),
        args.secs,
        args.audio
    );

    match args.mode.as_str() {
        "cpu" => run_cpu(rom, &args, true),
        "cpu-unpaced" => run_cpu(rom, &args, false),
        "eframe" => run_eframe(rom, &args),
        other => panic!("unknown --mode {other} (cpu | cpu-unpaced | eframe)"),
    }
}

/// The display-independent pass: a bare `egui::Context`, no window, no GPU, no eframe.
fn run_cpu(rom: Vec<u8>, args: &Args, paced: bool) {
    let mut eng = Engine::new(rom, args.audio);
    let ctx = egui::Context::default();
    let mut tex: Option<egui::TextureHandle> = None;

    // A window-sized viewport, so the docked layout and its text lay out at a realistic scale.
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 800.0));

    let start = Instant::now();
    let mut deadline = start;
    while start.elapsed().as_secs_f64() < args.secs {
        let now = Instant::now();
        if let Some(prev) = eng.last_iter {
            eng.b.period.push(ms(now - prev));
        }
        eng.last_iter = Some(now);
        let iter_t0 = now;

        let (emulate, audio_ms, convert) = eng.emulate_and_convert();

        // One `Context::run` covers upload + UI-build; the closure is where both happen, so the parts are
        // timed inside it and the tessellation is timed on the output afterwards.
        let mut upload = 0.0;
        let mut ui = 0.0;
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(start.elapsed().as_secs_f64()),
            ..Default::default()
        };
        eng.status = format!(
            "cpu-mode  frames={} drawn={}",
            eng.emulated_frames, eng.drawn_frames
        );
        let out = ctx.run_ui(raw, |root| {
            upload = eng.upload(root.ctx(), &mut tex);
            ui = eng.build_ui(root, tex.as_ref());
        });
        let t = Instant::now();
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        let tess = ms(t.elapsed());
        drop(prims);
        drop(out.textures_delta);

        eng.b.emulate.push(emulate);
        eng.b.audio.push(audio_ms);
        eng.b.convert.push(convert);
        eng.b.upload.push(upload);
        eng.b.ui.push(ui);
        eng.b.tessellate.push(tess);
        eng.b.cpu_total.push(ms(iter_t0.elapsed()));
        eng.iterations += 1;

        if paced {
            deadline += FRAME_PERIOD;
            let now = Instant::now();
            if deadline > now {
                std::thread::sleep(deadline - now);
            } else {
                // Fell behind — re-base rather than accumulate a debt that would then be sprinted off.
                deadline = now;
            }
        }
    }
    report(
        if paced {
            "cpu (paced 60 Hz)"
        } else {
            "cpu-unpaced"
        },
        &eng,
        start.elapsed().as_secs_f64(),
        None,
        args.audio,
    );
}

struct SpikeApp {
    eng: Engine,
    tex: Option<egui::TextureHandle>,
    start: Instant,
    secs: f64,
    expect_screen: Option<(u32, u32)>,
    checked_screen: bool,
    reported: bool,
    screen_seen: Option<(f32, f32)>,
    /// Whether audio was *asked for*. Distinguishes "no device here" (a reach limit worth reporting) from
    /// "switched off for this pass" (a choice).
    wanted_audio: bool,
}

impl eframe::App for SpikeApp {
    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        // --- The ownership check, before anything is drawn or driven. ---
        if !self.checked_screen {
            self.checked_screen = true;
            let monitor = ctx.input(|i| i.viewport().monitor_size);
            match (monitor, self.expect_screen) {
                (Some(m), Some((ew, eh))) => {
                    self.screen_seen = Some((m.x, m.y));
                    if (m.x.round() as u32, m.y.round() as u32) != (ew, eh) {
                        eprintln!(
                            "ABORT: the toolkit reports a {}x{} screen but this run demanded {ew}x{eh}. \
                             That is NOT the Xvfb display this run created — it is somebody's real \
                             compositor. Refusing to draw.",
                            m.x, m.y
                        );
                        std::process::exit(2);
                    }
                    eprintln!(
                        "display ownership CONFIRMED: toolkit reports {}x{}",
                        m.x, m.y
                    );
                }
                (None, Some(_)) => {
                    eprintln!(
                        "ABORT: the toolkit reports no monitor size, so display ownership cannot be \
                         verified. Refusing to draw."
                    );
                    std::process::exit(2);
                }
                _ => {}
            }
        }

        let now = Instant::now();
        if let Some(prev) = self.eng.last_iter {
            self.eng.b.period.push(ms(now - prev));
        }
        self.eng.last_iter = Some(now);

        let (emulate, audio_ms, convert) = self.eng.emulate_and_convert();
        let upload = self.eng.upload(ctx, &mut self.tex);
        self.eng.status = format!(
            "eframe  frames={} drawn={}",
            self.eng.emulated_frames, self.eng.drawn_frames
        );
        let ui = self.eng.build_ui(root, self.tex.as_ref());

        self.eng.b.emulate.push(emulate);
        self.eng.b.audio.push(audio_ms);
        self.eng.b.convert.push(convert);
        self.eng.b.upload.push(upload);
        self.eng.b.ui.push(ui);
        self.eng.b.tessellate.push(0.0); // owned by the backend in this mode
        self.eng
            .b
            .cpu_total
            .push(emulate + audio_ms + convert + upload + ui);
        self.eng.iterations += 1;

        // Free-run: never idle waiting for input.
        ctx.request_repaint();

        if self.start.elapsed().as_secs_f64() >= self.secs && !self.reported {
            self.reported = true;
            report(
                "eframe (Xvfb + llvmpipe)",
                &self.eng,
                self.start.elapsed().as_secs_f64(),
                self.screen_seen,
                self.wanted_audio,
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let _ = frame;
    }
}

fn run_eframe(rom: Vec<u8>, args: &Args) {
    let eng = Engine::new(rom, args.audio);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    let app = SpikeApp {
        eng,
        tex: None,
        start: Instant::now(),
        secs: args.secs,
        expect_screen: args.expect_screen,
        checked_screen: false,
        reported: false,
        screen_seen: None,
        wanted_audio: args.audio,
    };
    let r = eframe::run_native(
        "oracle-panels-spike",
        opts,
        Box::new(|_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    );
    if let Err(e) = r {
        eprintln!("eframe failed to start: {e}");
        std::process::exit(3);
    }
}

// ---------------------------------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------------------------------

fn report(mode: &str, eng: &Engine, elapsed: f64, screen: Option<(f32, f32)>, wanted_audio: bool) {
    println!("\n================ oracle-panels-spike ================");
    println!("mode                 {mode}");
    if let Some((w, h)) = screen {
        println!("toolkit screen       {w}x{h}");
    }
    println!("wall seconds         {elapsed:.2}");
    println!("loop iterations      {}", eng.iterations);
    println!(
        "emulated frames      {}  ({:.2}/s)",
        eng.emulated_frames,
        eng.emulated_frames as f64 / elapsed
    );
    println!(
        "drawn frames         {}  ({:.2}/s)   <-- sustained frame rate",
        eng.drawn_frames,
        eng.drawn_frames as f64 / elapsed
    );
    // Proof the emulate/convert costs are for a real picture and not a black screen: a run that never got
    // the VDP going would show 0 non-black pixels and every number above would be meaningless.
    match eng.image.as_ref() {
        Some(img) => {
            let lit = img
                .pixels
                .iter()
                .filter(|p| p.r() != 0 || p.g() != 0 || p.b() != 0)
                .count();
            println!(
                "last picture         {}x{}, {lit} non-black pixels ({:.1}%), {} distinct colours",
                img.size[0],
                img.size[1],
                lit as f64 * 100.0 / img.pixels.len() as f64,
                {
                    let mut c: Vec<u32> = img
                        .pixels
                        .iter()
                        .map(|p| {
                            p.to_array()[0] as u32 * 65536
                                + p.to_array()[1] as u32 * 256
                                + p.to_array()[2] as u32
                        })
                        .collect();
                    c.sort_unstable();
                    c.dedup();
                    c.len()
                }
            );
        }
        None => println!("last picture         NONE — the run never completed a frame"),
    }
    println!("\n-- per-iteration CPU cost, milliseconds --");
    println!(
        "{:<14} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "part", "mean", "median", "p95", "p99", "max", "n"
    );
    for (name, s) in [
        ("emulate", &eng.b.emulate),
        ("audio", &eng.b.audio),
        ("convert", &eng.b.convert),
        ("tex-upload", &eng.b.upload),
        ("ui-build", &eng.b.ui),
        ("tessellate", &eng.b.tessellate),
        ("CPU TOTAL", &eng.b.cpu_total),
        ("period", &eng.b.period),
    ] {
        println!("{}", s.row(name));
    }
    match eng.audio.as_ref() {
        Some(a) => {
            let c = &a.counters;
            let cb = c.callbacks.load(Ordering::Relaxed);
            let starved = c.starved_callbacks.load(Ordering::Relaxed);
            let lost = c.starved_samples.load(Ordering::Relaxed);
            let minocc = c.min_occupancy.load(Ordering::Relaxed);
            println!("\n-- audio (real device, gain 0.0) --");
            println!("device               {} Hz, {} ch", a.rate, a.channels);
            println!(
                "ring capacity        {} samples",
                audio::ring_capacity(&a.prod)
            );
            println!("callbacks            {cb}");
            let steady = c.starved_steady.load(Ordering::Relaxed);
            println!(
                "STARVED callbacks    {starved} total  ({:.4}%)",
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
                "starved samples      {lost}  ({:.1} ms of silence)",
                lost as f64 * 500.0 / a.rate as f64
            );
            println!(
                "leanest ring         {} samples ({:.1} ms)",
                if minocc == u64::MAX { 0 } else { minocc },
                if minocc == u64::MAX {
                    0.0
                } else {
                    minocc as f64 * 500.0 / a.rate as f64
                }
            );
            println!("producer DROPS       {} samples (ring full)", a.dropped);
        }
        None if wanted_audio => {
            println!("\n-- audio --");
            println!(
                "NOT MEASURED — audio was REQUESTED but no usable output device exists in this \
                 environment (see the warning above). The zero underruns you would otherwise read here \
                 would come from a sink that never ran."
            );
        }
        None => {
            println!("\n-- audio --");
            println!("not measured in this pass: audio was switched off with `--audio off`.");
        }
    }
    println!("=====================================================\n");
}
