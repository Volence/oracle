//! **The toolkit player** — the Oracle emulator hosted in a dockable `egui` / `egui_dock` UI.
//!
//! This is parcel 1 of the rebuild ruled in `docs/2026-09-02-player-pacing-design.md`: the crate, the
//! pacing, keyboard input, and a docked layout with the game screen in it. It is **not** the debug panels;
//! those are later parcels, and every one of them is already served by the Aether method table.
//!
//! `crates/oracle-frontend` — the minifb player — is untouched by this crate and keeps working. The two
//! share exactly one file, the audio substrate, included rather than forked (see the `audio` module
//! declaration below).
//!
//! # Modes
//!
//! * `--mode window` (default) — the player. Runs until the window is closed.
//! * `--mode bench-cpu --secs N` — the **pacing measurement**. No window, no GPU: a bare `egui::Context`
//!   driven through the identical per-frame pipeline at the governor's deadline. Because the governor
//!   rather than vsync sets the rate, the frame rate this reports is the design's own and is
//!   display-independent.
//! * `--mode bench-window --secs N --expect-screen WxH` — the real winit + wgpu stack, for N seconds, then
//!   a report and exit. Its CPU parts are honest; its presented frame rate is refused, because under
//!   `Xvfb` the rasteriser is `llvmpipe`.
//!
//! # The instrument rule
//!
//! This machine's owner is using it. Two guards, and the *second* is the real one:
//!
//! * `--expect-screen WxH` makes the process **ask the toolkit for its own screen size on the first frame**
//!   and `exit(2)` without drawing if it disagrees. Environment variables are setup, not a guard — a
//!   `DISPLAY` that silently falls back to the session would fail silently. Both bench modes require it.
//! * Both bench modes force **gain 0.0**, which multiplies on the producer side, so the ring dynamics, the
//!   feedback loop and every underrun count stay genuine while the amplitude is exactly zero.

// The player's audio substrate, included verbatim rather than re-implemented, so "the same audio path" is
// literally the same file.
//
// `oracle-frontend` is a binary crate with no `lib` target, so there is nothing to depend on. The
// alternatives were weighed: **copying** the file puts two tuned-by-measurement copies of the pacing policy
// on disk and lets them drift silently; giving `oracle-frontend` a `lib` target drags `minifb`, `x11-dl`
// and `gilrs` into this crate's graph to reach one file; **extracting a shared `oracle-audio` crate** is the
// right end state and is a parcel-2 line item, but it means editing `oracle-frontend`, and the standing
// instruction for this parcel is that the minifb player is not touched.
//
// Its `#[cfg(test)] mod tests` compiles here too, so `cargo test -p oracle-player` re-runs the substrate's
// own tests in this crate's context. That is the proof that the file this crate compiles is the file the
// player compiles.
#[allow(dead_code)]
#[path = "../../oracle-frontend/src/audio.rs"]
mod audio;

mod device;
mod input;
mod machine;
mod pacing;
mod report;
mod stats;
mod ui;

use std::time::{Duration, Instant};

use device::{loud, Device};
use machine::{ms, Machine};
use pacing::{Governor, FRAME_PERIOD};
use report::{Buckets, Reach};

// -------------------------------------------------------------------------------------------------------
// Arguments
// -------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Window,
    BenchCpu,
    BenchWindow,
}

struct Args {
    rom: String,
    mode: Mode,
    secs: f64,
    audio: bool,
    expect_screen: Option<(u32, u32)>,
    /// Window geometry the player asks for.
    size: (f32, f32),
    /// The governor's target rate. `None` — the default, and the only thing the player itself ever uses —
    /// means exactly [`pacing::FRAME_PERIOD`], with no float round-trip through a rate. **`Some(0.0)`
    /// switches the governor OFF**: the control, not a mode of the player, reproducing the spike's
    /// arrangement so the bench can measure the design against its own absence
    /// ([`pacing::Governor::unpaced`]).
    target_fps: Option<f64>,
}

fn usage() -> ! {
    loud(
        "usage: oracle-player --rom PATH [--mode window|bench-cpu|bench-window] [--secs N]\n\
         \x20              [--audio on|off] [--expect-screen WxH] [--size WxH] [--target-fps N]\n\
         \n\
         Both bench modes REQUIRE --expect-screen and force audio gain 0.0.\n\
         --target-fps 0 switches the GOVERNOR OFF. That is the control for the pacing design, not a\n\
         playable mode; the report labels any run made with it.",
    );
    std::process::exit(64);
}

fn parse_args() -> Args {
    let mut a = Args {
        rom: String::new(),
        mode: Mode::Window,
        secs: 60.0,
        audio: true,
        expect_screen: None,
        size: (1280.0, 800.0),
        target_fps: None,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let next = |i: usize, argv: &[String]| -> String {
        match argv.get(i + 1) {
            Some(v) => v.clone(),
            None => {
                loud(&format!("missing value for {}", argv[i]));
                usage()
            }
        }
    };
    while i < argv.len() {
        match argv[i].as_str() {
            "--rom" => {
                a.rom = next(i, &argv);
                i += 2;
            }
            "--mode" => {
                a.mode = match next(i, &argv).as_str() {
                    "window" => Mode::Window,
                    "bench-cpu" => Mode::BenchCpu,
                    "bench-window" => Mode::BenchWindow,
                    other => {
                        loud(&format!("unknown --mode {other}"));
                        usage()
                    }
                };
                i += 2;
            }
            "--secs" => {
                a.secs = next(i, &argv).parse().unwrap_or_else(|_| usage());
                i += 2;
            }
            "--audio" => {
                a.audio = next(i, &argv) == "on";
                i += 2;
            }
            "--expect-screen" => {
                let v = next(i, &argv);
                let (w, h) = v.split_once('x').unwrap_or_else(|| usage());
                a.expect_screen = Some((
                    w.parse().unwrap_or_else(|_| usage()),
                    h.parse().unwrap_or_else(|_| usage()),
                ));
                i += 2;
            }
            "--size" => {
                let v = next(i, &argv);
                let (w, h) = v.split_once('x').unwrap_or_else(|| usage());
                a.size = (
                    w.parse().unwrap_or_else(|_| usage()),
                    h.parse().unwrap_or_else(|_| usage()),
                );
                i += 2;
            }
            "--target-fps" => {
                a.target_fps = Some(next(i, &argv).parse().unwrap_or_else(|_| usage()));
                i += 2;
            }
            "-h" | "--help" => usage(),
            other => {
                loud(&format!("unknown flag {other}"));
                usage()
            }
        }
    }
    if a.rom.is_empty() {
        loud("--rom is required");
        usage();
    }
    if a.mode != Mode::Window && a.expect_screen.is_none() {
        loud(
            "REFUSING TO MEASURE: a bench mode without --expect-screen cannot prove it owns the display \
             it is about to draw on. Pass --expect-screen WxH (see run-bench.sh).",
        );
        std::process::exit(2);
    }
    a
}

fn main() {
    let args = parse_args();
    let rom = match std::fs::read(&args.rom) {
        Ok(r) => r,
        Err(e) => {
            loud(&format!("cannot read ROM {}: {e}", args.rom));
            std::process::exit(66);
        }
    };
    // Both bench modes are silent by construction; the window mode is the player and plays.
    let gain = if args.mode == Mode::Window { 1.0 } else { 0.0 };
    loud(&format!(
        "oracle-player: rom={} ({} bytes) mode={} audio={} gain={gain}",
        args.rom,
        rom.len(),
        match args.mode {
            Mode::Window => "window",
            Mode::BenchCpu => "bench-cpu",
            Mode::BenchWindow => "bench-window",
        },
        args.audio
    ));

    let device = if args.audio { Device::open(gain) } else { None };
    let machine = Machine::new(rom, device);

    match args.mode {
        Mode::BenchCpu => run_bench_cpu(machine, &args),
        Mode::Window | Mode::BenchWindow => run_window(machine, &args),
    }
}

// -------------------------------------------------------------------------------------------------------
// The loop body, shared by every mode
// -------------------------------------------------------------------------------------------------------

/// Everything one iteration needs that is not the toolkit.
struct Loop {
    machine: Machine,
    governor: Governor,
    buckets: Buckets,
    iterations: u64,
    frame_iterations: u64,
    /// Frame-owning iterations by how many emulated frames the audio ring asked for: `[0, 1, 2]`.
    frames_per_iter: [u64; 3],
    /// When the previous *frame-owning* iteration started, for the period series.
    last_frame_at: Option<Instant>,
    /// The input latch (see [`input::decide`]).
    latch: bool,
    status: String,
    dock: egui_dock::DockState<ui::Tab>,
    tex: Option<egui::TextureHandle>,
}

impl Loop {
    fn new(machine: Machine, now: Instant, target_fps: Option<f64>) -> Self {
        Self {
            machine,
            governor: match target_fps {
                None => Governor::start(now, FRAME_PERIOD),
                Some(f) if f <= 0.0 => {
                    loud(
                        "GOVERNOR OFF (--target-fps 0). This is the CONTROL — the spike's arrangement, \
                         with layer 1 removed. Nothing measured under it is the player's behaviour.",
                    );
                    Governor::unpaced(now)
                }
                Some(f) => {
                    loud(&format!(
                        "governor target OVERRIDDEN to {f} fps. The player's own rate is \
                         {:.3} ms; this run is not it.",
                        FRAME_PERIOD.as_secs_f64() * 1000.0
                    ));
                    Governor::start(now, Duration::from_secs_f64(1.0 / f))
                }
            },
            buckets: Buckets::default(),
            iterations: 0,
            frame_iterations: 0,
            frames_per_iter: [0; 3],
            last_frame_at: None,
            latch: false,
            status: String::from("starting"),
            dock: ui::initial_dock(),
            tex: None,
        }
    }

    /// **One iteration of the player.** The order here is the design:
    ///
    /// 1. Ask the governor whether this iteration owns a frame (layer 1 — the coarse rate limit).
    /// 2. If it does, sample the keyboard and hand it to the machine, which asks the **audio ring** how
    ///    many emulated frames to run (layer 2 — the master clock).
    /// 3. Upload the picture and lay out the UI, whether or not a frame ran, so an early wake still
    ///    re-presents the retained picture instead of flashing black.
    ///
    /// Returns the governor's verdict: whether this iteration owned a frame, and how long to ask the
    /// toolkit to wait before the next repaint.
    fn iterate(&mut self, ctx: &egui::Context, root: &mut egui::Ui, now: Instant) -> pacing::Tick {
        self.iterations += 1;
        let tick = self.governor.tick(now);

        let mut cost = machine::StepCost::default();
        if tick.run {
            if let Some(prev) = self.last_frame_at {
                self.buckets.period.push(ms(now - prev));
            }
            self.last_frame_at = Some(now);
            self.frame_iterations += 1;

            let keys = input::poll_pad(ctx);
            // egui 0.36 spells this `egui_wants_keyboard_input` (the `egui_` prefix distinguishes egui's
            // own focus from a hosting app's). It is true whenever a widget — a text field, a tab rename,
            // a future memory-panel search box — is consuming typing.
            let pad = input::decide(keys, ctx.egui_wants_keyboard_input(), &mut self.latch);
            cost = self.machine.step(pad);
            // `MAX_FRAMES_PER_ITER` is 2, so the bucket cannot overflow; clamp anyway rather than index
            // out of bounds if that constant is ever raised.
            self.frames_per_iter[cost.frames.min(2)] += 1;
        }

        // Only re-upload when a frame ran (or on the very first picture). An early wake re-presents the
        // texture already bound, which is both correct and free — uploading again would be 287 KB of
        // memcpy to hand egui a picture it is already holding.
        let upload = if tick.run || self.tex.is_none() {
            self.upload(ctx)
        } else {
            0.0
        };
        self.status = format!(
            "{} · {} frames · {} rebases",
            if self.governor.is_paced() {
                "governor on"
            } else {
                "GOVERNOR OFF (control)"
            },
            self.machine.frames(),
            self.governor.rebases()
        );
        let t = Instant::now();
        self.build_ui(root);
        let ui_ms = ms(t.elapsed());

        if tick.run {
            self.buckets.emulate.push(cost.emulate);
            self.buckets.audio.push(cost.audio);
            self.buckets.convert.push(cost.convert);
            self.buckets.upload.push(upload);
            self.buckets.ui.push(ui_ms);
            self.buckets
                .cpu_total
                .push(cost.emulate + cost.audio + cost.convert + upload + ui_ms);
        }
        tick
    }

    /// Hand the current picture to egui. Returns the milliseconds it took.
    fn upload(&mut self, ctx: &egui::Context) -> f64 {
        let Some(img) = self.machine.image() else {
            return 0.0;
        };
        let t = Instant::now();
        // `TextureHandle::set` takes the image by value, so this clone is 287 KB of memcpy per frame. The
        // spike measured the whole upload at 7 us, i.e. the clone is essentially all of it — worth
        // removing when the machine can hand over an owned image, and not worth a redesign before then.
        let opts = egui::TextureOptions::NEAREST;
        if self.tex.is_none() {
            self.tex = Some(ctx.load_texture("screen", img.clone(), opts));
        } else if let Some(h) = self.tex.as_mut() {
            h.set(img.clone(), opts);
        }
        ms(t.elapsed())
    }

    fn build_ui(&mut self, root: &mut egui::Ui) {
        // Disjoint field borrows: the dock mutably, everything the panels read immutably.
        let Loop {
            machine,
            governor,
            dock,
            tex,
            status,
            ..
        } = self;
        egui::Panel::top("bar").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.strong("oracle-player");
                ui.separator();
                ui.monospace(status.as_str());
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(root, |ui| {
                let mut panels = ui::Panels {
                    tex: tex.as_ref(),
                    machine,
                    governor,
                    status: status.as_str(),
                };
                egui_dock::DockArea::new(dock)
                    .style(egui_dock::Style::from_egui(ui.style().as_ref()))
                    .show_inside(ui, &mut panels);
            });
    }
}

// -------------------------------------------------------------------------------------------------------
// bench-cpu — the measurement this parcel owes
// -------------------------------------------------------------------------------------------------------

/// The display-independent pass: a bare `egui::Context`, no window, no GPU, no eframe, running the
/// identical pipeline against the identical governor.
///
/// **Why the frame rate from this mode is a real answer and the spike's was not.** The spike's paced mode
/// invented a deadline for the measurement only; the real `eframe` mode had none, which is why it produced
/// 92.87 fps and 22.71 fps. Here the deadline *is the player's own governor*, the same object the window
/// mode drives. What this mode cannot see is the backend's present cost, and that is why `bench-window`
/// exists.
fn run_bench_cpu(machine: Machine, args: &Args) {
    let start = Instant::now();
    let mut lp = Loop::new(machine, start, args.target_fps);
    let ctx = egui::Context::default();
    let screen =
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(args.size.0, args.size.1));
    let mut tessellate = crate::stats::Series::default();

    while start.elapsed().as_secs_f64() < args.secs {
        let now = Instant::now();
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(start.elapsed().as_secs_f64()),
            ..Default::default()
        };
        let mut tick = pacing::Tick {
            run: false,
            wait: Duration::ZERO,
            late_by: Duration::ZERO,
            rebased: false,
        };
        let mut out = ctx.run_ui(raw, |root| {
            let c = root.ctx().clone();
            tick = lp.iterate(&c, root, now);
        });
        // The backend owns tessellation in the window modes; here it is ours, and it is part of the CPU
        // cost the toolkit adds, so it is timed rather than skipped. Only frame-owning iterations are
        // recorded, so every bucket in the report has the same `n` and the columns can be read across.
        let t = Instant::now();
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        if tick.run {
            tessellate.push(ms(t.elapsed()));
        }
        drop(prims);
        // `TexturesDelta` PANICS on drop while it still holds unapplied deltas (epaint 0.36
        // `textures.rs:337`), because in a real backend an unapplied delta is a leaked GPU texture. There
        // is no backend here, so the deltas are discarded — but they have to be discarded *deliberately*,
        // through `clear()`, which is the API's own escape hatch. Note the panic is debug-only, so a
        // release-mode bench never sees it: `drop(out.textures_delta)` is silent in `--release` and
        // aborts under `cargo test`. (The throwaway spike still has that bug at
        // `crates/oracle-panels-spike/src/main.rs:628`; it has no test target, so nothing ever ran it in
        // debug.)
        out.textures_delta.clear();
        let wait = tick.wait;

        // Stand in for the toolkit's wait. `request_repaint_after` is what the window modes use; a sleep
        // is the same contract with no event queue to service.
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
    lp.buckets.tessellate = tessellate;
    report::print(&report::Run {
        label: if lp.governor.is_paced() {
            "bench-cpu (governor-paced, no window, no GPU)"
        } else {
            "bench-cpu -- CONTROL, GOVERNOR OFF (no window, no GPU)"
        },
        reach: Reach::DisplayIndependent,
        elapsed: start.elapsed().as_secs_f64(),
        iterations: lp.iterations,
        frame_iterations: lp.frame_iterations,
        buckets: &lp.buckets,
        machine: &lp.machine,
        governor: &lp.governor,
        frames_per_iter: lp.frames_per_iter,
        screen: None,
        wanted_audio: args.audio,
    });
}

// -------------------------------------------------------------------------------------------------------
// window / bench-window — the real stack
// -------------------------------------------------------------------------------------------------------

struct App {
    lp: Loop,
    start: Instant,
    /// `Some(secs)` in bench-window: report and close after that long. `None` in window mode.
    deadline: Option<f64>,
    expect_screen: Option<(u32, u32)>,
    checked_screen: bool,
    screen_seen: Option<(f32, f32)>,
    reported: bool,
    wanted_audio: bool,
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();

        // --- Display ownership, before anything is drawn or driven. ---
        //
        // The environment is setup, not a guard. This is the guard: the process asks the *toolkit* what
        // screen it is on and refuses to draw on one it was not told to expect. A geometry no real monitor
        // has (see run-bench.sh) makes the check a discriminator rather than a coincidence.
        if !self.checked_screen {
            self.checked_screen = true;
            let monitor = ctx.input(|i| i.viewport().monitor_size);
            match (monitor, self.expect_screen) {
                (Some(m), Some((ew, eh))) => {
                    self.screen_seen = Some((m.x, m.y));
                    if (m.x.round() as u32, m.y.round() as u32) != (ew, eh) {
                        loud(&format!(
                            "ABORT: the toolkit reports a {}x{} screen but this run demanded {ew}x{eh}. \
                             That is NOT the display this run created — it is somebody's real compositor. \
                             Refusing to draw.",
                            m.x, m.y
                        ));
                        std::process::exit(2);
                    }
                    loud(&format!(
                        "display ownership CONFIRMED: toolkit reports {}x{}",
                        m.x, m.y
                    ));
                }
                (None, Some(_)) => {
                    loud(
                        "ABORT: the toolkit reports no monitor size, so display ownership cannot be \
                         verified. Refusing to draw.",
                    );
                    std::process::exit(2);
                }
                // Window mode without --expect-screen: the player, on the user's own display.
                _ => {}
            }
        }

        let tick = self.lp.iterate(&ctx, root, Instant::now());

        // **The governor, expressed to the toolkit.** `request_repaint_after` is strictly better than
        // sleeping here: the event loop waits with a timeout and still services input, so a key press
        // wakes the loop immediately and is seen on the next frame rather than up to 16 ms later. The
        // early-wake branch in `Governor::tick` is what makes that safe — an input-driven wake presents
        // the retained picture and emulates nothing, so input responsiveness cannot advance the emulator
        // off cadence.
        ctx.request_repaint_after(tick.wait);

        if let Some(secs) = self.deadline {
            if self.start.elapsed().as_secs_f64() >= secs && !self.reported {
                self.reported = true;
                report::print(&report::Run {
                    label: if self.lp.governor.is_paced() {
                        "bench-window (real winit + wgpu stack)"
                    } else {
                        "bench-window -- CONTROL, GOVERNOR OFF (real winit + wgpu stack)"
                    },
                    reach: Reach::SoftwareRasteriser,
                    elapsed: self.start.elapsed().as_secs_f64(),
                    iterations: self.lp.iterations,
                    frame_iterations: self.lp.frame_iterations,
                    buckets: &self.lp.buckets,
                    machine: &self.lp.machine,
                    governor: &self.lp.governor,
                    frames_per_iter: self.lp.frames_per_iter,
                    screen: self.screen_seen,
                    wanted_audio: self.wanted_audio,
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn run_window(machine: Machine, args: &Args) {
    let start = Instant::now();
    let app = App {
        lp: Loop::new(machine, start, args.target_fps),
        start,
        deadline: if args.mode == Mode::BenchWindow {
            Some(args.secs)
        } else {
            None
        },
        expect_screen: args.expect_screen,
        checked_screen: false,
        screen_seen: None,
        reported: false,
        wanted_audio: args.audio,
    };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([args.size.0, args.size.1])
            .with_title("oracle-player"),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "oracle-player",
        opts,
        Box::new(|_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    ) {
        loud(&format!("eframe failed to start: {e}"));
        std::process::exit(3);
    }
}
