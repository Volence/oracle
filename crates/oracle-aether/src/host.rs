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
use crate::rpc::RpcError;
use crate::server::{spawn_accept, AcceptCtx, EngineMsg, Server, ServerConfig};
use oracle_core::bus::Observe;
use oracle_core::io::Pad;
use oracle_core::scanline_capture::ScanlineCapture;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::{System, MCLK_PER_FRAME};
use serde_json::{Map, Value};
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
    /// **Four** producers raise it, and the name is narrower than the meaning because the first two came
    /// first:
    ///
    /// - `emulator/reload_rom` — different cartridge bytes.
    /// - `emulator/restore` — a checkpoint carries its whole machine, ROM included (D13 rule 2), so it *may*
    ///   have brought a different cartridge back; the flag moves unconditionally rather than guessing.
    /// - `emulator/reset` — the same cartridge, back at its power-on anchor. The bytes did not change, but
    ///   everything clocked to the machine did, and a caller that resynchronised for the other two and not
    ///   for this one would be holding an audio clock and a frame counter from a timeline that no longer
    ///   exists.
    /// - [`Host::machine_replaced`] — a gesture at the embedder's **own window** replaced the machine
    ///   through no served method: a save-state load at its keys. §11.40 (CR-Q, 2026-09-05) added it, and
    ///   it is the producer whose absence was the defect: the window swapped its `System` and this flag
    ///   never moved, so a client attached over the socket learned nothing and the window's own repairs
    ///   never ran either.
    ///
    /// Read it as "resynchronise", not as "re-read the ROM".
    pub rom_changed: bool,
    /// **The symbol listing the engine resolves against was replaced** — and, unlike
    /// [`rom_changed`](PumpReport::rom_changed), that is *all* that happened when this is the only flag
    /// set. Nothing about the cartridge, the clock, the picture or the ROM path moved.
    ///
    /// A host that caches the listing (both of ours do — the panels and the status strips resolve names
    /// against a clone rather than through a call) re-derives it from
    /// [`Host::symbols`](Host::symbols). A host that does not cache it ignores this field.
    ///
    /// **Why this is not `rom_changed`.** `emulator/load_symbols` may be called at any time, against an
    /// unchanged cartridge, and it replaces the listing wholesale. Raising `rom_changed` for it would
    /// have been the cheap fix and the wrong signal: `rom_changed` means *the machine was replaced*, and
    /// the two hosts that read it answer by dropping a scanline capture, rebuilding an audio clock and
    /// re-keying save-state slots. A listing change invalidates none of those, so reusing the flag would
    /// have produced the right cache repair by way of an audible audio resync and a discarded capture —
    /// correct output for the wrong reason, and silent until somebody loaded symbols mid-session.
    ///
    /// **It is raised by every producer that replaces the listing, not only by the lone one.**
    /// `emulator/reload_rom`'s D7 drop and `emulator/restore`'s swap set this *and* `rom_changed`, so a
    /// host may react to this field alone and still be correct. That redundancy is deliberate: a flag
    /// that were true only when no other flag fired could not be read on its own.
    pub symbols_changed: bool,
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
    /// A breakpoint halt the **host's own run** observed, applied at the top of the next drain.
    ///
    /// Deferred for [`pending_free_run`](Host::pending_free_run)'s reason and one more of its own.
    /// The `emulator/stopped` this produces carries the machine stamp (D11), and the stopping `pc` is read
    /// off the machine — outside the window the engine holds the placeholder, so applying it where the
    /// observation is made would emit `frame 0, mclk 0, pc 0x00000000` for the one event a client most
    /// needs the truth about.
    ///
    /// **And the deferral is what makes the ordering expressible.** The halt is applied *after*
    /// `pending_free_run` in [`pump`](Host::pump), so a pause change queued in the same iteration cannot
    /// resurrect `free_run` over it. That collision is real and reachable: a human un-pausing the window on
    /// the very iteration whose frame hits a breakpoint queues `pending_free_run = Some(true)` from
    /// [`set_paused`](Host::set_paused) *after* the run has already halted, and applying that second would
    /// be a machine that pauses and instantly resumes.
    pending_break: Option<u32>,
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
            pending_break: None,
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

    /// **The other half of conflict 2, and the ONE implementation of it** — OR the client's held set into
    /// the pads a host is about to write, per button, both ports.
    ///
    /// It lives here rather than in a host because every term is `Host` state and because there were about
    /// to be two of it: `oracle-frontend`'s `Bus::merge_held` was the only copy, and `oracle-player` needed
    /// the same fact. This repo has a standing bar against a second spelling of one fact (the tabs parcel
    /// published `watch_wire_id`/`breakpoint_wire_id` rather than let a panel `format!` its own), and two
    /// merges agree right up until the day one of them learns about a button the other does not.
    ///
    /// # ⚑ There is deliberately no `is_serving()` early return, and its absence is the fix
    ///
    /// The copy this replaced opened with `if !self.host.is_serving() { return pads; }`. That was a **fast
    /// path, not a semantic**: `held` is `Pad::default()` until something calls `emulator/hold`, and a
    /// per-button OR with a default pad is the identity — so on the unserved frontend the gate and the
    /// merge return the same array (`unserved_merge_is_the_identity_the_is_serving_gate_used_to_shortcut`
    /// below proves it).
    ///
    /// Keeping it would have been actively wrong for the **hosted, socket-less** player, where `Host::call`
    /// is reachable in-process (contract D15: an in-process GUI *is* a client) and can therefore install a
    /// held set while `is_serving()` is false. Under the gate that set would sit in the engine, be reported
    /// back by `emulator/hold`'s own `held` array, and never reach the pad — a served capability answering
    /// that it took effect when it did not, which is the silent-wrong-answer class this surface cannot
    /// afford.
    pub fn merge_held(&self, pads: [Pad; 2]) -> [Pad; 2] {
        [
            crate::engine::merge_pads(pads[0], self.held(0)),
            crate::engine::merge_pads(pads[1], self.held(1)),
        ]
    }

    // ---------------------------------------------------------------- the glass (§11.29, CR-H)

    /// **Hand the bus the text the host's own present just put on the glass**, so `emulator/screen_text`
    /// answers with what a human can actually read on the window.
    ///
    /// The same seam shape as [`set_live_pads`](Host::set_live_pads), and for the same reason it needs no
    /// lock and no thread: hosted, the bus handlers run on the frontend's **own main thread**, synchronously,
    /// inside [`pump`](Host::pump). The push happens at the end of iteration *N*'s present and the next
    /// drain is at the top of *N+1*, so the served text describes the frame that is actually on the glass —
    /// never one being composed, never one that has not been shown.
    ///
    /// **Deliberately NOT gated on [`has_clients`](Host::has_clients)**, unlike
    /// [`publish_capture`](Host::publish_capture) beside it, and the difference is not an oversight. That
    /// gate skips a whole frame's memcpy; this is a handful of short strings the frontend composed anyway.
    /// Gating it would leave a client that connects mid-session reading `-32005 noDisplay` — *there is no
    /// window* — until the next present, which is a false answer to the one question this method exists to
    /// answer truthfully. The frontend does its own skipping, one level up, by not building a snapshot at
    /// all when it is not serving a socket.
    ///
    /// Safe to call outside a drain window: this is engine state, not `System` state, so it never answers
    /// for the placeholder machine.
    pub fn set_screen_text(&mut self, surfaces: Vec<crate::engine::ScreenSurface>) {
        self.engine.set_screen_text(surfaces);
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

    /// **The display layer mask** the engine's picture-serving rows read, lent to the process that owns the
    /// window.
    ///
    /// Exactly the [`watchpoints_mut`](Host::watchpoints_mut) argument one surface over. The player draws
    /// its own window and now has its own layer toggles; if it kept a mask of its own, a client's
    /// `emulator/set_layer_enabled` would change `emulator/screenshot` and not the picture on the monitor,
    /// and a palette toggle would do the reverse. There is one mask, it lives on the engine, and both the
    /// socket and the palette move that one — so they cannot drift, because there is nothing to drift apart
    /// *from*.
    ///
    /// Safe outside a drain window: the mask is engine state, never `System` state, so this never answers
    /// for the placeholder machine.
    pub fn layers(&self) -> oracle_core::render::LayerMask {
        self.engine.layers()
    }

    /// Set one layer's mask bit from the window side. Returns whether `layer` is a mask target at all
    /// (`false` for `Layer::Backdrop`). See [`layers`](Host::layers).
    pub fn set_layer(&mut self, layer: oracle_core::render::Layer, enabled: bool) -> bool {
        self.engine.set_layer(layer, enabled)
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
    /// **The third element is the breakpoint sink, bare**, and a host that drops its observation on the
    /// floor has a machine that stopped with nothing saying so. Hand it to
    /// [`record_break`](Host::record_break) after the run. `resume_pc` is the machine's PC *before* the
    /// run — see [`Engine::run_sinks`](crate::engine::Engine::run_sinks) for why the caller has to supply
    /// it and why this one half is not wrapped in [`Observe`](oracle_core::bus::Observe).
    pub fn run_sinks(
        &mut self,
        resume_pc: u32,
    ) -> (
        Option<Observe<&mut oracle_core::watchpoints::Watchpoints>>,
        Option<Observe<&mut oracle_core::profiler::Profiler>>,
        Option<crate::breakpoints::BreakStop<'_>>,
    ) {
        self.engine.run_sinks(resume_pc)
    }

    /// **Latch a breakpoint halt the host's own run observed**, for the top of the next
    /// [`pump`](Host::pump).
    ///
    /// `addr` is the address the sink from [`run_sinks`](Host::run_sinks) stopped on. Calling this is the
    /// whole of what a host owes the breakpoint surface: the sink already ended the run, and this is what
    /// turns that into a counted hit, a cleared pair of run flags, and the `emulator/stopped` the client
    /// waiting on `wait_for_break` is owed. A host that runs the sink and never calls this has the
    /// *worse* of the two failures — a machine that halts silently.
    ///
    /// Nothing here touches the engine, so it is safe outside a drain window; that is the point. See
    /// [`pending_break`](Host::pending_break) for why the apply is deferred and why it is ordered after
    /// `pending_free_run`.
    ///
    /// **An unapplied latch is never overwritten.** Today exactly one frame runs between a latch and the
    /// drain that takes it, so a second cannot arrive — but if one ever did, the *earlier* halt is the one
    /// that stopped the machine, and silently replacing it would report the wrong address for the stop.
    pub fn record_break(&mut self, addr: u32) {
        self.pending_break.get_or_insert(addr);
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

    /// **The breakpoint set**, forwarded — what is armed to stop this machine, for a host that shows it.
    ///
    /// Separate from [`read_instruments`](Host::read_instruments) rather than a fourth element of it, for
    /// the reason [`Engine::read_breakpoints`](crate::engine::Engine::read_breakpoints) gives: a
    /// breakpoint halts where an instrument records, and both are `&self` borrows, so a caller wanting
    /// all four live at once calls both.
    ///
    /// The same set `emulator/breakpoint_list` pages and `emulator/breakpoint_add` grows. A host's panel
    /// and a client's reply therefore cannot disagree about what is armed, and the shared borrow says in
    /// the type that the panel cannot arm anything through it.
    pub fn read_breakpoints(&self) -> &crate::breakpoints::Breakpoints {
        self.engine.read_breakpoints()
    }

    /// **The last breakpoint halt this bus actually performed**, forwarded — see
    /// [`Engine::last_break`](crate::engine::Engine::last_break).
    ///
    /// [`read_breakpoints`](Host::read_breakpoints) answers *what is armed*; this answers *what stopped
    /// the machine, where, and how many times*. A host drawing an alarm needs both, and neither is
    /// derivable from the other.
    ///
    /// **It reads through a latch this host may not have applied yet.** A halt observed by the host's own
    /// run is handed to [`record_break`](Host::record_break) and applied at the top of the next
    /// [`pump`](Host::pump) — so between those two points this still reports the *previous* halt. That is
    /// the right answer rather than a stale one: until the apply, no halt has happened, the machine is
    /// still marked running, and a surface that reported the pending observation would be announcing a
    /// stop that a client clearing the breakpoint in the same window can still cancel.
    pub fn last_break(&self) -> Option<crate::engine::LastBreak> {
        self.engine.last_break()
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
    /// The listing the engine resolves against **now** — see [`Engine::symbols`](crate::engine::Engine::symbols)
    /// for why an embedder must re-read it rather than trust the clone it handed to
    /// [`set_machine_info`](Host::set_machine_info).
    pub fn symbols(&self) -> Option<&SymbolTable> {
        self.engine.symbols()
    }

    /// The absolute path of the loaded image — see [`Engine::rom_path`](crate::engine::Engine::rom_path).
    pub fn rom_path(&self) -> Option<&str> {
        self.engine.rom_path()
    }

    pub fn framebuffer(&self) -> Option<crate::engine::FrameRef<'_>> {
        self.engine.latched_frame()
    }

    // ---------------------------------------------------------------- the synchronous call (D15)

    /// **Answer one command synchronously, in-process, against the caller's machine** — [`pump`](Host::pump)'s
    /// swap-and-dispatch without the queue, and without the wait.
    ///
    /// This is what the contract says an in-process GUI *is*. `protocol.md` D15: an in-process GUI is
    /// *"a consumer of the same registry, not a second server … it reads the method registry directly,
    /// in-process; it does not open a socket to itself."* A window that owns the machine can therefore ask
    /// the tool's own handler a question and get the tool's own answer, the tool's own refusal and the
    /// tool's own error text — no wire, no process boundary, no one-frame latency.
    ///
    /// **It is NOT a way around [`pump`](Host::pump) for socket clients.** Everything arriving on a socket
    /// is queued as an `EngineMsg` and answered by the drain, under [`HostConfig::pump_budget`] and
    /// [`HOSTED_MAX_RUN_FRAMES`] — the two bounds that keep one client from freezing the window. This entry
    /// point has neither, because there is nobody to be fair to: the only caller is the process that owns
    /// the loop, and a call it makes of itself is its own frame time to spend. Routing socket traffic
    /// through here would delete both bounds at once.
    ///
    /// Returns the handler's result and the D11 stamp read *at reply time*, both taken while the real
    /// machine is swapped in — the same pair `pump` sends back to a connection thread. Never the
    /// placeholder's `frame 0, mclk 0`.
    ///
    /// # `pending_free_run` / `pending_break` are deliberately NOT applied here
    ///
    /// [`pump`](Host::pump) applies both at the top of a drain, in that order, and the order is
    /// load-bearing (see [`pending_break`](Host::pending_break) for the worked collision). This call
    /// applies neither, and that is a decision rather than an omission:
    ///
    /// * **Painting must not emit protocol events.** Both applies emit `emulator/stopped` /
    ///   `emulator/resumed`. A panel repainting at 60 Hz through this entry point would be minting
    ///   run-control events as a side effect of drawing itself, which is a new class of wrong answer on a
    ///   surface whose whole job is to be readable.
    /// * **A second apply site adds an interleaving point the ordering argument does not cover.** The
    ///   argument is written for one site, where the pair is applied back to back. Split it across two and
    ///   a loop ordered `run ▸ record_break ▸ call ▸ set_paused ▸ pump` applies the halt here, then lets
    ///   [`set_paused`](Host::set_paused) — which compares against the engine's now-halted state — queue
    ///   `free_run = true` for the drain. That is a machine that stops on a breakpoint and silently
    ///   resumes: exactly the believable wrong answer the ordering exists to prevent, reintroduced by the
    ///   duplication.
    ///
    /// **What that costs, named rather than hidden.** Between a latch and the next drain, a `call` to a
    /// method that reports run state (`emulator/status`'s stamp, `running`) can answer with the run state
    /// from before the halt or before the pause change. It is bounded by one iteration and it
    /// self-corrects at the next [`pump`](Host::pump). In the meantime [`is_paused`](Host::is_paused) is
    /// the truthful host-side reading — it already consults `pending_free_run` — so a panel that wants the
    /// pause state should read *that*, not a `call`.
    ///
    /// # Re-entrancy
    ///
    /// A nested call — one made while the engine already holds the real machine — is **statically
    /// impossible**, not merely avoided. Both this and `pump` take `&mut self`, `Engine` holds no
    /// reference back to its `Host`, and a handler receives only `&mut Engine`, so there is no path from
    /// inside a dispatch to another `call`. The engine can never be asked to swap a machine in while it is
    /// already holding one.
    ///
    /// The one hazard the borrow checker does *not* catch is **handing this a different `System` than the
    /// one the loop pumps**. Nothing breaks — the swap is symmetric either way — but the reply then
    /// describes whichever machine was passed in. Pass the machine the loop owns.
    ///
    /// Panic-safety is [`pump`](Host::pump)'s, unchanged: if a handler panics mid-call the real machine is
    /// left inside the engine and the caller's `sys` holds the placeholder. Neither path guards it today.
    pub fn call(
        &mut self,
        sys: &mut System,
        method: &str,
        params: &Value,
    ) -> (Result<Value, RpcError>, Map<String, Value>) {
        let (result, stamp, _) = self.call_reporting(sys, method, params);
        (result, stamp)
    }

    /// **[`call`](Host::call), plus the same four-coordinate diff [`pump`](Host::pump) takes** — for an
    /// embedder that dispatches its own gestures synchronously and has to repair whatever they moved.
    ///
    /// # ⚑ Why this exists, and it is a defect report rather than a convenience
    ///
    /// [`pump`](Host::pump) snapshots the three generation counters **inside itself**, deliberately: that
    /// is what keeps [`set_machine_info`](Host::set_machine_info) from surfacing as a client's doing, and
    /// its own comment says so. The unintended half of that choice is that a change made by a `call`
    /// *between* two drains is invisible to **both**: it lands after drain N has read the counters back
    /// and before drain N+1 reads them at its start, so the delta is zero on either side and no
    /// [`PumpReport`] anywhere ever mentions it.
    ///
    /// `oracle-frontend` never noticed, because it dispatches its own F5 by calling
    /// [`System::load_rom`](oracle_core::system::System::load_rom) directly and repairs the window inline
    /// beside it. `oracle-player` dispatches **every** served method from its command palette through
    /// `call` — `emulator/reload_rom`, `emulator/reset`, `emulator/restore`, `emulator/run_frames`
    /// included — and so had no report to repair from at all: a palette reset left that window's audio
    /// clock and scanline capture on a timeline that no longer existed, its cached symbol listing and ROM
    /// path stale, and a palette `run_frames` drew a frame that never reached the glass. All of it
    /// silent, and all of it the repairs its own drain already performs for a *client* doing the same
    /// thing.
    ///
    /// So the diff is the one thing that was missing, taken here in exactly the shape and the order
    /// `pump` takes it, in the same file, so the two readings of "what did that move" cannot drift.
    ///
    /// **`calls` is 1 and `deferred` is `false`, always** — one dispatch, synchronous, no budget. The
    /// other four fields carry the same meanings [`PumpReport`] documents, including
    /// [`timeline_moved`](PumpReport::timeline_moved) for a gesture that ran or rewound the machine.
    ///
    /// This is **not** a second dispatch path: it is [`call`](Host::call)'s body, and `call` is now a
    /// wrapper over it, so there is one place a synchronous gesture is answered.
    pub fn call_reporting(
        &mut self,
        sys: &mut System,
        method: &str,
        params: &Value,
    ) -> (Result<Value, RpcError>, Map<String, Value>, PumpReport) {
        // Read before the swap, beside each other, exactly as `pump` reads them.
        let screen_gen = self.engine.screen_generation();
        let rom_gen = self.engine.rom_generation();
        let symbols_gen = self.engine.symbols_generation();
        self.engine.swap_system(sys);
        let mclk_before = self.engine.mclk();
        let result = self.engine.dispatch(method, params);
        // Read inside the window, exactly as the drain does, so the stamp is the real machine's.
        let stamp = self.engine.stamp();
        let mclk_after = self.engine.mclk();
        self.engine.swap_system(sys);
        let report = PumpReport {
            calls: 1,
            mclk_before,
            mclk_after,
            screen_changed: self.engine.screen_generation() != screen_gen,
            rom_changed: self.engine.rom_generation() != rom_gen,
            symbols_changed: self.engine.symbols_generation() != symbols_gen,
            deferred: false,
        };
        (result, stamp, report)
    }

    /// **Tell the bus that a gesture at this window replaced the machine** — §11.40 (CR-Q, 2026-09-05).
    ///
    /// The embedder has already put the new `System` in `sys` (a save-state load is a whole-value swap:
    /// `oracle_frontend::save_state::load` returns a complete machine or an `Err`, so there is no window
    /// in which half a machine is running). This lends that machine to the engine exactly as
    /// [`call_reporting`](Host::call_reporting) does, runs
    /// [`Engine::note_machine_replaced`](crate::engine::Engine::note_machine_replaced) inside the lend,
    /// and hands it back.
    ///
    /// **The lend is not ceremony.** Every event's `params` carries the machine stamp (§2.2), and the
    /// engine holds an inert placeholder `System` outside a lend window — so an emit taken outside one
    /// would put `frame 0, mclk 0` on the wire beside a `hitsDropped` that was real. It is the same
    /// reason [`publish_stamp`](Host::publish_stamp) documents for itself, and the stamp is published
    /// here too, inside the window, so a connection thread polling it does not answer from the timeline
    /// the load just left.
    ///
    /// # It returns a [`PumpReport`], and the embedder must react to it like any other
    ///
    /// `rom_changed` is `true` — that is the defect this closes — so the host's own repairs (audio
    /// resync, symbol re-derive, cached ROM path) run off the same union they run off for a client-driven
    /// `reload_rom`. `screen_changed` is `true` because the latched picture was invalidated.
    /// `timeline_moved()` is whatever the clock actually did: a slot written earlier winds it backwards,
    /// and `mclk_before`/`mclk_after` are both read inside the window, so both are the **new** machine's
    /// — the load has already happened by the time this is called and there is no honest reading of the
    /// old one left to take. A caller that needs the before-clock must read it before it swaps.
    ///
    /// `calls` is 1 and `deferred` is `false`, [`call_reporting`](Host::call_reporting)'s convention for a
    /// synchronous gesture.
    ///
    /// # ⚑ Only call this from a deployment that advertises the event
    ///
    /// §11.40 M2 makes `capabilities.events` per process, and emitting an event this process did not
    /// advertise contradicts §2.1's *"authoritative event set"*. The two are gated by one flag,
    /// [`EngineConfig::window_gestures`](crate::engine::EngineConfig::window_gestures); set it where the
    /// gesture is wired, and `note_machine_replaced` debug-asserts it.
    pub fn machine_replaced(
        &mut self,
        sys: &mut System,
        reason: crate::engine::MachineReplacedReason,
    ) -> PumpReport {
        // Read before the lend, beside each other, exactly as `pump` and `call_reporting` read them.
        let screen_gen = self.engine.screen_generation();
        let rom_gen = self.engine.rom_generation();
        let symbols_gen = self.engine.symbols_generation();
        self.engine.swap_system(sys);
        let mclk_before = self.engine.mclk();
        self.engine.note_machine_replaced(reason);
        let mclk_after = self.engine.mclk();
        // Inside the window, for `pump`'s reason: a connection thread reads this stamp without a round
        // trip, and after a state load the cached one describes a timeline that no longer exists.
        self.publish_stamp();
        self.engine.swap_system(sys);
        PumpReport {
            calls: 1,
            mclk_before,
            mclk_after,
            screen_changed: self.engine.screen_generation() != screen_gen,
            rom_changed: self.engine.rom_generation() != rom_gen,
            symbols_changed: self.engine.symbols_generation() != symbols_gen,
            deferred: false,
        }
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
        // Snapshotted here, *inside* `pump` and beside the other two, which is what keeps
        // [`Host::set_machine_info`] from surfacing as a client's doing. That setter calls
        // `Engine::set_symbols` and so moves the counter, but it is called from the host's own thread
        // between drains — so the bump lands before this line reads it, and the comparison at the bottom
        // sees no change. A window that swaps its own cartridge (the frontend's F5) therefore does not
        // get told about its own listing.
        let symbols_gen = self.engine.symbols_generation();

        self.engine.swap_system(sys);
        report.mclk_before = self.engine.mclk();
        if let Some(on) = self.pending_free_run.take() {
            self.engine.set_free_run(on);
        }
        // **Ordered AFTER `pending_free_run`, and the order is load-bearing.** Both are deferred changes to
        // the same pair of run flags, and they can be queued in one iteration: a human un-pausing the window
        // on the very frame that hits a breakpoint leaves `pending_free_run = Some(true)` beside a latched
        // halt. Applied the other way round, the un-pause would land second and put `free_run` back — a
        // machine that pauses and instantly resumes, which is a *new* believable wrong answer rather than a
        // missing one. A halt is the later fact and it wins.
        //
        // Also ordered *before* the drain below, so a `wait_for_break` or a `run_frames` answered in this
        // very drain sees the halted machine rather than the one from before the frame.
        if let Some(addr) = self.pending_break.take() {
            self.engine.halt_on_breakpoint(addr);
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
                    // **Published BEFORE the reply, and the order is the whole of `wait_for_break`'s
                    // instant-timeout defect.** A client that has this reply in hand can send its next
                    // request immediately, and if that request is `emulator/wait_for_break` its
                    // connection thread polls exactly this stamp to decide whether to wait at all.
                    // Publishing at the end of the drain instead let a `resume` be *answered* while the
                    // stamp still carried `running: false` from the halt before it — so the wait exited
                    // after ~0 ms and the engine, re-reading the live flag, replied
                    // `{"timeoutReached": true}` to a caller who had asked for ten seconds. That is a
                    // wrong answer, not a slow one, and peer load widened the window rather than
                    // creating it. `engine_loop` has always published in this order; this is the same
                    // order on the second run driver.
                    self.publish_stamp();
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
        // The end-of-drain publication, which the per-reply one above does not replace: this is the one
        // that carries the *frame the player just ran* into the stamp, on the overwhelmingly common
        // iteration where no command arrived at all.
        self.publish_stamp();
        self.engine.swap_system(sys);

        report.screen_changed = self.engine.screen_generation() != screen_gen;
        report.rom_changed = self.engine.rom_generation() != rom_gen;
        report.symbols_changed = self.engine.symbols_generation() != symbols_gen;
        report
    }

    /// Publish the machine coordinate a connection thread reads without a round trip — the cached stamp on
    /// envelope-level errors, and the run state `emulator/wait_for_break` polls.
    ///
    /// **Must be called inside a drain window**, like everything that reads the clocks: outside one the
    /// engine holds the placeholder `System` and this would publish `mclk 0`.
    fn publish_stamp(&self) {
        self.ctx
            .shared
            .store(self.engine.mclk(), self.engine.is_running());
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
    use std::sync::Arc;

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

    /// **The window's mask and the socket's mask are the same field** — the whole claim behind
    /// [`Host::layers`], and the one thing an in-process accessor can quietly get wrong by holding a copy.
    ///
    /// Driven from **both ends against each other**, which is what makes it a delegation test rather than a
    /// getter test: a `set_layer` from the window side is read back through the *served method*, and a
    /// `emulator/set_layer_enabled` from the client side is read back through the *window accessor*. A
    /// `Host` that kept its own `LayerMask` would pass each half against itself and fail both crossings.
    ///
    /// Planting the defect: make `Host::layers` answer `LayerMask::ALL` instead of `self.engine.layers()`
    /// — a getter that holds its own idea of the mask. The **socket→window** crossing fails with *"a client
    /// hid sprites and the window's accessor did not see it — the two are not one mask"*, `left: []`.
    /// Measured, and worth recording precisely: the window→socket crossing stayed **green** under that
    /// poison, because only the getter was copied and `set_layer` still reached the engine. That is exactly
    /// why both crossings are here — either one alone passes against a half-copy.
    #[test]
    fn the_window_and_the_socket_move_one_mask() {
        use oracle_core::render::Layer;
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        assert!(
            h.layers().is_all(),
            "a fresh host draws every layer, or the crossings below start from an unknown state"
        );

        // Window -> socket.
        assert!(h.set_layer(Layer::PlaneA, false), "planeA is a mask target");
        let (tx, rx) = mpsc::channel();
        h.tx.send(EngineMsg::Call {
            method: "emulator/get_layer_states".into(),
            params: serde_json::json!({}),
            reply: tx,
        })
        .expect("queue the call");
        h.pump(&mut sys);
        let r = rx.try_recv().expect("the drain answered");
        let v = r.result.expect("get_layer_states must answer");
        assert_eq!(
            v["planeA"],
            serde_json::json!(false),
            "the window hid planeA and the served getter did not see it — Host::layers is a copy"
        );
        assert_eq!(
            v["planeB"],
            serde_json::json!(true),
            "and nothing else moved"
        );

        // Socket -> window.
        let (tx, rx) = mpsc::channel();
        h.tx.send(EngineMsg::Call {
            method: "emulator/set_layer_enabled".into(),
            params: serde_json::json!({"layer": "sprites", "enabled": false}),
            reply: tx,
        })
        .expect("queue the call");
        h.pump(&mut sys);
        let r = rx.try_recv().expect("the drain answered");
        r.result.expect("set_layer_enabled must succeed");
        assert_eq!(
            h.layers().hidden(),
            vec!["planeA", "sprites"],
            "a client hid sprites and the window's accessor did not see it — the two are not one mask"
        );

        // The backdrop is not a target, and the refusal leaves the mask alone rather than pretending.
        let before = h.layers();
        assert!(!h.set_layer(Layer::Backdrop, false));
        assert_eq!(h.layers(), before, "a refused set must not move the mask");
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

    /// ★ **A synchronous gesture reports what it moved — and the next drain does not.**
    ///
    /// Both halves are the point, and the second one is why this method exists at all. `pump` snapshots
    /// the generation counters *inside itself* ([`Host::pump`]'s own comment says so, deliberately), so a
    /// change a `call` makes between two drains lands in the gap: after drain N read them back and before
    /// drain N+1 reads them at its start. Nothing anywhere ever mentions it.
    ///
    /// So the assertions are paired. The gesture's own report must name the change; the drain that
    /// follows it must **not**, because that is the fact an embedder has to be built against. An
    /// embedder that "simplified" [`Host::call_reporting`] away and read the next `PumpReport` instead
    /// would fail the first half here rather than discovering it as silence at a window.
    ///
    /// `emulator/reset` is the gesture because it moves three of the four coordinates at once — the ROM
    /// generation (`PumpReport::rom_changed`'s third producer), the clock, and the picture — with no
    /// file, no socket and no client involved.
    #[test]
    fn a_synchronous_gesture_reports_what_it_moved_and_the_next_drain_does_not() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        sys.run_frames(2); // a clock that is not 0, so a restart is visible as a *change*
        let before = sys.scheduler().now();
        assert!(before > 0, "the fixture must have a clock to restart");

        let (result, _stamp, report) =
            h.call_reporting(&mut sys, "emulator/reset", &serde_json::json!({}));
        assert!(result.is_ok(), "emulator/reset: {:?}", result.err());
        assert!(
            report.rom_changed,
            "a reset replaces the machine — the gesture's own report must say so"
        );
        assert!(
            report.timeline_moved(),
            "a reset restarts the clock, so the caller's audio ring and capture are on a dead timeline"
        );
        assert!(
            report.screen_changed,
            "a reset invalidates the picture, and the caller has to be told to stop presenting it"
        );
        assert_eq!(report.calls, 1, "one dispatch, one call");
        assert!(
            !report.deferred,
            "a synchronous gesture has no budget to run out"
        );
        assert_ne!(
            sys.scheduler().now(),
            before,
            "anti-vacuity: the reset must actually have moved the machine, or every flag above is a \
             claim about nothing"
        );

        // …and the half an embedder cannot see for itself.
        let after = h.pump(&mut sys);
        assert!(
            !after.rom_changed && !after.screen_changed && !after.timeline_moved(),
            "the drain after a synchronous gesture reports NOTHING about it — that is the gap \
             `call_reporting` exists to close, and an embedder that waits for this report waits forever"
        );
    }

    /// ★ **A window gesture reports what it moved, in the same terms a served method does** — §11.40
    /// (CR-Q, 2026-09-05).
    ///
    /// The row above is the same claim for a synchronous *method*; this is the claim for a gesture that
    /// no method produced, and it is the half the defect was. A window swapped its `System` and
    /// `rom_generation` never moved, so nothing downstream fired: not the client's event, and not the
    /// window's own repairs, which key off exactly this flag.
    ///
    /// **`rom_changed` is asserted HERE and not over a socket, and that is deliberate rather than
    /// convenient.** It was measured: deleting `self.rom_generation += 1` from
    /// [`Engine::note_machine_replaced`](crate::engine::Engine::note_machine_replaced) leaves every row
    /// in `tests/machine_replaced.rs`, in `tests/hosted.rs` and in `oracle-player`'s suite **green** —
    /// the event still goes out, the picture is still invalidated, the hits still drain. The generation
    /// counter is not on the wire and is not observable through any reply; the one place it surfaces is
    /// the `PumpReport` an embedder reads. So a gate for it has to be here, holding the report.
    #[test]
    fn a_window_gesture_reports_the_machine_replacement_the_way_a_method_would() {
        let mut h = Host::new(HostConfig {
            engine: EngineConfig {
                window_gestures: true,
                ..HostConfig::default().engine
            },
            ..HostConfig::default()
        });
        let mut sys = booted();
        // A latched picture and a clock that is not zero, so "invalidated" and "moved" are visible as
        // changes rather than as the state the fixture started in.
        h.pump(&mut sys);
        sys.run_frames(2);

        let report = h.machine_replaced(&mut sys, crate::engine::MachineReplacedReason::StateLoad);

        assert!(
            report.rom_changed,
            "the gesture replaced the machine and its own report must say so — this is the defect \
             §11.40 closes, and it is invisible everywhere else"
        );
        assert!(
            report.screen_changed,
            "the latched picture was invalidated, so the embedder must stop presenting it"
        );
        assert_eq!(report.calls, 1, "one gesture, one report");
        assert!(
            !report.deferred,
            "a synchronous gesture has no budget to run out"
        );
        assert!(
            !report.symbols_changed,
            "a state load keeps the cartridge, so the listing that bound to it still binds — this is \
             `reset`'s answer, not `reload_rom`'s"
        );

        // …and the drain that follows says nothing about it, which is `call_reporting`'s gap at a third
        // producer: an embedder that waited for the next `PumpReport` would wait forever.
        let after = h.pump(&mut sys);
        assert!(
            !after.rom_changed && !after.screen_changed,
            "the drain after a gesture reports NOTHING about it"
        );
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

    /// **The claim `Host::merge_held` makes when it drops the `is_serving()` early return**: on a host with
    /// nothing held the merge *is* the identity the gate used to shortcut, and on a host with something
    /// held it is not — even though that host is unserved.
    ///
    /// Both halves are in one test on purpose. The identity half alone goes green for a second reason
    /// entirely: a `merge_held` that returned its argument unconditionally — which is exactly what the
    /// deleted gate did on this host — passes it perfectly. The second half is what rules that out, and
    /// separating them would let a regression restore the gate and keep one green row.
    #[test]
    fn unserved_merge_is_the_identity_the_is_serving_gate_used_to_shortcut() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        assert!(
            !h.is_serving(),
            "the whole point is that no socket is bound; a served host would prove the other case"
        );

        // A pad with something in it, so an identity that came from returning `Pad::default()` twice
        // could not pass for the identity that comes from OR-ing with an empty held set.
        let human = [
            Pad {
                left: true,
                ..Pad::default()
            },
            Pad {
                start: true,
                ..Pad::default()
            },
        ];
        assert_ne!(human, [Pad::default(); 2], "the fixture pad is not empty");
        assert_eq!(
            h.merge_held(human),
            human,
            "nothing is held, so the merge must return the human's pads unchanged — this is the identity \
             the deleted `is_serving()` gate was shortcutting, and dropping it changed no answer"
        );

        // …and the same unserved host, after an in-process `emulator/hold`, must NOT be the identity.
        // This is the player's entire path: `Host::call` with no socket bound (contract D15).
        h.engine.swap_system(&mut sys);
        h.engine
            .dispatch("emulator/hold", &json!({"buttons": ["a"], "port": 1}))
            .expect("hold");
        h.engine.swap_system(&mut sys);
        assert!(
            !h.is_serving(),
            "still unserved — no socket was bound by that"
        );
        assert!(h.held(1).a, "the engine took the hold");

        let merged = h.merge_held(human);
        assert_ne!(
            merged, human,
            "an unserved host with a held set merged nothing — the `is_serving()` gate is back, and a \
             client's `emulator/hold` against the hosted player is inert again"
        );
        assert!(
            merged[1].a && merged[1].start,
            "port 1 carries both sources"
        );
        assert_eq!(
            merged[0], human[0],
            "and port 0, which nothing held, is untouched"
        );
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
            // `run_sinks`, never the bare instruments: an unwrapped watch would halt this loop. All three
            // halves ride, exactly as `oracle-frontend/src/main.rs` attaches them — a loop that took only
            // the watch is the M1 defect, and it is what the negative controls below run without. The
            // breakpoint half is bare and its observation is handed back, which is the loop's whole
            // obligation to that surface.
            let resume_pc = sys.cpu_regs().pc;
            let (watch, prof, mut brk) = h.run_sinks(resume_pc);
            {
                let mut sink = oracle_core::bus::Fanout::new(
                    &mut *cap,
                    oracle_core::bus::Fanout::new(
                        &mut brk,
                        oracle_core::bus::Fanout::new(watch, prof),
                    ),
                );
                sys.run_frames_with_sink(1, &mut sink);
            }
            if let Some((_, addr)) = brk.and_then(|b| b.fired) {
                h.record_break(addr);
            }
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

    // ------------------------------------------------------------------ the hosted breakpoint halt

    /// The head of the fixture ROM's inner stirring loop — a PC [`booted`]'s machine executes constantly.
    /// Anchored to the instruction rather than to the number by
    /// [`assert_hot_pc_is_the_stirring_loop`](tests::assert_hot_pc_is_the_stirring_loop).
    const HOT_PC: u32 = 0x0000_020E;

    /// `HOT_PC` names `move.w (A0), D0` (`$3010`) in the ROM [`booted`] loads. A test that armed a
    /// breakpoint at a dead address would pass its ordering assertions vacuously, so this is checked
    /// first in every one of them.
    fn assert_hot_pc_is_the_stirring_loop() {
        let rom = oracle_core::testrom::build();
        let a = HOT_PC as usize;
        let op = u16::from_be_bytes([rom[a], rom[a + 1]]);
        assert_eq!(
            op, 0x3010,
            "the fixture ROM moved: 0x{HOT_PC:08X} is no longer the hot loop"
        );
    }

    /// ## **A halt must outrank a pause change queued in the SAME iteration.**
    ///
    /// Both are deferred to the top of the next drain and both move the same pair of run flags, and there
    /// is one real, reachable way for them to collide: a **human un-pausing the window on the very
    /// iteration whose frame hits a breakpoint.** The frame runs and halts, then `set_paused(false)` sees
    /// the engine still paused, and queues `pending_free_run = Some(true)` behind the latched halt.
    ///
    /// Applied the other way round that is a machine which pauses and instantly resumes — a *new*
    /// believable wrong answer, and the one this parcel could most easily have shipped. Poison it by
    /// moving the `pending_break` apply above the `pending_free_run` apply in [`Host::pump`].
    ///
    /// **The two pre-pump assertions are what stop this going green for the wrong reason.** If no
    /// breakpoint fired, or if `set_paused` never queued the un-pause, the ordering is not exercised at
    /// all and the final assertion would hold for a reason that has nothing to do with the rule. Both are
    /// therefore checked as facts before the drain that has to resolve them.
    #[test]
    fn a_halt_outranks_an_unpause_queued_in_the_same_iteration() {
        assert_hot_pc_is_the_stirring_loop();
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        let mut cap = ScanlineCapture::new(Retain::LastFrame);

        // The window is paused, which is the state a human is un-pausing *out of*.
        h.set_paused(true);
        h.pump(&mut sys);
        assert!(h.is_paused(), "the fixture starts paused");

        call_ok(
            &mut h,
            &mut sys,
            "emulator/breakpoint_add",
            json!({"addr": crate::hex::addr(HOT_PC)}),
        );

        // The colliding iteration: the human un-paused, so this iteration runs a frame — and that frame
        // hits the breakpoint.
        player_frame(&mut h, &mut sys, &mut cap, true);
        h.set_paused(false);

        assert_eq!(
            h.pending_break,
            Some(HOT_PC),
            "no halt was latched, so this test would prove nothing about ordering"
        );
        assert_eq!(
            h.pending_free_run,
            Some(true),
            "no un-pause was queued, so there is nothing for the halt to outrank"
        );

        h.pump(&mut sys);

        assert!(
            h.is_paused(),
            "the un-pause landed after the halt and put `free_run` back — the window paused and \
             instantly resumed"
        );
        // …and it stays paused across the next iteration, which is the one the *player* actually runs:
        // it read `is_paused()` above, so it now mirrors `true` back and runs no frame. (A second
        // `set_paused(false)` here would be a human pressing un-pause *again*, which must resume and is
        // not what this test is about — asserting otherwise was an over-assertion this fixture caught.)
        h.set_paused(h.is_paused());
        h.pump(&mut sys);
        assert!(h.is_paused(), "and it must still be paused a drain later");
    }

    // ---------------------------------------------------------------- Host::call (D15)

    /// **`call` answers against the real machine, not the placeholder** — the property the whole swap
    /// exists for, checked on the synchronous path as well as the queued one.
    ///
    /// The negative control is the point: a `call` that forgot to swap would still *return* a stamp and a
    /// register block, all zeros, and read as a pass. So the fixture advances the machine to a coordinate
    /// the placeholder cannot have, and the stamp and `pc` are checked against it.
    #[test]
    fn call_answers_against_the_real_machine_and_stamps_it() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        sys.run_frames(3);
        let mclk = sys.scheduler().now();
        let pc = sys.cpu_regs().pc;
        assert!(
            mclk > 0 && pc != 0,
            "the fixture must not look like a placeholder"
        );

        let (result, stamp) = h.call(&mut sys, "emulator/registers", &json!({}));
        let v = result.expect("emulator/registers answers");
        assert_eq!(
            v["pc"],
            json!(crate::hex::addr(pc)),
            "the real machine's PC"
        );
        assert_eq!(stamp["mclk"], json!(mclk), "the real machine's clock (D11)");
        assert_eq!(
            sys.scheduler().now(),
            mclk,
            "and the machine came back — a call is a lend, not a take"
        );
    }

    /// A `call` refuses exactly as the socket does: same registry, same error, no second reading of it.
    #[test]
    fn call_refuses_an_unknown_method_the_way_dispatch_does() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        let (result, _) = h.call(&mut sys, "emulator/no_such_thing", &json!({}));
        let e = result.expect_err("an unknown method is refused, not answered");
        assert_eq!(e.code, crate::rpc::code::METHOD_NOT_FOUND);
    }

    /// **`call` applies neither deferred run-state change** — the decision in its own doc comment, pinned
    /// so a later "tidy-up" that copies `pump`'s preamble into it fails here rather than in the field.
    ///
    /// Both latches must still be sitting there afterwards, and the drain must still be the thing that
    /// takes them.
    #[test]
    fn call_leaves_the_deferred_run_state_changes_for_the_drain() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();

        // A fresh engine is not free-running, so it is the *un*-pause that queues a change; `set_paused`
        // compares against the engine's own state and records nothing when they already agree.
        h.set_paused(false);
        h.record_break(0x0000_1234);
        assert_eq!(
            h.pending_free_run,
            Some(true),
            "the fixture queued a pause change"
        );
        assert_eq!(h.pending_break, Some(0x0000_1234), "and a halt");

        let (result, _) = h.call(&mut sys, "emulator/registers", &json!({}));
        result.expect("the call itself succeeds");

        assert_eq!(
            h.pending_free_run,
            Some(true),
            "a call must not consume the deferred pause change — painting is not run control"
        );
        assert_eq!(
            h.pending_break,
            Some(0x0000_1234),
            "nor the latched halt: the pair's ordering is argued at exactly one site, and `pump` is it"
        );

        h.pump(&mut sys);
        assert_eq!(h.pending_free_run, None, "the drain is what takes them");
        assert_eq!(h.pending_break, None);
    }

    /// An unapplied halt is never replaced by a later one.
    ///
    /// Today exactly one frame runs between a latch and the drain that takes it, so a second cannot
    /// arrive — but the *earlier* halt is the one that stopped the machine, and silently overwriting it
    /// would report the wrong address for the stop. Pinned so the "keep the first" reading is a property
    /// rather than an accident of `Option::get_or_insert`.
    #[test]
    fn an_unapplied_halt_is_not_overwritten_by_a_later_one() {
        let mut h = Host::new(HostConfig::default());
        assert_eq!(h.pending_break, None, "a fresh host has nothing latched");
        h.record_break(0x0000_1234);
        h.record_break(0x0000_5678);
        assert_eq!(
            h.pending_break,
            Some(0x0000_1234),
            "the halt that stopped the machine is the first one, not the last"
        );
    }

    /// **A reply that changed the run state must not reach a client before the stamp says so.**
    ///
    /// This is the mechanism behind `emulator/wait_for_break`'s instant-timeout defect, tested at the seam
    /// where it lives. The connection thread does not ask the engine whether the machine is running — it
    /// polls [`crate::server::SharedStamp`], the snapshot this drain publishes. So the moment a client can
    /// send its *next* request is the moment that snapshot has to be true, and that moment is
    /// `reply.send`, not the end of the drain: a client holding the reply to `emulator/resume` can put
    /// `emulator/wait_for_break` on the wire immediately, and a stamp still carrying the previous halt's
    /// `running: false` makes the wait exit after ~0 ms and be answered `{"timeoutReached": true}`.
    ///
    /// `server::engine_loop` has always published before its reply; this drain is the second run driver,
    /// and it did not.
    ///
    /// **How this reads the ordering.** A recorder thread blocks on the reply channel and samples the
    /// published run state the instant it is woken — the mpsc channel is the happens-before edge, so a
    /// stamp published before the send is *guaranteed* visible and the green direction has no race in it.
    /// The red direction needs the sample to land before the end-of-drain publication, so the second
    /// queued command is a 120-frame run: hundreds of milliseconds of window in which the old ordering is
    /// caught. `pump_budget` is raised for the same reason — the default 4 ms would defer that second
    /// command to the next drain and close the window.
    ///
    /// Planting the defect: delete the `self.publish_stamp()` call above `reply.send` in
    /// [`Host::pump`]. The recorder then samples `true` — the run state from before the `pause` it is
    /// holding the reply to.
    #[test]
    fn a_state_changing_reply_is_not_sent_before_the_stamp_says_so() {
        let mut h = Host::new(HostConfig {
            // The window this test needs is the second command's duration; the default 4 ms budget would
            // defer it to the next drain and there would be no window at all.
            pump_budget: Duration::from_secs(60),
            ..HostConfig::default()
        });
        let mut sys = booted();

        // Get the bus free-running and *published* as such, so the stale reading the defect produces is a
        // real previous value rather than the never-written default.
        h.set_paused(false);
        h.pump(&mut sys);
        assert!(
            h.ctx.shared.is_running(),
            "the arrangement: the published stamp must say the machine is running before the pause \
             below, or a stale reading is indistinguishable from the default"
        );

        // Two commands in one drain. The first changes the run state and is the one whose reply we watch;
        // the second is only there to hold the drain open long enough for a late publication to be seen.
        let (tx_pause, rx_pause) = mpsc::channel();
        h.tx.send(EngineMsg::Call {
            method: "emulator/pause".into(),
            params: json!({}),
            reply: tx_pause,
        })
        .expect("queue the pause");
        let (tx_run, _rx_run) = mpsc::channel();
        h.tx.send(EngineMsg::Call {
            method: "emulator/run_frames".into(),
            params: json!({"frames": HOSTED_MAX_RUN_FRAMES}),
            reply: tx_run,
        })
        .expect("queue the run");

        let stamp = Arc::clone(&h.ctx.shared);
        let recorder = std::thread::spawn(move || {
            let r = rx_pause.recv().expect("the drain answered the pause");
            // Sampled the instant the reply arrives — which is the instant a real client could send its
            // next request.
            (r.result.is_ok(), stamp.is_running())
        });

        h.pump(&mut sys);
        let (ok, seen_running) = recorder.join().expect("recorder");

        assert!(ok, "emulator/pause must succeed on a free-running bus");
        assert!(
            !seen_running,
            "the reply to `emulator/pause` reached the client while the published stamp still said \
             `running: true`. A client that sends `emulator/wait_for_break` on the strength of this \
             reply has its wait decided by a snapshot from before the state change — which is exactly \
             how a ten-second wait comes back `timeoutReached` in under a millisecond. Publish before \
             the reply, as `server::engine_loop` does."
        );
        assert!(
            h.is_paused(),
            "and the pause really landed, so the reading above is about ordering and not about a \
             no-op"
        );
    }
}
