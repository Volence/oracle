//! The `Vdp` — the Sega 315-5313 video display processor's owned state.
//!
//! Plain owned data (`Clone` + bincode + `PartialEq`), a field of [`crate::system::System`]. It owns the
//! four Oracle-hashed regions (VRAM/CRAM/VSRAM + the 24 registers) at their fixed hardware sizes; the
//! rendering output stays **derived, not state** (nothing render-related serializes). Timing (the h/v
//! counters, vblank/hblank) is a pure function of the master clock, computed at read time — never an
//! incremental counter — so it is not stored here either.
//!
//! Behavioral facts implemented here are pinned in `docs/2026-07-16-vdp-recon.md` (cited R1–R12) against
//! the ratified design brief `docs/2026-07-01-vdp-design.md`; no emulator source informs this code
//! (clean-room, audit policy 3).

use crate::rng::SplitMix64;
use crate::state_hash::{CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};

/// Master-clock ticks per scanline (NTSC): the line is a fixed 3420 mclk.
pub const MCLK_PER_LINE: u64 = 3420;
/// Scanlines per frame (NTSC V28): 224 active + 38 blanking.
pub const LINES_PER_FRAME: u64 = 262;
/// Master-clock ticks per NTSC frame (`MCLK_PER_LINE * LINES_PER_FRAME` = 896_040).
pub const MCLK_PER_FRAME: u64 = MCLK_PER_LINE * LINES_PER_FRAME;

/// The V-counter value at which the vertical-blank status flag sets — line 224 (the `0xDF`→`0xE0`
/// transition, recon R2). Also the first non-active line.
const VBLANK_START_LINE: u64 = 0xE0;

/// The VDP's owned state. The four hashed regions are always allocated at their fixed hardware sizes
/// ([`crate::state_hash`]); the `state_hash`/`export_state` currencies read straight through them, so their
/// byte layout is frozen.
#[derive(Clone, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Vdp {
    /// 64 KiB video RAM.
    vram: Vec<u8>,
    /// 128 bytes of color RAM, stored in Oracle's byte layout (the `state_hash` currency defines the form).
    cram: Vec<u8>,
    /// 80 bytes of vertical-scroll RAM.
    vsram: Vec<u8>,
    /// The 24 VDP registers.
    regs: [u8; REG_COUNT],
    /// The frozen HV-counter value returned while the M3 latch (reg 0 bit 1) is set — an interim model of
    /// the HV counter latch (recon R2; the real trigger is the lightgun HL pin, which nothing on the Mega
    /// Drive pad path asserts). Populated when M3 is turned on by a register write (the ports slice); the
    /// read side ([`Vdp::hv_counter_read`]) consults it here.
    hv_latch: u16,
}

impl std::fmt::Debug for Vdp {
    /// Summarize instead of dumping the 64 KiB VRAM buffer (keeps assertion failures readable).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vdp")
            .field("vram", &format_args!("[{} bytes]", self.vram.len()))
            .field("cram", &format_args!("[{} bytes]", self.cram.len()))
            .field("vsram", &format_args!("[{} bytes]", self.vsram.len()))
            .field("regs", &self.regs)
            .finish()
    }
}

impl Vdp {
    /// Power on: allocate the four regions at their fixed sizes and seed VRAM with deterministic
    /// pseudo-random bytes drawn from the single seeded RNG (CRAM/VSRAM/registers start zeroed) — exactly
    /// what [`crate::system::System::new`] did before the extraction, so the power-on `state_hash` is
    /// byte-identical. The RNG is drawn from **after** the work-RAM fill, preserving the draw order.
    pub fn power_on(rng: &mut SplitMix64) -> Self {
        let mut vram = vec![0u8; VRAM_SIZE];
        crate::system::fill_random(rng, &mut vram);
        Self {
            vram,
            cram: vec![0u8; CRAM_SIZE],
            vsram: vec![0u8; VSRAM_SIZE],
            regs: [0u8; REG_COUNT],
            hv_latch: 0,
        }
    }

    /// Read-only access to VRAM (for the `state_hash`/`export_state` currencies and introspection).
    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    /// Read-only access to CRAM (Oracle byte layout).
    pub fn cram(&self) -> &[u8] {
        &self.cram
    }

    /// Read-only access to VSRAM.
    pub fn vsram(&self) -> &[u8] {
        &self.vsram
    }

    /// Read-only access to the 24 VDP registers.
    pub fn regs(&self) -> &[u8; REG_COUNT] {
        &self.regs
    }

    /// Mutable access to VRAM (used by tests to perturb state; the data-port write path lands in a later
    /// slice). Kept crate-internal-friendly but public for the `System::vram_mut` pass-through.
    pub fn vram_mut(&mut self) -> &mut [u8] {
        &mut self.vram
    }

    // --- Timing FSM: the readable h/v counters + status timing bits are PURE functions of the master
    // clock (granularity C — NTSC V28 geometry is hardcoded per audit policy 4). No incremental counter
    // is stepped; nothing here is stored state. All behavioral values are pinned in recon R2.

    /// H40 (40-cell / 320px) mode iff both horizontal-resolution select bits are set in reg $0C (RS0 = bit
    /// 0, RS1 = bit 7 — the official Sega manual "set both for 40-cell mode" rule); otherwise H32.
    fn h40(&self) -> bool {
        self.regs[0x0C] & 0x81 == 0x81
    }

    /// The readable H counter (recon R2): the top 8 bits of the 9-bit horizontal counter, which sweeps 342
    /// positions (H32) / 422 positions (H40) across the 3420-mclk line. The 8-bit value jumps H32
    /// `0x93`→`0xE9` / H40 `0xB6`→`0xE4` at horizontal retrace. `mclk % 3420` maps linearly across the
    /// positions (the sub-position phase within the line is pure timing).
    pub fn h_counter(&self, mclk: u64) -> u8 {
        let h40 = self.h40();
        let dot = mclk % MCLK_PER_LINE;
        let positions = if h40 { 422 } else { 342 };
        let pos9 = (dot * positions) / MCLK_PER_LINE; // the 9-bit counter position, 0..positions
        let index = (pos9 >> 1) as u16; // the readable value is the top 8 bits
        if h40 {
            if index <= 0xB6 {
                index as u8
            } else {
                (0xE4 + (index - 0xB7)) as u8
            }
        } else if index <= 0x93 {
            index as u8
        } else {
            (0xE9 + (index - 0x94)) as u8
        }
    }

    /// The readable V counter (recon R2, NTSC V28): the scanline number remapped so it jumps `0xEA`→`0xE5`
    /// at line 235 (235 + 27 = 262 lines). On hardware the V counter increments mid-line (R2 anchor H
    /// `0x84`→`0x85` H32 / `0xA4`→`0xA5` H40); we increment at the line boundary — a sub-line phase
    /// difference that is pure timing (documented open item, recon R2).
    pub fn v_counter(&self, mclk: u64) -> u8 {
        let line = (mclk % MCLK_PER_FRAME) / MCLK_PER_LINE; // 0..=261
        if line <= 0xEA {
            line as u8
        } else {
            (0xE5 + (line - 0xEB)) as u8
        }
    }

    /// The HV-counter port value ($C00008): `(V << 8) | H`. Frozen to the M3 latch while reg 0 bit 1 (M3)
    /// is set — the interim HV-latch model (recon R2; see [`Vdp::hv_latch`]).
    pub fn hv_counter_read(&self, mclk: u64) -> u16 {
        if self.regs[0] & 0x02 != 0 {
            self.hv_latch
        } else {
            ((self.v_counter(mclk) as u16) << 8) | self.h_counter(mclk) as u16
        }
    }

    /// VBlank status flag: set across the whole vertical-blank region — V counter ≥ `0xE0` (line ≥ 224, the
    /// `0xDF`→`0xE0` transition; recon R2). Pure function of mclk, no stored flag.
    pub fn vblank(&self, mclk: u64) -> bool {
        (mclk % MCLK_PER_FRAME) / MCLK_PER_LINE >= VBLANK_START_LINE
    }

    /// HBlank status flag: set across horizontal retrace, bounded by the pinned H anchors (recon R2): H32
    /// sets at `0x93` / clears at `0x05`; H40 sets at `0xB3` / clears at `0x06`. Derived from the readable H
    /// counter so the H↔mclk phase anchors live in exactly one place.
    pub fn hblank(&self, mclk: u64) -> bool {
        let h = self.h_counter(mclk);
        if self.h40() {
            h <= 0x05 || h >= 0xB3
        } else {
            h <= 0x04 || h >= 0x93
        }
    }

    /// The VDP status word ($C00004 read), with the timing bits live (recon R2). Bit layout (official Sega
    /// manual): b0 PAL, b1 DMA-busy, b2 HBlank, b3 VBlank, b4 odd-frame, b5 sprite-collision, b6
    /// sprite-overflow, b7 VINT(F), b8 FIFO-full, b9 FIFO-empty. This slice fills the FIFO-empty placeholder
    /// (the FIFO drains immediately this push) + the vblank/hblank timing bits; the interrupt / sprite /
    /// odd-frame bits land with their state in later slices. Not yet wired to the control port — that is the
    /// ports slice (which also clears the pending toggle on a status read).
    pub fn status_word(&self, mclk: u64) -> u16 {
        let mut s = 1u16 << 9; // FIFO empty (placeholder: immediate drain this push)
        if self.vblank(mclk) {
            s |= 1 << 3;
        }
        if self.hblank(mclk) {
            s |= 1 << 2;
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_on_allocates_fixed_region_sizes() {
        let mut rng = SplitMix64::new(1);
        let vdp = Vdp::power_on(&mut rng);
        assert_eq!(vdp.vram().len(), VRAM_SIZE);
        assert_eq!(vdp.cram().len(), CRAM_SIZE);
        assert_eq!(vdp.vsram().len(), VSRAM_SIZE);
        assert_eq!(vdp.regs().len(), REG_COUNT);
    }

    #[test]
    fn power_on_seeds_vram_zeros_the_rest() {
        let mut rng = SplitMix64::new(0xABCD);
        let vdp = Vdp::power_on(&mut rng);
        assert!(
            vdp.vram().iter().any(|&b| b != 0),
            "VRAM is seeded non-zero"
        );
        assert!(vdp.cram().iter().all(|&b| b == 0), "CRAM starts zeroed");
        assert!(vdp.vsram().iter().all(|&b| b == 0), "VSRAM starts zeroed");
        assert!(vdp.regs().iter().all(|&b| b == 0), "registers start zeroed");
    }

    #[test]
    fn same_rng_stream_yields_identical_vram() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        assert_eq!(Vdp::power_on(&mut a), Vdp::power_on(&mut b));
    }

    // --- Timing FSM (recon R2) ---------------------------------------------------------------------------

    fn fresh() -> Vdp {
        Vdp::power_on(&mut SplitMix64::new(1))
    }

    /// The first mclk-in-line dot whose readable H counter equals `target` (each value occurs across a line).
    fn dot_with_h(v: &Vdp, target: u8) -> u64 {
        (0..MCLK_PER_LINE)
            .find(|&d| v.h_counter(d) == target)
            .unwrap_or_else(|| panic!("H = {target:#04X} never occurs in a line"))
    }

    /// Collapse a per-dot sample stream to its distinct-in-order values.
    fn distinct_h(v: &Vdp) -> Vec<u8> {
        let mut seq: Vec<u8> = Vec::new();
        for dot in 0..MCLK_PER_LINE {
            let h = v.h_counter(dot);
            if seq.last() != Some(&h) {
                seq.push(h);
            }
        }
        seq
    }

    fn jumps(seq: &[u8]) -> Vec<(u8, u8)> {
        seq.windows(2)
            .filter(|w| w[1] != w[0].wrapping_add(1))
            .map(|w| (w[0], w[1]))
            .collect()
    }

    #[test]
    fn h_counter_h32_progression_and_jump() {
        let v = fresh(); // regs all zero → H32
        let seq = distinct_h(&v);
        assert_eq!(seq.first(), Some(&0x00), "H32 starts at 0x00");
        assert_eq!(seq.last(), Some(&0xFF), "H32 ends at 0xFF");
        assert_eq!(seq.len(), 171, "H32 has 171 distinct readable values");
        assert_eq!(jumps(&seq), vec![(0x93, 0xE9)], "H32 jumps 0x93→0xE9");
    }

    #[test]
    fn h_counter_h40_progression_and_jump() {
        let mut v = fresh();
        v.regs[0x0C] = 0x81; // RS0 | RS1 → H40
        let seq = distinct_h(&v);
        assert_eq!(seq.first(), Some(&0x00), "H40 starts at 0x00");
        assert_eq!(seq.last(), Some(&0xFF), "H40 ends at 0xFF");
        assert_eq!(seq.len(), 211, "H40 has 211 distinct readable values");
        assert_eq!(jumps(&seq), vec![(0xB6, 0xE4)], "H40 jumps 0xB6→0xE4");
    }

    #[test]
    fn v_counter_progression_and_jump() {
        let v = fresh();
        let mut seq: Vec<u8> = Vec::new();
        for line in 0..LINES_PER_FRAME {
            let vc = v.v_counter(line * MCLK_PER_LINE);
            if seq.last() != Some(&vc) {
                seq.push(vc);
            }
        }
        assert_eq!(seq.len(), 262, "262 distinct V values (235 + 27)");
        assert_eq!(seq.first(), Some(&0x00));
        assert_eq!(seq.last(), Some(&0xFF));
        assert_eq!(jumps(&seq), vec![(0xEA, 0xE5)], "V jumps 0xEA→0xE5");
    }

    #[test]
    fn hblank_h32_anchor_transitions() {
        let v = fresh(); // H32
        assert!(!v.hblank(dot_with_h(&v, 0x92)), "not in hblank at H=0x92");
        assert!(v.hblank(dot_with_h(&v, 0x93)), "hblank SETS at H=0x93");
        assert!(v.hblank(dot_with_h(&v, 0x04)), "still in hblank at H=0x04");
        assert!(!v.hblank(dot_with_h(&v, 0x05)), "hblank CLEARS at H=0x05");
    }

    #[test]
    fn hblank_h40_anchor_transitions() {
        let mut v = fresh();
        v.regs[0x0C] = 0x81; // H40
        assert!(!v.hblank(dot_with_h(&v, 0xB2)), "not in hblank at H=0xB2");
        assert!(v.hblank(dot_with_h(&v, 0xB3)), "hblank SETS at H=0xB3");
        assert!(v.hblank(dot_with_h(&v, 0x05)), "still in hblank at H=0x05");
        assert!(!v.hblank(dot_with_h(&v, 0x06)), "hblank CLEARS at H=0x06");
    }

    #[test]
    fn vblank_sets_at_line_224() {
        let v = fresh();
        assert_eq!(v.v_counter(223 * MCLK_PER_LINE), 0xDF, "line 223 → V=0xDF");
        assert!(!v.vblank(223 * MCLK_PER_LINE), "line 223 is active");
        assert_eq!(v.v_counter(224 * MCLK_PER_LINE), 0xE0, "line 224 → V=0xE0");
        assert!(
            v.vblank(224 * MCLK_PER_LINE),
            "vblank SETS at the 0xDF→0xE0 line"
        );
        assert!(
            v.vblank(261 * MCLK_PER_LINE),
            "vblank holds through the last line"
        );
    }

    #[test]
    fn status_word_reflects_the_timing_bits() {
        let v = fresh(); // H32
                         // Active display, H well inside the visible span (not hblank): only FIFO-empty (bit 9).
        let active = 100 * MCLK_PER_LINE + dot_with_h(&v, 0x40);
        assert_eq!(
            v.status_word(active),
            0x0200,
            "FIFO-empty only during active display"
        );
        // A vblank line sets bit 3.
        let in_vblank = 240 * MCLK_PER_LINE + dot_with_h(&v, 0x40);
        assert_eq!(
            v.status_word(in_vblank) & (1 << 3),
            1 << 3,
            "vblank bit (b3)"
        );
        // Inside horizontal retrace sets bit 2.
        let in_hblank = 100 * MCLK_PER_LINE + dot_with_h(&v, 0x93);
        assert_eq!(
            v.status_word(in_hblank) & (1 << 2),
            1 << 2,
            "hblank bit (b2)"
        );
    }

    #[test]
    fn hv_counter_read_combines_v_and_h() {
        let v = fresh();
        let mclk = 50 * MCLK_PER_LINE + dot_with_h(&v, 0x40);
        let expected = ((v.v_counter(mclk) as u16) << 8) | 0x40;
        assert_eq!(v.hv_counter_read(mclk), expected, "(V << 8) | H");
    }

    #[test]
    fn m3_latch_freezes_the_hv_read() {
        let mut v = fresh();
        v.regs[0] = 0x02; // M3 set (reg 0 bit 1)
        v.hv_latch = 0xABCD;
        assert_eq!(v.hv_counter_read(0), 0xABCD, "frozen regardless of mclk");
        assert_eq!(v.hv_counter_read(12_345), 0xABCD);
        v.regs[0] = 0x00; // M3 clear → live again
        assert_ne!(
            v.hv_counter_read(12_345),
            0xABCD,
            "returns the live counter"
        );
    }
}
