//! The `System` — the one struct that owns *all* machine state.
//!
//! RAM, the VDP memories (VRAM/CRAM/VSRAM) + registers, and the [`Scheduler`] (which owns the sole
//! master clock and sole RNG). It is plain owned data: `Clone` + bincode `Encode`/`Decode`, so a
//! snapshot is an O(struct) copy with no pointer fixup, and `state_hash` is byte-compatible with Oracle.
//!
//! Chips (the CPUs, the VDP) will be added as fields here and driven through a `Bus` adapter that borrows
//! the relevant fields per step (split-borrow). Memory regions are owned byte buffers, always allocated
//! at their fixed hardware sizes by [`System::new`].

use crate::bus::{BusEventSink, MegaDriveBus, Z80_RAM_SIZE};
use crate::m68000::microop::Cpu68000;
use crate::m68000::registers::Registers;
use crate::scheduler::{EventKind, Scheduler};
use crate::state_hash::{StateHash, CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};
use crate::vdp::{Vdp, LINES_PER_FRAME, MCLK_PER_LINE};

/// 68000 work RAM, `$FF0000..=$FFFFFF` (64 KiB).
pub const RAM_SIZE: usize = 0x10000;

/// Master-clock ticks per NTSC frame (H32: 262 scanlines × 3420 mclk).
pub const MCLK_PER_FRAME: u64 = 896_040;

/// Master-clock ticks per 68000 CPU cycle (the 68000 runs at mclk/7). The **one** place the CPU-cycle →
/// mclk conversion happens is [`System::run_until`]; a `* 7` anywhere else is a bug.
pub const MCLK_PER_CPU_CYCLE: u64 = 7;

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
    /// The open-bus latch: the last word driven on the 68000 bus, returned by reads of unmapped space.
    last_bus_word: u16,
    /// The 68000. Driven over a [`MegaDriveBus`] in [`System::step_cpu`]; `step()` returns CPU cycles.
    cpu: Cpu68000,
    /// The absolute mclk of the last frame boundary [`System::run_frames`] targeted. Frame deadlines are
    /// absolute (not `now + frame`), so a step that overshoots one frame's deadline by up to one
    /// instruction is absorbed in the next frame — long-run time stays exact. Serialized so the carry
    /// survives snapshot/restore. Reset to 0 at power-on.
    frame_boundary_mclk: u64,
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
            .field("cpu", &self.cpu)
            .field("frame_boundary_mclk", &self.frame_boundary_mclk)
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
            last_bus_word: 0,
            cpu: Cpu68000::new(power_on_regs()),
            frame_boundary_mclk: 0,
        }
    }

    /// Restore the deterministic power-on anchor (what the determinism gate resets to), preserving the
    /// cartridge ROM, then drive the real `/RESET` sequence: the CPU reads the initial SSP and PC from the
    /// ROM vector table and primes the prefetch queue, leaving the machine ready to execute from the ROM.
    /// The reset runs at the mclk-0 anchor — its cycles are not added to the master clock.
    pub fn reset(&mut self) {
        let rom = std::mem::take(&mut self.rom);
        *self = Self::new(self.seed);
        self.rom = rom;
        self.cpu.assert_reset();
        self.step_cpu(&mut ()); // services reset_pending: runs the power-on reset recipe over the bus
    }

    /// Load the cartridge ROM (`$000000–$3FFFFF` on the 68000 bus). Reads past its end are open bus.
    pub fn load_rom(&mut self, rom: Vec<u8>) {
        self.rom = rom;
    }

    /// Read-only access to the cartridge ROM.
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// Build a [`MegaDriveBus`] over this machine's memory (split-borrow) for a CPU step. The `sink` consumes
    /// the bus event stream (pass `&mut ()` for none). The real CPU drives this in Push C.
    pub fn mega_bus<'a, S: BusEventSink>(&'a mut self, sink: &'a mut S) -> MegaDriveBus<'a, S> {
        let now = self.scheduler.now();
        let System {
            rom,
            ram,
            z80_ram,
            vdp,
            last_bus_word,
            ..
        } = self;
        MegaDriveBus::new(rom, ram, z80_ram, vdp, now, last_bus_word, sink)
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
        // currency) followed by the zeroed reserved register sub-block. VDP / FM / PSG are fixed all-zero
        // placeholders (the layout is frozen; the contents fill in as each chip lands).
        out.extend_from_slice(&self.z80_ram);
        out.extend(std::iter::repeat_n(0u8, EXPORT_Z80_REGS_PLACEHOLDER));
        out.extend(std::iter::repeat_n(
            0u8,
            VRAM_SIZE + CRAM_SIZE + VSRAM_SIZE + REG_COUNT,
        ));
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
        let target = self.frame_boundary_mclk + frames * MCLK_PER_FRAME;
        self.run_until(target);
        self.frame_boundary_mclk = target;
    }

    /// Run until the master clock reaches `deadline_mclk`: pop any due scheduler events (Push C slice 5
    /// wires those to the IPL latch), step the CPU, and advance the clock by the step's cost. A CPU step
    /// may overshoot the deadline by up to one instruction (the ratified sync-on-demand model); the
    /// overshoot carries via [`run_frames`](Self::run_frames)'s absolute deadlines.
    ///
    /// **The one and only CPU-cycle → mclk conversion site**: `mclk += cycles × MCLK_PER_CPU_CYCLE`.
    pub fn run_until(&mut self, deadline_mclk: u64) {
        while self.scheduler.now() < deadline_mclk {
            // Deliver any events whose deadline has arrived (instruction-boundary granularity, consistent
            // with the ratified sync-on-demand model) before stepping — they may raise the pending latches.
            let now = self.scheduler.now();
            while let Some((deadline, kind)) = self.scheduler.pop_due(now) {
                self.deliver_event(deadline, kind);
            }
            let cycles = self.step_cpu(&mut ());
            self.scheduler.advance(cycles as u64 * MCLK_PER_CPU_CYCLE);
            // Re-derive the IPL latch after the step: a taken interrupt's fc=7 /INTAK cleared the VDP's
            // pending latch mid-step (so a delivered VInt does NOT re-fire after RTE), and any enable-bit
            // register write mid-step is picked up here too (recon R12).
            self.cpu.set_ipl(self.vdp.ipl());
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
                if line == 224 {
                    let off = self.vdp.vint_offset();
                    self.scheduler.schedule(deadline + off, EventKind::VInt);
                }
                self.scheduler
                    .schedule(deadline + MCLK_PER_LINE, EventKind::Scanline);
            }
            EventKind::HInt => self.vdp.raise_hint(),
            EventKind::VInt => self.vdp.raise_vint(),
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
        let System {
            cpu,
            rom,
            ram,
            z80_ram,
            vdp,
            last_bus_word,
            ..
        } = self;
        let mut bus = MegaDriveBus::new(rom, ram, z80_ram, vdp, now, last_bus_word, sink);
        cpu.step(&mut bus)
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
