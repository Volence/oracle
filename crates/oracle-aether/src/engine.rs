//! The emulator side of the bus: one [`Engine`] owning one [`System`], plus the **method table that is
//! simultaneously the dispatch table and the advertised method list**.
//!
//! # The generated method list (D4)
//!
//! `protocol.md` D4 replaces the sibling's hand-maintained `list_ops` — which advertised 34 of 47 real
//! ops and was already stale — with a self-describing surface, so *"the 47-vs-34 drift becomes
//! structurally impossible"*. Here that is literal: [`METHODS`] holds the function pointers, dispatch is
//! a lookup in it, and `initialize` reports its names. There is no second list to fall out of sync with,
//! in either direction.
//!
//! # Scope
//!
//! A deliberately thin subset of the 53-method catalog (`protocol.md` §6), chosen so that **every method
//! here appears in the catalog verbatim** — §8 forbids inventing ops. Adding the rest later is a new row
//! in [`METHODS`] and therefore a capability-flag change, not a breaking one. What is *not* implemented,
//! and the two places the contract could not be followed without a change request, are recorded in
//! `docs/2026-08-14-aether-change-requests.md`.
//!
//! The four **checkpoint** methods (§6.1, D13) are *additional to* that 53: the 2026-08-14 amendment adds
//! them to the catalog, and per D4/D5 a server that does not implement them simply does not advertise
//! them. They are advertised here, so `capabilities.checkpoints` carries the cap a client has to plan
//! around.

use crate::hex;
use crate::outbound::Subscribers;
use crate::rpc::{self, code, RpcError};
use oracle_core::bus::{Fanout, StopWhen};
use oracle_core::io::Pad;
use oracle_core::render::{CandidateVerdict, Layer, PixelState};
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::symbols::{BindingFault, RomBinding, SymbolTable};
use oracle_core::system::{System, MCLK_PER_FRAME, RAM_SIZE};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Work RAM's decode window on the 68000's 24-bit bus (`bus.rs`: `0xE0_0000..=0xFF_FFFF`, mirrored every
/// 64 KiB). The listing's 32-bit `FFFFxxxx` spelling of a RAM address masks into here — the trap recorded
/// as recon §9a.
const WORK_RAM_LO: u32 = 0x00E0_0000;
const WORK_RAM_HI: u32 = 0x00FF_FFFF;
/// The 68000 drives 24 address lines; anything above this cannot be a bus address.
const BUS_ADDR_MAX: u32 = 0x00FF_FFFF;
/// Active display height in lines (the region `render_line` covers).
const ACTIVE_LINES: u16 = 224;

/// Tunables. Every bound here is a **loud refusal** when exceeded, never a silent clamp: a clamped `len`
/// returns fewer bytes than asked for and the caller has no way to notice.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    /// Ceiling for one `emulator/run_frames` / `emulator/run_to` call. A single request must not be able
    /// to monopolise the emulator thread for an unbounded time — every other client is queued behind it.
    pub max_run_frames: u64,
    /// Ceiling for one memory/VRAM read, per `protocol.md` §6 (`len`? ≤ 4096).
    pub max_read_len: u64,
    /// Ceiling on `otherMatches` in a symbol lookup, per `protocol.md` §4 ("up to 5").
    pub max_symbol_matches: usize,
    /// **The checkpoint cap** (`protocol.md` §6.1, D13 rule 3), advertised in `initialize` as
    /// `capabilities.checkpoints.cap`. At the cap `emulator/checkpoint` **refuses** with `-32005`; it
    /// never evicts the oldest, because an id a client is still holding must never quietly start meaning
    /// nothing. Doubles as the default and maximum page size for `emulator/checkpoint_list` — there can
    /// never be more live checkpoints than this, so a bigger page could not return more.
    pub max_checkpoints: usize,
    /// Wall-clock pacing for free-running mode, or `None` to run flat out. **Pacing only** — it never
    /// touches an emulated stamp, so determinism is unaffected (recon §5 C2). Tests use `None`.
    pub free_run_pace: Option<Duration>,
    pub server_name: String,
    pub server_version: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_run_frames: 3600,
            max_read_len: 4096,
            max_symbol_matches: 5,
            // The contract's own advertised example (`"checkpoints":{"supported":true,"cap":8}`). A
            // snapshot is the whole machine, so the cap is a memory bound as much as a policy one.
            max_checkpoints: 8,
            free_run_pace: Some(Duration::from_micros(16_667)),
            server_name: "oracle-next".into(),
            server_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// One method: its wire name, its handler, and a one-line summary reported by `initialize`.
pub struct MethodSpec {
    pub name: &'static str,
    pub handler: fn(&mut Engine, &Value) -> Result<Value, RpcError>,
    pub summary: &'static str,
}

/// **The dispatch table and the advertised method list, as one object.** Every name here is a
/// `protocol.md` §6 catalog entry verbatim.
pub const METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: "emulator/status",
        handler: Engine::status,
        summary: "run state, PC/SP/SR, symbol at PC, loaded ROM",
    },
    MethodSpec {
        name: "emulator/registers",
        handler: Engine::registers,
        summary: "the 68000 architectural register file",
    },
    MethodSpec {
        name: "emulator/run_frames",
        handler: Engine::run_frames,
        summary: "advance N whole frames, then stop (emits resumed + stopped)",
    },
    MethodSpec {
        name: "emulator/run_to",
        handler: Engine::run_to,
        summary: "run until PC reaches an address or symbol, bounded (emits resumed + stopped)",
    },
    MethodSpec {
        name: "emulator/pause",
        handler: Engine::pause,
        summary: "leave free-running mode (emits stopped)",
    },
    MethodSpec {
        name: "emulator/resume",
        handler: Engine::resume,
        summary: "enter free-running mode (emits resumed)",
    },
    MethodSpec {
        name: "emulator/checkpoint",
        handler: Engine::checkpoint,
        summary: "capture the whole machine into a volatile in-memory slot and return its server-assigned id",
    },
    MethodSpec {
        name: "emulator/restore",
        handler: Engine::restore,
        summary: "restore the entire machine, ROM included, from a checkpoint",
    },
    MethodSpec {
        name: "emulator/checkpoint_list",
        handler: Engine::checkpoint_list,
        summary: "the live checkpoints, bounded and cursored",
    },
    MethodSpec {
        name: "emulator/checkpoint_drop",
        handler: Engine::checkpoint_drop,
        summary: "drop one checkpoint by id, or all of them, and report how many went",
    },
    MethodSpec {
        name: "emulator/read_memory",
        handler: Engine::read_memory,
        summary: "debug read of ROM or work RAM by address or symbol",
    },
    MethodSpec {
        name: "emulator/read_vram",
        handler: Engine::read_vram,
        summary: "debug read of VDP VRAM",
    },
    MethodSpec {
        name: "emulator/pixel_attribution",
        handler: Engine::pixel_attribution,
        summary: "why the dot at (x,y) is the colour it is: winner, cell/sprite, and the losing candidates",
    },
    MethodSpec {
        name: "emulator/state_hash",
        handler: Engine::state_hash,
        summary: "FNV-1a fingerprints of the VDP state regions",
    },
    MethodSpec {
        name: "emulator/screenshot",
        handler: Engine::screenshot,
        summary: "render the active display to a binary PPM file",
    },
    MethodSpec {
        name: "emulator/press",
        handler: Engine::press,
        summary: "tap buttons for N frames, then restore the held set",
    },
    MethodSpec {
        name: "emulator/hold",
        handler: Engine::hold,
        summary: "set or clear buttons in the held set (set semantics, never additive)",
    },
    MethodSpec {
        name: "emulator/release_all",
        handler: Engine::release_all,
        summary: "clear the held set on both pads",
    },
    MethodSpec {
        name: "emulator/lookup_symbol",
        handler: Engine::lookup_symbol,
        summary: "name -> address, or address -> nearest preceding label + displacement",
    },
    MethodSpec {
        name: "emulator/load_symbols",
        handler: Engine::load_symbols,
        summary: "load a sigil/AS .lst listing, refusing one that does not bind to the loaded ROM",
    },
    MethodSpec {
        name: "emulator/reload_rom",
        handler: Engine::reload_rom,
        summary: "reload the ROM from disk and reset (emits romReloaded)",
    },
];

/// The events this server actually emits. Advertised verbatim as `capabilities.events`, which
/// `protocol.md` §2.1 calls *"the authoritative event set"* — so it lists what we push, not what the
/// spec's example happens to show.
pub const EVENTS: &[&str] = &[
    "emulator/stopped",
    "emulator/resumed",
    "emulator/romReloaded",
];

/// One live checkpoint: a **whole machine**, a server-assigned id, and the coordinate it was taken at
/// (`protocol.md` §6.1, D13).
///
/// **Volatility is structural, not a policy someone has to remember** (D13 rule 1): the bytes live in
/// this `Vec` and there is no code path from here to the filesystem. The snapshot is a bincode encoding
/// of the live `System` struct and is version-fragile across builds *by design* — writing it to a file
/// would promise a durability the format does not have, so no "save to file" variant of these methods
/// exists and none may be added. Persistent save-states are a separate versioned artifact and a separate
/// change request.
///
/// Ids are assigned from a monotonic counter and **never reused**, including after a drop. Two clients
/// share one bus and one set of coordinates; a recycled id would let one client's `restore` silently
/// land on another client's machine.
///
/// **The counter stays a `u64` here; only the wire is a string** (D9 category 4, §6.1, §8 item 16). §6.1
/// blesses exactly this: typing the id as a string "does not cost a server the monotonic-counter
/// technique … it may keep ids in any order it likes internally, including a counter it formats as a
/// decimal string — because the client never compares two handles." The ordering is load-bearing
/// *internally* — it is what makes `checkpoint_list`'s cursor a resume point ("the first id strictly
/// greater than this") — and that machinery is untouched by the wire type. [`Checkpoint::wire_id`] is
/// the one place the two representations meet.
struct Checkpoint {
    id: u64,
    /// Carried back verbatim by `checkpoint_list` and never interpreted (§6.1).
    label: Option<String>,
    frame: u64,
    mclk: u64,
    /// bincode of the entire `System` — CPU, RAM, VDP, sound state **and the ROM** (D13 rule 2).
    snapshot: Vec<u8>,
    /// The engine-side shadows of machine state, captured with the machine so a restore cannot leave
    /// them describing a cartridge and a pad set that are no longer loaded. They are not extra state:
    /// [`Engine::held`] mirrors the pads inside `System`, `rom_path` names the image inside it, and the
    /// symbol table is a listing bound to *that* image. A restore that brought the ROM back but left
    /// `status` reporting the *other* image's path, or resolving names against the *other* image's
    /// listing, would be exactly the half-applied restore D13 rule 2 forbids.
    ///
    /// The symbol table rides along for D7's reason specifically: stale symbol resolution is this bus's
    /// named hazard ("the 'verified' literal went stale within the session"), and a `read_memory
    /// {symbol}` answered from the wrong cartridge's table reads a wrong address and reports success.
    /// [`Engine::reload_rom`] already drops a table that stops binding to the loaded image; a `restore`
    /// that did neither would be strictly weaker than `reload_rom` for the same cartridge transition.
    ///
    /// The table is an [`Arc`] so a capture is a refcount bump rather than a copy of every symbol — at
    /// the cap that is the difference between one listing in memory and one per slot.
    held: [Pad; 2],
    rom_path: Option<String>,
    symbols: Option<Arc<SymbolTable>>,
    symbols_path: Option<String>,
}

impl Checkpoint {
    /// This checkpoint's id **as it goes on the wire**: the internal counter formatted as a decimal
    /// string (§6.1, D9 category 4). Every wire position — `checkpoint`'s result, `checkpoint_list`'s
    /// entries, and the comparison an incoming `restore`/`checkpoint_drop` id is matched against — goes
    /// through here, so there is exactly one definition of the mapping and no way for two of them to
    /// drift apart. The decimal spelling is an implementation detail a client MUST NOT rely on; it is
    /// chosen only because it is the cheapest formatting of the counter that already exists.
    fn wire_id(&self) -> String {
        self.id.to_string()
    }
}

/// The emulator and everything the bus knows about it. Lives on exactly one thread (the core is
/// single-threaded and `System` is plain owned data); every connection reaches it through a channel.
pub struct Engine {
    sys: System,
    config: EngineConfig,
    subs: Subscribers,
    /// Shared, never mutated in place: a checkpoint captures the table by refcount (see [`Checkpoint`]).
    symbols: Option<Arc<SymbolTable>>,
    symbols_path: Option<String>,
    rom_path: Option<String>,
    /// **Free-running mode** — `emulator/resume` sets it, `emulator/pause` clears it. This is the *mode*
    /// question ("is the server advancing the machine on its own?"), and it is what
    /// [`require_paused`](Engine::require_paused) gates on.
    free_run: bool,
    /// **Is the machine advancing right now** — free-run, *or* inside a bounded `run_frames`/`run_to`/
    /// `press`. This is what the `running` field of every reply and event reports.
    ///
    /// The two are separate because collapsing them makes the event stream lie: a bounded run emits
    /// `emulator/resumed`, and with one flag that event would carry `running: false` — a client reading
    /// "resumed" and "not running" in the same message has been told two contradictory things about the
    /// same instant.
    running: bool,
    /// The **held** pad state per port, owned here rather than read back out of `System`, so `press` can
    /// restore exactly what was held before it and `hold` has unambiguous set-not-add semantics (the
    /// sibling's *"`hold` ADDS, it does not replace"* defect, recon §1c).
    held: [Pad; 2],
    /// The **live host** pad state per port — what a human is physically holding on the keyboard or a
    /// gamepad *right now*, published by whatever owns the machine (the player process; nothing at all in
    /// the standalone server, where it stays all-released and every pad expression below is therefore
    /// byte-identical to what it was before this field existed).
    ///
    /// It is kept apart from [`held`](Engine::held) because the two answer different questions and only one
    /// of them is the bus's to own: `held` is *the client's* held set, with the set-not-add semantics
    /// `emulator/hold` promises, and it must stay exactly what the client last asked for. Merging the human's
    /// live input into `held` would make `emulator/hold`'s reply — and `Engine::held` itself — report buttons
    /// no client ever pressed.
    live: [Pad; 2],
    /// The live checkpoints (§6.1, D13). In memory, per **server session**, capped by
    /// [`EngineConfig::max_checkpoints`], and owned by the engine rather than by a connection — the
    /// coordinates belong to the machine, so two clients on one bus see one set.
    checkpoints: Vec<Checkpoint>,
    /// Monotonic id source. Never rewound, never reused (see [`Checkpoint`]).
    next_checkpoint_id: u64,
    /// **The screen path.** Attached to every run this engine performs, so the picture a client asks for is
    /// the one the raster actually drew — see [`Engine::framebuffer`] for why a post-hoc render cannot be.
    screen: ScanlineCapture,
    /// The most recently completed frame, latched out of [`screen`](Engine::screen) after a run — or
    /// published from outside by [`publish_capture`](Engine::publish_capture) when something else owns the
    /// run loop. `None` until a whole frame has been drawn.
    last_frame: Option<CapturedFrame>,
    /// Bumped whenever [`last_frame`](Engine::last_frame) is replaced **by this engine's own run**. A host
    /// that drives the run loop itself watches this to learn that a client-driven run has moved the picture
    /// on underneath it; a frame it published itself does not bump it, because the publisher already has it.
    screen_generation: u64,
    /// Bumped whenever the cartridge in the machine is replaced — `emulator/reload_rom`, or an
    /// `emulator/restore` that brought a different image back (D13 rule 2). A host that derives anything
    /// from the ROM bytes (a save-state fingerprint, a symbol listing) watches this so it cannot keep
    /// describing a cartridge that is no longer loaded.
    rom_generation: u64,
}

/// One pixel, in the core's own line-delivery spelling.
pub type Rgb = (u8, u8, u8);

/// One whole frame of active display, line-major, exactly as the raster drew it.
#[derive(Clone, Debug)]
struct CapturedFrame {
    width: usize,
    rgb: Vec<Rgb>,
}

/// A borrowed frame: its width, and its pixels line-major. Height is always [`ACTIVE_LINES`].
pub type FrameRef<'a> = (usize, &'a [Rgb]);

impl Engine {
    pub fn new(sys: System, config: EngineConfig, subs: Subscribers) -> Self {
        Self {
            sys,
            config,
            subs,
            symbols: None,
            symbols_path: None,
            rom_path: None,
            free_run: false,
            running: false,
            held: [Pad::default(); 2],
            live: [Pad::default(); 2],
            checkpoints: Vec::new(),
            next_checkpoint_id: 1,
            screen: ScanlineCapture::new(Retain::LastFrame),
            last_frame: None,
            screen_generation: 0,
            rom_generation: 0,
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Whether the server loop should keep stepping the machine on its own (free-run mode).
    pub fn is_running(&self) -> bool {
        self.free_run
    }

    /// **Lend the machine to the engine, or take it back.** The whole of the hosted arrangement rests on
    /// this one call: the player process owns the `System` and swaps it in for the duration of a command
    /// drain, so every handler below runs against the real machine and stamps the real clocks (D11), then
    /// swaps it straight back out.
    ///
    /// A swap, rather than a borrow, because `Engine` also owns state that must survive between drains — the
    /// held set, the checkpoints, the symbol table, the free-run flag — and a borrowed-`System` engine would
    /// have to be rebuilt (and that state re-threaded) on every iteration. `System` is 1,152 bytes of struct
    /// (every large region is behind a `Vec`), so the exchange is two ~1 KB moves; it is not a copy of the
    /// machine.
    ///
    /// Between drains the engine holds an inert placeholder. That is why nothing outside a drain window may
    /// call a handler or [`stamp`](Engine::stamp) — it would answer for the placeholder. The host enforces
    /// this by doing every such thing inside the swapped window.
    pub fn swap_system(&mut self, other: &mut System) {
        std::mem::swap(&mut self.sys, other);
    }

    /// The buttons a client is holding on `port` (`emulator/hold`). A host merges these with the human's
    /// live input before it writes the pad — see [`set_live_pads`](Engine::set_live_pads).
    pub fn held(&self, port: usize) -> Pad {
        self.held[port & 1]
    }

    /// Publish what the human is physically holding, so the engine's own pad writes (`hold`, `press`,
    /// `release_all`) compose with live input instead of erasing it. Pure state: it never touches the
    /// machine, so it is safe to call outside a drain window.
    pub fn set_live_pads(&mut self, pads: [Pad; 2]) {
        self.live = pads;
    }

    /// Free-run mode, set from outside. **Hosted, this is the player's own pause state**: while the player is
    /// advancing the machine, the bus is free-running by definition, and `protocol.md` §6's run-control state
    /// rule then requires `run_frames`/`run_to`/`step*` to be refused with `-32005 machineRunning` — which is
    /// exactly what [`require_paused`](Engine::require_paused) already does.
    ///
    /// Emits the same `emulator/stopped` / `emulator/resumed` a client-driven `pause`/`resume` would, because
    /// from a subscriber's point of view nothing distinguishes them: the machine stopped, or it started.
    /// **Must be called inside a drain window** — the events it emits carry the machine stamp.
    pub fn set_free_run(&mut self, on: bool) -> bool {
        let was = self.free_run;
        if was == on {
            return was;
        }
        self.free_run = on;
        self.running = on;
        if on {
            self.emit_resumed();
        } else {
            self.emit_stopped("pause", self.sys.cpu_regs().pc, Map::new());
        }
        was
    }

    /// The machine's master clock. Only meaningful while the real machine is swapped in.
    pub fn mclk(&self) -> u64 {
        self.sys.scheduler().now()
    }

    /// The latched frame — line-major RGB and its width — or `None` before any whole frame has been drawn.
    pub fn latched_frame(&self) -> Option<FrameRef<'_>> {
        self.last_frame.as_ref().map(|f| (f.width, &f.rgb[..]))
    }

    /// How many times this engine's **own** runs have replaced the latched frame (see
    /// [`Engine::last_frame`]).
    pub fn screen_generation(&self) -> u64 {
        self.screen_generation
    }

    /// How many times the cartridge has been replaced (see [`Engine::rom_generation`]).
    pub fn rom_generation(&self) -> u64 {
        self.rom_generation
    }

    /// Hand the engine a frame drawn by somebody else's run loop, so `emulator/screenshot` and
    /// `emulator/state_hash {includeFramebuffer}` answer with the picture that is actually on the glass.
    ///
    /// Takes the [`ScanlineCapture`] rather than a packed buffer on purpose: it is the same input
    /// [`latch_screen`](Engine::latch_screen) consumes, run through the same reader, so a published frame and
    /// a client-driven one cannot disagree about geometry, ragged mode switches or which frame is the
    /// complete one. Returns whether a whole frame was found and taken.
    ///
    /// Deliberately does **not** bump [`screen_generation`](Engine::screen_generation): the publisher already
    /// has this image, and a bump means "the picture moved without you".
    pub fn publish_capture(&mut self, cap: &ScanlineCapture) -> bool {
        store_from_capture(&mut self.last_frame, cap)
    }

    /// Attach the ROM's own path so `emulator/reload_rom` and `emulator/status` can name it.
    pub fn set_rom_path(&mut self, path: Option<String>) {
        self.rom_path = path;
    }

    /// Install an already-parsed symbol table (the binary does this at startup from the `.lst` beside
    /// the ROM). The binding check is the caller's — see [`Engine::load_symbols`] for the wire path.
    pub fn set_symbols(&mut self, table: Option<SymbolTable>, path: Option<String>) {
        self.symbols = table.map(Arc::new);
        self.symbols_path = path;
    }

    /// One free-running frame, called by the server loop between command drains. Returns the pacing
    /// interval to sleep for, if any.
    pub fn free_run_step(&mut self) -> Option<Duration> {
        self.advance(1);
        self.config.free_run_pace
    }

    // ---------------------------------------------------------------- the screen path

    /// Advance the machine `frames` whole frames **through the screen capture**, then latch whatever frame
    /// the run completed.
    ///
    /// Every advancing path in this engine goes through here or through [`advance_until`](Engine::advance_until)
    /// — that is what makes `emulator/screenshot` scanline-accurate rather than a post-hoc guess.
    fn advance(&mut self, frames: u64) {
        self.sys.run_frames_with_sink(frames, &mut self.screen);
        self.latch_screen();
    }

    /// [`advance`](Engine::advance) with a stop predicate: the capture and the predicate ride the same run as
    /// a [`Fanout`], which is precisely what `System::run_until_stop` does internally with the predicate
    /// alone.
    fn advance_until<F: FnMut(u32, u64) -> bool>(
        &mut self,
        max_frames: u64,
        predicate: F,
    ) -> oracle_core::system::StopRecord {
        let mut stop = StopWhen::new(predicate);
        let mut sink = Fanout::new(&mut self.screen, &mut stop);
        let record = self.sys.run_frames_with_sink(max_frames, &mut sink);
        self.latch_screen();
        record
    }

    /// Take the completed frame out of the capture and release the capture's bookkeeping.
    ///
    /// The release is not optional: `ScanlineCapture`'s per-delivery log grows ~215 KB per emulated second
    /// under every retention policy and is bounded only by run length, so a long-lived capture on a
    /// free-running machine is an unbounded leak. `clear` drops the latched pixels too, which is why the
    /// frame is copied out first.
    fn latch_screen(&mut self) {
        if store_from_capture(&mut self.last_frame, &self.screen) {
            self.screen_generation += 1;
        }
        self.screen.clear();
    }

    /// Forget the picture entirely — the machine under it has been replaced. Also drops the capture's
    /// half-built frame, which would otherwise be spliced onto the next machine's first lines.
    fn invalidate_screen(&mut self) {
        self.last_frame = None;
        self.screen.clear();
        self.screen_generation += 1;
    }

    /// Write the pads the machine should see: **the client's held set OR the human's live input**, per port.
    ///
    /// The merge is the answer to "who owns the pad" when a client and a person share one machine. Either
    /// alternative loses information a caller cannot get back — a client-wins rule makes `emulator/hold` a
    /// silent input lockout, and a human-wins rule makes `hold` a no-op the moment anyone touches the
    /// keyboard — whereas OR is what the player already does between its own two input sources (keyboard and
    /// gamepad merge per button, so neither can suppress the other). In the standalone server `live` is
    /// all-released and this is byte-identical to writing `held` directly.
    fn apply_pads(&mut self) {
        for port in 0..2 {
            self.sys
                .set_pad(port, merge_pads(self.live[port], self.held[port]));
        }
    }

    // ---------------------------------------------------------------- dispatch

    /// Look a method up in [`METHODS`]. This is the *only* dispatch path, which is what makes the
    /// advertised list and the implemented set the same set by construction.
    pub fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        let Some(spec) = METHODS.iter().find(|m| m.name == method) else {
            return Err(
                RpcError::new(code::METHOD_NOT_FOUND, format!("no such method: {method}"))
                    .with_data(json!({"method": method})),
            );
        };
        (spec.handler)(self, params)
    }

    /// The deterministic, **emulated** machine coordinate carried by every reply, error and event.
    pub fn stamp(&self) -> Map<String, Value> {
        let mclk = self.sys.scheduler().now();
        rpc::stamp_object(mclk / MCLK_PER_FRAME, mclk, self.running)
    }

    // ---------------------------------------------------------------- handshake

    /// Build the `initialize` result: protocol version, capability flags, and the generated method list.
    pub fn initialize_result(&self, params: &Value) -> Result<Value, RpcError> {
        // Version negotiation (§2.1): a mismatch is -32015 carrying the versions we do support, never a
        // best-effort attempt to serve a protocol we do not implement.
        if let Some(v) = params.get("protocolVersion") {
            let asked = v.as_i64().ok_or_else(|| {
                RpcError::invalid_params("`protocolVersion` must be an integer (D5)")
            })?;
            if asked != rpc::PROTOCOL_VERSION {
                return Err(RpcError::new(
                    code::UNSUPPORTED_PROTOCOL_VERSION,
                    format!(
                        "this server speaks protocol version {}",
                        rpc::PROTOCOL_VERSION
                    ),
                )
                .with_data(json!({"supported": [rpc::PROTOCOL_VERSION], "requested": asked})));
            }
        }
        let methods: Vec<Value> = METHODS.iter().map(|m| json!(m.name)).collect();
        let method_docs: Map<String, Value> = METHODS
            .iter()
            .map(|m| (m.name.to_string(), json!(m.summary)))
            .collect();
        Ok(json!({
            "serverName": self.config.server_name,
            "serverVersion": self.config.server_version,
            "protocolVersion": rpc::PROTOCOL_VERSION,
            "capabilities": {
                // The authoritative event set (D6) — exactly what this server pushes.
                "events": EVENTS,
                // Method groups from the catalog that this thin slice does NOT implement. Clients branch
                // on these, never on the version integer (D5).
                "z80": false,
                "vgm": false,
                "objectDecoders": false,
                "profiler": false,
                "breakpoints": false,
                "watchpoints": false,
                "batch": false,
                // A 6-button pad is not modelled by the core, so x/y/z/mode are refused rather than
                // silently ignored.
                "sixButtonPad": false,
                // §6.1: an object rather than a bare boolean, because D13 requires the cap to be
                // discoverable *before* a client plans around it — a client that has to hit the limit
                // to learn it is a client that loses a checkpoint finding out. No `maxBytes`: this
                // server caps the count only, and advertising a byte ceiling it does not enforce would
                // be worse than omitting the optional key.
                "checkpoints": {
                    "supported": true,
                    "cap": self.config.max_checkpoints,
                },
                "symbolsLoaded": self.symbols.is_some(),
                "romLoaded": !self.sys.rom().is_empty(),
                // The three recon §4 non-negotiables, advertised so a client can assert them.
                "stampedReplies": true,
                "boundedArrays": true,
                "caveats": true,
            },
            "methods": methods,
            "methodSummaries": Value::Object(method_docs),
            "limits": {
                "maxRunFrames": self.config.max_run_frames,
                "maxReadLen": self.config.max_read_len,
                "maxLineBytes": rpc::MAX_LINE_BYTES,
            },
            // What the `frame` in every stamp actually *means* (`F-TRACE-PAL`). Advertised once, here,
            // rather than repeated on every reply: it is a property of the machine, not of the answer.
            // A client that caches frame coordinates across sessions can record the basis with them; a
            // client that ignores it was NTSC-only anyway. Constant while the core is NTSC-only — it
            // becomes a live value when PAL lands and this key does not change shape.
            "timingBasis": rpc::timing_basis_object(self.sys.timing_basis()),
        }))
    }

    // ---------------------------------------------------------------- events

    fn emit(&self, method: &str, mut params: Map<String, Value>) {
        for (k, v) in self.stamp() {
            params.insert(k, v);
        }
        self.subs
            .broadcast(&rpc::notification(method, Value::Object(params)));
    }

    fn emit_stopped(&self, reason: &str, pc: u32, extra: Map<String, Value>) {
        let mut params = extra;
        params.insert("reason".into(), json!(reason));
        params.insert("pc".into(), json!(hex::addr(pc)));
        if let Some((name, disp)) = self.symbol_at(pc) {
            params.insert("symbol".into(), json!(name));
            params.insert("symbolDisp".into(), json!(disp));
        }
        self.emit("emulator/stopped", params);
    }

    fn emit_resumed(&self) {
        self.emit("emulator/resumed", Map::new());
    }

    // ---------------------------------------------------------------- helpers

    /// Nearest preceding symbol for an address, plus displacement. `None` when no table is loaded or the
    /// address precedes every symbol in its address space.
    fn symbol_at(&self, addr: u32) -> Option<(String, u32)> {
        let table = self.symbols.as_ref()?;
        let r = table.resolve(addr)?;
        Some((r.to_string(), r.displacement))
    }

    /// Resolve an `addr`-or-`symbol` parameter pair. Symbol-first addressing is D7: clients resolve,
    /// they never hardcode a RAM literal — the contract records a session in which a "verified" literal
    /// went stale within the session because a 36-byte insertion slid the whole RAM block by +$24.
    fn resolve_target(&self, params: &Value) -> Result<u32, RpcError> {
        if let Some(name) = params.get("symbol") {
            let Some(name) = name.as_str() else {
                return Err(RpcError::invalid_params("`symbol` must be a string"));
            };
            let table = self.symbols.as_ref().ok_or_else(no_symbols)?;
            return table.address_of(name).ok_or_else(|| {
                RpcError::new(code::SYMBOL_NOT_FOUND, format!("no symbol named {name}"))
                    .with_data(json!({"symbol": name}))
            });
        }
        let Some(a) = params.get("addr") else {
            return Err(RpcError::invalid_params(
                "one of `addr` (hex string) or `symbol` (string) is required",
            ));
        };
        let addr = hex::parse_addr("addr", a)?;
        if addr > BUS_ADDR_MAX {
            return Err(out_of_range(addr, "the 68000 bus is 24 bits wide"));
        }
        Ok(addr)
    }

    /// A **debug** read straight out of the region, deliberately bypassing the bus.
    ///
    /// Bypassing is the right call for an inspection API (no side effects, no open-bus latch churn, no
    /// FIFO), but it means the value can differ from what a CPU read at the same address would return —
    /// so every reply built on this carries a `caveat` saying so. That is exactly the landmine the recon
    /// found in the sibling's `write_vram`, which bypasses the VDP port path and *"nothing in its
    /// docstring says so"*.
    fn debug_read(&self, addr: u32, len: usize) -> Result<(Vec<u8>, &'static str), RpcError> {
        let end = (addr as u64) + (len as u64) - 1;
        if (WORK_RAM_LO..=WORK_RAM_HI).contains(&addr) {
            if end > u64::from(WORK_RAM_HI) {
                return Err(out_of_range(
                    addr,
                    "the read would run past the end of work RAM",
                ));
            }
            let ram = self.sys.ram();
            let out = (0..len)
                .map(|i| ram[((addr as usize).wrapping_add(i)) & (RAM_SIZE - 1)])
                .collect();
            return Ok((out, "work RAM"));
        }
        let rom = self.sys.rom();
        if (addr as usize) < rom.len() {
            if end >= rom.len() as u64 {
                return Err(out_of_range(
                    addr,
                    "the read would run past the end of the ROM image",
                ));
            }
            return Ok((
                rom[addr as usize..addr as usize + len].to_vec(),
                "cartridge ROM",
            ));
        }
        Err(out_of_range(
            addr,
            "only cartridge ROM ($000000..rom_len) and work RAM ($E00000-$FFFFFF) are readable in this slice",
        ))
    }

    /// The active display as a row-major RGB framebuffer, its width, and **whether it is the frame the
    /// raster actually drew**.
    ///
    /// The latched frame is preferred, and it is preferred for a reason that was measured rather than
    /// argued: the fallback below re-renders every line out of the VDP state as it stands *now*, and a run
    /// ends in V-Blank, by which point a game has already rewritten CRAM for the next frame. Every mid-frame
    /// palette effect is therefore invisible to it — S3K's underwater split comes out in the above-water
    /// palette, bright red instead of slate blue — and the window hit exactly this bug over 6 of 17
    /// conformance ROMs (`docs/2026-08-15-scanline-golden-coverage.md`) before it was fixed the same way.
    ///
    /// The fallback is still reached, and is still the honest answer when it is: before the first whole frame
    /// has been drawn (a machine that has been reset but not run) there is no raster output to show, and a
    /// post-hoc render of the reset state is better than a black rectangle. Callers report which one they got
    /// — see the `caveat` on `emulator/screenshot`.
    fn framebuffer(&self) -> (usize, Vec<Rgb>, bool) {
        if let Some(f) = &self.last_frame {
            if f.width > 0 && f.rgb.len() == f.width * ACTIVE_LINES as usize {
                return (f.width, f.rgb.clone(), true);
            }
        }
        let width = self.sys.vdp().render_line(0).len();
        let mut fb = Vec::with_capacity(width * ACTIVE_LINES as usize);
        for line in 0..ACTIVE_LINES {
            fb.extend_from_slice(&self.sys.vdp().render_line(line));
        }
        (width, fb, false)
    }

    /// Refuse a run request while free-running. Doing it implicitly (pause, run, stay paused) would
    /// change the machine's mode as a side effect of a read-shaped call, which is exactly the class of
    /// silent state change this bus exists to make impossible — and which §5 and §8 item 12 explicitly
    /// forbid ("never pause or resume implicitly to make a call succeed").
    ///
    /// The code is [`code::INVALID_STATE`](crate::rpc::code::INVALID_STATE) (`-32005`), not `-32600`:
    /// the envelope is fine, the params are fine, nothing failed internally — the request is simply
    /// wrong *right now*. §5 names `emulator/run_frames` while free-running as its first worked example
    /// of exactly this, and the §6 run-control state rule requires `-32005` with
    /// `data.reason = "machineRunning"` for `run_to`, `run_to_scanline`, `run_frames` and `step*`.
    /// `reason` is the discriminant clients branch on; `message` names the fix, per §5.
    fn require_paused(&self, method: &str) -> Result<(), RpcError> {
        if self.free_run {
            return Err(RpcError::invalid_state(
                "machineRunning",
                format!("{method} needs the machine paused; call emulator/pause first"),
                Value::Null,
            ));
        }
        Ok(())
    }

    fn frame(&self) -> u64 {
        self.sys.scheduler().now() / MCLK_PER_FRAME
    }

    // ---------------------------------------------------------------- handlers

    fn status(&mut self, _params: &Value) -> Result<Value, RpcError> {
        let regs = self.sys.cpu_regs();
        let (pc, sp, sr) = (regs.pc, regs.a7(), regs.sr);
        let mut out = json!({
            "pc": hex::addr(pc),
            "sp": hex::addr(sp),
            "sr": hex::u16_hex(sr),
            // Deliberately the *emulated* frame index, not a UI counter. The sibling's `frame_token` is a
            // UI counter, which forced hand-rolled realignment three separate ways (recon §5 C2).
            "frameToken": self.frame(),
            "symbolCount": self.symbols.as_ref().map_or(0, |t| t.len()),
            "symbolsPath": self.symbols_path.clone(),
            "romPath": self.rom_path.clone(),
            "romBytes": self.sys.rom().len(),
            "romLoading": false,
        });
        if let Some((name, disp)) = self.symbol_at(pc) {
            out["symbolAtPc"] = json!(name);
            out["symbolDisp"] = json!(disp);
        }
        Ok(out)
    }

    fn registers(&mut self, _params: &Value) -> Result<Value, RpcError> {
        let r = self.sys.cpu_regs();
        let mut out = Map::new();
        for (i, d) in r.d.iter().enumerate() {
            out.insert(format!("d{i}"), json!(hex::addr(*d)));
        }
        for i in 0..8 {
            out.insert(format!("a{i}"), json!(hex::addr(r.addr_reg(i))));
        }
        out.insert("pc".into(), json!(hex::addr(r.pc)));
        out.insert("sp".into(), json!(hex::addr(r.a7())));
        out.insert("usp".into(), json!(hex::addr(r.usp)));
        out.insert("ssp".into(), json!(hex::addr(r.ssp)));
        out.insert("sr".into(), json!(hex::u16_hex(r.sr)));
        Ok(Value::Object(out))
    }

    fn run_frames(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/run_frames")?;
        let frames = match params.get("frames") {
            None => 1,
            Some(v) => hex::parse_count("frames", v, 1, self.config.max_run_frames)?,
        };
        self.running = true;
        self.emit_resumed();
        self.advance(frames);
        self.running = false;
        let pc = self.sys.cpu_regs().pc;
        // §3, §8 item 13: a completed `run_frames` is **`runFrames`**, not the nearest-looking `step`.
        // (CR-1 raised the gap — the enum had no value for a bounded frame advance — and was adopted on
        // 2026-08-14 as exactly this spelling, matching the existing `runTo`/`runToScanline`.) §3 pins
        // the two additive params with it: `frames` is REQUIRED here, and `deadlineReached` is always
        // `true`, the bound being the frame count itself.
        let mut extra = Map::new();
        extra.insert("frames".into(), json!(frames));
        extra.insert("deadlineReached".into(), json!(true));
        self.emit_stopped("runFrames", pc, extra);
        Ok(json!({
            "frames": frames,
            "frameToken": self.frame(),
            "pc": hex::addr(pc),
        }))
    }

    fn run_to(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/run_to")?;
        let target = self.resolve_target(params)?;
        // `maxFrames` is an *additive optional* param on a catalogued method, not a new op. Without a
        // bound, a target that is never reached is an unbounded run — i.e. exactly the transport hang
        // that destroyed a frozen repro frame in `aeon/docs/BUGS.md:494-551`.
        let max_frames = match params.get("maxFrames") {
            None => self.config.max_run_frames.min(600),
            Some(v) => hex::parse_count("maxFrames", v, 1, self.config.max_run_frames)?,
        };
        self.running = true;
        self.emit_resumed();
        let record = self.advance_until(max_frames, |pc, _| pc == target);
        self.running = false;
        let mut extra = Map::new();
        extra.insert("target".into(), json!(hex::addr(target)));
        extra.insert("deadlineReached".into(), json!(record.timed_out()));
        self.emit_stopped("runTo", record.pc, extra);

        // No `stoppedAtFrame`/`stoppedAtMclk`. They would be the envelope stamp (§2.2) spelled twice:
        // `record` is captured at the halt, and the stamp is computed on the same engine thread the
        // instant `dispatch` returns with the machine paused — and `run_to` *requires* a paused machine —
        // so nothing can advance between the two. §6.1 has already ruled this exact case for `restore`:
        // *"The `frame`/`mclk` in `checkpoint`'s result, and the whole of `restore`'s result, **are** the
        // machine stamp (§2.2) — no extra fields are needed and none should be invented."* Re-spelling the
        // stamp inside the result teaches clients that the stamp is not the answer, which is the one
        // lesson D11 exists to prevent. (CR-13, ruled 2026-08-15 —
        // `docs/2026-08-15-fable-ruling-cr13-cr14.md`; §6's row for this method is unchanged by it.)
        let mut out = json!({
            "target": hex::addr(target),
            "reached": record.fired(),
            "pc": hex::addr(record.pc),
            "maxFrames": max_frames,
        });
        if let Some((name, disp)) = self.symbol_at(record.pc) {
            out["symbol"] = json!(name);
            out["symbolDisp"] = json!(disp);
        }
        if record.timed_out() {
            out["caveat"] = json!(
                "the target PC was never reached within maxFrames — the run ended on its bound, so \
                 NOTHING about the machine state follows from where it stopped"
            );
        }
        Ok(out)
    }

    // `pause`/`resume` are the wire spelling of exactly one state change, so they share
    // [`set_free_run`](Engine::set_free_run) with the host's own pause mirroring — a client pausing the bus
    // and a person pressing the player's pause key must land in the same place, or the two would disagree
    // about whether the machine is running and every `-32005 machineRunning` decision after that is a coin
    // toss.
    fn pause(&mut self, _params: &Value) -> Result<Value, RpcError> {
        Ok(json!({"wasRunning": self.set_free_run(false)}))
    }

    fn resume(&mut self, _params: &Value) -> Result<Value, RpcError> {
        Ok(json!({"wasRunning": self.set_free_run(true)}))
    }

    fn read_memory(&mut self, params: &Value) -> Result<Value, RpcError> {
        let addr = self.resolve_target(params)?;
        let len = match params.get("len") {
            None => 1,
            Some(v) => hex::parse_count("len", v, 1, self.config.max_read_len)?,
        };
        let (data, region) = self.debug_read(addr, len as usize)?;
        let mut out = json!({
            "addr": hex::addr(addr),
            "len": data.len(),
            "bytes": hex::bytes(&data),
            "region": region,
            "caveat": "debug read: taken straight from the region, bypassing the bus — no open-bus \
                       latch, no VDP port path, no side effects. A CPU read at this address can differ.",
        });
        if let Some((name, disp)) = self.symbol_at(addr) {
            out["symbol"] = json!(name);
            out["symbolDisp"] = json!(disp);
        }
        Ok(out)
    }

    fn read_vram(&mut self, params: &Value) -> Result<Value, RpcError> {
        let vram = self.sys.vram();
        let addr = match params.get("addr") {
            None => 0,
            Some(v) => hex::parse_addr("addr", v)?,
        };
        let len = match params.get("len") {
            None => 32,
            Some(v) => hex::parse_count("len", v, 1, self.config.max_read_len)?,
        };
        let end = addr as u64 + len;
        if end > vram.len() as u64 {
            return Err(out_of_range(
                addr,
                "the read would run past the end of VRAM",
            ));
        }
        Ok(json!({
            "addr": hex::addr(addr),
            "len": len,
            "bytes": hex::bytes(&vram[addr as usize..end as usize]),
            "caveat": "debug read: taken straight from the VRAM array, bypassing the VDP port path, \
                       autoincrement, the FIFO and DMA.",
        }))
    }

    /// **Why is the dot at (x,y) the colour it is** — `protocol.md` §6, adopted as CR-10.
    ///
    /// Three normative behaviours, all of which are decisions rather than accidents:
    ///
    /// * **It is a pure read**, so it does **not** call [`require_paused`](Engine::require_paused). §6's
    ///   run-control state rule names the ops that mutate the timeline; a read mutates nothing, and the
    ///   torn-instant hazard of sampling a free-running machine is what D11's envelope stamp already
    ///   answers (`running: true` is the client's warning, and it is on every reply).
    /// * **It answers about the VDP's state *now***. The core re-derives the scanline on every call and
    ///   reads no framebuffer, so this and `emulator/screenshot` can legitimately disagree — and pausing
    ///   does **not** reconcile them, because attribution is a whole-frame-state read by construction.
    ///   On any ROM whose registers, CRAM or scroll moved mid-frame the two differ paused or not; the
    ///   reconciliation path is per-scanline capture, not `pause`.
    /// * **A dot outside the active display is refused** with `-32004`, carrying `width`/`height` in
    ///   `error.data` so the client learns the bound from the refusal. The core is deliberately *total*
    ///   there — it answers backdrop — which is right in-process and wrong on a wire: a client asking
    ///   about a dot that does not exist would get a well-formed backdrop answer indistinguishable from a
    ///   genuine backdrop dot. That is the silent-wrong-answer class this bus exists to prevent.
    ///
    /// Two cases that look like the third but are **not** errors, and must keep answering: a blanked
    /// display, and the leftmost-column blank at `x < 8`. Those dots exist, and the backdrop genuinely is
    /// what is shown. Both yield exactly one candidate.
    ///
    /// The reply's key set is **exactly** the schematized one — no surplus (the ruling's condition 4).
    fn pixel_attribution(&mut self, params: &Value) -> Result<Value, RpcError> {
        let vdp = self.sys.vdp();
        let (width, height) = vdp.active_display();
        // The schema bounds the *params* at 0..=511 (the widest addressable value); the ACTIVE bound is
        // the width/height reported below, and it is enforced separately so the two failures stay
        // distinguishable: a nonsensical coordinate is -32602, an off-display one is -32004.
        let coord = |field: &str| -> Result<u16, RpcError> {
            let v = params
                .get(field)
                .ok_or_else(|| RpcError::invalid_params(format!("`{field}` is required")))?;
            Ok(hex::parse_count(field, v, 0, 511)? as u16)
        };
        let x = coord("x")?;
        let y = coord("y")?;
        if x >= width || y >= height {
            return Err(RpcError::new(
                code::ADDRESS_OUT_OF_RANGE,
                format!(
                    "({x},{y}) is outside the active display ({width}x{height}) — the dot does not \
                     exist, so there is nothing to attribute it to"
                ),
            )
            .with_data(json!({"width": width, "height": height})));
        }

        let attr = vdp.pixel_attribution(x, y);
        let mut out = json!({
            "x": attr.x,
            "y": attr.y,
            "width": width,
            "height": height,
            "winner": layer_json(attr.winner),
            "cramIndex": attr.cram_index,
            "cramAddr": hex::addr(u32::from(attr.cram_index) * 2),
            "rgb": {"r": attr.rgb.0, "g": attr.rgb.1, "b": attr.rgb.2},
            "state": match attr.state {
                PixelState::Shadow => "shadow",
                PixelState::Normal => "normal",
                PixelState::Highlight => "highlight",
            },
            "candidates": attr.candidates.iter().map(|c| {
                let mut o = layer_json(c.layer);
                o["opaque"] = json!(c.opaque);
                o["priority"] = json!(c.priority);
                o["cramIndex"] = json!(c.cram_index);
                o["verdict"] = json!(match c.verdict {
                    CandidateVerdict::Won => "won",
                    CandidateVerdict::LostToPriority => "lostToPriority",
                    CandidateVerdict::Transparent => "transparent",
                    CandidateVerdict::Operator => "operator",
                });
                o
            }).collect::<Vec<_>>(),
        });

        // `cell` is present iff the winner is a plane or the window; the core returns `None` for
        // sprite/backdrop, so the iff is the core's own invariant rather than a second decision here.
        if let Some(cell) = attr.cell {
            out["cell"] = json!({
                "tile": cell.tile,
                "tileAddr": hex::addr(tile_addr(cell.tile)),
                "palette": cell.palette,
                "hflip": cell.hflip,
                "vflip": cell.vflip,
                "priority": cell.priority,
            });
        }
        if let Layer::Sprite(index) = attr.winner {
            let sprites = self.sys.vdp().sprites_decoded();
            let s = sprites.get(usize::from(index)).ok_or_else(|| {
                RpcError::new(
                    code::INTERNAL_ERROR,
                    format!(
                        "the renderer named sprite {index}, which the SAT decode does not have"
                    ),
                )
            })?;
            let sat_addr = (self.sys.vdp().sat_base() as u32 + u32::from(index) * 8) & 0xFFFF;
            let mut sp = json!({
                "index": s.index,
                "x": s.x,
                "y": s.y,
                "widthCells": s.width_cells,
                "heightCells": s.height_cells,
                "baseTile": s.tile,
                "palette": s.palette,
                "hflip": s.hflip,
                "vflip": s.vflip,
                "priority": s.priority,
                "satAddr": hex::addr(sat_addr),
            });
            // Absent, not invented: the winning sprite's box no longer containing the dot means the SAT
            // was rewritten between the render and this query.
            if let Some(tile) = oracle_core::render::sprite_tile_at(s, x, y) {
                sp["tile"] = json!(tile);
                sp["tileAddr"] = json!(hex::addr(tile_addr(tile)));
            }
            if s.cache_divergence {
                sp["cacheDivergence"] = json!(true);
            }
            out["sprite"] = sp;
        }
        Ok(out)
    }

    fn state_hash(&mut self, params: &Value) -> Result<Value, RpcError> {
        let include_fb = match params.get("includeFramebuffer") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                return Err(RpcError::invalid_params(
                    "`includeFramebuffer` must be a boolean (D9)",
                ))
            }
        };
        let h = self.sys.state_hash();
        let hex_of = oracle_core::state_hash::hex;
        let mut out = json!({
            "vram": hex_of(h.vram),
            "cram": hex_of(h.cram),
            "vsram": hex_of(h.vsram),
            "regs": hex_of(h.regs),
            "combined": hex_of(h.combined),
            "caveat": "these fingerprints cover VDP state only (VRAM, CRAM, VSRAM, VDP registers) — \
                       they say nothing about the CPU, work RAM, the Z80, SRAM or audio. Two machines \
                       agreeing here can still differ.",
        });
        if include_fb {
            let (_, fb, from_raster) = self.framebuffer();
            let mut bytes = Vec::with_capacity(fb.len() * 3);
            for (r, g, b) in fb {
                bytes.extend_from_slice(&[r, g, b]);
            }
            out["framebuffer"] = json!(hex_of(oracle_core::state_hash::fnv1a_bytes(&bytes)));
            // Two different pictures can be hashed here, so which one it was has to be on the wire: a
            // fingerprint whose provenance is ambiguous is a fingerprint two machines can disagree on for a
            // reason that has nothing to do with either machine.
            out["framebufferSource"] = json!(if from_raster { "raster" } else { "stateRender" });
        }
        Ok(out)
    }

    fn screenshot(&mut self, params: &Value) -> Result<Value, RpcError> {
        let path: PathBuf = match params.get("path") {
            None => std::env::temp_dir().join(format!("oracle-frame-{}.ppm", self.frame())),
            Some(Value::String(s)) => PathBuf::from(s),
            Some(_) => return Err(RpcError::invalid_params("`path` must be a string")),
        };
        let (width, fb, from_raster) = self.framebuffer();
        let mut bytes = Vec::with_capacity(20 + fb.len() * 3);
        bytes.extend_from_slice(format!("P6\n{width} {ACTIVE_LINES}\n255\n").as_bytes());
        for (r, g, b) in &fb {
            bytes.extend_from_slice(&[*r, *g, *b]);
        }
        std::fs::write(&path, &bytes).map_err(|e| {
            RpcError::new(
                code::INTERNAL_ERROR,
                format!("cannot write {}: {e}", path.display()),
            )
            .with_data(json!({"path": path.display().to_string()}))
        })?;
        let mut out = json!({
            "path": path.display().to_string(),
            "format": "ppm",
            "width": width,
            "height": ACTIVE_LINES,
            "bytes": bytes.len(),
            "source": if from_raster { "raster" } else { "stateRender" },
        });
        if !from_raster {
            // The honest caveat is now only true of the fallback — see [`Engine::framebuffer`]. Emitting it
            // unconditionally would be the mirror of the bug it warns about: telling a caller their
            // scanline-accurate capture is not one.
            out["caveat"] = json!(
                "no whole frame has been drawn yet, so this is rendered from the VDP state as it stands \
                 right now. Mid-frame CRAM/scroll changes that a real raster would show on different \
                 lines are NOT reproduced — run at least one frame for a scanline-accurate capture."
            );
        }
        Ok(out)
    }

    fn press(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/press")?;
        let port = parse_port(params)?;
        let buttons = parse_buttons(params)?;
        // A tap advances the machine, so its own ceiling cannot exceed the run ceiling: hosted, a 1,000-frame
        // tap would freeze the player's window (and its OS event pump) for as long as a 1,000-frame
        // `run_frames`, and bounding one without the other would just move the hole. `min` rather than a
        // rewrite because the default 3,600-frame server keeps the 1,000 it always had.
        let frame_cap = self.config.max_run_frames.min(1000);
        let frames = match params.get("frames") {
            None => 2,
            Some(v) => hex::parse_count("frames", v, 1, frame_cap)?,
        };
        let mut pad = merge_pads(self.live[port], self.held[port]);
        for b in &buttons {
            set_button(&mut pad, b, true);
        }
        self.sys.set_pad(port, pad);
        self.running = true;
        self.emit_resumed();
        self.advance(frames);
        self.running = false;
        // Restore exactly the held set — a tap must not leak into later frames, and must not clear a
        // button the client is separately holding (nor one the human is physically holding).
        self.apply_pads();
        let pc = self.sys.cpu_regs().pc;
        // A tap advances whole **frames**, so `step` is affirmatively wrong: §3 pins it as "one
        // instruction, or one instruction-shaped unit … **not** the value for a frame advance".
        // `runFrames` is merely imprecise — this was not an `emulator/run_frames` call — and the enum is
        // closed, so emitting a new value unilaterally is the invention §8 forbids. Between a value §3
        // rules out and the nearest admissible one, the nearest admissible one wins: a client watching
        // the stream sees "a bounded frame advance completed", which is true, instead of "an instruction
        // completed", which is not. The residual ambiguity — *which method* drove the advance — is
        // raised as CR-9 (`docs/2026-08-14-aether-change-requests.md`) and is the owner's to rule on;
        // this comment is the record of the choice, not a licence to keep it if the ruling differs.
        let mut extra = Map::new();
        extra.insert("frames".into(), json!(frames));
        extra.insert("deadlineReached".into(), json!(true));
        self.emit_stopped("runFrames", pc, extra);
        Ok(json!({
            "buttons": buttons,
            "frames": frames,
            "port": port,
            "frameToken": self.frame(),
        }))
    }

    fn hold(&mut self, params: &Value) -> Result<Value, RpcError> {
        let port = parse_port(params)?;
        let buttons = parse_buttons(params)?;
        let down = match params.get("down") {
            None => true,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(RpcError::invalid_params("`down` must be a boolean (D9)")),
        };
        for b in &buttons {
            set_button(&mut self.held[port], b, down);
        }
        self.apply_pads();
        Ok(json!({
            "buttons": buttons,
            "down": down,
            "port": port,
            "held": held_names(&self.held[port]),
        }))
    }

    fn release_all(&mut self, _params: &Value) -> Result<Value, RpcError> {
        // Clears **the client's** held set on both pads, which is all this method ever owned. A button the
        // human is physically holding is not the bus's to release, and pretending otherwise would last
        // exactly until the next iteration re-read the keyboard.
        self.held = [Pad::default(); 2];
        self.apply_pads();
        // `{}`, deliberately. §6's row for this method gives the result as `—`, and a `"released": true`
        // that no branch can ever set to `false` carries zero bits — it is a constant wearing an answer's
        // clothes. §6.1's ruling for `restore` is the precedent and it is verbatim applicable: the reply
        // *is* the machine stamp (§2.2), *"no extra fields are needed and none should be invented"*, so
        // `restore` emits `{}` too. (CR-13, ruled 2026-08-15 —
        // `docs/2026-08-15-fable-ruling-cr13-cr14.md`; §6's row is unchanged by that ruling.)
        Ok(json!({}))
    }

    fn lookup_symbol(&mut self, params: &Value) -> Result<Value, RpcError> {
        let table = self.symbols.as_ref().ok_or_else(no_symbols)?;
        let limit = self.config.max_symbol_matches;

        // address -> nearest preceding label + displacement (§4).
        if let Some(a) = params.get("addr") {
            let addr = hex::parse_addr("addr", a)?;
            let r = table.resolve(addr).ok_or_else(|| {
                RpcError::new(
                    code::SYMBOL_NOT_FOUND,
                    format!("no symbol at or before {}", hex::addr(addr)),
                )
                .with_data(json!({"addr": hex::addr(addr)}))
            })?;
            let mut out = json!({
                "query": hex::addr(addr),
                "name": r.to_string(),
                "rawName": r.symbol.name,
                "addr": hex::addr(r.symbol.addr),
                "disp": r.displacement,
                "ambiguous": r.symbol.demangled_ambiguous,
                "synthetic": r.symbol.is_synthetic,
            });
            if r.displacement > 0 {
                out["caveat"] = json!(format!(
                    "nearest *preceding* symbol: the address is ${:X} past it and may belong to no \
                     symbol at all (the listing carries no sizes).",
                    r.displacement
                ));
            }
            if r.symbol.demangled_ambiguous {
                out["caveat"] = json!(
                    "several different addresses share this readable name (a macro expanded more than \
                     once), so it does not identify a location — use `rawName`, which is unique."
                );
            }
            return Ok(out);
        }

        // name -> address, falling back to a bounded prefix search (§4).
        let Some(name) = params.get("name") else {
            return Err(RpcError::invalid_params(
                "one of `name` (string) or `addr` (hex string) is required",
            ));
        };
        let Some(name) = name.as_str() else {
            return Err(RpcError::invalid_params("`name` must be a string"));
        };
        if let Some(sym) = table.by_name(name) {
            return Ok(json!({
                "name": sym.name,
                "demangled": sym.demangled,
                "addr": hex::addr(sym.addr),
                "rawAddr": hex::addr(sym.raw_addr),
                "ambiguous": sym.demangled_ambiguous,
            }));
        }
        let exact_demangled = table.by_demangled(name);
        if !exact_demangled.is_empty() {
            let total = exact_demangled.len();
            let items: Vec<Value> = exact_demangled
                .iter()
                .take(limit)
                .map(|s| json!({"name": s.name, "addr": hex::addr(s.addr)}))
                .collect();
            let first = exact_demangled[0];
            let mut out = json!({
                "name": first.name,
                "demangled": first.demangled,
                "addr": hex::addr(first.addr),
                "otherMatches": rpc::bounded_array(items, total, 0, limit),
                "ambiguous": total > 1,
            });
            if total > 1 {
                out["caveat"] = json!(format!(
                    "{total} different addresses answer to this readable name; `addr` is only the \
                     first. Use the unique mangled `name` from `otherMatches`."
                ));
            }
            return Ok(out);
        }
        let prefixed = table.with_prefix(name);
        let by_demangled = table.with_demangled_prefix(name);
        let mut all: Vec<&oracle_core::symbols::Symbol> =
            prefixed.into_iter().chain(by_demangled).collect();
        all.sort_by_key(|s| (s.addr, s.name.clone()));
        all.dedup_by(|a, b| a.name == b.name && a.addr == b.addr);
        if all.is_empty() {
            return Err(RpcError::new(
                code::SYMBOL_NOT_FOUND,
                format!("no symbol named or prefixed {name}"),
            )
            .with_data(json!({"name": name})));
        }
        let total = all.len();
        let items: Vec<Value> = all
            .iter()
            .take(limit)
            .map(|s| json!({"name": s.name, "demangled": s.demangled, "addr": hex::addr(s.addr)}))
            .collect();
        Ok(json!({
            "query": name,
            "exact": false,
            "otherMatches": rpc::bounded_array(items, total, 0, limit),
            "caveat": "no symbol has that exact name; these are prefix matches and any of them may be \
                       the wrong one.",
        }))
    }

    fn load_symbols(&mut self, params: &Value) -> Result<Value, RpcError> {
        let Some(Value::String(path)) = params.get("path") else {
            return Err(RpcError::invalid_params("`path` (string) is required"));
        };
        let text = std::fs::read_to_string(path).map_err(|e| {
            RpcError::invalid_params(format!("cannot read {path}: {e}"))
                .with_data(json!({"path": path}))
        })?;
        let table = SymbolTable::parse(&text).map_err(|e| {
            RpcError::invalid_params(format!("{path} is not a usable listing: {e}"))
                .with_data(json!({"path": path}))
        })?;

        // §4's forward hook, live rather than reserved: the listing is validated against the image
        // actually loaded, and a listing from a different build shape is REFUSED. Of the symbols
        // `s4.lst` and `s4.debug.lst` share, 92.6% name a different address — a mismatched listing is
        // not degraded information, it is confidently wrong information.
        let binding = table.validate_against_rom(self.sys.rom());
        let (accepted, caveat) = match binding {
            RomBinding::Match { .. } => (
                true,
                Some(
                    "the deb2 appendix probe is a filter, not a proof: Match means \"not obviously \
                     wrong\", never \"proven right\" (two demo shapes can declare the same EndOfRom).",
                ),
            ),
            RomBinding::Mismatch(fault) => {
                return Err(RpcError::invalid_params(format!(
                    "{path} does not describe the loaded ROM: {}",
                    describe_fault(fault)
                ))
                .with_data(json!({"path": path, "binding": "mismatch"})));
            }
            RomBinding::Indeterminate(_) if !table.is_intact() => {
                // Fail-open closed (recon §9g): a listing that WOULD be refused becomes merely
                // Indeterminate once its EndOfRom row goes missing, and truncation removes rows from
                // the end — where EndOfRom sits.
                return Err(RpcError::invalid_params(format!(
                    "{path} cannot be bound to the loaded ROM and is not internally intact, so it \
                     cannot be trusted"
                ))
                .with_data(json!({"path": path, "binding": "indeterminate-and-damaged"})));
            }
            RomBinding::Indeterminate(_) => (
                true,
                Some(
                    "this listing declares no EndOfRom, so it could not be checked against the loaded \
                     ROM at all. Accepted unverified because it is internally intact.",
                ),
            ),
        };
        debug_assert!(accepted);

        let count = table.len();
        let modules = table.modules().len();
        self.symbols = Some(Arc::new(table));
        self.symbols_path = Some(path.clone());
        let mut out = json!({
            "path": path,
            "symbolCount": count,
            "moduleCount": modules,
            "binding": binding_name(&binding),
        });
        if let Some(c) = caveat {
            out["caveat"] = json!(c);
        }
        Ok(out)
    }

    fn reload_rom(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/reload_rom")?;
        let path = match params.get("path") {
            Some(Value::String(s)) => s.clone(),
            Some(_) => return Err(RpcError::invalid_params("`path` must be a string")),
            None => self
                .rom_path
                .clone()
                .ok_or_else(|| RpcError::invalid_params("no ROM path is known; pass `path`"))?,
        };
        let rom = std::fs::read(&path).map_err(|e| {
            RpcError::invalid_params(format!("cannot read {path}: {e}"))
                .with_data(json!({"path": path}))
        })?;
        let len = rom.len();
        self.sys.load_rom(rom);
        self.sys.reset();
        self.held = [Pad::default(); 2];
        self.rom_path = Some(path.clone());
        // A different cartridge draws a different picture, and the line stream restarts from the reset
        // vector — so the frame latched from the previous image is not "slightly stale", it is another
        // game's. Dropped rather than kept, which puts `framebuffer` back on its honest fallback until the
        // new image has drawn a frame of its own.
        self.invalidate_screen();
        self.rom_generation += 1;

        // A reload can invalidate the loaded symbols — that is D7's whole point. Re-run the binding
        // check and drop the table if it no longer describes the image.
        let mut symbols_dropped = false;
        if let Some(t) = &self.symbols {
            if matches!(
                t.validate_against_rom(self.sys.rom()),
                RomBinding::Mismatch(_)
            ) {
                self.symbols = None;
                self.symbols_path = None;
                symbols_dropped = true;
            }
        }

        let mut params_out = Map::new();
        params_out.insert("path".into(), json!(path));
        params_out.insert("symbolsDropped".into(), json!(symbols_dropped));
        self.emit("emulator/romReloaded", params_out);

        let mut out = json!({
            "reloaded": true,
            "path": path,
            "romBytes": len,
            "symbolsDropped": symbols_dropped,
        });
        if symbols_dropped {
            out["caveat"] = json!(
                "the loaded symbol listing no longer binds to this ROM image and was dropped; load \
                 the listing for the new build before resolving anything."
            );
        }
        Ok(out)
    }

    // ------------------------------------------------------- checkpoints (§6.1, D13)
    //
    // **Deliberately NOT `require_paused`.** §6's run-control state rule names exactly the ops that
    // demand a paused machine — `run_to`, `run_to_scanline`, `run_frames`, `step*` — because each of them
    // *advances* the machine and would fight the free-run loop for it. None of the four checkpoint
    // methods advances anything: they read or replace the machine wholesale, on the engine thread,
    // between frames (see `server::engine_loop`), so a capture taken while free-running is a coherent
    // frame-aligned coordinate and a restore leaves the mode exactly as it found it. §5's own worked
    // examples of `-32005` list only the cap and the unknown id for these methods, never "while
    // running". Refusing anyway would be a bound the contract does not ask for, and — since §5 also
    // forbids resolving a wrong-state case implicitly — the client's only recourse would be to pause,
    // call, and resume, which changes the machine's mode to service a call that never needed it.

    fn checkpoint(&mut self, params: &Value) -> Result<Value, RpcError> {
        // Ids are **server-assigned** (§6.1). A client-proposed `id` is not an error, it is simply not
        // an input — honouring one would let two clients on one bus overwrite each other's coordinates.
        let label =
            match params.get("label") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.clone()),
                Some(_) => return Err(RpcError::invalid_params(
                    "`label` must be a string (it is carried back verbatim and never interpreted)",
                )),
            };

        // D13 rule 3: refuse at the cap, never evict. Checked *before* the snapshot is taken so a
        // refusal costs nothing.
        let cap = self.config.max_checkpoints;
        let count = self.checkpoints.len();
        if count >= cap {
            return Err(RpcError::invalid_state(
                "checkpointCapReached",
                format!(
                    "all {cap} checkpoint slots are in use; make room first: \
                     emulator/checkpoint_drop"
                ),
                json!({"cap": cap, "count": count}),
            ));
        }

        let snapshot = self.sys.snapshot();
        let bytes = snapshot.len();
        let mclk = self.sys.scheduler().now();
        let id = self.next_checkpoint_id;
        self.next_checkpoint_id += 1;
        let slot = Checkpoint {
            id,
            label,
            frame: mclk / MCLK_PER_FRAME,
            mclk,
            snapshot,
            held: self.held,
            rom_path: self.rom_path.clone(),
            symbols: self.symbols.clone(),
            symbols_path: self.symbols_path.clone(),
        };
        // The handle goes out as a **JSON string** (D9 category 4, §6.1, §8 item 16, and
        // `#/$defs/handle` in the schema, which D14 makes the authority on the wire). It is issued to be
        // handed back and nothing else: a client MUST NOT parse it, order it, compare two of them or do
        // arithmetic on one. A number here would invite exactly that computation — an id assigned from a
        // counter *reads* like a slot index, which is how this server shipped one in good faith against
        // D9's earlier wording, and why the string is now pinned rather than left to a reading.
        let wire_id = slot.wire_id();
        self.checkpoints.push(slot);
        // `frame`/`mclk` are deliberately absent: §6.1 says they **are** the machine stamp, and the stamp
        // is merged in structurally after this returns (`rpc::stamp_result`). Emitting them here would be
        // a shadowed duplicate that the envelope overwrites with the identical values anyway — nothing
        // has advanced the machine between the capture above and the stamp.
        Ok(json!({"id": wire_id, "bytes": bytes}))
    }

    fn restore(&mut self, params: &Value) -> Result<Value, RpcError> {
        let id = parse_checkpoint_id(params)?;
        let Some(cp) = self.checkpoints.iter().find(|c| c.wire_id() == id) else {
            // §6.1: an unknown or already-dropped id is refused, never a silent no-op. A no-op here
            // would leave the caller running its next experiment against whatever machine happened to be
            // loaded, believing it had gone back.
            //
            // A well-formed string this server could never have issued (`"0x1"`, `"abc"`) lands here
            // too, and deliberately: to a client the handle is opaque, so "that is not one of mine" is
            // the only distinction the wire is allowed to make. Refusing garbage with a *different* code
            // than a dropped id would publish the internal spelling of an id — the one thing D9
            // category 4 exists to keep private — and would break the day the spelling changes.
            return Err(unknown_checkpoint(&id));
        };

        // D13 rule 2: the ENTIRE machine — CPU, RAM, VDP, sound state and the ROM. A checkpoint taken
        // before an `emulator/reload_rom` therefore brings the previous cartridge back; that is defined
        // behaviour, not an error case.
        let sys = System::restore(&cp.snapshot).map_err(|e| {
            RpcError::new(
                code::INTERNAL_ERROR,
                format!("checkpoint {id} could not be decoded: {e}"),
            )
            .with_data(json!({ "id": &id }))
        })?;
        let (held, rom_path) = (cp.held, cp.rom_path.clone());
        let (symbols, symbols_path) = (cp.symbols.clone(), cp.symbols_path.clone());
        // Nothing is applied until the decode has succeeded — a half-applied restore is exactly what
        // "MUST NOT partially restore" rules out.
        self.sys = sys;
        self.held = held;
        self.rom_path = rom_path;
        // The restored machine is a different timeline (D13 rule 2 makes it a different *cartridge* too, if
        // the checkpoint predates a `reload_rom`), so the latched frame and the half-built capture both
        // belong to a machine that is no longer here. `rom_generation` moves unconditionally: this restore
        // may or may not have swapped the image, and a host that has to guess which is a host that will
        // eventually guess wrong.
        self.invalidate_screen();
        self.rom_generation += 1;
        // The symbol table travels with the cartridge it was bound to (D7). It is deliberately *not*
        // re-validated here: the listing and the ROM were checked against each other when the listing was
        // loaded, and both halves are being replaced together from the same slot, so the pair is coherent
        // by construction. Re-running `validate_against_rom` would add a way for a restore to fail
        // half-way through — the one outcome §6.1 rules out — in service of an invariant already held.
        // The debug assertion below is that reasoning, made checkable.
        self.symbols = symbols;
        self.symbols_path = symbols_path;
        #[cfg(debug_assertions)]
        if let Some(t) = &self.symbols {
            debug_assert!(
                !matches!(
                    t.validate_against_rom(self.sys.rom()),
                    RomBinding::Mismatch(_)
                ),
                "a checkpoint's symbol table must still bind to the ROM restored from the same slot"
            );
        }

        // The whole result **is** the machine stamp (§6.1), reporting the restored coordinate — which is
        // precisely the confirmation the caller wants, so no extra field is needed and none is invented.
        Ok(json!({}))
    }

    fn checkpoint_list(&mut self, params: &Value) -> Result<Value, RpcError> {
        let total = self.checkpoints.len();
        let cap = self.config.max_checkpoints;
        // **The cursor is an `id`, not a position** — "resume at the first id strictly greater than
        // this". §6.1 requires that a client "must never be handed a partial list it can mistake for a
        // complete one", and it is explicit that two clients share one bus (the stated reason ids are
        // server-assigned at all). A positional cursor breaks precisely there: `checkpoint_drop`
        // compacts the `Vec`, so a drop *before* an outstanding cursor shifts every later slot left and
        // the next page steps over a live checkpoint — while still reporting `truncated: false`, which is
        // the one thing the sentence above forbids. Ids are monotonic and never reused, so an id cursor
        // is stable no matter what is dropped underneath it.
        //
        // The bound is the highest id the server has ever *issued*, not the live count: a cursor for
        // slots that have since been dropped is a legitimate stale page (answered below with an empty
        // one), while an id that was never handed out is a typo and is still refused loudly.
        let highest_issued = self.next_checkpoint_id - 1;
        let cursor = match params.get("cursor") {
            None => 0,
            Some(v) => parse_cursor(v, highest_issued)?,
        };
        let limit = match params.get("limit") {
            None => cap,
            Some(v) => hex::parse_count("limit", v, 1, cap as u64)? as usize,
        };
        // The continuation token below is "the last id on this page", which is only the right place to
        // resume if the slots are id-ascending. They are, by construction — `checkpoint` pushes ids from
        // a monotonic counter and `checkpoint_drop` uses `retain`, which preserves order — so this is the
        // assumption written down where it would break rather than left implicit.
        debug_assert!(
            self.checkpoints.windows(2).all(|w| w[0].id < w[1].id),
            "the checkpoint slots must stay id-ascending for the cursor to be a resume point"
        );
        // How many live slots the cursor is already past. This is what the shared bounded-array rule
        // needs in order to compute `truncated` correctly, and it is derived from the ids rather than
        // assumed from the cursor, so a drop under the client cannot make it lie.
        let skipped = self.checkpoints.iter().filter(|c| c.id <= cursor).count();
        let page_slots: Vec<&Checkpoint> = self
            .checkpoints
            .iter()
            .filter(|c| c.id > cursor)
            .take(limit)
            .collect();
        // The continuation token is the last id on this page, read from the **slot** rather than parsed
        // back out of the emitted JSON: the wire id is an opaque string now, and re-parsing a decimal out
        // of it would be this server doing to its own handle precisely what §6.1 forbids a client to do.
        let next_cursor = page_slots.last().map_or(cursor, |c| c.id);
        let items: Vec<Value> = page_slots
            .iter()
            .map(|c| {
                let mut e = json!({
                    "id": c.wire_id(),
                    "frame": c.frame,
                    "mclk": c.mclk,
                    "bytes": c.snapshot.len(),
                });
                // Optional per §6.1: a checkpoint taken without one carries no key, rather than an empty
                // string a client would have to guess the meaning of.
                if let Some(l) = &c.label {
                    e["label"] = json!(l);
                }
                e
            })
            .collect();

        // The house's one bounded-array rule (`rpc::bounded_array`, recon §4 non-negotiable #2) computes
        // the page; §6.1's spelling for this method names the array `checkpoints` and its continuation
        // token `cursor`, so the same envelope is emitted under the catalog's names rather than a second
        // pagination convention being invented alongside it. The helper is **not** changed to understand
        // ids — it is shared with `lookup_symbol`, whose cursor really is a position into an immutable
        // result set, and re-defining it for everyone to fix one caller would be the wrong trade. It is
        // fed the positional `skipped`, which is exactly what its `total`/`returned`/`truncated` maths
        // needs, and only the emitted continuation token is this method's own.
        let page = rpc::bounded_array(items, total, skipped, limit);
        let mut out = Map::new();
        out.insert("checkpoints".into(), page["items"].clone());
        out.insert("total".into(), page["total"].clone());
        out.insert("returned".into(), page["returned"].clone());
        out.insert("limit".into(), page["limit"].clone());
        out.insert("truncated".into(), page["truncated"].clone());
        // `cursor` is returned **when more remain** and is absent otherwise, so a client can never mistake
        // "here is where to continue" for "you are at the end". The helper's own `nextCursor` is
        // positional and is deliberately not emitted; only whether it exists is used.
        //
        // The token is emitted as a **JSON string**, which is what the contract schema types it as
        // (`schema/bus-protocol.schema.json`, `emulator/checkpoint_list` params *and* result) — §8
        // forbids inventing a wire shape alongside the catalog's. The string is also the shape the
        // §6.1 cursor paragraph wants: the token is **opaque**, "a client MUST NOT parse it", and a
        // bare number is an open invitation to the `cursor + 1` / `cursor > n` arithmetic that the
        // opacity rule exists to prevent. Quoting it does not *stop* a determined client — the value
        // is still an id in decimal — but it stops the accident.
        if page["nextCursor"].is_u64() {
            out.insert("cursor".into(), json!(next_cursor.to_string()));
        }
        Ok(Value::Object(out))
    }

    fn checkpoint_drop(&mut self, params: &Value) -> Result<Value, RpcError> {
        let all = match params.get("all") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(RpcError::invalid_params("`all` must be a boolean (D9)")),
        };
        if all {
            if params.get("id").is_some() {
                return Err(RpcError::invalid_params(
                    "`id` and `all` are mutually exclusive — pass one",
                ));
            }
            let removed = self.checkpoints.len();
            self.checkpoints.clear();
            return Ok(json!({"removed": removed}));
        }
        let id = parse_checkpoint_id(params)?;
        let before = self.checkpoints.len();
        self.checkpoints.retain(|c| c.wire_id() != id);
        // Unlike `restore`, dropping an id that is already gone is answered rather than refused: `removed`
        // is the count that actually went (§6.1), and `0` is a complete, machine-readable answer to
        // "is it gone?" — the caller's intent is satisfied either way. The hazard behind §6.1's refusal
        // is a `restore` that succeeds against a machine the client did not ask for, which has no
        // analogue here: nothing was restored, nothing was evicted, and no id changed meaning.
        Ok(json!({"removed": before - self.checkpoints.len()}))
    }
}

// -------------------------------------------------------------------- free functions

/// A checkpoint `id`: an **opaque JSON string** (D9 category 4, §6.1, §8 item 16), required.
///
/// **Strict — a string only, with no numeric fallback**, and deliberately unlike [`parse_cursor`] two
/// functions down, which accepts both. The two handles are handled differently because they reach the
/// server by different routes:
///
/// * A `cursor` is only ever *round-tripped*. A client takes the token this server issued and hands it
///   straight back, so a number-typed field somewhere in the client's own storage is a plausible
///   accident, and refusing a token we ourselves issued punishes the client for our bug.
/// * An `id` is the handle a **human hand-types** into the next call. Typing `{"id": 3}` **is** the
///   arithmetic-on-a-handle that D9 category 4 exists to forbid — the client has looked at the id,
///   decided it is the number three, and written it down as one. Accepting it would reward the
///   forbidden usage and, worse, make it invisible: everything would keep working right up until the
///   day the ids stop looking like small integers.
///
/// The strictness is affordable exactly once. §8 item 16 names it: this surface has no clients yet,
/// which is the cheapest moment the change will ever be available.
///
/// The refusals stay loud and keep their existing codes. The wrong JSON *type* is `-32602`; an id that
/// is not live — dropped, never issued, or a string this server could not have spelled — is `-32005
/// unknownCheckpoint` at the call site. Neither is ever clamped to a neighbour and neither is a silent
/// no-op.
fn parse_checkpoint_id(params: &Value) -> Result<String, RpcError> {
    match params.get("id") {
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        // `#/$defs/handle` is `{"type":"string","minLength":1}`; an empty handle is a shape violation,
        // not an unknown checkpoint, so it is refused here rather than looked up and missed.
        Some(Value::String(_)) => Err(RpcError::invalid_params(
            "`id` must be a non-empty string — pass back the handle `emulator/checkpoint` returned",
        )),
        None | Some(Value::Null) => Err(RpcError::invalid_params(
            "`id` (the opaque string handle returned by emulator/checkpoint) is required",
        )),
        Some(other) => Err(RpcError::invalid_params(format!(
            "`id` must be a JSON string — the handle is opaque and a client must not compute on it \
             (D9 category 4); got {}",
            hex::kind_of(other)
        ))),
    }
}

/// A `checkpoint_list` continuation token, resolved back to the id it stands for.
///
/// **A deliberate asymmetry: this server emits exactly one shape and accepts two** (Postel's law, and
/// nothing more than that — leniency here is a migration allowance, not a licence for the emit side
/// to drift).
///
/// * *Emitted:* a **JSON string**, always. That is what the contract schema types `cursor` as, on
///   both the params and the result of `emulator/checkpoint_list`, and §8 forbids inventing a
///   different wire shape alongside the catalog's. It is also the spelling §6.1's opacity rule
///   wants: "a client MUST NOT parse it", and a bare number invites exactly the `cursor + 1`
///   arithmetic that rule exists to prevent.
/// * *Accepted:* a string **or** a bare number. This server emitted numbers before this fix, so a
///   client written against that behaviour — or one that round-tripped a token through a
///   number-typed field — would break on a strict-string parse for no contract benefit. A cursor is
///   an opaque token the server itself issued; refusing to recognise a token we ourselves handed out
///   punishes the client for our bug.
///
/// The leniency stops at the shape. An out-of-range token is still refused loudly, never clamped
/// (the house rule), and a string that is not a token this server could have issued is refused too.
fn parse_cursor(v: &Value, max: u64) -> Result<u64, RpcError> {
    let n = match v {
        // The shape we emit.
        Value::String(s) => s.parse::<u64>().ok(),
        // The shape we used to emit.
        _ => v.as_u64(),
    };
    let Some(n) = n else {
        // Name what was actually sent: a malformed *string* is a different mistake from the wrong
        // JSON type, and reporting "not a string" for a string would be a riddle.
        let got = match v {
            Value::String(s) => format!("{s:?} is not one"),
            other => format!("got {}", hex::kind_of(other)),
        };
        return Err(RpcError::invalid_params(format!(
            "`cursor` must be a token returned by a previous `checkpoint_list` (a JSON string; a bare \
             number is also accepted, for clients written against the older numeric spelling) — {got}"
        )));
    };
    // Range-check through the house's shared counted-field rule so the bound and its error message
    // stay identical to every other paging parameter on this bus.
    hex::parse_count("cursor", &Value::from(n), 0, max)
}

/// The `-32005 unknownCheckpoint` refusal. `data.id` echoes the handle **as the client sent it** — a
/// string, per D9 category 4 — so a client can correlate the refusal with the call that caused it
/// without the server reformatting the one value it is not supposed to interpret.
fn unknown_checkpoint(id: &str) -> RpcError {
    RpcError::invalid_state(
        "unknownCheckpoint",
        // Quoted, because the id is now an arbitrary client-supplied string: unquoted, an id of `""` or
        // `" "` would produce a message with a hole in it.
        format!("no checkpoint {id:?} — it was never taken, or it has been dropped"),
        json!({ "id": id }),
    )
}

fn no_symbols() -> RpcError {
    RpcError::new(
        code::NO_SYMBOLS_LOADED,
        "no symbol table is loaded — call emulator/load_symbols first",
    )
}

/// A [`Layer`] as the `{layer, spriteIndex?}` object both `winner` and each `candidates[]` entry carry.
///
/// The values are camelCase — matching the `runTo`/`runToScanline` stopped-reason spelling (§3) rather
/// than Rust's variant names — and `spriteIndex` is present **if and only if** `layer` is `"sprite"`,
/// which is what makes one helper safe for both sites.
fn layer_json(layer: Layer) -> Value {
    match layer {
        Layer::Backdrop => json!({"layer": "backdrop"}),
        Layer::PlaneB => json!({"layer": "planeB"}),
        Layer::PlaneA => json!({"layer": "planeA"}),
        Layer::Window => json!({"layer": "window"}),
        Layer::Sprite(i) => json!({"layer": "sprite", "spriteIndex": i}),
    }
}

/// The VRAM byte address of pattern `tile`, wrapped into VRAM exactly as the core's tile addressing does.
/// A pattern is 32 bytes and 65536 is a multiple of 32, so a pattern never straddles the wrap.
fn tile_addr(tile: u16) -> u32 {
    (u32::from(tile) * 32) & 0xFFFF
}

fn out_of_range(addr: u32, why: &str) -> RpcError {
    RpcError::new(
        code::ADDRESS_OUT_OF_RANGE,
        format!("{}: {why}", hex::addr(addr)),
    )
    .with_data(json!({"addr": hex::addr(addr)}))
}

fn binding_name(b: &RomBinding) -> &'static str {
    match b {
        RomBinding::Match { .. } => "match",
        RomBinding::Mismatch(_) => "mismatch",
        RomBinding::Indeterminate(_) => "indeterminate",
    }
}

fn describe_fault(f: BindingFault) -> String {
    match f {
        BindingFault::EndOfRomOutOfRange {
            end_of_rom,
            rom_len,
        } => format!(
            "its EndOfRom ({}) is past the end of a {rom_len}-byte image",
            hex::addr(end_of_rom)
        ),
        BindingFault::NoAppendixMagic { offset, found } => format!(
            "no deb2 symbol appendix at its EndOfRom ({}) — found {:02X} {:02X}",
            hex::addr(offset),
            found[0],
            found[1]
        ),
        BindingFault::AppendixTooSmall { offset, len } => {
            format!("the appendix at {} is only {len} bytes", hex::addr(offset))
        }
    }
}

/// Buttons the 3-button Mega Drive pad the core models actually has.
const BUTTONS_3: &[&str] = &["up", "down", "left", "right", "a", "b", "c", "start"];
/// The 6-button additions listed in `protocol.md` §6. The core does not model a 6-button pad, so these
/// are refused by name rather than silently ignored — a silently-ignored button is a test that "passes"
/// while pressing nothing (the sibling's *"the `c` button never registers"*, recon §1c).
const BUTTONS_6: &[&str] = &["x", "y", "z", "mode"];

fn parse_buttons(params: &Value) -> Result<Vec<String>, RpcError> {
    let Some(v) = params.get("buttons") else {
        return Err(RpcError::invalid_params(
            "`buttons` (array of strings) is required",
        ));
    };
    let Some(arr) = v.as_array() else {
        return Err(RpcError::invalid_params(
            "`buttons` must be an array of strings",
        ));
    };
    if arr.len() > BUTTONS_3.len() {
        return Err(RpcError::invalid_params(format!(
            "`buttons` has {} entries; a 3-button pad has {}",
            arr.len(),
            BUTTONS_3.len()
        )));
    }
    let mut out = Vec::with_capacity(arr.len());
    for b in arr {
        let Some(name) = b.as_str() else {
            return Err(RpcError::invalid_params(
                "`buttons` must be an array of strings",
            ));
        };
        let lower = name.to_ascii_lowercase();
        if BUTTONS_6.contains(&lower.as_str()) {
            return Err(RpcError::invalid_params(format!(
                "\"{name}\" is a 6-button pad button; this core models a 3-button pad only \
                 (capability `sixButtonPad` is false)"
            ))
            .with_data(json!({"button": lower, "supported": BUTTONS_3})));
        }
        if !BUTTONS_3.contains(&lower.as_str()) {
            return Err(
                RpcError::invalid_params(format!("unknown button \"{name}\""))
                    .with_data(json!({"button": lower, "supported": BUTTONS_3})),
            );
        }
        out.push(lower);
    }
    Ok(out)
}

fn parse_port(params: &Value) -> Result<usize, RpcError> {
    match params.get("port") {
        None => Ok(0),
        Some(v) => Ok(hex::parse_count("port", v, 0, 1)? as usize),
    }
}

fn set_button(pad: &mut Pad, name: &str, down: bool) {
    match name {
        "up" => pad.up = down,
        "down" => pad.down = down,
        "left" => pad.left = down,
        "right" => pad.right = down,
        "a" => pad.a = down,
        "b" => pad.b = down,
        "c" => pad.c = down,
        "start" => pad.start = down,
        _ => unreachable!("parse_buttons rejects everything else"),
    }
}

/// Per-button OR of two pads. See [`Engine::apply_pads`] for why the merge is an OR and not a precedence
/// rule.
pub fn merge_pads(a: Pad, b: Pad) -> Pad {
    Pad {
        up: a.up || b.up,
        down: a.down || b.down,
        left: a.left || b.left,
        right: a.right || b.right,
        a: a.a || b.a,
        b: a.b || b.b,
        c: a.c || b.c,
        start: a.start || b.start,
    }
}

/// Read the most recently **completed** frame out of a [`Retain::LastFrame`] capture into `slot`, and report
/// whether there was one to take.
///
/// This is the same reader the window uses (`oracle-frontend`'s `blit_capture`), down to the two subtleties
/// that are not obvious and are both load-bearing:
///
/// * **The sum check is what proves the frame is the one just drawn.** A run that ends mid-frame leaves the
///   *previous* frame in `pixels()`, whose lines are no longer the tail of the delivery log; without the
///   check a torn run would hand back a frame stitched from two different geometries.
/// * **A frame is not guaranteed rectangular.** A game can switch H32↔H40 part-way down (S3K does exactly
///   that on the first frame after a soft reset), so the width is the width the frame *ended* on — what the
///   VDP is actually scanning out by V-Blank — and short lines are padded with black to reach it.
///
/// Written in place because it runs once per emulated frame on a free-running server: reusing the slot's
/// `Vec` makes the steady state a memcpy rather than a fresh 215 KB allocation and free every 16.7 ms. `slot`
/// is left completely untouched when there is nothing to take — every rejection above happens before the
/// first write — so a caller keeps presenting the frame it already had.
fn store_from_capture(slot: &mut Option<CapturedFrame>, cap: &ScanlineCapture) -> bool {
    let px = cap.pixels();
    let log = cap.lines();
    let height = ACTIVE_LINES as usize;
    if px.is_empty() || log.len() < height {
        return false;
    }
    let widths = &log[log.len() - height..];
    if widths.iter().map(|&(_, w)| w).sum::<usize>() != px.len() {
        return false;
    }
    let width = widths[height - 1].1;
    if width == 0 {
        return false;
    }
    let f = slot.get_or_insert_with(|| CapturedFrame {
        width,
        rgb: Vec::with_capacity(width * height),
    });
    f.width = width;
    f.rgb.clear();
    let mut at = 0;
    for &(_, line_width) in widths {
        let line = &px[at..at + line_width];
        at += line_width;
        for x in 0..width {
            f.rgb.push(line.get(x).copied().unwrap_or((0, 0, 0)));
        }
    }
    true
}

fn held_names(pad: &Pad) -> Vec<&'static str> {
    let mut v = Vec::new();
    for (name, on) in [
        ("up", pad.up),
        ("down", pad.down),
        ("left", pad.left),
        ("right", pad.right),
        ("a", pad.a),
        ("b", pad.b),
        ("c", pad.c),
        ("start", pad.start),
    ] {
        if on {
            v.push(name);
        }
    }
    v
}
