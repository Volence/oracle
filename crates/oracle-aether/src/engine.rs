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

use crate::breakpoints::{BreakStop, BreakpointId, Breakpoints};
use crate::build_info;
use crate::decoders;
use crate::hex;
use crate::objreq;
use crate::outbound::Subscribers;
use crate::rpc::{self, code, RpcError};
use oracle_core::bus::{
    BusEvent, BusEventSink, Fanout, Observe, StepRetire, StopWhen, Z80_RAM_SIZE,
};
use oracle_core::io::Pad;
// The 68000's own bus trait, brought in for `emulator/write_memory`: a poke travels the same `write8`
// the CPU drives, so the hardware mirror masking and the region decode are the machine's, not ours.
use oracle_core::m68000::bus68k::Bus68k;
// The control-flow classifier and the return-frame sizes, for the `step*` shadow stack. Both are pure
// functions of the opcode word, and both are the profiler's — a second copy of either is a mirror that can
// drift while both halves look right.
use oracle_core::m68000::decode::{control_flow_of, return_pop_bytes, ControlFlow};
use oracle_core::profiler::{CallerKey, Counts, EdgeCounts, Profiler};
use oracle_core::render::{CandidateVerdict, Layer, LayerMask, PixelState};
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
    /// **The breakpoint cap** (`protocol.md` §6, §11.21 design choice 3), advertised in `initialize` as
    /// `limits.maxBreakpoints` — on `limits` and **not** inside `capabilities.breakpoints`, because that
    /// capability is a **boolean** shipping clients already parse and §11.18 forbids widening an emitted
    /// shape under them. At the cap `emulator/breakpoint_add` refuses with
    /// `-32005 {reason:"breakpointCapReached", cap, count}` and *"MUST NOT silently grow past the
    /// advertised number"*.
    ///
    /// 32, the watch cap's number, for the watch cap's reason: a breakpoint is a small struct, so this is a
    /// policy bound rather than a memory one — and one instrument-shaped cap is easier for a client to hold
    /// than two arbitrary ones. §6 pins no number.
    pub max_breakpoints: usize,
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
            max_breakpoints: 32,
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
    // The five §11.21 breakpoint rows, in catalog order. **None is subject to §6's run-control state
    // rule** — arming, toggling and clearing mutate an observer, not the timeline — and the params sets
    // here are the schema fragments', derived by `tests/params_closure.rs` rather than transcribed.
    MethodSpec {
        name: "emulator/breakpoint_add",
        handler: Engine::breakpoint_add,
        summary: "arm an execution breakpoint at an address or symbol; returns its handle",
        params: &["addr", "enabled", "label", "symbol"],
    },
    MethodSpec {
        name: "emulator/breakpoint_set_enabled",
        handler: Engine::breakpoint_set_enabled,
        summary: "the one writer of a breakpoint's `enabled`; carries `hits` across the toggle",
        params: &["breakpoint", "enabled"],
    },
    MethodSpec {
        name: "emulator/breakpoint_list",
        handler: Engine::breakpoint_list,
        summary: "one page of the breakpoints held, each with its address, arm state and hit count",
        params: &["cursor", "limit"],
    },
    MethodSpec {
        name: "emulator/breakpoint_clear",
        handler: Engine::breakpoint_clear,
        summary: "clear one breakpoint by handle, or every breakpoint on the server",
        params: &["all", "breakpoint"],
    },
    MethodSpec {
        name: "emulator/wait_for_break",
        handler: Engine::wait_for_break,
        summary: "poll where the machine halted (deprecated by the `stopped` event; see §6 D6)",
        params: &["timeoutMs"],
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
    // Served 2026-08-27 against contract revision `091ac59`. The fragment is FIRST-FRAGMENT-transcribed
    // and carries three registered absences (audit D-16) served exactly as written — see the handler's
    // doc comment, and `docs/2026-08-27-write-vram.md` for the CR text sent upstream.
    MethodSpec {
        name: "emulator/write_vram",
        handler: Engine::write_vram,
        summary: "poke bytes into VDP VRAM (byte payload only; refused whole before any byte lands)",
        params: &["addr", "bytes"],
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
    // The layer-mask pair (§6's VRAM/CRAM/layers group, lines 1136 and 1192), served 2026-08-26. §11.22
    // pins the setter's enum to be the getter's key set, stated once in §6 so the two cannot drift; here
    // both are built from the same `mask_targets()` derivation for the same reason. Neither is subject to
    // §6's run-control state rule: the getter is a pure read, and the setter changes the DISPLAY and not
    // the machine.
    MethodSpec {
        name: "emulator/get_layer_states",
        handler: Engine::get_layer_states,
        summary: "which display layers are drawn: planeA, planeB, window, sprites",
        params: &[],
    },
    MethodSpec {
        name: "emulator/set_layer_enabled",
        handler: Engine::set_layer_enabled,
        summary: "show or hide one display layer (a compositing mask; the machine is untouched)",
        params: &["enabled", "layer"],
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
    // §6's `object / player decoders ⚙` group, schematized 2026-08-26 by §11.25 (CR-D). All three are
    // pure reads — no `require_paused`, exactly as `read`/`sprites`/`pixel_attribution`/`scanlines`, where
    // the envelope's `running` is the contract's whole answer to a torn sample. Every one of them carries
    // `layout`, and every one of them refuses `-32012` rather than decoding from a guessed base.
    MethodSpec {
        name: "emulator/object_list",
        handler: Engine::object_list,
        summary: "the live object pool: the active slots, decoded against a layout the reply names",
        params: &["fields", "includeBytes", "limit"],
    },
    MethodSpec {
        name: "emulator/player_state",
        handler: Engine::player_state,
        summary: "the player pool slot by slot, inactive slots included and said so",
        params: &["fields", "includeBytes"],
    },
    MethodSpec {
        name: "emulator/object_slot",
        handler: Engine::object_slot,
        summary: "one addressed object slot, decoded — or `active: false` when nothing lives there",
        params: &["fields", "includeBytes", "slot"],
    },
    MethodSpec {
        name: "emulator/z80_read",
        handler: Engine::z80_read,
        summary: "bytes from the Z80's own 0x0000-0x3FFF window, mirror included, bounded at both ends",
        params: &["addr", "len"],
    },
    MethodSpec {
        name: "emulator/z80_write",
        handler: Engine::z80_write,
        summary: "one byte, or a low-address-first `bytes` payload, into the Z80's window — paused only",
        params: &["addr", "bytes", "value"],
    },
    MethodSpec {
        name: "emulator/object_at",
        handler: Engine::object_at,
        summary: "what is showing at one screen dot: the layer, the act-world point, and the object slot \
                  that drew it — with each half naming its own unavailability",
        params: &["x", "y"],
    },
    // §6's three object **MUTATION** rows, adopted 2026-09-03 by §11.32 (CR-J). Three rows and not one
    // `object_request { op }`, because servedness on this bus is `methods` membership (§8 item 23) and
    // one row would make *can spawn* and *can delete* the same bit. All three are named in §6's
    // run-control state rule — they are writes and they advance the machine — so all three
    // `require_paused` and refuse `-32005 machineRunning` rather than pausing implicitly.
    MethodSpec {
        name: "emulator/object_spawn",
        handler: Engine::object_spawn,
        summary: "place one archetype in the live object pool, through the game's mailbox; returns its handle",
        params: &[
            "def",
            "defSymbol",
            "expectFrameToken",
            "flipH",
            "flipV",
            "maxFrames",
            "subtype",
            "x",
            "y",
        ],
    },
    MethodSpec {
        name: "emulator/object_move",
        handler: Engine::object_move,
        summary: "move one live dynamic object — POSITION ONLY, no clamp, velocity and animation untouched",
        params: &["expectFrameToken", "handle", "maxFrames", "slot", "x", "y"],
    },
    MethodSpec {
        name: "emulator/object_delete",
        handler: Engine::object_delete,
        summary: "delete one live dynamic object and its child chain; entity-window slots are refused",
        params: &["expectFrameToken", "handle", "maxFrames", "slot"],
    },
    MethodSpec {
        name: "emulator/screen_text",
        handler: Engine::screen_text,
        summary: "the text on the player's window — source and rendered both, per surface; refuses when \
                  there is no window",
        params: &[],
    },
];

/// The cap on how many surfaces one `emulator/screen_text` reply carries.
///
/// **A policy bound, and it is honest about being one.** The player's own surfaces are bounded already —
/// one title bar, one status line, at most `MAX_TOASTS` toasts — but the palette can list one row per file
/// in a directory, so the list is not bounded by the *design*. The reply therefore carries
/// `total`/`returned`/`truncated` (§2.4's flat spelling) and this cap makes `truncated` mean something
/// instead of being decorative. It is deliberately not a `cursor`: §2.4 clause (b) forbids a continuation
/// token on a method that accepts no continuation param, and this one accepts no params at all.
const MAX_SCREEN_SURFACES: usize = 64;

/// The events this server actually emits. Advertised verbatim as `capabilities.events`, which
/// `protocol.md` §2.1 calls *"the authoritative event set"* — so it lists what we push, not what the
/// spec's example happens to show.
pub const EVENTS: &[&str] = &[
    "emulator/stopped",
    "emulator/resumed",
    "emulator/romReloaded",
];

/// **The three values of `stopPrecision` (`protocol.md` §2.1, §11.31), strongest first.**
///
/// A type rather than three string literals for the reason §2.1 gives for the map itself: the ordering is
/// normative, because the binding rule ("at least as strong as") needs it, and an ordering cannot live in
/// a `&str`. Deriving `PartialOrd` from the declaration order below IS that order — `Exact < AfterCommit`
/// as a Rust comparison means `Exact` is *stronger*, which is why every use spells the comparison out
/// rather than leaving a reader to guess the polarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StopPrecision {
    /// The machine is halted at an instruction boundary, the instruction at `pc` has **not** executed,
    /// and where the stop had a triggering address `pc` **is** that address.
    Exact,
    /// Halted at an instruction boundary with the instruction at `pc` unexecuted, but caused by the
    /// instruction immediately before `pc`, which has fully committed. Read state as post-trigger.
    AfterCommit,
    /// `pc` is near the triggering address and the server promises **nothing** about which side of it.
    Approximate,
}

impl StopPrecision {
    /// The wire spelling. `serde` is deliberately not involved: §2.1 pins these three strings and a
    /// rename attribute is a place for them to be pinned twice.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::AfterCommit => "afterCommit",
            Self::Approximate => "approximate",
        }
    }

    /// The inverse of [`wire`](StopPrecision::wire) — so a conformance test reading a value back off the
    /// wire parses it through the *same* three strings this server writes, rather than through a fourth
    /// copy of the enum written in the test.
    pub fn from_wire(s: &str) -> Option<Self> {
        [Self::Exact, Self::AfterCommit, Self::Approximate]
            .into_iter()
            .find(|p| p.wire() == s)
    }

    /// `true` when `self` promises at least as much as `other` — §2.1's *"at least as strong as"*, which
    /// the binding rule is written in terms of. Spelled as a method because the polarity of the derived
    /// `Ord` (stronger compares *less*) is exactly the kind of thing a reader gets backwards.
    pub fn at_least_as_strong_as(self, other: Self) -> bool {
        self <= other
    }
}

/// **The single declaration of every stop `reason` this server can emit and the precision it is declared
/// at** — §2.1 rule 1's *"derived from the same registry that produces `methods` and `capabilities`"*,
/// spelled as one macro invocation so the four things that must agree cannot be edited apart.
///
/// The macro generates, from one row per reason: the [`StopReason`] variant, [`StopReason::ALL`], the
/// wire spelling, and the declared [`StopPrecision`]. There is no second list to keep in step, and
/// [`Engine::emit_stopped`] takes a `StopReason` rather than a `&str`, so **a reason that is not in this
/// table cannot reach the wire at all** — which is rule 1's "no fewer" half enforced by the compiler
/// rather than by a test. The "no more" half is not a type's job and is checked at runtime by
/// `tests/stop_precision.rs`, which drives the surface until it has observed every reason and compares
/// the set it collected against this table.
macro_rules! stop_reasons {
    ($( $(#[$m:meta])* $variant:ident => $wire:literal, $prec:expr );+ $(;)?) => {
        /// One `emulator/stopped` `reason` (§3's closed enum), restricted to the members this server has
        /// an emitting path for. Generated by [`stop_reasons!`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum StopReason { $( $(#[$m])* $variant ),+ }

        impl StopReason {
            /// Every reason this server can emit — the key set of the `initialize` handshake map.
            pub const ALL: &'static [StopReason] = &[ $( StopReason::$variant ),+ ];

            /// §3's spelling. camelCase for the multi-word ones, per §3's event-name rule.
            pub const fn wire(self) -> &'static str {
                match self { $( StopReason::$variant => $wire ),+ }
            }

            /// **The declared precision, and the only place it is written.** The handshake map and every
            /// `emulator/stopped` this server emits both read it here, so §2.1's binding rule — the
            /// event's value MUST be at least as strong as the declaration's — holds by construction
            /// rather than by inspection.
            pub const fn precision(self) -> StopPrecision {
                match self { $( StopReason::$variant => $prec ),+ }
            }
        }
    };
}

stop_reasons! {
    /// PC-armed. **Measured `exact`** (`tests/stop_precision.rs::measured_breakpoint_precision`): armed at
    /// an `addq.w #1, D0` whose only effect is `D0.w: 0 -> 1`, the halt reports the armed address with
    /// `D0.w` still `0`, over 8 trials. §11.31 requires this entry by name now that
    /// `capabilities.breakpoints` is `true`.
    Breakpoint => "breakpoint", StopPrecision::Exact;
    /// Access-armed (`stopAfter`). **Measured `afterCommit`**
    /// (`measured_watchpoint_precision`): armed on a `move.w D1, (abs).L`, the halt is at the FOLLOWING
    /// boundary with the write already in memory. §6's *"with the triggering instruction fully
    /// committed"*, confirmed rather than quoted.
    Watchpoint => "watchpoint", StopPrecision::AfterCommit;
    /// `step` / `step_over` / `step_out` — one condition, one reason (§3). **Measured `exact`**
    /// (`measured_step_precision`): a step onto the probe leaves the stepped instruction committed and
    /// the instruction at the reported `pc` unexecuted.
    Step => "step", StopPrecision::Exact;
    /// **Measured `exact`** (`measured_run_to_precision`, 8 trials), on both the event's `pc` and the
    /// reply's — which is the one §2.1 rule 4's readers act on.
    RunTo => "runTo", StopPrecision::Exact;
    /// No triggering address: §11.31 rules that this *"stops at the line's first instruction boundary,
    /// and that is `exact` because the definition binds `pc`, not the line"*. The half that is not
    /// definitional — that the reported `pc` is a real boundary whose implied machine state is the one
    /// the machine is in — is measured by `measured_addressless_stop_precision`.
    RunToScanline => "runToScanline", StopPrecision::Exact;
    /// A bounded frame advance completing — `run_frames` or `press`. §3: no triggering address, so
    /// *"`exact` by definition"*; boundary-checked with the two above.
    RunFrames => "runFrames", StopPrecision::Exact;
    /// A manual halt. §3: no triggering address, `exact` by definition; boundary-checked.
    Pause => "pause", StopPrecision::Exact;
}

// **`entry` is missing on purpose, and its absence is the rule working.** §3's enum has eight members;
// this server has an emitting path for seven. §2.1 rule 1 is *"no more, no fewer"* in **both**
// directions — *"a server that serves no breakpoints has no `breakpoint` entry, exactly as it has no
// `breakpoint_add`"* — and §11.31 kept key-set equality over the CR's own doubt precisely because *"an
// over-declared entry is an advertisement for a stop the server cannot produce, the class D4
// abolished"*. Declaring `entry: "exact"` would cost one line and would be that advertisement. The
// reading that this server never emits it is not taken from a grep over string literals: since
// `emit_stopped` takes a `StopReason`, there is no `entry` value to pass, and
// `tests/stop_precision.rs::the_reasons_this_server_emits_are_the_seven_it_names` drives the surface and
// checks the set it actually observed.

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
    /// **The breakpoint instrument (§6, §11.21 — CR-BP), owned here and attached to every run this engine
    /// drives.**
    ///
    /// Engine-owned for [`watchpoints`](Engine::watchpoints)'s reason, and outside `System` for the same
    /// one: a breakpoint is an *observer*, not machine state, so it survives `swap_system`, `restore`,
    /// `reset` and `reload_rom` untouched — which is what makes the `aeon` `evict_witness` idiom (arm a
    /// breakpoint, *then* `reload_rom`, then wait) work at all. §6 rules the surface *"not subject to the
    /// run-control state rule"*: arming, toggling and clearing are legal while the machine runs.
    ///
    /// **It rides every run either driver drives**, which is what makes the surface mean the same thing in
    /// both arrangements: the standalone free-run ([`free_run_step`](Engine::free_run_step)), every bounded
    /// advance ([`advance_with`](Engine::advance_with)), and — since `docs/2026-08-27-bp-hosted-halt.md` —
    /// the hosted player's own 60 Hz loop, which takes the sink from [`run_sinks`](Engine::run_sinks) and
    /// hands the observation back through [`Host::record_break`](crate::host::Host::record_break).
    ///
    /// That last one was a registered gap for exactly one day, and the shape of it is worth keeping: the
    /// player's loop carried no sink, *and* every bounded run that does carry one is refused `-32005
    /// machineRunning` while the player plays ([`require_paused`](Engine::require_paused)). The two halves
    /// composed to make the gap **total** in the arrangement the owner actually uses — the documented
    /// `resume` → `wait_for_break` idiom was exactly and only the broken path, and it failed by *timing
    /// out*, which is indistinguishable from "the ROM never reached that address".
    breakpoints: Breakpoints,
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
    /// **The display layer mask** (`emulator/get_layer_states` / `emulator/set_layer_enabled`).
    ///
    /// It lives *here*, on the engine, for the same reason [`watchpoints`](Engine::watchpoints) does, and
    /// the placement is the whole design rather than a filing decision. Three properties fall out of it
    /// and could not be had any other way:
    ///
    /// * **It is not machine state.** No `System` and no `Vdp` holds a mask, so it is in no bincode
    ///   snapshot and no `state_hash` input. `emulator/state_hash` and `emulator/memory_hash` cannot see
    ///   it even in principle.
    /// * **`reset` / `reload_rom` / `restore` cannot lose it.** All three replace `self.sys` and touch
    ///   nothing here, so a debugging session keeps its masks across a timeline jump — the thing a client
    ///   would have to notice going missing, silently, mid-investigation.
    /// * **It cannot perturb emulation.** The only render that writes to the chip
    ///   (`Vdp::render_scanline`, which commits the sprite-overflow / collision latches the ROM polls)
    ///   takes no mask argument at all, and this field reaches the VDP only as a parameter to the pure
    ///   `&self` renders. `System::run` is byte-for-byte unchanged.
    ///
    /// The cost is paid in [`framebuffer`](Engine::framebuffer): the latched raster frame was drawn
    /// unmasked, so a masked read cannot use it and re-renders from current VDP state instead. That is
    /// declared on the wire (`source: "stateRender"` plus a caveat naming the mask) rather than silently
    /// handing back an unmasked picture in answer to a masked question.
    layers: LayerMask,
    /// **The text a human can read on the player's window** (`emulator/screen_text`, §11.29 / CR-H).
    ///
    /// A snapshot of strings the frontend *already composed for drawing*, pushed once per present through
    /// [`Host::set_screen_text`](crate::host::Host::set_screen_text) — the same seam shape as
    /// [`set_live_pads`](Engine::set_live_pads). Never composed here: a handler that asked the frontend to
    /// *build* the text would run UI composition at an arbitrary point in the frame, which is the one
    /// version of this feature that could perturb anything. CR-H §7 refuses that design by name.
    ///
    /// It sits **here, on the engine**, for exactly the reasons [`layers`](Engine::layers) does, and the
    /// three properties are the whole reason the placement is not a filing decision:
    ///
    /// * **It is not machine state.** No `System` and no `Vdp` holds it, so it is in no bincode snapshot
    ///   and no `state_hash`/`memory_hash` input — those cannot see it even in principle.
    /// * **`reset` / `reload_rom` / `restore` cannot touch it.** All three replace `self.sys`; the glass
    ///   still says what it says.
    /// * **It cannot perturb emulation.** Nothing here reaches a render. The one render that writes to the
    ///   chip (`Vdp::render_scanline`, which commits the sprite-overflow / collision latches) is not on any
    ///   path from this field.
    ///
    /// **`None` means there is no window**, and that is load-bearing rather than incidental: a windowed
    /// player showing *no* text is the ordinary default launch state and pushes `Some(vec![])`. An empty
    /// list and an absent display must therefore stay distinguishable, which is why the handler REFUSES
    /// (`-32005`, `reason: "noDisplay"`) instead of serving an empty list. A headless `oracle-aether` never
    /// leaves `None`.
    screen_text: Option<Vec<ScreenSurface>>,
}

/// Which text surface of the player one [`ScreenSurface`] came from — the contract's closed `kind` enum
/// (`emulator/screen_text`, §11.29).
///
/// Closed rather than free-form so that adding a surface is a contract edit rather than a silent drift.
/// `TitleBar` is drawn by the **window manager**, not by the overlay, which is why the method is named
/// `screen_text` and not `overlay_*`: an enumeration of the overlay's own draw calls misses it entirely,
/// and so does any OCR of the presented framebuffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenSurfaceKind {
    StatusLine,
    Toast,
    Palette,
    Lens,
    TitleBar,
}

impl ScreenSurfaceKind {
    /// The wire spelling. The `enum` in the schema fragment is the authority; this is the only place the
    /// strings are written, so a handler cannot invent a sixth.
    pub fn wire(self) -> &'static str {
        match self {
            Self::StatusLine => "statusLine",
            Self::Toast => "toast",
            Self::Palette => "palette",
            Self::Lens => "lens",
            Self::TitleBar => "titleBar",
        }
    }
}

/// One text surface as the player composed it, and as it actually reached the glass.
///
/// **Both strings, deliberately.** `rendered`-only reports the message's shadow — a caller asking *"did the
/// player say why the ROM failed to open"* sees `…/LOCKED (PE` and cannot tell that `Permission denied` was
/// lost. `text`-only is structurally blind to the entire truncation defect class: it would report text that
/// is *not on screen* as though it were, which makes the readout useless for the one question it exists to
/// answer. Serving both costs near nothing, because the player's `fit` returns a borrowed prefix of a string
/// it composed anyway.
///
/// `truncated` is **not** carried here: the handler derives it from the two strings at serialisation time,
/// so a producer cannot set a flag that disagrees with the pair it sits beside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenSurface {
    pub kind: ScreenSurfaceKind,
    /// The SOURCE string the player composed.
    pub text: String,
    /// What is actually on the glass, after the player's own fit/truncation. A prefix of [`text`] today.
    pub rendered: String,
    /// Characters in [`text`](ScreenSurface::text) for which the player has no glyph — it draws a hollow
    /// box where they should be. Neither `text` nor `rendered` can express that, which is why the field
    /// exists: it turns a defect class with no observer into one a test can assert on. Empty when none,
    /// and REQUIRED so that "absent" and "none" are not the same artifact.
    pub unrenderable: Vec<String>,
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
    /// The breakpoint that ended the run, when that is what ended it — *"the earliest-added enabled
    /// breakpoint at that address"* (§6, §11.21). A third thing that can end one run, on the same argument
    /// [`stopped_by`](Advanced::stopped_by) was added for: a caller reading `StopRecord::fired` alone would
    /// report its own target as reached because an unrelated breakpoint halted the run.
    broke_at: Option<BreakpointId>,
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
            breakpoints: Breakpoints::new(),
            watches_issued: 0,
            profiler: Profiler::new(),
            profiler_armed: false,
            profiler_basis: None,
            screen: ScanlineCapture::new(Retain::LastFrame),
            last_frame: None,
            screen_generation: 0,
            rom_generation: 0,
            // Every layer drawn. `LayerMask::ALL` is the state in which every render path is byte-identical
            // to the code that ran before the mask existed, so a server nobody has masked anything on
            // behaves exactly as it did.
            layers: LayerMask::ALL,
            // `None` until a frontend pushes one, and it stays `None` for the whole life of a headless
            // server — which is what makes `emulator/screen_text`'s refusal the truth rather than a
            // placeholder.
            screen_text: None,
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

    /// Publish the text the player's present just put on the glass (`emulator/screen_text`).
    ///
    /// Pure state, exactly like [`set_live_pads`](Engine::set_live_pads): it never touches the machine, so
    /// it is safe to call outside a drain window, and it is invisible to every frozen currency
    /// ([`screen_text`](Engine::screen_text) explains why the field lives here rather than in `System`).
    ///
    /// Calling this at all is what tells the engine a window exists. **An empty `Vec` is a legitimate,
    /// meaningful push** — a player with F3 off, no toasts and nothing else on draws no characters at all,
    /// and that is the default launch — so it must never be elided into "do not push".
    pub fn set_screen_text(&mut self, surfaces: Vec<ScreenSurface>) {
        self.screen_text = Some(surfaces);
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
            self.emit_stopped(StopReason::Pause, self.sys.cpu_regs().pc, Map::new());
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

    /// **The listing this engine resolves against right now** — the one `emulator/lookup_symbol` answers
    /// from, not a copy of what somebody handed in at startup.
    ///
    /// It exists because the two can differ, and the difference is silent. `emulator/reload_rom` re-runs the
    /// binding check and **drops** the table when it no longer describes the image; an embedder that kept its
    /// own clone from [`Engine::set_symbols`] would go on resolving names against a listing this engine has
    /// already discarded — a panel and a served method giving two answers about one machine. A hosted caller
    /// re-derives from here when [`crate::host::PumpReport::rom_changed`] says the cartridge moved.
    pub fn symbols(&self) -> Option<&SymbolTable> {
        self.symbols.as_deref()
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
    ///
    /// **Absolutised here, at load** — §6's `romPath` is *"the absolute path of the loaded image"*, and
    /// until 2026-08-26 this echoed the launch argument verbatim, so `oracle-aether ./s4.bin` put `./s4.bin`
    /// on the wire (§12.2 item 9 of the CR-C ruling). A relative path is not a weaker answer, it is an
    /// answer that means something different to every reader: a client, a second client, and a log read
    /// tomorrow all resolve it against a working directory that is not this process's, and the two ways it
    /// is used — naming the image to a human and feeding `emulator/reload_rom`'s default — both break the
    /// moment anyone but this process resolves it.
    ///
    /// Done at the boundary rather than in `status` so every route agrees: the binary, the hosted
    /// embedder, `reload_rom` and a checkpoint restore all arrive here or at an already-absolutised value.
    pub fn set_rom_path(&mut self, path: Option<String>) {
        self.rom_path = path.map(|p| absolutise(&p));
    }

    /// Install an already-parsed symbol table (the binary does this at startup from the `.lst` beside
    /// the ROM). The binding check is the caller's — see [`Engine::load_symbols`] for the wire path.
    ///
    /// **Absolutised here, at load, by the same [`absolutise`] as [`Engine::set_rom_path`]** — §6's
    /// paths note as ridden by §11.30 (CR-I): "absolute paths SHOULD be reported" is a property of
    /// *every* success-reply field whose value is a filesystem path, not of the one key a ruling
    /// happened to name. Until 2026-08-30 this echoed the launch argument verbatim, so
    /// `oracle-aether s4.bin --symbols fixtures/aeon/s4.lst` put an absolute `romPath` and a relative
    /// `symbolsPath` on the wire *from one command line* — one of the two answering "which listing is
    /// this server using?" only for a reader who already shared this process's working directory.
    ///
    /// Same helper and the same SHOULD as the ROM path, deliberately not a stricter rule: a second rule
    /// is a second thing to get wrong, and the pass-through case ([`absolutise`]) is load-bearing for
    /// both.
    ///
    /// Done at the boundary rather than in `status` so every route agrees: the binary
    /// ([`crate::main`]), the hosted embedder ([`crate::host`]), [`Engine::load_symbols`] and a
    /// checkpoint restore all arrive here or at an already-absolutised value.
    pub fn set_symbols(&mut self, table: Option<SymbolTable>, path: Option<String>) {
        self.symbols = table.map(Arc::new);
        self.symbols_path = path.map(|p| absolutise(&p));
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
        // **The breakpoint rides here BARE, and that is the whole point of the surface.** This is the only
        // path a free-running machine executes on, so a breakpoint wrapped in [`Observe`] here would be a
        // breakpoint that never halts anything — the `resume` → `wait_for_break` idiom every consumer uses
        // goes through exactly this loop. The `stopAfter` watch beside it *is* wrapped, and the asymmetry is
        // principled rather than an oversight: a watch's stop is a **level** (`matched >= n` stays true
        // forever), so honouring it here would freeze the machine permanently, while a breakpoint's is an
        // **edge** evaluated per step boundary against the current PC, which cannot latch on.
        let resume_pc = self.sys.cpu_regs().pc;
        let mut brk = self
            .breakpoints
            .any_enabled()
            .then(|| BreakStop::new(&self.breakpoints, resume_pc));
        let mut sink = Fanout::new(
            &mut self.screen,
            Fanout::new(&mut brk, Fanout::new(Observe(armed), Observe(prof))),
        );
        self.sys.run_frames_with_sink(1, &mut sink);
        let observed = brk.and_then(|b| b.fired);
        self.latch_screen();
        // Free-running has no predicate of its own, so nothing can outrank the breakpoint here: an
        // observation on this path IS a firing, and is counted as one.
        if let Some((_, addr)) = observed {
            self.halt_on_breakpoint(addr);
        }
        self.config.free_run_pace
    }

    /// **The breakpoint halt, and the only place either run driver spells it.**
    ///
    /// `addr` is the address a [`BreakStop`] observed a step boundary on, *after* precedence has settled —
    /// so calling this is the statement "a breakpoint is what stopped this machine", and
    /// [`record_halt`](Breakpoints::record_halt) counts it here rather than in the sink.
    ///
    /// **It must clear BOTH flags.** They are separate on purpose ([`Engine::free_run`] /
    /// [`Engine::running`]) and a halt is the one event that ends both conditions at once: whoever is
    /// advancing the machine must stop doing it, *and* the machine must stop reporting itself as advancing.
    /// Clearing only `running` leaves the driver free-running while every reply says `running: false` — the
    /// exact contradiction the two-flag split exists to prevent, and it re-breaks once per frame forever
    /// (measured at **374,011** stops where the contract says 1, `docs/2026-08-27-breakpoints.md` §5).
    /// That defect is why this is one function and not two copies: there are now **two** run drivers that
    /// halt — this engine's own free-run step, and, since the hosted gap was closed, the player's loop by
    /// way of [`Host::pump`](crate::host::Host::pump) — and a second hand-written halt is a second chance
    /// to clear one flag.
    ///
    /// Not [`set_free_run`](Engine::set_free_run), which is `pause`'s path and would emit
    /// `reason: "pause"` — a knowing mislabel of a stop the client armed and is waiting to be told about
    /// by name.
    ///
    /// **Must be called inside a drain window**, like everything that touches the machine: it reads the
    /// stopping `pc` from `self.sys` and the event it emits carries the machine stamp (D11). Outside one,
    /// both would come from the placeholder `System` — `pc 0x00000000`, `frame 0, mclk 0`. That is why the
    /// hosted path *latches* its observation rather than applying it where it is made; see
    /// [`Host::record_break`](crate::host::Host::record_break).
    ///
    /// Returns whether a halt was recorded — `false` when no enabled breakpoint sits at `addr` any more,
    /// which a client that cleared it between the observation and the apply can produce.
    pub(crate) fn halt_on_breakpoint(&mut self, addr: u32) -> bool {
        let Some(id) = self.breakpoints.record_halt(addr) else {
            return false;
        };
        self.free_run = false;
        self.running = false;
        let pc = self.sys.cpu_regs().pc;
        let mut extra = Map::new();
        // §11.21's M2 clarification (ii): `breakpoint` is REQUIRED on the handle shape whenever
        // `reason` is `breakpoint`, and MUST NOT appear otherwise — the same if/then the schema
        // applies to `watch`.
        extra.insert("breakpoint".into(), json!(breakpoint_wire_id(id)));
        self.emit_stopped(StopReason::Breakpoint, pc, extra);
        true
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

    /// **Both run instruments and the breakpoint sink, borrowed for ONE run of a host's own loop.**
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
    /// **The third element is the breakpoint sink, and it is NOT wrapped in `Observe`.** That asymmetry is
    /// the whole content of this parcel. A watch's `stopAfter` is a *level* — `matched >= n` stays true
    /// forever — so a shared watch attached bare would end every one of a host's 1-frame runs before it
    /// began; a breakpoint's condition is an *edge*, re-evaluated per step boundary against the current PC,
    /// which cannot latch on. And a breakpoint that observes without halting is strictly **worse than one
    /// that is not attached at all**: it would count hits on a machine that never stopped, which is a
    /// believable wrong answer rather than a missing one. So the watch and the profiler are wrapped and
    /// this is bare, exactly as they are in [`free_run_step`](Engine::free_run_step).
    ///
    /// **`resume_pc` is the caller's, and it has to be.** [`BreakStop`] suppresses a fire at the PC the run
    /// *started* on, until one instruction has retired — without which a machine halted at a breakpoint
    /// could never be resumed past it. This engine cannot read that PC for itself here: outside a
    /// [`Host::pump`](crate::host::Host::pump) drain window it holds the placeholder `System`, whose PC is
    /// `0`. The run driver owns the real machine, so the run driver supplies it.
    ///
    /// The sink is attached only while some breakpoint is enabled, for [`watchpoints`](Engine::watchpoints)'
    /// reason one field over: an unarmed sink attached anyway costs the unarmed path a per-boundary lookup
    /// for nothing.
    ///
    /// **What a host owes in return**: the observation this sink latches has to come back, or the run
    /// stopped and nothing said so. See [`Host::record_break`](crate::host::Host::record_break) for where
    /// it goes and why it is applied at the top of the next drain rather than on the spot.
    ///
    /// Safe to call outside a drain window, like [`watchpoints_mut`](Engine::watchpoints_mut): all three
    /// are engine state rather than `System` state, so none answers for the placeholder machine.
    pub fn run_sinks(
        &mut self,
        resume_pc: u32,
    ) -> (
        Option<Observe<&mut Watchpoints>>,
        Option<Observe<&mut Profiler>>,
        Option<BreakStop<'_>>,
    ) {
        let watch_armed = self.watchpoints.watch_count() > 0;
        let profiler_armed = self.profiler_armed;
        let brk = self
            .breakpoints
            .any_enabled()
            .then(|| BreakStop::new(&self.breakpoints, resume_pc));
        (
            watch_armed.then_some(Observe(&mut self.watchpoints)),
            profiler_armed.then_some(Observe(&mut self.profiler)),
            brk,
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

    /// **The breakpoint set, for a host that wants to *show* what is armed to stop it.**
    ///
    /// The sibling of [`read_instruments`](Engine::read_instruments), and separate from it rather than a
    /// fourth element of that tuple for one reason: **a breakpoint is not an instrument.** The watch and
    /// the profiler *record* and are lent to a run wrapped in [`Observe`]; this set *halts* and is lent
    /// bare (see [`run_sinks`](Engine::run_sinks)). Both are shared borrows off `&self`, so a caller
    /// needing all four live at once simply calls both — which is the thing two `&mut self` accessors
    /// could not have done and the only reason `read_instruments` bundles three.
    ///
    /// R2's load-bearing case. This is the **same** `Breakpoints` `emulator/breakpoint_list`,
    /// `emulator/breakpoint_add` and `emulator/breakpoint_clear` operate on, so a panel drawing from it
    /// and a client reading the served row cannot disagree about what is armed. `&self` states the rest
    /// in the type: a panel cannot arm, disarm or clear through this borrow — every one of those goes
    /// back through [`Host::call`](crate::host::Host::call) and gets the handler's own refusal.
    ///
    /// **`hits` is not derivable from `enabled`, exactly as the profiler's armed flag is not derivable
    /// from its accumulator.** Disabling a breakpoint retains its count (§6: *"a client wanting a fresh
    /// count clears and re-adds"*), so a disabled row with 12,000 hits is a breakpoint that fired 12,000
    /// times and is not firing now — and a reader shown only the count could not tell it from a live one.
    ///
    /// Safe outside a drain window: the set is engine state rather than `System` state, so it does not
    /// answer for the placeholder machine.
    pub fn read_breakpoints(&self) -> &Breakpoints {
        &self.breakpoints
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
        // A plain frame advance has no predicate of its own, so it goes through the shared
        // `advance_with`/`attribute` pair with a null stop condition rather than building a fourth Fanout —
        // which is what keeps the breakpoint instrument on *this* path too. It was a separate Fanout until
        // breakpoints landed, and a separate Fanout is exactly how an instrument comes to ride four of the
        // five advancing shapes and silently miss the fifth.
        let mut never = StopWhen::new(|_, _| false);
        let (record, broke_at) = self.advance_with(frames, &mut never);
        self.attribute(record, false, broke_at)
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
        let (record, broke_at) = self.advance_with(max_frames, &mut stop);
        self.attribute(record, stop.fired(), broke_at)
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
        let (record, broke_at) = self.advance_with(max_frames, &mut stop);
        let fired = stop.fired;
        self.attribute(record, fired, broke_at)
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
        let (record, broke_at) = self.advance_with(max_frames, &mut stop);
        let fired = stop.fired;
        self.attribute(record, fired, broke_at)
    }

    /// The instrumented run every advancing method shares: the caller's stop condition, the screen capture,
    /// the watch instrument and the profiler in one [`Fanout`], for `max_frames` frames.
    ///
    /// The instrument rides here and it has to: a run that does not feed it produces a `seen == 0` reading —
    /// "the recorder was never attached" — from a run that really happened. The `Option` arm of
    /// `BusEventSink` is what expresses "only sometimes attached" without a second code path.
    fn advance_with<S: BusEventSink>(
        &mut self,
        max_frames: u64,
        stop: &mut S,
    ) -> (StopRecord, Option<(BreakpointId, u32)>) {
        // The run's own starting PC, latched *before* the run so the breakpoint sink can suppress the
        // re-trigger at it. See [`BreakStop`] for why that suppression is what makes a resume/wait/resume
        // loop able to make progress.
        let resume_pc = self.sys.cpu_regs().pc;
        let (record, broke_at) = {
            let mut brk = self
                .breakpoints
                .any_enabled()
                .then(|| BreakStop::new(&self.breakpoints, resume_pc));
            let armed = (self.watchpoints.watch_count() > 0).then_some(&mut self.watchpoints);
            let prof = self.profiler_armed.then_some(&mut self.profiler);
            let record = {
                let mut sink = Fanout::new(
                    &mut self.screen,
                    Fanout::new(stop, Fanout::new(&mut brk, Fanout::new(armed, prof))),
                );
                self.sys.run_frames_with_sink(max_frames, &mut sink)
            };
            (record, brk.and_then(|b| b.fired))
        };
        self.latch_screen();
        (record, broke_at)
    }

    /// **Two things can end an instrumented run, and the caller must not confuse them.**
    /// `StopRecord::fired` says only that *the sink* asked to stop — and with a watch in the Fanout that
    /// sink is an OR of two. `predicate_fired` is the caller's own condition, which is what `run_to` reports
    /// as `reached` and what a `step*` reports by emitting its `stopped`; anything else that ended the run
    /// early was a `stopAfter` watch, and is attributed rather than mislabelled as the caller's condition
    /// having been met.
    /// **Three things can now end an instrumented run, and the order they are checked in is the answer to
    /// "which one gets to name the stop".**
    ///
    /// 1. **The caller's own predicate** wins outright. A `run_to` whose target address also carries a
    ///    breakpoint reached its target; reporting `breakpoint` there would answer a question the caller
    ///    did not ask.
    /// 2. **A breakpoint**, next. It halts at an instruction boundary *before* the instruction runs, which
    ///    is the more precise of the two remaining conditions.
    /// 3. **A `stopAfter` watch**, last — it halts *after* a triggering instruction has committed, so if a
    ///    breakpoint fired on the same run the breakpoint is the earlier, sharper cause.
    ///
    /// Note that (2) is read from the sink's own latch rather than re-derived from the set, unlike (3):
    /// `watch_wanting_stop` has to re-derive because `Watchpoints::stop_requested` is one bool over every
    /// watch, whereas `BreakStop` already knows precisely which handle it stopped for.
    fn attribute(
        &mut self,
        record: StopRecord,
        predicate_fired: bool,
        observed: Option<(BreakpointId, u32)>,
    ) -> Advanced {
        // `&mut self` because THIS is where a firing is counted — see [`Breakpoints::record_halt`] for why
        // the count cannot happen in the sink. A breakpoint the caller's own predicate outranks did not
        // halt the machine and does not count.
        let broke_at = (!predicate_fired)
            .then_some(observed)
            .flatten()
            .and_then(|(_, addr)| self.breakpoints.record_halt(addr));
        let stopped_by = (record.fired() && !predicate_fired && broke_at.is_none())
            .then(|| self.watch_wanting_stop())
            .flatten();
        Advanced {
            record,
            predicate_fired,
            stopped_by,
            broke_at,
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
                // Derived, never asserted: the flag is true iff both rows are in `METHODS`, so serving one
                // of the pair cannot advertise the group (§11.28's rows are a pair, and a client that
                // branches on this would otherwise get a half-served surface reported as whole).
                "z80": METHODS.iter().any(|m| m.name == "emulator/z80_read")
                    && METHODS.iter().any(|m| m.name == "emulator/z80_write"),
                "vgm": false,
                // §11.25 / D4: *this build has the handlers*, never *a layout was detected*. True iff at
                // least one of the three ⚙ rows is in `methods` — S4's pin, taken because an "all three"
                // reading would have a build that dropped one row advertising `false` while serving two,
                // which is the under-advertising hazard §8 item 23 names in terms. Per-row servedness
                // stays `methods` membership and nothing else, so this flag never overrides it. The
                // detect result travels on every reply as `layout`, because `load_symbols` may be called
                // at any time and a handshake-time detect is stale by construction.
                "objectDecoders": METHODS.iter().any(|m| {
                    matches!(
                        m.name,
                        "emulator/object_slot" | "emulator/object_list" | "emulator/player_state"
                    )
                }),
                "profiler": true,
                // §11.21: *"`capabilities.breakpoints` stays a boolean meaning 'the family is served' and
                // is not widened, since a boolean a client already reads cannot become an object without
                // breaking that client"*. The cap therefore rides `limits.maxBreakpoints` below, and the
                // HANDLE-vs-address discriminator is the presence of `emulator/breakpoint_set_enabled` in
                // the `methods` list — which this server advertises, so a client reading that list learns
                // it is talking to the handle shape without asking a second question.
                "breakpoints": true,
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
                // **§11.21 design choice 3.** REQUIRED once the breakpoint family is advertised: §6 makes
                // the cap normative and says the refusal at it *"MUST NOT silently grow past the advertised
                // number"* — a number a client can only discover by hitting it is a number that costs a
                // lost breakpoint to learn. On `limits` rather than inside `capabilities.breakpoints`
                // because that capability is a boolean shipping clients already parse (§11.18).
                "maxBreakpoints": self.config.max_breakpoints,
            },
            // What the `frame` in every stamp actually *means* (`F-TRACE-PAL`). Advertised once, here,
            // rather than repeated on every reply: it is a property of the machine, not of the answer.
            // A client that caches frame coordinates across sessions can record the basis with them; a
            // client that ignores it was NTSC-only anyway. Constant while the core is NTSC-only — it
            // becomes a live value when PAL lands and this key does not change shape.
            "timingBasis": rpc::timing_basis_object(self.sys.timing_basis()),
            // **§2.1 / §11.31 — the per-`reason` stop-precision map.** Top-level, beside `timingBasis`
            // and `limits` and for their reason: every server that halts a machine HAS a stop precision,
            // so it is not something a server may or may not "support". Not inside
            // `capabilities.breakpoints`, which §11.21 deliberately kept a boolean and whose scope was
            // always too narrow for a key covering `runTo`, `step` and `watchpoint`; not inside `limits`,
            // where every key is a JSON number by rule.
            //
            // **Generated from `StopReason::ALL`, never written out.** Rule 1 requires the key set to be
            // *"the set of `reason` values this server can emit — no more, no fewer — derived from the
            // same registry that produces `methods` and `capabilities`"*, which is exactly the
            // `methodSummaries` discipline: a hand-written map beside a hand-written emitter is two
            // lists, and two lists drift.
            "stopPrecision": StopReason::ALL
                .iter()
                .map(|r| (r.wire().to_string(), json!(r.precision().wire())))
                .collect::<Map<String, Value>>(),
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

    /// **The one place an `emulator/stopped` is built**, which is what makes §3's *"REQUIRED on every
    /// `emulator/stopped`, for every `reason`"* structural rather than a rule fourteen call sites have to
    /// remember.
    ///
    /// `reason` is a [`StopReason`], not a `&str`: the reason and its declared precision come out of one
    /// registry, so the value on the wire cannot disagree with the value in the handshake, and a reason
    /// outside the registry cannot be emitted at all. §2.1's binding rule ("the event's value MUST be at
    /// least as strong as the declaration") is then satisfied by identity — this server emits exactly
    /// what it declared, never a stronger value it would also be entitled to emit, because a per-stop
    /// upgrade is a second source of truth for the same fact.
    fn emit_stopped(&self, reason: StopReason, pc: u32, extra: Map<String, Value>) {
        let mut params = extra;
        params.insert("reason".into(), json!(reason.wire()));
        params.insert("stopPrecision".into(), json!(reason.precision().wire()));
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
        if let Some(id) = run.broke_at {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("breakpoint".into(), json!(breakpoint_wire_id(id)));
            self.emit_stopped(StopReason::Breakpoint, pc, extra);
            return;
        }
        if let Some(id) = run.stopped_by {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("watch".into(), json!(watch_wire_id(id)));
            self.emit_stopped(StopReason::Watchpoint, pc, extra);
            return;
        }
        extra.insert("frames".into(), json!(frames));
        extra.insert("deadlineReached".into(), json!(true));
        self.emit_stopped(StopReason::RunFrames, pc, extra);
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
        // A breakpoint ends a bounded advance early exactly as a `stopAfter` watch does, and for the same
        // reason the rounding was refused: a caller establishing whether anything executed before its
        // breakpoint fired needs the exact count, including zero.
        if run.stopped_by.is_none() && run.broke_at.is_none() {
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
        symbol_at(self.symbols.as_deref()?, addr)
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
    /// One big-endian 16-bit word through [`Engine::debug_read`], so a word read inherits that
    /// function's region checks rather than reaching around them.
    ///
    /// Big-endian is not a choice here: it is how the 68000 stores, which is the rule `write_memory`'s
    /// own wording states and the reason the Z80 rows deliberately do NOT copy it.
    fn read_u16(&self, addr: u32) -> Result<u16, RpcError> {
        let (bytes, _) = self.debug_read(addr, 2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// The bus-space debug read, forwarded to the free [`debug_read`] so an **in-process panel and this
    /// handler run the same function over the same bytes** (the design's R1: one derivation, two
    /// consumers). See that function for why it is free rather than a method.
    fn debug_read(&self, addr: u32, len: usize) -> Result<(Vec<u8>, &'static str), RpcError> {
        debug_read(&self.sys, addr, len)
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
    ///
    /// # Why the mask is a parameter and not read off `self`
    ///
    /// Two callers want two different pictures out of this function and both are right.
    /// `emulator/screenshot` and `emulator/scanlines` want the *masked* picture — that is what the client
    /// asked to look at. `emulator/state_hash {includeFramebuffer}` wants the **unmasked** one: it is a
    /// determinism fingerprint of what the machine drew, and a digest that moved because a human toggled a
    /// debug layer would make two identical machines disagree for a reason that has nothing to do with
    /// either machine. Passing [`LayerMask::ALL`] explicitly at that call site is what states which of the
    /// two it is; there is deliberately no zero-argument version to fall into by accident.
    ///
    /// # A masked read cannot use the latched frame
    ///
    /// The retained frame was composited line by line *during the run*, by `Vdp::render_scanline`, which
    /// takes no mask — and it must not, since that is the render that commits the sprite latches. So a
    /// masked read takes the post-hoc path and says so (`from_raster: false`). Re-masking the latched RGB
    /// is not an option and not a shortcut missed: the retained rows are decoded colours with the losing
    /// layers already discarded, so "mask" applied there could only mean "paint over", which is the wrong
    /// answer this whole surface is built to avoid.
    fn framebuffer(&self, mask: LayerMask) -> (usize, Vec<Rgb>, bool) {
        if mask.is_all() {
            if let Some(f) = &self.last_frame {
                if f.width > 0 && f.rgb.len() == f.width * ACTIVE_LINES as usize {
                    return (f.width, f.rgb.clone(), true);
                }
            }
        }
        let width = self.sys.vdp().render_line_masked(0, mask).len();
        let mut fb = Vec::with_capacity(width * ACTIVE_LINES as usize);
        for line in 0..ACTIVE_LINES {
            fb.extend_from_slice(&self.sys.vdp().render_line_masked(line, mask));
        }
        (width, fb, false)
    }

    /// The masked layers' wire names, in [`Layer::ALL`] order, for a caveat that has to say *which* layers
    /// are hidden. Empty when nothing is masked.
    ///
    /// [`LayerMask::hidden`] is the whole body: the player's standing on-screen badge asks the identical
    /// question of the identical mask, and two functions answering it would be free to disagree about which
    /// layers a mask hides — in a caveat whose only job is to name them.
    fn masked_layer_names(&self) -> Vec<&'static str> {
        self.layers.hidden()
    }

    /// **The mask itself**, for a caller that owns the window this engine's picture is shown in.
    ///
    /// The player hosts this engine (`oracle-frontend`'s `bus.rs`), draws its own window, and now has its
    /// own layer toggles. Those toggles move *this* field — there is no second mask anywhere — which is what
    /// makes a mask set over the socket and a mask set from the palette the same mask, and what stops the
    /// window and `emulator/screenshot` from describing different pictures.
    ///
    /// Safe outside a drain window for the same reason [`watchpoints_mut`](Engine::watchpoints_mut) is: the
    /// mask is engine state and touches no `System`.
    pub fn layers(&self) -> LayerMask {
        self.layers
    }

    /// Set one layer's mask bit from outside the bus, returning whether `layer` is a mask target at all
    /// (`false` for [`Layer::Backdrop`], which leaves the mask untouched). The in-process twin of
    /// `emulator/set_layer_enabled`, and deliberately the same one line of state.
    pub fn set_layer(&mut self, layer: Layer, enabled: bool) -> bool {
        self.layers.set(layer, enabled)
    }

    /// The sentence `emulator/state_hash` owes when it hashed a framebuffer while a display mask was set —
    /// or `None` when no mask is set, which is what keeps the unmasked reply byte-identical to the one this
    /// row has always returned.
    ///
    /// Its subject is the opposite of [`mask_caveat`](Engine::mask_caveat)'s. That one tells a caller their
    /// *picture* is masked; this one tells them their *hash* is not — the two halves of the deliberate
    /// divergence between what `emulator/screenshot` shows and what this row fingerprints. Both read
    /// [`masked_layer_names`](Engine::masked_layer_names), so neither can name a layer the mask does not
    /// actually hide.
    fn masked_hash_caveat(&self) -> Option<String> {
        let masked = self.masked_layer_names();
        if masked.is_empty() {
            return None;
        }
        Some(format!(
            "A display layer mask is hiding {}, and `framebuffer` deliberately fingerprints the UNMASKED \
             picture — so it does NOT match what emulator/screenshot and emulator/scanlines are currently \
             showing you. That is on purpose: a determinism fingerprint that moved because someone hid a \
             layer would make two identical machines disagree for a reason that has nothing to do with \
             either machine. Clear the mask to fingerprint the picture you are looking at.",
            masked.join(", ")
        ))
    }

    /// The `caveat` a framebuffer read owes when **a mask** is what pushed it off the raster path, or
    /// `None` when no mask is set and the row's own pre-existing caveat is the true one.
    ///
    /// Deliberately additive. `screenshot` and `scanlines` each keep their existing text **byte-identical**
    /// for the case they were written for — telling a caller "the machine has not drawn a frame yet" when
    /// the real reason is their own mask would be a wrong answer dressed as a warning, and the reverse
    /// would silently retire a true one.
    fn mask_caveat(&self, kind: &str) -> Option<String> {
        let masked = self.masked_layer_names();
        if masked.is_empty() {
            return None;
        }
        Some(format!(
            "a display layer mask is active ({}), so this {kind} is composited from the VDP state as it \
             stands right now rather than taken from a retained raster frame — those are always drawn \
             unmasked. Mid-frame CRAM/scroll changes that a real raster would show on different lines are \
             NOT reproduced. The mask is a DISPLAY mask: it has not changed the machine, and \
             emulator/state_hash still fingerprints the unmasked picture.",
            masked.join(", ")
        ))
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
            // §11.29's rider (CR-H). Always emitted, in both directions: a caller that has to *probe*
            // `emulator/screen_text` by provoking a refusal cannot tell "no window" from "the call went
            // wrong", and an absent key would put it right back to guessing. Derived from the one field
            // the refusal is derived from, so the two can never disagree.
            "display": self.screen_text.is_some(),
        });
        if let Some((name, disp)) = self.symbol_at(pc) {
            out["symbolAtPc"] = json!(name);
            out["symbolDisp"] = json!(disp);
        }
        Ok(out)
    }

    /// **`emulator/screen_text`** (§11.29 / CR-H) — the text a human can read on the player window.
    ///
    /// A pure read of [`screen_text`](Engine::screen_text): no composition, no render, no `System` access,
    /// no timeline mutation. §6's run-control rule therefore does not reach it and it needs neither
    /// `require_paused` nor a `machineRunning` refusal.
    ///
    /// **The refusal, and why an empty list is forbidden.** The same `METHODS` list is served by a headless
    /// server *and* by the player, and "a window showing no text" is the default launch state — it already
    /// means something. An empty list would make *there is no screen* and *the screen is blank* the same
    /// artifact, which is this suite's recurring defect and the exact shape of a silent skip that reads as a
    /// pass. So no window is `-32005` with `reason: "noDisplay"` — §5's typed discriminant, the field
    /// clients branch on everywhere else on this bus — and `emulator/status`'s `display` lets a caller ask
    /// rather than probe by failing.
    ///
    /// **`truncated` is derived here, per surface, from `rendered != text`**, rather than carried across the
    /// seam. A producer cannot then publish a flag that disagrees with the pair beside it; a caller that
    /// wants the honest reading compares the two strings itself, which is what the fragment says the
    /// convenience flag is not a substitute for.
    ///
    /// **No number inside any of these strings is a bus field.** The status line's `F` is the window's own
    /// presentation counter (`F-WINDOW-BUS-FRAME-OFFBYONE`): it counts run *iterations*, so a mid-frame
    /// breakpoint stop adds a permanent +1 that never self-corrects, and a state load rewinds the clock
    /// while leaving it alone. This method serves the window's text **as text** and makes no claim that any
    /// number in it corresponds to `frameToken`.
    fn screen_text(&mut self, _params: &Value) -> Result<Value, RpcError> {
        let Some(all) = self.screen_text.as_ref() else {
            return Err(RpcError::invalid_state(
                "noDisplay",
                "this server has no window; screen text exists only in a hosted player",
                Value::Null,
            ));
        };
        let total = all.len();
        let surfaces: Vec<Value> = all
            .iter()
            .take(MAX_SCREEN_SURFACES)
            .map(|s| {
                json!({
                    "kind": s.kind.wire(),
                    "text": s.text,
                    "rendered": s.rendered,
                    // Derived, never carried. See the note above.
                    "truncated": s.rendered != s.text,
                    "unrenderable": s.unrenderable,
                })
            })
            .collect();
        Ok(json!({
            "surfaces": surfaces,
            "total": total,
            "returned": surfaces.len(),
            // §2.4 clause (a): REQUIRED even when false, so an absent field and a `false` one are not the
            // same artifact. Cursor-less by §2.4 clause (b) — this method accepts no continuation, so it
            // may not mint a token that can never be handed back.
            "truncated": total > surfaces.len(),
        }))
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
        if let Some(id) = run.broke_at {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("breakpoint".into(), json!(breakpoint_wire_id(id)));
            self.emit_stopped(StopReason::Breakpoint, record.pc, extra);
        } else if let Some(id) = run.stopped_by {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("watch".into(), json!(watch_wire_id(id)));
            self.emit_stopped(StopReason::Watchpoint, record.pc, extra);
        } else {
            extra.insert("target".into(), json!(hex::addr(target)));
            extra.insert("deadlineReached".into(), json!(!run.predicate_fired));
            self.emit_stopped(StopReason::RunTo, record.pc, extra);
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
        if let Some(id) = run.broke_at {
            out["caveat"] = json!(format!(
                "the target PC was never reached — breakpoint {} halted the run first, so NOTHING about \
                 the machine state follows from where it stopped",
                breakpoint_wire_id(id)
            ));
        } else if let Some(id) = run.stopped_by {
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
        if let Some(id) = run.broke_at {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("breakpoint".into(), json!(breakpoint_wire_id(id)));
            self.emit_stopped(StopReason::Breakpoint, record.pc, extra);
        } else if let Some(id) = run.stopped_by {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("watch".into(), json!(watch_wire_id(id)));
            self.emit_stopped(StopReason::Watchpoint, record.pc, extra);
        } else {
            // **No `line` key on the event, deliberately.** §3's `stopped` row lists no coordinate for this
            // reason, and `runToScanline` already names the condition; a new undeclared key would be exactly
            // the unregistered surplus CR-13 spent a ruling on. (`run_to`'s `target` predates that ruling and
            // is not a licence to add a second one.) The caller's own reply echoes `line`; a stream consumer
            // gets the reason, the `pc` and `deadlineReached`.
            extra.insert("deadlineReached".into(), json!(!run.predicate_fired));
            self.emit_stopped(StopReason::RunToScanline, record.pc, extra);
        }
        // `reached` is the LineStop's own verdict, never the sink's: with a `stopAfter` watch in the Fanout
        // `StopRecord::fired` means only "something asked to stop", and reading it here would report a line
        // as reached because an unrelated watch halted the run.
        let mut out = json!({
            "line": line,
            "reached": run.predicate_fired,
            "maxFrames": max_frames,
        });
        if let Some(id) = run.broke_at {
            out["caveat"] = json!(format!(
                "scanline {line} was never reached — breakpoint {} halted the run first, so NOTHING about \
                 the machine state follows from where it stopped",
                breakpoint_wire_id(id)
            ));
        } else if let Some(id) = run.stopped_by {
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
        // No `caveat`, and that is the fragment's word rather than an omission: it declares the key ABSENT
        // (the `sprites` precedent) because a step has no weaker-answer condition §6 names. §8 item 20's
        // closure would reject one, and the suite applies that closure to every reply.
        Ok(self.halt_result(pc))
    }

    /// **§6 — `emulator/step_over`.** No params → `pc`, `symbol`?, `symbolDisp`?
    ///
    /// **The empty result was the row until 2026-08-26, and §11.24 closed it.** §6 used to write `—` in
    /// both columns while `emulator/step` returned `pc` — an asymmetry §6 owned, which the fragments
    /// preserved on purpose and reported as **audit D-03**. The ruling went the other way: the three rows
    /// share **one stop condition** (§3 pins their `reason` as `step` for all three), and both servers
    /// already computed the halt PC and discarded it. So this row now returns `emulator/step`'s result,
    /// through [`halt_result`](Engine::halt_result) rather than a second spelling of the same answer.
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
        let pc = self.run_step(goal);
        Ok(self.halt_result(pc))
    }

    /// **§6 — `emulator/step_out`.** No params → `pc`, `symbol`?, `symbolDisp`?. Identical treatment to
    /// [`step_over`](Engine::step_over), for identical reasons (§11.24, closing audit D-03).
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
        let pc = self.run_step(goal);
        Ok(self.halt_result(pc))
    }

    /// **The result all three `step*` rows return** — `pc`, plus the symbol pair when it resolves.
    ///
    /// One builder, because since §11.24 there is one answer: the three rows share a single stop condition
    /// (§3), so they report the same halt, and the `stopped` event that accompanies them reports it too.
    /// Keeping the shape in one place is what stops the reply and the event drifting apart — and that
    /// drift is precisely what D-03 was: `emulator/step` returning a PC that `step_over` computed, held,
    /// and threw away.
    ///
    /// §4: the **BARE** label plus the number beside it. [`symbol_at`](Engine::symbol_at) returns exactly
    /// that pair — the same lookup [`emit_stopped`](Engine::emit_stopped) uses, deliberately not a second
    /// one — and the fragment's `$defs/symbolName` rejects a `+$hex` suffix outright. **Absent when
    /// nothing resolves**: a server MUST NOT fall back to the address string, so there is no `else` here
    /// on purpose, and `symbolDisp` goes with it, because a displacement from a symbol that was never
    /// reported is a number about nothing.
    fn halt_result(&self, pc: u32) -> Value {
        let mut out = json!({ "pc": hex::addr(pc) });
        if let Some((name, disp)) = self.symbol_at(pc) {
            out["symbol"] = json!(name);
            out["symbolDisp"] = json!(disp);
        }
        out
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
        if let Some(id) = run.broke_at {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("breakpoint".into(), json!(breakpoint_wire_id(id)));
            self.emit_stopped(StopReason::Breakpoint, pc, extra);
        } else if let Some(id) = run.stopped_by {
            extra.insert("deadlineReached".into(), json!(false));
            extra.insert("watch".into(), json!(watch_wire_id(id)));
            self.emit_stopped(StopReason::Watchpoint, pc, extra);
        } else {
            // `true` when the run ended on its frame bound rather than on the step condition (D12) — the
            // only channel on which a short step is visible at all, since none of the three results has a
            // key for it. `run_to` spells its complement the same way.
            extra.insert("deadlineReached".into(), json!(!run.predicate_fired));
            self.emit_stopped(StopReason::Step, pc, extra);
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
            // Forwarded to the free [`vdp_space_read`] for R1's reason (see [`debug_read`]): the Memory
            // panel reads the same three arrays under the same bound check, from the same function.
            _ => (vdp_space_read(&self.sys, space, addr, len)?, None),
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

    /// `emulator/write_vram` — bytes poked into VRAM (§6 *VRAM / CRAM / layers*, the row at line 1257).
    ///
    /// Served against contract revision `091ac59`. This row's fragment was **transcribed** in the
    /// 2026-08-22 first-fragment pass rather than repaired, and it carries three absences that are
    /// registered as audit **D-16**. All three are served **as written**, because a server that is
    /// quietly better than its contract is a divergence no client can discover:
    ///
    /// 1. **No pause gate.** §6's run-control state rule names `run_to`, `run_to_scanline`,
    ///    `run_frames`, `step*`, `press`, `play_input`, `reload_rom`, `write_memory`, `write_cram` and
    ///    `z80_write` — and **not this row**. So this handler does *not* call
    ///    [`require_paused`](Engine::require_paused), unlike its two sibling pokes. That is the
    ///    conservative direction and the only reversible one: §6 itself says *"relaxing a refusal later
    ///    is additive (D5); introducing one is not"*, so a server that invented the gate would have to
    ///    break clients to give it up, while a server that omits it can adopt the gate the day the
    ///    contract states it. The fragment's own `$comment` argues the rule *should* name this row —
    ///    §11.17's reason for naming `write_cram` (a game that composes its own state every frame
    ///    overwrites a direct write inside the frame it lands in) is if anything stronger for VRAM — and
    ///    that argument is filed upstream as a CR, not acted on here.
    /// 2. **No stated address bound.** §6's row states none. The bound enforced here is the one already
    ///    in force on the read side and stated by `emulator/read`'s note — *"Space sizes: bus 24-bit,
    ///    VRAM `$FFFF`, CRAM `$7F`, VSRAM `$4F`"* — so `read_vram` and `write_vram` refuse the same
    ///    addresses. Some bound is physically unavoidable (64 KiB of VRAM exists and no more); adopting
    ///    the read half's rather than inventing a second one is the smallest possible choice, and the
    ///    refusal follows every other write row in the catalog: **`-32004`, refused whole before any
    ///    byte lands, never clipped, never wrapped, never truncated**.
    /// 3. **No `value`+`width` spelling.** `bytes` is the only payload this row has — recorded upstream
    ///    as an asymmetry with `write_memory` and `z80_write` rather than a defect, since a tile blit is
    ///    a byte payload. Passing `value` or `width` here is therefore not a payload-spelling error but
    ///    an **undeclared param**, refused by §2.5's closure with `-32602` and the offending key named.
    ///
    /// Deliberately **no `len` ceiling**: `write_memory` bounds its payload by `limits.maxWriteLen` and
    /// its result's `len` by 4096, while this row's result declares `len` with `minimum: 0` and no
    /// maximum. The address bound caps a payload at 64 KiB on its own, so the row is not unbounded — it
    /// is bounded by the space instead of by a limit, and inventing a second, tighter refusal would be
    /// deviation 1's mistake in a different costume.
    ///
    /// The two **standing properties** of a poke, stated in §6 rather than as a `caveat` on every reply
    /// (§2.4; and the fragment declares `caveat` **absent**, so emitting one would fail item 20's
    /// closure): it is **never offered to the watch surface**, and it does **not repaint a frame already
    /// drawn**. Both fall out of [`Vdp::poke_vram`], which is where the reasoning lives — and which is
    /// also why this handler does not reach for `vram_mut`: that hook writes the bare array and would
    /// leave the sprite-attribute cache describing a table VRAM no longer holds.
    fn write_vram(&mut self, params: &Value) -> Result<Value, RpcError> {
        let addr = match params.get("addr") {
            None => {
                return Err(RpcError::invalid_params(
                    "`addr` is required (first VRAM byte to write)",
                ))
            }
            Some(v) => hex::parse_addr("addr", v)?,
        };
        let Some(b) = params.get("bytes") else {
            return Err(RpcError::invalid_params(
                "`bytes` is required — a hex payload is this row's only spelling",
            ));
        };
        let data = hex::parse_bytes("bytes", b)?;
        if data.is_empty() {
            return Err(RpcError::invalid_params(
                "`bytes` is empty — nothing to write",
            ));
        }

        // The whole request is checked before a single byte lands: a partial write followed by a refusal
        // leaves the caller unable to say what is in VRAM, which is the harm §11.22 named for `z80_write`
        // ("silently corrupts on overrun") in this row's own words.
        let size = self.sys.vram().len() as u64;
        let end = u64::from(addr) + data.len() as u64;
        if end > size {
            return Err(out_of_range(
                addr,
                "the write would run past the end of VRAM ($0000-$FFFF) — refused whole, never clipped",
            ));
        }

        let vdp = self.sys.vdp_mut();
        for (i, byte) in data.iter().enumerate() {
            vdp.poke_vram(addr as usize + i, *byte);
        }
        Ok(json!({ "addr": hex::addr(addr), "len": data.len() }))
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
    ///
    /// **Under a display layer mask it reports what was DRAWN, never what would have won.** `winner` is the
    /// post-mask winner and `rgb` is the masked render's pixel, so this row and the picture
    /// `emulator/screenshot` / `emulator/scanlines` hand back cannot disagree about a dot. A masked layer is
    /// **absent from `candidates`** rather than carrying a verdict: the list's contract is *"every layer
    /// that could have shown at this dot"*, and a masked one could not — it never entered the contest. The
    /// closed `verdict` vocabulary has no word for it either (`lostToPriority` names a reason that did not
    /// happen, `transparent` misreports opaque art, `operator` means a sprite operator), and inventing one
    /// would be a contract change taken unilaterally. `minItems: 1` already admits short lists — a blanked
    /// dot yields exactly one — so the shorter list is a shape the fragment allows.
    /// Parse and bounds-check a **native-dot** `x`/`y` pair, returning the dot and the active display.
    ///
    /// **Factored rather than copied, and that is the point.** `object_at`'s fragment says its params are
    /// *"`pixel_attribution`'s native-dot space with the same bounds"* — a claim that the two rows cannot
    /// drift apart. Two copies of this arithmetic would make that claim true only until somebody edited
    /// one of them, and the drift would be silent: both rows would still answer, about subtly different
    /// spaces. One function makes the CR's promise structural instead of aspirational.
    ///
    /// The two failures stay distinguishable, which is why the bound is applied twice: the schema bounds
    /// the *params* at `0..=511` (the widest addressable value) and a nonsensical coordinate is `-32602`,
    /// while the ACTIVE bound is the returned width/height and an off-display dot is `-32004`.
    fn native_dot(&mut self, params: &Value) -> Result<(u16, u16, u16, u16), RpcError> {
        let (width, height) = self.sys.vdp().active_display();
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
        Ok((x, y, width, height))
    }

    fn pixel_attribution(&mut self, params: &Value) -> Result<Value, RpcError> {
        let (x, y, width, height) = self.native_dot(params)?;
        let vdp = self.sys.vdp();

        let attr = vdp.pixel_attribution_masked(x, y, self.layers);
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
            // `LayerMask::ALL`, explicitly and always: this is a determinism fingerprint of what the
            // MACHINE drew. A display mask is the debugger's state, not the machine's, and a digest that
            // moved because someone hid plane A would make two identical machines disagree for a reason
            // that has nothing to do with either — the exact failure this whole hash exists to detect.
            let (_, fb, from_raster) = self.framebuffer(LayerMask::ALL);
            let mut bytes = Vec::with_capacity(fb.len() * 3);
            for (r, g, b) in fb {
                bytes.extend_from_slice(&[r, g, b]);
            }
            out["framebuffer"] = json!(hex_of(oracle_core::state_hash::fnv1a_bytes(&bytes)));
            // Two different pictures can be hashed here, so which one it was has to be on the wire: a
            // fingerprint whose provenance is ambiguous is a fingerprint two machines can disagree on for a
            // reason that has nothing to do with either machine.
            out["framebufferSource"] = json!(if from_raster { "raster" } else { "stateRender" });
            // **The other half of hashing at `LayerMask::ALL`.** A set mask is precisely one of the reasons
            // `framebufferSource` exists to rule out — the fragment's own words for that field are that *"a
            // fingerprint whose input provenance is unstated is worse than one that is simply wrong, because
            // two machines can disagree on it for a reason that has nothing to do with either machine"* —
            // and a mask is exactly such a reason. Left unsaid, a caller who hides plane A, screenshots, and
            // then hashes the framebuffer to pin what they are looking at gets the digest of a DIFFERENT
            // picture with nothing on the wire admitting it. The divergence is deliberate, so it is
            // announced; the hash itself does not move.
            //
            // Scoped to `include_fb` on purpose: with no framebuffer in the reply there is no unmasked
            // picture to disclaim, and a caveat that grew whenever a mask was set would change a reply that
            // has nothing to do with the mask.
            if let Some(extra) = self.masked_hash_caveat() {
                let base = out["caveat"]
                    .as_str()
                    .expect("state_hash always carries its own caveat")
                    .to_string();
                out["caveat"] = json!(format!("{base} {extra}"));
            }
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

        let (width, fb, from_raster) = self.framebuffer(self.layers);
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
            //
            // A mask has its own text and takes precedence, because when one is set it is *the* reason
            // these rows are post-hoc and the sentence below would be false. The unmasked text is
            // untouched.
            out["caveat"] = json!(self.mask_caveat("readback").unwrap_or_else(|| {
                "no completed frame is retained — the machine has not drawn one yet, or reset/reload_rom/\
                 restore dropped it — so these rows are rendered from the VDP state as it stands right \
                 now. Mid-frame CRAM/scroll changes that a real raster would show on different lines are \
                 NOT reproduced — run at least one frame for a scanline-accurate readback."
                    .to_string()
            }));
        }
        Ok(out)
    }

    // ----------------------------------------------------------------------------------------------
    // §6's object / player decoders (§11.25, CR-D)
    // ----------------------------------------------------------------------------------------------

    /// Read one whole record out of the bus, at the layout's own stride.
    fn slot_record(
        &self,
        layout: &decoders::ObjectLayout,
        slot: u32,
    ) -> Result<(u32, Vec<u8>), RpcError> {
        let addr = layout.slot_addr(slot);
        let (bytes, _) = self.debug_read(addr, layout.slot_bytes() as usize)?;
        Ok((addr, bytes))
    }

    /// Attach `name`/`nameDisp` for a decoded record, **or neither**.
    ///
    /// §4's identifying spelling, and §11.25's second hardening against the legacy server, which strips a
    /// `_Main` suffix so the name it reports resolves to nothing. [`Engine::symbol_at`] returns the bare
    /// label, which round-trips through `emulator/lookup_symbol`. The pair is omitted — never `""`, never
    /// a displacement without a name — when `ObjCodeBase` is absent or nothing resolves at the target.
    fn attach_code_name(&self, out: &mut Map<String, Value>, rec: &decoders::DecodedRecord<'_>) {
        attach_code_name(out, self.symbols.as_deref(), rec);
    }

    /// `emulator/object_list` — the active slots of the object pool (§6 ⚙, §11.25 D2).
    ///
    /// §2.4's flat bounded-list spelling, as `emulator/sprites` uses it, with one deliberate divergence
    /// from that row: `sprites` pins `total` to the table's size because every slot there is an item,
    /// while here an **empty slot is not an item**, so `total` counts active objects and the table's size
    /// lives in `layout.slotCount`. Two different facts, two homes. Presence *is* activity — an empty slot
    /// is omitted rather than returned with a flag that would always be true — so slot numbers are sparse.
    ///
    /// No cursor: the method accepts no continuation param, so a token it issued could never be handed
    /// back (§2.4 clause (b)).
    fn object_list(&mut self, params: &Value) -> Result<Value, RpcError> {
        let requested = decoder_fields_param(params)?;
        let include_bytes = decoder_include_bytes_param(params)?;
        let layout = decoders::derive(self.symbols.as_deref())?;
        let specs = match &requested {
            Some(names) => Some(layout.resolve_fields(names)?),
            None => None,
        };
        // Bounded at the pool's own size. This server advertises no `limits.maxObjectSlots`, so the
        // structural bound is the whole bound — which is the fragment's own account of what an absent
        // `maxObjectSlots` means. Refused above it, never clamped.
        let limit = match params.get("limit") {
            None => layout.slot_count() as usize,
            Some(v) => hex::parse_count("limit", v, 1, u64::from(layout.slot_count()))? as usize,
        };

        let mut total = 0usize;
        let mut items: Vec<Value> = Vec::new();
        for slot in 0..layout.slot_count() {
            let (addr, bytes) = self.slot_record(&layout, slot)?;
            let rec = decoders::DecodedRecord::new(&layout, slot, addr, bytes);
            if !rec.active() {
                continue;
            }
            total += 1;
            if items.len() < limit {
                let mut m = rec.to_json(true, specs.as_deref(), include_bytes);
                self.attach_code_name(&mut m, &rec);
                items.push(Value::Object(m));
            }
        }

        let bounded = rpc::bounded_array(items, total, 0, limit);
        let mut out = Map::new();
        out.insert("objects".into(), bounded["items"].clone());
        // `total: 0` beside `truncated: false` is "zero objects" as a stated fact, rather than an empty
        // list a client has to interpret.
        out.insert("total".into(), bounded["total"].clone());
        out.insert("returned".into(), bounded["returned"].clone());
        out.insert("limit".into(), bounded["limit"].clone());
        out.insert("truncated".into(), bounded["truncated"].clone());
        out.insert("layout".into(), layout.to_json());
        Ok(Value::Object(out))
    }

    /// `emulator/player_state` — the player pool, slot by slot (§6 ⚙, §11.25 D3).
    ///
    /// An **array**, never per-role keys: the legacy server's top-level key set varies by ROM
    /// (`player_1`/`player_2` on one branch, `main`/`sidekick` on the other, with an `engine` discriminant
    /// present on only one), which this repo's own transcription calls the biggest shape hazard in the
    /// eight. An array's key set does not vary, `role` carries the label without buying a key, and
    /// `layout` carries the discriminant on every reply.
    ///
    /// **Inactive slots are returned**, unlike `object_list`: "player 2 is not present" is the answer to
    /// the question asked, and a client must not have to infer it from an array's length against a bound
    /// it joins from elsewhere. When `active` is false the decoded keys are omitted rather than zeroed —
    /// see [`decoders::DecodedRecord::to_json`] for why that is a correctness rule and not a style.
    fn player_state(&mut self, params: &Value) -> Result<Value, RpcError> {
        let requested = decoder_fields_param(params)?;
        let include_bytes = decoder_include_bytes_param(params)?;
        let layout = decoders::derive(self.symbols.as_deref())?;
        let specs = match &requested {
            Some(names) => Some(layout.resolve_fields(names)?),
            None => None,
        };
        // Which slots are players is a property of the pool partition, so a listing that could not
        // produce one cannot answer this row at all — and refusing is the same call `derive` makes about
        // the base address. `object_list` and `object_slot` are unaffected: they need no partition.
        let pool = layout.player_pool()?;
        let (first, count) = (pool.first_slot, pool.slot_count);

        let mut players: Vec<Value> = Vec::with_capacity(count as usize);
        for slot in first..first + count {
            let (addr, bytes) = self.slot_record(&layout, slot)?;
            let rec = decoders::DecodedRecord::new(&layout, slot, addr, bytes);
            let mut m = rec.to_json(true, specs.as_deref(), include_bytes);
            // REQUIRED, `false` included: false is the answer, not the absence of one.
            m.insert("active".into(), json!(rec.active()));
            if rec.active() {
                self.attach_code_name(&mut m, &rec);
            }
            // **Survives inactivity** — the delta ruling's M5. The label is the slot's, not the
            // occupant's, and `layout.pools` carries pool names rather than per-slot roles, so forbidding
            // it here would delete the answer rather than displace it.
            if let Some(table) = self.symbols.as_deref() {
                if let Some(role) = decoders::slot_role(table, addr) {
                    m.insert("role".into(), json!(role));
                }
            }
            players.push(Value::Object(m));
        }

        // No `total`/`returned`/`truncated` and no cursor: the player pool is structurally bounded, and
        // §2.4 clause (d) says a structural bound takes neither.
        let mut out = Map::new();
        out.insert("players".into(), Value::Array(players));
        out.insert("layout".into(), layout.to_json());
        Ok(Value::Object(out))
    }

    /// The Z80 window's bounds check, shared by both rows so they cannot drift (§11.28).
    ///
    /// **Bounded at BOTH ends, and the end is the half that was missing.** The legacy server bounded only
    /// the start, then looped `addr + i` with no end check — so a multi-byte write near `$3FFF` folded past
    /// the window, clobbered `$0000`, and **replied success** (CR-B §5, read at `oracle-old d629771`).
    /// Refused **whole, before any byte lands**, never wrapped and never clamped.
    ///
    /// `-32004` rather than `-32602`: §11.28 aligned this with `read`/`memory_hash`/`write_memory`, which
    /// carry that code for the identical refusal. `-32602` stays for *shape* refusals — a `value` out of
    /// range, two payload spellings — and the two are different failures.
    fn z80_window(&self, addr: u32, len: usize) -> Result<(), RpcError> {
        z80_window(addr, len)
    }

    /// `emulator/z80_read` — the Z80's own 16 KB window (§6, §11.24 D-09, §11.28).
    ///
    /// `len` defaults to 1 and is capped at `$2000` by the fragment; above the ceiling it is **refused,
    /// never clamped** — the legacy server silently clamped `10000` to `8192`, which is a short read
    /// reported as a whole one.
    ///
    /// The `$2000`-`$3FFF` mirror is **the machine, not a defect**: it folds exactly as `z80/bus.rs` folds
    /// it, from the same mask, because a second implementation of the mirror here would be free to
    /// disagree with the one the guest sees.
    fn z80_read(&mut self, params: &Value) -> Result<Value, RpcError> {
        let addr = hex::parse_addr(
            "addr",
            params
                .get("addr")
                .ok_or_else(|| RpcError::invalid_params("`addr` is required"))?,
        )?;
        let len = match params.get("len") {
            Some(v) => hex::parse_count("len", v, 0, 0x2000)? as usize,
            None => 1,
        };
        // Forwarded to the free [`z80_read_window`] for R1's reason (see [`debug_read`]) — and the
        // `$2000-$3FFF` mirror fold is exactly the thing a second copy would be free to get wrong.
        let bytes = z80_read_window(&self.sys, addr, len)?;
        Ok(json!({"addr": hex::addr(addr), "len": len, "bytes": hex::bytes(&bytes)}))
    }

    /// `emulator/z80_write` — one byte per `value`, or a `bytes` payload laid down low-address-first.
    ///
    /// **There is no `width` and there will not be one** (§11.28, rejecting CR-B's B1): the Z80 bus is 8
    /// bits wide, so a multi-byte write is spelled `bytes` and the question of endianness never arises on
    /// the wire. Mirroring `write_memory`'s `width` was ruled against for a reason worth keeping visible —
    /// `write_memory`'s big-endian clause is a *consequence* of "as the 68000 stores", and copying the
    /// consequence to a little-endian CPU would land a pointer backwards.
    ///
    /// A **paused-machine write** under §6's run-control rule, with its siblings.
    fn z80_write(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/z80_write")?;
        let addr = hex::parse_addr(
            "addr",
            params
                .get("addr")
                .ok_or_else(|| RpcError::invalid_params("`addr` is required"))?,
        )?;
        let payload = match (params.get("bytes"), params.get("value")) {
            (Some(_), Some(_)) => {
                return Err(RpcError::invalid_params(
                    "`bytes` and `value` are two spellings of one payload — send one",
                ))
            }
            (Some(b), None) => hex::parse_bytes("bytes", b)?,
            // 0-255, refused outside rather than masked: a masked 0x1FF writing 0xFF is a wrong value
            // reported as success, which is the class §11.28 spends its first bullet on.
            (None, Some(v)) => vec![hex::parse_count("value", v, 0, 0xFF)? as u8],
            (None, None) => {
                return Err(RpcError::invalid_params(
                    "one of `bytes` or `value` is required",
                ))
            }
        };
        self.z80_window(addr, payload.len())?;
        let ram = self.sys.z80_ram_mut();
        for (i, b) in payload.iter().enumerate() {
            ram[(addr as usize + i) & (Z80_RAM_SIZE - 1)] = *b;
        }
        Ok(json!({"addr": hex::addr(addr), "len": payload.len()}))
    }

    /// `emulator/object_at` — one click, one answer, every failure named (§6 ⚙, §11.26 / CR-F).
    ///
    /// Joins a screen dot to the object that drew it. A **pure read** (no `require_paused`), on the
    /// `read`/`sprites`/`pixel_attribution`/`scanlines` footing, and a ⚙ decoder-group member: it derives
    /// the object layout and so inherits `-32012` when no listing is loaded.
    ///
    /// **The two halves are independent on purpose** (M3). A build can answer the camera and not the
    /// owner table, or the reverse; each half reports its own availability so the row answers what it can
    /// instead of refusing both. That is why `worldSource` exists as a field rather than as an inference
    /// from `world` being absent.
    ///
    /// ⚑ **Every address here is resolved BY SYMBOL, per loaded build, on every call** (§11.26 M3, and
    /// normative). Caching one would be the defect the CR was amended to forbid: `Camera_X`/`Camera_Y`
    /// **exist in a release build and MOVE** (`FFFFA576` vs `FFFFA604`), so a stale address yields no
    /// fault and a plausible number — click-to-world landing silently in the wrong place. `Sprite_Owner`
    /// is the kinder case, absent from release entirely, so its staleness at least announces itself.
    fn object_at(&mut self, params: &Value) -> Result<Value, RpcError> {
        let (x, y, _width, _height) = self.native_dot(params)?;
        let winner = self
            .sys
            .vdp()
            .pixel_attribution_masked(x, y, self.layers)
            .winner;
        // ⚙ group membership: refuses with -32012 when no listing is loaded, exactly as the decoder rows
        // do, rather than inventing a base address (§11.25).
        let layout = decoders::derive(self.symbols.as_deref())?;

        let mut out = Map::new();
        out.insert("dot".into(), json!({"x": x, "y": y}));

        // --- the world half -------------------------------------------------------------------
        // UNBIASED `Camera_X`/`Camera_Y`. The biased neighbours are the plausible liar the CR names:
        // `Camera_X_Biased` read 65504 in the same halt, which is obvious garbage unsigned and an
        // entirely believable -32 signed, offsetting every answer by 128 and being caught by nobody.
        let camera = self.symbols.as_deref().and_then(|t| {
            let cx = t.address_of("Camera_X")?;
            let cy = t.address_of("Camera_Y")?;
            Some((cx, cy))
        });
        match camera {
            Some((cx, cy)) => {
                let rx = u32::from(self.read_u16(cx)?);
                let ry = u32::from(self.read_u16(cy)?);
                out.insert("worldSource".into(), json!("camera"));
                out.insert(
                    "world".into(),
                    json!({"x": rx + u32::from(x), "y": ry + u32::from(y)}),
                );
            }
            // `world` is OMITTED, never zeroed: the schema enforces present-iff-camera, and a zero here
            // would be a coordinate a client would happily use.
            None => {
                out.insert("worldSource".into(), json!("unavailable"));
            }
        }

        out.insert("winner".into(), layer_json(winner));

        // --- the owner half -------------------------------------------------------------------
        let mut owner = Map::new();
        match self
            .symbols
            .as_deref()
            .and_then(|t| t.address_of("Sprite_Owner"))
        {
            // No table in this build at all. `unavailable`, and NO `raw` — there was no word to read, and
            // serving `0x0000` would be indistinguishable from the `none` that means the table answered.
            None => {
                owner.insert("kind".into(), json!("unavailable"));
            }
            Some(table_addr) => {
                // Indexed by SAT slot. A non-sprite winner has no entry to read, and its answer is the
                // same fact stated honestly: the table exists, and nothing in it owns this dot.
                let word = match winner {
                    Layer::Sprite(i) => self.read_u16(table_addr + 2 * u32::from(i))?,
                    _ => 0,
                };
                owner.insert("raw".into(), json!(hex::u16_hex(word)));
                match word {
                    0x0000 => owner.insert("kind".into(), json!("none")),
                    // ⚑ The sentinels are checked BEFORE any rebase, and that ordering is the whole
                    // guard. `DrawRings` stamps a bare `move.w #1`, not an address; rebasing `0x0001`
                    // would yield a garbage index and CONFIDENTLY NAME THE WRONG OBJECT. Two of the
                    // three sprites on the first screen this was tried on were rings, so this is the
                    // common case, not a corner.
                    0x0001 => owner.insert("kind".into(), json!("ring")),
                    0x0002 => owner.insert("kind".into(), json!("mask")),
                    w => {
                        // The word is the low 16 bits of an SST address; the base's low word is what it
                        // is measured against, so the arithmetic never leaves that space.
                        let base = layout.slot_addr(0) & 0xFFFF;
                        let stride = layout.slot_bytes();
                        let off = u32::from(w).wrapping_sub(base);
                        let slot = off / stride;
                        // Refuse rather than name a slot: a word that is not on a record boundary, or
                        // points past the table, is not an object address and must not be reported as
                        // one. `raw` is already served above, so the caller can audit exactly what we saw.
                        if u32::from(w) < base || off % stride != 0 || slot >= layout.slot_count() {
                            owner.insert("kind".into(), json!("none"))
                        } else {
                            owner.insert("slot".into(), json!(slot));
                            owner.insert("kind".into(), json!("object"))
                        }
                    }
                };
            }
        }
        out.insert("owner".into(), Value::Object(owner));
        out.insert("layout".into(), layout.to_json());
        Ok(Value::Object(out))
    }

    /// `emulator/object_slot` — one addressed slot (§6 ⚙, §11.25 D5).
    ///
    /// The single-slot projection of `object_list`, with the item keys hoisted to the top level plus
    /// `active` — because this row **addresses** a slot, so emptiness is an answer rather than an
    /// omission. A slot past the pool is `-32602` with the bound in `error.data`, never clamped: the
    /// fragment cannot bound it because the bound is a property of the loaded game. §11.25 records that
    /// the contract is split here — `pixel_attribution` answers `-32004` for a structurally identical
    /// refusal — and this family follows `scanlines`.
    fn object_slot(&mut self, params: &Value) -> Result<Value, RpcError> {
        let requested = decoder_fields_param(params)?;
        let include_bytes = decoder_include_bytes_param(params)?;
        let Some(raw_slot) = params.get("slot") else {
            return Err(RpcError::invalid_params(
                "`slot` is required — this row addresses one slot",
            ));
        };
        let layout = decoders::derive(self.symbols.as_deref())?;
        let specs = match &requested {
            Some(names) => Some(layout.resolve_fields(names)?),
            None => None,
        };
        let slot_count = layout.slot_count();
        // `parse_count` gives the shared `-32602` spelling for a non-number, a negative and an
        // out-of-range value alike; the typed `error.data` below is what the fragment asks for on top.
        let slot = hex::parse_count("slot", raw_slot, 0, u64::from(slot_count).saturating_sub(1))
            .map_err(|e| {
            if raw_slot.as_u64().is_some() {
                RpcError::invalid_params(format!(
                    "`slot` {} is past the end of the object pool — this build has {slot_count} \
                         slots (0..={}), and the bound is refused rather than clamped",
                    raw_slot,
                    slot_count - 1
                ))
                .with_data(json!({"slot": raw_slot, "slotCount": slot_count}))
            } else {
                e
            }
        })?;

        let (addr, bytes) = self.slot_record(&layout, slot as u32)?;
        let rec = decoders::DecodedRecord::new(&layout, slot as u32, addr, bytes);
        let mut out = rec.to_json(true, specs.as_deref(), include_bytes);
        out.insert("active".into(), json!(rec.active()));
        if rec.active() {
            self.attach_code_name(&mut out, &rec);
        }
        out.insert("layout".into(), layout.to_json());
        Ok(Value::Object(out))
    }

    // ----------------------------------------------------------------------------------------------
    // §6's three object MUTATION rows (§11.32, CR-J) — the live-object mailbox
    // ----------------------------------------------------------------------------------------------

    /// One whole mailbox exchange: resolve, assert, write payload, **write the flag last**, advance
    /// until the engine acknowledges, read the status. One indivisible operation on the engine thread.
    ///
    /// # Why the advance is here at all, and why it is not a mode change
    ///
    /// Aeon's prose says the mailbox is consumed "on a paused frame". That is the **game's**
    /// `Game_Paused`, tested at the first instruction of `RunObjects` — not this server's pause. Under
    /// an *emulator* pause no frames execute, `objreq_consume` never runs, the flag is never cleared,
    /// and a server that wrote the mailbox and then waited for the ack would **hang forever against a
    /// correctly-working engine**. So the server advances the machine itself.
    ///
    /// §5 forbids resolving a wrong-*state* case implicitly — pausing a running machine to service a
    /// call and leaving it paused. This changes no mode: the machine is paused before and paused after.
    /// It changes the machine's **position**, which is what `step`, `run_to` and `run_frames` all do
    /// under the same paused precondition, and `framesAdvanced` is on every reply, success and failure
    /// alike, so a caller can always reconstruct where it ended up.
    ///
    /// **No `resumed`/`stopped` events are emitted.** Those announce a change of mode to stream
    /// consumers, and no mode changed here; the stamp's `frame` moves visibly on the reply, which is
    /// the honest report of what did change. (`emulator/press` emits them because it *runs* the
    /// machine; this collects an acknowledgement.)
    ///
    /// # The residual window this cannot close
    ///
    /// Between the flag write and `objreq_consume` there is, by construction, the remainder of the
    /// current frame's object code. Paused at the game's frame top that remainder is empty; paused
    /// mid-frame it is not, and a slot deleted and recycled in it resolves to a **new occupant** — a
    /// clean `status 0` on the wrong object. `expectFrameToken` closes the client→server half
    /// completely; nothing outside the machine closes the other half. See
    /// [`Engine::objreq_midframe_caveat`] for what this server can and cannot tell about it.
    fn objreq_exchange(
        &mut self,
        method: &str,
        req: ObjReqRequest,
        params: &Value,
    ) -> Result<ObjReqAck, RpcError> {
        self.require_paused(method)?;
        // Resolved BY NAME, individually, on every call — never an offset from another cell (§11.32 J5).
        // A build missing any name is refused here, before anything is written anywhere.
        let mailbox = objreq::resolve(self.symbols.as_deref())?;
        mailbox.assert_layout()?;

        let max_frames = match params.get("maxFrames") {
            None => OBJREQ_DEFAULT_MAX_FRAMES,
            // The fragment declares no ceiling; this server bounds it by `limits.maxRunFrames` all the
            // same, because an unbounded budget is an unbounded run and that is the transport hang
            // `run_to`'s own `maxFrames` exists to prevent. Filed upstream as the row wanting the same
            // tie the run-shaped rows have.
            Some(v) => hex::parse_count("maxFrames", v, 1, self.config.max_run_frames)?,
        };
        if let Some(v) = params.get("expectFrameToken") {
            let want = hex::parse_count("expectFrameToken", v, 0, u64::MAX)?;
            let have = self.frame();
            if want != have {
                return Err(RpcError::invalid_state(
                    "frameMoved",
                    format!(
                        "the machine is at frame {have}, not the frame {want} this request was built \
                         against — refusing rather than acting on a machine that moved under the caller. \
                         Re-read the state you are addressing and try again."
                    ),
                    json!({"expectFrameToken": want, "frameToken": have, "framesAdvanced": 0}),
                ));
            }
        }

        // The payload, then the flag LAST. That ordering IS the concurrency control, and it is the one
        // line of this function that must not be reordered for tidiness.
        self.poke(mailbox.at(objreq::DEF), &req.def.to_be_bytes())?;
        self.poke(mailbox.at(objreq::X), &req.x.to_be_bytes())?;
        self.poke(mailbox.at(objreq::Y), &req.y.to_be_bytes())?;
        self.poke(mailbox.at(objreq::SLOT), &req.slot.to_be_bytes())?;
        self.poke(mailbox.at(objreq::PLACE), &req.place.to_be_bytes())?;
        self.poke(mailbox.at(objreq::OP), &[req.op])?;
        self.poke(mailbox.at(objreq::FLAG), &[1])?;

        let mut interrupted = false;
        let mut advanced = 0u64;
        for _ in 0..max_frames {
            // Counted through the same [`Engine::frames_advanced`] every other advancing row uses, one
            // frame at a time. **Not** as a total mclk delta divided by the frame length: an advance
            // that starts mid-frame ends at that frame's boundary, so two `advance(1)` calls from a
            // mid-frame pause cover less than two whole frames of clock and a division would report `1`
            // for two frames that really ran. Measured, on the frame-count probe this parcel took.
            let mclk_before = self.sys.scheduler().now();
            let run = self.advance(1);
            advanced += self.frames_advanced(&run, 1, mclk_before);
            if self.read_u8(mailbox.at(objreq::FLAG))? == 0 {
                break;
            }
            // A breakpoint or a `stopAfter` watch can end an advance early, which leaves the request
            // armed on a machine that did not finish its frame. Reported rather than retried: retrying
            // would run past a halt the caller asked for.
            if run.stopped_by.is_some() || run.broke_at.is_some() {
                interrupted = true;
                break;
            }
        }
        if self.read_u8(mailbox.at(objreq::FLAG))? != 0 {
            // **CANCEL, per §11.32 Q2.** Left armed, the request fires whenever the game next enters the
            // one state that carries the consumer — possibly minutes after the client was told it
            // failed, which is a world-change traced to an error reply. The clear is race-free because
            // the machine is paused. `Obj_Req_Op` is deliberately left alone, so a watchpoint can still
            // see what the last request was.
            self.poke(mailbox.at(objreq::FLAG), &[0])?;
            return Err(RpcError::invalid_state(
                "mailboxNotConsumed",
                format!(
                    "the game is not in a state that services this mailbox: {advanced} frame(s) ran and \
                     the request was never acknowledged{}. The consumer is spliced into one game state's \
                     frame top, so outside it every request times out. The request has been CANCELLED so \
                     it cannot fire later.",
                    if interrupted {
                        ", and a breakpoint or watch ended the advance early"
                    } else {
                        ""
                    }
                ),
                json!({
                    "cancelled": true,
                    "framesAdvanced": advanced,
                    "maxFrames": max_frames,
                    "advanceInterrupted": interrupted,
                }),
            ));
        }
        // The flag reads 0, so — and only so — the status byte is this request's.
        let status = self.read_u8(mailbox.at(objreq::STATUS))?;
        Ok(ObjReqAck {
            mailbox,
            status,
            frames_advanced: advanced,
        })
    }

    /// The reply body every one of the three rows shares: `handle`, `addr`, `slot`?, `framesAdvanced`,
    /// `layout`, `caveat`?.
    ///
    /// `handle` is the **low word of `addr`**, and the two are related by arithmetic the contract
    /// states, which is why it is a `$defs/hex` string and not an opaque handle. The address is the
    /// sign-extension of the handle — the engine's own `movea.w d1, a0`, not a convention invented
    /// here.
    fn objreq_reply(
        &mut self,
        handle: u16,
        frames_advanced: u64,
        layout: &decoders::ObjectLayout,
    ) -> Map<String, Value> {
        let addr = objreq_handle_addr(layout, handle);
        let mut out = Map::new();
        out.insert("handle".into(), json!(hex::u16_hex(handle)));
        out.insert("addr".into(), json!(hex::addr(addr)));
        if let Some(slot) = objreq_slot_of(layout, addr) {
            out.insert("slot".into(), json!(slot));
        }
        out.insert("framesAdvanced".into(), json!(frames_advanced));
        out.insert("layout".into(), layout.to_json());
        if let Some(c) = self.objreq_midframe_caveat() {
            out.insert("caveat".into(), json!(c));
        }
        out
    }

    /// `x`/`y` for a spawn or a move reply: **re-read from the record after the frame advance, never an
    /// echo of the accepted request** (§11.32's 2026-09-03 addendum, this lane's own ruling).
    ///
    /// Echoing carries zero information — the client already holds those numbers. The re-read is the
    /// actual state of the machine, which is what every other reply on this bus reports, and it is the
    /// only version that can be usefully wrong. **Stated limit:** it conflates *the engine adjusted your
    /// requested position on spawn* with *the object moved under its own velocity*; `framesAdvanced`
    /// says time passed and separates neither. Nobody may read it as a spawn-position confirmation.
    ///
    /// The values are the decoder's own signed reading of the position word, so they join
    /// `emulator/object_list` exactly. That is deliberately *not* the param's unsigned 0-65535: the
    /// reply's job is to agree with the other instrument on this bus, not with the request.
    ///
    /// A record that is no longer active after the advance — the object culled or collected itself
    /// inside the frame — still owes the fragment its required `x`/`y`, so the last written values are
    /// reported with a `caveat` naming that they are no longer live.
    fn objreq_position(
        &mut self,
        out: &mut Map<String, Value>,
        layout: &decoders::ObjectLayout,
        addr: u32,
    ) -> Result<(), RpcError> {
        let (bytes, _) = self.debug_read(addr, layout.slot_bytes() as usize)?;
        let slot = objreq_slot_of(layout, addr).unwrap_or(0);
        let rec = decoders::DecodedRecord::new(layout, slot, addr, bytes);
        let (x, y) = rec.position();
        out.insert("x".into(), json!(x));
        out.insert("y".into(), json!(y));
        if !rec.active() {
            // The record went inactive inside the advanced frame(s). `x`/`y` are REQUIRED here, so the
            // honest answer is the last values the game wrote plus a caveat that says so — rather than
            // omitting a required key or fabricating a live position. (The fragment describes `caveat`
            // as carrying exactly one thing, the mid-frame window; this is a second, rarer condition
            // that a client must not read past, and it is filed upstream as such.)
            out.insert(
                "caveat".into(),
                json!(
                    "the slot is no longer active: the object was removed inside the frame(s) this call \
                     advanced, so `x`/`y` are the last values its record carried and not a live position."
                ),
            );
        }
        Ok(())
    }

    /// The disclosed mid-frame window, **only where this server can tell that it applied**.
    ///
    /// It cannot, and saying so is the measurement's own answer rather than a hedge: this server pauses
    /// at an *instruction boundary*, and it has no landmark for the game's frame top. The consumer is a
    /// comptime template and declares no symbol at all, so there is nothing to resolve; the enclosing
    /// game-state proc is a symbol, but a PC inside it says nothing about whether this frame's
    /// `objreq_consume` has already run. An unconditional caveat on every reply is a field nobody
    /// reads, and §11.32 declares this key for exactly one condition, so a server that cannot tell
    /// emits none — which is this one, today. The window itself is disclosed on the row's description.
    fn objreq_midframe_caveat(&self) -> Option<String> {
        None
    }

    /// `emulator/object_spawn` — place one archetype (§6 ⚙, §11.32).
    fn object_spawn(&mut self, params: &Value) -> Result<Value, RpcError> {
        // Rule (5) wants `framesAdvanced` on EVERY reply, success and failure — so the
        // refusals that never got as far as an advance say `0` rather than saying nothing.
        // `0` is the answer, not the absence of one.
        self.object_spawn_inner(params)
            .map_err(objreq_frames_default)
    }

    fn object_spawn_inner(&mut self, params: &Value) -> Result<Value, RpcError> {
        let def = self.objreq_def_param(params)?;
        let x = objreq_pixel_param(params, "x")?;
        let y = objreq_pixel_param(params, "y")?;
        let subtype = match params.get("subtype") {
            None => 0u8,
            Some(v) => hex::parse_count("subtype", v, 0, 255)? as u8,
        };
        let flip_h = objreq_bool_param(params, "flipH")?;
        let flip_v = objreq_bool_param(params, "flipV")?;
        // The one rail pre-flighted here (§11.32 Q1): a `def` outside the cart window is refused BEFORE
        // any write, so a pointer that cannot be an archetype never reaches the machine. The other three
        // rails are the engine's, and its status 2 carries them.
        if def >= objreq::CART_WINDOW_END {
            return Err(RpcError::invalid_params(format!(
                "`def` {} is outside the cart address window ($000000-$3FFFFF); an archetype pointer \
                 cannot live there, so this is refused before anything is written",
                hex::addr(def)
            ))
            .with_data(json!({"def": hex::addr(def)})));
        }
        let layout = decoders::derive(self.symbols.as_deref())?;
        let ack = self.objreq_exchange(
            "emulator/object_spawn",
            ObjReqRequest {
                op: objreq::OP_SPAWN,
                def,
                x,
                y,
                slot: 0,
                place: objreq::place_word(subtype, flip_h, flip_v),
            },
            params,
        )?;
        if ack.status != objreq::OK {
            return Err(objreq::status_error(
                ack.status,
                ack.frames_advanced,
                &objreq::StatusContext {
                    def: Some(hex::addr(def)),
                    handle: None,
                    dynamic_slots: layout.pool("dynamic").map(|p| p.slot_count),
                },
            ));
        }
        // The engine publishes the new slot's handle into `Obj_Req_Slot` before it clears the flag.
        let handle = self.read_u16(ack.mailbox.at(objreq::SLOT))?;
        // …and it must seat in the pool this server decoded, or the two disagree about what the machine
        // is. Refused rather than answered: with a handle that seats nowhere, `slot` is omitted and
        // `x`/`y` become a decode of bytes that are not a record — the ⚙ group's rule (3) defect, at the
        // top level and dressed as a success.
        if objreq_slot_of(&layout, objreq_handle_addr(&layout, handle)).is_none() {
            return Err(RpcError::new(
                code::INTERNAL_ERROR,
                format!(
                    "the engine reported success and published handle {}, which does not seat in the \
                     object pool this server decoded — the reply would describe bytes that are not a \
                     record, so it is refused instead.",
                    hex::u16_hex(handle)
                ),
            )
            .with_data(json!({
                "handle": hex::u16_hex(handle),
                "framesAdvanced": ack.frames_advanced,
                "layout": layout.to_json(),
            })));
        }
        let mut out = self.objreq_reply(handle, ack.frames_advanced, &layout);
        self.objreq_position(&mut out, &layout, objreq_handle_addr(&layout, handle))?;
        Ok(Value::Object(out))
    }

    /// `emulator/object_move` — reposition one live dynamic object (§6 ⚙, §11.32).
    ///
    /// **Position only, and no clamp** — both are the engine's semantics and a client will assume the
    /// opposite of both. Velocity, status, angle and animation are untouched, so a moved badnik keeps
    /// doing whatever it was doing from its new place; and an out-of-act object is simply culled by the
    /// camera-distance test rather than being pulled back inside.
    fn object_move(&mut self, params: &Value) -> Result<Value, RpcError> {
        // Rule (5) wants `framesAdvanced` on EVERY reply, success and failure — so the
        // refusals that never got as far as an advance say `0` rather than saying nothing.
        // `0` is the answer, not the absence of one.
        self.object_move_inner(params)
            .map_err(objreq_frames_default)
    }

    fn object_move_inner(&mut self, params: &Value) -> Result<Value, RpcError> {
        let x = objreq_pixel_param(params, "x")?;
        let y = objreq_pixel_param(params, "y")?;
        let (handle, layout) = self.objreq_target(params)?;
        let ack = self.objreq_exchange(
            "emulator/object_move",
            ObjReqRequest {
                op: objreq::OP_MOVE,
                def: 0,
                x,
                y,
                slot: handle,
                place: 0,
            },
            params,
        )?;
        if ack.status != objreq::OK {
            return Err(objreq::status_error(
                ack.status,
                ack.frames_advanced,
                &objreq::StatusContext {
                    def: None,
                    handle: Some(hex::u16_hex(handle)),
                    dynamic_slots: layout.pool("dynamic").map(|p| p.slot_count),
                },
            ));
        }
        let mut out = self.objreq_reply(handle, ack.frames_advanced, &layout);
        self.objreq_position(&mut out, &layout, objreq_handle_addr(&layout, handle))?;
        Ok(Value::Object(out))
    }

    /// `emulator/object_delete` — remove one live dynamic object (§6 ⚙, §11.32).
    ///
    /// The delete **cascades**: the engine's `DeleteObject` takes the slot's child chain with it, so a
    /// debug-spawned parent takes its children and no lifetime tracking of ours is needed. A slot the
    /// entity window owns is refused (`slotOwnedByEntityWindow`) — the asymmetry with `object_move`,
    /// which is allowed on such a slot because it touches no bookkeeping.
    ///
    /// There is deliberately **no `deleted: true`** in the result: a field that is `true` on every
    /// success is §11.5's *"`released`'s defect with a useful name"*, and the failure path is a typed
    /// error, so the boolean could never carry information.
    fn object_delete(&mut self, params: &Value) -> Result<Value, RpcError> {
        // Rule (5) wants `framesAdvanced` on EVERY reply, success and failure — so the
        // refusals that never got as far as an advance say `0` rather than saying nothing.
        // `0` is the answer, not the absence of one.
        self.object_delete_inner(params)
            .map_err(objreq_frames_default)
    }

    fn object_delete_inner(&mut self, params: &Value) -> Result<Value, RpcError> {
        let (handle, layout) = self.objreq_target(params)?;
        let ack = self.objreq_exchange(
            "emulator/object_delete",
            ObjReqRequest {
                op: objreq::OP_DELETE,
                def: 0,
                x: 0,
                y: 0,
                slot: handle,
                place: 0,
            },
            params,
        )?;
        if ack.status != objreq::OK {
            return Err(objreq::status_error(
                ack.status,
                ack.frames_advanced,
                &objreq::StatusContext {
                    def: None,
                    handle: Some(hex::u16_hex(handle)),
                    dynamic_slots: layout.pool("dynamic").map(|p| p.slot_count),
                },
            ));
        }
        Ok(Value::Object(self.objreq_reply(
            handle,
            ack.frames_advanced,
            &layout,
        )))
    }

    /// `def` **or** `defSymbol`, exactly one — the `addr`|`symbol` pattern with the spellings
    /// deliberately not borrowed (§11.32 Q5: `addr` on a spawn row reads as *where to put it*, which is
    /// `x`/`y`).
    fn objreq_def_param(&self, params: &Value) -> Result<u32, RpcError> {
        match (params.get("def"), params.get("defSymbol")) {
            (Some(_), Some(_)) => Err(RpcError::invalid_params(
                "exactly one of `def` (hex string) and `defSymbol` (name) — both were given",
            )),
            (None, None) => Err(RpcError::invalid_params(
                "exactly one of `def` (hex string) and `defSymbol` (name) is required: the archetype to \
                 spawn from. `emulator/lookup_symbol`'s prefix search over `ObjDef_` lists them.",
            )),
            (Some(a), None) => hex::parse_addr("def", a),
            (None, Some(name)) => {
                let Some(name) = name.as_str() else {
                    return Err(RpcError::invalid_params("`defSymbol` must be a string"));
                };
                let table = self.symbols.as_ref().ok_or_else(no_symbols)?;
                table.address_of(name).ok_or_else(|| {
                    RpcError::new(code::SYMBOL_NOT_FOUND, format!("no symbol named {name}"))
                        .with_data(json!({"defSymbol": name}))
                })
            }
        }
    }

    /// `handle` **or** `slot`, exactly one, converted server-side to the handle the engine wants — plus
    /// the layout both the conversion and the reply need.
    ///
    /// `slot` is what `emulator/object_list` reports and `handle` is what a previous
    /// `emulator/object_spawn` returned; a client holds one of two spellings of the same thing and
    /// should not have to convert. Where `layout.pools` resolves, a slot outside the **dynamic** pool is
    /// refused pre-flight — the engine would answer `4` for it anyway, and `unknownSlot` on the player
    /// is a confusing right answer where *this row reaches the dynamic pool only* is a useful one.
    fn objreq_target(&mut self, params: &Value) -> Result<(u16, decoders::ObjectLayout), RpcError> {
        let layout = decoders::derive(self.symbols.as_deref())?;
        let handle = match (params.get("handle"), params.get("slot")) {
            (Some(_), Some(_)) => {
                return Err(RpcError::invalid_params(
                    "exactly one of `handle` (hex string) and `slot` (pool index) — both were given",
                ))
            }
            (None, None) => {
                return Err(RpcError::invalid_params(
                    "exactly one of `handle` (hex string) and `slot` (pool index) is required — this row \
                     names an object, and a request that names none is not a smaller request",
                ))
            }
            (Some(h), None) => {
                let raw = hex::parse_addr("handle", h)?;
                if raw > 0xFFFF {
                    return Err(RpcError::invalid_params(format!(
                        "`handle` {} is wider than 16 bits — a handle is the LOW WORD of an object's \
                         `addr`, not the whole address",
                        hex::addr(raw)
                    ))
                    .with_data(json!({"handle": hex::addr(raw)})));
                }
                raw as u16
            }
            (None, Some(s)) => {
                let slot = hex::parse_count("slot", s, 0, u64::from(u32::MAX))?;
                if slot >= u64::from(layout.slot_count()) {
                    // The ⚙ group's rule (4): refused with the bound, never clamped.
                    return Err(RpcError::invalid_params(format!(
                        "`slot` {slot} is past this layout's object pool"
                    ))
                    .with_data(json!({"slot": slot, "slotCount": layout.slot_count()})));
                }
                (layout.slot_addr(slot as u32) & 0xFFFF) as u16
            }
        };
        // Two different faults, and they must not share a message. A handle that does not land on a
        // record boundary at all can never name a slot in any pool; a handle that does, but lands
        // outside the dynamic pool, is the player/system/effect case §11.32 gives the cheap pre-flight.
        // The engine answers `4` for both, which is the right answer and a confusing one.
        let seated = objreq_slot_of(&layout, objreq_handle_addr(&layout, handle));
        let Some(slot) = seated else {
            return Err(RpcError::invalid_params(format!(
                "{} does not land on a record boundary of this layout, so it cannot be any object's \
                 handle. A handle is the LOW WORD of a record's `addr`, and records sit at the layout's \
                 own stride from its base.",
                hex::u16_hex(handle)
            ))
            .with_data(json!({
                "handle": hex::u16_hex(handle),
                "baseAddr": hex::addr(layout.slot_addr(0)),
                "slotBytes": layout.slot_bytes(),
            })));
        };
        if let Some(pool) = layout.pool("dynamic") {
            if slot < pool.first_slot || slot >= pool.first_slot.saturating_add(pool.slot_count) {
                return Err(RpcError::invalid_params(format!(
                    "this row reaches the DYNAMIC object pool only, and {} (slot {slot}) does not lie in \
                     it. Moving a player is `Debug_Warp_*`'s job, and the system and effect pools are \
                     the engine's.",
                    hex::u16_hex(handle)
                ))
                .with_data(json!({
                    "handle": hex::u16_hex(handle),
                    "slot": slot,
                    "pool": "dynamic",
                    "firstSlot": pool.first_slot,
                    "slotCount": pool.slot_count,
                })));
            }
        }
        Ok((handle, layout))
    }

    /// One byte read through [`Engine::debug_read`], so it inherits that function's region checks.
    fn read_u8(&self, addr: u32) -> Result<u8, RpcError> {
        let (bytes, _) = self.debug_read(addr, 1)?;
        Ok(bytes[0])
    }

    /// The write half of `emulator/write_memory`, without its param parsing: the work-RAM window check
    /// and a byte-by-byte write through the **real 68000 bus**, so the hardware mirror masking and the
    /// region decode are the machine's rather than ours.
    fn poke(&mut self, addr: u32, data: &[u8]) -> Result<(), RpcError> {
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
        Ok(())
    }

    /// `emulator/get_layer_states` — which display layers are drawn (`protocol.md` §6 line 1136).
    ///
    /// **All four keys, always.** The fragment requires every one, and the reason is §2.3's: a mask a reply
    /// omitted would read exactly like a mask that is off, so absence and `false` must not both be able to
    /// mean the same thing. The keys are emitted from [`mask_targets`] rather than written out here, which
    /// is what keeps them the same four names `emulator/set_layer_enabled` accepts (§11.22).
    ///
    /// A **pure read**: it does not move the timeline, so §6's run-control state rule does not reach it and
    /// there is no `require_paused`.
    fn get_layer_states(&mut self, _params: &Value) -> Result<Value, RpcError> {
        let mut out = Map::new();
        for (name, layer) in mask_targets() {
            out.insert(name.to_string(), json!(self.layers.shows(layer)));
        }
        Ok(Value::Object(out))
    }

    /// `emulator/set_layer_enabled` — show or hide one display layer (`protocol.md` §6 line 1192).
    ///
    /// # It is a display mask, and that is the whole of what it is
    ///
    /// The mask lives on the engine (see [`Engine::layers`]), never in the `System`. So this call cannot
    /// enter `emulator/state_hash` or `emulator/memory_hash`, cannot be undone by `emulator/reset`,
    /// `emulator/reload_rom` or a checkpoint `emulator/restore`, and cannot move a bit the ROM can read —
    /// sprite overflow and sprite collision are latched by the one render that takes no mask. §6's
    /// run-control state rule does not reach it either: it changes the picture, not the machine, so a
    /// free-running client can toggle a layer without pausing.
    ///
    /// # Refusals
    ///
    /// The contract declares **no** error condition on this row — the whole error surface is prose — so an
    /// unknown `layer` is `-32602` in the house spelling `parse_watch_space` established: name the field,
    /// list the accepted set, and carry it as a typed array in `error.data` for a client that branches on
    /// it. `backdrop` is refused by that same path and for the fragment's own reason: it is a
    /// pixel-attribution layer, not a mask target.
    ///
    /// `enabled` is the state the layer is in **after** the call, read back out of the mask rather than
    /// echoed from the request, so a set that did not take could not report that it had.
    fn set_layer_enabled(&mut self, params: &Value) -> Result<Value, RpcError> {
        let (name, layer) = parse_mask_layer(params)?;
        let enabled = parse_mask_enabled(params)?;
        // `parse_mask_layer` only ever yields a layer `mask_key` named, and `mask_key` names exactly the
        // targets `set` accepts — the backdrop is `None` in one and `false` in the other, from the same
        // exhaustive match over `Layer`. So this cannot fail; the assert is here to make that derivation a
        // checked claim rather than a remembered one.
        let applied = self.layers.set(layer, enabled);
        debug_assert!(
            applied,
            "`{name}` came out of mask_targets() but LayerMask::set refused it — the two derivations \
             from Layer have drifted"
        );
        Ok(json!({ "layer": name, "enabled": self.layers.shows(layer) }))
    }

    fn screenshot(&mut self, params: &Value) -> Result<Value, RpcError> {
        let path: PathBuf = match params.get("path") {
            None => std::env::temp_dir().join(format!("oracle-frame-{}.png", self.frame())),
            Some(Value::String(s)) => PathBuf::from(s),
            Some(_) => return Err(RpcError::invalid_params("`path` must be a string")),
        };
        let (width, fb, from_raster) = self.framebuffer(self.layers);
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
        // **Absolutised after the write succeeded**, by the one rule §11.30 puts on every success-reply
        // field whose value is a filesystem path. This key *looked* compliant already, but only by the
        // accident that its default is built from `temp_dir()` — a caller who passed `shot.png` got
        // `shot.png` back and had no way to find the file except by sharing this process's working
        // directory. The refusal above still quotes the caller's spelling: a refusal describes the
        // request, a success describes the state.
        let path = absolutise(&path.display().to_string());
        let mut out = json!({
            "path": path,
            "format": "png",
            "width": width,
            "height": ACTIVE_LINES,
            "bytes": bytes.len(),
            "source": if from_raster { "raster" } else { "stateRender" },
        });
        if !from_raster {
            // The honest caveat is now only true of the fallback — see [`Engine::framebuffer`]. Emitting it
            // unconditionally would be the mirror of the bug it warns about: telling a caller their
            // scanline-accurate capture is not one. A mask has its own text and takes precedence, for the
            // same reason: when one is set it is *the* reason this is post-hoc.
            out["caveat"] = json!(self.mask_caveat("capture").unwrap_or_else(|| {
                "no whole frame has been drawn yet, so this is rendered from the VDP state as it stands \
                 right now. Mid-frame CRAM/scroll changes that a real raster would show on different \
                 lines are NOT reproduced — run at least one frame for a scanline-accurate capture."
                    .to_string()
            }));
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
        // **Absolutised only after every refusal above has been passed** — the same placement, and the
        // same reason, as `reload_rom`'s: a refusal describes the *request*, so `error.data.path` quotes
        // the caller's own spelling back at them (§11.30's deliberate exception), while a success
        // describes the *state*, which §6 wants absolute.
        //
        // **One value, used for both the stored `symbolsPath` and the reply's `path`** — §11.30 M1:
        // "one method never reports one file under two spellings in one exchange." The store still goes
        // through [`Engine::set_symbols`], which absolutises again; that is deliberate rather than
        // redundant-and-tolerated. `absolutise` is idempotent on its own output (`canonicalize` of an
        // already-canonical path is itself; a pass-through label stays a pass-through label), so the two
        // agree by construction — and the boundary keeps holding for the routes that do NOT come through
        // here, which is the property that made `set_rom_path` the right shape.
        let path = absolutise(path);
        self.set_symbols(Some(table), Some(path.clone()));
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
        // Absolutised only after the read succeeded, so the *refusal* above still quotes the caller's own
        // spelling back at them — a client debugging a bad path wants to see what it sent. §6 wants the
        // absolute path for the image that is actually loaded, which is what everything below reports.
        let path = absolutise(&path);
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
        // **`symbols_path` is restored raw, and that is not a hole in §11.30's rule.** Both path fields
        // come back from the slot exactly as they went in, and they went in already resolved: every
        // store of either goes through [`Engine::set_symbols`] / [`Engine::set_rom_path`] or through
        // `reload_rom`, all of which absolutise at the load boundary — which is the whole point of
        // resolving there rather than in `status`. Checkpoints live in this process's memory and are
        // never written to disk, so no slot can predate the binary that made it and carry a spelling
        // from an older rule.
        //
        // Re-absolutising here would also be actively wrong for the pass-through case: a label with no
        // file behind it ("testrom") could acquire a manufactured path if something happened to create
        // a file of that name between the capture and the restore, which would change what a restore
        // reports about a machine it did not change.
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

    // ------------------------------------------------------- breakpoints (§6, §11.21 — CR-BP)
    //
    // **None of the four is subject to §6's run-control state rule, and §6 says so in as many words:**
    // *"arming, toggling and clearing mutate an observer, not the timeline, and are legal while running."*
    // That is not a convenience — the whole `resume` → arm → `wait_for_break` idiom depends on it, and
    // forcing a client to pause first would make the server change machine state on the caller's behalf,
    // which §5 forbids.
    //
    // **Identity is the handle, never the address** (§11.21). The amendment was raised on a measured harm:
    // an agent cleared seven breakpoints it judged "not mine", one of them at 1,691,410 hits. So a second
    // add at an occupied address is a second breakpoint, `clear` takes a handle or `all`, and there is no
    // clear-by-address anywhere on this surface.

    fn breakpoint_add(&mut self, params: &Value) -> Result<Value, RpcError> {
        let addr = self.resolve_exclusive_target(params)?;
        let enabled = match params.get("enabled") {
            None | Some(Value::Null) => true,
            Some(Value::Bool(b)) => *b,
            Some(other) => {
                return Err(RpcError::invalid_params(format!(
                    "`enabled` must be a boolean (D9 category 2) — got {}",
                    hex::kind_of(other)
                )))
            }
        };
        let label =
            match params.get("label") {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.clone(),
                Some(_) => return Err(RpcError::invalid_params(
                    "`label` must be a string (it is carried back verbatim and never interpreted)",
                )),
            };
        // **The cap, checked last**, on `watchpoint_add`'s reasoning: a request that is *also* malformed is
        // told about the malformation rather than about a cap it would not have reached. §11.21 makes the
        // refusal normative — *"MUST fail with -32005 carrying {reason:'breakpointCapReached', cap, count}
        // and MUST NOT silently grow past the advertised number"* — and it is `limits.maxBreakpoints` that
        // a client reads to plan around it.
        let cap = self.config.max_breakpoints;
        let count = self.breakpoints.len();
        if count >= cap {
            return Err(RpcError::invalid_state(
                "breakpointCapReached",
                format!(
                    "all {cap} breakpoint slots are in use; make room first: emulator/breakpoint_clear"
                ),
                json!({"cap": cap, "count": count}),
            ));
        }

        let id = self.breakpoints.add(addr, enabled, label.clone());
        let mut out = Map::new();
        out.insert("breakpoint".into(), json!(breakpoint_wire_id(id)));
        // The RESOLVED address — *"the answer, not merely an echo, when the request named a symbol"*.
        out.insert("addr".into(), json!(hex::addr(addr)));
        if let Some((name, disp)) = self.symbol_at(addr) {
            out.insert("symbol".into(), json!(name));
            out.insert("symbolDisp".into(), json!(disp));
        }
        // The arm state it ACTUALLY got, on `watchpoint_add`'s `op` precedent, so a caller that supplied
        // nothing is told what it got rather than having to know the default.
        out.insert("enabled".into(), json!(enabled));
        if !label.is_empty() {
            out.insert("label".into(), json!(label));
        }
        Ok(Value::Object(out))
    }

    /// **The one writer of `enabled`** (§11.21 design choice 2, audit D-13). `breakpoint_list` has reported
    /// the field since the first row and nothing on this bus could set it.
    ///
    /// **Refuses an unknown handle, deliberately unlike `breakpoint_clear`, which succeeds with
    /// `removed: 0`.** §6 states the asymmetry and its reason in one line: *"a client that thinks it is
    /// toggling something must learn it is toggling nothing"* — a delete that finds nothing has reached its
    /// goal; a toggle that finds nothing has not.
    fn breakpoint_set_enabled(&mut self, params: &Value) -> Result<Value, RpcError> {
        let handle = parse_breakpoint_handle(params, "breakpoint")?;
        // REQUIRED and not defaulted: *"a toggle whose argument may be omitted is a toggle whose caller
        // cannot tell which way it went."*
        let enabled = match params.get("enabled") {
            Some(Value::Bool(b)) => *b,
            None | Some(Value::Null) => return Err(RpcError::invalid_params(
                "`enabled` is required — the state to set. A toggle whose argument may be omitted \
                     is a toggle whose caller cannot tell which way it went",
            )),
            Some(other) => {
                return Err(RpcError::invalid_params(format!(
                    "`enabled` must be a boolean (D9 category 2) — got {}",
                    hex::kind_of(other)
                )))
            }
        };
        let Some(bp) =
            resolve_breakpoint_handle(&handle).and_then(|id| self.breakpoints.get_mut(id))
        else {
            return Err(RpcError::invalid_state(
                "unknownBreakpoint",
                format!(
                    "{handle:?} is not a breakpoint this server holds — it was cleared, or never issued"
                ),
                json!({ "breakpoint": handle }),
            ));
        };
        bp.enabled = enabled;
        // `hits` is carried ACROSS the toggle: disabling does not reset it, and this surface never resets it
        // at all — §6: *"a client wanting a fresh count clears and re-adds."*
        let hits = bp.hits;
        let id = bp.id;
        Ok(json!({
            "breakpoint": breakpoint_wire_id(id),
            "enabled": enabled,
            "hits": hits,
        }))
    }

    fn breakpoint_list(&mut self, params: &Value) -> Result<Value, RpcError> {
        // The cursor is a **breakpoint handle**, resolved to the id it stands for: "resume at the first id
        // strictly greater than this". Ids are monotonic and never reused, so a breakpoint cleared under an
        // outstanding cursor cannot make the next page step over a live one — the positional failure §2.4
        // clause (c) forbids. `watchpoint_list`'s cursor verbatim.
        let cursor = match params.get("cursor") {
            None => None,
            Some(v) => Some(self.parse_breakpoint_cursor(v)?),
        };
        // The default page is the cap: there can never be more live breakpoints than that, so a bigger page
        // could not return more. The 4096 ceiling is the house one.
        let limit = match params.get("limit") {
            None => self.config.max_breakpoints,
            Some(v) => hex::parse_count("limit", v, 1, MAX_PAGE)? as usize,
        };
        let total = self.breakpoints.len();
        let after = cursor.map_or(0, |c| c.0 + 1);
        let skipped = self.breakpoints.iter().filter(|b| b.id.0 < after).count();
        let page: Vec<Value> = self
            .breakpoints
            .iter()
            .filter(|b| b.id.0 >= after)
            .take(limit)
            .map(|b| {
                let mut e = Map::new();
                e.insert("breakpoint".into(), json!(breakpoint_wire_id(b.id)));
                if !b.label.is_empty() {
                    e.insert("label".into(), json!(b.label));
                }
                e.insert("addr".into(), json!(hex::addr(b.addr)));
                if let Some((name, disp)) = self.symbol_at(b.addr) {
                    e.insert("symbol".into(), json!(name));
                    e.insert("symbolDisp".into(), json!(disp));
                }
                e.insert("enabled".into(), json!(b.enabled));
                e.insert("hits".into(), json!(b.hits));
                Value::Object(e)
            })
            .collect();
        let next_cursor = self
            .breakpoints
            .iter()
            .filter(|b| b.id.0 >= after)
            .take(limit)
            .last()
            .map(|b| b.id);

        let bounded = rpc::bounded_array(page, total, skipped, limit);
        let mut out = Map::new();
        // §2.4's **flat** spelling, the same one `watchpoint_list` and `checkpoint_list` use: the list here
        // IS the whole result, so a `boundedList` container would buy one level of indirection and nothing
        // else. `total`/`returned`/`truncated` are required even when the page is complete.
        out.insert("breakpoints".into(), bounded["items"].clone());
        out.insert("total".into(), bounded["total"].clone());
        out.insert("returned".into(), bounded["returned"].clone());
        out.insert("limit".into(), bounded["limit"].clone());
        out.insert("truncated".into(), bounded["truncated"].clone());
        if bounded["truncated"] == json!(true) {
            if let Some(id) = next_cursor {
                out.insert("cursor".into(), json!(breakpoint_wire_id(id)));
            }
        }
        Ok(Value::Object(out))
    }

    /// **IDEMPOTENT**: an unknown handle succeeds with `removed: 0`, per §6.1's rule for `checkpoint_drop`
    /// and §6's breakpoint prose (audit D-15 closed). `removed: 0` is a complete, machine-readable answer to
    /// *"is it gone?"* for a handle that was retired, was never issued, or was never a handle at all.
    ///
    /// **`all: true` removes every breakpoint on the server, other clients' included** — §11.21 design
    /// choice 5: *"the one deliberately shared verb, kept because a session recovering a wedged machine
    /// needs it, and it is why `all` is a separate spelling rather than a wildcard handle."*
    fn breakpoint_clear(&mut self, params: &Value) -> Result<Value, RpcError> {
        let all = match params.get("all") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(RpcError::invalid_params("`all` must be a boolean (D9)")),
        };
        if all {
            if params.get("breakpoint").is_some() {
                return Err(RpcError::invalid_params(
                    "`breakpoint` and `all` are mutually exclusive — pass one",
                ));
            }
            return Ok(json!({"removed": self.breakpoints.clear()}));
        }
        let handle = parse_breakpoint_handle(params, "breakpoint")?;
        let removed = match resolve_breakpoint_handle(&handle) {
            Some(id) => usize::from(self.breakpoints.remove(id)),
            None => 0,
        };
        Ok(json!({ "removed": removed }))
    }

    /// **§6 run-control — `emulator/wait_for_break`**, deprecated by `emulator/stopped` (D6) and retained
    /// for one transition window. `timeoutMs`? → `pc`?, `symbol`?, `symbolDisp`?, `timeoutReached`?,
    /// `waitedMs`?
    ///
    /// **This handler is the POLL, not the wait, and that split is the whole answer to the transport
    /// question.** The engine thread is the only owner of `System` and it serialises: `dispatch` runs to
    /// completion before the next queued call is taken. A handler that slept here would therefore
    /// (a) stall every other client — *including the one that would tell it to stop* — and (b) be
    /// self-defeating, because the same thread's free-run step is what advances the machine, so the
    /// breakpoint being waited for could never fire and the wait could only ever time out. The waiting is
    /// done instead on the **calling connection's own thread**, which is already the thread that blocks in
    /// this architecture (see `server::wait_for_break_delay`); by the time this handler runs, the machine is
    /// either stopped or the deadline has passed. Nothing here sleeps, and nothing here runs the machine.
    ///
    /// **It never resumes a paused machine.** A machine that is not running has already broken, and
    /// starting one on the caller's behalf is precisely the machine-state change §5 forbids a server to
    /// make. So a paused machine answers immediately with its `pc`, which is also what makes
    /// `timeoutMs: 0` — §11.24's *"0 polls once and returns"* — fall out of the same code path.
    ///
    /// **`timeoutMs` and `timeoutReached`, not `maxFrames`/`reached`**: the named D12 exemption, ruled
    /// 2026-08-27, *"the retained, deprecated `emulator/wait_for_break` keeps its legacy spelling …
    /// because D-07 rules that a retained deprecated method preserves the legacy server's behaviour rather
    /// than reinventing it."* The exemption is this method's alone.
    ///
    /// **No `running` key** (D-05, §11.24): it is the machine stamp's and arrives through `replyFields`.
    /// Declaring it twice would invite this handler to think it owns the value.
    fn wait_for_break(&mut self, params: &Value) -> Result<Value, RpcError> {
        // Validated here even though the connection thread has already read it for its own bound: this is
        // the authority, and it is what makes `timeoutMs: 400000` a `-32602` rather than a five-minute
        // sleep. Refused above the ceiling, never clamped.
        let _timeout_ms = match params.get("timeoutMs") {
            None => DEFAULT_WAIT_TIMEOUT_MS,
            Some(v) => hex::parse_count("timeoutMs", v, 0, MAX_WAIT_TIMEOUT_MS)?,
        };
        let mut out = Map::new();
        // [`is_running`](Engine::is_running), i.e. the free-run MODE, not the transient `running` flag a
        // bounded run raises around itself: that one is false at every dispatch boundary, so reading it
        // here would report a halt on a machine that is still going — and it is also exactly the flag the
        // transport polls, so the two halves of this method agree by construction.
        if self.is_running() {
            // Still running at the moment the engine looked, so no halt was observed. §6 makes every
            // handler key optional, so the honest answer is the flag and nothing else: `pc` is deliberately
            // absent — *"absent when the wait timed out with the machine still running"* — because a PC
            // sampled from a machine that is still moving names an instruction that has already gone.
            out.insert("timeoutReached".into(), json!(true));
            return Ok(Value::Object(out));
        }
        let pc = self.sys.cpu_regs().pc;
        out.insert("pc".into(), json!(hex::addr(pc)));
        if let Some((name, disp)) = self.symbol_at(pc) {
            out.insert("symbol".into(), json!(name));
            // §11.24 audit D-08: the displacement lives in this number and never inside the name string.
            out.insert("symbolDisp".into(), json!(disp));
        }
        out.insert("timeoutReached".into(), json!(false));
        Ok(Value::Object(out))
    }

    /// A `breakpoint_list` continuation token, resolved back to the breakpoint id it stands for.
    ///
    /// Accepts a **retired** handle, unlike `watchpoint_hits`' filter: a client paging a list while another
    /// client clears an entry must not have its cursor refused for it. It refuses only a handle this server
    /// could never have issued, which is a typo rather than a race.
    fn parse_breakpoint_cursor(&self, v: &Value) -> Result<BreakpointId, RpcError> {
        let handle = match v {
            Value::String(s) if !s.is_empty() => s.clone(),
            _ => {
                return Err(RpcError::invalid_params(
                    "`cursor` must be the non-empty opaque string this server issued",
                ))
            }
        };
        resolve_breakpoint_handle(&handle)
            .filter(|id| self.breakpoints.was_issued(*id))
            .ok_or_else(|| {
                RpcError::invalid_params(format!(
                    "`cursor`: {handle:?} is not a handle this server issued — pass back the one \
                     emulator/breakpoint_list returned"
                ))
            })
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

/// The `fields` param, shape-checked **before** the layout is derived (§6 ⚙, §11.25).
///
/// Shape first, names second, and the order is deliberate: a malformed `fields` is a client bug that does
/// not depend on which game is loaded, so it should not be masked by `-32012` on a session with no
/// symbols. The *names* still cannot be checked until the layout is known — the catalogue is a property
/// of `layout.engine` — and that check lives in
/// [`ObjectLayout::resolve_fields`](crate::decoders::ObjectLayout::resolve_fields), which is still ahead
/// of any read.
///
/// `None` means the caller did not ask for fields, and the reply then carries no `fields` key at all;
/// `Some(vec![])` means it asked for none, which is an empty map. "Present iff the request asked for
/// fields" is the fragment's own wording, and an empty request is still a request.
fn decoder_fields_param(params: &Value) -> Result<Option<Vec<String>>, RpcError> {
    let Some(v) = params.get("fields") else {
        return Ok(None);
    };
    let Some(arr) = v.as_array() else {
        return Err(RpcError::invalid_params(
            "`fields` must be an array of layout field names",
        ));
    };
    let mut out = Vec::with_capacity(arr.len());
    for e in arr {
        match e.as_str() {
            Some(s) if !s.is_empty() => out.push(s.to_string()),
            _ => {
                return Err(RpcError::invalid_params(
                    "every `fields` entry must be a non-empty string",
                ))
            }
        }
    }
    Ok(Some(out))
}

/// The `includeBytes` param, defaulting to `false`.
///
/// Off by default so the default reply's key set is fully enumerated by the fragment, and because 66
/// records at `$50` is 5,280 bytes on every call for a caller who wanted two coordinates.
fn decoder_include_bytes_param(params: &Value) -> Result<bool, RpcError> {
    match params.get("includeBytes") {
        None => Ok(false),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| RpcError::invalid_params("`includeBytes` must be a boolean (D9)")),
    }
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

/// `emulator/wait_for_break`'s default bound, ms. **The legacy server's measured default**, preserved
/// rather than reinvented — §11.24 audit D-07: *"a retained deprecated method preserves behaviour."*
pub const DEFAULT_WAIT_TIMEOUT_MS: u64 = 30_000;

/// `emulator/wait_for_break`'s ceiling, ms. §11.24 D-07: `≤300000`, **refused above, never clamped**.
pub const MAX_WAIT_TIMEOUT_MS: u64 = 300_000;

/// A watch id **as it goes on the wire**: an opaque string (D9 category 4, §8 item 16).
///
/// The `w` prefix is not decoration. `checkpoint`'s handles are bare decimal strings and §6.1's own
/// commentary concedes that quoting a number "does not *stop* a determined client — the value is still an id
/// in decimal — but it stops the accident". A handle that is not a number at all stops the accident harder,
/// and this surface is where it matters most: the schema types the handle as a string in **five** places,
/// §8 item 16 records this server having shipped a numeric handle once already, and a watch id is precisely
/// the value §6 says cannot be an address or an index — one address may carry several watches, and the same
/// number names four different things across the four spaces.
///
/// **Public so an in-process panel spells a handle exactly once.** `oracle-player`'s Watchpoints tab reads
/// the instrument directly (design §4.4: a 60 Hz body reads a shared derivation) and then has to name a row
/// back to `emulator/watchpoint_clear`. A `format!("w{}", …)` written over there would be a second spelling
/// of this one, agreeing until the day this changes — which is the drift R2 exists to prevent, in the one
/// place where being wrong retires somebody else's watch.
pub fn watch_wire_id(id: WatchId) -> String {
    format!("w{}", id.0)
}

/// A breakpoint id **as it goes on the wire**: an opaque string (D9 category 4, §11.21).
///
/// `b`, for [`watch_wire_id`]'s reason and one of its own. §11.21 pins that the handle *"cannot be an
/// address"* — an address is exactly the spelling the amendment was raised to abolish, because clearing by
/// address is what silently disarmed another client's breakpoint at 1,691,410 hits. A prefixed non-number
/// is what stops a client writing `{"breakpoint": "0x1234"}` and having it work by accident.
///
/// Public for [`watch_wire_id`]'s reason: the Breakpoints tab reads
/// [`Engine::read_breakpoints`] directly and names its rows back to `emulator/breakpoint_set_enabled` and
/// `emulator/breakpoint_clear` through **this** function, never through a second `format!` of its own.
pub fn breakpoint_wire_id(id: BreakpointId) -> String {
    format!("b{}", id.0)
}

/// The inverse of [`breakpoint_wire_id`]. `None` for any string this server could not have spelled.
///
/// Strict, with no bare-number fallback: this handle has never had another spelling, so leniency would buy
/// nothing and would bless the `{"breakpoint": 3}` D9 category 4 forbids.
fn resolve_breakpoint_handle(handle: &str) -> Option<BreakpointId> {
    handle
        .strip_prefix('b')?
        .parse::<u32>()
        .ok()
        .map(BreakpointId)
}

/// The inverse of [`watch_wire_id`]. `None` for any string this server could not have spelled.
///
/// Deliberately strict about the spelling it accepts — no bare-number fallback, unlike [`parse_cursor`]'s
/// migration allowance. This handle has never had another spelling, so leniency here would buy nothing and
/// would quietly bless the `{"watch": 3}` that D9 category 4 exists to forbid.
fn resolve_watch_handle(handle: &str) -> Option<WatchId> {
    handle.strip_prefix('w')?.parse::<u32>().ok().map(WatchId)
}

/// A required opaque-string breakpoint handle. Strict for [`parse_watch_handle`]'s reason: this is the
/// handle a human hand-types into the next call, and typing `{"breakpoint": 3}` *is* the arithmetic on a
/// handle D9 category 4 forbids.
fn parse_breakpoint_handle(params: &Value, field: &str) -> Result<String, RpcError> {
    match params.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(Value::String(_)) => Err(RpcError::invalid_params(format!(
            "`{field}` must be a non-empty string — pass back the handle emulator/breakpoint_add returned"
        ))),
        None | Some(Value::Null) => Err(RpcError::invalid_params(format!(
            "`{field}` (the opaque string handle returned by emulator/breakpoint_add) is required"
        ))),
        Some(other) => Err(RpcError::invalid_params(format!(
            "`{field}` must be a JSON string — the handle is opaque and a client must not compute on it \
             (D9 category 4); got {}",
            hex::kind_of(other)
        ))),
    }
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

/// Parse `emulator/set_layer_enabled`'s `layer` into the wire name it was spelled with and the core
/// [`Layer`] it names.
///
/// **The accepted set is [`mask_targets`]'s, not a literal.** The message that lists it, the value that is
/// matched, and the array in `error.data` all read the same derivation, so a client told "one of X" can
/// always send X back.
///
/// # This refusal could not come from the params closure, and that is not a gap
///
/// [`unknown_params`] is §2.5's closure over top-level param *keys*; `layer` is a declared key whose
/// **value** is out of the fragment's enum, which that check cannot see and is not meant to. So the refusal
/// is here, in the shape [`parse_watch_space`] and [`parse_watch_mode`] already use for the other three
/// enum-valued params on this bus — one house spelling for "not one of these", not a new bespoke path.
fn parse_mask_layer(params: &Value) -> Result<(&'static str, Layer), RpcError> {
    let targets = mask_targets();
    let names: Vec<&'static str> = targets.iter().map(|(n, _)| *n).collect();
    let accepted = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    match params.get("layer") {
        None | Some(Value::Null) => Err(RpcError::invalid_params(format!(
            "`layer` is required — one of {accepted}"
        ))
        .with_data(json!({ "accepted": names }))),
        Some(Value::String(s)) => targets
            .iter()
            .find(|(n, _)| n == s)
            .copied()
            .ok_or_else(|| {
                RpcError::invalid_params(format!("`layer` must be one of {accepted}; got {s:?}"))
                    .with_data(json!({ "layer": s, "accepted": names }))
            }),
        Some(other) => Err(RpcError::invalid_params(format!(
            "`layer` must be a string — one of {accepted}; got {}",
            hex::kind_of(other)
        ))
        .with_data(json!({ "accepted": names }))),
    }
}

/// Parse `emulator/set_layer_enabled`'s `enabled`. **Required**, never defaulted: the fragment requires it,
/// and a missing flag quietly read as `false` would turn a malformed request into a layer disappearing.
fn parse_mask_enabled(params: &Value) -> Result<bool, RpcError> {
    match params.get("enabled") {
        Some(Value::Bool(b)) => Ok(*b),
        None | Some(Value::Null) => Err(RpcError::invalid_params(
            "`enabled` is required — a boolean saying whether the layer is drawn (D9 category 2)",
        )),
        Some(other) => Err(RpcError::invalid_params(format!(
            "`enabled` must be a boolean (D9); got {}",
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

/// **The mask vocabulary, derived — and now derived in the core**, where the player window can read the
/// same one. This is [`LayerMask::targets`] under its original local name; the name is kept because the
/// prose below and four call sites refer to it, and because "the vocabulary this server serves" is worth
/// having a word for even when the derivation has moved down a crate.
///
/// It is the single source for all four places the four names appear on the wire: the
/// `emulator/get_layer_states` reply's key set, `emulator/set_layer_enabled`'s accepted values, the refusal
/// message that lists them, and the caveat that names which layers are hidden. Nothing hand-transcribes the
/// list, and `tests/layers.rs::the_mask_vocabulary_is_the_contract_fragments_own` proves what it produces
/// equals the vendored fragment's enum — in both directions, and against **both** fragments, which is
/// §11.22's "the setter's enum IS the getter's key set" discharged by parse rather than by reading.
///
/// **It moved to `oracle-core` when the player grew layer toggles**, so the window's palette entries and its
/// on-screen "a mask is on" badge read the same four names this server does. A frontend that spelled the
/// layer `planeA` in the palette while the wire said something else would be the item-19 drift class wearing
/// a label's clothes — and the contract test above now pins the *core's* vocabulary as a side effect, which
/// is strictly more coverage than it had.
///
/// The mask vocabulary stays deliberately separate from [`layer_json`]'s even though four of the five names
/// coincide: the mask enum spells the sprite layer `sprites` (plural — the whole layer) where attribution
/// spells one dot's winner `sprite` (singular, with a `spriteIndex`). Two vocabularies that happen to
/// overlap; folding them into one function would make the next name collision a silent wire change.
/// `Layer::mask_key`'s match is **exhaustive on `Layer`**, so neither can fall behind the core's own idea of
/// what a layer is, and `Backdrop` is `None` because it is a pixel-attribution layer only — the floor the
/// fall-through ends at, never something to switch off — exactly as the contract fragment's `$comment` says.
fn mask_targets() -> Vec<(&'static str, Layer)> {
    LayerMask::targets()
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

// ---------------------------------------------------------------------------------------------------
// The shared derivations — one function, two consumers (debug-panels design §4.4 R1)
// ---------------------------------------------------------------------------------------------------
//
// Everything in this block was a private method on [`Engine`] until parcel 2b of the debug window. It is
// free and public now for one structural reason, and the reason is not convenience: an **in-process GUI
// is a consumer of the same registry, not a second server** (contract D15), and a panel that repaints at
// 60 Hz reads direct rather than dispatching. "Reads direct" is only safe if *direct* and *the handler*
// are literally the same code — otherwise the panel and the tool grow two region decodes, two bound
// checks and two mirror folds that agree right up until one of them is edited.
//
// So these are not helpers extracted for tidiness. Each is a place where a second copy would be a
// believable wrong answer: a region label, a refused-versus-clipped decision, and the Z80's
// `$2000-$3FFF` fold. The handlers above call them; `oracle-player`'s Memory panel calls them; there is
// no third implementation to drift.

/// **The bus-space debug read** — straight out of the region, deliberately bypassing the bus.
///
/// Bypassing is the right call for an inspection API (no side effects, no open-bus latch churn, no FIFO),
/// but it means the value can differ from what a CPU read at the same address would return — so the
/// read-shaped replies built on this (`read`, `read_memory`, `read_vram`) each carry a `caveat` saying
/// so. Not every caller wants that caveat: `memory_hash` is also built on this and deliberately carries
/// none — a fingerprint's provenance note lives in its own contract row, not in the reply envelope.
///
/// Returns the bytes and the **region label** the reply reports (`"work RAM"` / `"cartridge ROM"`).
/// Refused, never clipped: a clipped read reports bytes it never looked at.
pub fn debug_read(
    sys: &System,
    addr: u32,
    len: usize,
) -> Result<(Vec<u8>, &'static str), RpcError> {
    let end = (addr as u64) + (len as u64) - 1;
    if (WORK_RAM_LO..=WORK_RAM_HI).contains(&addr) {
        if end > u64::from(WORK_RAM_HI) {
            return Err(out_of_range(
                addr,
                "the read would run past the end of work RAM",
            ));
        }
        let ram = sys.ram();
        let out = (0..len)
            .map(|i| ram[((addr as usize).wrapping_add(i)) & (RAM_SIZE - 1)])
            .collect();
        return Ok((out, "work RAM"));
    }
    let rom = sys.rom();
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

/// **One of the VDP's three internal arrays, read** — `emulator/read`'s non-`bus` branch verbatim.
///
/// `space` must not be [`WatchSpace::Bus`]; that space is [`debug_read`]'s, because it is the only one
/// with a region decode and a symbol. Passing `Bus` here reads VSRAM, which is why the caller above
/// matches on the space rather than letting this function guess — stated rather than asserted, because a
/// silent wrong-array read is exactly the class this block exists to prevent.
pub fn vdp_space_read(
    sys: &System,
    space: WatchSpace,
    addr: u32,
    len: u64,
) -> Result<Vec<u8>, RpcError> {
    let mem: &[u8] = match space {
        WatchSpace::Vram => sys.vram(),
        WatchSpace::Cram => sys.vdp().cram(),
        _ => sys.vdp().vsram(),
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
    Ok(mem[addr as usize..end as usize].to_vec())
}

/// The Z80 window's bounds check, shared by `z80_read`, `z80_write` and the Memory panel so all three
/// refuse the same accesses (§11.28).
///
/// **Bounded at BOTH ends, and the end is the half that was missing.** The legacy server bounded only the
/// start, then looped `addr + i` with no end check — so a multi-byte write near `$3FFF` folded past the
/// window, clobbered `$0000`, and **replied success** (CR-B §5, read at `oracle-old d629771`). Refused
/// **whole, before any byte lands**, never wrapped and never clamped.
///
/// `-32004` rather than `-32602`: §11.28 aligned this with `read`/`memory_hash`/`write_memory`, which
/// carry that code for the identical refusal. `-32602` stays for *shape* refusals — a `value` out of
/// range, two payload spellings — and the two are different failures.
pub fn z80_window(addr: u32, len: usize) -> Result<(), RpcError> {
    let end = u64::from(addr) + len as u64;
    if addr > 0x3FFF || end > 0x4000 {
        return Err(RpcError::new(
            code::ADDRESS_OUT_OF_RANGE,
            format!(
                "the Z80 window is 0x0000-0x3FFF and this access ends at {} — refused whole rather \
                 than wrapped, because a wrapped write lands on 0x0000 and reports success",
                hex::addr(end.min(u64::from(u32::MAX)) as u32)
            ),
        )
        .with_data(json!({"window": {"lo": "0x00000000", "hi": "0x00003FFF"}})));
    }
    Ok(())
}

/// The Z80's own 16 KB window, read, **including the `$2000-$3FFF` mirror fold**.
///
/// The mirror is *the machine, not a defect*: it folds exactly as `z80/bus.rs` folds it, from the same
/// mask, because a second implementation of the mirror would be free to disagree with the one the guest
/// sees — and that disagreement would look like a plausible number, not like a fault.
pub fn z80_read_window(sys: &System, addr: u32, len: usize) -> Result<Vec<u8>, RpcError> {
    z80_window(addr, len)?;
    let ram = sys.z80_ram();
    Ok((0..len)
        .map(|i| ram[(addr as usize + i) & (Z80_RAM_SIZE - 1)])
        .collect())
}

/// The nearest **preceding** symbol for `addr`, as `emulator/status`'s `symbolAtPc` / `symbolDisp` and
/// `emulator/read_memory`'s `symbol` / `symbolDisp` report it.
///
/// Free and public so a window's status strip resolves a PC through the identical call rather than
/// through its own second reading of [`SymbolTable::resolve`]. The two would agree today and the pair
/// `(name, displacement)` is exactly the kind of thing that stops agreeing quietly — `name()` is the
/// *identifying* spelling and a panel reaching for `demangled` instead would show a name that does not
/// round-trip through `lookup_symbol` (§4, rewritten 2026-08-15).
pub fn symbol_at(table: &SymbolTable, addr: u32) -> Option<(String, u32)> {
    let r = table.resolve(addr)?;
    Some((r.name().to_string(), r.displacement))
}

/// Attach `name`/`nameDisp` for a decoded object record, **or neither** — §4's identifying spelling.
///
/// The three ⚙ decoder rows all end an item with this, and so does `oracle-player`'s Objects panel: the
/// composition `code_target()` → [`symbol_at`] is two steps and both of them are places a second copy
/// would be a believable wrong answer. `code_addr` is an **offset** from `ObjCodeBase`, not an address,
/// so a renderer that resolved the raw word would name a symbol near `$0000` and look entirely healthy;
/// and reaching for `demangled` rather than `name()` yields a spelling that does not round-trip through
/// `emulator/lookup_symbol`.
///
/// The pair is omitted — never `""`, never a displacement without a name — when there is no table, when
/// `ObjCodeBase` is absent from the one there is, or when nothing resolves at the target. That is also
/// §11.25's second hardening against the legacy server, which strips a `_Main` suffix and so reports a
/// name that resolves to nothing.
pub fn attach_code_name(
    out: &mut Map<String, Value>,
    table: Option<&SymbolTable>,
    rec: &decoders::DecodedRecord<'_>,
) {
    let (Some(table), Some(target)) = (table, rec.code_target()) else {
        return;
    };
    if let Some((name, disp)) = symbol_at(table, target) {
        out.insert("name".into(), json!(name));
        out.insert("nameDisp".into(), json!(disp));
    }
}

/// §6's `romPath` SHOULD be *the absolute path of the loaded image*. Make it one, or say nothing new.
///
/// **`canonicalize` and no fallback, deliberately.** The obvious alternative — join a relative path onto
/// the current directory whenever it does not start with `/` — is wrong in the one case that matters
/// here: this string is not always a filesystem path. A hosted embedder sets
/// [`crate::host::MachineInfo::rom_path`] to whatever names its image, `"testrom"` included, and there is
/// no file behind it. Prefixing a working directory onto that label would manufacture a path that
/// resolves to nothing and looks authoritative — a *worse* answer than the label, and §6's rule is a
/// SHOULD precisely so that "I cannot honestly say" stays available.
///
/// So: if the string names a file this process can resolve, report the resolved absolute path (symlinks
/// and `..` included — one image, one spelling, so two clients naming the same cartridge agree). If it
/// does not, it was never a path we could speak for, and it is passed through untouched.
///
/// # Why this is `pub` (parcel 2b)
///
/// It was private, and that privacy was the whole of a booked residual: parcel 2a's status strip showed
/// the player's `--rom` argument verbatim while `emulator/status.romPath` came through here, so R1's
/// one-derivation-two-consumers was defeated by a visibility modifier rather than by a decision. §11.30
/// (CR-I) had already ruled that reporting an absolute path is a property of *every* reply field
/// carrying a filesystem path — so the window showing a different string than the bus was a drift the
/// contract does not sanction, however honestly the row was labelled. Publishing this closes it: the
/// panel and `status` now normalise through the same four lines, including the pass-through case, which
/// is the half a re-implementation would be most likely to get wrong (see the paragraph above — the
/// string is not always a path).
pub fn absolutise(path: &str) -> String {
    match std::fs::canonicalize(path) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => path.to_string(),
    }
}

// ---------------------------------------------------------------------------------------------------
// The three object mutation rows' shared pieces (§11.32)
// ---------------------------------------------------------------------------------------------------

/// `maxFrames`' default. **Provisional in the contract and measured here.**
///
/// §11.32 Q3 left the frame counts unmeasured and said so. The measurement this parcel owed is banked in
/// `docs/2026-09-02-cr-spawn-mode.md` §17.2: the consumer sits at the game state's frame top, so a pause
/// anywhere inside that frame's remaining work needs the *next* frame top to reach it. Two, not one,
/// because from a mid-frame pause the first advanced frame may not reach a frame top.
const OBJREQ_DEFAULT_MAX_FRAMES: u64 = 2;

/// What the server writes into the mailbox for one request. Every field is written on every op — the
/// engine ignores the ones its op does not read — so there is no partially-written mailbox to reason
/// about, and a stale cell from a previous request can never be read as this one's.
struct ObjReqRequest {
    op: u8,
    def: u32,
    x: u16,
    y: u16,
    slot: u16,
    place: u16,
}

/// What one exchange yields: the resolved cells (so the caller can read what the engine published), the
/// status byte the cleared flag made valid, and how far the machine moved to get it.
struct ObjReqAck {
    mailbox: objreq::Mailbox,
    status: u8,
    frames_advanced: u64,
}

/// The record address a slot handle names, **in the same spelling `emulator/object_list` reports**.
///
/// The engine turns a handle into an address with `movea.w d1, a0` — a sign extension into
/// `$FFFFxxxx`. This bus is 24 bits wide and the listing writes a RAM address as `$00FFxxxx`, which is
/// what `object_list`'s `addr` carries and what `debug_read` accepts, so the high half is taken from
/// **the layout's own base** rather than from a `$00FF` literal. Same address, one derivation, and the
/// reply joins the decoder rows by string equality rather than by a client's arithmetic.
///
/// This was a live defect before it was a comment: sign-extending to a full `$FFFF9BA6` produced a
/// `-32004` on the server's own read-back, *after* a successful spawn — a machine that had changed and
/// a reply that said it had not.
fn objreq_handle_addr(layout: &decoders::ObjectLayout, handle: u16) -> u32 {
    (layout.slot_addr(0) & 0xFFFF_0000) | u32::from(handle)
}

/// Invert the layout's slot addressing: the pool index for a record address, or `None` where it does not
/// land on one. The server does this so the client never does address arithmetic; where it cannot be
/// answered the key is omitted rather than fabricated (the ⚙ group's rule (3)).
fn objreq_slot_of(layout: &decoders::ObjectLayout, addr: u32) -> Option<u32> {
    let base = layout.slot_addr(0);
    let stride = layout.slot_bytes();
    if stride == 0 {
        return None;
    }
    // Both addresses are compared masked to the work-RAM window, because a listing writes a RAM address
    // as `FFFFxxxx` and the bus decodes it mirrored — the same masking `debug_read` applies.
    let off = (addr & 0x00FF_FFFF).checked_sub(base & 0x00FF_FFFF)?;
    if off % stride != 0 {
        return None;
    }
    let slot = off / stride;
    (slot < layout.slot_count()).then_some(slot)
}

/// `framesAdvanced: 0` on a refusal that carries no count of its own.
///
/// Rule (5) puts `framesAdvanced` on every reply from these rows, success **and** failure, so a caller
/// that is refused still knows where its machine ended up. A refusal raised before any advance really
/// did advance zero frames, and zero is an answer.
///
/// **One refusal on these rows escapes this and it is not the handler's to reach:** §2.5's params
/// closure fires in [`Engine::dispatch`], before any handler runs, so an undeclared param is a
/// `-32602` with `unknownParams` and no `framesAdvanced`. That is one dispatcher shape shared by every
/// row in the catalog, and giving it a per-row key would be worse than the gap; the gap is filed
/// upstream rather than patched here.
fn objreq_frames_default(mut e: RpcError) -> RpcError {
    let mut data = match e.data.take() {
        Some(Value::Object(m)) => m,
        Some(other) => {
            let mut m = Map::new();
            m.insert("detail".into(), other);
            m
        }
        None => Map::new(),
    };
    data.entry("framesAdvanced".to_string()).or_insert(json!(0));
    e.with_data(Value::Object(data))
}

/// A world-pixel coordinate param: an integer, bounded by the engine's own 16-bit position cell, refused
/// and **never clamped** outside it.
///
/// Always required — both rows that take a position require both halves, and `object_delete` names a
/// thing rather than a place and never calls this. An optional spelling would be a default position,
/// which is a value the caller did not choose arriving in the machine.
fn objreq_pixel_param(params: &Value, field: &str) -> Result<u16, RpcError> {
    match params.get(field) {
        None => Err(RpcError::invalid_params(format!(
            "`{field}` is required (world pixels)"
        ))),
        Some(v) => Ok(hex::parse_count(field, v, 0, 0xFFFF)? as u16),
    }
}

/// An optional boolean param, refused rather than coerced when it is not one.
fn objreq_bool_param(params: &Value, field: &str) -> Result<bool, RpcError> {
    match params.get(field) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(RpcError::invalid_params(format!(
            "`{field}` must be a boolean"
        ))),
    }
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

/// The buttons a pad has down, by **the names the wire uses** — `emulator/hold`'s reply `held` array is
/// this function, and `parse_buttons` accepts exactly this vocabulary.
///
/// `pub` since `HELD-PADS-PLAYER`, for the same one-derivation reason [`absolutise`] and [`merge_pads`]
/// are: the player's status strip has to tell a human which buttons a client is holding, and a panel that
/// spelled the eight names for itself would be a second vocabulary that agrees with the handler's until
/// somebody adds a ninth button to one of them.
pub fn held_names(pad: &Pad) -> Vec<&'static str> {
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
