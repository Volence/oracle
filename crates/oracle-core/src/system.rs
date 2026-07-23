//! The `System` — the one struct that owns *all* machine state.
//!
//! RAM, the VDP memories (VRAM/CRAM/VSRAM) + registers, and the [`Scheduler`] (which owns the sole
//! master clock and sole RNG). It is plain owned data: `Clone` + bincode `Encode`/`Decode`, so a
//! snapshot is an O(struct) copy with no pointer fixup, and `state_hash` is byte-compatible with Oracle.
//!
//! Chips (the CPUs, the VDP) will be added as fields here and driven through a `Bus` adapter that borrows
//! the relevant fields per step (split-borrow). Memory regions are owned byte buffers, always allocated
//! at their fixed hardware sizes by [`System::new`].

use crate::bus::{BusEventSink, MegaDriveBus, SramMap, Z80_RAM_SIZE};
use crate::m68000::microop::Cpu68000;
use crate::m68000::registers::Registers;
use crate::scheduler::{EventKind, Scheduler};
use crate::state_hash::{StateHash, CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};
use crate::vdp::{Vdp, LINES_PER_FRAME, MCLK_PER_LINE};
use crate::ym2612::Ym2612;
use crate::z80::{Z80Bus, Z80};

/// 68000 work RAM, `$FF0000..=$FFFFFF` (64 KiB).
pub const RAM_SIZE: usize = 0x10000;

/// Master-clock ticks per NTSC frame (H32: 262 scanlines × 3420 mclk).
pub const MCLK_PER_FRAME: u64 = 896_040;

/// Master-clock ticks per 68000 CPU cycle (the 68000 runs at mclk/7). The **one** place the CPU-cycle →
/// mclk conversion happens is [`System::run_until`]; a `* 7` anywhere else is a bug.
pub const MCLK_PER_CPU_CYCLE: u64 = 7;

/// Master-clock ticks per Z80 cycle (the Z80 runs at mclk/15). The **one** place the Z80-cycle → mclk
/// conversion happens is the Z80 catch-up in [`System::run_until`] (the parallel of the 68000's single ×7
/// site); a `* 15` anywhere else is a bug.
pub const MCLK_PER_Z80_CYCLE: u64 = 15;

/// `export_state` format version (D8). Bumped when the layout changes; Push D freezes v1 + writes the
/// spec. First byte(s) of every `export_state` image.
pub const EXPORT_STATE_VERSION: u16 = 1;

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
/// PSG is the last region, so a future resize shifts nothing else.
const EXPORT_PSG_PLACEHOLDER: usize = 0x10;

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
            cpu: Cpu68000::new(power_on_regs()),
            frame_boundary_mclk: 0,
            z80: Z80::new(),
            z80_frontier_mclk: 0,
            z80_bank: 0,
            fm: Ym2612::new(),
        }
    }

    /// Restore the deterministic power-on anchor (what the determinism gate resets to), preserving the
    /// cartridge ROM, then drive the real `/RESET` sequence: the CPU reads the initial SSP and PC from the
    /// ROM vector table and primes the prefetch queue, leaving the machine ready to execute from the ROM.
    /// The reset runs at the mclk-0 anchor — its cycles are not added to the master clock.
    pub fn reset(&mut self) {
        let rom = std::mem::take(&mut self.rom);
        // Battery-backed SRAM survives a soft reset (its contents + the detected map are preserved, exactly
        // like the cartridge ROM); the `$A130F1` enable latch does NOT — real hardware powers up with SRAM
        // access off and the driver re-enables it. `sram_dirty` also clears (it is only a persistence throttle).
        let sram = std::mem::take(&mut self.sram);
        let (present, base, end, odd) = (
            self.sram_present,
            self.sram_base,
            self.sram_end,
            self.sram_odd,
        );
        *self = Self::new(self.seed);
        self.rom = rom;
        self.sram = sram;
        self.sram_present = present;
        self.sram_base = base;
        self.sram_end = end;
        self.sram_odd = odd;
        self.cpu.assert_reset();
        self.step_cpu(&mut ()); // services reset_pending: runs the power-on reset recipe over the bus
    }

    /// Load the cartridge ROM (`$000000–$3FFFFF` on the 68000 bus). Reads past its end are open bus. Parses
    /// the Genesis SRAM header (Fork 1c hybrid): a valid "RA" field (magic + a sane `$200000-$3FFFFF` range)
    /// allocates the SRAM buffer and records the map; an absent/garbage header leaves SRAM absent (pure ROM,
    /// currency-neutral). The header-less `$A130F1`-activity fallback is a **named deferral** (design Fork 1c)
    /// — not needed by the primary SRAM carts (Phantasy Star, Shining Force, S3K all set "RA") and not built.
    pub fn load_rom(&mut self, rom: Vec<u8>) {
        if let Some(m) = parse_sram_header(&rom) {
            self.sram_present = true;
            self.sram_base = m.base;
            self.sram_end = m.end;
            self.sram_odd = m.odd;
            self.sram = vec![0u8; sram_byte_len(m.base, m.end)];
        } else {
            self.sram_present = false;
            self.sram_base = 0;
            self.sram_end = 0;
            self.sram_odd = false;
            self.sram = Vec::new();
        }
        self.sram_dirty = false;
        self.rom = rom;
    }

    /// Read-only access to the cartridge ROM.
    pub fn rom(&self) -> &[u8] {
        &self.rom
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
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
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
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
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
    /// `docs/export-state-v1.md` (the frozen v1 spec). Region order:
    /// version → m68k regs → work RAM → Z80 RAM → Z80 regs → VDP → FM → PSG. The Z80 RAM is **live**
    /// (68000-reachable at `$A00000`); every not-yet-emulated chip's register/memory state serializes as a
    /// fixed all-zero reserved region. This is distinct from [`state_hash`](Self::state_hash) (the frozen
    /// Oracle-compatible VDP hash, kept for the live-Oracle differential).
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
            + EXPORT_PSG_PLACEHOLDER;
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
    pub fn run_frames_with_sink<S: BusEventSink>(&mut self, frames: u64, sink: &mut S) {
        let target = self.frame_boundary_mclk + frames * MCLK_PER_FRAME;
        self.run_until_with_sink(target, sink);
        self.frame_boundary_mclk = target;
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
    pub fn run_until_with_sink<S: BusEventSink>(&mut self, deadline_mclk: u64, sink: &mut S) {
        // Arm the VDP write-capture buffer for this run only if the sink wants VDP-internal writes
        // (watchpoints v2). Disarmed, the choke points are byte-for-byte the old hot path — this single query
        // is the whole cost when no VDP watch is attached. Restored to off on return.
        let capture = sink.wants_vdp_writes();
        self.vdp.set_write_capture(capture);
        while self.scheduler.now() < deadline_mclk {
            // Deliver any events whose deadline has arrived (instruction-boundary granularity, consistent
            // with the ratified sync-on-demand model) before stepping — they may raise the pending latches.
            let now = self.scheduler.now();
            while let Some((deadline, kind)) = self.scheduler.pop_due(now) {
                self.deliver_event(deadline, kind);
            }
            // Stamp the instruction about to execute (its PC) + the current frame, so a sink that attributes
            // accesses to their driving instruction (watchpoints) has that context. No-op for `&mut ()`.
            sink.on_step_boundary(self.cpu.regs.pc, self.scheduler.now() / MCLK_PER_FRAME);
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
    }

    /// Deliver a fired scheduler event (recon R7/R12). `deadline` is the event's absolute scheduled mclk (its
    /// line start, for the Scanline chain). The **Scanline** event self-reschedules every line and drives the
    /// per-line VDP housekeeping: HINT-counter bookkeeping (an underflow schedules an `HInt` at the pinned H
    /// anchor), and line 224 schedules the `VInt`. `HInt`/`VInt` delivery sets the VDP's pending latches; the
    /// IPL the CPU sees is always recomputed from `vdp.ipl()` (gated by the enable bits). `FrameEnd` is
    /// housekeeping.
    fn deliver_event(&mut self, deadline: u64, kind: EventKind) {
        match kind {
            EventKind::Scanline => {
                let line = (deadline / MCLK_PER_LINE) % LINES_PER_FRAME;
                if self.vdp.on_line_start(line as u16) {
                    let off = self.vdp.hint_offset();
                    self.scheduler.schedule(deadline + off, EventKind::HInt);
                }
                // Render active lines (0..=223) so the sprite overflow/collision status bits + the R10 masking
                // carry evolve during normal runs (games poll them). Currency-safe: the sprite flags/carry are
                // in neither frozen currency, and render output is discarded here. (recon R10 / design §5.)
                if line < 224 {
                    self.vdp.render_scanline(line as u16);
                }
                if line == 224 {
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
            EventKind::HInt => self.vdp.raise_hint(),
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
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
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
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
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
                ..
            } = self;
            while *z80_frontier_mclk < now {
                // The Z80 reads the FM timer at its own frontier (ZC4/FM7) — behind the 68000's `now`, both
                // absolute on the one timeline. Pass the frontier value at the start of this step as the FM's
                // `now`.
                let mut bus =
                    Z80Bus::new(z80_ram, rom, ram, z80_bank, fm, *z80_frontier_mclk, sink);
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
    use crate::bus::BusEvent;

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

    #[test]
    fn scanline_wiring_evolves_the_sprite_masking_carry_during_a_run() {
        // The Scanline event now calls render_scanline for active lines, so the sprite pipeline's state (here
        // the R10 dot-overflow masking carry) evolves during run_frames. Program nine 4-cell sprites on the
        // last active line (223) — 288 px > the 256-px H32 budget → dot overflow. The carry is NOT cleared by
        // status reads, so committing it on line 223 survives to the end of the frame (robust vs the ROM).
        let mut s = booted(0x1111);
        {
            let v = s.vdp_mut();
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
    fn no_ra_rom_never_maps_sram_even_after_a130f1_enable() {
        // Currency-neutrality micro-check: a ROM without an "RA" header never maps SRAM, even when a game
        // writes $A130F1 bit0 = 1. This is exactly the golden-ROM condition (fixture has no RA, no golden
        // touches $A130F1) → the $000000-$3FFFFF region reads/writes ROM byte-identically.
        use crate::m68000::bus68k::Bus68k;
        let mut s = System::new(0x99);
        s.load_rom(vec![0x5Au8; 0x4000]); // no RA magic
        assert!(!s.sram_present, "no RA → SRAM absent");
        assert!(s.sram.is_empty(), "no buffer allocated");
        let mut sink = ();
        s.mega_bus(&mut sink).write8(0xA1_30F1, 5, 0x01); // enable bit set…
        assert!(s.sram_enabled, "the latch still tracks the write");
        let v = {
            let mut bus = s.mega_bus(&mut sink);
            bus.write8(0x20_0001, 5, 0xFF); // dropped — no SRAM buffer (drives open bus to 0xFFFF)
            bus.read8(0xA1_0001, 5); // read the version reg → drives open bus to 0xA0A0
            bus.read8(0x20_0001, 5).0 // open bus low = 0xA0, NOT a retained 0xFF (no SRAM held it)
        };
        assert_eq!(
            v, 0xA0,
            "no RA: $200001 is open-bus/ROM, never a retained SRAM write"
        );
        assert!(!s.sram_dirty, "no SRAM buffer → nothing to dirty");
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
            + EXPORT_PSG_PLACEHOLDER;
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
}
