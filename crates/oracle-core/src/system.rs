//! The `System` — the one struct that owns *all* machine state.
//!
//! RAM, the VDP memories (VRAM/CRAM/VSRAM) + registers, and the [`Scheduler`] (which owns the sole
//! master clock and sole RNG). It is plain owned data: `Clone` + bincode `Encode`/`Decode`, so a
//! snapshot is an O(struct) copy with no pointer fixup, and `state_hash` is byte-compatible with Oracle.
//!
//! Chips (the CPUs, the VDP) will be added as fields here and driven through a `Bus` adapter that borrows
//! the relevant fields per step (split-borrow). Memory regions are owned byte buffers, always allocated
//! at their fixed hardware sizes by [`System::new`].

use crate::bus::{BusEventSink, MegaDriveBus, SramMap, StopWhen, Z80_RAM_SIZE};
use crate::m68000::microop::Cpu68000;
use crate::m68000::registers::Registers;
use crate::render::{report_rgb_with_cram, ScanlineScaffold};
use crate::scheduler::{EventKind, Scheduler};
use crate::state_hash::{StateHash, CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};
use crate::vdp::{Vdp, LINES_PER_FRAME, MCLK_PER_LINE};
use crate::ym2612::Ym2612;
use crate::z80::{Z80Bus, Z80};

/// 68000 work RAM, `$FF0000..=$FFFFFF` (64 KiB).
pub const RAM_SIZE: usize = 0x10000;

/// Master-clock ticks per NTSC frame (H32: 262 scanlines × 3420 mclk).
pub const MCLK_PER_FRAME: u64 = 896_040;

/// The video timing standard a set of frame/line stamps was produced under.
///
/// An enum rather than a bare string so a consumer *branches* on it; [`TimingStandard::as_str`] is the
/// wire spelling (`"ntsc"`). Only one variant exists today because the core is NTSC-only — `Pal` joins it
/// when PAL lands, and every consumer that already matches on this keeps compiling and keeps being right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimingStandard {
    /// 60 Hz, 262 lines/frame.
    Ntsc,
}

impl TimingStandard {
    /// The wire/report spelling, lowercase (`"ntsc"`).
    pub fn as_str(self) -> &'static str {
        match self {
            TimingStandard::Ntsc => "ntsc",
        }
    }
}

/// **The basis every emulated `frame` / line stamp in this crate is expressed in** (`F-TRACE-PAL`).
///
/// Carried alongside stamps rather than left implicit: a `frame` index means nothing without the frame
/// length that produced it, and once downstream tooling has cached frame coordinates an unlabeled basis
/// becomes an unfixable ambiguity in *other people's* data. It carries the numbers, not just the label, so
/// a consumer never has to look 896_040 up (and cannot look up the wrong one).
///
/// Both numbers are **derived** from [`MCLK_PER_FRAME`] and [`MCLK_PER_LINE`], the same constants
/// `System`'s frame arithmetic uses (`mclk / MCLK_PER_FRAME`), so the reported basis and the scheduler can
/// never disagree.
///
/// Today this is a constant ([`TimingBasis::NTSC`]) because the machine genuinely is NTSC-only. Read it
/// from [`System::timing_basis`] rather than from the constant: when region becomes machine state the
/// method's *value* goes live while its *signature* does not change, so no consumer breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimingBasis {
    /// The video standard (`ntsc`).
    pub standard: TimingStandard,
    /// Master-clock ticks in one frame — the divisor that turns an `mclk` stamp into a `frame` index.
    pub mclk_per_frame: u64,
    /// Scanlines in one frame, including blanking (262 NTSC).
    pub lines_per_frame: u64,
}

impl TimingBasis {
    /// The NTSC basis: 262 lines × 3420 mclk = 896_040 mclk/frame, derived from the scheduler's own
    /// constants.
    pub const NTSC: TimingBasis = TimingBasis {
        standard: TimingStandard::Ntsc,
        mclk_per_frame: MCLK_PER_FRAME,
        lines_per_frame: MCLK_PER_FRAME / MCLK_PER_LINE,
    };
}

// The reported basis must agree with the VDP's own line/frame geometry — checked at compile time so the
// two spellings of the frame length (this module's and `vdp`'s) can never drift apart silently.
const _: () = assert!(MCLK_PER_FRAME == crate::vdp::MCLK_PER_FRAME);
const _: () = assert!(TimingBasis::NTSC.lines_per_frame == LINES_PER_FRAME);

/// Master-clock ticks per 68000 CPU cycle (the 68000 runs at mclk/7). The **one** place the CPU-cycle →
/// mclk conversion happens is [`System::run_until`]; a `* 7` anywhere else is a bug.
pub const MCLK_PER_CPU_CYCLE: u64 = 7;

/// Master-clock ticks per Z80 cycle (the Z80 runs at mclk/15). The **one** place the Z80-cycle → mclk
/// conversion happens is the Z80 catch-up in [`System::run_until`] (the parallel of the 68000's single ×7
/// site); a `* 15` anywhere else is a bug.
pub const MCLK_PER_Z80_CYCLE: u64 = 15;

/// `export_state` format version (D8). Bumped when the layout changes; Push D freezes v1 + writes the
/// spec. First byte(s) of every `export_state` image. Bumped 1→2 at the SRAM go-live slice (S3): a new
/// 64 KiB SRAM tail region was appended (a genuine layout change — SRAM has no pre-carved reserve, unlike
/// the VDP/Z80 content-fills). See `docs/export-state-v1.md` (§v2 — SRAM region).
pub const EXPORT_STATE_VERSION: u16 = 2;

/// Why a sink-attached run ended. **The two outcomes are never merged into one "success" flag** — the
/// sibling emulator's `ok:true` alongside `timeout_reached:true` is an ambiguous-success defect
/// (`docs/2026-08-14-tooling-frontier-recon.md` §3 item 7), and a caller that cannot tell "my condition
/// happened" from "I gave up waiting" will confidently draw the wrong conclusion from the state it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// The sink asked the run to stop ([`BusEventSink::stop_requested`] returned `true`) — the condition
    /// was actually observed.
    SinkRequested,
    /// The run reached its time bound with the sink never asking to stop. The condition was **not**
    /// observed; nothing about the machine state may be assumed from this.
    DeadlineReached,
}

/// Where a sink-attached run stopped, and why.
///
/// Every stamp is **emulated**, never wall-clock (recon §5 C2): two runs of the same ROM with the same input
/// and the same power-on seed produce identical records, so a record is a citable coordinate rather than a
/// timing observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopRecord {
    /// Predicate fired, or bound reached — see [`StopReason`].
    pub reason: StopReason,
    /// The PC of the instruction that has **not** yet executed (the run stops before it commits).
    pub pc: u32,
    /// Emulated frame index at the stop (`mclk / MCLK_PER_FRAME`).
    pub frame: u64,
    /// Absolute emulated master clock at the stop.
    pub mclk: u64,
}

impl StopRecord {
    /// The condition was observed — the run ended early because the sink asked it to.
    pub fn fired(&self) -> bool {
        self.reason == StopReason::SinkRequested
    }

    /// The bound was hit without the condition ever being observed. **Not** a success.
    pub fn timed_out(&self) -> bool {
        self.reason == StopReason::DeadlineReached
    }
}

/// Serialized length of the 68000 register region in `export_state` (little-endian):
/// d0–d7 (8×4) + a0–a6 (7×4) + usp + ssp + pc (3×4) + sr (2) + prefetch (2×2) = 78 bytes.
const EXPORT_M68K_REGS_LEN: usize = 8 * 4 + 7 * 4 + 4 + 4 + 4 + 2 + 2 * 2;
/// Fixed reserved sub-block for the future Z80 register file, immediately after the live Z80 RAM. Zeroed
/// until the Z80 core lands; sized (0x40) with 2× margin over the Z80's ~0x20-byte architectural register
/// set so filling it later is a content change, not a layout change (no version bump).
const EXPORT_Z80_REGS_PLACEHOLDER: usize = 0x40;
/// Fixed all-zero FM-chip (YM2612) placeholder region — the 2-port × 0x100 addressable register-file scale.
/// The full internal FM state exceeds this (and is unreadable over BlastEm's RSP: YM2612 registers are
/// write-only), so a v2 version bump is the expected path when the FM core lands. FM is the second-to-last
/// region, so resizing it churns only the PSG offset.
const EXPORT_FM_PLACEHOLDER: usize = 0x200;
/// Fixed all-zero PSG (SN76489) placeholder region — register + latch scale (4 tone/noise channels + LFSR).
/// PSG is the second-to-last region since the SRAM go-live, so a future resize shifts only the SRAM offset.
const EXPORT_PSG_PLACEHOLDER: usize = 0x10;
/// Fixed 64 KiB (max standard cartridge SRAM) tail region holding the live SRAM contents left-justified and
/// zero-padded (empty when `!sram_present`). Added at the SRAM go-live slice (S3), bumping the version to 2 —
/// SRAM had no pre-carved reserve, so this is a genuine layout change. Fixed size keeps the layout stable
/// regardless of the specific cart's SRAM size; being the tail region, any future resize churns no other
/// offset. Holds only the raw SRAM byte lane — the `$A130F1` enable/write-protect latch, `sram_dirty`, and
/// the base/end/odd map stay bincode-only, and SRAM is deliberately excluded from `state_hash` (Oracle's
/// `OpStateHash` hashes VDP-only). See `docs/export-state-v1.md` (§v2).
const EXPORT_SRAM_LEN: usize = 0x1_0000;

/// The whole machine. One owner of all state.
#[derive(Clone, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct System {
    /// The power-on seed, retained so [`System::reset`] reproduces the exact power-on state.
    seed: u64,
    scheduler: Scheduler,
    /// Cartridge ROM (`$000000–$3FFFFF`). Owned + in `Clone`/bincode for now (correctness before snapshot
    /// cost; the checksum+reattach seam is a free future change per snapshot policy 5). Preserved across
    /// [`System::reset`] — a reset does not erase the cartridge.
    rom: Vec<u8>,
    ram: Vec<u8>,
    /// 8 KiB Z80 RAM, reachable from the 68000 at `$A00000` (nothing executes it in this pivot).
    z80_ram: Vec<u8>,
    /// The VDP (315-5313): owns VRAM/CRAM/VSRAM + the 24 registers (the four Oracle-hashed regions). Moved
    /// out of `System`'s loose fields; `state_hash`/`export_state` read through it. Its byte layout is frozen.
    vdp: Vdp,
    /// The I/O controller block (`$A10003–$A1001F`): data/control registers + injected pad state. In
    /// **neither** frozen currency (an export-v2 candidate, like the SAT cache) — it rides this snapshot but
    /// is not emitted by `export_state`. See `crate::io` and `docs/2026-07-17-io-recon.md`.
    io: crate::io::Io,
    /// The open-bus latch: the last word driven on the 68000 bus, returned by reads of unmapped space.
    last_bus_word: u16,
    /// The Z80 BUSREQ latch (`$A11100` bit0): `true` = 68000 has requested/been granted the bus, `false` =
    /// released (Z80 owns it). Drives the take-bus/release handshake real games spin on (DR-1 Gunstar). Rides
    /// this internal snapshot for determinism but is **not** emitted by `export_state` (a bus-arbitration
    /// scalar like `last_bus_word`, in neither frozen currency). Reset to `false` at power-on via `new`. See
    /// `docs/2026-07-22-z80-busreq-recon.md`.
    z80_busreq: bool,
    /// The Z80 RESET-release latch (`$A11200` bit0), stored positively: `true` = reset released (Z80 runs),
    /// `false` = reset asserted (Z80 held). **Power-on = `false`** — real hardware holds the Z80 in reset
    /// until the 68000 releases it. Together with `z80_busreq` it gates whether the Z80 steps
    /// (`z80_running && !z80_busreq`). Bus-arbitration scalar threaded like `z80_busreq`: in this bincode
    /// snapshot for determinism, **not** in `export_state`. See `docs/2026-07-22-z80-core-design.md` (ZC6).
    z80_running: bool,
    /// The cartridge SRAM-access-enable latch (`$A130F1` bit0): `true` once a game has written bit0 = 1 to
    /// `$A130F1` (SRAM mapped at `$200001+`), `false` after bit0 = 0 (ROM shown). **Power-on = `false`** —
    /// real hardware and the shipping drivers (S3K `sonic3k.asm:293` disables access at boot) power up with
    /// SRAM off. S0 promotes `$A130F1` from a drop-stub to a real latch but adds **no** SRAM buffer, so this
    /// scalar has no consumer yet and no golden ROM writes `$A130F1` → currency-neutral by construction.
    /// A cartridge bus-control scalar exactly like `z80_busreq`: rides this bincode snapshot for determinism,
    /// but is **not** in `export_state` and **not** in `state_hash`. Semantics pinned in
    /// `docs/2026-07-23-sram-design-recon.md` (§"S0 — `$A130F1` semantics").
    sram_enabled: bool,
    /// The cartridge SRAM write-protect latch (`$A130F1` bit1): `true` = SRAM read-only. Convention-pinned
    /// (no in-tree driver exercises it; the Sega mapper convention pairs enable at bit0 with write-protect at
    /// bit1) and latched now so S1's writable buffer can honor it without a second bus change. **Power-on =
    /// `false`**. Cartridge bus-control scalar like `sram_enabled`/`z80_busreq`: in this bincode snapshot for
    /// determinism, **not** in `export_state`/`state_hash`. See `docs/2026-07-23-sram-design-recon.md`.
    sram_write_protect: bool,
    /// SRAM-present flag: `true` once [`System::load_rom`] parsed a valid "RA" header (Fork 1c hybrid — magic
    /// at `$1B0-1`, range `$1B4-B` inside `$200000-$3FFFFF`). **`false` for every golden ROM** (the fixture
    /// has no "RA" field, no golden writes `$A130F1`) → SRAM overlays nothing and `$000000-$3FFFFF` reads and
    /// writes ROM byte-identically → currency-neutral by construction. Rides this bincode snapshot for
    /// determinism, but is **NOT** in `export_state` (the go-live is S3) and **NOT** in `state_hash` (never —
    /// Oracle's `OpStateHash` excludes SRAM). See `docs/2026-07-23-sram-design-recon.md` (§A3, Fork 1).
    sram_present: bool,
    /// Inclusive SRAM window base bus address (header `$1B4-7`). Meaningful only when `sram_present`.
    /// Snapshot-only, like `sram_present`.
    sram_base: u32,
    /// Inclusive SRAM window end bus address (header `$1B8-B`). Meaningful only when `sram_present`.
    /// Snapshot-only.
    sram_end: u32,
    /// SRAM byte-lane parity from header `$1B2` bit3: `true` = odd-byte cart (the default — the chip answers
    /// only odd bus addresses; the unused even parity falls through to ROM), `false` = even-byte. Snapshot-only.
    sram_odd: bool,
    /// The live cartridge SRAM bytes, sized to the detected chip (`(end-base)/2 + 1` for the every-other-byte
    /// wiring); **empty when `!sram_present`**. Real mutable state — rides this bincode snapshot (like
    /// `z80_ram`) so it survives save-states/determinism, but is **NOT** in `export_state` (that go-live is S3)
    /// and **NOT** in `state_hash` (Oracle excludes SRAM). See the design recon (§B5-B7, Fork 5).
    sram: Vec<u8>,
    /// Set on any guest write into visible SRAM; the frontend's persistence throttle (S2) polls it so a `.srm`
    /// is flushed only after a real save, not every frame (`sram_dirty()`/`clear_sram_dirty()` land in S2). A
    /// non-currency scalar (like `z80_frontier_mclk`): in this bincode snapshot for determinism, **not** in
    /// `export_state`/`state_hash`.
    sram_dirty: bool,
    /// Latched `true` the first time the guest writes visible SRAM and **never** cleared by
    /// [`clear_sram_dirty`](System::clear_sram_dirty) — the S4 "this cart actually uses SRAM" signal. With the
    /// header-less `$A130F1`-activity fallback (S4), EVERY cart now gets a non-empty SRAM buffer, so
    /// `sram_present` can no longer tell the frontend "should I make a `.srm`?". This flag can: it stays
    /// `false` unless the game truly stored save data, so a pure-ROM cart (`s4.soundtest.bin`, the fixture)
    /// still produces no file. Non-currency scalar like `sram_dirty` — in this bincode snapshot for
    /// determinism, **not** in `export_state`/`state_hash`. See `docs/2026-07-23-sram-design-recon.md` (S4).
    sram_used: bool,
    /// The 68000. Driven over a [`MegaDriveBus`] in [`System::step_cpu`]; `step()` returns CPU cycles.
    cpu: Cpu68000,
    /// The absolute mclk of the last frame boundary [`System::run_frames`] targeted. Frame deadlines are
    /// absolute (not `now + frame`), so a step that overshoots one frame's deadline by up to one
    /// instruction is absorbed in the next frame — long-run time stays exact. Serialized so the carry
    /// survives snapshot/restore. Reset to 0 at power-on.
    frame_boundary_mclk: u64,
    /// The Z80 sound CPU (register + interrupt state). Driven over a [`Z80Bus`] in the [`System::run_until`]
    /// catch-up; held in reset this slice (Z-skeleton), so it steps zero instructions. Its register region
    /// stays zeroed in `export_state` region 4 until the later Z-live go-live slice.
    z80: Z80,
    /// The absolute mclk up to which the Z80 has been simulated (its next-instruction boundary) — the Z80
    /// frontier the catch-up in [`System::run_until`] chases the 68000's clock with (ZC4). When the Z80 is
    /// gated off (held in reset / bus-granted) it is advanced to `now` each iteration so a later reset-release
    /// carries **zero** backlog (ZC5). Absolute + bincode-serialized (like `frame_boundary_mclk`) so the chase
    /// resumes exactly across snapshot/restore; **not** in `export_state` (a timing scalar). Power-on 0.
    z80_frontier_mclk: u64,
    /// The Z80's 9-bit bank-address register (`$6000`), serial-loaded LSB-first, selecting the 32 KiB 68000
    /// page the Z80's `$8000-$FFFF` window maps to (Plutiedev "Z80 banking"). A bus-arbitration-class scalar
    /// like `z80_busreq`: rides this bincode snapshot for determinism, **not** emitted by `export_state`.
    /// Power-on 0. No committed fixture releases the Z80, so it never changes in any gate.
    z80_bank: u16,
    /// The YM2612 FM chip — its timers (this slice). The status byte a `$A04000`/`$4000` read returns is
    /// derived from it: with the timers live, the SMPS driver's Timer-A overflow poll fires and the sequencer
    /// ticks (the silent-song bug this fixes). Owned like `vdp`; rides this bincode snapshot for determinism,
    /// but is **not** in `export_state` (region 6 stays the all-zero reserve) nor `state_hash`. Power-on
    /// all-zero → status reads `0x00`, byte-identical to the old stub until a timer is programmed. See
    /// `docs/2026-07-22-fm-timer-design.md`.
    fm: Ym2612,
    /// The deferred per-scanline emission scaffolding (`F-SCANLINE-SUBLINE`, decision D-1): the row resolved
    /// at the previous line's `Scanline` event, held until this line's event hands it to the sink.
    ///
    /// **Not machine state, and the type is what guarantees it** — [`ScanlineScaffold`]'s `PartialEq` is
    /// constant true and its `Encode`/`Decode` move zero bytes, so this field is invisible to
    /// `System: PartialEq`, to the bincode checkpoint format, and (being render output) to `state_hash` and
    /// `export_state` alike. It is deliberately **absent from the hand-written [`std::fmt::Debug`]** above,
    /// which mirrors the machine. It lives here rather than in `run_until_with_sink` because it must survive
    /// a run that ends mid-frame (decision D-2); `reset` clears it via the `Self::new` rebuild.
    scanline_scaffold: ScanlineScaffold,
}

impl std::fmt::Debug for System {
    /// Summarize instead of dumping the 64 KiB buffers (keeps assertion failures readable).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("System")
            .field("seed", &format_args!("{:#018X}", self.seed))
            .field("scheduler", &self.scheduler)
            .field("rom", &format_args!("[{} bytes]", self.rom.len()))
            .field("ram", &format_args!("[{} bytes]", self.ram.len()))
            .field("z80_ram", &format_args!("[{} bytes]", self.z80_ram.len()))
            .field(
                "last_bus_word",
                &format_args!("{:#06X}", self.last_bus_word),
            )
            .field("vdp", &self.vdp)
            .field("io", &self.io)
            .field("z80_busreq", &self.z80_busreq)
            .field("z80_running", &self.z80_running)
            .field("sram_enabled", &self.sram_enabled)
            .field("sram_write_protect", &self.sram_write_protect)
            .field("sram_present", &self.sram_present)
            .field("sram_base", &format_args!("{:#08X}", self.sram_base))
            .field("sram_end", &format_args!("{:#08X}", self.sram_end))
            .field("sram_odd", &self.sram_odd)
            .field("sram", &format_args!("[{} bytes]", self.sram.len()))
            .field("sram_dirty", &self.sram_dirty)
            .field("sram_used", &self.sram_used)
            .field("cpu", &self.cpu)
            .field("frame_boundary_mclk", &self.frame_boundary_mclk)
            .field("z80", &self.z80)
            .field("z80_frontier_mclk", &self.z80_frontier_mclk)
            .field("z80_bank", &self.z80_bank)
            .field("fm", &self.fm)
            .field(
                "state_hash.combined",
                &crate::state_hash::hex(self.state_hash().combined),
            )
            .finish()
    }
}

/// The 68000 register file before the power-on reset sequence runs (all zero; the reset recipe fetches
/// SSP/PC from the ROM vector table and primes the prefetch queue).
fn power_on_regs() -> Registers {
    Registers {
        d: [0; 8],
        a: [0; 7],
        usp: 0,
        ssp: 0,
        pc: 0,
        sr: 0,
        prefetch: [0; 2],
    }
}

/// Fill `buf` with deterministic bytes drawn from `rng` (8 bytes per draw, little-endian). Shared with the
/// [`Vdp`] power-on so VRAM is seeded from the same RNG stream, in the same draw order, as before the
/// extraction (keeps the power-on `state_hash` byte-identical).
pub(crate) fn fill_random(rng: &mut crate::rng::SplitMix64, buf: &mut [u8]) {
    let mut i = 0;
    while i < buf.len() {
        let chunk = rng.next_u64().to_le_bytes();
        let n = (buf.len() - i).min(8);
        buf[i..i + n].copy_from_slice(&chunk[..n]);
        i += n;
    }
}

/// Byte count of the SRAM chip for a detected `[base, end]` bus span: an 8-bit Genesis SRAM sits on one
/// byte lane, so it answers every *other* bus byte — `(end - base) / 2 + 1` chip bytes cover the span.
fn sram_byte_len(base: u32, end: u32) -> usize {
    ((end - base) / 2 + 1) as usize
}

/// Header-less SRAM fallback window (S4). Some real battery-save carts carry **no** "RA" header yet still
/// map SRAM through the `$A130F1` access latch — the reference case is `Sonic & Knuckles + Sonic 3` (USA),
/// whose `$1B0` header is blank but which does `move.b #1,($A130F1)` and stores at `$200001+`
/// (`skdisasm/sonic3k.constants.asm:218`, `sonic3k.asm:344/15756`; `CartRAM_Type` at `sonic3k.asm:83` =
/// odd-byte SRAM). When [`parse_sram_header`] finds no "RA", we provision this standard odd-byte page so
/// such carts can save. The map is **inert until `$A130F1` bit0 is set** (the mapping gate is
/// `sram_enabled && in-range && parity`), and no golden ROM writes `$A130F1` → currency stays byte-identical.
const SRAM_FALLBACK_BASE: u32 = 0x20_0001;
/// End of the fallback window: `$20FFFF` = a 64 KiB address page → 32 KiB of odd-byte storage
/// (`sram_byte_len(0x200001, 0x20FFFF) == 0x8000`), matching standard hardware and comfortably covering
/// S3&K's few-KiB usage (`sonic3k.constants.asm:249-267`).
const SRAM_FALLBACK_END: u32 = 0x20_FFFF;

/// Parse the Genesis cartridge SRAM header (Fork 1c). Returns the detected map when the "RA" magic is
/// present at `$1B0-1` **and** the declared range (`$1B4-7` start, `$1B8-B` end, both big-endian) is sane:
/// `start <= end` and the whole span sits inside the standard `$200000-$3FFFFF` SRAM window. The parity is
/// taken from `$1B2` bit3 (1 = odd-byte, the default; 0 = even-byte). An absent magic, an inverted/out-of-
/// range span, or a ROM too short to hold the header all yield `None` (no SRAM → pure ROM, currency-neutral).
/// EEPROM/serial-save carts use a different protocol and are a **named deferral** (design open question 4) —
/// this parser detects only parallel SRAM.
fn parse_sram_header(rom: &[u8]) -> Option<SramMap> {
    // Need bytes through the end-address field at $1B8-B.
    if rom.len() < 0x1BC {
        return None;
    }
    if &rom[0x1B0..0x1B2] != b"RA" {
        return None;
    }
    let odd = (rom[0x1B2] & 0x08) != 0;
    let base = u32::from_be_bytes([rom[0x1B4], rom[0x1B5], rom[0x1B6], rom[0x1B7]]);
    let end = u32::from_be_bytes([rom[0x1B8], rom[0x1B9], rom[0x1BA], rom[0x1BB]]);
    // Sanity: non-inverted, and the whole span inside the standard SRAM window.
    if base > end || base < 0x20_0000 || end > 0x3F_FFFF {
        return None;
    }
    Some(SramMap { base, end, odd })
}

impl System {
    /// Power on a fresh machine. RAM and VRAM are seeded with deterministic pseudo-random bytes from the
    /// single seeded RNG; CRAM/VSRAM/registers start zeroed. The same `seed` always yields identical state.
    pub fn new(seed: u64) -> Self {
        let mut scheduler = Scheduler::new(seed);
        let mut ram = vec![0u8; RAM_SIZE];
        fill_random(scheduler.rng_mut(), &mut ram);
        // VRAM is seeded next from the same RNG (Vdp::power_on draws after the work-RAM fill — the exact
        // pre-extraction order), so the power-on state_hash is byte-identical.
        let vdp = Vdp::power_on(scheduler.rng_mut());
        // Seed the self-rescheduling per-line Scanline chain that drives the VDP's HINT/VINT timing from
        // boot (recon R7/R12). It never advances the clock and (with the interrupt enables off) never raises
        // an interrupt, so it is invisible to the export_state / state_hash currencies.
        scheduler.schedule(0, EventKind::Scanline);
        Self {
            seed,
            scheduler,
            rom: Vec::new(),
            ram,
            z80_ram: vec![0u8; Z80_RAM_SIZE],
            vdp,
            io: crate::io::Io::default(),
            last_bus_word: 0,
            z80_busreq: false,
            z80_running: false,
            sram_enabled: false,
            sram_write_protect: false,
            sram_present: false,
            sram_base: 0,
            sram_end: 0,
            sram_odd: false,
            sram: Vec::new(),
            sram_dirty: false,
            sram_used: false,
            cpu: Cpu68000::new(power_on_regs()),
            frame_boundary_mclk: 0,
            z80: Z80::new(),
            z80_frontier_mclk: 0,
            z80_bank: 0,
            fm: Ym2612::new(),
            scanline_scaffold: ScanlineScaffold::default(),
        }
    }

    /// Restore the deterministic power-on anchor (what the determinism gate resets to), preserving the
    /// cartridge ROM, then drive the real `/RESET` sequence: the CPU reads the initial SSP and PC from the
    /// ROM vector table and primes the prefetch queue, leaving the machine ready to execute from the ROM.
    /// The reset runs at the mclk-0 anchor — its cycles are not added to the master clock.
    ///
    /// The reset's own bus traffic is discarded. To *observe* it — the first accesses in the machine's
    /// life — use [`reset_with_sink`](Self::reset_with_sink) or, better,
    /// [`boot_with_sink`](Self::boot_with_sink).
    pub fn reset(&mut self) {
        self.reset_with_sink(&mut ());
    }

    /// [`reset`](Self::reset) with an instrument attached **for the reset itself** (recon §5, C1).
    ///
    /// The reset recipe's six reads — the SSP/PC vector table at `$0`/`$2`/`$4`/`$6`, then the two
    /// prefetches at the new PC, all FC=6 — are the first bus accesses the machine ever makes. Until this existed they
    /// were unobservable by *any* caller — `reset` hardcoded the null sink — so every instrument in the
    /// tree necessarily attached after the machine had already come up. That is not a missing feature but a
    /// hole under every instrument: a capture armed after boot silently omits the window under
    /// investigation, and an *aggregate* over a mis-armed capture returns a plausible number rather than an
    /// error (this exact gap voided an investigation, see
    /// `docs/2026-07-23-timing-adjudication-oracle.md:3-11`).
    ///
    /// Arming is indivisible with the reset: there is no instant between "the machine is at its power-on
    /// anchor" and "the sink is attached" for a caller to miss. [`is_pristine_power_on`](
    /// Self::is_pristine_power_on) is the caller-visible check that the anchor was where it should be.
    ///
    /// One thing to know when reading such a capture: the reset is not an instruction, so no
    /// [`BusEventSink::on_step_boundary`] precedes it and a PC-attributing sink (a
    /// [`Watchpoints`](crate::watchpoints::Watchpoints)) stamps these accesses with `pc = 0`. That is the
    /// honest answer — no instruction drove them — not a lost PC.
    pub fn reset_with_sink<S: BusEventSink>(&mut self, sink: &mut S) {
        let rom = std::mem::take(&mut self.rom);
        // Battery-backed SRAM survives a soft reset (its contents + the detected map are preserved, exactly
        // like the cartridge ROM); the `$A130F1` enable latch does NOT — real hardware powers up with SRAM
        // access off and the driver re-enables it. `sram_dirty` also clears (it is only a persistence throttle).
        let sram = std::mem::take(&mut self.sram);
        let (present, base, end, odd, used) = (
            self.sram_present,
            self.sram_base,
            self.sram_end,
            self.sram_odd,
            self.sram_used,
        );
        *self = Self::new(self.seed);
        self.rom = rom;
        self.sram = sram;
        self.sram_present = present;
        self.sram_base = base;
        self.sram_end = end;
        self.sram_odd = odd;
        // `sram_used` is a "this cart has ever saved" latch, so it survives a soft reset alongside the SRAM
        // contents (the enable latch, by contrast, powers off — restored above only for the map, not enable).
        self.sram_used = used;
        // The machine is now exactly at its power-on anchor, with the sink already attached — this is the
        // arm point C1 requires, and it is not expressible as "reset, then arm".
        debug_assert!(
            self.is_pristine_power_on(),
            "reset must arm at the pristine power-on anchor"
        );
        self.cpu.assert_reset();
        self.step_cpu(sink); // services reset_pending: runs the power-on reset recipe over the bus
    }

    /// Power on a machine, load `rom`, and reset it with `sink` attached — **one indivisible call**, so
    /// "reset, then arm" is not expressible (recon §5, C1).
    ///
    /// `load_rom` must precede `reset` (the reset recipe reads its vectors out of the cartridge), which is
    /// exactly what makes the hand-rolled three-step dance easy to get wrong; this is the shape every
    /// instrument should boot through.
    pub fn boot_with_sink<S: BusEventSink>(seed: u64, rom: Vec<u8>, sink: &mut S) -> System {
        let mut sys = System::new(seed);
        sys.load_rom(rom);
        sys.reset_with_sink(sink);
        sys
    }

    /// Whether the machine is at its **pristine power-on anchor**: the reset recipe has not run, so no
    /// vector has been fetched and no instruction has executed (C1's "verifiable from the captured state
    /// itself" — the API exposes the check rather than assuming it).
    ///
    /// **Our anchor is all-zero**, not the `PC=0xFFFFFFFF, SP=0xFFFFFFFF, SR=0xFFFF` recorded in
    /// `docs/2026-08-14-tooling-frontier-recon.md` §5 — those values are the *sibling* Oracle's, and were
    /// never our own (`power_on_regs`, this module: every register, the SR and the prefetch queue start at
    /// 0). A caller porting that check across emulators must use this predicate, not those literals. This
    /// resolves the `F-TRACE-POWERON-CHECK` open question the trace-recorder design left unanswered.
    ///
    /// The clock is part of the check because the reset recipe runs at the mclk-0 anchor without advancing
    /// it: a non-zero clock means the machine has run regardless of what the registers say. One honest
    /// limitation: a cartridge whose reset vector is all zeros would leave the registers indistinguishable
    /// from the anchor, so this is "nothing has run", not a cryptographic proof.
    pub fn is_pristine_power_on(&self) -> bool {
        self.scheduler.now() == 0 && self.cpu.regs == power_on_regs()
    }

    /// The video timing basis every emulated `frame` / line stamp this machine produces is expressed in
    /// (`F-TRACE-PAL`). Constant today (the core is NTSC-only); this is the accessor that goes live when
    /// region becomes machine state, without its signature changing.
    pub fn timing_basis(&self) -> TimingBasis {
        TimingBasis::NTSC
    }

    /// Load the cartridge ROM (`$000000–$3FFFFF` on the 68000 bus). Reads past its end are open bus. Parses
    /// the Genesis SRAM header (Fork 1c hybrid): a valid "RA" field (magic + a sane `$200000-$3FFFFF` range)
    /// records the exact map. When there is **no** "RA" header (S4), we fall back to the standard odd-byte
    /// SRAM page (`$200001-$20FFFF` → 32 KiB) rather than leaving SRAM absent, so header-less battery carts
    /// (the reference case `Sonic & Knuckles + Sonic 3`, whose header is blank yet which saves via `$A130F1`)
    /// can still store. The header always wins when present. Either way a buffer is now provisioned for
    /// **every** cart, but it is inert until the game writes `$A130F1` bit0 = 1 (the `sram_index` gate) — and
    /// no golden ROM does, so the fallback is currency-neutral. Persistence is instead gated on
    /// [`sram_used`](System::sram_used) (set only on a real guest write). See the recon (S4, Fork 1c).
    pub fn load_rom(&mut self, rom: Vec<u8>) {
        let m = parse_sram_header(&rom).unwrap_or(SramMap {
            base: SRAM_FALLBACK_BASE,
            end: SRAM_FALLBACK_END,
            odd: true,
        });
        self.sram_present = true;
        self.sram_base = m.base;
        self.sram_end = m.end;
        self.sram_odd = m.odd;
        self.sram = vec![0u8; sram_byte_len(m.base, m.end)];
        self.sram_dirty = false;
        self.sram_used = false;
        self.rom = rom;
    }

    /// Read-only access to the cartridge ROM.
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// The live battery-backed SRAM bytes, for the frontend to persist to a `.srm` file (S2). Since S4 every
    /// cart has a provisioned buffer (the header-less fallback), so this is only empty before a ROM is loaded.
    /// These bytes are the chip's byte lane only — the odd/even bus parity is a mapping concern handled inside
    /// [`load_rom`](Self::load_rom)/the bus, invisible to the file image.
    pub fn sram(&self) -> &[u8] {
        &self.sram
    }

    /// Whether an SRAM map is provisioned for this cart. Since S4 this is `true` for **every** loaded ROM (a
    /// valid "RA" header, else the standard fallback page), so it is no longer the frontend's persistence
    /// signal — use [`sram_used`](System::sram_used) for "did the game actually save?". Still useful to a
    /// probe/tool confirming a buffer exists.
    pub fn sram_present(&self) -> bool {
        self.sram_present
    }

    /// Whether the guest has ever written visible SRAM this session (the S4 latch, never cleared by
    /// [`clear_sram_dirty`](Self::clear_sram_dirty)). The frontend creates/writes a `.srm` **only** when this
    /// is `true`, so the header-less fallback map never fabricates a save file for a cart that never stores
    /// (`s4.soundtest.bin`, the fixture → `false` → no file, no behaviour change). Reset on [`load_rom`], kept
    /// across a soft [`reset`](Self::reset). Snapshot-only; out of `export_state`/`state_hash`.
    pub fn sram_used(&self) -> bool {
        self.sram_used
    }

    /// Whether the game has enabled SRAM access via `$A130F1` bit0 (a harmless additive getter for probes).
    /// Powers on `false`; the driver sets it before touching the save window and clears it after.
    pub fn sram_enabled(&self) -> bool {
        self.sram_enabled
    }

    /// Copy a `.srm` file image into the SRAM buffer on boot (S2). Copies `min(bytes.len(), buffer.len())`
    /// bytes: a too-long file is truncated to the chip size, a too-short file leaves the remaining buffer bytes
    /// untouched (they start zeroed from [`load_rom`](Self::load_rom)). Since S4 a buffer always exists, so
    /// this always loads (the frontend calls it whenever a `.srm` is on disk). Loading a saved image is **not**
    /// a guest write, so neither `sram_dirty` nor `sram_used` is set.
    pub fn load_sram(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.sram.len());
        self.sram[..n].copy_from_slice(&bytes[..n]);
    }

    /// Whether the guest has written SRAM since the last [`clear_sram_dirty`](Self::clear_sram_dirty) (the
    /// persistence throttle: the frontend polls this, writes the `.srm`, then clears it).
    pub fn sram_dirty(&self) -> bool {
        self.sram_dirty
    }

    /// Clear the SRAM dirty flag after the frontend has persisted the buffer to disk (S2). Deliberately does
    /// **not** clear [`sram_used`](System::sram_used) (that is a permanent "this cart saves" signal).
    pub fn clear_sram_dirty(&mut self) {
        self.sram_dirty = false;
    }

    /// Inject 3-button pad state (Player 1 = port 0, Player 2 = port 1). Deterministic injected state — the
    /// core has no host-input path; tests, the future frontend, and the title-screen run all drive input
    /// through here. The next Data-register read the guest performs reflects it (recon IO4).
    pub fn set_pad(&mut self, port: usize, pad: crate::io::Pad) {
        self.io.set_pad(port, pad);
    }

    /// The injected pad state for a port (0 = P1, 1 = P2).
    pub fn pad(&self, port: usize) -> crate::io::Pad {
        self.io.pad(port)
    }

    /// Build a [`MegaDriveBus`] over this machine's memory (split-borrow) for a CPU step. The `sink` consumes
    /// the bus event stream (pass `&mut ()` for none). The real CPU drives this in Push C.
    pub fn mega_bus<'a, S: BusEventSink>(&'a mut self, sink: &'a mut S) -> MegaDriveBus<'a, S> {
        let now = self.scheduler.now();
        // Build the (Copy) SRAM map by value before the split-borrow, so the mutable buffer/dirty borrows
        // are the only ones the bus holds. `None` when no cart declared SRAM (every golden) → no overlay.
        let sram_map = self.sram_present.then_some(SramMap {
            base: self.sram_base,
            end: self.sram_end,
            odd: self.sram_odd,
        });
        let System {
            rom,
            ram,
            z80_ram,
            vdp,
            io,
            last_bus_word,
            z80_busreq,
            z80_running,
            z80_bank,
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
            sram_used,
            fm,
            ..
        } = self;
        MegaDriveBus::new(
            rom,
            ram,
            z80_ram,
            vdp,
            io,
            now,
            last_bus_word,
            z80_busreq,
            z80_running,
            z80_bank,
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
            sram_used,
            sram_map,
            fm,
            sink,
        )
    }

    /// Serialize the entire machine to a bincode snapshot. O(struct) with no pointer fixup.
    pub fn snapshot(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, bincode::config::standard())
            .expect("System is infallibly encodable")
    }

    /// Restore a machine from a snapshot produced by [`System::snapshot`].
    pub fn restore(bytes: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        let (system, _len) = bincode::decode_from_slice(bytes, bincode::config::standard())?;
        Ok(system)
    }

    /// The VDP `state_hash`, byte-compatible with Oracle. Note: 68000 RAM is **not** part of this hash
    /// (Oracle hashes VDP memory + registers only); RAM is still part of the bincode snapshot.
    pub fn state_hash(&self) -> StateHash {
        StateHash::compute(
            self.vdp.vram(),
            self.vdp.cram(),
            self.vdp.vsram(),
            self.vdp.regs(),
        )
    }

    /// The canonical cross-backend differential currency (integration-pivot D8), laid out in a fixed
    /// region order with fixed sizes so the layout never shifts as chips land. Push D freezes v1 + writes
    /// `docs/export-state-v1.md` (the frozen v1 spec + the v2 SRAM go-live). Region order:
    /// version → m68k regs → work RAM → Z80 RAM → Z80 regs → VDP → FM → PSG → SRAM. The Z80 RAM is **live**
    /// (68000-reachable at `$A00000`); every not-yet-emulated chip's register/memory state serializes as a
    /// fixed all-zero reserved region. The trailing SRAM region (v2) holds the live cartridge SRAM contents
    /// left-justified in a fixed 64 KiB block, zero-padded (all-zero when the cart has no SRAM). This is
    /// distinct from [`state_hash`](Self::state_hash) (the frozen Oracle-compatible VDP hash, kept for the
    /// live-Oracle differential — SRAM is deliberately excluded there).
    ///
    /// Instruction-boundary only: `run_frames` leaves the CPU quiesced at an instruction boundary, so this
    /// never captures mid-instruction state.
    pub fn export_state(&self) -> Vec<u8> {
        let total = 2
            + EXPORT_M68K_REGS_LEN
            + RAM_SIZE
            + Z80_RAM_SIZE
            + EXPORT_Z80_REGS_PLACEHOLDER
            + (VRAM_SIZE + CRAM_SIZE + VSRAM_SIZE + REG_COUNT)
            + EXPORT_FM_PLACEHOLDER
            + EXPORT_PSG_PLACEHOLDER
            + EXPORT_SRAM_LEN;
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&EXPORT_STATE_VERSION.to_le_bytes());
        // m68k regs (little-endian): d0–d7, a0–a6, usp, ssp, pc, sr, prefetch[0..2] = 78 bytes.
        let r = &self.cpu.regs;
        for v in r.d {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in r.a {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&r.usp.to_le_bytes());
        out.extend_from_slice(&r.ssp.to_le_bytes());
        out.extend_from_slice(&r.pc.to_le_bytes());
        out.extend_from_slice(&r.sr.to_le_bytes());
        out.extend_from_slice(&r.prefetch[0].to_le_bytes());
        out.extend_from_slice(&r.prefetch[1].to_le_bytes());
        debug_assert_eq!(
            out.len(),
            2 + EXPORT_M68K_REGS_LEN,
            "regs region is 78 bytes"
        );
        out.extend_from_slice(&self.ram);
        // Z80: the live 8 KiB Z80 RAM (68000-reachable at $A00000 — real mutable state that must be in the
        // currency) followed by the register sub-block. Region 4 is now LIVE (Z-live go-live): the 30-byte
        // architectural register file (ZC9 layout), padded to the reserved 0x40. Because every committed
        // fixture holds the Z80 in reset (all-zero reset model), these 30 bytes are all zero and the export
        // golden does not move — a content change at unchanged size, no version bump (docs/export-state-v1.md).
        out.extend_from_slice(&self.z80_ram);
        let z80_regs = self.z80.export_region();
        out.extend_from_slice(&z80_regs);
        out.extend(std::iter::repeat_n(
            0u8,
            EXPORT_Z80_REGS_PLACEHOLDER - z80_regs.len(),
        ));
        // VDP region (now LIVE): the four Oracle-hashed regions at their frozen sizes, in the state_hash
        // order VRAM → CRAM → VSRAM → regs. This fills the previously-zeroed reserve at UNCHANGED size — the
        // designed v1 *content* change, NOT a layout change (no version bump); see docs/export-state-v1.md.
        out.extend_from_slice(self.vdp.vram());
        out.extend_from_slice(self.vdp.cram());
        out.extend_from_slice(self.vdp.vsram());
        out.extend_from_slice(self.vdp.regs());
        // FM / PSG remain fixed all-zero placeholders (they fill when those chips land).
        out.extend(std::iter::repeat_n(0u8, EXPORT_FM_PLACEHOLDER));
        out.extend(std::iter::repeat_n(0u8, EXPORT_PSG_PLACEHOLDER));
        // SRAM (v2 go-live): the live cartridge SRAM contents (empty when !sram_present), left-justified and
        // zero-padded to the fixed 64 KiB region. Only the raw byte lane — the $A130F1 enable/write-protect
        // latch, sram_dirty, and the base/end/odd map stay bincode-only. This region added a slot with no
        // pre-carved reserve → the one deliberate v1→v2 layout bump (see the golden regen in
        // tests/export_state_v1.rs, same commit). Standard carts are <= 64 KiB; the `.min` keeps the region
        // rigidly 0x10000 even for a pathological in-window header that declares a larger span.
        let n = self.sram.len().min(EXPORT_SRAM_LEN);
        out.extend_from_slice(&self.sram[..n]);
        out.extend(std::iter::repeat_n(0u8, EXPORT_SRAM_LEN - n));
        debug_assert_eq!(out.len(), total);
        out
    }

    /// FNV-1a 64-bit hash of [`export_state`](Self::export_state) — the determinism gate's currency. An
    /// independent hasher (does not touch the frozen Oracle `state_hash` FNV layout).
    pub fn export_state_hash(&self) -> u64 {
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        for b in self.export_state() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        h
    }

    /// Read-only access to the 68000 register file (for the differential harness / debuggers).
    pub fn cpu_regs(&self) -> &Registers {
        &self.cpu.regs
    }

    /// Step the CPU exactly one instruction (or exception entry) over the Mega Drive bus, returning the CPU
    /// cycles it took. The differential harness drives this to compare architectural state at instruction
    /// boundaries; it does **not** advance the master clock (the caller owns time).
    pub fn step_instruction(&mut self) -> u32 {
        self.step_cpu(&mut ())
    }

    /// Read-only access to the 68000 work RAM.
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Read-only access to the 8 KiB Z80 sound RAM (introspection — e.g. checking a driver upload landed).
    pub fn z80_ram(&self) -> &[u8] {
        &self.z80_ram
    }

    /// The live Z80 BUSREQ latch (`$A11100` bit0): `true` = the 68000 has requested/been granted the Z80
    /// bus. Read-only introspection (`F-TRACE-EXPOSE-LATCHES`) — the latch is written only by the guest,
    /// through the bus (`bus.rs` `store_byte`, even byte only). Powers on `false`.
    ///
    /// Together with [`z80_running`](Self::z80_running) this is the Z80-window gate the bus applies:
    /// the `$A00000-$A0FFFF` window forwards (and the Z80 is stopped) only while `busreq && running`.
    /// Note the read is a *sample at the moment you call it* — a tool that must classify each bus access
    /// by the latch state **at that access** still has to follow the write stream.
    pub fn z80_busreq(&self) -> bool {
        self.z80_busreq
    }

    /// The live Z80 RESET-release latch (`$A11200` bit0), stored positively: `true` = reset released (the
    /// Z80 runs), `false` = held in reset. Read-only introspection (`F-TRACE-EXPOSE-LATCHES`); the guest
    /// writes it through the bus. **Powers on `false`** — hardware holds the Z80 in reset until the 68000
    /// releases it. Same sampling caveat as [`z80_busreq`](Self::z80_busreq).
    pub fn z80_running(&self) -> bool {
        self.z80_running
    }

    /// Read-only access to the YM2612 timer/status model (introspection / debuggers), chiefly for its
    /// [`addr_latch`](crate::ym2612::Ym2612::addr_latch) — the latch-then-data protocol's currently
    /// latched register number per bank.
    pub fn fm(&self) -> &crate::ym2612::Ym2612 {
        &self.fm
    }

    /// Read-only access to VRAM.
    pub fn vram(&self) -> &[u8] {
        self.vdp.vram()
    }

    /// Mutable access to VRAM (used by the VDP / bus adapter; here it also lets tests perturb state).
    pub fn vram_mut(&mut self) -> &mut [u8] {
        self.vdp.vram_mut()
    }

    /// Read-only access to the VDP (introspection / debuggers).
    pub fn vdp(&self) -> &Vdp {
        &self.vdp
    }

    /// Mutable access to the VDP (tests / the eventual debugger — e.g. setting up interrupt enables).
    pub fn vdp_mut(&mut self) -> &mut Vdp {
        &mut self.vdp
    }

    /// The scheduler (sole master clock + RNG).
    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    /// Mutable scheduler access.
    pub fn scheduler_mut(&mut self) -> &mut Scheduler {
        &mut self.scheduler
    }

    /// Advance the machine by `frames` rendered frames, deterministically, leaving it paused at an
    /// instruction boundary. Frame deadlines are **absolute** — `run_until(frame_boundary + frames ×
    /// MCLK_PER_FRAME)` — so any overshoot from the previous frame is absorbed here and long-run time stays
    /// exact (`run_frames(n)` ≡ n × `run_frames(1)`).
    pub fn run_frames(&mut self, frames: u64) {
        self.run_frames_with_sink(frames, &mut ());
    }

    /// Like [`run_frames`](Self::run_frames), but with a [`BusEventSink`] attached for the whole run so a
    /// consumer (a [`crate::watchpoints::Watchpoints`], a recorder, a decoder) observes every bus access. The
    /// sink is the caller's — `System` never stores it, so it is in neither frozen currency and cannot move a
    /// state hash. Passing `&mut ()` (what `run_frames` does) is the untouched null-sink path.
    /// Returns the [`StopRecord`] for the run: with no stop signal in play this is always
    /// [`StopReason::DeadlineReached`] and the call is exactly the pre-stop-signal behaviour.
    pub fn run_frames_with_sink<S: BusEventSink>(
        &mut self,
        frames: u64,
        sink: &mut S,
    ) -> StopRecord {
        let target = self.frame_boundary_mclk + frames * MCLK_PER_FRAME;
        let record = self.run_until_with_sink(target, sink);
        self.frame_boundary_mclk = match record.reason {
            // Ran the whole budget: the pre-existing rule — absolute deadlines absorb the overshoot.
            StopReason::DeadlineReached => target,
            // Stopped early: the frame grid has only advanced to the last *whole* frame boundary actually
            // crossed, so anchor there. `.max` keeps the anchor monotonic (it can never move backwards, and
            // `now >= frame_boundary_mclk` always holds on entry, so this is belt-and-braces).
            StopReason::SinkRequested => (self.scheduler.now() / MCLK_PER_FRAME * MCLK_PER_FRAME)
                .max(self.frame_boundary_mclk),
        };
        record
    }

    /// **Run until a predicate fires, with a bounded fallback** — the predicate-driven run.
    ///
    /// Steps the machine for at most `max_frames`, calling `predicate(pc, frame)` at every instruction
    /// boundary (`pc` = the instruction about to execute, `frame` = the emulated frame index), and stops at
    /// the first boundary where it returns `true`. The bound is not optional: an unbounded `run_until` that
    /// silently hangs is strictly worse than a hand-tuned frame budget, so a predicate that never fires
    /// degrades to exactly `run_frames(max_frames)`.
    ///
    /// The returned [`StopRecord`] says **which of those two happened** — `record.fired()` vs
    /// `record.timed_out()` — and carries the deterministic emulated stamp (`pc`, `frame`, `mclk`) of the
    /// stopping point. The two outcomes are never conflated into one "success" flag.
    ///
    /// For a condition that depends on bus traffic rather than the PC, write a sink and pass it to
    /// [`run_frames_with_sink`](Self::run_frames_with_sink) — that is the same mechanism; this is the
    /// closure-shaped convenience over it (see [`crate::bus::StopWhen`], and [`crate::bus::Fanout`] to keep
    /// another sink attached at the same time).
    pub fn run_until_stop<F: FnMut(u32, u64) -> bool>(
        &mut self,
        max_frames: u64,
        predicate: F,
    ) -> StopRecord {
        let mut sink = StopWhen::new(predicate);
        self.run_frames_with_sink(max_frames, &mut sink)
    }

    /// Run until the master clock reaches `deadline_mclk`: pop any due scheduler events (Push C slice 5
    /// wires those to the IPL latch), step the CPU, and advance the clock by the step's cost. A CPU step
    /// may overshoot the deadline by up to one instruction (the ratified sync-on-demand model); the
    /// overshoot carries via [`run_frames`](Self::run_frames)'s absolute deadlines.
    ///
    /// **The one and only CPU-cycle → mclk conversion site**: `mclk += cycles × MCLK_PER_CPU_CYCLE`.
    pub fn run_until(&mut self, deadline_mclk: u64) {
        self.run_until_with_sink(deadline_mclk, &mut ());
    }

    /// Like [`run_until`](Self::run_until), but with a [`BusEventSink`] attached for the whole run (see
    /// [`run_frames_with_sink`](Self::run_frames_with_sink)). Immediately before each CPU step it calls
    /// [`BusEventSink::on_step_boundary`] with the PC of the instruction about to execute and the current
    /// frame, so a consumer that attributes accesses to their writing instruction (watchpoints) has that
    /// context; the default `on_step_boundary` is a no-op, so `&mut ()` is byte-for-byte the old hot path.
    ///
    /// **Early stop.** Right after that stamp — and *before* the instruction executes — the loop asks
    /// [`BusEventSink::stop_requested`]; `true` ends the run at that instruction boundary. The machine is
    /// therefore never left mid-instruction: `pc` in the returned [`StopRecord`] is the instruction that has
    /// **not** yet run, and the state is exactly as resumable as a deadline exit. A sink that raises its flag
    /// during a step (from `on_event` / `on_vdp_write`) is honoured at the *next* boundary, i.e. after the
    /// triggering instruction has fully committed.
    ///
    /// With no sink overriding `stop_requested` the added code is `if false { break }`: the instruction
    /// stream, the bus traffic and the clock are unchanged **by construction**, not merely by optimisation.
    pub fn run_until_with_sink<S: BusEventSink>(
        &mut self,
        deadline_mclk: u64,
        sink: &mut S,
    ) -> StopRecord {
        // Arm the VDP write-capture buffer for this run only if the sink wants VDP-internal writes
        // (watchpoints v2). Disarmed, the choke points are byte-for-byte the old hot path — this single query
        // is the whole cost when no VDP watch is attached. Restored to off on return.
        let capture = sink.wants_vdp_writes();
        self.vdp.set_write_capture(capture);
        // Arm the deferred scanline emitter for this run (`F-SCANLINE-SUBLINE`), off the same capability
        // query that already gates the RGB decode. A row retained by a previous run survives into this one
        // (decision D-2) — but only while a scanline-wanting sink is still attached: an unarmed run drops it
        // rather than leave a stale row to be handed to whatever sink shows up next. Unarmed, this is one
        // extra `wants_scanlines()` per run and nothing else.
        if !sink.wants_scanlines() {
            self.scanline_scaffold.clear();
        }
        let mut reason = StopReason::DeadlineReached;
        while self.scheduler.now() < deadline_mclk {
            // Deliver any events whose deadline has arrived (instruction-boundary granularity, consistent
            // with the ratified sync-on-demand model) before stepping — they may raise the pending latches.
            let now = self.scheduler.now();
            while let Some((deadline, kind)) = self.scheduler.pop_due(now) {
                self.deliver_event(deadline, kind, sink);
            }
            // Stamp the instruction about to execute (its PC) + the current frame, so a sink that attributes
            // accesses to their driving instruction (watchpoints) has that context. No-op for `&mut ()`.
            sink.on_step_boundary(self.cpu.regs.pc, self.scheduler.now() / MCLK_PER_FRAME);
            // The stop signal. Asked here — after the stamp, before the instruction commits — so the run
            // always ends on an instruction boundary with `pc` on the not-yet-executed instruction. With the
            // trait default this is `if false`, so the no-predicate path is unchanged by construction.
            if sink.stop_requested() {
                reason = StopReason::SinkRequested;
                break;
            }
            let cycles = self.step_cpu(sink);
            // Drain the VDP writes this step produced (empty unless armed) and deliver each to the sink, paired
            // with the step-boundary PC/frame it just stamped — this is where a DMA write learns the
            // instruction that triggered it. Empty at every instruction boundary (the `dma_pending` precedent).
            if capture {
                for w in self.vdp.take_write_captures() {
                    sink.on_vdp_write(w);
                }
            }
            self.scheduler.advance(cycles as u64 * MCLK_PER_CPU_CYCLE);
            // Catch the Z80 up to the 68000's new `now` (ZC4): the fixed total order is events → 68000 step
            // → Z80 catch-up → IPL. Gated on `z80_running && !z80_busreq`; held in reset this slice, so the
            // catch-up runs zero instructions and only tracks `now`.
            self.catch_up_z80(sink);
            // Re-derive the IPL latch after the step: a taken interrupt's fc=7 /INTAK cleared the VDP's
            // pending latch mid-step (so a delivered VInt does NOT re-fire after RTE), and any enable-bit
            // register write mid-step is picked up here too (recon R12).
            self.cpu.set_ipl(self.vdp.ipl());
        }
        // Disarm — leave the VDP as the run found it (a subsequent null-sink run must stay on the hot path).
        if capture {
            self.vdp.set_write_capture(false);
        }
        let mclk = self.scheduler.now();
        StopRecord {
            reason,
            pc: self.cpu.regs.pc,
            frame: mclk / MCLK_PER_FRAME,
            mclk,
        }
    }

    /// Emit the row retained by the previous line's `Scanline` event, if there is one
    /// (`F-SCANLINE-SUBLINE`, §A(ii)).
    ///
    /// The row is decoded against **its own line-start CRAM snapshot**, not against live CRAM, which is what
    /// makes the one-line lag invisible: `resolve_line` is index-domain and never reads CRAM, so replaying
    /// the retained `PixelResolution` vector through the snapshot reproduces exactly the bytes
    /// `Vdp::report_rgb` produced at line start.
    ///
    /// The journal is **empty in this slice** — slice 3 lands the mechanism and the currency-neutrality
    /// claim, and nothing else; slice 4 fills the journal at the VDP's CRAM choke and splits the decode into
    /// per-landing segments here.
    fn flush_pending_row<S: BusEventSink>(&mut self, sink: &mut S) {
        let Some(row) = self.scanline_scaffold.take() else {
            return;
        };
        debug_assert!(
            row.journal.is_empty(),
            "slice 3 keeps the sub-line journal empty — a landing here would mean the row's bytes moved \
             before the slice that is allowed to move them"
        );
        let rgb = report_rgb_with_cram(&row.cram, &row.report);
        sink.on_scanline(row.report.line, &rgb);
    }

    /// Deliver a fired scheduler event (recon R7/R12). `deadline` is the event's absolute scheduled mclk (its
    /// line start, for the Scanline chain). The **Scanline** event self-reschedules every line, drives the
    /// per-line VDP housekeeping, schedules an `HInt` at the pinned H anchor **unconditionally every line**,
    /// and line 224 schedules the `VInt`. The `HInt` event runs the HINT-counter bookkeeping *at the anchor*
    /// (recon R7 — the decrement/underflow/reload phase is ~79% through the line, so a reg-10 write earlier
    /// in the same line is visible to this line's reload; the S3K/aeon arm-chain idiom depends on this) and
    /// sets the HINT pending latch only on underflow. `VInt` delivery sets the VINT pending latch; the
    /// IPL the CPU sees is always recomputed from `vdp.ipl()` (gated by the enable bits). `FrameEnd` is
    /// housekeeping. The `sink` only matters to a scanline-capture consumer
    /// ([`BusEventSink::wants_scanlines`]); every other sink (including `&mut ()`) leaves this the untouched
    /// hot path.
    ///
    /// The `Scanline` arm also **emits the previous line's row** to an opted-in sink, before anything else it
    /// does (`F-SCANLINE-SUBLINE`) — see [`flush_pending_row`](Self::flush_pending_row).
    fn deliver_event<S: BusEventSink>(&mut self, deadline: u64, kind: EventKind, sink: &mut S) {
        match kind {
            EventKind::Scanline => {
                let line = (deadline / MCLK_PER_LINE) % LINES_PER_FRAME;
                // **Flush first** (`F-SCANLINE-SUBLINE`, §A(ii)). The row resolved at the *previous* line's
                // event is emitted here, decoded against the CRAM that was live at its own line start. The
                // flush must precede everything else in this arm — in particular it must precede line 224's
                // `on_frame_boundary` — or a frame-accumulating sink's buffer is one row short at the
                // boundary and the exact `[Line(0)..Line(223), Boundary(f)]` interleaving breaks. That
                // ordering is a hard requirement of the design, not an implementation detail, and
                // `tests/scanline_capture.rs` pins it.
                self.flush_pending_row(sink);
                // Schedule the HInt anchor event unconditionally every line: the counter bookkeeping
                // itself runs at the anchor (recon R7), not here, so that mid-line reg-10 writes from an
                // HInt handler are visible to this line's reload (the S3K/aeon arm-chain idiom).
                self.scheduler
                    .schedule(deadline + self.vdp.hint_offset(), EventKind::HInt);
                // Render active lines (0..=223) so the sprite overflow/collision status bits + the R10 masking
                // carry evolve during normal runs (games poll them). Currency-safe: the sprite flags/carry are
                // in neither frozen currency, and render output is discarded here — unless the sink opts in
                // (conformance Limitation L1), in which case the already-built report is retained and decoded
                // to RGB at the next line's event. The sink is the caller's; `System` never stores it.
                if line < 224 {
                    let report = self.vdp.render_scanline(line as u16);
                    if sink.wants_scanlines() {
                        // Retain the resolved row + a 128-byte CRAM snapshot instead of decoding it now.
                        // `render_scanline` itself has NOT moved — same instant, same inputs, same sprite
                        // latch commit — so the unarmed hot path and every ROM's timing are untouched; only
                        // the instant the sink is handed the bytes moves, by one line.
                        self.scanline_scaffold.stash(report, self.vdp.cram());
                    }
                }
                if line == 224 {
                    // The frame-structure hook (`F-SCANLINE-CAPTURE`). Active display has just ended, so a
                    // frame-accumulating sink's buffer holds exactly one complete frame right here — see
                    // [`BusEventSink::on_frame_boundary`] for why this instant, and not line 0, is the
                    // boundary. Defaulted to a no-op, so `&mut ()` is byte-for-byte the old hot path.
                    //
                    // The index is derived from this event's own `deadline`, NOT `self.scheduler.now()`: one
                    // `step_cpu` can advance the clock past several frames (a 68k->VRAM DMA is billed as CPU
                    // wait cycles on a single instruction), after which `pop_due` drains the backlog in a
                    // burst at a `now()` already past all of them, and every boundary in the burst would
                    // report the same frame. Load-bearing — see the hook's doc comment.
                    sink.on_frame_boundary(deadline / MCLK_PER_FRAME);
                    let off = self.vdp.vint_offset();
                    self.scheduler.schedule(deadline + off, EventKind::VInt);
                }
                // Deassert the Z80 `/INT` line at the top of a new frame (ZC14): the VDP pulses the Z80 vblank
                // interrupt for the vblank period, so an un-accepted request does not linger across frames.
                if line == 0 {
                    self.z80.set_int_line(false);
                }
                self.scheduler
                    .schedule(deadline + MCLK_PER_LINE, EventKind::Scanline);
            }
            // The HINT-counter decrement/underflow/reload runs HERE, at the pinned H anchor (recon R7) —
            // NOT at line start — so a reg-10 write earlier in this same line lands before the reload.
            // The anchor offset is < MCLK_PER_LINE, so dividing the event's own deadline still recovers
            // the line the anchor belongs to.
            EventKind::HInt => {
                let line = (deadline / MCLK_PER_LINE) % LINES_PER_FRAME;
                if self.vdp.hint_anchor_tick(line as u16) {
                    self.vdp.raise_hint();
                }
            }
            // VInt raises the 68000's vblank IPL *and* asserts the Z80's `/INT` line (ZC14): on the Genesis the
            // same vblank drives both CPUs' vblank interrupts. The Z80 accepts it if its driver has run `EI`.
            EventKind::VInt => {
                self.vdp.raise_vint();
                self.z80.set_int_line(true);
            }
            EventKind::FrameEnd => {}
        }
        // Any delivered event may change the pending latches — re-derive the IPL the CPU sees (recon R12).
        self.cpu.set_ipl(self.vdp.ipl());
    }

    /// Step the 68000 once through a split-borrow [`MegaDriveBus`], returning the CPU cycles it consumed.
    /// The `sink` consumes the bus event stream (pass `&mut ()` for no instrumentation). `self` is
    /// destructured so the CPU field and the memory fields borrow disjointly (the CPU holds no bus).
    pub fn step_cpu<S: BusEventSink>(&mut self, sink: &mut S) -> u32 {
        let now = self.scheduler.now();
        let sram_map = self.sram_present.then_some(SramMap {
            base: self.sram_base,
            end: self.sram_end,
            odd: self.sram_odd,
        });
        let System {
            cpu,
            rom,
            ram,
            z80_ram,
            vdp,
            io,
            last_bus_word,
            z80_busreq,
            z80_running,
            z80_bank,
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
            sram_used,
            fm,
            ..
        } = self;
        let mut bus = MegaDriveBus::new(
            rom,
            ram,
            z80_ram,
            vdp,
            io,
            now,
            last_bus_word,
            z80_busreq,
            z80_running,
            z80_bank,
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
            sram_used,
            sram_map,
            fm,
            sink,
        );
        cpu.step(&mut bus)
    }

    /// Chase the Z80 frontier up to the 68000's current `now` (ZC4/ZC5) — the parallel of the 68000's clock
    /// site, and the **one and only** Z80-cycle → mclk conversion (`t × MCLK_PER_Z80_CYCLE`).
    ///
    /// When the Z80 is gated on (`z80_running && !z80_busreq`) it runs whole instructions until its absolute
    /// frontier reaches or passes `now`, carrying the bounded overshoot forward — the identical
    /// absolute-deadline pattern the 68000 frame loop uses. When gated off (held in reset or bus-granted to
    /// the 68000) the frontier is advanced to `now`, so it runs nothing but never falls behind: a later
    /// reset-release resumes from `now` with **zero** backlog (ZC5). This slice holds the Z80 in reset
    /// (`z80_running == false` in every fixture), so only the gated-off branch is ever taken and
    /// [`Z80::step`] is never reached.
    fn catch_up_z80<S: BusEventSink>(&mut self, sink: &mut S) {
        let now = self.scheduler.now();
        if self.z80_running && !self.z80_busreq {
            let System {
                z80,
                z80_ram,
                rom,
                ram,
                z80_bank,
                z80_frontier_mclk,
                fm,
                vdp,
                ..
            } = self;
            while *z80_frontier_mclk < now {
                // The Z80 reads the FM timer at its own frontier (ZC4/FM7) — behind the 68000's `now`, both
                // absolute on the one timeline. Pass the frontier value at the start of this step as the FM's
                // `now`. The VDP port mirror ($7F04+) reads at the same frontier instant (K2).
                let mut bus = Z80Bus::new(
                    z80_ram,
                    rom,
                    ram,
                    z80_bank,
                    fm,
                    vdp,
                    *z80_frontier_mclk,
                    sink,
                );
                let t = z80.step(&mut bus);
                *z80_frontier_mclk += t as u64 * MCLK_PER_Z80_CYCLE;
            }
        } else {
            // Held in reset / bus-granted: run nothing, but track `now` so reset-release carries no backlog.
            self.z80_frontier_mclk = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{BusEvent, BusOp, Size};

    /// A booted machine: the test ROM loaded and the power-on reset driven (prefetch primed at the ROM's
    /// entry point), ready to run real code.
    fn booted(seed: u64) -> System {
        let mut s = System::new(seed);
        s.load_rom(crate::testrom::build());
        s.reset();
        s
    }

    /// A generous upper bound on how far one CPU step can overshoot a frame deadline, in mclk. The test
    /// ROM's longest instruction is a few dozen CPU cycles; the worst case anywhere (DIV / RESET's
    /// `Internal{124}`) is ~150 CPU cycles ≈ 1,050 mclk, so this covers it with margin.
    const OVERSHOOT_SLACK_MCLK: u64 = 2_000;

    /// `F-TRACE-EXPOSE-LATCHES`: the two arbiter accessors report the latches the bus actually wrote, and
    /// report *different* ones. Every step below moves exactly one latch and pins both, so a body that
    /// returned the other field (or a constant) fails here rather than passing on a lucky agreement — the
    /// two are deliberately driven to opposite values in the last step.
    #[test]
    fn arbiter_latch_accessors_report_what_the_bus_latched() {
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0xA11100);
        // Power-on: BUSREQ released, Z80 held in reset (hardware's power-on state, `System::new`).
        assert!(!s.z80_busreq(), "power-on: BUSREQ not requested");
        assert!(!s.z80_running(), "power-on: Z80 held in reset");

        // $A11200 bit0 = 1 releases the Z80 from reset; it must NOT move BUSREQ.
        s.mega_bus(&mut ()).write8(0xA1_1200, 5, 0x01);
        assert!(s.z80_running(), "reset released after $A11200 = 1");
        assert!(!s.z80_busreq(), "$A11200 did not touch the BUSREQ latch");

        // $A11100 bit0 = 1 requests the bus; now both are set.
        s.mega_bus(&mut ()).write8(0xA1_1100, 5, 0x01);
        assert!(s.z80_busreq(), "BUSREQ granted after $A11100 = 1");
        assert!(s.z80_running(), "reset stays released");

        // Release BUSREQ only: the two latches now hold OPPOSITE values, so an accessor wired to the wrong
        // field is caught.
        s.mega_bus(&mut ()).write8(0xA1_1100, 5, 0x00);
        assert!(!s.z80_busreq(), "BUSREQ released after $A11100 = 0");
        assert!(s.z80_running(), "releasing BUSREQ does not re-assert reset");
    }

    /// `F-TRACE-EXPOSE-LATCHES`: the FM address latch is reachable from the machine, and a 68k-side write
    /// to the `$A04000` window lands in it (the same latch `Ym2612::addr_latch` pins directly).
    #[test]
    fn fm_address_latch_is_reachable_through_the_system() {
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0xA0_4000);
        assert_eq!(s.fm().addr_latch(0), 0x00, "power-on latch is zero");
        // Even port = address latch. $27 is the timer-control register the SMPS driver programs.
        s.mega_bus(&mut ()).write8(0xA0_4000, 5, 0x27);
        assert_eq!(
            s.fm().addr_latch(0),
            0x27,
            "the 68k-side even-port write latched the register number"
        );
        // The odd (data) port writes the register, it does not re-latch.
        s.mega_bus(&mut ()).write8(0xA0_4001, 5, 0x05);
        assert_eq!(
            s.fm().addr_latch(0),
            0x27,
            "a data write leaves the address latch alone"
        );
    }

    #[test]
    fn run_frames_is_deterministic() {
        let mut a = booted(7);
        let mut b = booted(7);
        a.run_frames(10);
        b.run_frames(10);
        assert_eq!(a.export_state_hash(), b.export_state_hash());
    }

    #[test]
    fn injected_pad_state_round_trips_a_snapshot() {
        // The Io block rides the bincode snapshot (it is in neither frozen currency, but must survive
        // snapshot/restore for determinism). Injected pad state comes back byte-identical.
        use crate::io::Pad;
        let mut sys = System::new(1);
        sys.set_pad(
            0,
            Pad {
                start: true,
                up: true,
                ..Default::default()
            },
        );
        sys.set_pad(
            1,
            Pad {
                a: true,
                ..Default::default()
            },
        );
        let restored = System::restore(&sys.snapshot()).unwrap();
        assert_eq!(
            restored.pad(0),
            Pad {
                start: true,
                up: true,
                ..Default::default()
            }
        );
        assert_eq!(
            restored.pad(1),
            Pad {
                a: true,
                ..Default::default()
            }
        );
        assert_eq!(restored, sys, "the whole machine round-trips, Io included");
    }

    #[test]
    fn run_frames_evolves_state() {
        // The real CPU runs the test ROM's RAM-stirring loop, so the export_state (CPU regs + work RAM)
        // changes every frame.
        let mut s = booted(7);
        let before = s.export_state_hash();
        s.run_frames(1);
        assert_ne!(s.export_state_hash(), before);
    }

    #[test]
    fn run_frames_n_equals_n_times_one() {
        // The overshoot-carry invariant: absolute frame deadlines make bulk stepping identical to
        // single-frame stepping, byte-for-byte, on the real CPU.
        let mut bulk = booted(123);
        let mut stepwise = booted(123);
        bulk.run_frames(5);
        for _ in 0..5 {
            stepwise.run_frames(1);
        }
        assert_eq!(bulk.export_state_hash(), stepwise.export_state_hash());
        assert_eq!(bulk, stepwise, "full state matches, not just the hash");
    }

    #[test]
    fn run_frames_tracks_absolute_frame_boundaries_with_bounded_overshoot() {
        let mut s = booted(7);
        s.run_frames(3);
        assert_eq!(
            s.frame_boundary_mclk,
            3 * MCLK_PER_FRAME,
            "the frame boundary is the exact absolute multiple"
        );
        let now = s.scheduler().now();
        let window = 3 * MCLK_PER_FRAME..3 * MCLK_PER_FRAME + OVERSHOOT_SLACK_MCLK;
        assert!(
            window.contains(&now),
            "now ({now}) is at-or-just-past the boundary (one-instruction overshoot)"
        );
    }

    #[test]
    fn overshoot_never_accumulates_across_many_frames() {
        // 100 single-frame steps must leave the frame boundary at exactly 100 frames — overshoot from each
        // frame is absorbed by the next, never drifting.
        let mut s = booted(0xBEEF);
        for _ in 0..100 {
            s.run_frames(1);
        }
        assert_eq!(s.frame_boundary_mclk, 100 * MCLK_PER_FRAME);
        let now = s.scheduler().now();
        let window = 100 * MCLK_PER_FRAME..100 * MCLK_PER_FRAME + OVERSHOOT_SLACK_MCLK;
        assert!(window.contains(&now));
    }

    /// **C1.** The reset recipe's bus traffic — the first accesses in the machine's life — is visible to an
    /// attached sink. Before `reset_with_sink` this stream was unobservable to every possible caller, so
    /// this test is the whole point of the change: with the sink plumbing removed (`step_cpu(&mut ())`) the
    /// capture is empty and the vector-read assertions below fail rather than passing vacuously.
    #[test]
    fn boot_with_sink_captures_the_reset_vector_fetches() {
        let rom = crate::testrom::build();
        let ssp = u32::from_be_bytes([rom[0], rom[1], rom[2], rom[3]]);
        let entry = u32::from_be_bytes([rom[4], rom[5], rom[6], rom[7]]);
        let mut sink: Vec<BusEvent> = Vec::new();
        let s = System::boot_with_sink(0x51, rom, &mut sink);

        // The four vector-table reads: words at $0/$2/$4/$6, in order, all supervisor PROGRAM space (fc 6 —
        // the reset vector is the one vector that is not fc 5 data).
        let vec_reads: Vec<BusEvent> = sink.iter().copied().filter(|e| e.addr < 8).collect();
        assert_eq!(
            vec_reads.len(),
            4,
            "the reset vector fetches must reach the sink (got {sink:?})"
        );
        for (i, e) in vec_reads.iter().enumerate() {
            assert_eq!(e.op, BusOp::Read, "vector read {i}");
            assert_eq!(e.size, Size::Word, "vector read {i}");
            assert_eq!(e.fc, 6, "vector read {i} is supervisor program space");
            assert_eq!(e.addr, i as u32 * 2, "vector read {i} address");
        }
        assert_eq!(
            [
                vec_reads[0].value,
                vec_reads[1].value,
                vec_reads[2].value,
                vec_reads[3].value
            ],
            [ssp >> 16, ssp & 0xFFFF, entry >> 16, entry & 0xFFFF],
            "the captured words are the ROM's SSP and entry vectors"
        );
        // ...and the two prefetches at the new PC, so the capture covers the whole power-on sequence.
        assert!(
            sink.iter()
                .any(|e| e.addr == entry && e.op == BusOp::Read && e.fc == 6),
            "the post-reset prefetch at the entry point is captured too"
        );
        // The capture describes the boot that actually happened: the machine ended up on that vector.
        assert_eq!(s.cpu.regs.pc, entry, "reset primed the PC from the vector");
        assert_eq!(
            s.cpu.regs.ssp, ssp,
            "reset primed A7 (the SSP) from the vector"
        );
    }

    /// **C1's verifiable half.** The arm point is checkable, and our anchor is all-zero — *not* the
    /// sibling emulator's `PC=0xFFFFFFFF, SP=0xFFFFFFFF, SR=0xFFFF` (resolves `F-TRACE-POWERON-CHECK`).
    #[test]
    fn pristine_power_on_is_observable_and_ends_at_the_reset() {
        let mut s = System::new(0x51);
        assert!(s.is_pristine_power_on(), "a fresh machine is pristine");
        assert_eq!(
            (s.cpu.regs.pc, s.cpu.regs.ssp, s.cpu.regs.sr),
            (0, 0, 0),
            "our power-on anchor is all-zero, not the sibling's 0xFFFF.. values"
        );
        s.load_rom(crate::testrom::build());
        assert!(
            s.is_pristine_power_on(),
            "loading a cartridge runs nothing — still pristine, so this is a valid arm point"
        );
        s.reset();
        assert!(
            !s.is_pristine_power_on(),
            "once the vectors are fetched the machine is no longer pristine"
        );
    }

    /// The no-instrumentation path is unchanged: attaching a sink to the reset observes it and nothing more.
    #[test]
    fn reset_with_a_sink_leaves_identical_machine_state() {
        let mut plain = System::new(0x51);
        plain.load_rom(crate::testrom::build());
        plain.reset();

        let mut sink: Vec<BusEvent> = Vec::new();
        let watched = System::boot_with_sink(0x51, crate::testrom::build(), &mut sink);

        assert_eq!(
            plain.state_hash(),
            watched.state_hash(),
            "observing the reset must not change it"
        );
        assert!(!sink.is_empty(), "and the observation is not empty");
    }

    /// `F-TRACE-PAL`: the reported basis is the arithmetic the stamps are actually computed with.
    #[test]
    fn timing_basis_is_the_scheduler_arithmetic() {
        let mut s = booted(0x51);
        let basis = s.timing_basis();
        assert_eq!(basis.standard.as_str(), "ntsc");
        assert_eq!(basis.mclk_per_frame, MCLK_PER_FRAME);
        assert_eq!(basis.lines_per_frame, LINES_PER_FRAME);
        assert_eq!(basis.mclk_per_frame, basis.lines_per_frame * MCLK_PER_LINE);
        // The frame index a caller reads back is `mclk / mclk_per_frame` in this basis, not an opaque count.
        let rec = s.run_until_stop(3, |_pc, frame| frame >= 2);
        assert!(rec.fired());
        assert_eq!(rec.frame, rec.mclk / basis.mclk_per_frame);
    }

    #[test]
    fn step_cpu_records_a_rom_read_event() {
        // The real CPU fetches its opcode from ROM through the MegaDriveBus; the event stream sees it.
        let mut s = booted(0x42);
        let mut sink: Vec<BusEvent> = Vec::new();
        s.step_cpu(&mut sink);
        assert!(
            !sink.is_empty(),
            "a CPU step drives at least one bus access"
        );
    }

    #[test]
    fn scheduled_vint_is_delivered_as_a_level_6_interrupt() {
        use crate::scheduler::EventKind;
        // A level-6 VInt is taken only when the VDP's VINT enable (IE0, reg 1 bit 5) is on AND the CPU mask
        // is below 6 (the ROM's first instruction lowers it to 0). Enable IE0, latch a VInt at mclk 0, and
        // the handler writes its $1234 sentinel to $FF8000 (outside the stirred range).
        let mut s = booted(0x1357);
        s.vdp_mut().control_write(0x8120, 0); // reg 1 = 0x20 → IE0 (VINT enable)
        s.scheduler_mut().schedule(0, EventKind::VInt);
        let idx = (crate::testrom::INT_SENTINEL_ADDR & 0xFFFF) as usize;
        s.run_frames(1);
        assert_eq!(
            &s.ram()[idx..idx + 2],
            &crate::testrom::INT_SENTINEL.to_be_bytes(),
            "the interrupt handler ran and wrote its sentinel"
        );
    }

    #[test]
    fn interrupts_do_not_fire_while_the_enable_bits_are_off() {
        // The auto Scanline chain sets the VDP's pending latches (VInt at line 224, HInt on underflow) every
        // frame, but with the interrupt enables off (power-on) `vdp.ipl()` stays 0, so no interrupt is taken.
        // The sentinel region ($FF8000, never touched by the main loop) stays at its power-on value.
        let mut s = booted(0x2468);
        let idx = (crate::testrom::INT_SENTINEL_ADDR & 0xFFFF) as usize;
        let before = [s.ram()[idx], s.ram()[idx + 1]];
        s.run_frames(2);
        assert_eq!(
            [s.ram()[idx], s.ram()[idx + 1]],
            before,
            "no interrupt fired while the enable bits are off"
        );
        // The latches DID get set (they are just not gated into the IPL) — proving the chain is live.
        assert!(
            s.vdp().vint_pending(),
            "the VInt latch was set by the auto Scanline chain"
        );
    }

    /// The anchor-phase discriminator for the HINT arm-chain (recon R7, TmEE reload-on-event), pinned
    /// through the real scheduler: the counter bookkeeping runs at the H anchor (~79% through the line),
    /// NOT at line start, so a reg-10 rewrite that lands after line L's start but before its anchor is
    /// visible to line L's underflow reload — the S3K/aeon HInt-handler idiom (the handler re-arms reg 10
    /// mid-line). Line-start phasing fails exactly here: it reloads the stale 0 at line L's start before
    /// the write lands, so the pending latch re-fires on every following line instead of staying quiet
    /// for K lines. (The reload-reads-live-reg10 half of the contract is pinned separately at the Vdp
    /// level: `vdp::tests::hint_reg10_write_before_anchor_is_seen_by_this_lines_reload`.)
    #[test]
    fn hint_reg10_rewrite_after_line_start_is_seen_by_that_lines_anchor_reload() {
        let mut s = booted(0x0A26);
        s.vdp_mut().control_write(0x8A00, 0); // reg 10 = 0 → underflow (and reload) every active line
        let anchor = s.vdp().hint_offset(); // in-line mclk offset of the H anchor (~79% of the line)
        const L: u64 = 50; // an active line well below 224; L+K+1 stays inside frame 0
        const K: u16 = 5;
        // Run into line L: past its start, and provably before its anchor even at worst-case overshoot.
        s.run_until(L * MCLK_PER_LINE + 100);
        assert!(
            100 + OVERSHOOT_SLACK_MCLK < anchor,
            "test premise: the CPU stopped before line L's anchor"
        );
        // Clear the latch set by earlier lines' underflows (the enables are off, so it only latched),
        // then re-arm reg 10 = K mid-line — the arm-chain write the anchor phase must make visible.
        s.vdp_mut().acknowledge(4);
        assert!(
            !s.vdp().hint_pending(),
            "latch clear before line L's anchor"
        );
        s.vdp_mut().control_write(0x8A00 | K, 0);
        // Past line L's anchor: the counter was 0 → the underflow fires…
        s.run_until(L * MCLK_PER_LINE + anchor + 300);
        assert!(
            s.vdp().hint_pending(),
            "line L's anchor fired (counter was 0 at the tick)"
        );
        s.vdp_mut().acknowledge(4);
        // …and the reload picked up the freshly-written K, so lines L+1 ..= L+K stay quiet.
        s.run_until((L + u64::from(K)) * MCLK_PER_LINE + anchor + 300);
        assert!(
            !s.vdp().hint_pending(),
            "no HINT for K lines after the mid-line re-arm to K"
        );
        // The (K+1)-th line after L underflows again (reg10 = K → next fire K+1 lines later).
        s.run_until((L + u64::from(K) + 1) * MCLK_PER_LINE + anchor + 300);
        assert!(s.vdp().hint_pending(), "fires again on line L+K+1");
    }

    #[test]
    fn scanline_wiring_evolves_the_sprite_masking_carry_during_a_run() {
        // The Scanline event now calls render_scanline for active lines, so the sprite pipeline's state (here
        // the R10 dot-overflow masking carry) evolves during run_frames. Program nine 4-cell sprites on the
        // last active line (223) — 288 px > the 256-px H32 budget → dot overflow. The carry is NOT cleared by
        // status reads, so committing it on line 223 survives to the end of the frame (robust vs the ROM).
        let mut s = booted(0x1111);
        {
            let v = s.vdp_mut();
            v.control_write(0x8104, 0); // reg 1 = $04 → M5 set: regs 11+ are only writable in mode 5
            v.control_write(0x8F02, 0); // reg 15 = autoinc 2
            v.control_write(0x8510, 0); // reg 5 = 0x10 → SAT base 0x2000
            let base = v.sat_base();
            for i in 0..9u16 {
                let link = if i + 1 < 9 { i + 1 } else { 0 };
                let addr = (base + i as usize * 8) as u16;
                v.control_write((0x01u16 << 14) | (addr & 0x3FFF), 0); // VRAM write, code low = 1
                v.control_write((addr >> 14) & 0x0003, 0); // code high = 0
                v.data_write(223 + 128); // Y = screen 223 (the last active line)
                v.data_write((0x0C << 8) | link); // size 4×1 (32 px), link
                v.data_write(0x0001); // tile 1
                v.data_write(128 + i * 32); // X, stepped across the line
            }
        }
        assert!(
            !s.vdp().sprite_dot_overflow_carry(),
            "the carry is clear before the run"
        );
        s.run_frames(1);
        assert!(
            s.vdp().sprite_dot_overflow_carry(),
            "render_scanline committed the dot-overflow carry on line 223 during the run"
        );
    }

    /// A sink that opts into rows and counts them — the minimum needed to arm the deferred emitter.
    #[derive(Default)]
    struct RowCounter(usize);

    impl BusEventSink for RowCounter {
        fn on_event(&mut self, _event: BusEvent) {}
        fn wants_scanlines(&self) -> bool {
            true
        }
        fn on_scanline(&mut self, _line: u16, _rgb: &[(u8, u8, u8)]) {
            self.0 += 1;
        }
    }

    /// One mclk short of line 100's event: the events for lines 0..=99 have fired, so rows 0..=98 are out
    /// and row 99 is retained. The instant an armed run is allowed to end holding a row is exactly the
    /// instant decision D-1 has to be true at.
    const MID_FRAME_MCLK: u64 = 100 * MCLK_PER_LINE - 1;

    /// **Decision D-1, executed.** A retained row is render scaffolding, not machine state: an armed run
    /// that ends mid-frame while holding a row reaches a machine that is equal — as a whole struct, as a
    /// `state_hash`, as an `export_state`, and byte-for-byte as a bincode checkpoint — to an unarmed run of
    /// the same length. The checkpoint claim is the load-bearing one: zero encoded bytes is what lets a
    /// snapshot taken before this field existed still restore.
    #[test]
    fn a_retained_row_is_invisible_to_the_machine_and_to_the_checkpoint() {
        let mut plain = booted(0x5EED);
        let mut tapped = booted(0x5EED);
        plain.run_until(MID_FRAME_MCLK);
        let mut sink = RowCounter::default();
        tapped.run_until_with_sink(MID_FRAME_MCLK, &mut sink);

        // Non-vacuity: "the retained row is invisible" says nothing unless a row is actually retained.
        assert_eq!(
            tapped.scanline_scaffold.pending_line(),
            Some(99),
            "the armed run ended holding line 99's row — without this the comparisons below are vacuous"
        );
        assert_eq!(sink.0, 99, "and had already emitted rows 0..=98");
        assert_eq!(
            plain.scanline_scaffold.pending_line(),
            None,
            "the unarmed run retained nothing — the two machines really do differ in the field"
        );

        assert_eq!(
            plain, tapped,
            "the WHOLE machine is equal: ScanlineScaffold's PartialEq is constant true (D-1)"
        );
        assert_eq!(plain.state_hash(), tapped.state_hash());
        assert_eq!(plain.export_state(), tapped.export_state());
        let (plain_bytes, tapped_bytes) = (plain.snapshot(), tapped.snapshot());
        assert_eq!(
            plain_bytes, tapped_bytes,
            "the checkpoint is byte-identical: the retained row encodes as ZERO bytes, so the snapshot \
             format is unchanged and a pre-slice snapshot still decodes"
        );
        let restored = System::restore(&tapped_bytes).expect("the snapshot round-trips");
        assert_eq!(
            restored.scanline_scaffold.pending_line(),
            None,
            "a restored machine holds no row — the same state reset leaves"
        );
        assert_eq!(restored, tapped, "and is equal to the machine it came from");
    }

    /// The scaffolding's lifetime rules: it **persists across runs** (decision D-2 — a run that ends
    /// mid-frame must not drop the row, or the frame is silently one row short), it is **dropped by an
    /// unarmed run** (so a stale row can never be handed to a sink that did not resolve it), and it is
    /// **cleared by `reset`**.
    #[test]
    fn a_retained_row_crosses_a_run_boundary_but_not_an_unarmed_run_or_a_reset() {
        let mut s = booted(0x5EED);
        let mut sink = RowCounter::default();
        s.run_until_with_sink(MID_FRAME_MCLK, &mut sink);
        assert_eq!(s.scanline_scaffold.pending_line(), Some(99));
        assert_eq!(sink.0, 99);

        // D-2: the next armed run delivers the row the previous one resolved. The deadline is a whole line
        // further on because the first run overshot line 100's event by up to one instruction — a shorter
        // one would return without the loop body ever running.
        s.run_until_with_sink(101 * MCLK_PER_LINE, &mut sink);
        assert_eq!(
            sink.0, 100,
            "line 100's event flushed the row retained by the previous RUN, not merely the previous line"
        );
        assert_eq!(s.scanline_scaffold.pending_line(), Some(100));

        // `reset` drops it, matching the rule that reset/reload/restore drop the retained frame.
        s.reset();
        assert_eq!(s.scanline_scaffold.pending_line(), None);

        // An unarmed run drops it rather than carrying it to whatever sink attaches next.
        let mut s2 = booted(0x5EED);
        let mut sink2 = RowCounter::default();
        s2.run_until_with_sink(MID_FRAME_MCLK, &mut sink2);
        assert_eq!(s2.scanline_scaffold.pending_line(), Some(99));
        s2.run_until(101 * MCLK_PER_LINE);
        assert_eq!(
            s2.scanline_scaffold.pending_line(),
            None,
            "an unarmed run drops the retained row at arming time"
        );
    }

    #[test]
    fn a_delivered_vint_is_taken_once_and_does_not_refire_after_rte() {
        // The docket test (recon R12): with IE0 enabled, the auto VInt fires once per frame; the counting ISR
        // increments $FF8000 and RTEs. The fc=7 /INTAK during the interrupt clears the pending latch, so it
        // is NOT re-taken after RTE — over 2 frames the counter advances by exactly 2 (a broken deassert
        // would re-fire in a tight loop and blow the count far past 2).
        let mut s = System::new(0x0BAD_F00D);
        s.load_rom(crate::testrom::build_vint_counter());
        s.reset();
        s.vdp_mut().control_write(0x8120, 0); // enable IE0
        let idx = (crate::testrom::INT_SENTINEL_ADDR & 0xFFFF) as usize;
        let before = u16::from_be_bytes([s.ram()[idx], s.ram()[idx + 1]]);
        s.run_frames(2);
        let after = u16::from_be_bytes([s.ram()[idx], s.ram()[idx + 1]]);
        assert_eq!(
            after.wrapping_sub(before),
            2,
            "exactly one VInt taken per frame — no re-fire after RTE"
        );
    }

    #[test]
    fn new_is_deterministic_for_same_seed() {
        let a = System::new(0xC0FFEE);
        let b = System::new(0xC0FFEE);
        assert_eq!(a, b);
        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn clone_preserves_state_hash() {
        let a = System::new(0x1234);
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn power_on_seeds_ram_and_vram() {
        let s = System::new(0xABCD_1234);
        assert!(
            s.ram().iter().any(|&b| b != 0),
            "RAM should be seeded non-zero"
        );
        assert!(
            s.vram().iter().any(|&b| b != 0),
            "VRAM should be seeded non-zero"
        );
    }

    #[test]
    fn different_seeds_yield_different_state() {
        let a = System::new(1);
        let b = System::new(2);
        assert_ne!(a.state_hash().vram, b.state_hash().vram);
        assert_ne!(a.state_hash().combined, b.state_hash().combined);
    }

    #[test]
    fn reset_restores_power_on_state() {
        // reset() now also drives the power-on /RESET sequence (priming the CPU from the ROM vector table),
        // so the deterministic anchor is a freshly-reset machine — compared here against another one.
        let mut s = booted(0x9999);
        let fresh = booted(0x9999);
        s.vram_mut()[0] ^= 0xFF;
        s.vram_mut()[VRAM_SIZE - 1] ^= 0xFF;
        s.run_frames(2);
        assert_ne!(s, fresh);
        s.reset();
        assert_eq!(
            s, fresh,
            "reset returns to the deterministic power-on anchor"
        );
        assert_eq!(s.state_hash(), fresh.state_hash());
    }

    #[test]
    fn mega_bus_reads_and_writes_the_systems_memory() {
        use crate::bus::MD_VERSION;
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x1234);
        let mut sink: Vec<BusEvent> = Vec::new();
        {
            let mut bus = s.mega_bus(&mut sink);
            // A work-RAM write through the map lands in the System's RAM (mirrored window → ram[0]).
            bus.write16(0xFF_0000, 5, 0xABCD);
            assert_eq!(bus.read16(0xFF_0000, 5).0, 0xABCD);
            // The fixed version register is reachable through the same adapter.
            assert_eq!(bus.read8(0xA1_0001, 5).0, MD_VERSION);
        }
        assert_eq!(
            s.ram()[0],
            0xAB,
            "the map write reached System RAM (high byte)"
        );
        assert_eq!(s.ram()[1], 0xCD, "low byte");
    }

    #[test]
    fn sram_control_latch_powers_on_disabled() {
        // Power-on default: SRAM access disabled, not write-protected (ROM shown at $200000+). No golden ROM
        // writes $A130F1, so the latch stays here for every gate → currency-neutral (design §"S0 semantics").
        let s = System::new(0x130F);
        assert!(!s.sram_enabled, "SRAM disabled at power-on");
        assert!(
            !s.sram_write_protect,
            "SRAM not write-protected at power-on"
        );
    }

    /// A synthetic ROM carrying a valid Genesis "RA" SRAM header: magic at `$1B0-1`, parity in `$1B2` bit3,
    /// and the `[base, end]` span in `$1B4-B` (big-endian). Long enough to hold the header + a sane reset
    /// vector, so it also survives `reset()` if a test needs to boot it.
    fn rom_with_sram(base: u32, end: u32, odd: bool) -> Vec<u8> {
        let mut rom = vec![0u8; 0x1000];
        rom[0x1B0] = b'R';
        rom[0x1B1] = b'A';
        // 0xA0 = the "backup RAM present" bits of a real header; bit3 (0x08) = odd-byte lane.
        rom[0x1B2] = 0xA0 | if odd { 0x08 } else { 0x00 };
        rom[0x1B4..0x1B8].copy_from_slice(&base.to_be_bytes());
        rom[0x1B8..0x1BC].copy_from_slice(&end.to_be_bytes());
        rom
    }

    #[test]
    fn parse_sram_header_reads_a_valid_ra_field() {
        let rom = rom_with_sram(0x20_0001, 0x20_3FFF, true);
        let m = parse_sram_header(&rom).expect("a valid RA header parses");
        assert_eq!((m.base, m.end, m.odd), (0x20_0001, 0x20_3FFF, true));
        // The even-byte parity bit round-trips too.
        let ev = parse_sram_header(&rom_with_sram(0x20_0000, 0x20_1FFE, false)).unwrap();
        assert!(!ev.odd, "bit3 clear → even-byte cart");
    }

    #[test]
    fn parse_sram_header_is_none_without_ra_magic() {
        assert!(
            parse_sram_header(&vec![0u8; 0x1000]).is_none(),
            "no magic → no SRAM"
        );
        assert!(
            parse_sram_header(&crate::testrom::build()).is_none(),
            "the fixture ROM has no RA field → currency-neutral"
        );
    }

    #[test]
    fn parse_sram_header_rejects_garbage_and_out_of_range() {
        // Inverted span (base > end).
        assert!(parse_sram_header(&rom_with_sram(0x20_3FFF, 0x20_0001, true)).is_none());
        // Span outside the standard $200000-$3FFFFF window.
        assert!(parse_sram_header(&rom_with_sram(0x10_0000, 0x10_FFFF, false)).is_none());
        // Too short to hold the header (magic would be past the end).
        assert!(parse_sram_header(&vec![0u8; 0x100]).is_none());
    }

    #[test]
    fn load_sram_truncates_and_zero_pads_and_leaves_dirty_untouched() {
        let mut s = System::new(0x5A);
        s.load_rom(rom_with_sram(0x20_0001, 0x20_3FFF, true)); // 0x2000-byte chip
        assert_eq!(s.sram().len(), 0x2000);
        assert!(!s.sram_dirty(), "fresh cart is clean");

        // A too-short image copies its bytes and leaves the rest zeroed; loading is not a guest write.
        s.load_sram(&[0x11, 0x22, 0x33]);
        assert_eq!(&s.sram()[..3], &[0x11, 0x22, 0x33]);
        assert!(s.sram()[3..].iter().all(|&b| b == 0), "tail stays zero");
        assert!(
            !s.sram_dirty(),
            "load_sram is not a guest write → not dirty"
        );

        // A too-long image is truncated to the chip size (no panic, extra bytes discarded).
        let big = vec![0xAAu8; 0x2000 + 16];
        s.load_sram(&big);
        assert_eq!(s.sram().len(), 0x2000);
        assert!(
            s.sram().iter().all(|&b| b == 0xAA),
            "chip filled, no overrun"
        );
        assert!(!s.sram_dirty());
    }

    #[test]
    fn sram_dirty_flag_round_trips_via_public_api() {
        let mut s = System::new(0x5A);
        s.load_rom(rom_with_sram(0x20_0001, 0x20_3FFF, true));
        s.sram_dirty = true; // simulate a guest write having dirtied it
        assert!(s.sram_dirty());
        s.clear_sram_dirty();
        assert!(!s.sram_dirty(), "clear_sram_dirty resets the throttle flag");
    }

    #[test]
    fn sram_used_latches_on_first_write_and_survives_clear_dirty() {
        // S4: sram_used is the permanent "this cart saves" signal. It latches on the first guest SRAM write
        // and, unlike sram_dirty, is NOT cleared by clear_sram_dirty (the debounce reset must not erase it).
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x5A);
        s.load_rom(rom_with_sram(0x20_0001, 0x20_3FFF, true));
        assert!(!s.sram_used(), "fresh cart has not saved");
        s.mega_bus(&mut ()).write8(0xA1_30F1, 5, 0x01); // enable SRAM
        s.mega_bus(&mut ()).write8(0x20_0001, 5, 0x42); // first guest write
        assert!(s.sram_used(), "first write latches sram_used");
        assert!(s.sram_dirty(), "…and dirties");
        s.clear_sram_dirty();
        assert!(
            !s.sram_dirty() && s.sram_used(),
            "clear_dirty keeps sram_used latched"
        );
        // Loading a save image is not a guest write → neither flag moves.
        let mut s2 = System::new(0x5A);
        s2.load_rom(rom_with_sram(0x20_0001, 0x20_3FFF, true));
        s2.load_sram(&[0x99; 4]);
        assert!(
            !s2.sram_used() && !s2.sram_dirty(),
            "load_sram is not a guest write"
        );
    }

    #[test]
    fn no_ra_cart_gets_the_fallback_map_but_stays_inert_until_used() {
        // S4: a no-"RA" cart (the fixture) now gets the standard fallback SRAM page, so a buffer EXISTS — but
        // it is inert (never mapped) until the game enables $A130F1, and `sram_used` stays false so the
        // frontend makes no `.srm`. This is the currency-neutral fixture condition (no golden writes $A130F1).
        let mut s = System::new(0x5A);
        s.load_rom(crate::testrom::build()); // no RA header → fallback page provisioned
        assert!(
            s.sram_present(),
            "fallback map is provisioned for every cart"
        );
        assert_eq!(s.sram_base, SRAM_FALLBACK_BASE);
        assert_eq!(s.sram_end, SRAM_FALLBACK_END);
        assert!(s.sram_odd, "fallback is the odd-byte lane");
        assert_eq!(s.sram().len(), 0x8000, "64 KiB page → 32 KiB odd-byte chip");
        assert!(
            !s.sram_enabled(),
            "not enabled until the game writes $A130F1"
        );
        assert!(
            !s.sram_used(),
            "no guest write yet → the frontend makes no file"
        );
        // load_sram now always copies (a buffer exists); it is not a guest write, so nothing dirties.
        s.load_sram(&[1, 2, 3, 4]);
        assert_eq!(&s.sram()[..4], &[1, 2, 3, 4]);
        assert!(!s.sram_dirty());
        assert!(!s.sram_used());
    }

    #[test]
    fn sram_saves_within_a_session_with_odd_byte_addressing() {
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x5A);
        s.load_rom(rom_with_sram(0x20_0001, 0x20_3FFF, true));
        assert!(s.sram_present, "RA header detected");
        assert_eq!(s.sram_base, 0x20_0001);
        assert_eq!(s.sram_end, 0x20_3FFF);
        // 8 KiB chip on the odd lane: (0x203FFF - 0x200001)/2 + 1.
        assert_eq!(s.sram.len(), 0x2000);

        let mut sink = ();
        // Before enabling, the window reads ROM (open bus past this short ROM's end), never SRAM.
        {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0xA1_30F1, 5, 0x00); // ensure disabled
            bus.write8(0x20_0001, 5, 0x77); // dropped — SRAM not enabled
        }
        assert!(!s.sram_dirty, "a write while disabled does not dirty SRAM");

        // Enable via $A130F1 bit0, then write + read back an odd SRAM cell.
        s.mega_bus(&mut sink).write8(0xA1_30F1, 5, 0x01);
        {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0x20_0001, 5, 0xC5);
        }
        assert!(s.sram_dirty, "a guest SRAM write sets the dirty flag");
        assert_eq!(
            s.mega_bus(&mut sink).read8(0x20_0001, 5).0,
            0xC5,
            "SRAM read-back at the odd address"
        );

        // An even address in an odd-byte cart is the unused parity → ROM/open bus, NOT the SRAM cell.
        {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0xFF_0000, 5, 0x11); // drive open bus = 0x1111
            assert_ne!(
                bus.read8(0x20_0000, 5).0,
                0xC5,
                "even address (unused parity) is not SRAM"
            );
        }

        // Write-protect (bit1 = 1) blocks stores while keeping SRAM mapped for reads.
        s.mega_bus(&mut sink).write8(0xA1_30F1, 5, 0x03);
        {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0x20_0001, 5, 0x99); // dropped by write-protect
        }
        assert_eq!(
            s.mega_bus(&mut sink).read8(0x20_0001, 5).0,
            0xC5,
            "write-protect blocked the store"
        );

        // Disable (bit0 = 0) → the window reads ROM again, not the retained SRAM cell.
        s.mega_bus(&mut sink).write8(0xA1_30F1, 5, 0x00);
        {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0xFF_0000, 5, 0x22); // open bus = 0x2222
            assert_ne!(
                bus.read8(0x20_0001, 5).0,
                0xC5,
                "disabled → ROM shown, not SRAM"
            );
        }
    }

    #[test]
    fn sram_contents_and_map_survive_a_snapshot() {
        // SRAM is real mutable state → it rides the bincode snapshot (like z80_ram), even though it is out of
        // export_state/state_hash. A cart with a written SRAM cell round-trips byte-for-byte.
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x5A);
        s.load_rom(rom_with_sram(0x20_0001, 0x20_3FFF, true));
        s.mega_bus(&mut ()).write8(0xA1_30F1, 5, 0x01);
        s.mega_bus(&mut ()).write8(0x20_0001, 5, 0xC5);
        let back = System::restore(&s.snapshot()).expect("snapshot decodes");
        assert_eq!(s, back, "the whole machine round-trips, SRAM included");
        assert!(back.sram_present && back.sram[0] == 0xC5);
    }

    #[test]
    fn no_ra_rom_maps_fallback_sram_after_a130f1_enable() {
        // S4: the header-less fallback. A ROM with NO "RA" header (like Sonic & Knuckles + Sonic 3) still
        // saves — enabling $A130F1 bit0 maps the standard fallback page at $200001+, and a write there is
        // retained + latches `sram_used`. Before enabling and at the unused parity, the window reads ROM.
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x99);
        s.load_rom(vec![0x5Au8; 0x4000]); // no RA magic → fallback map
        assert!(s.sram_present, "no RA → fallback SRAM provisioned");
        assert_eq!(s.sram_base, SRAM_FALLBACK_BASE);
        assert_eq!(s.sram.len(), 0x8000, "fallback = 32 KiB odd-byte chip");
        let mut sink = ();
        // Enable via $A130F1 bit0, then write + read back an odd fallback cell.
        s.mega_bus(&mut sink).write8(0xA1_30F1, 5, 0x01);
        assert!(s.sram_enabled, "the latch tracks the enable write");
        {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0x20_0001, 5, 0xFF); // hits fallback sram[0]
        }
        assert_eq!(
            s.mega_bus(&mut sink).read8(0x20_0001, 5).0,
            0xFF,
            "no-RA fallback SRAM retains + serves the guest write"
        );
        assert!(s.sram_dirty, "a guest SRAM write dirties the buffer");
        assert!(s.sram_used, "…and latches sram_used (this cart saves)");
        assert_eq!(s.sram[0], 0xFF, "sram[0] backs $200001 (odd-byte lane)");
        // The even neighbour $200000 is the unused parity → ROM/open bus, NOT the SRAM cell (drive the open-bus
        // latch to a distinct value first, then confirm the read echoes it, not the 0xFF SRAM byte).
        {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0xFF_0000, 5, 0x11); // work-RAM write drives open bus = 0x1111
            assert_ne!(
                bus.read8(0x20_0000, 5).0,
                0xFF,
                "even address (unused parity) is not the SRAM cell"
            );
        }
    }

    #[test]
    fn no_a130f1_write_never_maps_sram_currency_neutral() {
        // Currency-neutrality micro-check: with a buffer now provisioned for every cart, the guarantee is that
        // WITHOUT an $A130F1 enable write, SRAM never maps — exactly the golden-ROM condition (no golden ROM
        // touches $A130F1). The fixture and a plain synthetic ROM both stay inert: sram_enabled false, a write
        // into the window stores NOTHING (buffer stays all-zero), and sram_used/dirty stay false → no persistence.
        use crate::m68000::bus68k::Bus68k;
        for rom in [crate::testrom::build(), vec![0x5Au8; 0x4000]] {
            let mut s = System::new(0x99);
            s.load_rom(rom);
            assert!(s.sram_present, "buffer provisioned…");
            assert!(!s.sram_enabled(), "…but never enabled (no $A130F1 write)");
            let mut sink = ();
            {
                let mut bus = s.mega_bus(&mut sink);
                bus.write8(0x20_0001, 5, 0xFF); // dropped — SRAM disabled (falls through to ROM/open-bus)
            }
            assert!(
                s.sram.iter().all(|&b| b == 0),
                "disabled: the SRAM buffer stores nothing (stays all-zero)"
            );
            assert!(!s.sram_dirty, "disabled write does not dirty SRAM");
            assert!(
                !s.sram_used,
                "…and never latches sram_used → no `.srm` created"
            );
        }
    }

    #[test]
    fn sram_control_latch_tracks_a130f1_bit0_and_bit1() {
        // $A130F1 bit0 = SRAM enable, bit1 = write-protect. A byte write to the odd address $A130F1 is exactly
        // what the shipping S3K driver issues (`move.b #1,($A130F1)`), so store_byte sees the meaningful byte.
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x130F);
        // Enable SRAM (bit0 = 1), write-protect clear.
        s.mega_bus(&mut ()).write8(0xA1_30F1, 5, 0x01);
        assert!(s.sram_enabled, "bit0 = 1 enables SRAM");
        assert!(!s.sram_write_protect, "bit1 still clear");
        // Both bits: enable + write-protect.
        s.mega_bus(&mut ()).write8(0xA1_30F1, 5, 0x03);
        assert!(s.sram_enabled, "bit0 still set");
        assert!(s.sram_write_protect, "bit1 = 1 sets write-protect");
        // Write-protect only (bit1 = 1, bit0 = 0): SRAM mapped-off but protect latched.
        s.mega_bus(&mut ()).write8(0xA1_30F1, 5, 0x02);
        assert!(!s.sram_enabled, "bit0 = 0 disables SRAM");
        assert!(s.sram_write_protect, "bit1 = 1 keeps write-protect");
        // Clear both (what the driver writes to stop SRAM access, sonic3k.asm:293).
        s.mega_bus(&mut ()).write8(0xA1_30F1, 5, 0x00);
        assert!(!s.sram_enabled, "bit0 = 0 clears enable");
        assert!(!s.sram_write_protect, "bit1 = 0 clears write-protect");
    }

    #[test]
    fn sram_control_latch_ignores_the_even_neighbour_byte() {
        // $A130F1 is the ODD byte of its word. A byte write to the even neighbour $A130F0 must NOT latch (the
        // meaningful control byte lands on the odd half), mirroring how $A11101 is inert for the Z80 latch.
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x130F);
        s.mega_bus(&mut ()).write8(0xA1_30F0, 5, 0xFF);
        assert!(
            !s.sram_enabled,
            "a write to even $A130F0 does not touch the latch"
        );
        assert!(
            !s.sram_write_protect,
            "even-byte write leaves write-protect clear"
        );
    }

    #[test]
    fn a130f1_is_write_only_reads_are_open_bus() {
        // The register is write-only (no in-tree driver ever reads it): S0 adds no read arm, so a read of
        // $A130F1 returns the open-bus latch, NOT the enable bit. Enable SRAM, then drive a distinct word on
        // the bus (0xABAB via a RAM read) and confirm the $A130F1 read echoes open bus (0xAB), not the latch.
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x130F);
        let mut sink = ();
        let v = {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0xA1_30F1, 5, 0x01); // sram_enabled = true
            bus.write8(0xFF_0000, 5, 0xAB); // drive 0xABAB onto the open-bus latch (a RAM byte write)
            bus.read8(0xA1_30F1, 5).0
        };
        assert!(s.sram_enabled);
        assert_eq!(
            v, 0xAB,
            "a $A130F1 read returns open bus (0xAB), proving there is no read arm reflecting the enable latch"
        );
    }

    #[test]
    fn sram_control_latch_round_trips_through_the_snapshot() {
        // The latch rides the bincode snapshot for determinism (like z80_busreq) even though it is out of
        // export_state/state_hash. A machine with SRAM enabled + write-protected round-trips byte-for-byte.
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x130F);
        s.mega_bus(&mut ()).write8(0xA1_30F1, 5, 0x03);
        assert!(s.sram_enabled && s.sram_write_protect);
        let back = System::restore(&s.snapshot()).expect("snapshot decodes");
        assert_eq!(s, back, "the SRAM control latch survives snapshot/restore");
        assert!(back.sram_enabled && back.sram_write_protect);
    }

    #[test]
    fn reset_preserves_the_loaded_rom() {
        // Use the valid test ROM: reset() now *drives* the power-on sequence over the ROM vector table, so
        // the ROM must have a sane reset vector (a garbage ROM would fault during reset). The invariant
        // under test is that reset does not erase the cartridge.
        let rom = crate::testrom::build();
        let mut s = System::new(0x55);
        s.load_rom(rom.clone());
        s.reset();
        assert_eq!(
            s.rom(),
            &rom[..],
            "a reset does not erase the cartridge ROM"
        );
    }

    #[test]
    fn garbage_rom_reset_halts_without_spinning() {
        // A garbage ROM gives an odd reset vector (SSP = PC = $FFFFFFFF): the power-on reset faults on the
        // odd first prefetch, and the resulting group-0 frame's own stacking faults again at the odd SSP —
        // a double bus fault (M68000UM §5.4.4). The CPU must HALT, not spin `MicroState.cycles` to a u32
        // overflow. (Before the double-fault wiring this test hung / overflowed.)
        use crate::m68000::microop::CpuState;
        let mut s = System::new(0x99);
        s.load_rom(vec![0xFFu8; 0x100]);
        s.reset();
        assert_eq!(
            s.cpu.state(),
            CpuState::Halted,
            "a double bus fault during reset halts the processor"
        );
    }

    #[test]
    fn export_state_has_the_fixed_layout_and_version() {
        let s = System::new(0x1234);
        let img = s.export_state();
        let expected = 2
            + EXPORT_M68K_REGS_LEN
            + RAM_SIZE
            + Z80_RAM_SIZE
            + EXPORT_Z80_REGS_PLACEHOLDER
            + (VRAM_SIZE + CRAM_SIZE + VSRAM_SIZE + REG_COUNT)
            + EXPORT_FM_PLACEHOLDER
            + EXPORT_PSG_PLACEHOLDER
            + EXPORT_SRAM_LEN;
        assert_eq!(img.len(), expected, "export_state total length");
        assert_eq!(
            u16::from_le_bytes([img[0], img[1]]),
            EXPORT_STATE_VERSION,
            "version field first"
        );
        // Work RAM follows the version + the 78-byte regs region.
        let ram_off = 2 + EXPORT_M68K_REGS_LEN;
        assert_eq!(
            &img[ram_off..ram_off + RAM_SIZE],
            s.ram(),
            "the work-RAM region mirrors System RAM"
        );
    }

    #[test]
    fn export_state_captures_live_z80_ram() {
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x77);
        // $A00000 -> z80_ram[0]. A 68000 write to Z80 RAM is real mutable state, so it must be visible in
        // the differential currency (it was silently serialized as zeros before the v1 freeze).
        // K4-3: the window forwards only while BUSREQ is granted AND reset released — do what real
        // loader code does before storing.
        s.mega_bus(&mut ()).write8(0xA1_1200, 5, 0x01);
        s.mega_bus(&mut ()).write8(0xA1_1100, 5, 0x01);
        s.mega_bus(&mut ()).write8(0xA0_0000, 5, 0xAB);
        let img = s.export_state();
        let z80_off = 2 + EXPORT_M68K_REGS_LEN + RAM_SIZE;
        assert_eq!(
            img[z80_off], 0xAB,
            "the live Z80 RAM byte appears at the Z80-RAM offset in export_state"
        );
        // The reserved Z80-register sub-block that follows the 0x2000 of live RAM is zeroed (it fills when
        // the Z80 core lands — a content change, not a layout change).
        let regs_off = z80_off + Z80_RAM_SIZE;
        assert!(
            img[regs_off..regs_off + EXPORT_Z80_REGS_PLACEHOLDER]
                .iter()
                .all(|&b| b == 0),
            "the reserved Z80-register sub-block is zeroed"
        );
    }

    #[test]
    fn export_state_z80_register_region_is_driven_and_zero_at_reset() {
        // Z-live go-live: export_state region 4 (Z80 regs, 0x40 @ the offset after the live Z80 RAM) is now
        // DRIVEN from the Z80 struct's register file (ZC9 layout), no longer a hardcoded zero fill. Because
        // every committed fixture holds the Z80 in reset (all-zero reset model) and the reset struct is
        // all-zero, the region still emits all zeros even after a run — so the export golden does NOT move.
        let mut s = booted(0x2E80);
        s.run_frames(3);
        let img = s.export_state();
        let regs_off = 2 + EXPORT_M68K_REGS_LEN + RAM_SIZE + Z80_RAM_SIZE;
        assert!(
            img[regs_off..regs_off + EXPORT_Z80_REGS_PLACEHOLDER]
                .iter()
                .all(|&b| b == 0),
            "export_state region 4 stays zeroed at reset (all-zero reset model → golden frozen at go-live)"
        );
    }

    /// Byte offset of the v2 SRAM tail region: everything before it summed (the whole image sans the final
    /// 0x10000 SRAM block). Written as the region ladder, matching `export_state`'s own arithmetic.
    const EXPORT_SRAM_OFF: usize = 2
        + EXPORT_M68K_REGS_LEN
        + RAM_SIZE
        + Z80_RAM_SIZE
        + EXPORT_Z80_REGS_PLACEHOLDER
        + (VRAM_SIZE + CRAM_SIZE + VSRAM_SIZE + REG_COUNT)
        + EXPORT_FM_PLACEHOLDER
        + EXPORT_PSG_PLACEHOLDER;

    #[test]
    fn export_state_sram_region_is_live_left_justified_and_zero_padded() {
        // v2 SRAM go-live: a written SRAM cell appears in export_state's fixed 64 KiB tail region,
        // left-justified with zero padding after, and the region is exactly 0x10000 long.
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x5A);
        s.load_rom(rom_with_sram(0x20_0001, 0x20_3FFF, true)); // 0x2000-byte odd-lane chip
        assert!(s.sram_present, "RA header detected");

        // Enable SRAM, then write known bytes to the first three odd cells: 0x200001→sram[0], etc.
        let mut sink = ();
        s.mega_bus(&mut sink).write8(0xA1_30F1, 5, 0x01);
        {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0x20_0001, 5, 0xDE);
            bus.write8(0x20_0003, 5, 0xAD);
            bus.write8(0x20_0005, 5, 0xBE);
        }
        assert_eq!(&s.sram()[..3], &[0xDE, 0xAD, 0xBE], "guest writes landed");

        let img = s.export_state();
        // The whole image ends with exactly one 0x10000 SRAM block.
        assert_eq!(
            img.len(),
            EXPORT_SRAM_OFF + 0x1_0000,
            "SRAM region is the final 0x10000 bytes"
        );
        let sram = &img[EXPORT_SRAM_OFF..EXPORT_SRAM_OFF + 0x1_0000];
        assert_eq!(sram.len(), 0x1_0000, "SRAM region is exactly 64 KiB");
        // Left-justified: the live chip bytes at the front.
        assert_eq!(&sram[..3], &[0xDE, 0xAD, 0xBE], "SRAM bytes left-justified");
        // Zero-padded: everything past the 0x2000-byte chip is zero.
        assert!(
            sram[0x2000..].iter().all(|&b| b == 0),
            "the region is zero-padded past the live chip"
        );
    }

    #[test]
    fn export_state_sram_region_is_all_zero_without_a_save() {
        // The golden/fixture path. Since S4 the fixture cart gets a zeroed fallback SRAM buffer, but with no
        // guest write it stays all-zero, so the fixed 64 KiB SRAM region serializes byte-identically to the
        // old empty-buffer case — this is why the go-live golden stays valid (an all-zero fallback buffer is
        // export-equivalent to an empty one, so the currency golden is untouched by S4).
        let mut s = System::new(0x5A);
        s.load_rom(crate::testrom::build()); // no RA → zeroed fallback buffer, never written
        assert!(s.sram_present(), "fixture now has a fallback buffer");
        assert!(
            !s.sram_used(),
            "…but no guest write → export region stays all-zero"
        );
        assert!(
            s.sram().iter().all(|&b| b == 0),
            "fallback buffer is zeroed"
        );

        let img = s.export_state();
        assert_eq!(img.len(), EXPORT_SRAM_OFF + 0x1_0000);
        assert!(
            img[EXPORT_SRAM_OFF..EXPORT_SRAM_OFF + 0x1_0000]
                .iter()
                .all(|&b| b == 0),
            "no save → an all-zero 64 KiB SRAM region (export-equivalent to the old empty buffer)"
        );
    }

    #[test]
    fn snapshot_round_trips_the_z80_and_its_frontier() {
        // The Z80 struct + the z80_running / z80_frontier_mclk scalars ride the bincode snapshot (determinism)
        // even though they are not in export_state. A booted, run machine round-trips them byte-for-byte.
        let mut s = booted(0x5A5A);
        s.run_frames(2);
        // The frontier tracked `now` while the Z80 sat in reset (gated off), so it is non-trivial to carry.
        assert_eq!(
            s.z80_frontier_mclk,
            s.scheduler().now(),
            "the gated-off frontier tracks now (zero backlog on a future reset-release)"
        );
        assert!(!s.z80_running, "no fixture releases the Z80 from reset");
        let back = System::restore(&s.snapshot()).expect("snapshot decodes");
        assert_eq!(
            s, back,
            "the whole machine round-trips, Z80 + frontier included"
        );
    }

    #[test]
    fn z80_executes_in_the_run_loop_when_released() {
        // The out-of-band Z-live harness (ZC13): release the Z80, load a small program into Z80 RAM, run one
        // frame, and assert it executed real instructions through the System run loop over the Z80Bus. No
        // committed fixture does this (they all hold the Z80 in reset), so this is opt-in and touches no
        // frozen currency — it proves the wiring works, the way SST proves the 68000 out-of-band.
        let mut s = booted(0x2E80);
        // A tiny program at $0000: LD A,$5A ; LD ($1000),A ; HALT.
        let program = [0x3E, 0x5A, 0x32, 0x00, 0x10, 0x76];
        s.z80_ram[..program.len()].copy_from_slice(&program);
        // Release the Z80 from reset — what a 68000 `$A11200` bit0 = 1 write does; the frontier already tracks
        // `now`, so the chase starts with zero backlog (ZC5).
        s.z80_running = true;
        s.run_frames(1);
        assert_eq!(
            s.z80_ram[0x1000], 0x5A,
            "the Z80 executed LD ($1000),A and stored into its RAM"
        );
        let r = s.z80.regs();
        assert_eq!(r.a, 0x5A, "A holds the loaded immediate");
        assert!(r.halted, "the Z80 reached HALT and idled there");
        // The frontier kept pace with the 68000's clock (it ran real instructions, not just tracked `now`).
        assert!(
            s.z80_frontier_mclk >= s.scheduler().now(),
            "the released Z80's frontier reaches the 68000 clock"
        );
    }

    #[test]
    fn z80_reads_rom_through_the_bank_window_in_the_run_loop() {
        // A released Z80 fetches data from 68k ROM through its $8000-$FFFF window — the path a real sound
        // driver uses to read music/DAC data. Program: LD A,($8000) ; LD ($1000),A ; HALT, with bank = 0 so
        // the window base is $000000 (the ROM's first bytes). Asserts the byte the Z80 read matches the ROM.
        let mut s = booted(0x2E80);
        let rom_byte = s.rom()[0x0000];
        let program = [0x3A, 0x00, 0x80, 0x32, 0x00, 0x10, 0x76];
        s.z80_ram[..program.len()].copy_from_slice(&program);
        s.z80_running = true;
        s.run_frames(1);
        assert_eq!(
            s.z80_ram[0x1000], rom_byte,
            "the Z80 read ROM $000000 through the bank window (bank 0)"
        );
    }

    #[test]
    fn z80_takes_the_vblank_interrupt_and_runs_its_im1_handler() {
        // The Z-live interrupt path (ZC14): a released Z80 running with interrupts enabled idles in HALT until
        // the VDP's vblank raises its `/INT` line, then vectors to the IM 1 handler at $0038 and runs it. This
        // is the timing spine a real SMPS driver rides. Out-of-band (no committed fixture releases the Z80).
        let mut s = booted(0x2E80);
        // Main program at $0000: EI ; HALT — enable interrupts, then idle waiting for vblank.
        s.z80_ram[0x0000] = 0xFB; // EI
        s.z80_ram[0x0001] = 0x76; // HALT
                                  // IM 1 handler at $0038: LD A,$99 ; LD ($1000),A ; HALT.
        s.z80_ram[0x0038] = 0x3E; // LD A,n
        s.z80_ram[0x0039] = 0x99;
        s.z80_ram[0x003A] = 0x32; // LD (nn),A
        s.z80_ram[0x003B] = 0x00;
        s.z80_ram[0x003C] = 0x10;
        s.z80_ram[0x003D] = 0x76; // HALT
                                  // Force IM 1 (the Genesis BIOS/SMPS mode) via the public register-view constructor; the reset default
                                  // IM 0 also vectors to $0038 here, but pin IM 1 so the test asserts the real path.
        s.z80 = crate::z80::Z80::from_regs(&crate::z80::Z80Regs {
            im: 1,
            ..Default::default()
        });
        s.z80_running = true;
        s.run_frames(1);
        assert_eq!(
            s.z80_ram[0x1000], 0x99,
            "the IM 1 vblank handler ran and stored its sentinel"
        );
        let r = s.z80.regs();
        assert!(!r.iff1, "interrupt acceptance cleared IFF1");
        assert!(r.halted, "the handler's final HALT re-idled the Z80");
    }

    #[test]
    fn z80_fm_and_psg_writes_surface_through_the_run_loop_sink() {
        // The Phase RT tap end-to-end: a released Z80 whose driver writes the FM latch and the PSG must
        // surface those register writes as BusEvents through the sink-generic run loop. Out-of-band (no
        // committed fixture releases the Z80), so it touches no frozen currency. The 68000 also emits its own
        // events into the same sink, so we assert the two expected sound writes are PRESENT (not exact count).
        let mut s = booted(0x2E80);
        // Program at $0000: LD A,$22 ; LD ($4000),A ; LD A,$9F ; LD ($7F11),A ; HALT.
        let program = [
            0x3E, 0x22, 0x32, 0x00, 0x40, 0x3E, 0x9F, 0x32, 0x11, 0x7F, 0x76,
        ];
        s.z80_ram[..program.len()].copy_from_slice(&program);
        s.z80_running = true;
        let mut sink: Vec<BusEvent> = Vec::new();
        s.run_frames_with_sink(1, &mut sink);
        assert!(
            sink.iter().any(|e| e.op == crate::bus::BusOp::Write
                && e.fc == 0
                && e.addr == 0x4000
                && e.value == 0x22),
            "the Z80's FM write ($4000 <- $22) surfaced as a BusEvent"
        );
        assert!(
            sink.iter().any(|e| e.op == crate::bus::BusOp::Write
                && e.fc == 0
                && e.addr == 0x7F11
                && e.value == 0x9F),
            "the Z80's PSG write ($7F11 <- $9F) surfaced as a BusEvent"
        );
    }

    #[test]
    fn vgm_logger_captures_z80_fm_and_psg_writes_end_to_end() {
        // RT-2 end-to-end: a released Z80 whose driver latches an FM register (addr + data) and writes a PSG
        // byte must surface through the sink-generic run loop into a VgmLogger as decoded records. Out-of-band
        // (no committed fixture releases the Z80), so it touches no frozen currency.
        use crate::vgm::{SoundChip, VgmLogger};
        let mut s = booted(0x2E80);
        // Program at $0000:
        //   LD A,$28 ; LD ($4000),A   ; FM bank-0 address latch (reg $28)
        //   LD A,$F0 ; LD ($4001),A   ; FM bank-0 data → completes the triple
        //   LD A,$9F ; LD ($7F11),A   ; PSG latch byte
        //   HALT
        let program = [
            0x3E, 0x28, 0x32, 0x00, 0x40, // LD A,$28 ; LD ($4000),A
            0x3E, 0xF0, 0x32, 0x01, 0x40, // LD A,$F0 ; LD ($4001),A
            0x3E, 0x9F, 0x32, 0x11, 0x7F, // LD A,$9F ; LD ($7F11),A
            0x76, // HALT
        ];
        s.z80_ram[..program.len()].copy_from_slice(&program);
        s.z80_running = true;
        let mut logger = VgmLogger::new();
        s.run_frames_with_sink(1, &mut logger);

        // The completed FM triple {Ym2612, port 0, reg $28, value $F0} was captured.
        assert!(
            logger.records().iter().any(|r| r.chip == SoundChip::Ym2612
                && r.port == 0
                && r.reg == 0x28
                && r.value == 0xF0),
            "the Z80's FM register write ($28 <- $F0) decoded into a record"
        );
        // The PSG byte was captured.
        assert!(
            logger
                .records()
                .iter()
                .any(|r| r.chip == SoundChip::Psg && r.value == 0x9F),
            "the Z80's PSG write ($7F11 <- $9F) decoded into a record"
        );
        assert!(logger.fm_writes() >= 1, "at least one FM write recorded");
        assert!(logger.psg_writes() >= 1, "at least one PSG write recorded");
    }

    #[test]
    fn export_state_hash_is_deterministic_and_seed_sensitive() {
        assert_eq!(
            System::new(9).export_state_hash(),
            System::new(9).export_state_hash(),
            "same seed -> same export_state_hash"
        );
        assert_ne!(
            System::new(1).export_state_hash(),
            System::new(2).export_state_hash(),
            "different seeds -> different export_state_hash (the gate has teeth)"
        );
    }

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut s = System::new(0x5EED);
        s.run_frames(2);
        let snap = s.snapshot();
        let back = System::restore(&snap).expect("snapshot should decode");
        assert_eq!(s, back);
        assert_eq!(s.state_hash(), back.state_hash());
    }

    // -----------------------------------------------------------------------------------------------
    // The stop signal / predicate-driven run
    // -----------------------------------------------------------------------------------------------

    /// A sink that asks to stop after exactly `n` instruction boundaries, latching once fired. Written by
    /// hand (rather than via `StopWhen`) so the tests below also cover a *foreign* sink driving the seam.
    #[derive(Default)]
    struct StopAfter {
        n: u64,
        seen: u64,
    }

    impl BusEventSink for StopAfter {
        fn on_event(&mut self, _event: crate::bus::BusEvent) {}
        fn on_step_boundary(&mut self, _pc: u32, _frame: u64) {
            self.seen += 1;
        }
        fn stop_requested(&self) -> bool {
            self.seen > self.n
        }
    }

    /// **The currency argument, executed.** A sink that never asks to stop must leave the run
    /// byte-for-byte what the null sink produces — same instruction stream, same clock, same everything.
    #[test]
    fn a_never_firing_stop_signal_leaves_the_run_identical_to_the_null_sink() {
        let mut plain = booted(7);
        let mut instrumented = booted(7);
        plain.run_frames(3);
        let mut never = StopWhen::new(|_pc, _frame| false);
        let record = instrumented.run_frames_with_sink(3, &mut never);
        assert!(
            record.timed_out(),
            "a predicate that is never true must report DeadlineReached, not success"
        );
        assert_eq!(
            plain.export_state_hash(),
            instrumented.export_state_hash(),
            "the export-state currency is unmoved by an attached (never-firing) stop signal"
        );
        assert_eq!(
            plain, instrumented,
            "the WHOLE machine is identical, not merely the hash"
        );
        assert_eq!(
            plain.frame_boundary_mclk, instrumented.frame_boundary_mclk,
            "the frame anchor is untouched on the deadline path"
        );
    }

    /// The run stops **before** the flagged instruction commits: `pc` in the record is the instruction that
    /// has not yet executed, and it is the machine's live PC.
    #[test]
    fn a_predicate_stops_before_the_flagged_instruction_commits() {
        // Find a PC the ROM actually reaches, by recording the 40th step boundary of a plain run.
        let mut probe = booted(7);
        let mut boundaries: Vec<u32> = Vec::new();
        {
            let mut collect = StopWhen::new(|pc, _f| {
                boundaries.push(pc);
                false
            });
            probe.run_frames_with_sink(1, &mut collect);
        }
        let target = boundaries[40];

        let mut s = booted(7);
        let record = s.run_until_stop(1, |pc, _frame| pc == target);
        assert!(record.fired(), "the predicate must have fired");
        assert_eq!(
            record.pc, target,
            "the record names the flagged instruction"
        );
        assert_eq!(
            s.cpu.regs.pc, target,
            "the machine is parked ON that instruction — it has not executed"
        );
    }

    /// `fired()` and `timed_out()` are never both-ish: a predicate that cannot match reports the bound, and
    /// the caller can tell the two apart without guessing.
    #[test]
    fn an_unmatched_predicate_reports_the_bound_and_not_success() {
        let mut s = booted(7);
        let record = s.run_until_stop(2, |pc, _frame| pc == 0x00FF_FFFE);
        assert!(!record.fired());
        assert!(record.timed_out());
        assert_eq!(record.reason, StopReason::DeadlineReached);
        assert_eq!(
            s.frame_boundary_mclk,
            2 * MCLK_PER_FRAME,
            "a bounded fallback runs the full budget, exactly like run_frames(2)"
        );
    }

    /// **Resumability.** Stopping early and continuing to the same absolute deadline must land on exactly the
    /// state an uninterrupted run reaches — i.e. the stop is a pause at an instruction boundary, not a
    /// perturbation. This is the property that lets a stop condition be used inside a currency-bearing run.
    #[test]
    fn a_stopped_run_resumes_to_the_state_an_uninterrupted_run_reaches() {
        let deadline = 3 * MCLK_PER_FRAME;

        let mut straight = booted(0x1234);
        straight.run_until(deadline);

        let mut interrupted = booted(0x1234);
        let mut stop = StopAfter { n: 500, seen: 0 };
        let record = interrupted.run_until_with_sink(deadline, &mut stop);
        assert!(
            record.fired(),
            "the sink must actually have stopped the run"
        );
        assert!(
            record.mclk < deadline,
            "and it must have stopped strictly before the deadline ({} < {deadline})",
            record.mclk
        );
        // Resume to the SAME absolute deadline with no sink attached.
        interrupted.run_until(deadline);

        assert_eq!(
            straight, interrupted,
            "an interrupted-and-resumed run is byte-identical to an uninterrupted one"
        );
    }

    /// Stamps are emulated, never wall-clock (recon §5 C2): the same ROM + seed + predicate yields the same
    /// record, run after run.
    #[test]
    fn stop_records_are_reproducible() {
        let mut a = booted(0x99);
        let mut b = booted(0x99);
        let ra = a.run_until_stop(2, |_pc, frame| frame >= 1);
        let rb = b.run_until_stop(2, |_pc, frame| frame >= 1);
        assert_eq!(ra, rb, "identical runs produce identical stop records");
        assert!(ra.fired());
        assert_eq!(ra.frame, ra.mclk / MCLK_PER_FRAME);
    }

    /// After an early stop the frame anchor moves to the last WHOLE frame boundary crossed — it neither
    /// claims frames that were not run nor goes backwards.
    #[test]
    fn an_early_stop_anchors_the_frame_grid_at_the_last_whole_frame() {
        let mut s = booted(0x2468);
        let record = s.run_frames_with_sink(10, &mut StopWhen::new(|_pc, frame| frame >= 4));
        assert!(record.fired());
        assert_eq!(record.frame, 4, "stopped inside frame 4");
        assert_eq!(
            s.frame_boundary_mclk,
            4 * MCLK_PER_FRAME,
            "the anchor is the last whole frame boundary crossed, not the unrun 10-frame target"
        );
        // And the anchor is usable: a following run advances from there.
        s.run_frames(1);
        assert_eq!(s.frame_boundary_mclk, 5 * MCLK_PER_FRAME);
    }
}
