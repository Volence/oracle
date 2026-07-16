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
}
