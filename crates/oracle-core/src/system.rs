//! The `System` — the one struct that owns *all* machine state.
//!
//! RAM, the VDP memories (VRAM/CRAM/VSRAM) + registers, and the [`Scheduler`] (which owns the sole
//! master clock and sole RNG). It is plain owned data: `Clone` + bincode `Encode`/`Decode`, so a
//! snapshot is an O(struct) copy with no pointer fixup, and `state_hash` is byte-compatible with Oracle.
//!
//! Chips (the CPUs, the VDP) will be added as fields here and driven through a `Bus` adapter that borrows
//! the relevant fields per step (split-borrow). Memory regions are owned byte buffers, always allocated
//! at their fixed hardware sizes by [`System::new`].

use crate::scheduler::Scheduler;
use crate::state_hash::{StateHash, CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};

/// 68000 work RAM, `$FF0000..=$FFFFFF` (64 KiB).
pub const RAM_SIZE: usize = 0x10000;

/// The whole machine. One owner of all state.
#[derive(Clone, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct System {
    /// The power-on seed, retained so [`System::reset`] reproduces the exact power-on state.
    seed: u64,
    scheduler: Scheduler,
    ram: Vec<u8>,
    vram: Vec<u8>,
    cram: Vec<u8>,
    vsram: Vec<u8>,
    vdp_regs: [u8; REG_COUNT],
}

impl std::fmt::Debug for System {
    /// Summarize instead of dumping the 64 KiB buffers (keeps assertion failures readable).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("System")
            .field("seed", &format_args!("{:#018X}", self.seed))
            .field("scheduler", &self.scheduler)
            .field("ram", &format_args!("[{} bytes]", self.ram.len()))
            .field("vram", &format_args!("[{} bytes]", self.vram.len()))
            .field("cram", &format_args!("[{} bytes]", self.cram.len()))
            .field("vsram", &format_args!("[{} bytes]", self.vsram.len()))
            .field("vdp_regs", &self.vdp_regs)
            .field(
                "state_hash.combined",
                &crate::state_hash::hex(self.state_hash().combined),
            )
            .finish()
    }
}

/// Fill `buf` with deterministic bytes drawn from `rng` (8 bytes per draw, little-endian).
fn fill_random(rng: &mut crate::rng::SplitMix64, buf: &mut [u8]) {
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
        let mut vram = vec![0u8; VRAM_SIZE];
        fill_random(scheduler.rng_mut(), &mut ram);
        fill_random(scheduler.rng_mut(), &mut vram);
        Self {
            seed,
            scheduler,
            ram,
            vram,
            cram: vec![0u8; CRAM_SIZE],
            vsram: vec![0u8; VSRAM_SIZE],
            vdp_regs: [0u8; REG_COUNT],
        }
    }

    /// Restore the exact power-on state (the deterministic anchor the determinism gate resets to).
    pub fn reset(&mut self) {
        *self = Self::new(self.seed);
    }

    /// The VDP `state_hash`, byte-compatible with Oracle. Note: 68000 RAM is **not** part of this hash
    /// (Oracle hashes VDP memory + registers only); RAM is still part of the bincode snapshot.
    pub fn state_hash(&self) -> StateHash {
        StateHash::compute(&self.vram, &self.cram, &self.vsram, &self.vdp_regs)
    }

    /// Read-only access to the 68000 work RAM.
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Read-only access to VRAM.
    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    /// Mutable access to VRAM (used by the VDP / bus adapter; here it also lets tests perturb state).
    pub fn vram_mut(&mut self) -> &mut [u8] {
        &mut self.vram
    }

    /// The scheduler (sole master clock + RNG).
    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    /// Mutable scheduler access.
    pub fn scheduler_mut(&mut self) -> &mut Scheduler {
        &mut self.scheduler
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut s = System::new(0x9999);
        let fresh = System::new(0x9999);
        s.vram_mut()[0] ^= 0xFF;
        s.vram_mut()[VRAM_SIZE - 1] ^= 0xFF;
        assert_ne!(s.state_hash(), fresh.state_hash());
        s.reset();
        assert_eq!(s, fresh);
        assert_eq!(s.state_hash(), fresh.state_hash());
    }

    #[test]
    fn bincode_roundtrip_preserves_state() {
        let s = System::new(0x5EED);
        let cfg = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&s, cfg).unwrap();
        let (back, _len): (System, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        assert_eq!(s, back);
        assert_eq!(s.state_hash(), back.state_hash());
    }
}
