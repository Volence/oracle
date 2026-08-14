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
use oracle_core::io::Pad;
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
    /// The live checkpoints (§6.1, D13). In memory, per **server session**, capped by
    /// [`EngineConfig::max_checkpoints`], and owned by the engine rather than by a connection — the
    /// coordinates belong to the machine, so two clients on one bus see one set.
    checkpoints: Vec<Checkpoint>,
    /// Monotonic id source. Never rewound, never reused (see [`Checkpoint`]).
    next_checkpoint_id: u64,
}

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
            checkpoints: Vec::new(),
            next_checkpoint_id: 1,
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Whether the server loop should keep stepping the machine on its own (free-run mode).
    pub fn is_running(&self) -> bool {
        self.free_run
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
        self.sys.run_frames(1);
        self.config.free_run_pace
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

    /// The active display as a row-major RGB framebuffer, plus its width.
    fn framebuffer(&self) -> (usize, Vec<(u8, u8, u8)>) {
        let width = self.sys.vdp().render_line(0).len();
        let mut fb = Vec::with_capacity(width * ACTIVE_LINES as usize);
        for line in 0..ACTIVE_LINES {
            fb.extend_from_slice(&self.sys.vdp().render_line(line));
        }
        (width, fb)
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
        self.sys.run_frames(frames);
        self.running = false;
        let pc = self.sys.cpu_regs().pc;
        // CR-1: §3's `reason` enum has no value for "a bounded frame advance completed", so this reports
        // the nearest listed reason and puts the precise one in an additive field.
        let mut extra = Map::new();
        extra.insert("frames".into(), json!(frames));
        extra.insert("deadlineReached".into(), json!(true));
        self.emit_stopped("step", pc, extra);
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
        let record = self.sys.run_until_stop(max_frames, |pc, _| pc == target);
        self.running = false;
        let mut extra = Map::new();
        extra.insert("target".into(), json!(hex::addr(target)));
        extra.insert("deadlineReached".into(), json!(record.timed_out()));
        self.emit_stopped("runTo", record.pc, extra);

        let mut out = json!({
            "target": hex::addr(target),
            "reached": record.fired(),
            "pc": hex::addr(record.pc),
            "stoppedAtFrame": record.frame,
            "stoppedAtMclk": record.mclk,
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

    fn pause(&mut self, _params: &Value) -> Result<Value, RpcError> {
        let was = self.free_run;
        self.free_run = false;
        self.running = false;
        if was {
            self.emit_stopped("pause", self.sys.cpu_regs().pc, Map::new());
        }
        Ok(json!({"wasRunning": was}))
    }

    fn resume(&mut self, _params: &Value) -> Result<Value, RpcError> {
        let was = self.free_run;
        self.free_run = true;
        self.running = true;
        if !was {
            self.emit_resumed();
        }
        Ok(json!({"wasRunning": was}))
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
            let (_, fb) = self.framebuffer();
            let mut bytes = Vec::with_capacity(fb.len() * 3);
            for (r, g, b) in fb {
                bytes.extend_from_slice(&[r, g, b]);
            }
            out["framebuffer"] = json!(hex_of(oracle_core::state_hash::fnv1a_bytes(&bytes)));
        }
        Ok(out)
    }

    fn screenshot(&mut self, params: &Value) -> Result<Value, RpcError> {
        let path: PathBuf = match params.get("path") {
            None => std::env::temp_dir().join(format!("oracle-frame-{}.ppm", self.frame())),
            Some(Value::String(s)) => PathBuf::from(s),
            Some(_) => return Err(RpcError::invalid_params("`path` must be a string")),
        };
        let (width, fb) = self.framebuffer();
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
        Ok(json!({
            "path": path.display().to_string(),
            "format": "ppm",
            "width": width,
            "height": ACTIVE_LINES,
            "bytes": bytes.len(),
            "caveat": "the whole frame is rendered from the VDP state as it is right now. Mid-frame \
                       CRAM/scroll changes that a real raster would show on different lines are NOT \
                       reproduced — this is a state render, not a scanline-accurate capture.",
        }))
    }

    fn press(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/press")?;
        let port = parse_port(params)?;
        let buttons = parse_buttons(params)?;
        let frames = match params.get("frames") {
            None => 2,
            Some(v) => hex::parse_count("frames", v, 1, 1000)?,
        };
        let mut pad = self.held[port];
        for b in &buttons {
            set_button(&mut pad, b, true);
        }
        self.sys.set_pad(port, pad);
        self.running = true;
        self.emit_resumed();
        self.sys.run_frames(frames);
        self.running = false;
        // Restore exactly the held set — a tap must not leak into later frames, and must not clear a
        // button the client is separately holding.
        self.sys.set_pad(port, self.held[port]);
        let pc = self.sys.cpu_regs().pc;
        let mut extra = Map::new();
        extra.insert("frames".into(), json!(frames));
        extra.insert("deadlineReached".into(), json!(true));
        self.emit_stopped("step", pc, extra);
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
        self.sys.set_pad(port, self.held[port]);
        Ok(json!({
            "buttons": buttons,
            "down": down,
            "port": port,
            "held": held_names(&self.held[port]),
        }))
    }

    fn release_all(&mut self, _params: &Value) -> Result<Value, RpcError> {
        self.held = [Pad::default(); 2];
        self.sys.set_pad(0, Pad::default());
        self.sys.set_pad(1, Pad::default());
        Ok(json!({"released": true}))
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
        self.checkpoints.push(Checkpoint {
            id,
            label,
            frame: mclk / MCLK_PER_FRAME,
            mclk,
            snapshot,
            held: self.held,
            rom_path: self.rom_path.clone(),
            symbols: self.symbols.clone(),
            symbols_path: self.symbols_path.clone(),
        });
        // `frame`/`mclk` are deliberately absent: §6.1 says they **are** the machine stamp, and the stamp
        // is merged in structurally after this returns (`rpc::stamp_result`). Emitting them here would be
        // a shadowed duplicate that the envelope overwrites with the identical values anyway — nothing
        // has advanced the machine between the capture above and the stamp.
        Ok(json!({"id": id, "bytes": bytes}))
    }

    fn restore(&mut self, params: &Value) -> Result<Value, RpcError> {
        let id = parse_checkpoint_id(params)?;
        let Some(cp) = self.checkpoints.iter().find(|c| c.id == id) else {
            // D13 rule 4: an unknown or already-dropped id is refused, never a silent no-op. A no-op here
            // would leave the caller running its next experiment against whatever machine happened to be
            // loaded, believing it had gone back.
            return Err(unknown_checkpoint(id));
        };

        // D13 rule 2: the ENTIRE machine — CPU, RAM, VDP, sound state and the ROM. A checkpoint taken
        // before an `emulator/reload_rom` therefore brings the previous cartridge back; that is defined
        // behaviour, not an error case.
        let sys = System::restore(&cp.snapshot).map_err(|e| {
            RpcError::new(
                code::INTERNAL_ERROR,
                format!("checkpoint {id} could not be decoded: {e}"),
            )
            .with_data(json!({"id": id}))
        })?;
        let (held, rom_path) = (cp.held, cp.rom_path.clone());
        let (symbols, symbols_path) = (cp.symbols.clone(), cp.symbols_path.clone());
        // Nothing is applied until the decode has succeeded — a half-applied restore is exactly what
        // "MUST NOT partially restore" rules out.
        self.sys = sys;
        self.held = held;
        self.rom_path = rom_path;
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
            Some(v) => hex::parse_count("cursor", v, 0, highest_issued)?,
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
        let items: Vec<Value> = self
            .checkpoints
            .iter()
            .filter(|c| c.id > cursor)
            .take(limit)
            .map(|c| {
                let mut e = json!({
                    "id": c.id,
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

        // The continuation token is the last id on this page. Taken *before* the items are handed to
        // the bounded-array helper, which consumes them.
        let next_cursor = items
            .last()
            .and_then(|e| e["id"].as_u64())
            .unwrap_or(cursor);

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
        if page["nextCursor"].is_u64() {
            out.insert("cursor".into(), json!(next_cursor));
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
        self.checkpoints.retain(|c| c.id != id);
        // Unlike `restore`, dropping an id that is already gone is answered rather than refused: `removed`
        // is the count that actually went (§6.1), and `0` is a complete, machine-readable answer to
        // "is it gone?" — the caller's intent is satisfied either way. The hazard D13 rule 4 names is a
        // `restore` that succeeds against a machine the client did not ask for, which has no analogue
        // here: nothing was restored, nothing was evicted, and no id changed meaning.
        Ok(json!({"removed": before - self.checkpoints.len()}))
    }
}

// -------------------------------------------------------------------- free functions

/// A checkpoint `id`: a **JSON number** per D9 (an id is a slot handle, not an address), required.
fn parse_checkpoint_id(params: &Value) -> Result<u64, RpcError> {
    let Some(v) = params.get("id") else {
        return Err(RpcError::invalid_params("`id` (a JSON number) is required"));
    };
    hex::parse_count("id", v, 0, u64::MAX)
}

fn unknown_checkpoint(id: u64) -> RpcError {
    RpcError::invalid_state(
        "unknownCheckpoint",
        format!("no checkpoint {id} — it was never taken, or it has been dropped"),
        json!({ "id": id }),
    )
}

fn no_symbols() -> RpcError {
    RpcError::new(
        code::NO_SYMBOLS_LOADED,
        "no symbol table is loaded — call emulator/load_symbols first",
    )
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
