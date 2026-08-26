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
//!
//! The four **watchpoint** methods (§6, §11.8 — CR-11 and CR-12) arrived the same way on 2026-08-15: §6's
//! single `watchpoint_add` row became four, and `capabilities.watchpoints` carries the spaces, the watch
//! cap and the hit-ring depth for the same reason `checkpoints` carries its own. The instrument they expose
//! is engine-owned and **lent** — see [`Engine::watchpoints`], which is the one place the hosted
//! arrangement's two-run-drivers problem is answered.

use crate::build_info;
use crate::hex;
use crate::outbound::Subscribers;
use crate::rpc::{self, code, RpcError};
use oracle_core::bus::{BusEvent, BusEventSink, Fanout, Observe, StepRetire, StopWhen};
use oracle_core::io::Pad;
// The 68000's own bus trait, brought in for `emulator/write_memory`: a poke travels the same `write8`
// the CPU drives, so the hardware mirror masking and the region decode are the machine's, not ours.
use oracle_core::m68000::bus68k::Bus68k;
// The control-flow classifier and the return-frame sizes, for the `step*` shadow stack. Both are pure
// functions of the opcode word, and both are the profiler's — a second copy of either is a mirror that can
// drift while both halves look right.
use oracle_core::m68000::decode::{control_flow_of, return_pop_bytes, ControlFlow};
use oracle_core::profiler::{CallerKey, Counts, EdgeCounts, Profiler};
use oracle_core::render::{CandidateVerdict, Layer, PixelState};
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::symbols::{BindingFault, Indeterminate, RomBinding, SymbolTable};
use oracle_core::system::{
    StopRecord, System, TimingBasis, MCLK_PER_CPU_CYCLE, MCLK_PER_FRAME, RAM_SIZE,
};
// The frame's line count, for `emulator/run_to_scanline`'s unreachable-target caveat. Read from the VDP's
// own constant rather than written down here: 262 is a property of the machine, and a second copy of it
// would be a number that looks authoritative while the timing basis moved underneath it.
use oracle_core::vdp::LINES_PER_FRAME;
use oracle_core::watchpoints::{
    CensusKey, Stamp, Watch, WatchHit, WatchId, WatchMode, WatchOp, WatchReport, WatchSpace,
    WatchVia, Watchpoints,
};
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
/// Function code for the debug poke path: supervisor data, matching what the replay runner arms with.
const FC_SUPERVISOR_DATA: u8 = 5;
/// Active display height in lines (the region `render_line` covers).
const ACTIVE_LINES: u16 = 224;
/// The largest `line` `emulator/run_to_scanline` accepts — **the contract's number, not this core's**.
///
/// §6's row spells the span `0-511`, deliberately wider than `emulator/scanlines`' 0-223 because a raster
/// target may legitimately sit in blanking; the fragment's `maximum` transcribes it. It is *not* a video
/// mode: this core runs [`LINES_PER_FRAME`](oracle_core::vdp::LINES_PER_FRAME) = 262 lines, so 262-511 are
/// accepted and answered `reached: false` with a caveat rather than refused — see
/// [`Engine::run_to_scanline`]. `tests/run_to_scanline.rs` re-derives this bound by parsing the vendored
/// fragment, so a contract that widens or narrows it cannot leave this constant behind.
const MAX_SCANLINE_TARGET: u64 = 511;

/// Tunables. Every bound here is a **loud refusal** when exceeded, never a silent clamp: a clamped `len`
/// returns fewer bytes than asked for and the caller has no way to notice.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    /// Ceiling for one `emulator/run_frames` / `emulator/run_to` call. A single request must not be able
    /// to monopolise the emulator thread for an unbounded time — every other client is queued behind it.
    pub max_run_frames: u64,
    /// Ceiling for one memory/VRAM read, per `protocol.md` §6 (`len`? ≤ 4096).
    pub max_read_len: u64,
    /// Ceiling for one `emulator/write_memory` payload, per `protocol.md` §6 / §11.13. Advertised as
    /// `limits.maxWriteLen`, which the schema makes REQUIRED for any server advertising the method — and
    /// over it the poke is **refused, never truncated**: a truncating write reports a success the caller
    /// cannot distinguish from a complete one.
    pub max_write_len: u64,
    /// Ceiling for one `emulator/memory_hash` range, per `protocol.md` §6 / §11.13. Advertised as
    /// `limits.maxHashLen`. Far larger than `max_read_len` on purpose: a hash returns a fixed-size answer,
    /// so the ceiling here bounds work rather than reply size, and hashing a whole 4 MiB cartridge window
    /// is the row's headline use.
    pub max_hash_len: u64,
    /// Ceiling on `emulator/get_profiler_frames`' `top` — the most expensive routine rows it will return
    /// in one reply (`protocol.md` §6, §11.16). Advertised as `limits.maxProfilerRoutines` and **refused,
    /// never clamped**: the legacy surface clamped, so a caller could not tell a full list from a clipped
    /// one.
    pub max_profiler_routines: usize,
    /// Depth of the opt-in per-frame ring, and the ceiling on `get_profiler_frames`' `frames`
    /// (`protocol.md` §6, §11.16). Advertised as `limits.maxProfilerFrames`. A ring, so a profiler left
    /// armed across a long session keeps the frames nearest the symptom rather than growing without bound.
    pub max_profiler_frames: usize,
    /// Ceiling on `emulator/get_profiler_frames`' `topCallers` — the most expensive **call edges** it will
    /// return per routine row (`protocol.md` §6, §11.18). Advertised as `limits.maxProfilerCallers`, whose
    /// **presence is the capability signal** for the caller lens, and refused above rather than clamped.
    ///
    /// **A reply bound, not a retention bound**, and the distinction is the difference between a true count
    /// and a misleading one: the accumulator keeps every observed edge and this decides how many are
    /// *sent*, which is what makes a row's `callersTotal` the number of distinct callers rather than the
    /// number that survived a ceiling. `max_profiler_frames` is the opposite — a real ring depth, where
    /// rows beyond it are gone.
    pub max_profiler_callers: usize,
    /// Ceiling for one `emulator/play_input` timeline (`protocol.md` §6, §11.11). Advertised as
    /// `limits.maxInputRows` because a client that must hit a limit to learn it loses the work it was
    /// doing when it found out.
    pub max_input_rows: usize,
    /// Ceiling on `otherMatches` in a symbol lookup, per `protocol.md` §4 ("up to 5").
    pub max_symbol_matches: usize,
    /// **The checkpoint cap** (`protocol.md` §6.1, D13 rule 3), advertised in `initialize` as
    /// `capabilities.checkpoints.cap`. At the cap `emulator/checkpoint` **refuses** with `-32005`; it
    /// never evicts the oldest, because an id a client is still holding must never quietly start meaning
    /// nothing. Doubles as the default and maximum page size for `emulator/checkpoint_list` — there can
    /// never be more live checkpoints than this, so a bigger page could not return more.
    pub max_checkpoints: usize,
    /// **The watch cap** (`protocol.md` §6, D13 rule 3 applied to watchpoints), advertised in `initialize`
    /// as `capabilities.watchpoints.maxWatches`. At the cap `emulator/watchpoint_add` **refuses** with
    /// `-32005 {reason:"watchCapReached", cap, count}`; it never silently grows past it and never evicts.
    ///
    /// The reason is sharper here than for checkpoints: a silently-dropped watch produces a `seen`-positive,
    /// `matched`-zero hits read, which is **indistinguishable from a genuine negative finding** — the one
    /// failure this whole instrument exists to make impossible.
    pub max_watches: usize,
    /// Capacity of the shared hit ring, in hits — `capabilities.watchpoints.ringCap`. Past it the recorder
    /// drops oldest-first and counts the loss in `watchpoint_hits.dropped`. Advertised *before* a client
    /// plans around it, for D13's reason: a client sweeping a hot range needs the number in advance, not
    /// after losing evidence to it.
    pub watch_ring_cap: usize,
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
            // The schema's own `len` maximum for the row (4096 bytes), which is also the house read
            // ceiling: a poke and a read-back should be able to name the same length.
            max_write_len: 4096,
            // The schema's own `len` maximum for `emulator/memory_hash` (4 MiB) — one whole cartridge
            // window in a single call.
            max_hash_len: 4_194_304,
            // Enough that a real ROM's hot set arrives whole — a Sonic-scale frame touches a few hundred
            // routines — while still bounding one reply.
            max_profiler_routines: 512,
            // Two seconds of NTSC frames: long enough to see a stutter in context, short enough that the
            // ring is a rounding error next to the machine it profiles.
            max_profiler_frames: 120,
            // A routine's distinct callers are a far smaller set than the routines themselves — a hot leaf
            // is reached from a handful of sites, not from hundreds — so this is sized to carry a real
            // row's edge list WHOLE rather than to page it, which is what keeps `callersTruncated: false`
            // the ordinary case and the two normative sums assertable with `==`.
            max_profiler_callers: 64,
            max_input_rows: 256,
            max_symbol_matches: 5,
            // The contract's own advertised example (`"checkpoints":{"supported":true,"cap":8}`). A
            // snapshot is the whole machine, so the cap is a memory bound as much as a policy one.
            max_checkpoints: 8,
            // The contract's own advertised example for this capability
            // (`"watchpoints":{"supported":true,"maxWatches":32,…}`). A watch is a small struct plus its
            // census map, so 32 is a policy bound rather than a memory one — but it is refused at, loudly.
            max_watches: 32,
            // 4,096 hits. Sized against the measured volumes this instrument exists for — a single test ROM
            // writes 4,923,206 CRAM words over 120 frames — where the honest answer is never "the ring held
            // it all" but "the ring held the tail and `dropped` says how much it did not".
            watch_ring_cap: 4096,
            free_run_pace: Some(Duration::from_micros(16_667)),
            server_name: "oracle-next".into(),
            server_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// One method: its wire name, its handler, a one-line summary reported by `initialize`, and the set of
/// top-level `params` keys it accepts.
pub struct MethodSpec {
    pub name: &'static str,
    pub handler: fn(&mut Engine, &Value) -> Result<Value, RpcError>,
    pub summary: &'static str,
    /// **The closed top-level params key set** (`protocol.md` §2.5 / §8 item 22, added 2026-08-19 by
    /// §11.17). [`Engine::dispatch`] refuses any request carrying a key that is not here, with `-32602`,
    /// **before the handler runs** — so a write refused for an unknown param has written nothing.
    ///
    /// This list is not hand-maintained prose: the schema fragment is the wire authority (D14), and
    /// `tests/params_closure.rs::every_advertised_method_declares_exactly_its_fragments_params` derives
    /// each set by **parsing** the vendored schema and compares. A key added to a fragment without being
    /// added here — or the reverse — turns that suite red, which is what keeps the two from drifting the
    /// way §11.10's founding count defect did. The initial 37 rows were *generated* from the schema for
    /// the same reason: the table starts life equal to the authority rather than transcribed from it.
    pub params: &'static [&'static str],
}

/// **The dispatch table and the advertised method list, as one object.** Every name here is a
/// `protocol.md` §6 catalog entry verbatim.
pub const METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: "emulator/set_profiler",
        handler: Engine::set_profiler,
        summary: "arm or disarm the per-invocation cycle accountant (arming resets it)",
        params: &["callers", "enabled", "perFrame"],
    },
    MethodSpec {
        name: "emulator/get_profiler",
        handler: Engine::get_profiler,
        summary: "the accountant's state: armed, frames recorded, rows accumulated",
        params: &[],
    },
    MethodSpec {
        name: "emulator/get_profiler_frames",
        handler: Engine::get_profiler_frames,
        summary: "the accumulated sample: per-routine rows, interrupt buckets by cause, per-frame ring",
        params: &["frames", "top", "topCallers"],
    },
    MethodSpec {
        name: "emulator/status",
        handler: Engine::status,
        summary: "run state, PC/SP/SR, symbol at PC, loaded ROM",
        params: &[],
    },
    MethodSpec {
        name: "emulator/registers",
        handler: Engine::registers,
        summary: "the 68000 architectural register file",
        params: &[],
    },
    // The three `step*` rows (§6 lines 851-853), in catalog order. All three require a paused machine
    // through §6's run-control state rule, and all three emit `stopped` with `reason: "step"` — §3 pins one
    // stop condition across the three, so `step_over` and `step_out` get no reason of their own.
    MethodSpec {
        name: "emulator/step",
        handler: Engine::step,
        summary: "retire N instructions, then stop (emits resumed + stopped)",
        params: &["count"],
    },
    MethodSpec {
        name: "emulator/step_over",
        handler: Engine::step_over,
        summary: "step one instruction, running any call it makes to completion (emits resumed + stopped)",
        params: &[],
    },
    MethodSpec {
        name: "emulator/step_out",
        handler: Engine::step_out,
        summary: "run until the current subroutine returns (emits resumed + stopped)",
        params: &[],
    },
    MethodSpec {
        name: "emulator/run_frames",
        handler: Engine::run_frames,
        summary: "advance N whole frames, then stop (emits resumed + stopped)",
        params: &["frames"],
    },
    MethodSpec {
        name: "emulator/run_to",
        handler: Engine::run_to,
        summary: "run until PC reaches an address or symbol, bounded (emits resumed + stopped)",
        params: &["addr", "maxFrames", "symbol"],
    },
    MethodSpec {
        name: "emulator/run_to_scanline",
        handler: Engine::run_to_scanline,
        summary: "run until the raster reaches a scanline, bounded (emits resumed + stopped)",
        params: &["line", "maxFrames"],
    },
    MethodSpec {
        name: "emulator/pause",
        handler: Engine::pause,
        summary: "leave free-running mode (emits stopped)",
        params: &[],
    },
    MethodSpec {
        name: "emulator/resume",
        handler: Engine::resume,
        summary: "enter free-running mode (emits resumed)",
        params: &[],
    },
    MethodSpec {
        name: "emulator/checkpoint",
        handler: Engine::checkpoint,
        summary: "capture the whole machine into a volatile in-memory slot and return its server-assigned id",
        params: &["label"],
    },
    MethodSpec {
        name: "emulator/restore",
        handler: Engine::restore,
        summary: "restore the entire machine, ROM included, from a checkpoint",
        params: &["id"],
    },
    MethodSpec {
        name: "emulator/checkpoint_list",
        handler: Engine::checkpoint_list,
        summary: "the live checkpoints, bounded and cursored",
        params: &["cursor", "limit"],
    },
    MethodSpec {
        name: "emulator/checkpoint_drop",
        handler: Engine::checkpoint_drop,
        summary: "drop one checkpoint by id, or all of them, and report how many went",
        params: &["all", "id"],
    },
    MethodSpec {
        name: "emulator/watchpoint_add",
        handler: Engine::watchpoint_add,
        summary: "arm a recording watch over an address range in one of the four spaces, and return its handle",
        params: &["addr", "censusKey", "label", "len", "mode", "read", "space", "stopAfter", "symbol", "write"],
    },
    MethodSpec {
        name: "emulator/watchpoint_clear",
        handler: Engine::watchpoint_clear,
        summary: "retire one watch by handle, or all of them, and report how many went",
        params: &["all", "watch"],
    },
    MethodSpec {
        name: "emulator/watchpoint_list",
        handler: Engine::watchpoint_list,
        summary: "the armed watches and what each has observed, bounded and cursored",
        params: &["cursor", "limit"],
    },
    MethodSpec {
        name: "emulator/watchpoint_hits",
        handler: Engine::watchpoint_hits,
        summary: "the recorded hit log — polled, non-destructive, with dropped/seen/matched beside it",
        params: &["cursor", "limit", "watch"],
    },
    MethodSpec {
        name: "emulator/read",
        handler: Engine::read,
        summary: "one byte read across the bus/vram/cram/vsram spaces — the read half of the watch surface",
        params: &["addr", "len", "space", "symbol"],
    },
    MethodSpec {
        name: "emulator/read_memory",
        handler: Engine::read_memory,
        summary: "debug read of ROM or work RAM by address or symbol",
        params: &["addr", "len", "symbol"],
    },
    MethodSpec {
        name: "emulator/read_vram",
        handler: Engine::read_vram,
        summary: "debug read of VDP VRAM",
        params: &["addr", "len"],
    },
    MethodSpec {
        name: "emulator/read_cram",
        handler: Engine::read_cram,
        summary: "the palette as STORED: one line's 16 entries or all 64, with the cramAddr join key",
        params: &["line"],
    },
    MethodSpec {
        name: "emulator/write_cram",
        handler: Engine::write_cram,
        summary: "poke one palette entry (paused machine only; one colour spelling, refused never masked)",
        params: &["b", "g", "index", "line", "r", "raw"],
    },
    MethodSpec {
        name: "emulator/pixel_attribution",
        handler: Engine::pixel_attribution,
        // Not "the losing candidates", which is the 2026-07-01 design doc's phrase and outlived the
        // shape it described: `candidates` carries EVERY layer that could have shown, the winner
        // included and marked `verdict: "won"` — a blanked dot returns exactly one entry, which is
        // the winner. Both the normative schema's own description and
        // `tests/pixel_attribution.rs`'s `cands[0]["verdict"] == "won"` say so.
        summary: "why the dot at (x,y) is the colour it is: winner, cell/sprite, and every candidate layer's verdict",
        params: &["x", "y"],
    },
    MethodSpec {
        name: "emulator/sprites",
        handler: Engine::sprites,
        summary: "the sprite attribute table in slot order, with the parse cap and the stale-cache flag",
        params: &["limit"],
    },
    MethodSpec {
        name: "emulator/state_hash",
        handler: Engine::state_hash,
        summary: "FNV-1a fingerprints of the VDP state regions",
        params: &["includeFramebuffer"],
    },
    MethodSpec {
        name: "emulator/screenshot",
        handler: Engine::screenshot,
        // The summary names PNG because the handler writes PNG, and that pairing is held by
        // `handshake.rs::a_summary_that_names_a_format_must_name_the_format_the_reply_returns`
        // rather than by anyone remembering. It said "binary PPM" for six days after the encoder
        // changed — the change was written down in a comment beside the encoder, which is the one
        // place a wire-facing claim is guaranteed not to be re-read.
        summary: "write the active display to a PNG file, and reply with its path and size",
        params: &["path"],
    },
    MethodSpec {
        name: "emulator/press",
        handler: Engine::press,
        summary: "tap buttons for N frames, then restore the held set",
        params: &["buttons", "frames", "port"],
    },
    MethodSpec {
        name: "emulator/play_input",
        handler: Engine::play_input,
        summary: "play a pad timeline: the pad each frame is a pure function of the rows, nothing else",
        params: &["maxFrames", "rows"],
    },
    MethodSpec {
        name: "emulator/hold",
        handler: Engine::hold,
        summary: "set or clear buttons in the held set (set semantics, never additive)",
        params: &["buttons", "down", "port"],
    },
    MethodSpec {
        name: "emulator/release_all",
        handler: Engine::release_all,
        summary: "clear the held set on both pads",
        params: &[],
    },
    MethodSpec {
        name: "emulator/lookup_symbol",
        handler: Engine::lookup_symbol,
        summary: "name -> address, or address -> nearest preceding label + displacement",
        params: &["addr", "name"],
    },
    MethodSpec {
        name: "emulator/load_symbols",
        handler: Engine::load_symbols,
        summary: "load a sigil/AS .lst listing, refusing one that does not bind to the loaded ROM",
        params: &["path"],
    },
    MethodSpec {
        name: "emulator/reload_rom",
        handler: Engine::reload_rom,
        summary: "reload the ROM from disk and reset (emits romReloaded)",
        params: &["path"],
    },
    MethodSpec {
        name: "emulator/write_memory",
        handler: Engine::write_memory,
        summary: "poke bytes into the work-RAM window (paused machine only; refused never clipped)",
        params: &["addr", "bytes", "disp", "symbol", "value", "width"],
    },
    MethodSpec {
        name: "emulator/reset",
        handler: Engine::reset,
        summary: "drive the /RESET sequence — back to the power-on anchor, SRAM and symbols kept",
        params: &[],
    },
    MethodSpec {
        name: "emulator/memory_hash",
        handler: Engine::memory_hash,
        summary: "fingerprint a byte range (FNV-1a-64 + CRC-32) without moving it — the hash state_hash cannot give",
        params: &["addr", "len", "symbol"],
    },
    MethodSpec {
        name: "emulator/scanlines",
        handler: Engine::scanlines,
        summary: "read the drawn rows back — the live raster when a frame is retained, and the reply says which",
        params: &["count", "startLine"],
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
    /// **The watchpoint instrument (§6, CR-11/CR-12), owned here and lent out.**
    ///
    /// Engine-owned because the standalone server drives its own runs and has nowhere else to put it; lent
    /// out through [`watchpoints_mut`](Engine::watchpoints_mut) because in the **hosted** arrangement the
    /// player owns the run loop and this engine only borrows the machine inside `Host::pump`. There are two
    /// run drivers, and an instrument attached to only one of them sees nothing while the other runs —
    /// honestly reported as `seen == 0`, and useless. One instrument, both drivers, which is also what makes
    /// the player's panel and the bus's `watchpoint_hits` incapable of disagreeing (contract §8 item 19).
    ///
    /// Not part of `System`, so it survives `swap_system`, `restore` and `reload_rom` untouched: watches are
    /// engine-owned and are **not** auto-cleared (§6), because a watch with `stopAfter` changes how the
    /// machine runs and disarming one nobody asked to disarm is a machine-state change §5 forbids.
    watchpoints: Watchpoints,
    /// **The profiler (§6, CR-26), owned here and attached to every run this engine drives.**
    ///
    /// Engine-owned for the same reason as [`watchpoints`](Engine::watchpoints): there are two run
    /// drivers (this engine's own handlers and, hosted, the player's loop), and an instrument attached to
    /// one of them measures nothing while the other runs. One instrument, both drivers.
    ///
    /// Retained across a disarm, because §11.16 makes disarm a *stop recording*, not a *discard*: a client
    /// arms, runs, disarms and reads. It is arming that resets, and there is no resume in this revision.
    profiler: Profiler,
    /// Whether the profiler rides the runs. Distinct from "has a sample": a disarmed instrument keeps
    /// everything it accumulated and can still be read.
    profiler_armed: bool,
    /// The timing basis at the moment the sample was armed. `budgetPct` is derived from the basis, so a
    /// basis that changed mid-sample makes the derivation ambiguous and the contract requires omitting the
    /// key rather than averaging over the change.
    profiler_basis: Option<TimingBasis>,
    /// How many watch handles this engine has ever issued. Equal to the core facility's own next id — this
    /// engine is its only caller — and used for exactly one thing: telling a handle that was **never
    /// issued** (a typo) from one that was issued and has since been cleared (a retired watch, whose hits
    /// are still legitimately queryable). Never rewound.
    watches_issued: u32,
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

/// What one advancing run did, in the terms its caller has to branch on.
///
/// It exists because a `stopAfter` watch made "the sink asked to stop" ambiguous. Before watchpoints there
/// was exactly one thing in the Fanout that could end a run early, so [`StopRecord::fired`] answered
/// "did my condition happen?" directly. Now two can, and a caller that reads the record alone would report
/// a target as *reached* because an unrelated watch halted the run — the ambiguous-success defect
/// [`StopReason`](oracle_core::system::StopReason) was split in two to prevent, rebuilt one level up.
struct Advanced {
    record: StopRecord,
    /// The `run_to` predicate's own verdict. Always `false` for a plain frame advance, which has none.
    predicate_fired: bool,
    /// The watch whose `stopAfter` ended the run, when that is what ended it (§6: *"the halt always names
    /// its watch"*).
    stopped_by: Option<WatchId>,
}

/// One subroutine frame a step-shaped run watched open, in the only two terms a return is matched on.
///
/// Both fields are load-bearing and neither is sufficient alone — this is the profiler's rule
/// ([`Profiler::close_routine`](oracle_core::profiler::Profiler), `profiler.rs`), reused rather than
/// re-invented: a user routine's frame and a supervisor routine's frame can sit at the *same numeric* stack
/// pointer while having nothing to do with each other, so matching on the pointer alone closes the wrong
/// frame on a coincidence.
#[derive(Clone, Copy, Debug)]
struct OpenFrame {
    /// The active A7 immediately after the `JSR`/`BSR` pushed — i.e. pointing at the return address. The
    /// matching return leaves `entry_sp + return_pop_bytes(opcode)` behind, and **exactly** that.
    entry_sp: u32,
    /// The mode the call retired in. [`StepRetire::sp`](oracle_core::bus::StepRetire) is mode-selected, so
    /// a frame is only comparable to a return taken in the same mode.
    supervisor: bool,
}

/// What a step-shaped run is waiting for.
enum StepGoal {
    /// `emulator/step` — retire this many more instructions, then stop. Counted down on **retires**, never
    /// on step boundaries: `BusEventSink`'s own doc records that the boundary hook fires for an instruction
    /// that does not run on the stopping iteration and fires again for that same PC when the caller
    /// resumes, so a boundary counter is off by one exactly when a caller resumes a step.
    Instructions(u64),
    /// `emulator/step_over` across a `JSR`/`BSR` — stop when the frame that call opens closes again.
    ///
    /// The frame is not known when the run starts: the callee's entry is not decodable from the opcode, and
    /// the entry SP is whatever the push left behind. So the first retire of the run *is* the call, and it
    /// is what seeds [`StepStop::opened`].
    OverCall,
    /// `emulator/step_out` — stop on the first return that closes a frame this run never watched open, i.e.
    /// a return out of the frame that was already live when the caller asked.
    ///
    /// `sp0`/`supervisor` are the machine's own coordinates at that moment. They are a **guard, not the
    /// match**: the frame's entry SP is unknowable from here (however many locals the routine has already
    /// pushed is not recoverable), so what identifies the return is that it matched nothing we opened —
    /// and `sp0` is what stops the `move.l addr,-(sp)` / `rts` dispatch idiom from counting as one.
    OutOfFrame { sp0: u32, supervisor: bool },
}

/// The sink behind the three `step*` rows (§6 lines 851-853) — the run-control methods whose stop condition
/// is *instruction-shaped* rather than a PC (`run_to`) or a frame count (`run_frames`).
///
/// It raises its flag from [`on_step_retire`](oracle_core::bus::BusEventSink::on_step_retire), which is what
/// gives all three the semantics a debugger means by "step": the run ends at the **next** instruction
/// boundary, with the instruction that satisfied the condition fully committed, and the reported `pc` is the
/// instruction about to execute rather than the one that just did.
///
/// **Why it walks a shadow stack instead of comparing stack depth.** A tolerant `sp >= sp0` rule is wrong in
/// both directions and the core already documents why: the `move.l addr,-(sp)` / `rts` dispatch idiom
/// "returns" to a pushed target while leaving the stack exactly where it found it, and an interrupt taken in
/// user mode switches A7 to a different stack entirely, where any numeric comparison against the user stack
/// is meaningless. So a return closes a frame only on the profiler's exact rule — `entry_sp + pop == sp` and
/// the modes agree — and a return that matches nothing closes nothing.
struct StepStop {
    goal: StepGoal,
    /// The frames this run has watched open, innermost last. Empty at the start of every run: it records
    /// what *this* run saw, never the machine's real call stack, which nothing in the core can enumerate.
    opened: Vec<OpenFrame>,
    /// Set when [`opened`](StepStop::opened) hit its cap and a frame went untracked, after which a return
    /// matching nothing is no longer evidence of anything. **Suppresses the stop rather than guessing**: the
    /// run then ends on its frame bound and says so with `deadlineReached`, which is a worse answer than the
    /// right one and a far better answer than a confident wrong `pc`.
    lost_track: bool,
    fired: bool,
}

impl StepStop {
    fn new(goal: StepGoal) -> Self {
        // `count: 0` is a legal request (the fragment's `minimum` is 0) and it means what it says: retire
        // nothing. Firing before the first boundary is how "nothing" is spelled, and it is a real answer —
        // a caller establishing where the machine already is, without moving it.
        let fired = matches!(goal, StepGoal::Instructions(0));
        Self {
            goal,
            opened: Vec::new(),
            lost_track: false,
            fired,
        }
    }

    fn push_frame(&mut self, frame: OpenFrame) {
        if self.opened.len() >= oracle_core::profiler::MAX_DEPTH {
            self.lost_track = true;
            return;
        }
        self.opened.push(frame);
    }

    /// Close the innermost frame this run opened that the return at `sp_after` actually unwinds, if any.
    /// Returns whether one was found.
    ///
    /// Searches the whole stack innermost-first rather than the top alone, for the profiler's reason: one
    /// frame left wedged by a return we could not match would otherwise bury every frame under it forever.
    /// Everything above a match was abandoned by a return this run never saw, and is dropped with it.
    fn close_frame(&mut self, opcode: u16, sp_after: u32, supervisor: bool) -> bool {
        let pop = return_pop_bytes(opcode);
        let Some(idx) = self
            .opened
            .iter()
            .rposition(|f| f.entry_sp.wrapping_add(pop) == sp_after && f.supervisor == supervisor)
        else {
            return false;
        };
        self.opened.truncate(idx);
        true
    }
}

/// The sink behind `emulator/run_to_scanline` (§6 line 855) — the run-control method whose stop condition
/// is *raster-shaped* rather than a PC (`run_to`), a frame count (`run_frames`) or an instruction
/// (`step*`).
///
/// It raises its flag from [`on_line_start`](oracle_core::bus::BusEventSink::on_line_start), which the core
/// delivers for **every** line of the frame — blanking included — and which the run loop pops before it asks
/// [`stop_requested`](oracle_core::bus::BusEventSink::stop_requested). So the run ends with the target
/// line's first instruction not yet executed, and the machine is parked at the top of the line rather than
/// somewhere inside it.
///
/// **The condition is the NEXT start of the line, never "the raster is already there".** `run_to`'s
/// predicate is evaluated at the first step boundary of its own run, so `run_to` at the parked PC fires
/// without advancing; transcribing that here would be wrong, because a line is a *recurring* condition and
/// a PC is a point in a program. A caller stepping frame by frame with the same target — the obvious use,
/// and the one a raster debugger writes first — would get a no-op forever on the second call: the
/// level-versus-edge freeze [`Observe`](oracle_core::bus::Observe) exists to prevent, rebuilt in a handler.
/// Firing on the next line start makes each call advance at most one frame and land at a *reproducible*
/// position, which is the whole point of stopping on a raster coordinate.
struct LineStop {
    /// The wanted line. Held as `u32` because the fragment's range (0-511) is wider than this core's frame
    /// (`LINES_PER_FRAME`), so a target that never matches is a legal request rather than an impossible
    /// value — see [`Engine::run_to_scanline`].
    target: u32,
    fired: bool,
}

impl BusEventSink for LineStop {
    fn on_event(&mut self, _event: BusEvent) {}

    fn on_line_start(&mut self, line: u16, _frame: u64) {
        // Latches: once fired the run is ending, and a second line event in the same drained backlog must
        // not un-fire it.
        self.fired |= u32::from(line) == self.target;
    }

    fn stop_requested(&self) -> bool {
        self.fired
    }
}

impl BusEventSink for StepStop {
    fn on_event(&mut self, _event: BusEvent) {}

    fn on_step_retire(&mut self, r: StepRetire) {
        if self.fired {
            return;
        }
        if let StepGoal::Instructions(remaining) = &mut self.goal {
            // Every retire counts, executed or not. An exception entry retires without running the
            // instruction at `pc`, and a caller who steps into an interrupt has stepped — §3's unit is "one
            // instruction, or one instruction-shaped unit", and the alternative (skip it, keep stepping)
            // would silently run the whole handler for a caller who asked for one step.
            *remaining = remaining.saturating_sub(1);
            self.fired = *remaining == 0;
            return;
        }
        // Everything below classifies the opcode, so it must first know the CPU ran it. On an exception
        // entry, an idle slice, or an aborted instruction the opcode names something that did not execute,
        // and classifying it would open a frame nothing pushed or close one nothing returned from.
        if !r.executed {
            // ... with one exception. `OverCall` was told by the *pending* opcode that this first step is a
            // call; if that step did not execute, the call never happened and there is no frame coming. The
            // honest answer is the step that did happen, not a wait for a return that will never arrive.
            if matches!(self.goal, StepGoal::OverCall) && self.opened.is_empty() {
                self.fired = true;
            }
            return;
        }
        match control_flow_of(r.opcode) {
            ControlFlow::Call => self.push_frame(OpenFrame {
                entry_sp: r.sp,
                supervisor: r.supervisor,
            }),
            ControlFlow::Return => {
                let closed = self.close_frame(r.opcode, r.sp, r.supervisor);
                match &self.goal {
                    StepGoal::OverCall => {
                        // The call this run stepped over is the bottom frame, so the run ends when the
                        // stack it opened drains — not on the first return, which may be a callee's.
                        self.fired = closed && self.opened.is_empty();
                    }
                    StepGoal::OutOfFrame { sp0, supervisor } => {
                        // A return that closed one of our own frames is a nested call coming back, not our
                        // exit. A return that closed nothing left a frame we never saw opened — ours — but
                        // only if it actually unwound past where we started, which is what rules out the
                        // dispatch idiom (it lands exactly on `sp0`, never above it).
                        self.fired = !closed
                            && !self.lost_track
                            && r.supervisor == *supervisor
                            && r.sp > *sp0;
                    }
                    StepGoal::Instructions(_) => unreachable!("returned above"),
                }
            }
            // An `RTE` unwinds an exception frame off the supervisor stack, which is not a frame this run
            // ever opened and not one it can be waiting for. The handler's own calls and returns balance
            // inside it, so ignoring it is what keeps an interrupt from moving the count.
            ControlFlow::InterruptReturn | ControlFlow::Jump | ControlFlow::None => {}
        }
    }

    fn stop_requested(&self) -> bool {
        self.fired
    }
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
        let watchpoints = Watchpoints::new(config.watch_ring_cap);
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
            watchpoints,
            watches_issued: 0,
            profiler: Profiler::new(),
            profiler_armed: false,
            profiler_basis: None,
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
        // **The instrument is fed here, and its stop signal is deliberately dropped** ([`Observe`]).
        //
        // Fed, because free-running is still the machine running: a watch armed by one client while another
        // resumed the bus must observe these frames, or its silence says nothing. Stop-suppressed, because
        // `stopAfter` is a *level* and not an edge — `matched >= n` stays true forever — so honouring it
        // here would end every subsequent free-run step before it began, which is a permanent freeze of a
        // machine nobody asked to pause rather than a stop condition. §6 rules this exact case: a
        // `stopAfter` watch on a free-running machine "is answered by attribution rather than by a gate",
        // and the attribution lands on the runs a client actually bounded.
        let armed = (self.watchpoints.watch_count() > 0).then_some(&mut self.watchpoints);
        // The profiler rides here for the same reason the watch does: free-running is still the machine
        // running, and a sample that skipped these frames would report a per-frame cost for frames it never
        // saw. It has no stop signal of its own, so the `Observe` around it is belt-and-braces rather than
        // load-bearing — but it is the same shape, and a reader should not have to check which halves can
        // reach back into the run.
        let prof = self.profiler_armed.then_some(&mut self.profiler);
        let mut sink = Fanout::new(&mut self.screen, Fanout::new(Observe(armed), Observe(prof)));
        self.sys.run_frames_with_sink(1, &mut sink);
        self.latch_screen();
        self.config.free_run_pace
    }

    // ---------------------------------------------------------------- the screen path

    /// Advance the machine `frames` whole frames **through the screen capture**, then latch whatever frame
    /// the run completed.
    ///
    /// Every advancing path in this engine goes through here or through [`advance_until`](Engine::advance_until)
    /// — that is what makes `emulator/screenshot` scanline-accurate rather than a post-hoc guess.
    /// The watchpoint instrument, borrowed. **The seam the hosted arrangement turns on** — see
    /// [`Engine::watchpoints`] for why there is exactly one of these and why it is not inside `System`.
    ///
    /// Safe to call outside a `Host::pump` drain window, unlike every handler on this type: the instrument
    /// is engine state, not machine state, so it does not answer for the placeholder `System`.
    pub fn watchpoints_mut(&mut self) -> &mut Watchpoints {
        &mut self.watchpoints
    }

    /// **Both run instruments, borrowed for ONE run of a host's own loop.**
    ///
    /// This is the seam the hosted arrangement turns on, and since CR-26 there are two instruments behind
    /// it, not one. There are **two run drivers**: standalone, this engine advances the machine itself and
    /// [`advance`](Engine::advance) attaches both; hosted, the player owns the loop and the engine only
    /// borrows the machine inside [`Host::pump`](crate::host::Host::pump). An instrument attached only to
    /// the engine's own runs therefore sees **nothing** while the player is running the game — the watch
    /// reports `seen == 0` and the profiler reports `frameCount: 0` with no rows, each of them honest ("the
    /// recorder was never attached to the run") and useless, about frames that really happened.
    ///
    /// **Why one call and not two accessors.** One run needs both, and two `&mut self` accessors cannot
    /// both be live in one sink expression — the borrow checker forbids exactly the arrangement the run
    /// requires. Splitting the borrow is only possible here, where the two are separate fields, so the
    /// split happens once, at the bottom, and every layer above forwards the pair verbatim.
    ///
    /// **The arming conditions live here too**, so a host cannot get them subtly wrong: the watch is
    /// attached when the shared instrument holds any watch at all (a watch a socket client armed is as real
    /// as one the panel armed, and asking the instrument how many it holds is the one question that covers
    /// both sources), and the profiler when a client has armed it — which is engine state a host has no
    /// other way to see. An unarmed instrument attached anyway would still count every bus event, which
    /// costs the unarmed path something for nothing and makes `seen > 0` mean less than it should.
    ///
    /// **Both halves are wrapped in [`Observe`], and for the watch that is load-bearing.** A watch armed
    /// with `stopAfter` raises `stop_requested` on a *level* (`matched >= n`, permanently), so a shared
    /// instrument attached bare would end every one of a host's 1-frame runs before it began — a client's
    /// stop condition turned into a frozen window on a machine nobody asked to pause. The observations
    /// still land, so `seen` still means "the recorder rode this run"; only the halt, which belongs to the
    /// runs a client bounded, is dropped. The profiler declares no stop of its own, so its wrapper is
    /// belt-and-braces rather than load-bearing — kept because a reader should not have to check which
    /// halves can reach back into the run.
    ///
    /// Safe to call outside a drain window, like [`watchpoints_mut`](Engine::watchpoints_mut): both are
    /// engine state rather than `System` state, so neither answers for the placeholder machine.
    pub fn run_sinks(
        &mut self,
    ) -> (
        Option<Observe<&mut Watchpoints>>,
        Option<Observe<&mut Profiler>>,
    ) {
        let watch_armed = self.watchpoints.watch_count() > 0;
        let profiler_armed = self.profiler_armed;
        (
            watch_armed.then_some(Observe(&mut self.watchpoints)),
            profiler_armed.then_some(Observe(&mut self.profiler)),
        )
    }

    /// **The read half of [`run_sinks`](Engine::run_sinks)** — both instruments, plus whether the profiler
    /// is armed, for a host that wants to *show* what it has been feeding.
    ///
    /// One call for the same reason `run_sinks` is one call: a caller assembling a per-frame view needs
    /// both borrows live at once, and two separate accessors on `&mut self` cannot be. These are shared
    /// borrows, so this one does not even need `&mut` — which is itself the point, because a panel that
    /// could mutate the instrument it is drawing would be able to move a number a client is gating on.
    ///
    /// The `bool` is the ARMED flag and it is not derivable from the [`Profiler`]: a disarmed instrument
    /// **retains** its sample (§11.16 — arming resets, disarming retains, reading never clears), so an
    /// accumulator holding rows says nothing about whether it is still recording. Reporting the two
    /// separately is what lets a reader tell "measuring now" from "here is what was measured".
    ///
    /// Safe outside a drain window, like [`watchpoints_mut`](Engine::watchpoints_mut): both are engine
    /// state rather than `System` state, so neither answers for the placeholder machine.
    pub fn read_instruments(&self) -> (&Watchpoints, &Profiler, bool) {
        (&self.watchpoints, &self.profiler, self.profiler_armed)
    }

    /// Advance the machine `frames` whole frames through the screen capture **and the watch instrument**,
    /// then latch whatever frame the run completed.
    ///
    /// The instrument rides the same run as the capture, which is what gives a `stopAfter` watch its halt:
    /// `Watchpoints::stop_requested` becomes true mid-step and the run ends at the next instruction
    /// boundary, with the triggering instruction fully committed. Attached only when a watch is actually
    /// registered — an unarmed instrument would still count every bus event into `seen`, which costs the
    /// unarmed path something for nothing and, worse, makes `seen > 0` mean less than it should.
    fn advance(&mut self, frames: u64) -> Advanced {
        let record = {
            let armed = (self.watchpoints.watch_count() > 0).then_some(&mut self.watchpoints);
            let prof = self.profiler_armed.then_some(&mut self.profiler);
            let mut sink = Fanout::new(&mut self.screen, Fanout::new(armed, prof));
            self.sys.run_frames_with_sink(frames, &mut sink)
        };
        self.latch_screen();
        // Nothing else in this run could have asked to stop, so a `SinkRequested` here **is** a watch.
        let stopped_by = record.fired().then(|| self.watch_wanting_stop()).flatten();
        Advanced {
            record,
            predicate_fired: false,
            stopped_by,
        }
    }

    /// The lowest-id armed watch that has reached its `stopAfter` threshold, if any.
    ///
    /// `Watchpoints::stop_requested` is one bool over every watch, which is all a *run* needs; a `stopped`
    /// event needs the identity, because §6's answer to "a `stopAfter` watch on a machine somebody else is
    /// running" is attribution rather than a gate — *"the halt always names its watch"*. Lowest id, to match
    /// the rule the recorder itself uses when several watches match one access.
    fn watch_wanting_stop(&self) -> Option<WatchId> {
        self.watchpoints
            .watches()
            .into_iter()
            .filter(|w| w.stop_after.is_some_and(|n| w.matched >= n))
            .map(|w| w.id)
            .min()
    }

    /// [`advance`](Engine::advance) with a stop predicate: the capture and the predicate ride the same run as
    /// a [`Fanout`], which is precisely what `System::run_until_stop` does internally with the predicate
    /// alone.
    fn advance_until<F: FnMut(u32, u64) -> bool>(
        &mut self,
        max_frames: u64,
        predicate: F,
    ) -> Advanced {
        let mut stop = StopWhen::new(predicate);
        let record = self.advance_with(max_frames, &mut stop);
        self.attribute(record, stop.fired())
    }

    /// [`advance_until`](Engine::advance_until) for the three `step*` rows (§6 lines 851-853): the same run,
    /// the same instruments, a [`StepStop`] where the PC predicate would be.
    ///
    /// It goes through `run_frames_with_sink` like every other advance here, and that is not incidental.
    /// `System::step_instruction` looks like the primitive a `step` wants and is not one: it drives the CPU
    /// **without advancing the master clock**, delivering no scheduler events, no Z80 catch-up and no IPL
    /// re-derive, so a step built on it would move the PC while the machine around it stood still — an
    /// interrupt that can never arrive, and a `frame`/`mclk` stamp frozen against a PC that moved. It has no
    /// caller in any `src/` for that reason. `run_frames_with_sink` is additionally the only entry point
    /// that maintains the private frame anchor an early stop has to re-anchor, so bypassing it would corrupt
    /// every subsequent `run_frames`.
    fn advance_stepping(&mut self, max_frames: u64, goal: StepGoal) -> Advanced {
        let mut stop = StepStop::new(goal);
        let record = self.advance_with(max_frames, &mut stop);
        let fired = stop.fired;
        self.attribute(record, fired)
    }

    /// [`advance_until`](Engine::advance_until) for `emulator/run_to_scanline` (§6 line 855): the same run,
    /// the same instruments, a [`LineStop`] where the PC predicate would be.
    ///
    /// It shares `advance_with`/`attribute` with the other three advancing shapes rather than growing a
    /// fourth Fanout, so the raster run carries the screen capture, the watch instrument and the profiler
    /// like every other advance — and so a `stopAfter` watch that ends this run is *attributed* rather than
    /// reported as the line having been reached.
    fn advance_to_line(&mut self, max_frames: u64, target: u32) -> Advanced {
        let mut stop = LineStop {
            target,
            fired: false,
        };
        let record = self.advance_with(max_frames, &mut stop);
        let fired = stop.fired;
        self.attribute(record, fired)
    }

    /// The instrumented run every advancing method shares: the caller's stop condition, the screen capture,
    /// the watch instrument and the profiler in one [`Fanout`], for `max_frames` frames.
    ///
    /// The instrument rides here and it has to: a run that does not feed it produces a `seen == 0` reading —
    /// "the recorder was never attached" — from a run that really happened. The `Option` arm of
    /// `BusEventSink` is what expresses "only sometimes attached" without a second code path.
    fn advance_with<S: BusEventSink>(&mut self, max_frames: u64, stop: &mut S) -> StopRecord {
        let record = {
            let armed = (self.watchpoints.watch_count() > 0).then_some(&mut self.watchpoints);
            let prof = self.profiler_armed.then_some(&mut self.profiler);
            let mut sink = Fanout::new(
                &mut self.screen,
                Fanout::new(stop, Fanout::new(armed, prof)),
            );
            self.sys.run_frames_with_sink(max_frames, &mut sink)
        };
        self.latch_screen();
        record
    }

    /// **Two things can end an instrumented run, and the caller must not confuse them.**
    /// `StopRecord::fired` says only that *the sink* asked to stop — and with a watch in the Fanout that
    /// sink is an OR of two. `predicate_fired` is the caller's own condition, which is what `run_to` reports
    /// as `reached` and what a `step*` reports by emitting its `stopped`; anything else that ended the run
    /// early was a `stopAfter` watch, and is attributed rather than mislabelled as the caller's condition
    /// having been met.
    fn attribute(&self, record: StopRecord, predicate_fired: bool) -> Advanced {
        let stopped_by = (record.fired() && !predicate_fired)
            .then(|| self.watch_wanting_stop())
            .flatten();
        Advanced {
            record,
            predicate_fired,
            stopped_by,
        }
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
    /// advertised list and the implemented set the same set by construction — and, since §11.17, the one
    /// place §2.5's params closure has to live for it to bind bus-wide.
    pub fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        let Some(spec) = METHODS.iter().find(|m| m.name == method) else {
            return Err(
                RpcError::new(code::METHOD_NOT_FOUND, format!("no such method: {method}"))
                    .with_data(json!({"method": method})),
            );
        };
        // §2.5: request params are closed. Checked here rather than in each handler, so the rule cannot
        // be forgotten by the next method and so the refusal **precedes any effect** — a write refused
        // for an unknown param has written nothing.
        //
        // `initialize` is exempt and is exempt *structurally*: it is the handshake, handled before
        // dispatch and absent from METHODS, so the closure's own spelling cannot reach it. That is the
        // one place on this bus where a client describes itself, and a version-skewed negotiation must
        // survive the step whose job is surviving skew.
        if let Some(unknown) = unknown_params(spec, params) {
            return Err(unknown);
        }
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
        // §2.1 (§11.23): which implementation answered, and which build of it. Both are read from
        // `build_info` and **never** from `self.config` — §2.1 makes a config-settable value a violation
        // rather than a supported deployment, and the check is source-level (`tests/server_build.rs`).
        // `serverName` beside them stays a deployment label, and stops being an identity.
        let mut server_build = json!({
            "id": build_info::SERVER_BUILD_ID,
            "source": build_info::SERVER_BUILD_SOURCE,
        });
        // `dirty` is REQUIRED under `source: "vcs"` and meaningless otherwise, which is why it is an
        // `Option` in the generated constant rather than a `bool` with a made-up value for the tarball
        // case: the schema's `if`/`then` is conditional on purpose, and emitting `dirty: false` from a
        // build that never consulted a working tree would be exactly the self-report §2.1 bars.
        if let Some(d) = build_info::SERVER_BUILD_DIRTY {
            server_build["dirty"] = json!(d);
        }
        Ok(json!({
            "serverName": self.config.server_name,
            "serverVersion": self.config.server_version,
            "implementation": build_info::IMPLEMENTATION,
            "serverBuild": server_build,
            "protocolVersion": rpc::PROTOCOL_VERSION,
            "capabilities": {
                // The authoritative event set (D6) — exactly what this server pushes.
                "events": EVENTS,
                // Method groups from the catalog that this thin slice does NOT implement. Clients branch
                // on these, never on the version integer (D5).
                "z80": false,
                "vgm": false,
                "objectDecoders": false,
                "profiler": true,
                "breakpoints": false,
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
                // §6, and an object for the same reason `checkpoints` is one: a client that has to hit a
                // limit to learn it is a client that loses evidence finding out. All four spaces, because
                // the core's VDP-internal write capture is live — a server without it would advertise
                // `["bus"]` and a client would not have to arm a watch to find that out.
                "watchpoints": {
                    "supported": true,
                    "spaces": ["bus", "vram", "cram", "vsram"],
                    "maxWatches": self.config.max_watches,
                    "ringCap": self.config.watch_ring_cap,
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
                "maxInputRows": self.config.max_input_rows,
                "maxWriteLen": self.config.max_write_len,
                "maxHashLen": self.config.max_hash_len,
                // REQUIRED once the profiler methods are advertised — a cap a client can only discover by
                // hitting it is a cap that costs work to learn.
                "maxProfilerRoutines": self.config.max_profiler_routines,
                "maxProfilerFrames": self.config.max_profiler_frames,
                // **The caller lens's capability signal** (§11.18). Its PRESENCE is what tells a client the
                // lens exists at all — a server without the lens omits this key and refuses
                // `set_profiler{callers:true}` as an undeclared param — so it is advertised here rather
                // than added to `capabilities`, on the `maxWriteLen`/`maxHashLen` precedent applied to one
                // lens rather than to one method. This server implements the lens, so the key is always
                // present and the no-lens direction has no branch here to go stale.
                "maxProfilerCallers": self.config.max_profiler_callers,
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

    /// Emit the `emulator/stopped` for a **bounded frame advance** — `run_frames` or `press` — choosing the
    /// stop *condition* and carrying the params that identify which instance of it fired.
    ///
    /// Two conditions can end one of these runs, and §11.7's house rule decides how each is spelled:
    ///
    /// * **the frame count ran out** → `reason: "runFrames"`, with `frames` and `deadlineReached: true`.
    ///   §3 was widened by §11.7 so this value covers `emulator/press` as well as `emulator/run_frames`:
    ///   *"`reason` names the condition that ended the run, never the method that drove it."* A ninth enum
    ///   value for `press` was drafted and refused — it would have made this enum name a **caller** for the
    ///   first time, and would still not have said *what* was pressed.
    /// * **a `stopAfter` watch matched its threshold** → `reason: "watchpoint"` with `watch` naming it.
    ///   `frames` is omitted, because §3 requires it only for `runFrames` and this run did not end on its
    ///   frame count; `deadlineReached` is `false`, because the run ended on a condition, not on its bound.
    ///
    /// `buttons`/`port` are the CR-9 half and are passed in by `press` alone. **The rule that they are
    /// present iff a press drove the advance is behavioural and cannot be schema-enforced** — the event
    /// deliberately carries no method discriminator, which is the cost §11.7 records rather than hides. The
    /// enforceable half, that the two travel together, is a `dependentRequired` in the schema *and* is
    /// structural here: they enter as one `Option` of a pair and there is no path that inserts one alone.
    fn emit_run_stop(
        &self,
        run: &Advanced,
        pc: u32,
        frames: u64,
        input: Option<(&[String], usize)>,
    ) {
        let mut extra = Map::new();
        if let Some((buttons, port)) = input {
            extra.insert("buttons".into(), json!(buttons));
            extra.insert("port".into(), json!(port));
        }
        if let Some(id) = run.stopped_by {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("watch".into(), json!(watch_wire_id(id)));
            self.emit_stopped("watchpoint", pc, extra);
            return;
        }
        extra.insert("frames".into(), json!(frames));
        extra.insert("deadlineReached".into(), json!(true));
        self.emit_stopped("runFrames", pc, extra);
    }

    /// Whole emulated frames a bounded advance actually completed, as the reply must report them.
    ///
    /// **Exact, including zero.** This rounded `0` up to `1` for exactly as long as it took to raise the
    /// question: `run_frames.frames` and `press.frames` were `"Frames actually advanced"` with
    /// `minimum: 1`, which was exact while the only way a bounded advance could end was by exhausting its
    /// count — and stopped being exact the moment §11.8 gave a watch a `stopAfter`, which can end one
    /// inside its own first frame. The contract was amended rather than the number bent
    /// (`empyrean` `34a1993`, §11.9 / CR-17): `minimum: 0` on both, with the reachability in the field's
    /// own description.
    ///
    /// *Why the rounding was worth refusing rather than documenting:* a count that silently becomes `1`
    /// when it was `0` is wrong precisely when a caller most needs it right — the caller is establishing
    /// whether anything executed at all before its watch fired. That is the defect §11.5 struck
    /// `run_to.stoppedAtFrame` for, wearing a different hat.
    fn frames_advanced(&self, run: &Advanced, requested: u64, mclk_before: u64) -> u64 {
        if run.stopped_by.is_none() {
            return requested;
        }
        self.sys.scheduler().now().saturating_sub(mclk_before) / MCLK_PER_FRAME
    }

    // ---------------------------------------------------------------- helpers

    /// Nearest preceding symbol for an address, plus displacement. `None` when no table is loaded or the
    /// address precedes every symbol in its address space.
    ///
    /// **The name is the bare identifying spelling; the displacement is the number beside it.** That is
    /// §4's rule — *"the displacement lives in this number and NEVER inside the name string"* — and the
    /// schema enforces it by pattern: every `symbol`/`symbolAtPc` field on this bus is
    /// `$defs/symbolName`, which rejects a `+$hex` suffix outright, because those fields MUST round-trip
    /// back through a `symbol` param.
    ///
    /// This used `Resolution`'s `Display`, which appends `+$hex` for a human reading a disassembly line,
    /// and so emitted `Probe+$2` at any displaced address — conformant only by accident, at every
    /// address that happened to land exactly on a label. Every caller here feeds a wire field, so every
    /// caller wanted [`Resolution::name`].
    fn symbol_at(&self, addr: u32) -> Option<(String, u32)> {
        let table = self.symbols.as_ref()?;
        let r = table.resolve(addr)?;
        Some((r.name().to_string(), r.displacement))
    }

    /// Resolve an `addr`-or-`symbol` parameter pair. Symbol-first addressing is D7: clients resolve,
    /// they never hardcode a RAM literal — the contract records a session in which a "verified" literal
    /// went stale within the session because a 36-byte insertion slid the whole RAM block by +$24.
    /// [`resolve_target`](Self::resolve_target) plus `emulator/write_memory`'s optional **`disp`**
    /// (§2.5, added 2026-08-19 by §11.17).
    ///
    /// `disp` is the ergonomic half of the same change request whose other half is the params closure,
    /// and the two answer **different halves** of one complaint: a client wanting to write `Player_1`+2
    /// had no way to say so, reached for a parameter name, and was silently obeyed at `Player_1`+0.
    /// Closing params stops the silence; this gives the request somewhere to go.
    ///
    /// Three properties, each of which the fragment also enforces:
    ///
    /// - **Valid only with `symbol`.** With `addr` it is arithmetic the caller has already done, and
    ///   `{addr, disp}` is `-32602` (the fragment's `dependentRequired`).
    /// - **Non-negative**, because it mirrors the `symbolDisp` a read reply hands back — a displacement
    ///   from the *nearest preceding* symbol, which cannot be negative. `{symbol, symbolDisp}` out,
    ///   `{symbol, disp}` in: D7's round trip made literal.
    /// - The **displaced** address is the one that must land in the work-RAM window, and the one the
    ///   reply echoes. Both fall out of returning it here, before the caller's bounds check.
    ///
    /// Kept on this method rather than folded into `resolve_target`, deliberately: `read`/`read_memory`
    /// share that helper and do not declare `disp`, and a helper that quietly honoured a key those
    /// fragments do not carry would put the server back on the wrong side of the rule above it.
    /// [`resolve_target`](Self::resolve_target) under the **`oneOf` the fragments actually declare**:
    /// `addr` XOR `symbol`, both present is `-32602`.
    ///
    /// `resolve_target` checks `symbol` first and returns, so `{addr, symbol}` silently ignored the `addr`
    /// and answered about the symbol. Five fragments spell `oneOf [{required:[addr]}, {required:[symbol]}]`
    /// — `run_to`, `read_memory`, `write_memory`, `memory_hash`, `watchpoint_add` — and a JSON-Schema
    /// `oneOf` over two required-key branches means *exactly* one: both present matches both branches and
    /// fails. So a request the contract refuses was being answered, with the caller's other key dropped on
    /// the floor. Registered as a live request-side divergence on 2026-08-22 (the acceptance-21 survey,
    /// §2.2) and closed here.
    ///
    /// **Why it is not the flat params closure's job.** [`unknown_params`] is method-agnostic and knows only
    /// which keys are *legal*; an alternation is about which are legal *together*. Hoisting it to
    /// [`dispatch`](Self::dispatch) would need the alternation on every [`MethodSpec`] row, and would then
    /// preempt four hand-written refusals — `checkpoint_drop`'s `id`/`all`, `watchpoint_clear`'s
    /// `watch`/`all`, `write_memory`'s `bytes`/`value`, `write_cram`'s `r,g,b`/`raw` — each of which names
    /// its own two alternatives in words. A generic refusal that replaced those with a worse message would
    /// be a regression wearing a refactor's clothes. In-handler, before any effect, is what those four
    /// already do and is the house shape.
    ///
    /// **`emulator/read` is deliberately NOT routed through here.** Alone among this helper's callers its
    /// fragment declares no `oneOf`, so `{addr, symbol}` is a request the contract *permits* and refusing it
    /// would be the invention ban read the other way round — this server narrowing a shape a second
    /// conformant server would accept. The omission looks like a transcription gap rather than a decision
    /// (every other addr-or-symbol row in the catalog carries the alternation), and it is reported upward
    /// rather than repaired locally.
    fn resolve_exclusive_target(&self, params: &Value) -> Result<u32, RpcError> {
        if params.get("addr").is_some() && params.get("symbol").is_some() {
            return Err(RpcError::invalid_params(
                "`addr` and `symbol` are alternatives — pass exactly one. Both were given, and the two \
                 can name different places: resolving one and dropping the other would answer a question \
                 that was not asked",
            )
            .with_data(json!({"conflictingParams": ["addr", "symbol"]})));
        }
        self.resolve_target(params)
    }

    fn resolve_displaced_target(&self, params: &Value) -> Result<u32, RpcError> {
        let base = self.resolve_exclusive_target(params)?;
        let Some(v) = params.get("disp") else {
            return Ok(base);
        };
        if params.get("symbol").is_none() {
            return Err(RpcError::invalid_params(
                "`disp` is valid only with `symbol` — with `addr` it is arithmetic the caller has \
                 already done",
            ));
        }
        let Some(d) = v.as_u64() else {
            return Err(RpcError::invalid_params(
                "`disp` must be a non-negative integer — it mirrors `symbolDisp`, a displacement from \
                 the nearest preceding symbol, which cannot be negative",
            ));
        };
        let sum = u64::from(base) + d;
        if sum > u64::from(BUS_ADDR_MAX) {
            // `data.addr` carries the SUM, not the base: the message complains about `symbol` + `disp`,
            // and a data field naming a different number than the sentence beside it is the join a
            // client cannot make. Formatted here rather than through `out_of_range` because the sum can
            // exceed `u32` — reporting a truncated version of the value we are refusing for being too
            // large would be its own small lie.
            let addr = format!("0x{sum:08X}");
            return Err(RpcError::new(
                code::ADDRESS_OUT_OF_RANGE,
                format!("{addr}: `symbol` + `disp` runs past the end of the 24-bit bus"),
            )
            .with_data(json!({"addr": addr})));
        }
        Ok(sum as u32)
    }

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
    /// so the read-shaped replies built on this (`read`, `read_memory`, `read_vram`) each carry a
    /// `caveat` saying so. That is exactly the landmine the recon found in the sibling's `write_vram`,
    /// which bypasses the VDP port path and *"nothing in its docstring says so"*. Not every caller wants
    /// that caveat, though: `memory_hash` is also built on this and deliberately carries none — a
    /// fingerprint's provenance note lives in its own contract row, not in the reply envelope.
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
        let mclk_before = self.sys.scheduler().now();
        let run = self.advance(frames);
        self.running = false;
        let frames = self.frames_advanced(&run, frames, mclk_before);
        let pc = self.sys.cpu_regs().pc;
        // §3, §8 item 13: a completed `run_frames` is **`runFrames`**, not the nearest-looking `step`.
        // (CR-1 raised the gap — the enum had no value for a bounded frame advance — and was adopted on
        // 2026-08-14 as exactly this spelling, matching the existing `runTo`/`runToScanline`.) §3 pins
        // the two additive params with it: `frames` is REQUIRED here, and `deadlineReached` is always
        // `true`, the bound being the frame count itself — unless a `stopAfter` watch ended the run first,
        // in which case the condition that ended it was the watch and `emit_run_stop` says so.
        self.emit_run_stop(&run, pc, frames, None);
        Ok(json!({
            "frames": frames,
            "frameToken": self.frame(),
            "pc": hex::addr(pc),
        }))
    }

    fn run_to(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/run_to")?;
        let target = self.resolve_exclusive_target(params)?;
        // `maxFrames` is an *additive optional* param on a catalogued method, not a new op. Without a
        // bound, a target that is never reached is an unbounded run — i.e. exactly the transport hang
        // that destroyed a frozen repro frame in `aeon/docs/BUGS.md:494-551`.
        let max_frames = match params.get("maxFrames") {
            None => self.config.max_run_frames.min(600),
            Some(v) => hex::parse_count("maxFrames", v, 1, self.config.max_run_frames)?,
        };
        self.running = true;
        self.emit_resumed();
        let run = self.advance_until(max_frames, |pc, _| pc == target);
        let record = run.record;
        self.running = false;
        // A `stopAfter` watch can end this run too, and when it does the run reached neither its target nor
        // its bound. §6 answers that case by attribution: the halt names its watch, and `runTo` would be a
        // knowing mislabel — the same class of error §8 item 13 names for `step` on a frame advance.
        let mut extra = Map::new();
        if let Some(id) = run.stopped_by {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("watch".into(), json!(watch_wire_id(id)));
            self.emit_stopped("watchpoint", record.pc, extra);
        } else {
            extra.insert("target".into(), json!(hex::addr(target)));
            extra.insert("deadlineReached".into(), json!(!run.predicate_fired));
            self.emit_stopped("runTo", record.pc, extra);
        }

        // No `stoppedAtFrame`/`stoppedAtMclk`. They would be the envelope stamp (§2.2) spelled twice:
        // `record` is captured at the halt, and the stamp is computed on the same engine thread the
        // instant `dispatch` returns with the machine paused — and `run_to` *requires* a paused machine —
        // so nothing can advance between the two. §6.1 has already ruled this exact case for `restore`:
        // *"The `frame`/`mclk` in `checkpoint`'s result, and the whole of `restore`'s result, **are** the
        // machine stamp (§2.2) — no extra fields are needed and none should be invented."* Re-spelling the
        // stamp inside the result teaches clients that the stamp is not the answer, which is the one
        // lesson D11 exists to prevent. (CR-13, ruled 2026-08-15 —
        // `docs/2026-08-15-fable-ruling-cr13-cr14.md`; §6's row for this method is unchanged by it.)
        //
        // **`reached` is the predicate's own verdict, never the sink's.** Since a `stopAfter` watch joined
        // the Fanout, `StopRecord::fired` means only "*something* asked to stop", and reading it here would
        // report a target as reached because an unrelated watch halted the run — the ambiguous-success
        // defect `StopReason` was split in two to prevent, rebuilt one level up.
        let mut out = json!({
            "target": hex::addr(target),
            "reached": run.predicate_fired,
            "pc": hex::addr(record.pc),
            "maxFrames": max_frames,
        });
        if let Some((name, disp)) = self.symbol_at(record.pc) {
            out["symbol"] = json!(name);
            out["symbolDisp"] = json!(disp);
        }
        if let Some(id) = run.stopped_by {
            out["caveat"] = json!(format!(
                "the target PC was never reached — watch {} hit its stopAfter threshold and ended the \
                 run first, so NOTHING about the machine state follows from where it stopped",
                watch_wire_id(id)
            ));
        } else if !run.predicate_fired {
            out["caveat"] = json!(
                "the target PC was never reached within maxFrames — the run ended on its bound, so \
                 NOTHING about the machine state follows from where it stopped"
            );
        }
        Ok(out)
    }

    /// **§6 line 855 — `emulator/run_to_scanline`.** `line` (0-511), `maxFrames`? (≥1, def 600) → `line`,
    /// `reached`, `maxFrames`, `caveat`? *(D12)*
    ///
    /// `run_to`'s sibling: bounded, and it says whether it fired. Everything structural is shared with it —
    /// [`require_paused`](Self::require_paused) for §6's run-control state rule, the `maxFrames` bound and
    /// its 600 default, `advance_with`/`attribute` for the instrumented run, watch attribution when a
    /// `stopAfter` watch ended the run instead. What differs is the condition and two things the fragment
    /// says about the answer.
    ///
    /// **D-04, transcribed rather than repaired: the result carries no `pc`.** `run_to`'s does (plus
    /// `symbol`/`symbolDisp`), so a caller that ran to a scanline cannot learn where the 68000 stopped
    /// without a second call. The fragment's own `$comment` registers the asymmetry as a defect and this
    /// server reports it rather than fixing it locally — adding `pc` here would be a key on the wire that no
    /// contract text describes, which is CR-13's whole subject. The `emulator/stopped` event this call emits
    /// *does* carry `pc` (§3 requires it), so the information is on the bus for a stream consumer; it is only
    /// the caller's own reply that lacks it. That is the CR this method's experience argues for.
    ///
    /// **Lines 262-511 are contractually legal and physically unreachable, and are run rather than
    /// refused.** This core is NTSC V28: [`LINES_PER_FRAME`](oracle_core::vdp::LINES_PER_FRAME) is 262, so
    /// `on_line_start` never delivers a line above 261 and no run can ever reach one. The fragment's range is
    /// §6's and is deliberately wider than `emulator/scanlines`' 0-223 because *"a raster target may
    /// legitimately sit in blanking"*; it is not video-mode-aware. Three reasons this answers rather than
    /// refuses:
    ///
    /// * **Refusing a value the fragment declares legal is a unilateral divergence** — §8's invention ban
    ///   read the other way round. The 0-511 span is the contract's, and narrowing it to this core's timing
    ///   basis would make one conformant server refuse what another accepts, which is the failure the closed
    ///   catalog exists to prevent. (Refusing *outside* 0-511 is a different thing and is still `-32602`,
    ///   refused never clipped: that bound is the fragment's own.)
    /// * **`caveat` is declared on this row**, unusually — one of only two in the catalog — precisely because
    ///   D12 gives it SHOULD force here. This is what it is for: the reply says *in words* that the line
    ///   cannot occur in this video mode, which is strictly more than `reached: false` alone carries.
    /// * **The house property holds either way**: the answer is exact, or the server says it is not.
    ///
    /// The cost is honest and worth naming: `{line: 300}` burns the whole `maxFrames` budget to answer a
    /// question that was decidable at parse time. Short-circuiting it would be cheaper and would make an
    /// unreachable line observably *different* from a reachable line that simply never came round —
    /// different frames advanced, a different machine at the end — for a caller who cannot tell the two
    /// cases apart from the contract. Uniform behaviour under one bound is the better trade, and the caveat
    /// pays for it.
    fn run_to_scanline(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/run_to_scanline")?;
        // D9 category 2 — a line number is a JSON integer, never a hex string — and the 0-511 bound is the
        // fragment's own. Out of range is `-32602`, refused and never clipped, the same shape
        // `parse_cram_line` uses: a clipped raster target stops somewhere the caller did not ask for and
        // says it succeeded.
        let line = match params.get("line") {
            None => {
                return Err(RpcError::invalid_params(
                    "`line` is required — the scanline to run to (integer 0-511)",
                ))
            }
            Some(v) => hex::parse_count("line", v, 0, MAX_SCANLINE_TARGET)?,
        } as u32;
        let max_frames = match params.get("maxFrames") {
            None => self.config.max_run_frames.min(600),
            Some(v) => hex::parse_count("maxFrames", v, 1, self.config.max_run_frames)?,
        };
        self.running = true;
        self.emit_resumed();
        let run = self.advance_to_line(max_frames, line);
        let record = run.record;
        self.running = false;
        let mut extra = Map::new();
        if let Some(id) = run.stopped_by {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("watch".into(), json!(watch_wire_id(id)));
            self.emit_stopped("watchpoint", record.pc, extra);
        } else {
            // **No `line` key on the event, deliberately.** §3's `stopped` row lists no coordinate for this
            // reason, and `runToScanline` already names the condition; a new undeclared key would be exactly
            // the unregistered surplus CR-13 spent a ruling on. (`run_to`'s `target` predates that ruling and
            // is not a licence to add a second one.) The caller's own reply echoes `line`; a stream consumer
            // gets the reason, the `pc` and `deadlineReached`.
            extra.insert("deadlineReached".into(), json!(!run.predicate_fired));
            self.emit_stopped("runToScanline", record.pc, extra);
        }
        // `reached` is the LineStop's own verdict, never the sink's: with a `stopAfter` watch in the Fanout
        // `StopRecord::fired` means only "something asked to stop", and reading it here would report a line
        // as reached because an unrelated watch halted the run.
        let mut out = json!({
            "line": line,
            "reached": run.predicate_fired,
            "maxFrames": max_frames,
        });
        if let Some(id) = run.stopped_by {
            out["caveat"] = json!(format!(
                "scanline {line} was never reached — watch {} hit its stopAfter threshold and ended the \
                 run first, so NOTHING about the machine state follows from where it stopped",
                watch_wire_id(id)
            ));
        } else if !run.predicate_fired {
            // Two ways not to fire, and they are not the same fact. Saying "the budget ran out" about a line
            // that cannot exist would send a caller to raise `maxFrames` forever.
            out["caveat"] = json!(if line >= LINES_PER_FRAME as u32 {
                format!(
                    "scanline {line} cannot occur in this video mode — the frame is {LINES_PER_FRAME} \
                     lines (0-{}), so the run ended on its maxFrames bound and NOTHING about the machine \
                     state follows from where it stopped",
                    LINES_PER_FRAME - 1
                )
            } else {
                format!(
                    "scanline {line} was never reached within maxFrames — the run ended on its bound, so \
                     NOTHING about the machine state follows from where it stopped"
                )
            });
        }
        Ok(out)
    }

    /// **§6 line 851 — `emulator/step`.** `count`? → `pc`, `symbol`?, `symbolDisp`?
    ///
    /// `count` is served **exactly as the fragment writes it**: `integer, minimum 0`, no default, no
    /// ceiling. Every sibling count in the catalog states its bounds (`run_frames.frames? (≥1, def 1)`,
    /// `press.frames? (1-1000, def 2)`, `run_to.maxFrames? (≥1, def 600)`); this one states none, and the
    /// fragment says so deliberately — inventing a bound here would make the schema the author of a
    /// constraint the contract never agreed. It is registered upstream as **audit D-02**, and this server
    /// reports the defect rather than repairing it locally.
    ///
    /// Two consequences of that, both visible from here and neither hidden:
    ///
    /// * **The default is this server's invention, because the contract has none.** An omitted `count` has
    ///   to mean *something*, and `1` is the only reading that matches what every sibling spells out and
    ///   what the word "step" means. But it is a choice, so two conformant servers could disagree about
    ///   `{}` — which is the half of D-02 a client hits first.
    /// * **A step still runs inside a frame bound**, because an unbounded count is an unbounded run wearing
    ///   a different name, and the core refuses those on principle (*"an unbounded `run_until` that silently
    ///   hangs is strictly worse than a hand-tuned frame budget"*). A count too large to retire inside the
    ///   bound stops early — and `step`'s result has **no key that can say so**, no `reached`, and a
    ///   `caveat` the fragment declares absent. The `stopped` event carries `deadlineReached` and is the
    ///   only place the shortfall is visible. That is the CR this method's experience argues for.
    fn step(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/step")?;
        // `minimum: 0` is the fragment's floor and `u64::MAX` is its stated absence of a ceiling. Neither is
        // a policy this server chose; both are transcribed.
        let count = match params.get("count") {
            None => 1,
            Some(v) => hex::parse_count("count", v, 0, u64::MAX)?,
        };
        let pc = self.run_step(StepGoal::Instructions(count));
        let mut out = json!({ "pc": hex::addr(pc) });
        // §4: the BARE label plus the number beside it. `symbol_at` returns exactly that pair, and the
        // fragment's `$defs/symbolName` rejects a `+$hex` suffix outright. **Absent when nothing resolves**
        // — a server MUST NOT fall back to the address string, so there is no `else` here on purpose.
        if let Some((name, disp)) = self.symbol_at(pc) {
            out["symbol"] = json!(name);
            out["symbolDisp"] = json!(disp);
        }
        // No `caveat`, and that is the fragment's word rather than an omission: it declares the key ABSENT
        // (the `sprites` precedent) because a step has no weaker-answer condition §6 names. §8 item 20's
        // closure would reject one, and the suite applies that closure to every reply.
        Ok(out)
    }

    /// **§6 line 852 — `emulator/step_over`.** No params, and **no result keys at all**.
    ///
    /// The empty result is §6's row (`—` in both columns), not an omission: the answer arrives on the event
    /// channel, whose `stopped` params carry `pc`, `symbol?` and `symbolDisp?`. That `emulator/step` *does*
    /// return `pc` while this returns nothing is an asymmetry §6 owns and the fragments deliberately
    /// preserved — **audit D-03** — so it is served as written and reported, never smoothed over here.
    ///
    /// **What it actually does, and why it is not a `step` in disguise.** The pending opcode is classified
    /// before the run. If it is not a `JSR`/`BSR` there is nothing to step over and this is one instruction,
    /// which is the correct answer rather than a fallback — including for `JMP`, which enters a routine no
    /// `RTS` will pair with this frame, so "over" it is meaningless. If it *is* a call, the run continues
    /// until the frame that call opens closes again, matched on the profiler's exact rule.
    fn step_over(&mut self, _params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/step_over")?;
        // `prefetch[0]` is the opcode word about to execute — the same value `StepRetire::opcode` reports
        // for this step, read from the same place before it runs.
        let pending = self.sys.cpu_regs().prefetch[0];
        let goal = match control_flow_of(pending) {
            ControlFlow::Call => StepGoal::OverCall,
            _ => StepGoal::Instructions(1),
        };
        self.run_step(goal);
        Ok(json!({}))
    }

    /// **§6 line 853 — `emulator/step_out`.** No params, no result keys. Identical treatment to
    /// [`step_over`](Engine::step_over), for identical reasons (audit D-03).
    ///
    /// Runs until the frame that was already live returns. The frame's entry SP is **not** knowable from
    /// here — however many locals the routine has already pushed is not recoverable, and nothing in the core
    /// enumerates live frames — so what identifies the exit is a return that closes nothing this run watched
    /// open, guarded by having actually unwound past the stack pointer the caller asked from.
    fn step_out(&mut self, _params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/step_out")?;
        let regs = self.sys.cpu_regs();
        let goal = StepGoal::OutOfFrame {
            sp0: regs.a7(),
            supervisor: regs.supervisor(),
        };
        self.run_step(goal);
        Ok(json!({}))
    }

    /// The run all three `step*` rows share: advance under a [`StepGoal`], then report the halt.
    ///
    /// Returns the PC the machine stopped at, which is `emulator/step`'s whole result and is what the
    /// `stopped` event carries for all three.
    ///
    /// **The `stopped` reason is `step` for all three methods.** §3 pins that explicitly — *"`step` covers
    /// `step`, `step_over` and `step_out` because those three share one stop condition"* — so neither
    /// `step_over` nor `step_out` gets a reason of its own, and `reason` names the condition rather than the
    /// method that drove it.
    ///
    /// **Unless a watch ended it first.** A `stopAfter` watch can halt this run as it can halt any other,
    /// and when it does the step's condition was *not* met. Calling that a completed `step` would be the
    /// knowing mislabel §8 item 13 names, so the halt is attributed to the watch exactly as `run_to` and
    /// `run_frames` attribute theirs.
    fn run_step(&mut self, goal: StepGoal) -> u32 {
        // The bound is a server policy and it has to be: none of the three rows takes a param that could
        // carry one, and their params objects are closed, so there is nowhere for a caller to put a budget.
        // `run_to`'s own default is the precedent for the number.
        let max_frames = self.config.max_run_frames.min(600);
        self.running = true;
        self.emit_resumed();
        let run = self.advance_stepping(max_frames, goal);
        self.running = false;
        let pc = self.sys.cpu_regs().pc;
        let mut extra = Map::new();
        if let Some(id) = run.stopped_by {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("watch".into(), json!(watch_wire_id(id)));
            self.emit_stopped("watchpoint", pc, extra);
        } else {
            // `true` when the run ended on its frame bound rather than on the step condition (D12) — the
            // only channel on which a short step is visible at all, since none of the three results has a
            // key for it. `run_to` spells its complement the same way.
            extra.insert("deadlineReached".into(), json!(!run.predicate_fired));
            self.emit_stopped("step", pc, extra);
        }
        pc
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
        let addr = self.resolve_exclusive_target(params)?;
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

    /// `emulator/write_memory` — the poke primitive (§6 memory, CR-21 / §11.13).
    ///
    /// Work-RAM window only, refused never clipped; exactly one payload spelling; requires a paused
    /// machine (named in §6's run-control state rule for `press`'s reason — a poke mid-free-run
    /// mutates the timeline just as surely). Bytes travel the bus path, so hardware mirror masking
    /// applies and no `ram_mut` debug back door exists on core. The sink is `()` on purpose: a poke
    /// is a debugger access, not a guest access — it is never offered to the watch surface, because
    /// a hit's `pc` names the instruction that drove the access and a poke has none to name.
    fn write_memory(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/write_memory")?;
        let addr = self.resolve_displaced_target(params)?;
        let data: Vec<u8> = match (params.get("bytes"), params.get("value")) {
            (Some(_), Some(_)) => {
                return Err(RpcError::invalid_params(
                    "`bytes` and `value` are alternatives — pass exactly one",
                ))
            }
            (Some(b), None) => {
                if params.get("width").is_some() {
                    return Err(RpcError::invalid_params(
                        "`width` goes with `value`; a `bytes` payload carries its own length",
                    ));
                }
                let d = hex::parse_bytes("bytes", b)?;
                if d.is_empty() {
                    return Err(RpcError::invalid_params(
                        "`bytes` is empty — nothing to write",
                    ));
                }
                if d.len() as u64 > self.config.max_write_len {
                    return Err(RpcError::invalid_params(format!(
                        "`bytes` is {} bytes; the ceiling is limits.maxWriteLen = {} — refused, never truncated",
                        d.len(),
                        self.config.max_write_len
                    )));
                }
                d
            }
            (None, Some(v)) => {
                // D9 category 2: a count/width-bearing number is a JSON number, never a hex string. The
                // citation stays here rather than in the message — the client has no D9 to look up.
                let Some(value) = v.as_u64() else {
                    return Err(RpcError::invalid_params(
                        "`value` must be a non-negative integer",
                    ));
                };
                let width = match params.get("width").and_then(Value::as_u64) {
                    Some(w @ (1 | 2 | 4)) => w as usize,
                    Some(_) => return Err(RpcError::invalid_params("`width` must be 1, 2 or 4")),
                    None => {
                        return Err(RpcError::invalid_params(
                            "`value` requires `width` (1, 2 or 4)",
                        ))
                    }
                };
                if value >= 1u64 << (width * 8) {
                    return Err(RpcError::invalid_params(format!(
                        "`value` {value} does not fit width {width}"
                    )));
                }
                // Big-endian, as the 68000 stores.
                value.to_be_bytes()[8 - width..].to_vec()
            }
            (None, None) => {
                return Err(RpcError::invalid_params(
                    "one of `bytes` (hex string) or `value`+`width` is required",
                ))
            }
        };
        let end = u64::from(addr) + data.len() as u64 - 1;
        if !(WORK_RAM_LO..=WORK_RAM_HI).contains(&addr) || end > u64::from(WORK_RAM_HI) {
            return Err(out_of_range(
                addr,
                "only the work-RAM window ($E00000-$FFFFFF) is writable; ROM and I/O writes are refused",
            ));
        }
        let mut sink = ();
        let mut bus = self.sys.mega_bus(&mut sink);
        for (i, b) in data.iter().enumerate() {
            bus.write8(addr + i as u32, FC_SUPERVISOR_DATA, *b);
        }
        Ok(json!({ "addr": hex::addr(addr), "len": data.len() }))
    }

    /// `emulator/read` — one byte read across the four address spaces (§6 memory, added by §11.12 / CR-20).
    ///
    /// **This is the read half of the watch surface.** A `cram`/`vsram` watch hit reports `space` *and*
    /// `addr`, and before this row nothing on the bus accepted that pair back — the client held a
    /// coordinate it could not use. The `space` vocabulary is `watchpoint_add`'s, unchanged, and reuses its
    /// parser so the two surfaces cannot drift apart.
    ///
    /// A pure read: no `require_paused`. The Z80's space is deliberately absent — `emulator/z80_read` keeps
    /// its own row and its own catalogued bounds.
    fn read(&mut self, params: &Value) -> Result<Value, RpcError> {
        let space = parse_watch_space(params)?;
        // §4's round-trip rule, and `watchpoint_add`'s reason verbatim: a symbol names a 68000 address, and
        // a VDP-internal byte address has no symbol. Checked before resolution so the refusal names the
        // real mistake rather than "no symbol named …".
        if params.get("symbol").is_some() && space != WatchSpace::Bus {
            return Err(RpcError::invalid_params(format!(
                "`symbol` is valid only with space \"bus\" — a VDP-internal byte address has no symbol \
                 (got space {:?})",
                space_name(space)
            )));
        }
        // **The permissive resolver, and that is a decision.** Every other `addr`-or-`symbol` row in the
        // catalog declares `oneOf [{required:[addr]}, {required:[symbol]}]` and this one does not, so
        // `{addr, symbol}` is a request this fragment permits — and `resolve_exclusive_target` would refuse
        // what a second conformant server accepts. Reported upward as a probable transcription gap rather
        // than repaired here; until it moves, this row keeps the symbol-first resolution it has always had.
        let addr = self.resolve_target(params)?;
        let len = match params.get("len") {
            None => 1,
            Some(v) => hex::parse_count("len", v, 1, self.config.max_read_len)?,
        };

        // Refused, never clipped: a clipped read reports bytes it never looked at, which is the one answer
        // a read must never be able to give.
        let (bytes, region) = match space {
            WatchSpace::Bus => {
                let (data, region) = self.debug_read(addr, len as usize)?;
                (data, Some(region))
            }
            _ => {
                let mem: &[u8] = match space {
                    WatchSpace::Vram => self.sys.vram(),
                    WatchSpace::Cram => self.sys.vdp().cram(),
                    _ => self.sys.vdp().vsram(),
                };
                let end = u64::from(addr) + len;
                if end > mem.len() as u64 {
                    return Err(out_of_range(
                        addr,
                        &format!(
                            "the read would run past the end of {} ({} bytes)",
                            space_name(space),
                            mem.len()
                        ),
                    ));
                }
                (mem[addr as usize..end as usize].to_vec(), None)
            }
        };

        let mut out = Map::new();
        // Echoed so the reply is self-describing: an `addr` means nothing without the space it is in.
        out.insert("space".into(), json!(space_name(space)));
        out.insert("addr".into(), json!(hex::addr(addr)));
        out.insert("len".into(), json!(bytes.len()));
        out.insert("bytes".into(), json!(hex::bytes(&bytes)));
        // `region`, `symbol` and `symbolDisp` appear **iff** the space is `bus` — enforced in the schema in
        // both directions, and here by construction.
        if let Some(region) = region {
            out.insert("region".into(), json!(region));
            if let Some((name, disp)) = self.symbol_at(addr) {
                out.insert("symbol".into(), json!(name));
                out.insert("symbolDisp".into(), json!(disp));
            }
        }
        Ok(Value::Object(out))
    }

    /// `emulator/read_cram` — the palette, read (§6 VRAM/CRAM/layers, specified by §11.17 / CR-27b).
    ///
    /// **The entry is the STORED colour, never the displayed one.** `raw` is the 9-bit word the chip
    /// holds and `r`/`g`/`b` its 3-bit components — `emulator/write_cram`'s own spelling, so a read entry
    /// hands straight back to a write. The colour a dot is actually *shown* in is
    /// [`pixel_attribution`](Self::pixel_attribution)'s `rgb`, which runs these components through an
    /// intensity ramp at the resolved shadow/highlight state and differs whenever that state is not
    /// `normal`. **No 8-bit expansion is emitted here**, deliberately: the catalog has never pinned a
    /// ramp, the two servers that compute one disagree at three of the eight levels (`F-CRAM-RAMP`), and
    /// a number two conformant servers answer differently is worse than an absent one.
    ///
    /// `cramAddr` rides on every entry although it is derivable, because it is the **join key** three
    /// other surfaces speak and `(line, index)` is not: `pixel_attribution.cramAddr`, the `space`+`addr`
    /// pair a `cram` watch hit reports, and `emulator/read` with `space: "cram"`. `pixel_attribution`'s
    /// `cramIndex` is deliberately not carried — that method emits it only because it has no
    /// `(line, index)` pair to give.
    ///
    /// A **pure read**: no `require_paused`, on the `read`/`sprites`/`pixel_attribution`/`scanlines`
    /// precedent. D11's stamp is the whole answer to a torn palette sample.
    fn read_cram(&mut self, params: &Value) -> Result<Value, RpcError> {
        let line = match params.get("line") {
            None => None,
            Some(v) => Some(parse_cram_line(v)?),
        };
        let cram = self.sys.vdp().cram();
        // Entries are line-ascending then index-ascending and contiguous, and the range is fixed by the
        // request: one line's 16 or the whole 64. `palette` is bounded by the video hardware, so §2.4
        // clause (d) gives it neither a truncation flag nor a cursor — a partial palette is not
        // expressible, and there is nothing for a client to page through.
        let entries: Vec<Value> = match line {
            Some(l) => (0..16).map(|i| cram_entry(cram, l, i)).collect(),
            None => (0..4)
                .flat_map(|l| (0..16).map(move |i| (l, i)))
                .map(|(l, i)| cram_entry(cram, l, i))
                .collect(),
        };
        let mut out = Map::new();
        // Echoed IFF the param was given — its presence is what tells a client which of the two answers
        // it is holding, and the fragment ties the echo to the array's length in BOTH directions.
        if let Some(l) = line {
            out.insert("line".into(), json!(l));
        }
        out.insert("palette".into(), Value::Array(entries));
        Ok(Value::Object(out))
    }

    /// `emulator/write_cram` — one palette entry, poked (§6, specified by §11.17 / CR-27a).
    ///
    /// **Requires a paused machine** (`-32005`, `data.reason = "machineRunning"`). That gate is
    /// *demand-side confirmation*, not symmetry with `write_memory`: the requester established that the
    /// unpaused case fails for engine reasons anyway — a composed-per-frame palette pipeline overwrites a
    /// direct CRAM write within the frame — and that where this method earns its keep is the paused
    /// machine, inspecting a colour and tweaking it with nothing stepping on it.
    ///
    /// **Exactly one colour spelling**: all three of `r`/`g`/`b`, or `raw`. Both, neither, a partial
    /// triple, and a partial triple beside `raw` are each `-32602`. A `raw` carrying bits outside the
    /// chip's `$0EEE` mask is `-32602` too — **refused, never masked** — because the reply's whole job is
    /// to say where the write landed, and a reply reporting a value the caller did not send is exactly
    /// the silent mutation this bus refuses everywhere else. `line`/`index` out of range are refused,
    /// never clipped.
    ///
    /// The two **standing properties** of a poke (stated in §6 rather than as a `caveat` on every reply,
    /// per §2.4's advisory — and the fragment declares `caveat` absent, so emitting one would fail item
    /// 20's closure): it is **never offered to the watch surface**, and it does **not repaint a frame
    /// already drawn**. Both fall out of [`Vdp::poke_cram`], which is where the reasoning lives.
    fn write_cram(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/write_cram")?;
        let line =
            parse_cram_line(params.get("line").ok_or_else(|| {
                RpcError::invalid_params("`line` is required (palette line, 0-3)")
            })?)?;
        let index = match params.get("index") {
            None => {
                return Err(RpcError::invalid_params(
                    "`index` is required (entry within the line, 0-15)",
                ))
            }
            Some(v) => match v.as_u64() {
                Some(n) if n <= 15 => n as u8,
                Some(n) => {
                    return Err(RpcError::invalid_params(format!(
                        "`index` {n} is outside 0-15 — refused, never clipped"
                    )))
                }
                None => {
                    return Err(RpcError::invalid_params(
                        "`index` must be an integer 0-15 (D9 category 2)",
                    ))
                }
            },
        };

        // The alternation, refused in all four bad spellings before anything is written.
        let triple = ["r", "g", "b"].map(|k| params.get(k));
        let any_component = triple.iter().any(Option::is_some);
        let word = match (any_component, params.get("raw")) {
            (true, Some(_)) => {
                return Err(RpcError::invalid_params(
                    "`r`/`g`/`b` and `raw` are alternatives — pass exactly one spelling",
                ))
            }
            (false, None) => return Err(RpcError::invalid_params(
                "a colour is required: all three of `r`/`g`/`b` (0-7 each), or `raw` (<= 0x0EEE)",
            )),
            (true, None) => {
                let mut c = [0u16; 3];
                for (slot, (name, v)) in c.iter_mut().zip(["r", "g", "b"].iter().zip(triple)) {
                    let Some(v) = v else {
                        return Err(RpcError::invalid_params(format!(
                            "`{name}` is missing — `r`, `g` and `b` travel together; a partial triple \
                             is refused"
                        )));
                    };
                    match v.as_u64() {
                        Some(n) if n <= 7 => *slot = n as u16,
                        Some(n) => {
                            return Err(RpcError::invalid_params(format!(
                                "`{name}` {n} is outside 0-7 — a stored component is 3-bit"
                            )))
                        }
                        None => {
                            return Err(RpcError::invalid_params(format!(
                                "`{name}` must be an integer 0-7 (D9 category 2)"
                            )))
                        }
                    }
                }
                // `---- BBB- GGG- RRR-`, the layout `Vdp::cram_decoded` reads back.
                (c[0] << 1) | (c[1] << 5) | (c[2] << 9)
            }
            (false, Some(v)) => {
                let Some(n) = v.as_u64() else {
                    return Err(RpcError::invalid_params(
                        "`raw` must be a non-negative integer (D9 category 2)",
                    ));
                };
                // Refused, never masked. The schema's `maximum: 3822` is the coarse mechanical half; this
                // is the exact-mask rule it cannot express, and it stands to that bound exactly as
                // `write_memory`'s must-fit-`width` stands to its `value` bound.
                if n & !0x0EEE != 0 {
                    return Err(RpcError::invalid_params(format!(
                        "`raw` {n:#06X} carries bits outside the chip's 0x0EEE mask \
                         (---- BBB- GGG- RRR-) — refused, never masked"
                    )));
                }
                n as u16
            }
        };

        let entry = line * 16 + index;
        let stored = self.sys.vdp_mut().poke_cram(entry, word);
        Ok(json!({
            "line": line,
            "index": index,
            "cramAddr": hex::addr(u32::from(entry) * 2),
            "value": hex::u16_hex(stored),
        }))
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

    /// `emulator/memory_hash` — fingerprint a byte range without moving it (§6 memory, CR-23 /
    /// §11.13). A pure read: no `require_paused`, answered at the engine thread's single coherent
    /// point like every other handler. Routes via `debug_read` (the two-region rule the contract
    /// spells out); the FNV is `state_hash`'s family with the contract's pinned parameters, the
    /// CRC-32 is IEEE/zlib so a cart-window hash matches the ROM file.
    fn memory_hash(&mut self, params: &Value) -> Result<Value, RpcError> {
        let addr = self.resolve_exclusive_target(params)?;
        let Some(l) = params.get("len") else {
            return Err(RpcError::invalid_params(
                "`len` is required — a hash without a length hashes nothing",
            ));
        };
        let len = hex::parse_count("len", l, 1, self.config.max_hash_len)?;
        let (data, region) = self.debug_read(addr, len as usize)?;
        Ok(json!({
            "addr": hex::addr(addr),
            "len": data.len(),
            "region": region,
            "fnv1a64": oracle_core::state_hash::hex(oracle_core::state_hash::fnv1a_bytes(&data)),
            "crc32": format!("0x{:08X}", crate::crc32::crc32(&data)),
        }))
    }

    // ---------------------------------------------------------------- the profiler (§6, CR-26)

    /// `emulator/set_profiler` — arm or disarm the accountant (§6, §11.16).
    ///
    /// **Arming RESETS**; disarming RETAINS. There is no resume in this revision, so a second arm discards
    /// an in-flight sample — which is why `enabled: true` builds a fresh `Profiler` rather than flipping a
    /// flag. Disarm only stops the feed: a client arms, runs, disarms and reads, and the read must still
    /// answer.
    ///
    /// **Synchronous, and never refused for run state.** No `require_paused` here: the sample's edges are
    /// frame boundaries, not the instant this command landed, so a free-running arm is exactly as well
    /// defined as a paused one. The arm takes effect on the very next run, because it is engine state and
    /// the run reads it when it builds its sink.
    fn set_profiler(&mut self, params: &Value) -> Result<Value, RpcError> {
        let Some(enabled) = params.get("enabled").and_then(Value::as_bool) else {
            return Err(RpcError::invalid_params(
                "`enabled` is required and must be a boolean",
            ));
        };
        let per_frame = match params.get("perFrame") {
            None => false,
            Some(v) => v.as_bool().ok_or_else(|| {
                RpcError::invalid_params("`perFrame` must be a boolean when present")
            })?,
        };
        // **Every arming flag resets together** (§11.18). A second arm starts a fresh sample under exactly
        // the lenses *this* call names, so a client arming `callers` on an already-armed instrument gets
        // caller data for a new sample and not for the one it was watching.
        let callers = match params.get("callers") {
            None => false,
            Some(v) => v.as_bool().ok_or_else(|| {
                RpcError::invalid_params("`callers` must be a boolean when present")
            })?,
        };
        if enabled {
            self.profiler = Profiler::with_lenses(
                if per_frame {
                    self.config.max_profiler_frames
                } else {
                    0
                },
                callers,
            );
            // Latched at the arm, so a basis that changes mid-sample is detectable at the read rather
            // than silently averaged over.
            self.profiler_basis = Some(self.sys.timing_basis());
        }
        self.profiler_armed = enabled;
        Ok(json!({
            "enabled": self.profiler_armed,
            // Echoed rather than assumed: it is the key that decides whether `get_profiler_frames`
            // answers a `frames` param or refuses it.
            "perFrame": self.profiler.per_frame_armed(),
            // The same, for the lens (§11.18). Present because THIS server implements it and advertises
            // `limits.maxProfilerCallers`; a server without the lens omits both, so absence means *no
            // caller lens here* and never *the lens is off*. Read off the instrument rather than from
            // `callers` above, so a disarm reports what the retained sample actually holds.
            "callers": self.profiler.callers_armed(),
        }))
    }

    /// **A timeline jump drops the sample and keeps the arming** (§6, CR-26; the N4 ruling).
    ///
    /// `emulator/reset`, `emulator/reload_rom` and `emulator/restore` do not advance the machine — they
    /// *replace* it. The measurement that was in flight belongs to the machine that is no longer here, and
    /// the rule this follows is the one the rest of this surface follows: **never serve a dead machine's
    /// data**. Keeping it would let a client divide cycles from two unrelated timelines by one frame count
    /// and get a per-frame figure of nothing at all.
    ///
    /// `restore` is the case that proves it rather than merely illustrating it: a checkpoint's machine has
    /// its own stack, so the profiler's **shadow stack** — open frames keyed by the `entry_sp` they were
    /// entered at — is describing returns that will now never come. Carrying it across would not just
    /// blend two samples; it would mis-attribute the new machine's returns to the old machine's frames.
    ///
    /// **The arming survives.** `enabled` and `perFrame` are the client's *instruction*, not the machine's
    /// state: a client that armed the accountant and then rewound to a checkpoint wants to measure what
    /// happens next, and silently disarming would answer its next read with an empty sample it had no way
    /// to predict. So the instrument is rebuilt in the pose it was armed in and measurement restarts from
    /// the new timeline's first boundary. The basis is re-latched for the same reason it is latched at the
    /// arm: the machine it describes has just been replaced.
    fn restart_profiler_sample(&mut self) {
        if !self.profiler_armed && self.profiler.frames() == 0 {
            return; // nothing armed and nothing held: rebuilding would be a no-op with a cost
        }
        self.profiler = Profiler::with_lenses(
            if self.profiler.per_frame_armed() {
                self.config.max_profiler_frames
            } else {
                0
            },
            // The lens is the client's instruction too, so it survives the jump with the rest of the pose.
            self.profiler.callers_armed(),
        );
        if self.profiler_armed {
            self.profiler_basis = Some(self.sys.timing_basis());
        }
    }

    /// `emulator/get_profiler` — the instrument's state, not its data (§6, §11.16). A pure read that
    /// clears nothing and is never refused for run state.
    fn get_profiler(&mut self, _params: &Value) -> Result<Value, RpcError> {
        Ok(json!({
            "enabled": self.profiler_armed,
            "perFrame": self.profiler.per_frame_armed(),
            // The third arming fact (§11.18), on the same conditional-presence rule `set_profiler`'s echo
            // takes. This method reports the instrument's STATE and carries no rows, so a caller LIST — and
            // the `callersNotArmed` refusal that goes with it — cannot appear here at all.
            "callers": self.profiler.callers_armed(),
            // The SAME number `get_profiler_frames` reports as `frameCount`. The legacy surface had two
            // counts that could differ — one echoed the request, one counted pushes — and only one of them
            // was ever the divisor.
            "framesRecorded": self.profiler.frames(),
            "routineCount": self.profiler.routine_count(),
        }))
    }

    /// `emulator/get_profiler_frames` — the accumulated sample (§6, §11.16).
    ///
    /// Division happens **here**, in the server, over a sample delimited by frame boundaries at both ends.
    /// Every row figure and both buckets are therefore emitted **twice**: divided, and undivided as a
    /// `*Total` partner (§11.16 delta 3). The pair is tied — `divided == total / frameCount` when
    /// `frameCount > 0` — so a total bounds its partner's truncation rather than being a second reading.
    ///
    /// The reconciliation identity every reply satisfies, in the form a client should check:
    ///
    /// ```text
    /// Σ routines[].cyclesSelfTotal + Σ interrupts[].cyclesSelfTotal + unattributedCycles == sampleCycles
    /// ```
    ///
    /// — **exact, unconditionally**: no `× frameCount`, no `perFrameExact` branch, every term a REQUIRED
    /// key. The divided view reconstructs the same identity as
    /// `(Σ cyclesSelf) × frameCount + unattributedCycles`, which closes on the nose when `perFrameExact`
    /// and otherwise falls short by at most `frameCount − 1` per summed figure and never over. That
    /// hedging is a property of the divided view alone.
    fn get_profiler_frames(&mut self, params: &Value) -> Result<Value, RpcError> {
        // `top` bounds the rows. Refused above the cap, never clamped: the legacy surface clamped, so a
        // caller could not tell a full list from a clipped one.
        let top = match params.get("top") {
            None => self.config.max_profiler_routines,
            Some(v) => {
                hex::parse_count("top", v, 1, self.config.max_profiler_routines as u64)? as usize
            }
        };
        // `frames` bounds the per-frame list — and cannot affect an answer that has no per-frame list, so
        // it is REFUSED rather than ignored. This refusal is about the INSTRUMENT's state, which is why the
        // run-state exemption above does not reach it.
        let frames = match params.get("frames") {
            None => self.config.max_profiler_frames,
            Some(v) => {
                if !self.profiler.per_frame_armed() {
                    return Err(RpcError::new(
                        code::INVALID_STATE,
                        "`frames` bounds the per-frame list, and this sample was not armed with \
                         set_profiler{perFrame:true} — arm it and re-run, or drop the param",
                    )
                    .with_data(json!({"reason": "perFrameNotArmed"})));
                }
                hex::parse_count("frames", v, 1, self.config.max_profiler_frames as u64)? as usize
            }
        };
        // `topCallers` narrows an armed row's edge list and never conjures one — presence is decided by the
        // ARM, exactly as the per-frame list's is. So the same two refusals apply, in the same order:
        // -32005 when the lens is not armed (a parameter that cannot affect the answer is worse than one
        // that is rejected), -32602 above the advertised cap, refused and never clamped.
        let top_callers = match params.get("topCallers") {
            None => self.config.max_profiler_callers,
            Some(v) => {
                if !self.profiler.callers_armed() {
                    return Err(RpcError::new(
                        code::INVALID_STATE,
                        "`topCallers` bounds each routine row's caller list, and this sample was not \
                         armed with set_profiler{callers:true} — arm it and re-run, or drop the param",
                    )
                    .with_data(json!({"reason": "callersNotArmed"})));
                }
                hex::parse_count("topCallers", v, 1, self.config.max_profiler_callers as u64)?
                    as usize
            }
        };

        let report = self.profiler.report();
        let n = report.frame_count;

        // The undivided partners come from the very map `report()` divided, read back through the
        // accessor rather than recomputed — so a row's `cyclesTotal` cannot be a second measurement that
        // disagrees with the `cycles` beside it. Keyed identically by construction; `unwrap_or_default`
        // is the type's requirement, not a fallback anything is expected to take.
        let sample_rows = self.profiler.sample_routines();
        // **The call edges, indexed by callee**, so a row's list is a lookup rather than a scan of the whole
        // edge map per row. Divided and undivided are paired here for the row's own reason: the undivided
        // partner is read back out of the very accumulator `report()` divided, so an edge's `cyclesTotal`
        // cannot be a second measurement that disagrees with the `cycles` beside it.
        //
        // Empty unless the lens is armed, which is what makes an unarmed reply byte-identical to the one
        // this surface sent before the lens existed: nothing below has a set to emit.
        let sample_edges = self.profiler.sample_callers();
        let mut edges: std::collections::BTreeMap<u32, Vec<(CallerKey, EdgeCounts, EdgeCounts)>> =
            std::collections::BTreeMap::new();
        for (&(callee, caller), &c) in &report.callers {
            let t = sample_edges
                .get(&(callee, caller))
                .copied()
                .unwrap_or_default();
            edges.entry(callee).or_default().push((caller, c, t));
        }
        for list in edges.values_mut() {
            list.sort_by(profiler_edge_order);
        }
        // Rows, ordered by `cycles` descending so a truncated list is the expensive end rather than an
        // arbitrary slice.
        //
        // **Then by `cyclesTotal` descending, and only then by address.** The divided figure is floored,
        // so on a long sample many genuinely different rows share one `cycles` value — and with the
        // address as the only tie-break, `top` would then keep the *lowest-addressed* of them rather than
        // the most expensive, which is the one thing this ordering exists to prevent. The undivided
        // partner separates them exactly (it is the same accumulator, unfloored), so this is a strict
        // refinement: it can only reorder rows the old comparator called equal. The address stays last,
        // so the order is still total and two identical boots cannot disagree — a spread of 0 across
        // boots is this surface's bar.
        let mut rows: Vec<(u32, Counts, Counts)> = report
            .routines
            .into_iter()
            .map(|(addr, c)| (addr, c, sample_rows.get(&addr).copied().unwrap_or_default()))
            .collect();
        rows.sort_by(profiler_row_order);
        let total_rows = rows.len();
        let items: Vec<Value> = rows
            .into_iter()
            .take(top)
            .map(|(addr, c, t)| {
                self.profiler_row(
                    addr,
                    c,
                    t,
                    self.profiler
                        .callers_armed()
                        .then(|| edges.get(&addr).map(Vec::as_slice).unwrap_or_default()),
                    top_callers,
                )
            })
            .collect();

        let sample_buckets = self.profiler.sample_interrupts();
        let bucket = |level: u8| {
            let c = report.interrupts.get(&level).copied().unwrap_or_default();
            // A bucket is emitted for both causes whether or not the sample has one, so the default here
            // is a real case: an all-zero pair, which the pair invariant is satisfied by.
            let t = sample_buckets.get(&level).copied().unwrap_or_default();
            json!({
                "cycles": c.cycles,
                "cyclesSelf": c.self_cycles,
                "stallCycles": c.stall_cycles,
                "calls": c.calls,
                "cyclesTotal": t.cycles,
                "cyclesSelfTotal": t.self_cycles,
                "stallCyclesTotal": t.stall_cycles,
                "callsTotal": t.calls,
            })
        };

        let mut out = json!({
            "frameCount": n,
            "sampleCycles": report.sample_cycles,
            "totalCycles": report.total_cycles,
            "unattributedCycles": report.unattributed_cycles,
            "abandonedFrames": report.abandoned_frames,
            "depthExceeded": report.depth_exceeded,
            "perFrameExact": report.per_frame_exact,
            "routines": rpc::bounded_array(items, total_rows, 0, top),
            "interrupts": {
                "hint": bucket(oracle_core::profiler::LEVEL_HINT),
                "vint": bucket(oracle_core::profiler::LEVEL_VINT),
            },
        });

        // `budgetPct` is DERIVED from the basis this same server advertises — never a hardcoded NTSC
        // constant, which is wrong by ~16% the moment the machine is PAL. Exactly one of the two keys.
        //
        // **The omitted arm is currently unreachable, and that is recorded rather than removed.** The
        // basis can only *change* if the machine can hold more than one, and today `TimingBasis` has a
        // single NTSC value — so `budget_pct` never answers `None` and no test can drive this branch
        // honestly. It stays because the day a second basis exists (PAL) the branch is the difference
        // between an omitted figure and a wrong one, and three design notes belong with it for that day:
        // the basis is latched at the ARM (`set_profiler`), the comparison is against the basis *now*,
        // and a sample that straddled a change has no single budget to be a percentage of — which is why
        // the answer is an omission with a reason and not an average of two bases.
        match self.budget_pct(report.total_cycles) {
            Some(pct) => out["budgetPct"] = json!(pct),
            None => out["budgetPctOmitted"] = json!("timingBasisChanged"),
        }

        if self.profiler.per_frame_armed() {
            let ring = self.profiler.per_frame();
            let held = ring.len();
            let rows: Vec<Value> = ring
                .iter()
                .skip(held.saturating_sub(frames)) // the most recent `frames`
                .map(|f| {
                    json!({
                        "frame": f.frame,
                        "cycles": f.cycles,
                        "stallCycles": f.stall_cycles,
                        "hintCycles": f.hint_cycles,
                        "vintCycles": f.vint_cycles,
                    })
                })
                .collect();
            // Offset `0`, deliberately: this window is the most-recent TAIL of the ring, not a forward
            // page of it, so the question `truncated` answers — "does the client have everything?" — is
            // `returned < total`, which is what an offset of 0 computes. Passing the tail's real start
            // would make a 2-of-4 reply claim `truncated: false`, i.e. the exact confusion §2.4 clause (a)
            // requires the key to prevent.
            //
            // **`total` is the sample's frame count, not the ring's occupancy**, and the difference is the
            // whole point of the key. The ring is bounded (`limits.maxProfilerFrames`), so a sample longer
            // than it has already *dropped* its oldest rows — and a `total` taken from `ring.len()` would
            // equal `returned`, making `truncated: false` on a reply that is missing hundreds of frames.
            // Answering with the frames the sample actually has makes the shortfall visible, which is what
            // §2.4 clause (a) asks the pair to say.
            out["perFrame"] = rpc::bounded_array(rows, n as usize, 0, frames);
        }

        // §2.4's advisory, applied: a caveat present on every reply is one clients learn to ignore, so it
        // appears only when there is something to say.
        if let Some(c) = profiler_caveat(report.abandoned_frames, report.depth_exceeded) {
            out["caveat"] = json!(c);
        }
        Ok(out)
    }

    /// One `routines[]` row. `addr` is canonical 24-bit; `name`/`disp` travel together or not at all.
    ///
    /// **The name is the BARE label**, never a `name+$hex` composite — §4's rule, which `$defs/symbolName`
    /// enforces by pattern. And it comes from the loaded `SymbolTable`, whose lookups refuse equate rows in
    /// both directions (`oracle_core::symbols`'s
    /// `equates_are_not_addressable_in_either_direction`), so a `.lst` full of `EQU` constants cannot put a
    /// non-address symbol on a row keyed by an address.
    ///
    /// **Bounded by [`MAX_SYMBOL_DISPLACEMENT`], like every other symbolised address in this house.** An
    /// unbounded `resolve` answers with the nearest *preceding* label however far back it is, so a row whose
    /// entry sits in data, in a gap, or past the end of the listing's coverage would be named after a
    /// routine thousands of bytes earlier — legal, plausible, and wrong, which is the worst of the three.
    /// Beyond the bound the row carries **no name at all**, and the address it is keyed by is still there:
    /// the symbol was always an annotation on the address, never a replacement for it.
    ///
    /// `c` is the divided row and `t` its **undivided partner** over the whole sample (§11.16 delta 3).
    /// The two are the same four quantities, not two measurements: `t` is the accumulator `report()`
    /// divided to get `c`, so `divided == total / frameCount` holds by construction rather than by
    /// agreement. `callsTotal` is the field the ask was raised for — a per-frame count is the one figure
    /// here that division destroys rather than truncates.
    ///
    /// `edges` is `Some` **exactly when the caller lens is armed** (§11.18), and the four `callers*` keys
    /// then arrive **as a set** — the fragment ties them with `dependentRequired`, so a half-served lens
    /// cannot pass validation. `None` emits none of them, which is what keeps an unarmed reply byte-
    /// identical to the one this surface sent before the lens existed.
    fn profiler_row(
        &self,
        addr: u32,
        c: Counts,
        t: Counts,
        edges: Option<&[(CallerKey, EdgeCounts, EdgeCounts)]>,
        top_callers: usize,
    ) -> Value {
        let mut row = json!({
            "addr": hex::addr(addr),
            "cycles": c.cycles,
            "cyclesSelf": c.self_cycles,
            "stallCycles": c.stall_cycles,
            "calls": c.calls,
            "cyclesTotal": t.cycles,
            "cyclesSelfTotal": t.self_cycles,
            "stallCyclesTotal": t.stall_cycles,
            "callsTotal": t.calls,
        });
        if let Some(table) = self.symbols.as_ref() {
            if let Some(r) = table.resolve_within(addr, MAX_SYMBOL_DISPLACEMENT) {
                row["name"] = json!(r.symbol.name);
                row["disp"] = json!(r.displacement);
            }
        }
        if let Some(edges) = edges {
            // §2.4's FLAT spelling scoped to an ITEM (§11.18 widened that section for exactly this case):
            // the three companions ride as PREFIXED siblings of the row rather than as a nested
            // `{items,total,…}` container, because a container inside an item of a container makes a client
            // reach an element through three levels of `.items` and read the word `total` at two scopes.
            //
            // **`callersTotal` is the count before bounding**, which is what makes it the true number of
            // distinct callers: the cap is a REPLY bound, not a retention bound, so nothing was thrown away
            // to produce it. No `callersLimit` sibling — the applied ceiling is one number for the whole
            // reply, and repeating it per row would be a constant field wearing signal's clothes. No cursor
            // either: this method accepts no continuation param (§2.4 clause b).
            let items: Vec<Value> = edges
                .iter()
                .take(top_callers)
                .map(|(caller, c, t)| self.profiler_caller_edge(*caller, *c, *t))
                .collect();
            row["callersReturned"] = json!(items.len());
            row["callersTruncated"] = json!(items.len() < edges.len());
            row["callersTotal"] = json!(edges.len());
            row["callers"] = Value::Array(items);
        }
        row
    }

    /// One `callers[]` entry: **one call edge**, symmetric with the routine row wherever the two describe
    /// the same quantity (§11.18).
    ///
    /// Every divided figure carries its undivided partner, which is the row's own discipline at a smaller
    /// denominator — and a smaller denominator is where division does *more* damage, not less. **No
    /// `stallCycles`**: the requesting client declined a per-edge stall figure on measured grounds, and the
    /// fragment bars the key outright rather than leaving it undeclared.
    ///
    /// `callerAddr` is **absent rather than fabricated** when there is no calling routine to name, and
    /// `entryKind` then says which of the three genuinely different absences it is. The two are a
    /// biconditional the fragment enforces with `if`/`then`/`else`, so a reply that carried both, or
    /// neither, would fail validation on the way out rather than reach a client.
    fn profiler_caller_edge(&self, caller: CallerKey, c: EdgeCounts, t: EdgeCounts) -> Value {
        let mut edge = json!({
            "cycles": c.cycles,
            "cyclesSelf": c.self_cycles,
            "calls": c.calls,
            "cyclesTotal": t.cycles,
            "cyclesSelfTotal": t.self_cycles,
            "callsTotal": t.calls,
        });
        match caller {
            CallerKey::Routine(addr) => {
                edge["callerAddr"] = json!(hex::addr(addr));
                // The BARE label and an integer displacement beside it, on the row's own rule — and with
                // the row's caveat sharpened: when the caller is the frame the sample OPENED on, this
                // address is real but is not an entry point, so a non-zero `callerDisp` here is the
                // ordinary answer rather than a failed lookup.
                if let Some(table) = self.symbols.as_ref() {
                    if let Some(r) = table.resolve_within(addr, MAX_SYMBOL_DISPLACEMENT) {
                        edge["callerName"] = json!(r.symbol.name);
                        edge["callerDisp"] = json!(r.displacement);
                    }
                }
            }
            CallerKey::Interrupt(level) => {
                // Split BY CAUSE, never collapsed into one `interrupt` value: this family keys interrupt
                // accounting by the acknowledged level everywhere else, and these two spellings join the
                // edge to the bucket it came from by name.
                debug_assert!(
                    level == oracle_core::profiler::LEVEL_HINT
                        || level == oracle_core::profiler::LEVEL_VINT,
                    "this machine's VDP drives levels 4 and 6 only (`Vdp::ipl`) and the wire enum carries \
                     exactly those two causes; level {level} has no spelling"
                );
                edge["entryKind"] = json!(if level == oracle_core::profiler::LEVEL_VINT {
                    "vint"
                } else {
                    "hint"
                });
            }
            CallerKey::Root => edge["entryKind"] = json!("root"),
            CallerKey::DepthCap => edge["entryKind"] = json!("depthCap"),
        }
        edge
    }

    /// `totalCycles` as a percentage of one frame's CPU-cycle budget, or `None` when the basis changed
    /// inside the sample and no single budget describes it.
    fn budget_pct(&self, total_cycles: u64) -> Option<f64> {
        let now = self.sys.timing_basis();
        match self.profiler_basis {
            Some(armed) if armed != now => return None,
            _ => {}
        }
        let cycles_per_frame = now.mclk_per_frame / MCLK_PER_CPU_CYCLE;
        (cycles_per_frame > 0).then(|| total_cycles as f64 * 100.0 / cycles_per_frame as f64)
    }

    /// `emulator/sprites` — the sprite attribute table as a table (§6, added by §11.10 / CR-18).
    ///
    /// A pure read: no `require_paused`, and **no `caveat`** — the envelope's `running` is the contract's
    /// answer to a torn sample, and §2.4 rule 4 makes that an active decision rather than an omission.
    fn sprites(&mut self, params: &Value) -> Result<Value, RpcError> {
        // Bounded at the table's own size: a page that could never be filled is a policy wearing a count's
        // name (§11.8). `parse_count` gives the shared -32602 spelling for a non-number or a zero.
        let limit = match params.get("limit") {
            None => SAT_SLOTS,
            Some(v) => hex::parse_count("limit", v, 1, SAT_SLOTS as u64)? as usize,
        };
        let vdp = self.sys.vdp();
        // `parsedMax` comes from core, never from a local `if h40 { 80 } else { 64 }`: the contract forbids
        // this handler deriving it, so that the number can never drift from the one the sprite walk uses.
        let parsed_max = vdp.parsed_sprite_max();
        let sat_base = vdp.sat_base() as u32;
        let decoded = vdp.sprites_decoded();
        let total = decoded.len();

        // Slot order, index-ascending — pinned by the contract precisely because the SAT's *other* reading
        // is link-ordered. `take` off the front is that order; there is no cursor to resume from.
        let items: Vec<Value> = decoded
            .iter()
            .take(limit)
            .map(|s| {
                json!({
                    "index": s.index,
                    "x": s.x,
                    "y": s.y,
                    "widthCells": s.width_cells,
                    "heightCells": s.height_cells,
                    "link": s.link,
                    "baseTile": s.tile,
                    "palette": s.palette,
                    "hflip": s.hflip,
                    "vflip": s.vflip,
                    "priority": s.priority,
                    // Always present, `false` included: the two agreeing is a real answer, and a field that
                    // only appears in the unusual case is a field nobody reads.
                    "cacheDivergence": s.cache_divergence,
                })
            })
            .collect();

        let bounded = rpc::bounded_array(items, total, 0, limit);
        let mut out = Map::new();
        // §2.4's flat spelling, as `watchpoint_hits` uses it: the list is the result, and `satBase` /
        // `parsedMax` are scalars beside it rather than a container level nothing would read.
        out.insert("sprites".into(), bounded["items"].clone());
        // 80 always — the size of the table, not the parse cap and not the page. An H32 server reporting
        // 64 here would be a defensible misreading, which is why the schema pins it as a const.
        out.insert("total".into(), bounded["total"].clone());
        out.insert("returned".into(), bounded["returned"].clone());
        out.insert("limit".into(), bounded["limit"].clone());
        out.insert("truncated".into(), bounded["truncated"].clone());
        out.insert("satBase".into(), json!(hex::addr(sat_base)));
        out.insert("parsedMax".into(), json!(parsed_max));
        Ok(Value::Object(out))
    }

    /// `emulator/scanlines` — read the drawn rows back (§6 VRAM/CRAM/layers, CR-24 / §11.14).
    ///
    /// A pure read of the retained frame [`Engine::framebuffer`] already serves `screenshot` from: the live
    /// per-line raster when a completed frame is retained, the state render otherwise — and the answering
    /// `source` says which, because a row whose provenance is unstated is structurally blind to exactly the
    /// mid-frame effects this row exists to see. No `require_paused`, exactly as `read`, `sprites` and
    /// `pixel_attribution`: the envelope's `running` is the contract's whole answer to a torn sample.
    ///
    /// The bounds are **refused, never clipped** (`-32602`). A clipped range hands back fewer rows than were
    /// asked for with nothing on the wire saying so, which is the failure mode `hex::parse_count` exists to
    /// rule out; the sum bound is checked here because no static schema can see it.
    fn scanlines(&mut self, params: &Value) -> Result<Value, RpcError> {
        let lines = u64::from(ACTIVE_LINES);
        let start = match params.get("startLine") {
            None => 0,
            Some(v) => hex::parse_count("startLine", v, 0, lines - 1)?,
        };
        // The default is "through line 223" — the whole active display from wherever the caller started, so
        // that `{}` means the picture and a caller need not know 224 to ask for it.
        let count = match params.get("count") {
            None => lines - start,
            Some(v) => hex::parse_count("count", v, 1, lines)?,
        };
        if start + count > lines {
            return Err(RpcError::invalid_params(format!(
                "`startLine` {start} + `count` {count} runs past the active display \
                 (lines 0..={}) — refused, never clipped",
                lines - 1
            )));
        }

        let (width, fb, from_raster) = self.framebuffer();
        // `mode` is derived from the answering frame's own width rather than from a VDP register read: the
        // frame readers normalize a mid-frame width switch to the width the frame ended on, and a `mode`
        // taken from the register could name a width the rows do not have. The fragment ties
        // mode <-> width <-> rgb length with an if/then, so a disagreement here is a rejected reply.
        let mode = if width == 320 { "h40" } else { "h32" };
        let rows: Vec<Value> = (start..start + count)
            .map(|line| {
                let off = line as usize * width;
                // D9 category 1: `0x` + `RR``GG``BB` per pixel, left to right, S/H already applied by the
                // renderer that produced these values.
                let flat: Vec<u8> = fb[off..off + width]
                    .iter()
                    .flat_map(|&(r, g, b)| [r, g, b])
                    .collect();
                json!({"line": line, "width": width, "rgb": hex::bytes(&flat)})
            })
            .collect();

        let mut out = json!({
            "startLine": start,
            "mode": mode,
            "source": if from_raster { "raster" } else { "stateRender" },
            "rows": rows,
        });
        if !from_raster {
            // Declared, not omitted by accident, and only on the fallback — `screenshot`'s precedent. The
            // caveat's tie to `source` is left mechanically unenforced in the fragment (the CR-24 ruling's
            // disposition (a), matching `screenshot`), which makes emitting it correctly here the only
            // thing standing between a caller and a silently post-hoc answer.
            out["caveat"] = json!(
                "no completed frame is retained — the machine has not drawn one yet, or reset/reload_rom/\
                 restore dropped it — so these rows are rendered from the VDP state as it stands right \
                 now. Mid-frame CRAM/scroll changes that a real raster would show on different lines are \
                 NOT reproduced — run at least one frame for a scanline-accurate readback."
            );
        }
        Ok(out)
    }

    fn screenshot(&mut self, params: &Value) -> Result<Value, RpcError> {
        let path: PathBuf = match params.get("path") {
            None => std::env::temp_dir().join(format!("oracle-frame-{}.png", self.frame())),
            Some(Value::String(s)) => PathBuf::from(s),
            Some(_) => return Err(RpcError::invalid_params("`path` must be a string")),
        };
        let (width, fb, from_raster) = self.framebuffer();
        // PNG, not the PPM this wrote before. A PPM is what a project emits *before* anyone asks for
        // screenshots: nothing displays it inline, so the reference MCP was handing a model 200 KB of
        // undecodable bytes labelled `image/png`. The encoder is ours (`crate::png`) rather than a
        // dependency — the same call RetroArch and stb_image_write made — so this crate's runtime deps
        // stay `oracle-core` + `serde_json`.
        let bytes = crate::png::encode(&fb, width as u32, u32::from(ACTIVE_LINES));
        std::fs::write(&path, &bytes).map_err(|e| {
            RpcError::new(
                code::INTERNAL_ERROR,
                format!("cannot write {}: {e}", path.display()),
            )
            .with_data(json!({"path": path.display().to_string()}))
        })?;
        let mut out = json!({
            "path": path.display().to_string(),
            "format": "png",
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
        let mclk_before = self.sys.scheduler().now();
        let run = self.advance(frames);
        self.running = false;
        let frames = self.frames_advanced(&run, frames, mclk_before);
        // Restore exactly the held set — a tap must not leak into later frames, and must not clear a
        // button the client is separately holding (nor one the human is physically holding).
        self.apply_pads();
        let pc = self.sys.cpu_regs().pc;
        // A tap advances whole **frames**, so `step` is affirmatively wrong: §3 pins it as "one
        // instruction, or one instruction-shaped unit … **not** the value for a frame advance". This server
        // emitted `runFrames` here before CR-9 was ruled on, calling it "merely imprecise" and recording the
        // residual ambiguity — *which method* drove the advance — as the open question.
        //
        // **CR-9 ruled that `runFrames` was not imprecise but exactly right, and that the missing half was
        // never the enum.** §3 now defines the value by its condition — "a bounded frame advance ran to
        // completion — `emulator/run_frames`, `emulator/press`, or any future method whose stop condition is
        // an exhausted frame count" — and the two drafted options were both refused: a ninth enum value
        // would have made this enum name a *caller* for the first time, and a bare `reason: "press"` still
        // would not have said *what* was pressed or on which pad, which is precisely what a subscriber that
        // was not the caller cannot otherwise recover. So the input rides as params, and this is the one
        // call site that supplies them.
        self.emit_run_stop(&run, pc, frames, Some((&buttons, port)));
        Ok(json!({
            "buttons": buttons,
            "frames": frames,
            "port": port,
            "frameToken": self.frame(),
        }))
    }

    /// `emulator/play_input` — the pad as a timeline (§6 input, added by §11.11 / CR-19).
    ///
    /// **The pad at frame N is a pure function of `rows`, and of nothing else.** Both non-row sources are
    /// suspended for the duration and restored afterwards: the client's `held` set *and* the host's
    /// `live` input. That is why this writes `set_pad` directly from the rows each frame instead of going
    /// through [`Engine::apply_pads`], which merges all three — merging is exactly the accumulation this
    /// method exists to remove, and "apply the rows on top of what is already held" is the easier
    /// implementation the contract had to forbid by name.
    fn play_input(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/play_input")?;
        let rows = parse_input_rows(params, self.config.max_input_rows)?;

        // Absent, the ceiling is the timeline's own length; present, it may TRUNCATE it. Rows starting at
        // or beyond the ceiling simply never apply — which falls out of the loop bound rather than needing
        // a rule of its own.
        let largest_end = rows.iter().map(|r| r.end).max().unwrap_or(0);
        let max_frames = match params.get("maxFrames") {
            None => largest_end.min(self.config.max_run_frames),
            Some(v) => hex::parse_count("maxFrames", v, 0, self.config.max_run_frames)?,
        };
        let total = largest_end.min(max_frames);

        self.running = true;
        self.emit_resumed();
        let mclk_before = self.sys.scheduler().now();
        // One `resumed` at the start and one `stopped` at the end — never one per frame, even though the
        // machine really is advanced a frame at a time. The per-frame advance is how the pad gets applied
        // at each boundary; it is not a sequence of runs the client asked for.
        let mut run: Option<Advanced> = None;
        let mut completed = 0u64;
        for frame in 0..total {
            for port in 0..2 {
                // A port no row covers is fully released — `Pad::default()`, not whatever was held.
                self.sys.set_pad(port, pad_at(&rows, port, frame));
            }
            let step = self.advance(1);
            let stopped = step.stopped_by.is_some();
            run = Some(step);
            if stopped {
                break;
            }
            completed += 1;
        }
        // A timeline truncated to zero frames still owes a record — CR-17 made the 0-frame outcome
        // reachable, and a zero-frame advance is the honest way to produce one.
        let run = match run {
            Some(r) => r,
            None => self.advance(0),
        };
        self.running = false;
        // Exact, including zero (§11.9): a watch with `stopAfter` can end the run inside frame 0, and the
        // completed-frame count is then whole frames of elapsed mclk, the same arithmetic `press` uses.
        let frames = if run.stopped_by.is_some() {
            self.frames_advanced(&run, completed, mclk_before)
        } else {
            completed
        };
        // Restore both suspended sources, unchanged. Not cleared: a button the human is physically holding
        // is not this method's to release.
        self.apply_pads();
        let pc = self.sys.cpu_regs().pc;
        self.emit_run_stop(&run, pc, frames, None);
        Ok(json!({
            "frames": frames,
            "frameToken": self.frame(),
            "pc": hex::addr(pc),
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
            // §4, rewritten 2026-08-15: `name` is the **identifying** spelling on every branch and it
            // MUST round-trip. This branch used to emit `Resolution`'s `Display` — the *readable* name
            // with a `+$hex` displacement glued on — which meant the one field D7 exists to make reliable
            // was the one field that could not be handed back: `lookup_symbol {name:"EntryPoint+$10"}`
            // answered `-32013`. The readable form moves to `demangled` (display only) and the
            // displacement stays in `disp`, where it already was. `$defs/symbolName` now rejects the old
            // spelling outright, which is D14's reason for expressing the rule as a pattern.
            let mut out = json!({
                "query": hex::addr(addr),
                "name": r.symbol.name,
                "addr": hex::addr(r.symbol.addr),
                "disp": r.displacement,
                "ambiguous": r.symbol.demangled_ambiguous,
                "synthetic": r.symbol.is_synthetic,
            });
            if r.symbol.demangled != r.symbol.name {
                out["demangled"] = json!(r.symbol.demangled);
            }
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
                     once), so `demangled` does not identify a location — `name` is the unique one."
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
            // §4's exact shape. `exact` is REQUIRED on the name direction and present in **both** cases —
            // it used to appear only on the prefix branch, where it is always `false`, which is a field
            // nobody reads (§11.5: "`released`'s defect with a useful name").
            let mut out = json!({
                "name": sym.name,
                "addr": hex::addr(sym.addr),
                "rawAddr": hex::addr(sym.raw_addr),
                "ambiguous": sym.demangled_ambiguous,
                "exact": true,
            });
            // "Present when it differs from `name`" (§4), same rule as the other three branches. An
            // unmangled listing does not pay for a key that repeats `name` verbatim.
            if sym.demangled != sym.name {
                out["demangled"] = json!(sym.demangled);
            }
            return Ok(out);
        }
        let exact_demangled = table.by_demangled(name);
        if !exact_demangled.is_empty() {
            let total = exact_demangled.len();
            let items: Vec<Value> = exact_demangled
                .iter()
                .copied()
                .take(limit)
                .map(match_item)
                .collect();
            let first = exact_demangled[0];
            // Still `exact: true`: the query matched a symbol's readable spelling **exactly**, and §4
            // reads `exact` as "a symbol has exactly this name" against "these are prefix guesses".
            // Nothing here was guessed. What the client cannot rely on is that the *readable* name
            // identifies a location, and that is `ambiguous`'s job, not `exact`'s. `query` is carried
            // because the reply's `name` is the mangled spelling — not the one that was asked for — so
            // without it the reply does not record the request (§4's own reason for the field).
            let mut out = json!({
                "query": name,
                "name": first.name,
                "demangled": first.demangled,
                "addr": hex::addr(first.addr),
                "otherMatches": rpc::bounded_array(items, total, 0, limit),
                "ambiguous": total > 1,
                "exact": true,
            });
            if total > 1 {
                out["caveat"] = json!(format!(
                    "{total} different addresses answer to this readable name; `addr` is only the \
                     first. Use the unique `name` from `otherMatches`."
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
        // **One** item shape, not one per branch (§4/CR-14). This branch used to emit `demangled`
        // unconditionally and the branch above used to omit it, so a client had to know which branch it
        // was on in order to read the list.
        let items: Vec<Value> = all.iter().copied().take(limit).map(match_item).collect();
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
                     wrong\", never \"proven right\" (two demo shapes can declare the same EndOfRom)."
                        .to_string(),
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
            // The two Indeterminate shapes are NOT the same finding and must not share a sentence. One
            // listing gave us no offset to probe; the other gave us one and it says "there is no
            // appendix here" — which is a fact about the image, not a gap in the listing.
            RomBinding::Indeterminate(Indeterminate::EndOfRomIsImageEnd { rom_len }) => (
                true,
                Some(format!(
                    "this listing declares EndOfRom at exactly the image's end (${rom_len:X} bytes), \
                     which is the no-appendix shape a stock AS disassembly has — `RomEndLoc: dc.l \
                     EndOfRom-1` puts the symbol one past the last byte, so there is nothing to probe \
                     rather than a probe that failed. Accepted unverified because it is internally \
                     intact.",
                )),
            ),
            RomBinding::Indeterminate(Indeterminate::NoEndOfRomSymbol) => (
                true,
                Some(
                    "this listing declares no EndOfRom, so it could not be checked against the loaded \
                     ROM at all. Accepted unverified because it is internally intact."
                        .to_string(),
                ),
            ),
        };
        debug_assert!(accepted);

        let count = table.len();
        let modules = table.modules().len();
        // **Why `symbolCount` can be smaller than the listing's own footer**, answered where the consumer
        // meets the discrepancy rather than only in a doc they would have to know to look for.
        //
        // A stock AS listing emits its own build metadata as pseudo-symbols whose value is a string or a
        // float — `ARCHITECTURE : "x86_64-unknown-linux" -`, `DATE`, `TIME`, `MOMCPUNAME`, `CONSTPI` —
        // and the `N symbols` footer counts them. They carry no address, so they are consumed rather
        // than ingested (`SymbolTable::non_address_rows`) and `symbolCount` is the number of rows that
        // can actually answer a lookup. On `s1disasm`'s `sonic.lst` that is 12,405 against a declared
        // 12,410, and the five are exactly these. Carried in the EXISTING `caveat` string, deliberately:
        // a new reply key is contract surface and this is an explanation, not a datum a client branches
        // on.
        let addressless = table.non_address_rows();
        let caveat = match (caveat, addressless) {
            (c, 0) => c,
            (c, n) => {
                let note = format!(
                    "{n} row(s) in this listing declare a value that is not an address (AS emits its \
                     build metadata — ARCHITECTURE, DATE, TIME — as pseudo-symbols), so they are \
                     counted by the file's own `N symbols` footer but cannot answer a lookup and are \
                     not in symbolCount."
                );
                Some(match c {
                    Some(existing) => format!("{existing} {note}"),
                    None => note,
                })
            }
        };
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

    /// `emulator/reset` — drive the /RESET sequence against the current cartridge (§6 run-control,
    /// result defined by CR-22 / §11.13).
    ///
    /// Deliberately NOT `require_paused`: a reset replaces the machine wholesale between frames —
    /// it advances nothing and cannot fight the free-run loop — and the contract forbids changing
    /// the run state here (paused stays paused, free-running keeps running). Symbols are KEPT: the
    /// image is unchanged, so the binding that survived boot survives this (contrast `reload_rom`,
    /// which re-validates). The generation bump is `restore`'s precedent — the timeline jumped, and
    /// a hosted player resyncs off `PumpReport::rom_changed`.
    fn reset(&mut self, _params: &Value) -> Result<Value, RpcError> {
        self.sys.reset();
        // Held pads clear because a reset is a cold start and a cold start has nobody holding anything:
        // the debugger's injected input is the debugger's state, not the machine's, and a `hold` left
        // armed across a reset would silently steer the boot sequence — the exact preamble a scene
        // reproduction depends on being deterministic. (`reload_rom` clears them for the same reason.)
        self.held = [Pad::default(); 2];
        self.invalidate_screen();
        // The sample measured the machine this reset just replaced — see `restart_profiler_sample`.
        self.restart_profiler_sample();
        self.rom_generation += 1;
        Ok(json!({ "deferred": false }))
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
        self.restart_profiler_sample();
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
        // …and so does the profiler's sample, whose shadow stack is now describing a machine whose
        // returns will never come. Arming survives; the measurement restarts.
        self.restart_profiler_sample();
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

        // The house's one bounded-array rule (`rpc::bounded_array`, recon §4 non-negotiable #2, now
        // contract §2.4) computes the page; §6.1's spelling for this method names the array `checkpoints`
        // and its continuation token `cursor`, so the same envelope is emitted under the catalog's names
        // rather than a second pagination convention being invented alongside it. The helper is **not**
        // changed to understand ids — it is fed the positional `skipped`, which is exactly what its
        // `total`/`returned`/`truncated` maths needs, and the continuation token is this method's own.
        //
        // **This method is now the only place a cursor is minted on this bus** (§2.4 clause (b)): the
        // helper stopped emitting one, because `lookup_symbol` — its other caller — accepts no cursor
        // param and so may not publish a token that can never be handed back.
        let page = rpc::bounded_array(items, total, skipped, limit);
        let mut out = Map::new();
        out.insert("checkpoints".into(), page["items"].clone());
        out.insert("total".into(), page["total"].clone());
        out.insert("returned".into(), page["returned"].clone());
        out.insert("limit".into(), page["limit"].clone());
        out.insert("truncated".into(), page["truncated"].clone());
        // `cursor` is returned **when more remain** and is absent otherwise, so a client can never mistake
        // "here is where to continue" for "you are at the end". "More remain" is read off `truncated`,
        // which is the same bit the helper's old positional `nextCursor` encoded — that token was only
        // ever consulted for its existence here, never for its value, which is why removing it costs this
        // method nothing.
        //
        // The token is emitted as a **JSON string**, which is what the contract schema types it as
        // (`schema/bus-protocol.schema.json`, `emulator/checkpoint_list` params *and* result) — §8
        // forbids inventing a wire shape alongside the catalog's. The string is also the shape the
        // §6.1 cursor paragraph wants: the token is **opaque**, "a client MUST NOT parse it", and a
        // bare number is an open invitation to the `cursor + 1` / `cursor > n` arithmetic that the
        // opacity rule exists to prevent. Quoting it does not *stop* a determined client — the value
        // is still an id in decimal — but it stops the accident.
        if page["truncated"] == json!(true) {
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

    // ------------------------------------------------------- watchpoints (§6, CR-11 / CR-12)
    //
    // **None of the four is subject to §6's run-control state rule**, and §6 says so in as many words:
    // arming and clearing "mutate an **observer**, not the timeline", and `watchpoint_list`/`watchpoint_hits`
    // are pure reads. The one case that genuinely differs — a `stopAfter` watch armed while something else
    // is running the machine — is answered by *attribution* rather than by a gate: the halt always names its
    // watch (`emit_run_stop`). Refusing to arm while free-running would force a client to pause, arm and
    // resume, which is the machine-state change §5 forbids a server to make on a caller's behalf.
    //
    // **Hits are polled and never pushed**, and this server defines no per-hit event. The volumes are the
    // argument and they are this repo's own: 4,923,206 CRAM writes over 120 frames in one test ROM. Pushing
    // that would feed one bounded lossy stage (the ring) into another (the per-connection event queue) and
    // move `droppedEvents` for reasons having nothing to do with a client's ability to keep up — degrading
    // the exact signal D17 defines for `stopped` and `romReloaded`. The one push this capability needs
    // already exists: `stopped {reason:"watchpoint", watch}`.

    fn watchpoint_add(&mut self, params: &Value) -> Result<Value, RpcError> {
        let space = parse_watch_space(params)?;
        // §6: a symbol names a 68000 address, and a VDP-internal byte address has no symbol. Checked before
        // resolution so the refusal names the real mistake instead of "no symbol named …".
        if params.get("symbol").is_some() && space != WatchSpace::Bus {
            return Err(RpcError::invalid_params(format!(
                "`symbol` is valid only with space \"bus\" — a VDP-internal byte address has no symbol \
                 (got space {:?})",
                space_name(space)
            )));
        }
        let addr = self.resolve_exclusive_target(params)?;
        let len = match params.get("len") {
            None => 1,
            Some(v) => hex::parse_count("len", v, 1, MAX_WATCH_LEN)?,
        };
        // A range whose **end** runs off the bus is refused rather than clipped: a clipped watch reports a
        // negative finding about addresses it never looked at, which is the one answer this instrument must
        // never be able to give. `resolve_target` has already bounded the *base* to the 68000's 24 bits for
        // every space — that check is shared and is why the VDP spaces need no ceiling of their own here,
        // a base under 24 bits plus a `len` the schema caps at 16 MiB cannot leave a `u32`.
        let hi = u64::from(addr) + len - 1;
        if space == WatchSpace::Bus && hi > u64::from(BUS_ADDR_MAX) {
            return Err(out_of_range(
                addr,
                "the watched range would run past the end of the 68000's 24-bit address space",
            ));
        }
        let op = parse_watch_op(params)?;
        let (mode, census_key) = parse_watch_mode(params)?;
        let stop_after = match params.get("stopAfter") {
            None => None,
            Some(v) => Some(hex::parse_count("stopAfter", v, 1, u64::MAX)?),
        };
        let label =
            match params.get("label") {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.clone(),
                Some(_) => return Err(RpcError::invalid_params(
                    "`label` must be a string (it is carried back verbatim and never interpreted)",
                )),
            };

        // **D13 rule 3, verbatim.** Checked last, so a request that is *also* malformed is told about the
        // malformation rather than about a cap it would not have reached anyway — and checked at all,
        // because the alternative failure is the worst this instrument has: a silently-dropped watch reads
        // out as `seen` positive and `matched` zero, which is exactly what a genuine negative finding looks
        // like. Never grow past the number, never evict a handle a client is still holding.
        let cap = self.config.max_watches;
        let count = self.watchpoints.watch_count();
        if count >= cap {
            return Err(RpcError::invalid_state(
                "watchCapReached",
                format!(
                    "all {cap} watch slots are in use; make room first: emulator/watchpoint_clear"
                ),
                json!({"cap": cap, "count": count}),
            ));
        }

        let hi = hi as u32;
        let mut w = match space {
            WatchSpace::Bus => Watch::bus(addr..=hi, op, label.clone()),
            other => Watch::vdp(other, addr..=hi, op, label.clone()),
        }
        .mode(mode);
        if let Some(n) = stop_after {
            w = w.stop_after(n);
        }
        let id = self.watchpoints.add(w);
        self.watches_issued = self.watches_issued.max(id.0 + 1);

        // Exactly the schematized keys, and the resolved values rather than the caller's: `op` says what
        // `read`/`write` actually became, so a caller that supplied neither is told it got a write watch.
        let mut out = Map::new();
        out.insert("watch".into(), json!(watch_wire_id(id)));
        out.insert("space".into(), json!(space_name(space)));
        out.insert("addr".into(), json!(hex::addr(addr)));
        out.insert("len".into(), json!(len));
        out.insert("op".into(), json!(op_name(op)));
        out.insert("mode".into(), json!(mode_name(mode)));
        if let Some(k) = census_key {
            // Always `Some` here: `parse_watch_mode` is this path's only constructor and it accepts exactly
            // the three spellings §6 exposes.
            out.insert("censusKey".into(), json!(census_key_name(k)));
        }
        if let Some(n) = stop_after {
            out.insert("stopAfter".into(), json!(n));
        }
        if !label.is_empty() {
            out.insert("label".into(), json!(label));
        }
        Ok(Value::Object(out))
    }

    fn watchpoint_clear(&mut self, params: &Value) -> Result<Value, RpcError> {
        let all = match params.get("all") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(RpcError::invalid_params("`all` must be a boolean (D9)")),
        };
        if all {
            if params.get("watch").is_some() {
                return Err(RpcError::invalid_params(
                    "`watch` and `all` are mutually exclusive — pass one",
                ));
            }
            let removed = self.watchpoints.watch_count();
            self.watchpoints.clear();
            return Ok(json!({"removed": removed}));
        }
        let handle = parse_watch_handle(params, "watch")?;
        // **Deliberately permissive, and deliberately unlike the `watch` filter on `watchpoint_hits`.**
        // §6.1's rule for `checkpoint_drop` applies here for §6.1's reason: deletion is idempotent, and an
        // error a client must learn to swallow teaches clients to swallow errors. `removed: 0` is a
        // complete, machine-readable answer to "is it gone?" for a handle that was retired, was never
        // issued, or was never a handle at all. Nothing was evicted and no id changed meaning.
        let removed = match resolve_watch_handle(&handle) {
            Some(id) => usize::from(self.watchpoints.remove(id)),
            None => 0,
        };
        // Recorded hits are **not** deleted with the watch. A destructive clear would let one client erase
        // another's evidence on a shared bus, and it is what makes a retired handle legible: its hits keep
        // naming it while `watchpoint_list` no longer does, and ids are never reused, so that test cannot
        // give a false negative.
        Ok(json!({ "removed": removed }))
    }

    fn watchpoint_list(&mut self, params: &Value) -> Result<Value, RpcError> {
        let reports = self.watchpoints.watches();
        let total = reports.len();
        // The cursor is a **watch handle**, resolved to the id it stands for: "resume at the first id
        // strictly greater than this". Ids are monotonic and never reused, so a watch cleared under an
        // outstanding cursor cannot make the next page step over a live one — the positional failure §2.4
        // clause (c) forbids.
        let cursor = match params.get("cursor") {
            None => None,
            Some(v) => Some(self.parse_watch_cursor(v)?),
        };
        // House ceiling 4096, the same one `read_memory` and `watchpoint_hits` carry. The default is the
        // watch cap: there can never be more live watches than that, so a bigger page could not return more.
        let limit = match params.get("limit") {
            None => self.config.max_watches,
            Some(v) => hex::parse_count("limit", v, 1, MAX_PAGE)? as usize,
        };
        let after = cursor.map_or(0, |c| c.0 + 1);
        let skipped = reports.iter().filter(|r| r.id.0 < after).count();
        let page: Vec<&WatchReport> = reports
            .iter()
            .filter(|r| r.id.0 >= after)
            .take(limit)
            .collect();
        let next_cursor = page.last().map(|r| r.id);
        let items: Vec<Value> = page.iter().map(|r| watch_report_json(r)).collect();

        let bounded = rpc::bounded_array(items, total, skipped, limit);
        let mut out = Map::new();
        // §2.4's **flat** spelling, the same one `checkpoint_list` uses: the list here *is* the whole
        // result, so wrapping it in a `boundedList` container would buy one level of indirection and
        // nothing else. `total`/`returned`/`truncated` are required even when the page is complete.
        out.insert("watches".into(), bounded["items"].clone());
        out.insert("total".into(), bounded["total"].clone());
        out.insert("returned".into(), bounded["returned"].clone());
        out.insert("limit".into(), bounded["limit"].clone());
        out.insert("truncated".into(), bounded["truncated"].clone());
        if bounded["truncated"] == json!(true) {
            if let Some(id) = next_cursor {
                out.insert("cursor".into(), json!(watch_wire_id(id)));
            }
        }
        // The instrument is shared with the player's own panel, which holds a `&mut Watchpoints` and is not
        // limited to the three census keys §6 exposes. A watch grouped by one of core's other four is
        // reported without a `censusKey` — never relabelled as the nearest exposed one, which would put a
        // wrong name on a correct number — and this says so, because a census with no key is otherwise a
        // reader's puzzle. §2.4: optional, singular, surfaced verbatim, never parsed.
        if page
            .iter()
            .any(|r| matches!(r.mode, WatchMode::Census(k) if census_key_name(k).is_none()))
        {
            out.insert(
                "caveat".into(),
                json!(
                    "at least one listed watch groups by a census key this bus does not expose, so its                      `censusKey` is absent while its `census` counts are real — the watch was armed                      locally rather than over this socket"
                ),
            );
        }
        Ok(Value::Object(out))
    }

    fn watchpoint_hits(&mut self, params: &Value) -> Result<Value, RpcError> {
        // **`hits()`, never `take_hits()`.** A draining read is one client stealing another's evidence on a
        // shared bus — the same hazard §6.1 refuses for checkpoints — and it would make the reply's own
        // `total` unreproducible: a second identical call would answer differently for no reason the client
        // could see.
        let filter = match params.get("watch") {
            None => None,
            Some(_) => {
                let handle = parse_watch_handle(params, "watch")?;
                // Unlike `watchpoint_clear`, this one refuses a handle this server could never have issued.
                // The distinction is decidable and it matters: a **retired** handle must keep working here,
                // because clearing a watch does not delete its hits and reading them back is how a stale
                // instrument's evidence stays distinguishable from a live one's. A handle that was never
                // issued is a typo, and answering a typo with an honest-looking empty page is exactly the
                // silent wrong answer this surface exists to prevent.
                Some(self.resolve_issued_handle(&handle, "watch")?)
            }
        };
        let cursor = match params.get("cursor") {
            None => None,
            Some(v) => Some(parse_cursor(v, u64::MAX)?),
        };
        let limit = match params.get("limit") {
            None => DEFAULT_HITS_PAGE,
            Some(v) => hex::parse_count("limit", v, 1, MAX_PAGE)? as usize,
        };

        let hits = self.watchpoints.hits();
        let matching: Vec<&WatchHit> = hits
            .iter()
            .filter(|h| filter.is_none_or(|w| h.watch == w))
            .collect();
        // §2.4 clause (a)'s `total`: hits the ring currently **holds** that match this query. Not `matched`
        // (accesses, including ones no ring stored) and not `dropped` (hits the ring has discarded). Three
        // numbers, three questions.
        let total = matching.len();
        let after = cursor.map_or(0, |c| c + 1);
        let skipped = matching.iter().filter(|h| h.seq < after).count();
        let page: Vec<&WatchHit> = matching
            .into_iter()
            .filter(|h| h.seq >= after)
            .take(limit)
            .collect();
        let next_cursor = page.last().map(|h| h.seq);
        let items: Vec<Value> = page
            .iter()
            .map(|h| self.watch_hit_json(h))
            .collect::<Vec<_>>();

        let bounded = rpc::bounded_array(items, total, skipped, limit);
        let mut out = Map::new();
        out.insert("hits".into(), bounded["items"].clone());
        out.insert("total".into(), bounded["total"].clone());
        out.insert("returned".into(), bounded["returned"].clone());
        out.insert("limit".into(), bounded["limit"].clone());
        out.insert("truncated".into(), bounded["truncated"].clone());
        if bounded["truncated"] == json!(true) {
            if let Some(seq) = next_cursor {
                out.insert("cursor".into(), json!(seq.to_string()));
            }
        }
        // **The three honesty numbers, and they are three different questions.** `dropped` is loss at record
        // time and rides in the body rather than the envelope because it is an *instrument* fact — one
        // number, identical for every client — unlike `droppedEvents` (§2.3), which is per-connection.
        // `seen` is the structural negative control: `seen > 0` with `matched == 0` is a live instrument
        // that found nothing, while `seen == 0` is an instrument that was never attached to the run and a
        // zero from it means nothing at all. `matched` counts accesses across every mode, including the
        // census modes that store no hit — so it is a count of writes and never a measure of change.
        out.insert("dropped".into(), json!(self.watchpoints.dropped()));
        out.insert("seen".into(), json!(self.watchpoints.seen()));
        out.insert("matched".into(), json!(self.watchpoints.matched()));
        Ok(Value::Object(out))
    }

    /// One recorded hit, in exactly the schematized keys.
    ///
    /// The two presence rules are **structural**, not stylistic, and are enforced here as the schema
    /// enforces them on the wire: `old` is emitted **iff** the space is not `bus`, because `on_event` builds
    /// every bus hit with `old: 0` unconditionally (the 68000 bus event stream carries no prior value) and
    /// emitting that zero would assert something false; `fc` is emitted **iff** the space *is* `bus`,
    /// because `on_vdp_write` hardwires `fc: 0` and a VDP-internal write's CPU-vs-DMA attribution is `via`.
    /// Where `old` is present, `old != value` is the exact per-write change test — the measurement a raw
    /// write count misleads about.
    fn watch_hit_json(&self, h: &WatchHit) -> Value {
        let mut e = Map::new();
        e.insert("watch".into(), json!(watch_wire_id(h.watch)));
        e.insert("space".into(), json!(space_name(h.space)));
        e.insert("addr".into(), json!(hex::addr(h.addr)));
        e.insert("value".into(), json!(hex::addr(h.value)));
        if h.space == WatchSpace::Bus {
            e.insert("fc".into(), json!(h.fc));
        } else {
            e.insert("old".into(), json!(hex::addr(h.old)));
        }
        e.insert("size".into(), json!(h.size.bytes()));
        e.insert("op".into(), json!(bus_op_name(h.op)));
        e.insert("via".into(), json!(via_name(h.via)));
        e.insert("pc".into(), json!(hex::addr(h.pc)));
        if let Some((name, disp)) = self.symbol_at(h.pc) {
            e.insert("symbol".into(), json!(name));
            e.insert("symbolDisp".into(), json!(disp));
        }
        // `frame`/`mclk` live **inside** the hit and never at the top level of the result, where §2.2's
        // envelope stamp would overwrite them with the machine's *current* coordinate — a silent wrong
        // answer of exactly the class D11 exists to prevent.
        e.insert("frame".into(), json!(h.frame));
        e.insert("mclk".into(), json!(h.mclk));
        e.insert("seq".into(), json!(h.seq));
        Value::Object(e)
    }

    /// A `watchpoint_list` continuation token, resolved back to the watch id it stands for.
    fn parse_watch_cursor(&self, v: &Value) -> Result<WatchId, RpcError> {
        let handle = match v {
            Value::String(s) if !s.is_empty() => s.clone(),
            _ => {
                return Err(RpcError::invalid_params(
                    "`cursor` must be the non-empty opaque string this server issued",
                ))
            }
        };
        self.resolve_issued_handle(&handle, "cursor")
    }

    /// Resolve a handle this server **must have issued**, refusing anything else by name.
    fn resolve_issued_handle(&self, handle: &str, field: &str) -> Result<WatchId, RpcError> {
        resolve_watch_handle(handle)
            .filter(|id| id.0 < self.watches_issued)
            .ok_or_else(|| {
                RpcError::invalid_params(format!(
                    "`{field}`: {handle:?} is not a handle this server issued — pass back one \
                     emulator/watchpoint_add returned"
                ))
            })
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

// -------------------------------------------------------------------- watchpoints (§6)

/// House ceiling on one page of a bounded list — the same 4096 `read_memory` carries. A `limit` bounded on
/// one list and unbounded on its twin is two policies wearing one name.
/// Slots in the sprite attribute table. The table is this size in both modes; how many of them the
/// hardware *parses* is `parsedMax` and is core's answer, not this crate's (§11.10).
const SAT_SLOTS: usize = 80;

const MAX_PAGE: u64 = 4096;
/// `watchpoint_hits`' catalog default page size.
const DEFAULT_HITS_PAGE: usize = 100;
/// Ceiling on one watched range, from the schema (`len` ≤ 16 MiB).
const MAX_WATCH_LEN: u64 = 16_777_216;

/// A watch id **as it goes on the wire**: an opaque string (D9 category 4, §8 item 16).
///
/// The `w` prefix is not decoration. `checkpoint`'s handles are bare decimal strings and §6.1's own
/// commentary concedes that quoting a number "does not *stop* a determined client — the value is still an id
/// in decimal — but it stops the accident". A handle that is not a number at all stops the accident harder,
/// and this surface is where it matters most: the schema types the handle as a string in **five** places,
/// §8 item 16 records this server having shipped a numeric handle once already, and a watch id is precisely
/// the value §6 says cannot be an address or an index — one address may carry several watches, and the same
/// number names four different things across the four spaces.
fn watch_wire_id(id: WatchId) -> String {
    format!("w{}", id.0)
}

/// The inverse of [`watch_wire_id`]. `None` for any string this server could not have spelled.
///
/// Deliberately strict about the spelling it accepts — no bare-number fallback, unlike [`parse_cursor`]'s
/// migration allowance. This handle has never had another spelling, so leniency here would buy nothing and
/// would quietly bless the `{"watch": 3}` that D9 category 4 exists to forbid.
fn resolve_watch_handle(handle: &str) -> Option<WatchId> {
    handle.strip_prefix('w')?.parse::<u32>().ok().map(WatchId)
}

/// A required opaque-string handle param. Strict — a string only, for [`parse_checkpoint_id`]'s reason:
/// this is the handle a human hand-types into the next call, and typing `{"watch": 3}` *is* the arithmetic
/// on a handle that D9 category 4 forbids.
fn parse_watch_handle(params: &Value, field: &str) -> Result<String, RpcError> {
    match params.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(Value::String(_)) => Err(RpcError::invalid_params(format!(
            "`{field}` must be a non-empty string — pass back the handle emulator/watchpoint_add returned"
        ))),
        None | Some(Value::Null) => Err(RpcError::invalid_params(format!(
            "`{field}` (the opaque string handle returned by emulator/watchpoint_add) is required"
        ))),
        Some(other) => Err(RpcError::invalid_params(format!(
            "`{field}` must be a JSON string — the handle is opaque and a client must not compute on it \
             (D9 category 4); got {}",
            hex::kind_of(other)
        ))),
    }
}

fn parse_watch_space(params: &Value) -> Result<WatchSpace, RpcError> {
    match params.get("space") {
        None | Some(Value::Null) => Ok(WatchSpace::Bus),
        Some(Value::String(s)) => match s.as_str() {
            "bus" => Ok(WatchSpace::Bus),
            "vram" => Ok(WatchSpace::Vram),
            "cram" => Ok(WatchSpace::Cram),
            "vsram" => Ok(WatchSpace::Vsram),
            other => Err(RpcError::invalid_params(format!(
                "`space` must be one of \"bus\", \"vram\", \"cram\", \"vsram\"; got {other:?}"
            ))),
        },
        Some(other) => Err(RpcError::invalid_params(format!(
            "`space` must be a string; got {}",
            hex::kind_of(other)
        ))),
    }
}

/// Resolve `read`/`write` into the op filter §6 pins.
///
/// **Neither given means write-only** — the recorded purpose of this instrument is *"who wrote this?"* — and
/// both true means any access. A write watch also matches the 68000 TAS (its read-modify-write store);
/// a read watch does not.
///
/// Both explicitly `false` is **refused**, not honoured. It arms a watch that can never match, whose reading
/// is `seen` positive and `matched` zero — indistinguishable on the wire from a live instrument that found
/// nothing, which is the single failure mode this whole surface exists to make impossible.
fn parse_watch_op(params: &Value) -> Result<WatchOp, RpcError> {
    let flag = |name: &str| -> Result<Option<bool>, RpcError> {
        match params.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(other) => Err(RpcError::invalid_params(format!(
                "`{name}` must be a boolean (D9); got {}",
                hex::kind_of(other)
            ))),
        }
    };
    let (read, write) = (flag("read")?, flag("write")?);
    match (read.unwrap_or(false), write.unwrap_or(false)) {
        (true, true) => Ok(WatchOp::Any),
        (true, false) => Ok(WatchOp::Read),
        (false, true) => Ok(WatchOp::Write),
        // Neither *given* is the documented default; both given as `false` is a request for a watch that
        // matches nothing, and is named rather than silently turned into a write watch.
        (false, false) if read.is_none() && write.is_none() => Ok(WatchOp::Write),
        (false, false) => Err(RpcError::invalid_params(
            "`read: false, write: false` arms a watch that can never match — omit both for the \
             write-only default, or set at least one true",
        )),
    }
}

/// Resolve `mode` and `censusKey` together, because neither is legal without the other's agreement.
///
/// §6 and the schema both enforce this in **both** directions: a `censusKey` without `mode: "census"` is
/// `-32602` and MUST NOT be silently ignored — a param this bus quietly dropped would be a caller believing
/// it asked for a grouping it did not get, which is §5's refuse-and-name ethos applied one level down — and
/// `mode: "census"` without a key has nothing to group by.
fn parse_watch_mode(params: &Value) -> Result<(WatchMode, Option<CensusKey>), RpcError> {
    let key = match params.get("censusKey") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(match s.as_str() {
            "addr" => CensusKey::Addr,
            "value" => CensusKey::Value,
            "via" => CensusKey::Via,
            other => {
                return Err(RpcError::invalid_params(format!(
                    "`censusKey` must be one of \"addr\", \"value\", \"via\"; got {other:?}"
                )))
            }
        }),
        Some(other) => Err(RpcError::invalid_params(format!(
            "`censusKey` must be a string; got {}",
            hex::kind_of(other)
        )))?,
    };
    let census = match params.get("mode") {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => match s.as_str() {
            "record" => false,
            "census" => true,
            other => {
                return Err(RpcError::invalid_params(format!(
                    "`mode` must be \"record\" or \"census\"; got {other:?}"
                )))
            }
        },
        Some(other) => {
            return Err(RpcError::invalid_params(format!(
                "`mode` must be a string; got {}",
                hex::kind_of(other)
            )))
        }
    };
    match (census, key) {
        (true, Some(k)) => Ok((WatchMode::Census(k), Some(k))),
        (true, None) => Err(RpcError::invalid_params(
            "`mode: \"census\"` requires `censusKey` — a census with no key has nothing to group by",
        )),
        (false, Some(_)) => Err(RpcError::invalid_params(
            "`censusKey` is only meaningful with `mode: \"census\"` and is refused without it rather \
             than ignored — a param this bus dropped silently would be a caller believing it asked for \
             a grouping it did not get",
        )),
        (false, None) => Ok((WatchMode::Record, None)),
    }
}

fn space_name(s: WatchSpace) -> &'static str {
    match s {
        WatchSpace::Bus => "bus",
        WatchSpace::Vram => "vram",
        WatchSpace::Cram => "cram",
        WatchSpace::Vsram => "vsram",
    }
}

fn op_name(o: WatchOp) -> &'static str {
    match o {
        WatchOp::Read => "read",
        WatchOp::Write => "write",
        WatchOp::Any => "any",
    }
}

/// A recorded access's own op. `tas` is the 68000 read-modify-write store; it matches a write watch and not
/// a read one, and it is spelled out on the wire rather than folded into `write` — the whole point of a
/// "who modified this?" watch is that the atomic case is visible.
fn bus_op_name(o: oracle_core::bus::BusOp) -> &'static str {
    match o {
        oracle_core::bus::BusOp::Read => "read",
        oracle_core::bus::BusOp::Write => "write",
        oracle_core::bus::BusOp::Tas => "tas",
    }
}

fn via_name(v: WatchVia) -> &'static str {
    match v {
        WatchVia::Bus => "bus",
        WatchVia::Direct => "direct",
        WatchVia::Dma => "dma",
    }
}

/// The wire spelling of a mode. Core has a third, [`WatchMode::Count`], which this bus deliberately does
/// **not** expose: `matched` is required on every hits read and is counted in every mode, so a count-only
/// watch is what `record` already gives a client that reads `matched` and ignores `hits`.
fn mode_name(m: WatchMode) -> &'static str {
    match m {
        WatchMode::Census(_) => "census",
        _ => "record",
    }
}

/// The wire spelling of a census key, or `None` for one this bus does not expose.
///
/// **An `Option` rather than a `_ => "addr"` fallback, and the difference is the whole point.** Core has
/// seven `CensusKey` variants and §6 exposes three; the other four are reachable on the *shared* instrument,
/// because the player's panel holds a `&mut Watchpoints` too and could arm one. Mapping an unexposed key to
/// the nearest exposed spelling would put a **wrong label on a correct number** — a client would read an
/// `AddrPage(8)` census as an `addr` census and conclude the ROM touches 60 addresses when it touches
/// 15,000 pages' worth. Omitting the key says "this census is not one you asked for" and the caller's
/// `caveat` says why.
fn census_key_name(k: CensusKey) -> Option<&'static str> {
    match k {
        CensusKey::Addr => Some("addr"),
        CensusKey::Value => Some("value"),
        CensusKey::Via => Some("via"),
        _ => None,
    }
}

/// One `watchpoint_list` entry, in exactly the schematized keys.
///
/// `census`, `distinctKeys`, `keyCap`, `keysCapped` and `censusOverflow` are emitted **only** in census
/// mode. In `record` mode core reports `distinct_keys: 0` and `keys_capped: false`, which are not answers —
/// they are the absence of a census wearing an answer's clothes, and a client comparing `distinctKeys` to
/// `matched` across a mixed list would read them as findings.
///
/// `keysCapped` and `censusOverflow` are **typed keys, not a caveat**, per §2.4 rule 3: a capped census
/// makes `distinctKeys` a *lower bound*, and that is a consequence a client must act on.
fn watch_report_json(r: &WatchReport) -> Value {
    let mut e = Map::new();
    e.insert("watch".into(), json!(watch_wire_id(r.id)));
    if !r.label.is_empty() {
        e.insert("label".into(), json!(r.label));
    }
    e.insert("space".into(), json!(space_name(r.space)));
    e.insert("addr".into(), json!(hex::addr(*r.range.start())));
    e.insert(
        "len".into(),
        json!(u64::from(*r.range.end() - *r.range.start()) + 1),
    );
    e.insert("op".into(), json!(op_name(r.op)));
    e.insert("mode".into(), json!(mode_name(r.mode)));
    if let WatchMode::Census(k) = r.mode {
        // Absent when the key has no wire spelling — see [`census_key_name`]. `watchpoint_list` adds the
        // `caveat` that explains the hole rather than leaving a reader to guess at a census with no key.
        if let Some(name) = census_key_name(k) {
            e.insert("censusKey".into(), json!(name));
        }
    }
    if let Some(n) = r.stop_after {
        e.insert("stopAfter".into(), json!(n));
    }
    // Counted in EVERY mode, including the ones that store nothing. A count of writes, **not** a measure of
    // how much the value moved — see the census below, and `docs/2026-08-15-watchpoint-bus-surface.md` §2.1.
    e.insert("matched".into(), json!(r.matched));
    if let Some(s) = r.first {
        e.insert("first".into(), watch_stamp_json(&s));
    }
    if let Some(s) = r.last {
        e.insert("last".into(), watch_stamp_json(&s));
    }
    if let Some(census) = &r.census {
        let rows: Vec<Value> = census
            .iter()
            .map(|(k, c)| json!({"key": k, "count": c}))
            .collect();
        e.insert("census".into(), Value::Array(rows));
        e.insert("distinctKeys".into(), json!(r.distinct_keys));
        e.insert("keyCap".into(), json!(r.key_cap));
        e.insert("keysCapped".into(), json!(r.keys_capped));
        e.insert("censusOverflow".into(), json!(r.census_overflow));
    }
    Value::Object(e)
}

/// A `first`/`last` coordinate. **Nested**, never spread at the top level of a result: §2.2's envelope stamp
/// overwrites same-named keys, so a top-level `frame` here would come back as the machine's *now*.
fn watch_stamp_json(s: &Stamp) -> Value {
    json!({
        "pc": hex::addr(s.pc),
        "frame": s.frame,
        "mclk": s.mclk,
        "seq": s.seq,
    })
}

/// One `otherMatches` entry, in the **single** item shape §4 pins: `{name, addr, demangled?}`.
///
/// `name` is the identifying spelling, because this is the value a client hands straight back to resolve
/// the match — the same round-trip promise the top-level `name` carries. `demangled` rides along only when
/// it differs, so a listing with no mangling does not pay for a key that repeats `name` verbatim.
fn match_item(s: &oracle_core::symbols::Symbol) -> Value {
    let mut o = json!({"name": s.name, "addr": hex::addr(s.addr)});
    if s.demangled != s.name {
        o["demangled"] = json!(s.demangled);
    }
    o
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

/// §2.5's closure, as one check: any top-level `params` key the method's fragment does not declare.
///
/// The refusal names the offending key **and** lists the keys the method accepts, so it is also the fix
/// (§5's *"Refuse, name the reason, and name the fix"*), and `error.data.unknownParams` carries the keys
/// as a typed array because a client acting on *which* key was rejected needs a field rather than prose
/// (§2.4 rule 3). Every unknown key is reported, not just the first — a client that guessed two names
/// should not have to round-trip twice to learn both.
///
/// **Why this is worth the cost.** An optional param added in a later amendment stops being invisible to
/// older servers: they refuse it, by name. That is the trade, made on purpose. The case that prompted it
/// is measured, not hypothetical — a client wanting to write `Player_1`+2 reached for `offset:`, then for
/// `disp:`, and was told OK both times while this server wrote `Player_1`+0 and answered with an address
/// the client had never asked for. Guessing a parameter name only resembled a discovery mechanism while
/// it silently succeeded.
///
/// Non-object params are not this function's business: a non-object is either absent (fine — every
/// required key is the handler's own check) or a type error the handler reports in its own words.
fn unknown_params(spec: &MethodSpec, params: &Value) -> Option<RpcError> {
    let obj = params.as_object()?;
    let unknown: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|k| !spec.params.contains(k))
        .collect();
    if unknown.is_empty() {
        return None;
    }
    let accepted = if spec.params.is_empty() {
        "none — this method takes no params".to_string()
    } else {
        spec.params.join(", ")
    };
    Some(
        RpcError::invalid_params(format!(
            "{} does not accept {}; accepted params: {accepted}",
            spec.name,
            unknown
                .iter()
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", "),
        ))
        .with_data(json!({"unknownParams": unknown})),
    )
}

/// Parse a `line` param for either CRAM method. Out of range is `-32602` — **refused, never clipped** —
/// and shared by both so the two rows cannot drift on the bound or on the wording.
fn parse_cram_line(v: &Value) -> Result<u8, RpcError> {
    match v.as_u64() {
        Some(n) if n <= 3 => Ok(n as u8),
        Some(n) => Err(RpcError::invalid_params(format!(
            "`line` {n} is outside 0-3 — refused, never clipped"
        ))),
        // D9 category 2: an index is a JSON number, never a hex string.
        None => Err(RpcError::invalid_params(
            "`line` must be an integer 0-3 (D9 category 2)",
        )),
    }
}

/// One `emulator/read_cram` palette entry: the stored word and its three stored components, plus the
/// `(line, index)` pair a write takes back and the `cramAddr` join key.
fn cram_entry(cram: &[u8], line: u8, index: u8) -> Value {
    let entry = usize::from(line) * 16 + usize::from(index);
    let b = entry * 2;
    // Masked on the way out as well as on the way in: `write_target` and `poke_cram` both store masked
    // words, so this is belt-and-braces — but `raw` is contractually "the stored word masked to the 9-bit
    // colour", and a component derived from an unmasked bit would be outside its declared 0-7.
    let raw = ((u16::from(cram[b]) << 8) | u16::from(cram[b | 1])) & 0x0EEE;
    json!({
        "line": line,
        "index": index,
        "cramAddr": hex::addr(entry as u32 * 2),
        "raw": hex::u16_hex(raw),
        "r": (raw >> 1) & 0x07,
        "g": (raw >> 5) & 0x07,
        "b": (raw >> 9) & 0x07,
    })
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

/// One row of an `emulator/play_input` timeline: the buttons it **contributes** over `[start, end)` on
/// one port. Not a complete pad state — under union no single row is complete.
struct InputRow {
    start: u64,
    end: u64,
    port: usize,
    pad: Pad,
}

/// The pad for `frame` on `port`: the **union** of every row covering it, and `Pad::default()` when none
/// does.
///
/// Union rather than later-row-wins, and the contract rules it normatively: the union is
/// **order-independent**, so the pad depends on the row *set* and rows need not be sorted or disjoint.
/// Later-row-wins would make row order load-bearing — a place two conformant servers would silently
/// disagree — and would cost the two-row "hold right, tap A at 120" script that motivates the shape.
fn pad_at(rows: &[InputRow], port: usize, frame: u64) -> Pad {
    let mut pad = Pad::default();
    for r in rows
        .iter()
        .filter(|r| r.port == port && r.start <= frame && frame < r.end)
    {
        pad.up |= r.pad.up;
        pad.down |= r.pad.down;
        pad.left |= r.pad.left;
        pad.right |= r.pad.right;
        pad.a |= r.pad.a;
        pad.b |= r.pad.b;
        pad.c |= r.pad.c;
        pad.start |= r.pad.start;
    }
    pad
}

/// Parse and bound `rows`. Every refusal here is `-32602` and names what was wrong, because a timeline
/// that silently drops a row is a script that looks like it did something it did not.
fn parse_input_rows(params: &Value, cap: usize) -> Result<Vec<InputRow>, RpcError> {
    let Some(v) = params.get("rows") else {
        return Err(RpcError::invalid_params("`rows` (array) is required"));
    };
    let Some(arr) = v.as_array() else {
        return Err(RpcError::invalid_params("`rows` must be an array"));
    };
    // An empty timeline is a request to do nothing, refused rather than silently satisfied.
    if arr.is_empty() {
        return Err(RpcError::invalid_params(
            "`rows` is empty — a timeline that applies to no frame is refused rather than treated as a \
             no-op, so a script cannot look like it ran when it did not",
        ));
    }
    if arr.len() > cap {
        return Err(RpcError::invalid_params(format!(
            "`rows` has {} entries; this server accepts {cap} (limits.maxInputRows)",
            arr.len()
        )));
    }
    let mut out = Vec::with_capacity(arr.len());
    for (i, row) in arr.iter().enumerate() {
        let at = |what: &str| format!("rows[{i}]: {what}");
        let Some(obj) = row.as_object() else {
            return Err(RpcError::invalid_params(at("must be an object")));
        };
        let num = |key: &str| -> Result<u64, RpcError> {
            match obj.get(key) {
                Some(Value::Number(n)) if n.is_u64() => Ok(n.as_u64().unwrap()),
                Some(_) => Err(RpcError::invalid_params(at(&format!(
                    "`{key}` must be a non-negative whole number (D9 category 2)"
                )))),
                None => Err(RpcError::invalid_params(at(&format!(
                    "`{key}` is required"
                )))),
            }
        };
        let (start, end) = (num("start")?, num("end")?);
        if end <= start {
            return Err(RpcError::invalid_params(at(&format!(
                "`end` ({end}) must be greater than `start` ({start}) — the interval is half-open \
                 [start, end), so an empty one is a row that says nothing"
            ))));
        }
        let port = parse_port(row)?;
        let names = parse_buttons(row)?;
        let mut pad = Pad::default();
        for b in &names {
            set_button(&mut pad, b, true);
        }
        out.push(InputRow {
            start,
            end,
            port,
            pad,
        });
    }
    Ok(out)
}

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

/// The `caveat` `emulator/get_profiler_frames` carries when the accountant lost the thread of the
/// program's stack, or `None` when it did not.
///
/// §2.4's advisory, applied: a caveat present on every reply is one clients learn to ignore, so this
/// answers `None` on the ordinary sample. **And it names only the counter that is actually non-zero** —
/// "lost 0 frame(s) and declined 3 call(s)" sends a reader looking for the zero half, which is a sentence
/// about the message format rather than about their sample.
///
/// A free function with its own tests because neither non-zero case is reachable from the fixture ROMs:
/// both counters are recovery events (a return the shadow stack could not match, a call past its depth
/// bound), the fixtures are well-behaved by construction, and probing all eight `ProfilerShape`s — plus
/// recursion 30,000 deep — produced zero of each. The *counting* is pinned in `oracle_core::profiler`;
/// what is pinned here is the sentence a client reads.
fn profiler_caveat(abandoned_frames: u64, depth_exceeded: u64) -> Option<String> {
    let mut said: Vec<String> = Vec::new();
    if abandoned_frames > 0 {
        said.push(format!("the shadow stack lost {abandoned_frames} frame(s)"));
    }
    if depth_exceeded > 0 {
        said.push(format!(
            "the shadow stack declined {depth_exceeded} call(s) at its depth bound"
        ));
    }
    (!said.is_empty()).then(|| {
        format!(
            "{}; those cycles are reported but the affected rows' `calls` understate their invocations",
            said.join(" and ")
        )
    })
}

/// How far past a symbol an address may sit before the name stops being useful — the same bound the
/// player's own lenses use (`oracle-frontend`'s `MAX_SYMBOL_DISPLACEMENT`), and for the same reason.
///
/// Aeon's listings are dense, so an address more than 4 KiB past the nearest label is almost certainly in
/// data or off the end of the image, where naming the previous routine is actively misleading rather than
/// merely imprecise. Past this the answer is no name; the raw address is always there either way.
///
/// The filter itself is `SymbolTable::resolve_within`, whose refusal is pinned by
/// `oracle_core::symbols`'s `resolve_within_rejects_an_implausibly_distant_answer`.
const MAX_SYMBOL_DISPLACEMENT: u32 = 0x1000;

/// The order `emulator/get_profiler_frames` puts routine rows in: **most expensive first, and the tie-break
/// is the undivided figure before the address.**
///
/// Each row is `(entry address, divided counts, undivided counts)`.
///
/// Extracted from the handler so the tie-break has a witness. The primary key is exercised by every wire
/// test that reads a sample, but a *tie* on the divided figure is not reachable from the fixture ROMs —
/// every routine in them runs a fixed number of times per frame, so its divided cycles scale with the
/// sample instead of flooring together — and an ordering rule whose interesting case no test can reach is
/// an ordering rule nothing checks.
///
/// Why the second key exists at all: `cycles` is floored, so on a long sample many genuinely different rows
/// share one value, and with the address as the only tie-break `top` would keep the **lowest-addressed** of
/// them rather than the most expensive — the exact confusion the ordering exists to prevent. The undivided
/// partner separates them without truncation, which makes this a strict refinement: it can only reorder
/// rows the address-only comparator called equal. The address stays last so the order is still **total**,
/// and two identical boots cannot disagree.
fn profiler_row_order(a: &(u32, Counts, Counts), b: &(u32, Counts, Counts)) -> std::cmp::Ordering {
    b.1.cycles
        .cmp(&a.1.cycles)
        .then(b.2.cycles.cmp(&a.2.cycles))
        .then(a.0.cmp(&b.0))
}

/// The same ordering one level down, for a row's `callers[]` (§11.18): `cycles` descending, so a bounded
/// edge list is the expensive end rather than an arbitrary slice.
///
/// The undivided partner is the tie-break for the reason it is on the rows — the divided figure is floored,
/// so on a long sample genuinely different edges share one `cycles` value, and with the caller key as the
/// only tie-break `topCallers` would keep the *lowest-keyed* of them rather than the most expensive. The
/// key stays last so the order is total and two identical boots cannot disagree.
fn profiler_edge_order(
    a: &(CallerKey, EdgeCounts, EdgeCounts),
    b: &(CallerKey, EdgeCounts, EdgeCounts),
) -> std::cmp::Ordering {
    b.1.cycles
        .cmp(&a.1.cycles)
        .then(b.2.cycles.cmp(&a.2.cycles))
        .then(a.0.cmp(&b.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(addr: u32, divided: u64, total: u64) -> (u32, Counts, Counts) {
        (
            addr,
            Counts {
                cycles: divided,
                ..Counts::default()
            },
            Counts {
                cycles: total,
                ..Counts::default()
            },
        )
    }

    /// **A floored tie is broken by the undivided figure, and only then by the address.**
    ///
    /// The middle two rows are what the ruling is about: both report `cycles: 7` because integer division
    /// floored them together, and they are *not* equally expensive — one really cost 799 cycles and the
    /// other 700. Ordering them by address would put a cheaper row above a dearer one and, under `top`,
    /// keep the wrong one.
    #[test]
    fn the_row_order_breaks_a_floored_tie_by_the_undivided_figure() {
        let mut rows = [
            row(0x0000_1000, 7, 700),
            row(0x0000_0400, 9, 900),
            row(0x0000_0800, 7, 799),
            row(0x0000_0100, 7, 700), // a genuine tie with the first row, on both figures
        ];
        rows.sort_by(profiler_row_order);
        assert_eq!(
            rows.iter().map(|r| r.0).collect::<Vec<_>>(),
            vec![0x0000_0400, 0x0000_0800, 0x0000_0100, 0x0000_1000],
            "expensive first; the 7/799 row outranks both 7/700 rows; the two identical rows fall back \
             to ascending address, which is what keeps the order total"
        );
    }

    /// **Both directions of the caveat's MUST-NOT, and the wording nit that came with them.** A clean
    /// sample says nothing; a dirty one says exactly what happened and mentions **only** the counter that
    /// fired.
    #[test]
    fn the_caveat_appears_only_when_there_is_something_to_say_and_names_only_that() {
        assert_eq!(
            profiler_caveat(0, 0),
            None,
            "the ordinary sample carries no caveat at all"
        );

        let lost = profiler_caveat(2, 0).expect("a lost frame is worth saying");
        assert!(lost.contains("lost 2 frame(s)"), "{lost}");
        assert!(
            !lost.contains("depth bound"),
            "the counter that did not fire is not mentioned: {lost}"
        );

        let declined = profiler_caveat(0, 3).expect("a declined call is worth saying");
        assert!(declined.contains("declined 3 call(s)"), "{declined}");
        assert!(
            !declined.contains("lost"),
            "…and not in this direction either: {declined}"
        );

        let both = profiler_caveat(2, 3).expect("both");
        assert!(
            both.contains("lost 2 frame(s)") && both.contains("declined 3 call(s)"),
            "{both}"
        );
        // Whatever the combination, the consequence a reader needs is always there.
        for c in [&lost, &declined, &both] {
            assert!(
                c.contains("understate their invocations"),
                "the caveat says what it means for the numbers: {c}"
            );
        }
    }

    /// The refinement is **strict**: it never reorders rows the primary key already separates, whichever
    /// way the undivided figures happen to sit. A total that disagrees with its divided partner cannot
    /// promote a row past a genuinely more expensive one.
    #[test]
    fn the_undivided_tie_break_never_overrides_the_divided_order() {
        let mut rows = [row(0x0000_0200, 5, 5_000_000), row(0x0000_0300, 6, 6)];
        rows.sort_by(profiler_row_order);
        assert_eq!(
            rows[0].0, 0x0000_0300,
            "`cycles` decides first, always: the second key is a tie-break and not a second opinion"
        );
    }
}
