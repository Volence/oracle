//! The player's side of the **Aether capability layer**, hosted in this process.
//!
//! The decision this implements: *the player process owns the machine, hosts the capability layer, and serves
//! Aether from it.* It is forced by the contract rather than chosen for convenience — a checkpoint is "a
//! serialization of the live emulator struct" (D13/§6.1), every reply carries the machine's `frame`/`mclk` at
//! reply time (D11), and the trust model is the emulator process serving a loopback-only socket it created
//! (D8). None of those can be honoured by a layer that does not have hands on this `System`.
//!
//! Everything hard lives in [`oracle_aether::host`]. What is left here is the four places the player's own
//! loop has to meet it, and this module exists mostly so those four places read as one thing:
//!
//! 1. **Pause is one flag.** The player's pause state *is* the bus's free-run state, and a client's
//!    `emulator/pause` is the player's. Written before the drain, read back after it.
//! 2. **Pads merge.** A client's held set ORs with live keyboard/gamepad input, exactly as the keyboard and
//!    the gamepad already OR with each other.
//! 3. **The picture is published.** The frame the window drew is the frame `emulator/screenshot` serves.
//! 4. **The drain is bounded.** One call per iteration, `try_recv`-only, with a wall-clock budget — see
//!    [`oracle_aether::host::HostConfig::pump_budget`].
//!
//! **Serving is opt-in** (`--aether`, or `--socket PATH`). Without it nothing binds, no filesystem entry is
//! created, no thread starts, and the pump is an identity operation on the machine.
//!
//! The mirror image of this file, [`crate::bus`] in a build without the `aether` feature, is a set of
//! no-ops with the same surface — which is why the run loop calls into it unconditionally and has no
//! `#[cfg]` of its own.

use crate::spawn;
use oracle_aether::breakpoints::BreakStop;
use oracle_aether::host::{Host, HostConfig};
use oracle_core::bus::Observe;
use oracle_core::io::Pad;
use oracle_core::profiler::Profiler;
use oracle_core::scanline_capture::ScanlineCapture;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use oracle_core::watchpoints::Watchpoints;
use serde_json::{json, Value};
use std::path::PathBuf;

/// What the bus should know about the loaded cartridge — the ROM's path, and the listing bound to it (D7).
/// Declared here rather than re-exported so the stub build has the identical type.
#[derive(Default)]
pub struct MachineInfo {
    pub rom_path: Option<String>,
    pub symbols: Option<SymbolTable>,
    pub symbols_path: Option<String>,
}

impl From<MachineInfo> for oracle_aether::host::MachineInfo {
    fn from(m: MachineInfo) -> Self {
        Self {
            rom_path: m.rom_path,
            symbols: m.symbols,
            symbols_path: m.symbols_path,
        }
    }
}

/// What one [`Bus::pump`] did, in the terms the run loop reacts to. Deliberately not `oracle-aether`'s own
/// report type: the stub build has to produce this too, and the loop must not be able to tell them apart.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pumped {
    /// The machine's clock moved under the loop (a client-driven run, or a checkpoint restore that rewound
    /// it). Audio, the frame counter and the scanline capture all have to resynchronise, exactly as they do
    /// after a save-state load.
    pub timeline_moved: bool,
    /// A client-driven run replaced the picture — present [`Bus::present_frame`] instead of the retained
    /// framebuffer, which is now a frame from before the run.
    pub screen_changed: bool,
    /// The cartridge was replaced (`emulator/reload_rom`, or a restore that may have). Anything derived from
    /// the ROM bytes is stale.
    pub rom_changed: bool,
    /// **A client replaced the symbol listing** (`emulator/load_symbols`), or a cartridge change replaced
    /// or dropped it. This window keeps its own clone of the listing — `dump_hits` symbolises watchpoint
    /// PCs out of it and the panels resolve names against it — so a stale clone is the D7 drift with the
    /// engine on the other side of it: `emulator/lookup_symbol` answering out of one listing while this
    /// window names addresses out of another.
    ///
    /// **Not folded into [`rom_changed`](Pumped::rom_changed)**, whose reaction here re-keys every
    /// save-state slot and puts a line in the overlay saying the cartridge was replaced. Loading symbols
    /// replaces no cartridge, and a slot re-key for a listing change is a repair that invalidates work
    /// nothing invalidated.
    pub symbols_changed: bool,
    /// Whole emulated frames the drain advanced.
    pub frames_advanced: u64,
}

/// What a launch that was never asked to serve prints, so that "the bus is off" is a sentence somewhere
/// rather than the absence of one.
///
/// It names the three ways to turn it on because the reader of this line is, by construction, someone who
/// wanted the bus and did not get it — a message that reports a state without naming the remedy sends them
/// to the `--help` this line could have been. [`crate::bus_stub`] carries its own twin of this constant
/// (same opening words, a different reason), and a test in each file pins that opening so the two builds
/// cannot start describing the same state differently.
const NOT_SERVING: &str =
    "aether: not serving — no --aether given, so nothing can attach to this window \
     (pass --aether, or --socket PATH, or set ORACLE_AETHER=1)";

/// The hosted capability layer, or an inert placeholder when `--aether` was not asked for.
pub struct Bus {
    host: Host,
}

impl Bus {
    /// Build the layer. When `socket` is `None` **nothing is bound** — the returned bus is inert and every
    /// method below is a no-op, which is what makes the default launch identical to a build without any of
    /// this.
    ///
    /// A bind failure is reported and degraded to inert, never fatal: someone who launched a game to play it
    /// should not be stopped by a stale socket file, and the message says exactly what did not happen.
    ///
    /// **All three outcomes say so out loud, including the default one.** Until 2026-08-29 the `None` arm
    /// printed nothing at all, so a launch without `--aether` emitted no line about Aether anywhere — and an
    /// absence is not a statement. The observed cost was the owner launching twice in one evening, going to a
    /// client, and finding it offline with nothing on either side able to say why; a bus that is off is a fact
    /// about this window, and the window is the thing that has to report it (aurora's ask, 2026-08-28).
    pub fn start(socket: Option<Option<PathBuf>>, info: MachineInfo) -> Self {
        // The hit ring is sized by the *player*, not by the headless default: this instrument is shared with
        // the pixel-attribution panel (see [`Bus::watchpoints_mut`]), and a panel that silently held fewer
        // hits when the bus happened to be compiled in would be the drift item 19 exists to prevent, wearing
        // a config value's clothes.
        let mut config = HostConfig::default();
        config.engine.watch_ring_cap = crate::WATCH_CAP;
        let mut host = Host::new(config);
        host.set_machine_info(info.into());
        // A `match` rather than `if let ... else`, so that **deleting the silent case is a compile error**
        // rather than a silent regression. That is the defect being repaired here: the arm that says nothing
        // is the one nothing can detect the absence of, and no test over this file's output could have caught
        // its removal — a unit test cannot read `println!`, and a test that could would be testing the
        // harness. The type system can, so it does.
        match socket {
            Some(path) => match host.serve(path) {
                Ok(p) => println!(
                    "aether: serving on {} (mode 0600, {} methods, protocol version {})",
                    p.display(),
                    oracle_aether::engine::METHODS.len(),
                    oracle_aether::rpc::PROTOCOL_VERSION
                ),
                Err(e) => eprintln!("aether: NOT serving — cannot bind the socket ({e})"),
            },
            None => println!("{NOT_SERVING}"),
        }
        Self { host }
    }

    /// Whether the bus is actually bound and reachable. The status line's `AETHER` field reads this rather
    /// than re-deriving it from the command line, so a launch that asked to serve and failed to bind reads
    /// `AETHER OFF` — true, and the answer a flag-derived field would get wrong in the one case that matters.
    pub fn is_serving(&self) -> bool {
        self.host.is_serving()
    }

    /// **The display layer mask — the engine's, not a copy of it.**
    ///
    /// The window's layer toggles and a client's `emulator/set_layer_enabled` move *the same*
    /// [`LayerMask`](oracle_core::render::LayerMask), for the reason the watch ring is shared one field up:
    /// a second mask on this side would mean a socket client hiding plane A changed
    /// `emulator/screenshot` and not the picture on the monitor, and the palette doing the reverse. There is
    /// nothing here to drift apart from — this is a lend, not a mirror.
    ///
    /// It is the engine's for a second reason worth stating: the mask is engine state, so it survives
    /// `reset` / `reload_rom` / `restore` (all three replace the `System`, none touches it) and appears in
    /// no snapshot and no hash. A frontend-owned mask would have had to re-establish all of that by hand.
    pub fn layers(&self) -> oracle_core::render::LayerMask {
        self.host.layers()
    }

    /// Set one layer's mask bit. `false` means `layer` is not a mask target (the backdrop), and the mask is
    /// left untouched rather than pretending to have applied.
    pub fn set_layer(&mut self, layer: oracle_core::render::Layer, enabled: bool) -> bool {
        self.host.set_layer(layer, enabled)
    }

    /// Conflict 2 — merge the client's held set into the pads the loop is about to write. See
    /// [`Host::held`](oracle_aether::host::Host::held).
    ///
    /// **The body moved to [`Host::merge_held`](oracle_aether::host::Host::merge_held) in
    /// `HELD-PADS-PLAYER`** and this is now a delegation, because `oracle-player` needs the identical
    /// merge and a second copy of it is the drift that bar exists to prevent. The `is_serving()` early
    /// return that used to open this function is gone; `Host::merge_held`'s doc has the proof that it was
    /// a fast path rather than a semantic, and the test one crate over pins it.
    pub fn merge_held(&self, pads: [Pad; 2]) -> [Pad; 2] {
        self.host.merge_held(pads)
    }

    /// The other half of conflict 2: tell the bus what the human is holding, so `emulator/press` and
    /// `emulator/hold` compose with it instead of erasing it.
    pub fn set_live_pads(&mut self, pads: [Pad; 2]) {
        self.host.set_live_pads(pads);
    }

    /// **Publish the text this present just put on the glass**, so `emulator/screen_text` can answer with
    /// what a human can actually read on the window (contract §11.29, CR-H).
    ///
    /// The only place the frontend's presentation model
    /// ([`screen_text::Surface`](crate::screen_text::Surface)) becomes the bus's, and it is a translation and
    /// nothing else — no fitting, no composing, no filtering. The two types exist for the same reason
    /// [`MachineInfo`] has a twin in `bus_stub`: a build without the `aether` feature does not depend on
    /// `oracle-aether` at all, so the run loop's own vocabulary cannot be the bus's.
    ///
    /// `truncated` is not carried across: the engine derives it from `rendered != text`, so there is no flag
    /// here that could disagree with the pair beside it.
    pub fn set_screen_text(&mut self, surfaces: Vec<crate::screen_text::Surface>) {
        use crate::screen_text::Kind;
        use oracle_aether::engine::{ScreenSurface, ScreenSurfaceKind};
        self.host.set_screen_text(
            surfaces
                .into_iter()
                .map(|s| ScreenSurface {
                    kind: match s.kind {
                        Kind::TitleBar => ScreenSurfaceKind::TitleBar,
                        Kind::StatusLine => ScreenSurfaceKind::StatusLine,
                        Kind::Toast => ScreenSurfaceKind::Toast,
                    },
                    text: s.text,
                    rendered: s.rendered,
                    unrenderable: s.unrenderable,
                })
                .collect(),
        );
    }

    /// **Conflict 4: the watch instrument the player's own run loop must feed.**
    ///
    /// The player owns the loop, so a `Watchpoints` that only the bus's own runs fed would see nothing while
    /// the window is running the game — and would report `seen == 0`, which correctly means "the recorder was
    /// never attached" and is useless. So the loop arms and reads *this* one instead of a private instance,
    /// and the pixel-attribution panel and `emulator/watchpoint_hits` become two readers of one instrument
    /// rather than two instruments that have to be kept in step (contract §8 item 19).
    ///
    /// Returned unconditionally, not only while serving: a `Host` exists either way, and switching
    /// instruments on `--aether` would mean the panel behaved differently depending on whether a socket was
    /// bound — a difference nobody asked for and the harder one to debug.
    pub fn watchpoints_mut(&mut self) -> &mut Watchpoints {
        self.host.watchpoints_mut()
    }

    /// **Both instruments the player's run loop must feed** — the watch, and since CR-26 the profiler.
    ///
    /// The whole argument lives in [`Engine::run_sinks`](oracle_aether::engine::Engine::run_sinks): the
    /// arming conditions, the [`Observe`](oracle_core::bus::Observe) wrappers, and why the pair comes from
    /// one call (one run needs both, and two `&mut self` accessors cannot both be live in the sink
    /// expression the run requires). The player's job is only to put what comes back into the per-frame
    /// sink it already builds for the scanline capture.
    ///
    /// A profiler is the case that made this a pair: a client arms it over the socket while the window is
    /// playing, and a loop that fed only the watch would answer that client `frameCount: 0` with no rows —
    /// "the game did nothing" — about the frames it was watching go past.
    ///
    /// The third half is the **breakpoint sink, bare** — the halt is the one thing it is for, and an
    /// `Observe` around it would count hits on a window that never stopped. `resume_pc` is this machine's
    /// PC before the run, which the loop has and the engine (holding its placeholder `System` outside the
    /// drain) does not. Feed the observation back through [`break_observed`] and [`Bus::record_break`].
    pub fn run_sinks(
        &mut self,
        resume_pc: u32,
    ) -> (
        Option<Observe<&mut Watchpoints>>,
        Option<Observe<&mut Profiler>>,
        Option<BreakStop<'_>>,
    ) {
        self.host.run_sinks(resume_pc)
    }

    /// Hand back the halt the sink from [`run_sinks`](Bus::run_sinks) observed. Applied at the top of the
    /// next [`pump`](Bus::pump), which is what stamps its `emulator/stopped` with the real machine instead
    /// of the bus's placeholder.
    pub fn record_break(&mut self, addr: u32) {
        self.host.record_break(addr);
    }

    /// **What the lens layer reads, from one call** — the watch instrument, the profiler, and whether a
    /// client has the profiler armed.
    ///
    /// One call rather than three accessors because the draw pass needs all of them live at once inside a
    /// single `FrameCtx`, and `watchpoints_mut` is `&mut self`: the two borrows could not coexist. Shared
    /// borrows throughout, which also states the guarantee — a panel cannot move a number a client is
    /// gating on.
    ///
    /// The armed flag is not derivable from the accumulator: disarming RETAINS the sample, so rows exist
    /// whether or not anything is still recording, and a panel that showed only the rows could not tell
    /// the two apart.
    pub fn read_instruments(&self) -> (&Watchpoints, &Profiler, bool) {
        self.host.read_instruments()
    }

    /// Conflict 1, outbound: the player's pause state becomes the bus's free-run state.
    pub fn set_paused(&mut self, paused: bool) {
        self.host.set_paused(paused);
    }

    /// Conflict 1, inbound: whether a client has paused or resumed the machine.
    pub fn is_paused(&self) -> bool {
        self.host.is_paused()
    }

    /// Conflict 3, the common half: hand the bus the frame the window just drew. Skipped internally while
    /// nobody is connected, so an unwatched player pays nothing for it.
    pub fn publish(&mut self, cap: &ScanlineCapture) {
        self.host.publish_capture(cap);
    }

    /// Conflict 3, the other half: after a client-driven run the window's retained framebuffer is a frame
    /// from before the run, and the bus is holding the one the raster actually drew. Pack it into `buf` and
    /// return its width.
    pub fn present_frame(&self, buf: &mut Vec<u32>) -> Option<usize> {
        let (width, px) = self.host.framebuffer()?;
        if width == 0 || px.is_empty() {
            return None;
        }
        buf.clear();
        buf.reserve(px.len());
        for &(r, g, b) in px {
            buf.push((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
        }
        Some(width)
    }

    /// Drain every queued command against the machine. Bounded, non-blocking, and the only point in the loop
    /// where anything other than the player touches the `System`.
    pub fn pump(&mut self, sys: &mut System) -> Pumped {
        let r = self.host.pump(sys);
        Pumped {
            timeline_moved: r.timeline_moved(),
            screen_changed: r.screen_changed,
            rom_changed: r.rom_changed,
            symbols_changed: r.symbols_changed,
            frames_advanced: r.frames_advanced(),
        }
    }

    /// **The listing the engine resolves against right now**, for the loop to re-derive its own clone from
    /// after [`Pumped::symbols_changed`]. `None` both when nothing was ever loaded and when a
    /// `reload_rom` dropped one on the D7 check — the two are the same state to a caller, which is why
    /// this returns the engine's answer rather than reporting a delta.
    pub fn symbols(&self) -> Option<&SymbolTable> {
        self.host.symbols()
    }

    /// Re-point the bus at a cartridge the *player* just swapped (F5). Without this `emulator/status` keeps
    /// naming the previous image and `read_memory {symbol}` keeps resolving against its listing — the exact
    /// stale-symbol hazard D7 exists to prevent.
    pub fn set_machine_info(&mut self, info: MachineInfo) {
        self.host.set_machine_info(info.into());
    }

    // ---------------------------------------------------------------- spawn mode (LIVE-OBJECTS)

    /// **Every archetype a click could place**, out of the listing this engine resolves against.
    ///
    /// One `Host::call` to `emulator/lookup_symbol`'s bounded prefix search — the row §11.32 §9.1 names
    /// as already being the archetype catalogue, which is why no catalogue row was proposed. Nothing is
    /// hard-coded here and nothing is cached: `load_symbols` may be called at any point after the
    /// handshake, so the list is read at the moment the mode is armed and the mode is disarmed whenever
    /// the machine's listing changes.
    ///
    /// Every failure comes back as the server's own words (`-32012` *you forgot to load symbols* against
    /// `-32013` *this build has no such name* is exactly the distinction a person hits here, and §8.2
    /// keeps them apart on purpose).
    pub fn archetypes(&mut self, sys: &mut System) -> Result<spawn::Archetypes, spawn::Refusal> {
        let v = self.call(
            sys,
            "emulator/lookup_symbol",
            json!({"name": spawn::ARCHETYPE_PREFIX}),
        )?;
        // The exact branch: a symbol literally named `ObjDef_`. Vanishingly unlikely and handled anyway,
        // because the alternative is an empty list from a reply that found something.
        if v["exact"] == json!(true) {
            let name = v["name"].as_str().unwrap_or_default().to_string();
            return Ok(spawn::Archetypes {
                total: 1,
                names: vec![name],
            });
        }
        let page = &v["otherMatches"];
        let names: Vec<String> = page["items"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        // `total` from the envelope, not from `names.len()` — the two differ exactly when the search was
        // cut, which is the case the note exists for.
        let total = page["total"].as_u64().unwrap_or(names.len() as u64) as usize;
        Ok(spawn::Archetypes { names, total })
    }

    /// **Place `archetype` where the window was clicked.**
    ///
    /// Two calls, because the click is in *screen* dots and the mailbox wants *world* pixels:
    ///
    /// 1. `emulator/object_at`, whose `world{x,y}` is `Camera_X`/`Camera_Y` plus the dot (§11.26 M3, and
    ///    the join §11.32 §11 names as the GUI's one extra dependency). It is a pure read and needs no
    ///    pause.
    /// 2. `emulator/object_spawn { defSymbol, x, y }`, which is where the pause requirement, the mailbox
    ///    handshake and all five engine refusals live.
    ///
    /// **The `world` half is refused rather than guessed.** §11.26 makes `worldSource` a field precisely
    /// so its absence is not inferred from a missing `world`, and a build without the camera symbols gets
    /// a sentence instead of a coordinate — a spawn at the raw dot would land somewhere plausible and
    /// wrong, which is the failure class this whole row is written against.
    ///
    /// **UNMEASURED, and named rather than assumed** (§11.32 §11's own flag): that `object_at`'s world
    /// space is the same flat world-pixel space `Obj_Req_X`/`Y` want. Aeon states it is *"the same
    /// convention as `Warp_Req_X/Y`"*; nothing in this repo has confirmed the two agree against a running
    /// game, and if they do not, this needs a conversion that no CR has specified.
    pub fn spawn_at(
        &mut self,
        sys: &mut System,
        archetype: &str,
        dot: (u16, u16),
    ) -> Result<spawn::Placed, spawn::Refusal> {
        let (dx, dy) = dot;
        let at = self.call(sys, "emulator/object_at", json!({"x": dx, "y": dy}))?;
        let source = at["worldSource"].as_str().unwrap_or("unavailable");
        let world = match (source, at["world"]["x"].as_u64(), at["world"]["y"].as_u64()) {
            ("camera", Some(x), Some(y)) => (x as u32, y as u32),
            _ => {
                return Err(spawn::Refusal::local(format!(
                    "this build cannot turn a click into a world position (object_at answered \
                     worldSource={source:?}): `Camera_X` and `Camera_Y` are not both in the loaded \
                     listing, and spawning at the raw screen dot ({dx},{dy}) would place the object \
                     somewhere plausible and wrong"
                )))
            }
        };
        let placed = self.call(
            sys,
            "emulator/object_spawn",
            json!({"defSymbol": archetype, "x": world.0, "y": world.1}),
        )?;
        Ok(spawn::Placed {
            handle: placed["handle"].as_str().unwrap_or_default().to_string(),
            addr: placed["addr"].as_str().unwrap_or_default().to_string(),
            slot: placed["slot"].as_i64(),
            asked: world,
            now: (
                placed["x"].as_i64().unwrap_or_default(),
                placed["y"].as_i64().unwrap_or_default(),
            ),
            frames_advanced: placed["framesAdvanced"].as_u64().unwrap_or_default(),
            caveat: placed["caveat"].as_str().map(str::to_string),
        })
    }

    /// One synchronous in-process dispatch, with the error translated into this crate's vocabulary and
    /// **nothing else** — the message is moved, never rewritten.
    ///
    /// [`Host::call`](oracle_aether::host::Host::call) rather than the drain: the drain is the socket
    /// clients' path and is bounded so one of them cannot freeze the window, whereas a call the window
    /// makes of itself is its own frame time to spend. D15's *"an in-process GUI is a consumer of the same
    /// registry, not a second server"*, taken literally.
    ///
    /// Private, and only per-**gesture** callers reach it. A panel body repainting through here would be
    /// the arrangement `Host::call`'s own doc warns about; the per-frame readers in this crate go to the
    /// core directly, exactly as [`crate::pick`] does.
    fn call(
        &mut self,
        sys: &mut System,
        method: &str,
        params: Value,
    ) -> Result<Value, spawn::Refusal> {
        let (result, _stamp) = self.host.call(sys, method, &params);
        result.map_err(|e| spawn::Refusal {
            code: Some(e.code),
            reason: e
                .data
                .as_ref()
                .and_then(|d| d["reason"].as_str())
                .map(str::to_string),
            message: e.message,
        })
    }
}

/// The address a breakpoint sink halted the run on, or `None` if nothing fired.
///
/// A free function rather than a method because the sink borrows the [`Bus`] for the length of the run, and
/// the loop needs that borrow released before it can call [`Bus::record_break`] — consuming the sink here is
/// what ends it. The stub build has a twin with the identical signature and a body that can only answer
/// `None`, which is what keeps the run loop one shape.
pub fn break_observed(brk: Option<BreakStop<'_>>) -> Option<u32> {
    brk.and_then(|b| b.fired).map(|(_, addr)| addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::bus::Fanout;
    use oracle_core::scanline_capture::Retain;
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    /// **The default launch says the bus is off, and says how to turn it on.** The `None` arm printed nothing
    /// at all until 2026-08-29, and an absence is not a statement — the observable cost was a client reported
    /// as offline with no line anywhere explaining why.
    ///
    /// Asserted on the constant rather than by capturing stdout: `println!` in a test binary is captured by
    /// the harness rather than by us, and a test that shells out to grep its own output would be testing the
    /// harness. What can go wrong here is the *wording*, and that is what this pins.
    #[test]
    fn the_not_serving_line_names_the_state_and_the_remedy() {
        // The shared opening, which `bus_stub.rs` pins on its own twin so the two builds cannot describe the
        // same state in different words.
        assert!(
            NOT_SERVING.starts_with("aether: not serving"),
            "{NOT_SERVING:?}"
        );
        // Every way the contract offers to turn it on, so a reader is never sent to `--help`.
        for remedy in ["--aether", "--socket", "ORACLE_AETHER"] {
            assert!(
                NOT_SERVING.contains(remedy),
                "the line must name {remedy:?} as a way to serve: {NOT_SERVING:?}"
            );
        }
        // The line-continuation in the literal must not have eaten the space before the parenthesis.
        assert!(
            !NOT_SERVING.contains("window(pass"),
            "the continued literal lost its word break: {NOT_SERVING:?}"
        );
    }

    /// A bus that was never asked to serve reports that it is not serving, and one that binds reports that it
    /// is. This is what the status line's `AETHER` field reads, so it must be the *bus's* answer and not a
    /// re-reading of the command line.
    #[test]
    fn is_serving_answers_for_the_socket_and_not_for_the_flag() {
        let inert = Bus::start(None, MachineInfo::default());
        assert!(!inert.is_serving(), "no socket was requested");

        // The same shape the socket tests below use: unique per process and per thread, so a parallel run
        // cannot have two tests fighting over one path.
        let path = std::env::temp_dir().join(format!(
            "oracle-is-serving-{}-{:?}.sock",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let served = Bus::start(Some(Some(path.clone())), MachineInfo::default());
        assert!(served.is_serving(), "a bound socket is serving");
        drop(served);
        let _ = std::fs::remove_file(&path);
    }

    /// A minimal NDJSON client over the socket this bus really binds — the one thing no in-process
    /// shortcut can stand in for, because arming the profiler is something only a *client* can do.
    ///
    /// It interleaves reads with [`Bus::pump`] because that is the hosted arrangement: the engine answers
    /// nothing until the player drains it, so a test that blocked on the socket without pumping would
    /// deadlock exactly as a player that forgot to pump would.
    struct Peer {
        w: UnixStream,
        r: BufReader<UnixStream>,
        id: u64,
    }

    impl Peer {
        fn connect(path: &std::path::Path) -> Self {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match UnixStream::connect(path) {
                    Ok(s) => {
                        s.set_read_timeout(Some(Duration::from_millis(20)))
                            .expect("a read timeout, so the pump gets a turn");
                        let r = BufReader::new(s.try_clone().expect("clone the stream"));
                        return Self { w: s, r, id: 1 };
                    }
                    Err(e) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5));
                        let _ = e;
                    }
                    Err(e) => panic!("connect: {e}"),
                }
            }
        }

        fn send(&mut self, line: &Value) {
            writeln!(self.w, "{line}").expect("write");
            self.w.flush().expect("flush");
        }

        /// Send a request and pump until its reply comes back. The pump is the player's own loop doing
        /// its one bounded drain per iteration.
        fn call(&mut self, bus: &mut Bus, sys: &mut System, method: &str, params: Value) -> Value {
            let id = self.id;
            self.id += 1;
            self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                bus.pump(sys);
                let mut line = String::new();
                match self.r.read_line(&mut line) {
                    Ok(0) => panic!("the server closed the connection"),
                    Ok(_) => {
                        let v: Value = serde_json::from_str(&line).expect("NDJSON");
                        if v["id"] == json!(id) {
                            assert!(v.get("error").is_none(), "{method}: {}", v["error"]);
                            return v["result"].clone();
                        }
                        // An event or another reply — keep going.
                    }
                    Err(_) if Instant::now() < deadline => {}
                    Err(e) => panic!("{method}: no reply ({e})"),
                }
            }
        }
    }

    /// One iteration of the **player's** run loop, in exactly the shape `main.rs` uses it: the machine is
    /// advanced through the scanline capture and whatever [`Bus::run_sinks`] lends it.
    fn player_frame(bus: &mut Bus, sys: &mut System, cap: &mut ScanlineCapture, lend: bool) {
        if lend {
            let resume_pc = sys.cpu_regs().pc;
            let (watch, prof, mut brk) = bus.run_sinks(resume_pc);
            {
                let mut sink =
                    Fanout::new(&mut *cap, Fanout::new(&mut brk, Fanout::new(watch, prof)));
                sys.run_frames_with_sink(1, &mut sink);
            }
            if let Some(addr) = break_observed(brk) {
                bus.record_break(addr);
            }
        } else {
            sys.run_frames_with_sink(1, &mut *cap);
        }
        cap.clear();
    }

    /// ## ★ The player's loop feeds a profiler a CLIENT armed (CR-26 / M1).
    ///
    /// This is the frontend half of `oracle_aether::host`'s hosted-profiler witness, and it exists because
    /// the defect it guards was a defect **of this file's caller**: the loop attached the watch and not the
    /// profiler, so a client that armed the accountant on a *playing* machine — the only machine anybody
    /// plays — got `frameCount: 0` and no rows back, which reads as "the game did nothing" and is about
    /// frames that really happened.
    ///
    /// Nothing here is simulated: the bus binds its real socket, a real NDJSON client does the handshake
    /// and arms the profiler over it, and the sample is read back the same way. The only thing standing in
    /// for the window is [`player_frame`], which is the loop's sink expression and nothing else.
    #[test]
    fn the_players_run_feeds_a_profiler_a_client_armed() {
        let path = std::env::temp_dir().join(format!(
            "oracle-prof-seam-{}-{:?}.sock",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);

        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build_profiler(
            oracle_core::testrom::ProfilerShape::CallsLeaf { k: 3 },
        ));
        sys.reset();
        let mut cap = ScanlineCapture::new(Retain::LastFrame);
        let mut bus = Bus::start(Some(Some(path.clone())), MachineInfo::default());

        let mut peer = Peer::connect(&path);
        peer.call(
            &mut bus,
            &mut sys,
            "initialize",
            json!({
                "clientId": "frontend-test",
                "clientName": "bus seam",
                "clientVersion": "0",
                "protocolVersion": 1,
                "clientCapabilities": {"events": false},
            }),
        );
        peer.send(&json!({"jsonrpc":"2.0","method":"initialized"}));

        let armed = peer.call(
            &mut bus,
            &mut sys,
            "emulator/set_profiler",
            json!({"enabled": true}),
        );
        assert_eq!(armed["enabled"], json!(true));

        // The negative control, first: frames the loop does not lend the instruments to. The machine moves
        // and the sample stays empty — the M1 defect, reproduced deliberately so the assertion below is
        // known to be measuring the attach and not the fixture.
        for _ in 0..3 {
            player_frame(&mut bus, &mut sys, &mut cap, false);
        }
        let unlent = peer.call(
            &mut bus,
            &mut sys,
            "emulator/get_profiler_frames",
            json!({}),
        );
        assert_eq!(
            unlent["frameCount"],
            json!(0),
            "an unlent profiler reports the player's frames as no frames at all: {unlent}"
        );

        // Now the loop as it actually runs.
        for _ in 0..6 {
            player_frame(&mut bus, &mut sys, &mut cap, true);
        }
        let s = peer.call(
            &mut bus,
            &mut sys,
            "emulator/get_profiler_frames",
            json!({}),
        );
        // `>= 4` rather than `== 6`, and the slack is the sample's own edges: a sample is delimited by
        // frame boundaries, so the frame in flight when it opened and the one in flight when it was read
        // are not whole frames of it. What is being asserted is that the player's frames landed in a
        // sample that was empty three frames ago, which no off-by-one can fake.
        assert!(
            s["frameCount"].as_u64().is_some_and(|n| n >= 4),
            "the client's profiler rode the PLAYER's frames: {s}"
        );
        assert!(
            s["sampleCycles"].as_u64().is_some_and(|c| c > 0),
            "and measured them: {s}"
        );
        assert!(
            s["routines"]["items"]
                .as_array()
                .is_some_and(|r| !r.is_empty()),
            "the fixture calls a leaf three times a frame, so there are rows: {s}"
        );

        drop(peer);
        let _ = std::fs::remove_file(&path);
    }

    /// The head of the fixture ROM's inner stirring loop (`testrom.rs`: *"$00020E  inner: move.w (A0),
    /// D0"*, encoding `$3010`), checked against the ROM image below rather than copied as a number.
    const HOT_PC: u32 = 0x0000_020E;

    /// One whole iteration of `main.rs`'s loop, orderings included: the frame (with the breakpoint sink
    /// and the halt handed back), then `set_paused`, then the drain, then follow the bus's answer. This
    /// is the shape the halt has to survive, and the only thing standing in for the window.
    fn player_iteration(
        bus: &mut Bus,
        sys: &mut System,
        cap: &mut ScanlineCapture,
        paused: &mut bool,
    ) {
        if !*paused {
            player_frame(bus, sys, cap, true);
        }
        bus.set_paused(*paused);
        bus.pump(sys);
        *paused = bus.is_paused();
    }

    /// ## ★ The frontend's own seam: a breakpoint a CLIENT armed halts the PLAYER's loop.
    ///
    /// The `oracle_aether::host` tests prove the halt; this proves that *this file* carries it — that
    /// `Bus::run_sinks`'s third half, `break_observed` and `Bus::record_break` are wired to the host and
    /// not merely present. `bus_stub.rs` has the identical surface with an always-`None` sink, so the run
    /// loop compiles unchanged in both builds; this is the half that has something to check.
    ///
    /// The negative control is the first phase, and it is what makes the second mean anything: with no
    /// breakpoint armed the same loop runs freely and the clock climbs, so a fixture that simply never
    /// runs cannot pass.
    #[test]
    fn a_breakpoint_a_client_armed_halts_the_players_loop() {
        let rom = oracle_core::testrom::build();
        let a = HOT_PC as usize;
        assert_eq!(
            u16::from_be_bytes([rom[a], rom[a + 1]]),
            0x3010,
            "the fixture ROM moved: 0x{HOT_PC:08X} is no longer the hot loop, and this test would arm a \
             dead address"
        );

        let path = std::env::temp_dir().join(format!(
            "oracle-bp-seam-{}-{:?}.sock",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);

        let mut sys = System::new(0x5EED);
        sys.load_rom(rom);
        sys.reset();
        let mut cap = ScanlineCapture::new(Retain::LastFrame);
        let mut bus = Bus::start(Some(Some(path.clone())), MachineInfo::default());

        let mut peer = Peer::connect(&path);
        peer.call(
            &mut bus,
            &mut sys,
            "initialize",
            json!({
                "clientId": "frontend-test",
                "clientName": "bus seam",
                "clientVersion": "0",
                "protocolVersion": 1,
                "clientCapabilities": {"events": false},
            }),
        );
        peer.send(&json!({"jsonrpc":"2.0","method":"initialized"}));

        // Phase 1, the negative control: the window plays, nothing is armed, the clock climbs.
        let mut paused = false;
        for _ in 0..3 {
            player_iteration(&mut bus, &mut sys, &mut cap, &mut paused);
        }
        assert!(!paused, "an unarmed window must keep playing");
        let before = sys.scheduler().now();
        assert!(before > 0, "…and must actually have run");

        // Phase 2: a client arms one — legal while the machine runs, since the breakpoint surface is
        // exempt from the run-control state rule.
        let added = peer.call(
            &mut bus,
            &mut sys,
            "emulator/breakpoint_add",
            json!({"addr": format!("0x{HOT_PC:08X}")}),
        );
        let handle = added["breakpoint"].as_str().expect("a handle").to_string();

        for _ in 0..3 {
            player_iteration(&mut bus, &mut sys, &mut cap, &mut paused);
        }
        assert!(
            paused,
            "the client's breakpoint did not reach the player's loop through this file"
        );

        // The window really stopped: further iterations move no emulated time at all.
        let halted_at = sys.scheduler().now();
        for _ in 0..5 {
            player_iteration(&mut bus, &mut sys, &mut cap, &mut paused);
        }
        assert_eq!(
            sys.scheduler().now(),
            halted_at,
            "the loop kept emulating after the halt"
        );

        // …and the surface agrees, over the wire, at the exact address.
        let w = peer.call(
            &mut bus,
            &mut sys,
            "emulator/wait_for_break",
            json!({"timeoutMs": 0}),
        );
        assert_eq!(w["timeoutReached"], json!(false), "{w}");
        assert_eq!(
            w["pc"],
            json!(format!("0x{HOT_PC:08X}")),
            "the stop must be at the breakpoint, not wherever the frame happened to end: {w}"
        );
        let l = peer.call(&mut bus, &mut sys, "emulator/breakpoint_list", json!({}));
        let row = l["breakpoints"]
            .as_array()
            .expect("breakpoints[]")
            .iter()
            .find(|b| b["breakpoint"] == json!(handle))
            .unwrap_or_else(|| panic!("no row for {handle} in {l}"));
        assert_eq!(row["hits"], json!(1), "one halt is one hit: {l}");

        drop(peer);
        let _ = std::fs::remove_file(&path);
    }
}
