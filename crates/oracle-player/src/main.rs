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

mod bus;
mod device;
mod input;
mod layout;
mod machine;
mod memory;
mod objects;
mod pacing;
mod report;
mod stats;
mod symbols;
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
    /// `--symbols PATH`. `None` means *discover* — the `.lst` beside the ROM, which is where
    /// `sigil build --emit-lst` writes it and exactly what `oracle-frontend` already looks for. The two
    /// absences are not the same absence; see [`crate::symbols::Source`].
    symbols: Option<String>,
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
        "usage: oracle-player --rom PATH [--symbols PATH] [--mode window|bench-cpu|bench-window]\n\
         \x20              [--secs N] [--audio on|off] [--expect-screen WxH] [--size WxH]\n\
         \x20              [--target-fps N]\n\
         \n\
         --symbols names a .lst listing. Without it the player looks for <rom>.lst beside the ROM,\n\
         which is where `sigil build --emit-lst` writes it. A NAMED listing that is missing is fatal;\n\
         a discovered one that is missing is not. Neither overrides the ROM-binding check.\n\
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
        symbols: None,
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
            "--symbols" => {
                a.symbols = Some(next(i, &argv));
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

    // Opt-in symbols, loaded here while `rom` is still ours to borrow: the binding check probes the
    // image's `deb2` appendix at the offset the listing's own `EndOfRom` names, so it needs the bytes,
    // not the core. Exactly where `oracle-frontend` does it, for exactly that reason.
    let (sym_path, sym_source) = symbols::resolve(&args.rom, args.symbols.as_deref());
    let loaded = symbols::load(&sym_path, sym_source, &rom);
    if let Some(fatal) = loaded.fatal {
        loud(&format!("symbols: {fatal}"));
        std::process::exit(66);
    }

    let device = if args.audio { Device::open(gain) } else { None };
    let machine = Machine::new(rom, device);

    match args.mode {
        Mode::BenchCpu => run_bench_cpu(machine, &args, loaded),
        Mode::Window | Mode::BenchWindow => run_window(machine, &args, loaded),
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
    /// The `--rom` argument, verbatim. The status strip absolutises it through the bus's own helper.
    rom_path: String,
    /// **The hosted Aether bus** — unbound, unserved, never pumped inside a frame. See [`crate::bus`] for
    /// what it does not do and why, and for where the pause mirror lands.
    bus: bus::Bus,
    /// The listing, kept beside the bus rather than inside it: the engine owns an `Arc<SymbolTable>` it
    /// resolves with, and the panel borrows *this* one. Two clones of one parse, never two parses.
    symbols: Option<oracle_core::symbols::SymbolTable>,
    /// The Memory panel's state between repaints.
    mem: memory::MemoryPanel,
    /// The Objects panel's state between repaints — which row is expanded, and nothing else. The pool
    /// itself is re-derived each repaint, never cached.
    objects: objects::ObjectsPanel,
    /// **Whether this iteration advances the machine**, and nothing more.
    ///
    /// It is not a second copy of the bus's run state — it is re-read from [`bus::Bus::is_paused`] twice
    /// per iteration and never written from a click. The transport bar changes the run state by asking the
    /// *tool* (`emulator/pause`), and this follows whatever the tool did, so a refusal leaves it exactly
    /// where it was without the bar having to know what a refusal means.
    paused: bool,
    /// The transport bar's echo of the last answer the bus gave it. Rendered verbatim; never composed.
    transport: ui::Transport,
}

impl Loop {
    fn new(
        mut machine: Machine,
        now: Instant,
        target_fps: Option<f64>,
        rom_path: String,
        loaded: symbols::Loaded,
    ) -> Self {
        // ⚑ The pause mirror lands HERE, before the governor starts and outside every measured bucket.
        //
        // `Host::set_paused` queues; `Host::pump` applies. The player has no pause control in this
        // parcel (the transport bar is 2c), so the mirrored value cannot change and one drain at setup
        // is the whole of it — which is why `iterate` below is byte-identical to parcel 1's and its
        // pacing numbers still stand. Without this the engine keeps `Engine::new`'s `free_run: false`
        // and every paused-only write would succeed against a machine running at 60 Hz.
        let symbols = loaded.table;
        let bus = bus::Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo {
                rom_path: Some(rom_path.clone()),
                // The engine takes its own `Arc` of the table; the clone is one parse shared, so the
                // panel and `emulator/lookup_symbol` cannot resolve against two different listings.
                symbols: symbols.clone(),
                symbols_path: loaded.path.map(|p| p.display().to_string()),
            },
            false,
        );
        Self {
            machine,
            rom_path,
            bus,
            symbols,
            mem: memory::MemoryPanel::default(),
            objects: objects::ObjectsPanel::default(),
            // The player plays. This is the same `false` handed to `Bus::new` above, and the two are one
            // fact: the bus was just told the loop is running, and the loop is.
            paused: false,
            transport: ui::Transport::default(),
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

        // --- ⚑ Conflict 1, inbound: adopt the bus's run state, and do it FIRST. ---
        //
        // **One adoption, at the top, and it is load-bearing in both directions.** Three things move the
        // engine's run flags behind this field's back, and all three land before the next iteration
        // begins: a breakpoint halt applied by the drain at the bottom of the previous iteration, and
        // `emulator/pause` / `emulator/resume` issued by the transport bar during the previous
        // `build_ui`. Reading here is what makes every one of them take effect before the governor's tick
        // below decides whether to run a frame.
        //
        // **Reading it later instead is the bug that hides.** `Bus::mirror_pause` calls
        // `Host::set_paused(self.paused)`, which compares against the *engine's* flag — so an adoption
        // that happens after the drain lets the drain mirror a stale `false` over a pause the bar just
        // asked for, queue `free_run = true`, and resume a machine the human stopped. One iteration
        // later the field would agree with the bus again and nothing would look wrong.
        //
        // Read from `Bus::is_paused` rather than tracked here: it consults `pending_free_run`, which a
        // `Host::call` does not apply, and is the one truthful reading (bus.rs). This field is a cache of
        // *that*, refreshed once per iteration — not a second opinion (R2).
        self.paused = self.bus.is_paused();

        let tick = self.governor.tick(now);

        let mut cost = machine::StepCost::default();
        if tick.run {
            if let Some(prev) = self.last_frame_at {
                self.buckets.period.push(ms(now - prev));
            }
            self.last_frame_at = Some(now);
            self.frame_iterations += 1;

            // ⚑ A paused player owns its frame and does not advance the machine. The period, the upload
            // and the UI below all still happen — a paused window is not a frozen one — but nothing here
            // touches the clock, which is the whole content of the promise `Host::set_paused` makes to
            // the bus on the next line.
            if !self.paused {
                let keys = input::poll_pad(ctx);
                // egui 0.36 spells this `egui_wants_keyboard_input` (the `egui_` prefix distinguishes
                // egui's own focus from a hosting app's). It is true whenever a widget — a text field, a
                // tab rename, a future memory-panel search box — is consuming typing.
                let pad = input::decide(keys, ctx.egui_wants_keyboard_input(), &mut self.latch);
                cost = self.machine.step(pad, &mut self.bus);
                // `MAX_FRAMES_PER_ITER` is 2, so the bucket cannot overflow; clamp anyway rather than
                // index out of bounds if that constant is ever raised.
                self.frames_per_iter[cost.frames.min(2)] += 1;
            }
        }

        // --- ⚑ The drain: one bounded, non-blocking pump per iteration. ---
        //
        // **AFTER the frame, not before it, and the ordering is the halt path.** `Machine::step` latched
        // any breakpoint halt through `Bus::record_break`, and a latch is applied at the top of the next
        // `Host::pump` and nowhere else. Draining here applies it in the *same* iteration that observed
        // it, so `is_paused()` below is true before the governor's next tick and no further frame runs.
        // Draining at the top of `iterate` instead — which is where parcel 2b's module doc predicted this
        // call would move — would leave the halt unapplied across the following tick, and the player would
        // run one extra frame past a breakpoint it had already stopped on. The *adoption* is what moved to
        // the top; the drain stays here, behind the frame that latches into it.
        //
        // There is deliberately **no second `self.paused = ...` after this**. An earlier draft had one,
        // and both it and the one at the top were then individually removable with every test still
        // green — two lines, each covering for the other, which is how a redundancy passes for a
        // safeguard. The adoption at the top is the one that is correct on its own, for both the halt and
        // the transport bar, and it is now the only one.
        let t_bus = Instant::now();
        self.bus
            .mirror_pause(self.machine.system_mut(), self.paused);
        let bus_ms = ms(t_bus.elapsed());

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
        // The transport bar inside `build_ui` routes its gestures through `Host::call`, which is
        // deliberately NOT a drain and applies neither pending change (host.rs) — so a pause or resume it
        // just issued has already moved the engine's own flags. It is adopted at the TOP of the next
        // iteration, before that iteration's tick decides whether to run a frame, which is why no frame
        // slips through between the click and the pause taking effect.

        if tick.run {
            self.buckets.emulate.push(cost.emulate);
            self.buckets.audio.push(cost.audio);
            self.buckets.convert.push(cost.convert);
            self.buckets.upload.push(upload);
            self.buckets.ui.push(ui_ms);
            self.buckets.bus.push(bus_ms);
            self.buckets
                .cpu_total
                .push(cost.emulate + cost.audio + cost.convert + upload + ui_ms + bus_ms);
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
        // Disjoint field borrows: the dock, the machine, the bus and the Memory panel's state mutably
        // (`Host::call` lends the machine to the engine and takes it back), everything else immutably.
        let Loop {
            machine,
            governor,
            dock,
            tex,
            status,
            rom_path,
            bus,
            symbols,
            mem,
            objects,
            transport,
            ..
        } = self;
        egui::Panel::top("bar").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.strong("oracle-player");
                ui.separator();
                // ⚑ A CONTROL, NOT A TAB. Things you *do* are controls; the `Tab` enum is for things you
                // *look at*, and adding a variant here would also owe `layout::LAYOUT_VERSION` a bump and
                // discard every stored layout on the owner's machine.
                transport.bar(ui, machine, bus);
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
                    bus,
                    mem,
                    objects,
                    governor,
                    status: status.as_str(),
                    rom_path: rom_path.as_str(),
                    symbols: symbols.as_ref(),
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
fn run_bench_cpu(machine: Machine, args: &Args, loaded: symbols::Loaded) {
    let start = Instant::now();
    let mut lp = Loop::new(machine, start, args.target_fps, args.rom.clone(), loaded);
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
    /// **Whether this run may read or write persisted state at all.** True in window mode, false in
    /// `bench-window` — see [`run_window`] for why a measured mode is kept hermetic.
    persist: bool,
}

impl eframe::App for App {
    /// Called by eframe on shutdown and on its own auto-save interval, and only because
    /// `eframe/persistence` is on. Everything about the format lives in [`crate::layout`].
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.persist {
            layout::save(storage, &self.lp.dock);
        }
    }

    /// egui's own memory (window positions, collapsing headers, scroll offsets) rides the same switch as
    /// the dock: a `bench-window` run must not inherit the operator's UI state, or its `ui` bucket is
    /// measuring somebody's saved scroll position.
    fn persist_egui_memory(&self) -> bool {
        self.persist
    }

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

fn run_window(machine: Machine, args: &Args, loaded: symbols::Loaded) {
    let start = Instant::now();
    // **Only the player persists.** `bench-window` is a measurement, and eframe's restore path is not
    // symmetric with its save path: `persist_window` gates *writing* the window geometry
    // (`eframe-0.36.1/src/native/epi_integration.rs:412`) but `load_window_settings` on the way in is not
    // gated by it at all (`wgpu_integration.rs:1105`). So a bench run sharing the player's storage file
    // would silently inherit whatever size the operator last dragged the window to and quietly ignore
    // `--size`, which the `--expect-screen` guard cannot catch — it checks the *monitor*, not the window.
    // Pointing the bench at a per-process scratch file makes the measured modes read nothing and write
    // nothing that outlives them.
    let persist = args.mode == Mode::Window;
    let mut app = App {
        lp: Loop::new(machine, start, args.target_fps, args.rom.clone(), loaded),
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
        persist,
    };
    let scratch = (!persist).then(|| {
        std::env::temp_dir().join(format!("oracle-player-bench-{}.ron", std::process::id()))
    });
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([args.size.0, args.size.1])
            .with_title("oracle-player"),
        persist_window: persist,
        persistence_path: scratch.clone(),
        ..Default::default()
    };
    let outcome = eframe::run_native(
        "oracle-player",
        opts,
        Box::new(move |cc| {
            if persist {
                // A public *field* on `CreationContext` (`epi.rs:64`), not the `storage()` accessor
                // `Frame` carries — and `None` here whenever eframe could not open a storage file.
                let (dock, outcome) = layout::load(cc.storage);
                app.lp.dock = dock;
                match outcome {
                    layout::Outcome::Restored => loud("layout: restored from the last session"),
                    layout::Outcome::Absent => {
                        loud("layout: none stored yet — the default arrangement")
                    }
                    // Reported, never raised. A layout that will not load is not a question the user has
                    // to answer; they get the default back and the reason goes to stderr with everything
                    // else this process says.
                    layout::Outcome::Discarded(why) => loud(&format!(
                        "layout: stored layout DISCARDED ({why:?}), falling back to the default \
                         arrangement. Nothing is wrong; the format this build reads is v{}.",
                        layout::LAYOUT_VERSION
                    )),
                }
            }
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    );
    if let Some(path) = scratch {
        let _ = std::fs::remove_file(path);
    }
    if let Err(e) = outcome {
        loud(&format!("eframe failed to start: {e}"));
        std::process::exit(3);
    }
}

// -------------------------------------------------------------------------------------------------------
// ⚑ The shipped loop, driven — parcel 3
// -------------------------------------------------------------------------------------------------------

/// The seam's own tests live in [`crate::bus`] and drive `Machine::step` directly, which leaves one gap
/// they cannot close: **they replicate `Loop::iterate`'s order rather than running it.** A `Loop` that
/// stopped reading [`bus::Bus::is_paused`] back would leave every one of them green while the player ran
/// straight through every breakpoint — the "both sides agree because neither side is the shipped one"
/// shape this repo has paid for before.
///
/// So this drives the real [`Loop::iterate`], through the same bare `egui::Context` [`run_bench_cpu`] uses.
///
/// `oracle-aether` is `#![cfg(unix)]`, so this module is too.
#[cfg(all(test, unix))]
mod loop_tests {
    use super::*;

    /// `move.w (A0),D0` in the fixture ROM's inner loop — the same address `crate::bus`'s seam tests arm
    /// at, and *checked* there against the ROM's own bytes rather than re-checked here.
    const HOT_PC: u32 = 0x0000_020E;

    fn a_loop() -> Loop {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        machine
            .system_mut()
            .set_pad(0, oracle_core::io::Pad::default());
        // The governor is switched OFF, so every `iterate` owns its frame and this test never waits on a
        // wall clock. That is the same switch the pacing CONTROL uses.
        Loop::new(
            machine,
            Instant::now(),
            Some(0.0),
            String::from("(fixture)"),
            symbols::Loaded {
                table: None,
                path: None,
                fatal: None,
            },
        )
    }

    /// Drive the real loop `n` times and return `(emulated frames, iterations actually run)`.
    ///
    /// **The two are counted separately and neither is assumed from `n`**: `egui::Context::run_ui` may run
    /// its callback more than once for a single call when a repaint is requested mid-frame, so `n` is a
    /// lower bound on iterations and not an equality. Measured this the hard way — a first version
    /// asserted `frames == n` and got 13 from 12.
    fn drive(lp: &mut Loop, ctx: &egui::Context, n: usize) -> (u64, u64) {
        let before = lp.machine.frames();
        let iters_before = lp.iterations;
        for _ in 0..n {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let out = ctx.run_ui(raw, |root| {
                let c = root.ctx().clone();
                lp.iterate(&c, root, Instant::now());
            });
            // `TexturesDelta` panics on drop in debug while it holds unapplied deltas; there is no backend
            // here, so they are discarded through the API's own escape hatch, exactly as `run_bench_cpu`
            // does. Getting this wrong aborts the test with a message about textures.
            let mut d = out.textures_delta;
            d.clear();
        }
        (lp.machine.frames() - before, lp.iterations - iters_before)
    }

    /// ★ **The shipped loop stops running frames on a breakpoint, and keeps turning while stopped.**
    ///
    /// **The alternative green paths, ruled out:**
    ///
    /// 1. *The loop never ran at all* (a governor that owned no frames, an `iterate` that did nothing) —
    ///    ruled out by the control: the same loop with nothing armed must advance the machine on every
    ///    one of its iterations.
    /// 2. *The loop stopped because it froze rather than paused* — ruled out by `iterations`, which must
    ///    keep climbing after the halt. A frozen loop and a paused one both stop the clock; only one of
    ///    them is correct, and this is what tells them apart.
    #[test]
    fn the_players_own_loop_stops_running_frames_on_a_breakpoint() {
        const N: usize = 12;
        let ctx = egui::Context::default();

        // (1) THE CONTROL, first: with nothing armed **every** iteration advances the machine, so the
        // shortfall asserted below is caused by the breakpoint and by nothing else about this fixture.
        {
            let mut lp = a_loop();
            let (advanced, iters) = drive(&mut lp, &ctx, N);
            assert!(iters >= N as u64, "the loop did not iterate at all");
            assert_eq!(
                advanced, iters,
                "the unarmed loop skipped a frame on some iteration, so a shortfall below would not be \
                 evidence of anything"
            );
            assert!(!lp.paused, "the unarmed loop paused itself");
        }

        let mut lp = a_loop();
        // Armed through the bus, exactly as a client or the next parcel's panel would.
        let armed = lp.bus.call(
            lp.machine.system_mut(),
            "emulator/breakpoint_add",
            &serde_json::json!({"addr": format!("0x{HOT_PC:08X}")}),
        );
        assert!(!armed.is_err(), "arming the fixture breakpoint was refused");

        let (advanced, iters) = drive(&mut lp, &ctx, N);
        assert!(
            advanced < iters,
            "the shipped loop ran a frame on every one of its {iters} iterations with a breakpoint \
             armed — it is not following `Bus::is_paused`, so the halt the bus recorded never reached \
             the loop"
        );
        assert!(lp.paused, "the loop's own pause must have followed the bus");
        assert_eq!(
            lp.machine.system().cpu_regs().pc,
            HOT_PC,
            "the loop stopped somewhere other than the breakpoint"
        );

        // (2) Stopped, not frozen: the loop keeps iterating and the clock stands still.
        let clock = lp.machine.system().scheduler().now();
        let (advanced, iters) = drive(&mut lp, &ctx, 5);
        assert!(
            iters >= 5,
            "the loop stopped turning altogether, which is a hang and not a pause"
        );
        assert_eq!(advanced, 0, "a paused loop must not advance the machine");
        assert_eq!(
            lp.machine.system().scheduler().now(),
            clock,
            "the emulated clock moved while the player was paused"
        );
    }

    /// ★ **A pause the transport bar asked for survives the loop's own drain.**
    ///
    /// This is the hazard that nearly shipped uncovered, and it is a *silent* one. `Transport::issue`
    /// calls `emulator/pause` through `Host::call`, which moves the engine's flags but is deliberately not
    /// a drain. The loop's `Bus::mirror_pause` then calls `Host::set_paused(self.paused)` — comparing
    /// against the engine's flag — so if `self.paused` is still the stale `false`, the very next drain
    /// queues `free_run = true` and **resumes the machine the human just stopped**, with the button
    /// flipping back to "pause" as if the click had never happened.
    ///
    /// `emulator/pause` is issued here between `drive` calls, which is the same position relative to the
    /// next iteration's adoption that a click inside `build_ui` occupies.
    ///
    /// **The alternative green paths, ruled out:**
    ///
    /// 1. *The machine was already paused.* Ruled out by driving it first and asserting it advanced.
    /// 2. *It stayed paused because the loop stopped running.* Ruled out by requiring the loop to keep
    ///    iterating, and by resuming through the bus at the end and watching frames start again — a loop
    ///    frozen for any other reason could not do that.
    #[test]
    fn a_pause_asked_for_through_the_bus_is_not_undone_by_the_next_drain() {
        let ctx = egui::Context::default();
        let mut lp = a_loop();

        let (advanced, _) = drive(&mut lp, &ctx, 3);
        assert!(
            advanced > 0,
            "the fixture must be running before it is paused"
        );

        // Exactly what `Transport::issue` does for the ⏸ button.
        let a = lp
            .bus
            .call(lp.machine.system_mut(), ui::PAUSE, &serde_json::json!({}));
        assert!(!a.is_err(), "emulator/pause was refused");
        assert!(
            lp.bus.is_paused(),
            "the call must move the bus's own reading"
        );

        let (advanced, iters) = drive(&mut lp, &ctx, 6);
        assert!(iters >= 6, "the loop stopped turning");
        assert_eq!(
            advanced, 0,
            "the loop kept emulating after `emulator/pause` — its drain mirrored a stale `false` over \
             the pause and queued `free_run = true`, resuming a machine the human stopped"
        );
        assert!(lp.paused, "and the loop's own field must agree");

        // (2) …and it is a pause, not a wedge: resuming through the same surface starts frames again.
        let a = lp
            .bus
            .call(lp.machine.system_mut(), ui::RESUME, &serde_json::json!({}));
        assert!(!a.is_err(), "emulator/resume was refused");
        let (advanced, _) = drive(&mut lp, &ctx, 3);
        assert!(
            advanced > 0,
            "the player never resumed, so the zero above was a stuck loop rather than a pause"
        );
    }
}
