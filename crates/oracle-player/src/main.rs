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
mod nav;
mod objects;
mod pacing;
mod report;
mod screen;
mod stats;
mod stopping;
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
    /// `--dock every-tab`. **A measurement arrangement, not a layout.** See [`ui::every_tab_dock`]:
    /// `egui_dock` draws only a leaf's *active* tab, so measuring the cost of three panels that share a
    /// pane measures one of them. In window mode it also **suppresses the restore and the save**, because
    /// a bench arrangement written back over the operator's own layout would be this flag doing something
    /// nobody asked it to.
    dock_every_tab: bool,
    /// `--bench-arm`. **A measurement fixture, refused outside a bench mode.** See
    /// [`arm_for_measurement`]: the three stopping panels are empty until something is armed, so a
    /// panel-cost run without this measures three headlines and calls it three panels.
    bench_arm: bool,
    /// **Whether this window serves the Aether bus to EXTERNAL clients, and on what path** —
    /// `--aether` / `--socket PATH` / `ORACLE_AETHER=1` (`PLAYER-SERVE`).
    ///
    /// The nesting is `oracle-frontend`'s and is carried rather than re-invented: `None` = *do not bind*
    /// (the default, and the launch the player has always had), `Some(None)` = *bind the contract's own
    /// resolved default path* (§7.1), `Some(Some(p))` = *bind `p`*. Three states, because "serve" and
    /// "serve here" are different asks and an `Option<PathBuf>` cannot hold both.
    ///
    /// It changes nothing about how the window talks to itself — see [`crate::bus`]'s module doc, which
    /// carries D15's rule that an in-process GUI is a consumer of the registry and not a second server.
    socket: Option<Option<std::path::PathBuf>>,
}

fn usage() -> ! {
    loud(
        "usage: oracle-player --rom PATH [--symbols PATH] [--mode window|bench-cpu|bench-window]\n\
         \x20              [--secs N] [--audio on|off] [--expect-screen WxH] [--size WxH]\n\
         \x20              [--target-fps N] [--dock default|every-tab] [--bench-arm]\n\
         \x20              [--aether | --socket PATH]\n\
         \n\
         --aether serves the Aether control bus from this process, so an external tool can attach to\n\
         THIS window (also: ORACLE_AETHER=1). --socket PATH does the same on a path you name, and\n\
         implies --aether. Without either, nothing binds and nothing can attach; the launch says so.\n\
         The default path is the contract's own ($ORACLE_SOCKET, $EXODUS_SOCKET,\n\
         $XDG_RUNTIME_DIR/oracle.sock, /tmp/oracle.sock) because that is the one every client\n\
         resolver looks on. If another emulator is already live there the bind is REFUSED and said\n\
         out loud, never silently moved to a second path.\n\
         \n\
         --symbols names a .lst listing. Without it the player looks for <rom>.lst beside the ROM,\n\
         which is where `sigil build --emit-lst` writes it. A NAMED listing that is missing is fatal;\n\
         a discovered one that is missing is not. Neither overrides the ROM-binding check.\n\
         \n\
         Both bench modes REQUIRE --expect-screen and force audio gain 0.0.\n\
         --target-fps 0 switches the GOVERNOR OFF. That is the control for the pacing design, not a\n\
         playable mode; the report labels any run made with it.\n\
         \n\
         --dock every-tab puts every tab in a leaf of its own, so every panel body runs on every\n\
         frame. It is the arrangement the PANEL-COST measurement needs (egui_dock draws only a\n\
         leaf's active tab, so tabs sharing a pane cost one body, not three) and it neither reads\n\
         nor writes a stored layout.\n\
         \n\
         --bench-arm arms sixteen (disabled) breakpoints, a work-RAM write watch and the profiler,\n\
         through the served surface, so the three stopping panels have ROWS to draw. Without it a\n\
         panel-cost run measures three empty headlines. Bench modes only.",
    );
    std::process::exit(64);
}

fn parse_args() -> Args {
    parse_args_from(
        std::env::args().skip(1).collect(),
        std::env::var_os("ORACLE_AETHER"),
    )
}

/// The testable half of [`parse_args`], over an arbitrary argv and an arbitrary `ORACLE_AETHER`.
///
/// Split out by `PLAYER-SERVE` so the socket decision — three flags folding into one three-state value —
/// is checkable without a process. **Only the accepting paths are reachable from a test**: every refusal
/// in here goes through [`usage`], which `exit(64)`s, and that is this parser's pre-existing shape rather
/// than something this split introduced. A test that asserted on a refusal would take the test binary
/// down with it.
fn parse_args_from(argv: Vec<String>, aether_env: Option<std::ffi::OsString>) -> Args {
    let mut a = Args {
        rom: String::new(),
        symbols: None,
        mode: Mode::Window,
        secs: 60.0,
        audio: true,
        expect_screen: None,
        size: (1280.0, 800.0),
        target_fps: None,
        dock_every_tab: false,
        bench_arm: false,
        // Serving is **opt-in**, and the default is "no socket exists". The environment is read at the
        // same place as the flags, and folded into the same value, so `--aether` and `ORACLE_AETHER` are
        // one decision with one spelling and the usage text can be truthful about both. `ORACLE_AETHER=0`
        // is an explicit *off*, not a present-therefore-on — copied from `oracle-frontend` exactly.
        socket: aether_env.is_some_and(|v| v != "0").then_some(None),
    };
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
            "--dock" => {
                a.dock_every_tab = match next(i, &argv).as_str() {
                    "default" => false,
                    "every-tab" => true,
                    other => {
                        loud(&format!("unknown --dock {other} (default|every-tab)"));
                        usage()
                    }
                };
                i += 2;
            }
            "--bench-arm" => {
                a.bench_arm = true;
                i += 1;
            }
            // `--aether` turns serving on and **preserves any path already chosen** — `--socket P
            // --aether` must still bind `P`, because asking for a specific path and then binding
            // somewhere else is worse than either flag alone. `flatten()` is what carries it.
            "--aether" => {
                a.socket = Some(a.socket.flatten());
                i += 1;
            }
            // `--socket PATH` implies `--aether`: naming a path and then not serving on it would be a
            // flag that does nothing.
            "--socket" => {
                a.socket = Some(Some(std::path::PathBuf::from(next(i, &argv))));
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
    if a.bench_arm && a.mode == Mode::Window {
        loud(
            "--bench-arm is a MEASUREMENT FIXTURE, not a feature: it arms sixteen breakpoints, a \
             work-RAM watch and the profiler behind the human's back, and a player that did that at \
             launch would be lying about who armed them. Use it with a bench mode.",
        );
        std::process::exit(64);
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
    /// The three stopping tabs' boxes and their last answers. **What is armed is not here** — it is the
    /// `Host`'s, read every repaint (R2).
    stopping: stopping::Panel,
}

impl Loop {
    fn new(
        mut machine: Machine,
        now: Instant,
        target_fps: Option<f64>,
        rom_path: String,
        loaded: symbols::Loaded,
        socket: Option<Option<std::path::PathBuf>>,
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
            socket,
        );
        // ⚑ **Unconditional, and there is deliberately no arm here to delete.** All three outcomes —
        // serving, failed-to-bind, never-asked — are one `println!` of one string, because the defect this
        // repairs is a launch that says *nothing* when the bus is off: an absence is not a statement, and
        // the measured cost of the frontend's version of that absence was the owner launching twice in one
        // evening and going to a window nothing could attach to. `Bus::announcement` composes the sentence
        // and `ServeOutcome::sentence` is a value a test reads, so unlike the frontend's three inline
        // prints, the wording here is covered.
        println!("{}", bus.announcement());
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
            stopping: stopping::Panel::default(),
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
            // the bus below.
            //
            // A paused iteration is deliberately **absent** from `frames_per_iter` rather than counted as
            // a 0. That bucket is "how many frames the AUDIO RING asked for", and `report` normalises it
            // against its own sum — so a pause stays out of the fine trim's statistics instead of being
            // reported as the governor running fast, which is what a 0 there means.
            if !self.paused {
                let keys = input::poll_pad(ctx);
                // egui 0.36 spells this `egui_wants_keyboard_input` (the `egui_` prefix distinguishes
                // egui's own focus from a hosting app's). It is true whenever a widget — a text field, a
                // tab rename, a future memory-panel search box — is consuming typing.
                let pad = input::decide(keys, ctx.egui_wants_keyboard_input(), &mut self.latch);
                // Port 1 is the keyboard's empty pad — this player binds no second controller. It is
                // still handed over rather than dropped, because `Machine::step` merges a client's
                // `emulator/hold` set into *both* ports and the status strip shows both.
                cost = self
                    .machine
                    .step([pad, oracle_core::io::Pad::default()], &mut self.bus);
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
        // it, so the adoption at the TOP of the next iteration already sees a paused bus and that
        // iteration's tick runs no frame.
        //
        // Draining at the top instead — which is where parcel 2b's module doc predicted this call would
        // move — would land *after* that adoption and leave the halt unapplied for the tick that follows
        // it, so the player would run one extra frame past a breakpoint it had already stopped on. The
        // *adoption* is what moved to the top; the drain stays here, behind the frame that latches into it.
        //
        // There is deliberately **no second `self.paused = ...` after this**. An earlier draft had one,
        // and both it and the one at the top were then individually removable with every test still
        // green — two lines, each covering for the other, which is how a redundancy passes for a
        // safeguard. The adoption at the top is the one that is correct on its own, for both the halt and
        // the transport bar, and it is now the only one.
        //
        // **And the drain's answer is acted on, in the same call.** `bus::drain` mirrors the pause, pumps,
        // and performs every repair the returned `PumpReport` obliges this window to make — the picture a
        // client's run drew, the capture and audio ring a machine replacement invalidated, the symbol cache
        // a ROM reload may have dropped. It is one function rather than a pump here and a reaction below it
        // because `PLAYER-SERVE` shipped exactly that second shape and the reaction was missing; a drain
        // whose answer the caller may decline to mention is the shape that made the omission expressible.
        // The whole of it is inside the `bus` bucket, for `Machine::step`'s reason one module over: timing
        // only the pump would make this parcel's own cost structurally invisible.
        let t_bus = Instant::now();
        bus::drain(
            &mut self.machine,
            &mut self.bus,
            &mut self.symbols,
            &mut self.rom_path,
            self.paused,
        );
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
        let drew = self.build_ui(root);
        let ui_ms = ms(t.elapsed());

        // --- ⚑ What the window says, published for `emulator/screen_text` (§11.29, CR-H). ---
        //
        // **Here, after `build_ui` and after the drain, and both halves of that position are the point.**
        // `Host::set_screen_text`'s own doc names the trap: text describing a frame *not yet presented* is
        // a false answer to the one question the method answers truthfully. `drew` cannot exist before
        // `build_ui` returns it — that is why it is a return value and not a helper this line could have
        // called earlier — and the next drain is at step 3 of iteration N+1, after eframe has presented
        // what was just composed. So a client's read lands on the frame that is on the glass, never on one
        // mid-composition. Design §5.8.2 booked this call's absence; this is it.
        //
        // **Gated on `is_serving`, deliberately not on `has_clients`**, which is `oracle-frontend`'s split
        // and its reason travels unchanged: with no socket bound no client can exist, so the snapshot is
        // pure cost — but gating on *attachment* would leave a client that connects mid-session reading
        // `-32005 noDisplay` ("there is no window") until the next present, which is exactly the false
        // answer the method exists to prevent. The skip belongs one level up, and this is that level.
        //
        // **Missing glyphs are asked of the live `egui::Context`**, through the family that drew each run
        // (`screen::Run::mono`). A hand-written table of characters this build cannot draw would be a
        // second opinion about a font, and the wrong one first.
        //
        // ⚑ **But NOT through `Fonts::has_glyph`, which is the obvious call and is wrong here.**
        // `screen::Glyphs` carries the measurement: on egui 0.36 `has_glyph` calls the letter `A`
        // undrawable in the monospace family and `▶` undrawable in the proportional one, on a build that
        // draws both — 26 invented hollow boxes on this bar. What it actually answers is *"is this char
        // owned by the same face as `◻`?"*. The atlas rectangle a glyph samples cannot lie that way,
        // because it IS what the renderer reads, so that is what is compared.
        if self.bus.is_serving() {
            let mut glyphs = screen::Glyphs::new(ctx);
            let surfaces =
                screen::snapshot(ui::APP_NAME, &drew, &mut |c, mono| glyphs.drawable(c, mono));
            self.bus.set_screen_text(surfaces);
        }
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

    /// **Returns the top bar's text runs, in draw order** — the readback `emulator/screen_text` serves
    /// (§11.29). Returned rather than re-derived by a helper beside this: see [`crate::screen`] for why
    /// handing back what was drawn is a different guarantee from computing the same thing twice.
    ///
    /// The `DockArea` below contributes **nothing**, and that is the parcel's central decision rather
    /// than an omission — `egui_dock` draws only each leaf's active tab, and what an active body reveals
    /// depends on a scroll offset computed inside egui's paint. `crate::screen`'s header argues it.
    fn build_ui(&mut self, root: &mut egui::Ui) -> Vec<screen::Run> {
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
            stopping,
            transport,
            ..
        } = self;
        let mut drew = Vec::new();
        egui::Panel::top("bar").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.strong(ui::APP_NAME);
                drew.push(screen::Run::label(ui::APP_NAME));
                ui.separator();
                // ⚑ **The panel nav, and it lives HERE rather than in the dock on purpose.** `egui_dock`
                // draws only each leaf's active tab, so six of the eight panels are behind another title
                // at any moment; a nav that was itself a `Tab` could be behind one too, which is the
                // failure it exists to repair. Outside the dock it cannot be hidden, and the stored
                // layout neither carries it nor changes shape because of it. See `crate::nav`.
                let mut nav_runs = nav::bar(ui, dock);
                if let Some(first) = nav_runs.first_mut() {
                    first.sep_before = true;
                }
                drew.append(&mut nav_runs);
                ui.separator();
                // ⚑ A CONTROL, NOT A TAB. Things you *do* are controls; the `Tab` enum is for things you
                // *look at*, and adding a variant here would also owe `layout::LAYOUT_VERSION` a bump and
                // discard every stored layout on the owner's machine.
                let mut bar = transport.bar(ui, machine, bus);
                // The bar's first run sits immediately after the separator drawn above; the rest carry
                // their own. Set here rather than inside `bar`, because the separator is drawn here.
                if let Some(first) = bar.first_mut() {
                    first.sep_before = true;
                }
                drew.append(&mut bar);
                ui.separator();
                ui.monospace(status.as_str());
                drew.push(screen::Run::mono_after_sep(status.as_str()));
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
                    stopping,
                    governor,
                    status: status.as_str(),
                    rom_path: rom_path.as_str(),
                    symbols: symbols.as_ref(),
                };
                egui_dock::DockArea::new(dock)
                    .style(egui_dock::Style::from_egui(ui.style().as_ref()))
                    .show_inside(ui, &mut panels);
            });
        drew
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
/// **Arm the three instruments, so a panel-cost measurement measures panels with rows in them.**
///
/// ⚑ *Why this exists at all.* The three stopping tabs are empty until a human arms something, so a bench
/// run against a fresh player measures three headlines and an add box and reports it as *the cost of the
/// Breakpoints, Watchpoints and Profiler panels*. That is the same vacuity as a parity test comparing `[]`
/// against `[]`: it passes under any breakage and it answers a question nobody asked. The expensive parts
/// of these bodies are the parts that only exist once there is something to draw — sixteen breakpoint rows,
/// a hit log, and a `BTreeMap` of routines sorted every frame — so the measurement arms them.
///
/// **Every arm goes through `Host::call`**, exactly as a human's click would: this fixture cannot reach a
/// state a user could not, and if a handler refuses, the run says so and continues rather than measuring a
/// state it believes it is in and is not.
///
/// Two deliberate choices, both of which shape what the numbers mean:
///
/// * **The breakpoints are armed DISABLED.** An enabled breakpoint at an address this ROM executes would
///   halt the player and there would be no run to measure. Disabled rows draw identically — the row body
///   does not branch on `enabled` except to pick a word and a dimming — so the panel cost is the same and
///   the machine keeps running. It also means `Breakpoints::any_enabled()` is false and no `BreakStop` is
///   attached, so this run does not price the halt sink; that is the seam parcel's measurement and it is
///   inside `emulate`, not `ui-build`.
/// * **The watch is wide and the profiler is armed with `perFrame`**, which is what puts real rows in
///   front of the panels. Both attach a sink to the run, so `emulate` will move. That is the *instrument's*
///   cost and not the panel's, and the bucket split is exactly what keeps the two answers apart.
fn arm_for_measurement(lp: &mut Loop) {
    let Loop { bus, machine, .. } = lp;
    let mut refused = 0usize;
    let mut call = |method: &str, params: serde_json::Value| {
        let a = bus.call(machine.system_mut(), method, &params);
        if let bus::Answer::Err(e) = &a {
            refused += 1;
            loud(&format!(
                "bench-arm: {method} REFUSED {} {}",
                e.code, e.message
            ));
        }
    };
    for i in 0..16 {
        call(
            stopping::BREAKPOINT_ADD,
            serde_json::json!({"addr": format!("0x{:06X}", 0x200 + i * 4), "enabled": false,
                               "label": format!("bench{i}")}),
        );
    }
    call(
        stopping::WATCHPOINT_ADD,
        serde_json::json!({"addr": "0xFF0000", "len": 0x10000u64, "space": "bus",
                           "read": false, "write": true, "label": "bench-ram"}),
    );
    call(
        stopping::SET_PROFILER,
        serde_json::json!({"enabled": true, "perFrame": true, "callers": false}),
    );

    // **Loud on unmeasurable.** A fixture that armed nothing would leave three empty panels and the run
    // would report their cost as though it had measured full ones — the silent wrong answer this whole
    // flag exists to avoid. So the state is read back from the instruments themselves, not assumed from
    // the calls having been made.
    let breakpoints = bus.read_breakpoints().len();
    let (watch, _, armed) = bus.read_instruments();
    let watches = watch.watch_count();
    if refused > 0 || breakpoints == 0 || watches == 0 || !armed {
        loud(&format!(
            "REFUSING TO MEASURE: --bench-arm armed {breakpoints} breakpoints, {watches} watches, \
             profiler={armed} ({refused} refusals). The panels would be EMPTY and this run would report \
             the cost of three headlines as the cost of three panels."
        ));
        std::process::exit(2);
    }
    loud(&format!(
        "bench-arm: {breakpoints} breakpoints (disabled), {watches} watch, profiler armed with perFrame \
         — the panels have rows"
    ));
}

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
    let mut lp = Loop::new(
        machine,
        start,
        args.target_fps,
        args.rom.clone(),
        loaded,
        args.socket.clone(),
    );
    if args.dock_every_tab {
        // The measurement arrangement. Announced, because a run whose layout differs from the default
        // must say so in its own output or its numbers get compared against ones taken under the other.
        loud("dock: EVERY TAB IN ITS OWN LEAF — every panel body runs every frame (--dock every-tab)");
        lp.dock = ui::every_tab_dock();
    }
    if args.bench_arm {
        arm_for_measurement(&mut lp);
    }
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
    // **`--dock every-tab` suppresses persistence in either direction**, and that is the flag being
    // honest rather than a special case: it is a measurement arrangement, so restoring over it would
    // defeat it and saving it would overwrite the operator's own layout with a bench rig.
    let persist = args.mode == Mode::Window && !args.dock_every_tab;
    let mut app = App {
        lp: Loop::new(
            machine,
            start,
            args.target_fps,
            args.rom.clone(),
            loaded,
            args.socket.clone(),
        ),
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
    if args.dock_every_tab {
        loud("dock: EVERY TAB IN ITS OWN LEAF — layout persistence is OFF for this run (--dock every-tab)");
        app.lp.dock = ui::every_tab_dock();
    }
    if args.bench_arm {
        arm_for_measurement(&mut app.lp);
    }
    let scratch = (!persist).then(|| {
        std::env::temp_dir().join(format!("oracle-player-bench-{}.ron", std::process::id()))
    });
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([args.size.0, args.size.1])
            .with_title(ui::APP_NAME),
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

    /// ★ **The measurement fixture actually arms something**, and the panels it is measured through
    /// actually have rows.
    ///
    /// This is the anti-vacuity gate for the *measurement* rather than for a test. If
    /// [`arm_for_measurement`]'s calls were refused — a renamed param, a cap, a `require_paused` nobody
    /// expected — the run would draw three empty panels and the report would name their cost as the cost
    /// of three full ones. `arm_for_measurement` exits(2) on that in production; here it is checked, so
    /// the failure is a red test rather than a silently smaller number in a table.
    ///
    /// **The third assertion:** the instruments are read back through the panels' own view functions and
    /// asserted to be drawing rows — `Live::has_rows()` and a non-empty table. Asserting only that the
    /// `Host::call`s succeeded would go green on a fixture that armed things the panels do not show.
    #[test]
    fn the_measurement_fixture_puts_rows_in_all_three_panels() {
        let mut lp = a_loop();
        arm_for_measurement(&mut lp);

        let bp = stopping::breakpoints(lp.bus.read_breakpoints(), None);
        assert_eq!(bp.rows.len(), 16, "the fixture should arm sixteen rows");
        assert_eq!(
            bp.armed, 0,
            "the fixture's breakpoints must be DISABLED — an enabled one at an executed address halts \
             the player and there is no run to measure"
        );
        assert!(
            bp.live.has_rows(),
            "an empty Breakpoints table measures nothing"
        );

        let (w, p, armed) = lp.bus.read_instruments();
        let wv = stopping::watches(w);
        assert_eq!(wv.watches.len(), 1);
        assert!(wv.live.has_rows());
        assert!(
            armed,
            "the profiler must be armed or its panel draws a headline and nothing else"
        );
        assert!(
            p.per_frame_armed(),
            "perFrame is what fills the per-frame ring"
        );
        assert_eq!(
            stopping::profiler_live(p, armed),
            stopping::Live::Yes,
            "a measurement of the Profiler panel taken while it reads NEVER ARMED is a measurement of a \
             paragraph of text"
        );

        // …and once the machine runs, the panels have rows that cost something to draw. Without this the
        // three assertions above are satisfied by an armed instrument that never recorded anything.
        let ctx = egui::Context::default();
        for _ in 0..8 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(1600.0, 1000.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |root| {
                let c = root.ctx().clone();
                lp.iterate(&c, root, Instant::now());
            });
            out.textures_delta.clear();
        }
        let (w, p, armed) = lp.bus.read_instruments();
        assert!(
            !w.hits().is_empty(),
            "the work-RAM watch recorded nothing in eight frames, so the hit log the Watchpoints panel \
             is measured drawing is EMPTY"
        );
        assert!(
            p.routine_count() > 0,
            "the profiler recorded no routines, so the grid the Profiler panel is measured drawing is \
             EMPTY — and its per-frame sort, the one thing here that is not O(1), is sorting nothing"
        );
        assert!(armed, "the run must not have disarmed the instrument");
    }

    /// ★ **The SHIPPED loop acts on the drain's report** — not merely the helper beside it.
    ///
    /// `crate::bus`'s tests drive `bus::drain` directly, which is the right level for *what* each
    /// `PumpReport` field makes the window do. What none of them can see is whether `Loop::iterate` calls
    /// it at all: reverted to `Bus::mirror_pause`, this crate would compile, every one of those tests
    /// would stay green, and the window would be back to discarding the report. `#[must_use]` narrows that
    /// to a warning, and a warning is silenced with `let _`.
    ///
    /// So this drives the real `Loop::iterate` — governor, egui context, panels and all — and observes the
    /// one repair that nothing else in an iteration can produce: the symbol cache. Frames do not touch it,
    /// the UI does not touch it, and `Host::call` cannot raise `rom_changed` because it is not a drain.
    /// `lp.symbols` going `None` here means `bus::drain` ran inside the shipped loop.
    ///
    /// **The alternative green paths, ruled out:** the cache is asserted `Some` with its count before the
    /// client is spawned (it did not start empty); the reload's own `symbolsDropped` is asserted `true`
    /// (the engine really discarded it, so `None` cannot come from somewhere else); and the loop is left
    /// running rather than paused, so nothing here depends on a contrivance about frames.
    #[test]
    fn the_shipped_loop_re_derives_its_symbol_cache_when_a_client_reloads_the_rom() {
        use std::io::{BufRead as _, Write as _};

        let tag = format!("{}-{}", std::process::id(), line!());
        let socket = std::env::temp_dir().join(format!("pl-{tag}.sock"));
        let rom_path = std::env::temp_dir().join(format!("pl-{tag}.bin"));
        std::fs::write(&rom_path, oracle_core::testrom::build()).expect("write the fixture ROM");

        let table = oracle_core::symbols::SymbolTable::parse(crate::bus::pumped::LST)
            .expect("parse the fixture listing");
        let count = table.len();
        let mut lp = Loop::new(
            Machine::new(oracle_core::testrom::build(), None),
            Instant::now(),
            Some(0.0),
            rom_path.display().to_string(),
            symbols::Loaded {
                table: Some(table),
                path: None,
                fatal: None,
            },
            // A **private** path, not the well-known default `a_loop` declines to bind: nothing else can
            // collide with it and nothing on the developer's box is looking for it.
            Some(Some(socket.clone())),
        );
        assert_eq!(
            lp.symbols.as_ref().map(|t| t.len()),
            Some(count),
            "the fixture loop must start out holding the listing"
        );

        let path = rom_path.display().to_string();
        let client = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let stream = loop {
                match std::os::unix::net::UnixStream::connect(&socket) {
                    Ok(s) => break s,
                    Err(e) => {
                        assert!(Instant::now() < deadline, "connect: {e}");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut send = |v: serde_json::Value| {
                writeln!(writer, "{v}").unwrap();
                writer.flush().unwrap();
            };
            let recv = |reader: &mut std::io::BufReader<std::os::unix::net::UnixStream>| loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).expect("read") > 0, "hung up");
                let v: serde_json::Value = serde_json::from_str(&line).expect("bad JSON");
                if v.get("id").is_some_and(|i| !i.is_null()) {
                    return v;
                }
            };
            send(
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "clientId":"loop-wiring","clientName":"loop","clientVersion":"0",
                "protocolVersion":1,"clientCapabilities":{"events":false}}}),
            );
            recv(&mut reader);
            send(serde_json::json!({"jsonrpc":"2.0","method":"initialized"}));
            // `reload_rom` is refused against a running machine, so the client stops this window first —
            // which is exactly what a real one has to do.
            send(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"emulator/pause","params":{}}));
            recv(&mut reader);
            send(
                serde_json::json!({"jsonrpc":"2.0","id":3,"method":"emulator/reload_rom",
                "params":{"path": path}}),
            );
            recv(&mut reader)
        });

        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(20);
        while lp.symbols.is_some() {
            assert!(
                Instant::now() < deadline,
                "the client's reload never reached the shipped loop's symbol cache — `Loop::iterate` is \
                 not acting on the drain's report"
            );
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |root| {
                let c = root.ctx().clone();
                lp.iterate(&c, root, Instant::now());
            });
            out.textures_delta.clear();
            std::thread::sleep(Duration::from_millis(1));
        }
        let reply = client.join().expect("the client thread");
        assert_eq!(
            reply["result"]["symbolsDropped"],
            serde_json::json!(true),
            "the engine kept the listing, so the `None` above is not evidence of a re-derivation"
        );
        assert!(
            lp.bus.symbols().is_none(),
            "the engine still holds a listing it said it dropped"
        );
        let _ = std::fs::remove_file(&rom_path);
    }

    /// ★ **A client can read this window's glass, and what it reads follows the window** —
    /// `emulator/screen_text` (§11.29, CR-H), the gap design §5.8.2 booked as *"unwired here"*.
    ///
    /// **Driven over a real socket, because nothing else reaches the code.** `Host::pump` snapshots its
    /// generation counters at its own top (`host.rs:640`), so an in-process `Host::call` deliberately does
    /// not surface changes the way a client's does; and the push under test lives in `Loop::iterate`,
    /// which `crate::screen`'s unit tests cannot run. A private `/tmp` path, never `$XDG_RUNTIME_DIR` —
    /// `crate::bus::serving`'s header has the three reasons.
    ///
    /// # ⚑ The two states start DELIBERATELY OUT OF STEP, and that is the whole anti-vacuity measure
    ///
    /// The fixture loop is **running**, so its bar says `⏸ pause`; the client then stops it, so the bar
    /// must come to say `▶ resume`. A snapshot composed once at setup, one pushed from a hardcoded run
    /// state, or one pinned to the value `Loop::new` was built with passes *neither* transition — whereas
    /// a test that started paused would be green against all three, because the answer it wanted was
    /// already true before anything ran. The two reads are then asserted **unequal**, which is the
    /// assertion a re-publishing-every-frame-anyway implementation cannot fake.
    ///
    /// # The alternative green paths, each ruled out by a named assertion
    ///
    /// 1. *The fixture already had a snapshot, so the first read witnesses nothing.* Checked in-process
    ///    **before the client is spawned**: `emulator/screen_text` refuses `-32005 noDisplay`. That is also
    ///    this test's loud-on-unmeasurable clause — *we have not drawn yet* arrives as a typed refusal,
    ///    never as an empty surface list that would read as *the screen is blank*.
    /// 2. *The loop was paused all along, so `⏸ pause` proves nothing.* `machine.frames() > 0` says the
    ///    window really was advancing the machine when it said it.
    /// 3. *The client's pause never took effect and the label changed for some other reason.* The frame
    ///    counter is re-read after the flip and asserted **stopped**.
    /// 4. *The push happens but reports something other than the bar.* The line is asserted to carry all
    ///    three pieces the bar draws — the app name, the transport labels, the loop's own status string —
    ///    and the title bar to be a separate surface with the window-manager's own text.
    #[test]
    fn a_client_reads_this_windows_top_bar_and_it_follows_the_run_state() {
        use std::io::{BufRead as _, Write as _};

        let tag = format!("{}-{}", std::process::id(), line!());
        let socket = std::env::temp_dir().join(format!("pst-{tag}.sock"));
        let unlink = socket.clone();
        let mut lp = Loop::new(
            Machine::new(oracle_core::testrom::build(), None),
            Instant::now(),
            // The governor OFF, so every iteration owns its frame and this test never waits on a clock.
            Some(0.0),
            String::from("(fixture)"),
            symbols::Loaded {
                table: None,
                path: None,
                fatal: None,
            },
            Some(Some(socket.clone())),
        );
        assert!(
            lp.bus.is_serving(),
            "the fixture did not bind {}, so no client can reach it and nothing below is a test",
            socket.display()
        );
        // (1) The premise, in-process and before anyone attaches: nothing has been drawn, and the bus
        // says so with a typed refusal rather than an empty list.
        let probe = lp.bus.call(
            lp.machine.system_mut(),
            "emulator/screen_text",
            &serde_json::json!({}),
        );
        assert_eq!(
            probe.reason(),
            Some("noDisplay"),
            "the fixture already had a screen-text snapshot (or refused for another reason), so the \
             reads below would witness nothing"
        );

        let client = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let stream = loop {
                match std::os::unix::net::UnixStream::connect(&socket) {
                    Ok(s) => break s,
                    Err(e) => {
                        assert!(Instant::now() < deadline, "connect: {e}");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut id = 0i64;
            let mut call = |reader: &mut std::io::BufReader<std::os::unix::net::UnixStream>,
                            method: &str,
                            params: serde_json::Value| {
                id += 1;
                writeln!(
                    writer,
                    "{}",
                    serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
                )
                .unwrap();
                writer.flush().unwrap();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).expect("read") > 0, "hung up");
                    let v: serde_json::Value = serde_json::from_str(&line).expect("bad JSON");
                    if v.get("id").is_some_and(|i| !i.is_null()) {
                        assert!(v.get("error").is_none(), "{method} failed: {}", v["error"]);
                        return v["result"].clone();
                    }
                }
            };
            call(
                &mut reader,
                "initialize",
                serde_json::json!({"clientId":"screen-text","clientName":"st","clientVersion":"0",
                    "protocolVersion":1,"clientCapabilities":{"events":false}}),
            );
            // No `initialized` notification: `Session::on_message` gates ordinary methods on the
            // `initialize` REQUEST alone (session.rs:96), and `initialized` only opens the event
            // subscription this client declined. Omitted deliberately, not forgotten.
            let status = call(&mut reader, "emulator/status", serde_json::json!({}));

            // The window is RUNNING here. Its bar says `⏸ pause`, and this read is what pins that.
            let running = call(&mut reader, "emulator/screen_text", serde_json::json!({}));
            call(&mut reader, "emulator/pause", serde_json::json!({}));

            // …and now it must come to say `▶ resume`. Polled with a deadline that FAILS rather than
            // falling through to a green assertion on the last thing read.
            let deadline = Instant::now() + Duration::from_secs(20);
            let paused = loop {
                let v = call(&mut reader, "emulator/screen_text", serde_json::json!({}));
                if v["surfaces"][1]["text"]
                    .as_str()
                    .expect("a statusLine surface")
                    .contains(crate::ui::RESUME_LABEL)
                {
                    break v;
                }
                assert!(
                    Instant::now() < deadline,
                    "the window never came to say `{}` after the client paused it: {v}",
                    crate::ui::RESUME_LABEL
                );
                std::thread::sleep(Duration::from_millis(2));
            };
            (status, running, paused)
        });

        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !client.is_finished() {
            assert!(
                Instant::now() < deadline,
                "the client never finished — `Loop::iterate` is not publishing the bar, or not draining"
            );
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |root| {
                let c = root.ctx().clone();
                lp.iterate(&c, root, Instant::now());
            });
            out.textures_delta.clear();
            std::thread::sleep(Duration::from_millis(1));
        }
        let (status, running, paused) = client.join().expect("the client thread");

        // (2) The window really was advancing the machine when its bar said `⏸ pause`.
        let ran = lp.machine.frames();
        assert!(
            ran > 0,
            "the fixture never ran a frame, so `{}` in the first read is the paused default rather \
             than the state this test set out of step",
            crate::ui::PAUSE_LABEL
        );

        assert_eq!(
            status["display"],
            serde_json::json!(true),
            "`emulator/status` must let a client ASK whether there is a window rather than probe by \
             provoking a refusal (§11.29's rider)"
        );

        // --- the shape of the answer ---
        for (what, v) in [("running", &running), ("paused", &paused)] {
            assert_eq!(v["total"], serde_json::json!(2), "{what}: two surfaces");
            assert_eq!(v["returned"], serde_json::json!(2), "{what}: none elided");
            assert_eq!(v["surfaces"][0]["kind"], serde_json::json!("titleBar"));
            assert_eq!(
                v["surfaces"][0]["text"],
                serde_json::json!(crate::ui::APP_NAME),
                "{what}: the title bar carries the window manager's own string"
            );
            assert_eq!(
                v["surfaces"][0]["unrenderable"],
                serde_json::json!([]),
                "{what}: REQUIRED even when empty — absent and none must not be one artifact"
            );
            assert_eq!(v["surfaces"][1]["kind"], serde_json::json!("statusLine"));
            let line = v["surfaces"][1]["text"].as_str().expect("a string");
            assert!(!line.is_empty(), "{what}: a blank line is not an answer");
            assert!(
                line.contains(crate::ui::APP_NAME)
                    && line.contains(crate::ui::STEP_LABEL)
                    && line.contains("frames")
                    && line.contains("rebases"),
                "{what}: the line must be the WHOLE bar — name, transport, and the loop's own status \
                 string: {line:?}"
            );
            assert_eq!(
                v["surfaces"][1]["unrenderable"],
                serde_json::json!([]),
                "{what}: this build draws every character of its own top bar — a hollow box here is \
                 the F-FONT-* defect class, measured from the atlas rectangle each glyph samples \
                 (`screen::Glyphs`; egui's own `has_glyph` reports 25 boxes here that are not there): \
                 {line:?}"
            );
            assert_eq!(
                v["surfaces"][1]["truncated"],
                serde_json::json!(false),
                "{what}: `rendered` equals `text` for this window (F-PLAYER-SCREENTEXT-CLIP)"
            );
        }

        let a = running["surfaces"][1]["text"].as_str().unwrap();
        let b = paused["surfaces"][1]["text"].as_str().unwrap();
        assert!(
            a.contains(crate::ui::PAUSE_LABEL) && !a.contains(crate::ui::RESUME_LABEL),
            "a running window offers `{}`: {a:?}",
            crate::ui::PAUSE_LABEL
        );
        assert!(
            b.contains(crate::ui::RESUME_LABEL) && !b.contains(crate::ui::PAUSE_LABEL),
            "a paused window offers `{}`: {b:?}",
            crate::ui::RESUME_LABEL
        );
        assert_ne!(
            a, b,
            "the two reads are the same string, so the snapshot was composed once and never again — \
             the agreement above is two copies of one untouched value"
        );

        // (3) …and the pause really is what stopped it: the counter does not move again.
        let ctx2 = egui::Context::default();
        let (frames, _) = drive(&mut lp, &ctx2, 5);
        assert_eq!(
            frames, 0,
            "the window ran {frames} more frames after the client paused it, so the label flipped \
             without the run state following it"
        );
        assert_eq!(lp.machine.frames(), ran, "…and the total is unchanged");
        drop(lp);
        let _ = std::fs::remove_file(&unlink);
    }

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
            // The fixture loop binds nothing. A test that served would put a real socket on the
            // developer's box for the length of a `cargo test`, and on the well-known path at that.
            None,
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

// -------------------------------------------------------------------------------------------------------
// ⚑ PLAYER-SERVE — the three switches that fold into one three-state value
// -------------------------------------------------------------------------------------------------------

#[cfg(test)]
mod socket_args {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn parse(argv: &[&str]) -> Args {
        parse_args_from(argv.iter().map(|s| s.to_string()).collect(), None)
    }

    fn parse_env(argv: &[&str], env: &str) -> Args {
        parse_args_from(
            argv.iter().map(|s| s.to_string()).collect(),
            Some(OsString::from(env)),
        )
    }

    /// **The control, first.** A launch with no switch binds nothing, and that has to be established
    /// before any of the "turns it on" assertions mean anything — every one of them would pass just as
    /// green against a default of `Some(None)`.
    #[test]
    fn the_default_is_no_socket() {
        assert_eq!(parse(&["--rom", "r.bin"]).socket, None);
    }

    /// `--aether` alone means *serve on the contract's resolved default* — `Some(None)`, which is not the
    /// same value as `Some(Some(default_path))` and must not be flattened into one. The parser does not
    /// resolve; §7.1's resolver does, at bind time, and it reads the environment.
    #[test]
    fn the_bare_flag_asks_for_the_resolved_default_and_does_not_resolve_it_here() {
        assert_eq!(parse(&["--aether", "--rom", "r.bin"]).socket, Some(None));
    }

    /// `--socket PATH` implies `--aether`, and **`--socket P --aether` still binds `P`**.
    ///
    /// ⚑ That second case is the one worth having. Written as `socket = Some(None)`, `--aether` would
    /// silently discard a path the operator named and bind the well-known one instead — the exact
    /// "silently moved to a second path" failure the usage text promises does not happen. Both orders are
    /// checked, because a fold that is order-dependent is a fold that will be got wrong once.
    #[test]
    fn a_named_path_survives_the_bare_flag_in_either_order() {
        let want = Some(Some(PathBuf::from("/tmp/x.sock")));
        assert_eq!(
            parse(&["--socket", "/tmp/x.sock", "--rom", "r.bin"]).socket,
            want
        );
        assert_eq!(
            parse(&["--socket", "/tmp/x.sock", "--aether", "--rom", "r.bin"]).socket,
            want,
            "--aether after --socket must not discard the named path"
        );
        assert_eq!(
            parse(&["--aether", "--socket", "/tmp/x.sock", "--rom", "r.bin"]).socket,
            want,
            "…and --socket after --aether must not be ignored"
        );
    }

    /// `ORACLE_AETHER` is a third spelling of the same decision, folded at the same place — and
    /// **`ORACLE_AETHER=0` is an explicit off, not a present-therefore-on**. Copied from
    /// `oracle-frontend`, including that asymmetry, because two spellings of one switch behaving
    /// differently in the two players is the drift this shape exists to prevent.
    #[test]
    fn the_environment_turns_it_on_and_zero_turns_it_off() {
        assert_eq!(parse_env(&["--rom", "r.bin"], "1").socket, Some(None));
        assert_eq!(
            parse_env(&["--rom", "r.bin"], "0").socket,
            None,
            "ORACLE_AETHER=0 is an explicit refusal, not a set variable"
        );
        // A flag still wins over an environment that said no — the flag is the more specific statement.
        assert_eq!(
            parse_env(&["--aether", "--rom", "r.bin"], "0").socket,
            Some(None)
        );
        // …and a named path composes with the environment rather than being overridden by it.
        assert_eq!(
            parse_env(&["--socket", "/tmp/y.sock", "--rom", "r.bin"], "1").socket,
            Some(Some(PathBuf::from("/tmp/y.sock")))
        );
    }

    /// The switches change **nothing else**. A flag that quietly moved the mode, the rate or the dock
    /// would be a flag doing something nobody asked it to, and this is cheap to pin.
    #[test]
    fn serving_does_not_disturb_any_other_argument() {
        let plain = parse(&[
            "--rom",
            "r.bin",
            "--mode",
            "bench-cpu",
            "--expect-screen",
            "1x1",
        ]);
        let served = parse(&[
            "--rom",
            "r.bin",
            "--mode",
            "bench-cpu",
            "--expect-screen",
            "1x1",
            "--aether",
        ]);
        assert_eq!(plain.rom, served.rom);
        assert_eq!(plain.expect_screen, served.expect_screen);
        assert_eq!(plain.secs, served.secs);
        assert_eq!(plain.target_fps, served.target_fps);
        assert_eq!(plain.dock_every_tab, served.dock_every_tab);
        assert_eq!(plain.bench_arm, served.bench_arm);
        assert!(plain.mode == Mode::BenchCpu && served.mode == Mode::BenchCpu);
        assert_ne!(plain.socket, served.socket, "…and the one it does change");
    }
}
