//! **The capability layer, hosted inside whatever process owns the machine.**
//!
//! [`crate::server`] is the standalone arrangement: it takes a `System`, puts it on a thread of its own, and
//! that thread is the only thing in the world that ever advances it. That is the right shape for a headless
//! bus, and it stays exactly as it was.
//!
//! It is the wrong shape the moment a *player* exists. A player has its own run loop — one paced by the audio
//! device, feeding a window, reading a keyboard — and there cannot be two things advancing one machine. So
//! this module inverts the control: the player owns the `System` and owns the loop, and the capability layer
//! becomes something the player **drains once per iteration**.
//!
//! # Why this and not a capability layer in some other process
//!
//! Three clauses of `empyrean/contract/protocol.md` decide it, and each one is a hard requirement rather than
//! a preference:
//!
//! * **D13 / §6.1** — a checkpoint is *"a serialization of the live emulator struct"*. A capability layer that
//!   cannot reach the live struct cannot implement `emulator/checkpoint` at all.
//! * **D11** — every reply carries the machine's `frame`/`mclk` **at reply time**. Only the holder of the
//!   clocks can stamp truthfully; anything else is quoting a number it was told earlier.
//! * **D8** — the trust model is *the emulator process* serving a loopback-only socket it created.
//!
//! A layer outside the machine's process would need a second protocol between itself and the player to satisfy
//! any of those — which is precisely the un-specced drift surface this design exists to eliminate.
//!
//! # The seam
//!
//! ```text
//!   client sockets ──▶ accept thread ──▶ connection threads ──┐
//!                       (shared verbatim with `server`)       │ EngineMsg (mpsc)
//!                                                             ▼
//!   ┌────────────────────── the player's own thread ────────────────────────┐
//!   │  poll input ▸ run 0..2 frames ▸ blit ▸ present ▸ **Host::pump(&mut sys)** │
//!   └───────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! [`Host::pump`] is the whole seam. It lends the machine to the [`Engine`](crate::engine::Engine) with
//! [`swap_system`](crate::engine::Engine::swap_system), answers every command that is already queued, and
//! hands the machine straight back. `System` never leaves the player's thread's control for longer than that,
//! so it stays single-threaded and deterministic exactly as the core's charter requires.
//!
//! # Two properties that must not be weakened, and are not
//!
//! **A slow or dead client can never wedge the emulator, and never stalls the player's frame loop.** It is
//! the same structural argument the standalone server rests on, unchanged: no socket is ever written from the
//! thread that owns the machine. Connection threads do all socket I/O; events go through
//! [`Outbound::push_event`](crate::outbound::Outbound::push_event), which drops oldest-first rather than
//! waiting. `pump` itself only ever `try_recv`s, so an idle bus costs one non-blocking channel poll per frame
//! and a client that stops reading its replies stalls nothing but its own reader thread.
//!
//! **No single command can freeze the window.** See [`HostConfig::pump_budget`] and
//! [`HOSTED_MAX_RUN_FRAMES`].

use crate::engine::{Engine, EngineConfig};
use crate::outbound::DEFAULT_CAPACITY;
use crate::server::{spawn_accept, AcceptCtx, EngineMsg, Server, ServerConfig};
use oracle_core::bus::Observe;
use oracle_core::io::Pad;
use oracle_core::scanline_capture::ScanlineCapture;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::{System, MCLK_PER_FRAME};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

/// The hosted ceiling for one `emulator/run_frames` / `emulator/run_to` / `emulator/press`.
///
/// The standalone server's 3,600 is a *fairness* bound — one client must not monopolise the engine thread —
/// and 60 emulated seconds is a perfectly reasonable thing to ask a headless bus for. Hosted, the same call
/// runs on the thread that also pumps the OS event queue and presents the window, so 3,600 frames is a
/// minute-long freeze of a live application. 120 frames is ~2 s of emulated time and a few hundred
/// milliseconds of wall clock — the same order as a save-state load, which the player already does inline.
///
/// It is a **refusal**, not a clamp: over the bound, `emulator/run_frames` answers `-32602` naming the limit,
/// and the limit itself is advertised in `initialize` as `limits.maxRunFrames`, so a client discovers it
/// before it hits it and can simply call five times instead of once. A silent clamp would return fewer frames
/// than asked for with no way to notice — the failure mode this whole surface is built to avoid.
pub const HOSTED_MAX_RUN_FRAMES: u64 = 120;

/// How long one [`Host::pump`] may spend answering queued commands before it leaves the rest for the next
/// iteration. See [`HostConfig::pump_budget`].
pub const DEFAULT_PUMP_BUDGET: Duration = Duration::from_millis(4);

/// Host tunables.
#[derive(Clone, Debug)]
pub struct HostConfig {
    pub engine: EngineConfig,
    /// Per-connection outbound queue depth. Events beyond it are dropped oldest-first and counted.
    pub event_queue_cap: usize,
    /// Wall-clock ceiling on **one drain**, checked between commands.
    ///
    /// Two different overruns have to be bounded and they need two different mechanisms.
    /// [`HOSTED_MAX_RUN_FRAMES`] bounds how long a *single* command can take; this bounds how many commands
    /// one iteration will run before deferring the rest, so a client that queues fifty calls cannot turn one
    /// frame into fifty calls' worth of stall. Nothing is dropped — the leftovers are still in the channel
    /// and the next iteration takes them, which is 16.7 ms later.
    ///
    /// It is checked *between* commands, never inside one: a command that has begun always runs to
    /// completion, because a half-executed `run_frames` is not a thing the protocol can describe.
    pub pump_budget: Duration,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig {
                max_run_frames: HOSTED_MAX_RUN_FRAMES,
                // The player paces the machine (its audio device is the master clock). A hosted engine never
                // free-runs on its own, so it has no pacing interval of its own to honour.
                free_run_pace: None,
                ..EngineConfig::default()
            },
            event_queue_cap: DEFAULT_CAPACITY,
            pump_budget: DEFAULT_PUMP_BUDGET,
        }
    }
}

/// What one [`Host::pump`] did, in the terms the host's run loop has to react to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PumpReport {
    /// Commands answered this drain.
    pub calls: usize,
    /// The machine coordinate before and after. Both are read *inside* the drain window, so they are the
    /// real machine's, never the placeholder's.
    pub mclk_before: u64,
    pub mclk_after: u64,
    /// A client-driven run replaced the picture: the caller's own framebuffer is now stale and it should
    /// present [`Host::framebuffer`] instead. Also set when the picture was *invalidated* (a restore, a ROM
    /// reload) — in which case there is nothing to present and the caller keeps what it has until its own
    /// next frame.
    pub screen_changed: bool,
    /// **The machine was replaced wholesale under the caller** — not advanced, replaced — so anything it
    /// derives from that machine rather than reading back out of it is now stale: a save-state fingerprint,
    /// a symbol listing, a cached ROM header, an audio clock keyed to the old timeline.
    ///
    /// Three producers raise it, and the name is narrower than the meaning because the first two came first:
    ///
    /// - `emulator/reload_rom` — different cartridge bytes.
    /// - `emulator/restore` — a checkpoint carries its whole machine, ROM included (D13 rule 2), so it *may*
    ///   have brought a different cartridge back; the flag moves unconditionally rather than guessing.
    /// - `emulator/reset` — the same cartridge, back at its power-on anchor. The bytes did not change, but
    ///   everything clocked to the machine did, and a caller that resynchronised for the other two and not
    ///   for this one would be holding an audio clock and a frame counter from a timeline that no longer
    ///   exists.
    ///
    /// Read it as "resynchronise", not as "re-read the ROM".
    pub rom_changed: bool,
    /// The drain ended on [`HostConfig::pump_budget`] rather than on an empty queue. Anything still queued
    /// is taken next iteration; nothing is ever lost. Note it does **not** promise that something *was*
    /// left over — an mpsc queue cannot be peeked, so this is honestly "stopped on the clock, not on the
    /// queue", which is the only thing the drain can know.
    pub deferred: bool,
}

impl PumpReport {
    /// Whether the machine's timeline moved under the caller — a bounded run, or a restore that rewound it.
    /// A caller that keeps anything clocked to the machine (audio, a frame counter, a scanline capture) has
    /// to resynchronise exactly as it would after a save-state load.
    pub fn timeline_moved(&self) -> bool {
        self.mclk_after != self.mclk_before
    }

    /// Whole emulated frames the drain advanced, or 0 if the timeline went backwards (a restore).
    pub fn frames_advanced(&self) -> u64 {
        self.mclk_after.saturating_sub(self.mclk_before) / MCLK_PER_FRAME
    }
}

/// The capability layer, hosted. Owns the bus's state; borrows the machine one drain at a time.
pub struct Host {
    engine: Engine,
    rx: Receiver<EngineMsg>,
    tx: Sender<EngineMsg>,
    ctx: AcceptCtx,
    accept: Option<std::thread::JoinHandle<()>>,
    socket_path: Option<PathBuf>,
    pump_budget: Duration,
    /// A free-run change requested from outside the drain window, applied at the top of the next one.
    ///
    /// Deferred rather than applied on the spot because
    /// [`set_free_run`](crate::engine::Engine::set_free_run) emits `emulator/stopped` /
    /// `emulator/resumed`, and every event carries the machine stamp (D11). Outside the window the engine
    /// holds the placeholder, so an event emitted there would be stamped `frame 0, mclk 0` — a lie about the
    /// exact instant a client most needs the truth about.
    pending_free_run: Option<bool>,
}

impl Host {
    /// A host with no socket. **Serving is opt-in**: nothing is bound, no filesystem entry is created and no
    /// thread is started until [`serve`](Host::serve) is called, so a player that never asks for the bus
    /// behaves exactly as it did before this existed.
    ///
    /// The engine is built around an inert placeholder `System` — it is the empty shell the caller's real
    /// machine is exchanged against on every [`pump`](Host::pump), and it never runs a single instruction.
    pub fn new(config: HostConfig) -> Self {
        let ctx = AcceptCtx::new(config.event_queue_cap);
        let (tx, rx) = mpsc::channel::<EngineMsg>();
        let engine = Engine::new(System::new(0), config.engine, ctx.subs.clone());
        Self {
            engine,
            rx,
            tx,
            ctx,
            accept: None,
            socket_path: None,
            pump_budget: config.pump_budget,
            pending_free_run: None,
        }
    }

    /// Attach what the bus should know about the loaded cartridge — the same two things
    /// [`Machine`](crate::server::Machine) carries into the standalone server, so `emulator/status` can name
    /// the ROM and `emulator/read_memory {symbol}` can resolve against the right listing (D7).
    pub fn set_machine_info(&mut self, machine_info: MachineInfo) {
        self.engine.set_rom_path(machine_info.rom_path);
        self.engine
            .set_symbols(machine_info.symbols, machine_info.symbols_path);
    }

    /// Bind the socket and start accepting. `None` resolves the path exactly as `protocol.md` §7.1 specifies
    /// (`$ORACLE_SOCKET` → `$EXODUS_SOCKET` → `$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock`), and the
    /// 0600 enforcement and the live-server check are [`Server::bind`]'s, unchanged.
    pub fn serve(&mut self, socket_path: Option<PathBuf>) -> std::io::Result<PathBuf> {
        if self.accept.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "this host is already serving",
            ));
        }
        let config = ServerConfig {
            socket_path: socket_path.unwrap_or_else(crate::server::default_socket_path),
            engine: self.engine.config().clone(),
            event_queue_cap: self.ctx.event_queue_cap,
        };
        let (listener, config) = Server::bind(config)?.into_parts();
        self.accept = Some(spawn_accept(listener, &self.ctx, self.tx.clone()));
        self.socket_path = Some(config.socket_path.clone());
        Ok(config.socket_path)
    }

    pub fn socket_path(&self) -> Option<&Path> {
        self.socket_path.as_deref()
    }

    pub fn is_serving(&self) -> bool {
        self.accept.is_some()
    }

    /// Whether any client is connected right now. A host uses this to skip per-frame work nobody would read
    /// — publishing the picture, most of all, which is a whole frame's memcpy.
    pub fn has_clients(&self) -> bool {
        self.ctx.live.load(Ordering::SeqCst) > 0
    }

    // ---------------------------------------------------------------- run state (conflict 1)

    /// Mirror the host's own pause state onto the bus.
    ///
    /// `free_run` means *"something other than this client is advancing the machine"*, and hosted, that
    /// something is the player. So an un-paused player **is** a free-running bus, and `protocol.md` §6's
    /// run-control state rule then requires `run_frames`/`run_to`/`step*` to be refused with
    /// `-32005 machineRunning` — which is the correct answer, not a workaround: a client stepping a machine
    /// that a 60 Hz loop is also advancing would be reading coordinates that mean nothing.
    ///
    /// Applied at the top of the next [`pump`](Host::pump) so the resulting event is stamped truthfully.
    pub fn set_paused(&mut self, paused: bool) {
        // Compared against the **engine's** state, never against a pending value that has not landed yet.
        // Comparing against the pending one loses a change when this is called twice between drains: the
        // second call would see its own request already recorded, conclude nothing needs doing, and clear it.
        let want = !paused;
        self.pending_free_run = (want != self.engine.is_running()).then_some(want);
    }

    /// Whether the machine should be paused — **the bus's answer**, which a client can have changed with
    /// `emulator/pause` / `emulator/resume`. A host reads this after every pump and follows it, which is what
    /// makes those two methods work at all when the player owns the loop.
    pub fn is_paused(&self) -> bool {
        !self.free_run_now()
    }

    fn free_run_now(&self) -> bool {
        self.pending_free_run
            .unwrap_or_else(|| self.engine.is_running())
    }

    // ---------------------------------------------------------------- input (conflict 2)

    /// The buttons a client is holding on `port` (`emulator/hold`). A host ORs these into the pad it writes,
    /// so client-held buttons and live human input compose instead of erasing each other — see
    /// [`Engine::apply_pads`](crate::engine::Engine) for why OR and not a precedence rule.
    pub fn held(&self, port: usize) -> Pad {
        self.engine.held(port)
    }

    /// Tell the bus what the human is physically holding, so the engine's own pad writes (`hold`, `press`,
    /// `release_all`) compose with it rather than dropping it on the floor.
    pub fn set_live_pads(&mut self, pads: [Pad; 2]) {
        self.engine.set_live_pads(pads);
    }

    // ---------------------------------------------------------------- the instrument (conflict 4)

    /// **The watchpoint instrument, lent to the host's own run loop.**
    ///
    /// This is the fourth conflict, and it is the one a naive implementation gets wrong. There are **two run
    /// drivers**: in the standalone server the engine advances the machine itself, and hosted, the player
    /// does — the engine only borrows it inside [`pump`](Host::pump). A `Watchpoints` owned by the engine and
    /// attached only to the engine's own runs would therefore see **nothing** while the player is running,
    /// and would report it as `seen == 0` — which is honest ("the recorder was never attached to the run")
    /// and useless.
    ///
    /// So the instrument is engine-owned and lent here. The host's loop puts it in the sink it already
    /// builds per frame, exactly as it does the scanline capture, and the panel that reads it locally and the
    /// bus's `emulator/watchpoint_hits` then read **one** instrument. That is contract §8 item 19's
    /// guarantee made structural rather than promised: they cannot drift, because there is nothing for them
    /// to drift apart *from*.
    ///
    /// Safe to call outside a drain window, unlike anything that touches the machine: watches are engine
    /// state, not `System` state, so this never answers for the placeholder.
    pub fn watchpoints_mut(&mut self) -> &mut oracle_core::watchpoints::Watchpoints {
        self.engine.watchpoints_mut()
    }

    /// **Both instruments, wrapped for attaching to the host's own run** — the watch and, since CR-26, the
    /// profiler. See [`Engine::run_sinks`](crate::engine::Engine::run_sinks) for the whole argument: why
    /// they are lent rather than owned by the run driver, why the arming conditions live down there rather
    /// than in the host's loop, why both are wrapped in [`Observe`](oracle_core::bus::Observe), and why the
    /// pair comes from one call rather than two accessors.
    ///
    /// A host puts *these* in the per-frame sink it already builds for the scanline capture, and never the
    /// bare instruments. `None` means "not armed, attach nothing" — which is a live case on nearly every
    /// frame and is why the halves are `Option`s rather than always-on sinks.
    pub fn run_sinks(
        &mut self,
    ) -> (
        Option<Observe<&mut oracle_core::watchpoints::Watchpoints>>,
        Option<Observe<&mut oracle_core::profiler::Profiler>>,
    ) {
        self.engine.run_sinks()
    }

    /// **The read half of [`run_sinks`](Host::run_sinks)**, forwarded: both instruments and the profiler's
    /// armed flag, from one shared borrow. See
    /// [`Engine::read_instruments`](crate::engine::Engine::read_instruments) for why it is one call, why
    /// the flag is separate from the accumulator, and why nothing here needs `&mut`.
    ///
    /// A host draws its panels from *these* — the same instruments its loop feeds and the bus serves, so a
    /// local readout and a client's reply cannot disagree.
    pub fn read_instruments(
        &self,
    ) -> (
        &oracle_core::watchpoints::Watchpoints,
        &oracle_core::profiler::Profiler,
        bool,
    ) {
        self.engine.read_instruments()
    }

    // ---------------------------------------------------------------- the picture (conflict 3)

    /// Hand the bus the frame the host's own run loop just drew, so `emulator/screenshot` and
    /// `emulator/state_hash {includeFramebuffer}` answer with what is actually on the glass.
    ///
    /// Cheap to call unconditionally: it is skipped outright while nobody is connected, and it takes the
    /// caller's [`ScanlineCapture`] as-is rather than asking for a converted buffer.
    pub fn publish_capture(&mut self, cap: &ScanlineCapture) {
        if self.has_clients() {
            self.engine.publish_capture(cap);
        }
    }

    /// The frame the bus would serve — the last one drawn, line-major RGB, with its width. `None` before any
    /// whole frame exists. A host presents this after a [`PumpReport::screen_changed`] drain.
    pub fn framebuffer(&self) -> Option<crate::engine::FrameRef<'_>> {
        self.engine.latched_frame()
    }

    // ---------------------------------------------------------------- the drain

    /// **Answer every queued command against the caller's machine, then give it straight back.**
    ///
    /// The machine is swapped in for the duration (see
    /// [`swap_system`](crate::engine::Engine::swap_system)) so every handler stamps the real clocks, and
    /// swapped back out before this returns. `System` is 1,152 bytes of struct — every large region is behind
    /// a `Vec` — so the exchange is two ~1 KB moves per frame, not a copy of the machine.
    ///
    /// Never blocks: the channel is polled with `try_recv`, so an idle bus costs one non-blocking poll.
    pub fn pump(&mut self, sys: &mut System) -> PumpReport {
        let mut report = PumpReport::default();
        let screen_gen = self.engine.screen_generation();
        let rom_gen = self.engine.rom_generation();

        self.engine.swap_system(sys);
        report.mclk_before = self.engine.mclk();
        if let Some(on) = self.pending_free_run.take() {
            self.engine.set_free_run(on);
        }
        let deadline = Instant::now() + self.pump_budget;
        loop {
            match self.rx.try_recv() {
                Ok(EngineMsg::Shutdown) => break,
                Ok(EngineMsg::Call {
                    method,
                    params,
                    reply,
                }) => {
                    let result = self.engine.dispatch(&method, &params);
                    let stamp = self.engine.stamp();
                    report.calls += 1;
                    // The reply goes into a channel the connection thread owns; if that thread has gone
                    // away the send fails and is dropped. Nothing here waits on a socket.
                    let _ = reply.send(crate::server::CallResult { result, stamp });
                }
                Ok(EngineMsg::Initialize { params, reply }) => {
                    let result = self.engine.initialize_result(&params);
                    let stamp = self.engine.stamp();
                    report.calls += 1;
                    let _ = reply.send(crate::server::CallResult { result, stamp });
                }
                // `Disconnected` cannot happen — this host holds a `Sender` itself — but treating it as
                // "nothing more to do" is the only sane reading if it ever did.
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
            // Checked *between* commands: one that has started always finishes.
            if Instant::now() >= deadline {
                report.deferred = true;
                break;
            }
        }
        report.mclk_after = self.engine.mclk();
        // Published while the real machine is still swapped in, so the cached stamp a connection thread uses
        // for envelope-level errors is at most one host iteration stale rather than the placeholder's zero.
        self.ctx
            .shared
            .store(report.mclk_after, self.engine.is_running());
        self.engine.swap_system(sys);

        report.screen_changed = self.engine.screen_generation() != screen_gen;
        report.rom_changed = self.engine.rom_generation() != rom_gen;
        report
    }

    /// Stop accepting, hang up on every client, and unlink the socket. Idempotent; also runs on drop.
    pub fn shutdown(&mut self) {
        if self.accept.is_none() {
            return;
        }
        self.ctx.close_all();
        if let Some(t) = self.accept.take() {
            let _ = t.join();
        }
        if let Some(p) = self.socket_path.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// What the bus knows about the loaded cartridge. The hosted twin of
/// [`Machine`](crate::server::Machine)'s three non-`System` fields — hosted, the `System` itself belongs to
/// the caller, so it is not in here.
#[derive(Default)]
pub struct MachineInfo {
    pub rom_path: Option<String>,
    pub symbols: Option<SymbolTable>,
    pub symbols_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::scanline_capture::Retain;
    use serde_json::{json, Value};

    fn booted() -> System {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        sys
    }

    /// A host that was never asked to serve creates nothing and does nothing — the "default launch is
    /// byte-identical" guarantee, checked rather than asserted in prose.
    #[test]
    fn an_unserved_host_binds_nothing_and_pumps_nothing() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        let before = sys.scheduler().now();
        assert!(!h.is_serving());
        assert!(h.socket_path().is_none());
        assert!(!h.has_clients());
        let r = h.pump(&mut sys);
        assert_eq!(r.calls, 0, "nothing to answer");
        assert!(!r.timeline_moved() && !r.screen_changed && !r.rom_changed);
        assert_eq!(sys.scheduler().now(), before, "and never touches the clock");
        // The machine came back byte-identical: an idle drain is an identity operation on it, which is what
        // "the default launch behaves exactly as it did" has to mean at this level.
        assert_eq!(sys.state_hash().combined, booted().state_hash().combined);
    }

    /// The machine really does come back, and it comes back advanced — the swap is a lend, not a copy.
    #[test]
    fn the_machine_is_lent_and_returned() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        sys.run_frames(3);
        let mclk = sys.scheduler().now();
        h.engine.swap_system(&mut sys);
        assert_eq!(h.engine.mclk(), mclk, "the engine sees the real machine");
        h.engine.swap_system(&mut sys);
        assert_eq!(sys.scheduler().now(), mclk, "and the caller gets it back");
    }

    /// Put one call through the real channel the connection threads use, and drain it with a real
    /// [`Host::pump`] — so these tests exercise the seam rather than the engine behind it.
    fn call(h: &mut Host, sys: &mut System, method: &str, params: serde_json::Value) -> PumpReport {
        let (tx, rx) = mpsc::channel();
        h.tx.send(EngineMsg::Call {
            method: method.into(),
            params,
            reply: tx,
        })
        .expect("queue the call");
        let report = h.pump(sys);
        let r = rx.try_recv().expect("the drain answered");
        assert!(r.result.is_ok(), "{method}: {:?}", r.result.err());
        report
    }

    /// Conflict 1, both directions: the host's pause state becomes the bus's `free_run`, and a client's
    /// `emulator/pause` becomes the host's.
    #[test]
    fn pause_state_is_one_flag_shared_by_both_sides() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();

        // The host is running, so the bus is free-running…
        h.set_paused(false);
        assert!(!h.is_paused(), "the pending change is visible immediately");
        // …and asking twice before a drain must not cancel the request (the pending value is compared
        // against the engine, not against itself).
        h.set_paused(false);
        assert!(!h.is_paused(), "a repeated request is still a request");
        h.pump(&mut sys);
        assert!(!h.is_paused(), "and it survives the drain that applied it");

        // …and §6's run-control state rule therefore refuses a client-driven run.
        let (tx, rx) = mpsc::channel();
        h.tx.send(EngineMsg::Call {
            method: "emulator/run_frames".into(),
            params: json!({"frames": 1}),
            reply: tx,
        })
        .unwrap();
        let before = sys.scheduler().now();
        h.pump(&mut sys);
        let e = rx.try_recv().unwrap().result.expect_err("must be refused");
        assert_eq!(e.code, crate::rpc::code::INVALID_STATE);
        assert_eq!(e.data.unwrap()["reason"], json!("machineRunning"));
        assert_eq!(sys.scheduler().now(), before, "and nothing advanced");

        // The other direction: a client pausing the bus pauses the host.
        call(&mut h, &mut sys, "emulator/pause", json!({}));
        assert!(h.is_paused(), "a client's pause reaches the host");
        // …and now the same run is allowed.
        let r = call(
            &mut h,
            &mut sys,
            "emulator/run_frames",
            json!({"frames": 2}),
        );
        assert_eq!(r.frames_advanced(), 2);
        assert!(r.timeline_moved());

        // And a client resuming un-pauses it again.
        call(&mut h, &mut sys, "emulator/resume", json!({}));
        assert!(!h.is_paused(), "a client's resume reaches the host too");
    }

    /// Conflict 2: a client's held set and the human's live input merge per button, and neither erases the
    /// other.
    #[test]
    fn held_buttons_and_live_input_merge() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        h.set_live_pads([
            Pad {
                left: true,
                ..Pad::default()
            },
            Pad::default(),
        ]);
        h.engine.swap_system(&mut sys);
        h.engine
            .dispatch("emulator/hold", &json!({"buttons": ["a"]}))
            .expect("hold");
        h.engine.swap_system(&mut sys);

        assert!(h.held(0).a, "the bus reports only what the client holds");
        assert!(!h.held(0).left, "and never the human's own buttons");
        let merged = crate::engine::merge_pads(
            Pad {
                left: true,
                ..Pad::default()
            },
            h.held(0),
        );
        assert!(merged.a && merged.left, "the host writes both");
    }

    /// The long-run bound is a refusal that names the limit, and the limit is the hosted one.
    #[test]
    fn a_run_longer_than_the_hosted_bound_is_refused_not_clamped() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        h.engine.swap_system(&mut sys);
        let e = h
            .engine
            .dispatch(
                "emulator/run_frames",
                &json!({"frames": HOSTED_MAX_RUN_FRAMES + 1}),
            )
            .expect_err("over the bound");
        assert_eq!(e.code, crate::rpc::code::INVALID_PARAMS);
        // At the bound it succeeds, so the boundary is inclusive and the refusal is not off by one.
        let before = h.engine.mclk();
        h.engine
            .dispatch(
                "emulator/run_frames",
                &json!({"frames": HOSTED_MAX_RUN_FRAMES}),
            )
            .expect("at the bound");
        assert_eq!(
            (h.engine.mclk() - before) / MCLK_PER_FRAME,
            HOSTED_MAX_RUN_FRAMES
        );
        h.engine.swap_system(&mut sys);
        // …and the advertised limit is the one enforced, so a client can plan around it.
        assert_eq!(h.engine.config().max_run_frames, HOSTED_MAX_RUN_FRAMES);
    }

    /// [`booted`], with the VDP's VINT enable (reg 1 bit 5) on, so the fixture's interrupt handler actually
    /// runs and writes its `$1234` sentinel to `$FF8000` once per frame. At power-on IE0 is off and the
    /// latch is set every frame without ever reaching the IPL, so the handler never runs — the same pose
    /// `system.rs`'s own interrupt tests use.
    fn booted_with_vint() -> System {
        let mut sys = booted();
        sys.vdp_mut().control_write(0x8120, 0);
        sys
    }

    /// [`call`], returning the handler's `result` instead of the drain report.
    fn call_ok(h: &mut Host, sys: &mut System, method: &str, params: serde_json::Value) -> Value {
        let (tx, rx) = mpsc::channel();
        h.tx.send(EngineMsg::Call {
            method: method.into(),
            params,
            reply: tx,
        })
        .expect("queue the call");
        h.pump(sys);
        rx.try_recv()
            .expect("the drain answered")
            .result
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    /// One iteration of the **player's** run loop, in the shape `oracle-frontend/src/main.rs` uses it:
    /// the machine is advanced through the scanline capture *and* the bus's watch instrument, borrowed for
    /// the run. `armed` mirrors the loop's own attach condition.
    fn player_frame(h: &mut Host, sys: &mut System, cap: &mut ScanlineCapture, armed: bool) {
        if armed {
            // `run_sinks`, never the bare instruments: an unwrapped watch would halt this loop. Both
            // halves ride, exactly as `oracle-frontend/src/main.rs` attaches them — a loop that took only
            // the watch is the M1 defect, and it is what the negative controls below run without.
            let (watch, prof) = h.run_sinks();
            let mut sink = oracle_core::bus::Fanout::new(
                &mut *cap,
                oracle_core::bus::Fanout::new(watch, prof),
            );
            sys.run_frames_with_sink(1, &mut sink);
        } else {
            sys.run_frames_with_sink(1, &mut *cap);
        }
        cap.clear();
    }

    /// ## ★ NON-WAIVABLE ★ — **contract §8 item 19, made executable: the bus and the panel read ONE
    /// instrument.**
    ///
    /// This is the test the whole hosted arrangement turns on, and it is here rather than in an
    /// integration test because it has to reach the instrument from **both** sides at once: over the wire
    /// as a client does, and through [`Host::watchpoints_mut`] as the player's run loop and its `W`-key
    /// panel do. No socket client can do the second.
    ///
    /// It asserts three things in one run:
    ///
    /// 1. **A watch armed over the bus observes frames the PLAYER ran.** That is the two-run-drivers
    ///    problem: an engine-owned instrument attached only to the engine's own runs would see nothing
    ///    here and report `seen == 0` — honest ("the recorder was never attached") and useless.
    /// 2. **The bus's `watchpoint_hits` and the panel's `Watchpoints::hits()` agree hit for hit.** Not
    ///    "are kept in step" — they cannot disagree, because there is nothing for them to disagree with.
    /// 3. **The failure it prevents is real**, shown by the negative control: a frame run *without*
    ///    lending the instrument moves the machine and leaves `seen` exactly where it was.
    #[test]
    fn the_bus_and_the_panel_read_one_instrument() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted_with_vint();
        let mut cap = ScanlineCapture::new(Retain::LastFrame);

        // --- Armed over the wire, by a client that does not own the run loop. ---
        let armed = call_ok(
            &mut h,
            &mut sys,
            "emulator/watchpoint_add",
            json!({"addr": "0x00FF8000", "len": 2, "label": "vint sentinel"}),
        );
        let handle = armed["watch"]
            .as_str()
            .expect("a string handle")
            .to_string();

        // --- The negative control, first, while it is still unambiguous. A run the instrument is not lent
        //     to advances the machine and tells the recorder nothing. This is precisely what a naive
        //     engine-owned instrument would do on EVERY frame of a hosted session.
        let mclk = sys.scheduler().now();
        player_frame(&mut h, &mut sys, &mut cap, false);
        assert!(sys.scheduler().now() > mclk, "the machine really advanced");
        assert_eq!(
            h.watchpoints_mut().seen(),
            0,
            "an unlent instrument sees nothing — the failure item 19 exists to prevent"
        );

        // --- Now the real loop: the player runs the machine and lends the bus's instrument to it. ---
        for _ in 0..4 {
            player_frame(&mut h, &mut sys, &mut cap, true);
        }
        assert!(
            h.watchpoints_mut().seen() > 0,
            "the bus's watch rode the PLAYER's run"
        );

        // --- Read it both ways. ---
        let wire = call_ok(&mut h, &mut sys, "emulator/watchpoint_hits", json!({}));
        // The panel's read is `hits()` — non-destructive, so this test's own two readers cannot steal each
        // other's evidence either, which is the same property a second client relies on.
        let panel: Vec<(u32, u32, u64, u64)> = h
            .watchpoints_mut()
            .hits()
            .iter()
            .map(|hit| (hit.addr, hit.value, hit.seq, hit.frame))
            .collect();

        assert!(!panel.is_empty(), "the VInt handler wrote the sentinel");
        assert_eq!(
            wire["total"].as_u64().unwrap() as usize,
            panel.len(),
            "the bus and the panel hold the same number of hits"
        );
        let from_wire: Vec<(u32, u32, u64, u64)> = wire["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hit| {
                let hex = |k: &str| {
                    u32::from_str_radix(hit[k].as_str().unwrap().trim_start_matches("0x"), 16)
                        .unwrap()
                };
                (
                    hex("addr"),
                    hex("value"),
                    hit["seq"].as_u64().unwrap(),
                    hit["frame"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            from_wire, panel,
            "hit for hit: the bus and the panel are two readers of ONE instrument"
        );
        // …and every hit names the handle the wire issued, so the two views are not merely the same
        // length, they are the same watch's evidence.
        for hit in wire["hits"].as_array().unwrap() {
            assert_eq!(hit["watch"], json!(handle));
        }

        // A watch the PANEL arms is visible to the bus in the same way — the parity is symmetric, which is
        // what makes "one instrument" a structural claim rather than a direction of travel.
        h.watchpoints_mut().add_watch(
            0x00FF_0000..=0x00FF_0001,
            oracle_core::watchpoints::WatchOp::Write,
            "panel",
        );
        let list = call_ok(&mut h, &mut sys, "emulator/watchpoint_list", json!({}));
        assert_eq!(
            list["total"],
            json!(2),
            "the bus sees the panel's watch too"
        );
        assert_eq!(list["watches"][1]["label"], json!("panel"));
    }

    /// ## ★ The same claim for the PROFILER (CR-26): a sample armed over the bus measures the frames the
    /// PLAYER ran.
    ///
    /// The watch test above is the pattern; this is the second instrument, and it had the same hole. A
    /// profiler attached only to the engine's own runs answers a hosted client with `frameCount: 0` and no
    /// rows — indistinguishable from *"the game did nothing"*, about frames that really happened, on the
    /// one machine anybody actually plays. The fix is the same seam ([`Host::run_sinks`]), so the witness
    /// is the same shape:
    ///
    /// 1. **The negative control first**, while it is unambiguous: frames the instruments are not lent to
    ///    advance the machine and leave the sample empty. That is precisely what an unattached profiler
    ///    does on EVERY frame of a hosted session.
    /// 2. **Then the real loop**: the player runs the machine, lends both instruments, and the sample the
    ///    wire serves has those frames in it — with rows, and with the reconciliation identity closing over
    ///    a sample that only the player's runs produced.
    /// 3. **And arming is invisible to the loop's shape**: the same `player_frame` call is what feeds it,
    ///    so nothing here depends on the player knowing a profiler exists.
    #[test]
    fn a_profiler_armed_over_the_bus_measures_the_players_own_frames() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted_with_vint();
        let mut cap = ScanlineCapture::new(Retain::LastFrame);

        let armed = call_ok(
            &mut h,
            &mut sys,
            "emulator/set_profiler",
            json!({"enabled": true}),
        );
        assert_eq!(armed["enabled"], json!(true));

        // --- The negative control. Frames without the instruments: the machine moves, the sample does not.
        let mclk = sys.scheduler().now();
        for _ in 0..3 {
            player_frame(&mut h, &mut sys, &mut cap, false);
        }
        assert!(sys.scheduler().now() > mclk, "the machine really advanced");
        let unlent = call_ok(&mut h, &mut sys, "emulator/get_profiler_frames", json!({}));
        assert_eq!(
            unlent["frameCount"],
            json!(0),
            "an unlent profiler reports frames it never saw as no frames at all — the M1 defect: {unlent}"
        );

        // --- The real loop.
        for _ in 0..4 {
            player_frame(&mut h, &mut sys, &mut cap, true);
        }
        let s = call_ok(&mut h, &mut sys, "emulator/get_profiler_frames", json!({}));
        let frames = s["frameCount"].as_u64().expect("a count");
        // Not `== 4`: a sample is delimited by frame boundaries at both ends, so the frame in flight when
        // it opened and the one in flight when it is read are not whole frames of it. The claim being
        // made is that the player's frames landed in a sample that was empty a moment ago.
        assert!(
            frames >= 3,
            "the bus's profiler rode the PLAYER's runs: {s}"
        );
        assert!(
            s["sampleCycles"].as_u64().is_some_and(|c| c > 0),
            "and measured them: {s}"
        );
        assert!(
            s["routines"]["items"]
                .as_array()
                .is_some_and(|r| !r.is_empty()),
            "the fixture's VInt handler and its callees are rows: {s}"
        );

        // The identity, over a sample no engine-driven run contributed to. Undivided, so this needs no
        // `perFrameExact` branch — see `tests/profiler.rs`.
        let sum = |v: &Value| -> u64 {
            let rows: u64 = v["routines"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["cyclesSelfTotal"].as_u64().unwrap())
                .sum();
            rows + ["hint", "vint"]
                .iter()
                .map(|k| v["interrupts"][*k]["cyclesSelfTotal"].as_u64().unwrap())
                .sum::<u64>()
        };
        assert_eq!(
            sum(&s) + s["unattributedCycles"].as_u64().unwrap(),
            s["sampleCycles"].as_u64().unwrap(),
            "the hosted sample reconciles exactly, like any other: {s}"
        );

        // …and `get_profiler`'s count is the same number, from a sample the player produced.
        let state = call_ok(&mut h, &mut sys, "emulator/get_profiler", json!({}));
        assert_eq!(state["framesRecorded"], json!(frames));
    }

    /// The other consequence of one shared instrument: the panel can arm a census by a key this **bus** does
    /// not expose. §6 exposes three of core's seven `CensusKey` variants, and the panel is not limited to
    /// them, so `watchpoint_list` must answer for a watch it could not have created.
    ///
    /// It reports the census counts, which are real, **omits** `censusKey`, and says why in a `caveat`.
    /// Relabelling the key as the nearest exposed spelling — an `AddrPage(8)` census reported as `addr` —
    /// would put a wrong name on a correct number, and a client would read a page count as an address count.
    #[test]
    fn a_census_by_a_key_this_bus_does_not_expose_is_reported_without_one() {
        use oracle_core::watchpoints::{CensusKey, Watch, WatchMode, WatchOp};
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted_with_vint();
        let mut cap = ScanlineCapture::new(Retain::LastFrame);
        h.watchpoints_mut().add(
            Watch::bus(0x00FF_0000..=0x00FF_FFFF, WatchOp::Write, "panel pages")
                // 256-byte pages: a legitimate core census with no wire spelling.
                .mode(WatchMode::Census(CensusKey::AddrPage(8))),
        );
        player_frame(&mut h, &mut sys, &mut cap, true);

        let list = call_ok(&mut h, &mut sys, "emulator/watchpoint_list", json!({}));
        let w = &list["watches"][0];
        assert_eq!(w["mode"], json!("census"));
        assert!(
            w.get("censusKey").is_none(),
            "an unexposed key must not be relabelled as an exposed one: {w}"
        );
        assert!(
            w["census"].as_array().is_some_and(|c| !c.is_empty()),
            "the counts are real and are reported: {w}"
        );
        let caveat = list["caveat"].as_str().expect("the hole is explained");
        assert!(caveat.contains("censusKey"), "{caveat}");
    }

    /// A `stopAfter` watch armed over the bus does **not** silently halt the player's own loop: the halt is
    /// a property of a run whose sink asked for it, and the player's loop does not consult it. §6 answers
    /// this case by attribution rather than by a gate, and the attribution is on the *bus's* runs.
    #[test]
    fn a_stop_after_watch_does_not_wedge_the_players_loop() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted_with_vint();
        let mut cap = ScanlineCapture::new(Retain::LastFrame);
        call_ok(
            &mut h,
            &mut sys,
            "emulator/watchpoint_add",
            json!({"addr": "0x00FF8000", "len": 2, "stopAfter": 1}),
        );
        let mclk = sys.scheduler().now();
        for _ in 0..3 {
            player_frame(&mut h, &mut sys, &mut cap, true);
        }
        assert_eq!(
            (sys.scheduler().now() - mclk) / MCLK_PER_FRAME,
            3,
            "the window keeps running: a watch cannot pause a machine nobody asked it to pause"
        );
    }

    /// Conflict 3: after a client-driven run the picture has moved, and the host is told so.
    #[test]
    fn a_client_driven_run_moves_the_picture_and_reports_it() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        assert!(h.framebuffer().is_none(), "nothing drawn yet");
        h.engine.swap_system(&mut sys);
        let gen = h.engine.screen_generation();
        h.engine
            .dispatch("emulator/run_frames", &json!({"frames": 2}))
            .expect("run");
        h.engine.swap_system(&mut sys);
        assert!(h.engine.screen_generation() > gen, "the picture moved");
        let (w, px) = h.framebuffer().expect("a frame was captured");
        assert!(
            w > 0 && px.len() == w * 224,
            "a whole frame, not a fragment"
        );
    }
}
