//! `state_hash` — FNV-1a-64 fingerprints of VDP memory + registers.
//!
//! **Byte-compatible with Oracle's `OpStateHash`** (`../oracle-old/linux-port/gui/ControlSocket.cpp`,
//! cross-checked 2026-06-24). This compatibility is a hard requirement and a known footgun: the
//! differential harness and the determinism gate both compare these values. Do not change the byte
//! order, the masking, the region sizes, or the output format without re-verifying against Oracle.

/// FNV-1a-64 offset basis.
pub const FNV_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
/// FNV-1a-64 prime.
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// Hashed region sizes (fixed hardware constants from Oracle's `IS315_5313.h`, *not* runtime sizes).
pub const VRAM_SIZE: usize = 0x10000;
pub const CRAM_SIZE: usize = 0x80;
pub const VSRAM_SIZE: usize = 0x50;
pub const REG_COUNT: usize = 24;

/// Fold one byte into an FNV-1a-64 accumulator (XOR-then-multiply; only the low 8 bits matter).
#[inline]
fn fnv1a(h: u64, byte: u8) -> u64 {
    (h ^ byte as u64).wrapping_mul(FNV_PRIME)
}

/// FNV-1a-64 over a byte slice, starting from the basis.
pub fn fnv1a_bytes(data: &[u8]) -> u64 {
    data.iter().fold(FNV_BASIS, |h, &b| fnv1a(h, b))
}

/// The five FNV-1a-64 fingerprints, byte-compatible with Oracle's `state_hash` op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateHash {
    pub vram: u64,
    pub cram: u64,
    pub vsram: u64,
    pub regs: u64,
    pub combined: u64,
}

impl StateHash {
    /// Compute the fingerprints from the four hashed regions, in Oracle's exact byte order:
    /// VRAM → CRAM → VSRAM → REGS. Each region has its own accumulator; `combined` is one continuous
    /// stream over the concatenation. `regs` is the 24 VDP registers as bytes (low 8 bits each).
    ///
    /// # The sizes and the order are the signature's (lens M70)
    ///
    /// Each region is an array of its own hardware size, and the four sizes all differ ([`VRAM_SIZE`],
    /// [`CRAM_SIZE`], [`VSRAM_SIZE`], [`REG_COUNT`]), so a region of the wrong length **and** two regions
    /// passed in each other's place are compile errors, not a wrong fingerprint in a release build. The
    /// call in Oracle's order compiles, and is the all-zero golden the tests below pin:
    ///
    /// ```
    /// use oracle_core::state_hash::{StateHash, CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};
    /// let (vram, cram, vsram, regs) =
    ///     ([0u8; VRAM_SIZE], [0u8; CRAM_SIZE], [0u8; VSRAM_SIZE], [0u8; REG_COUNT]);
    /// let h = StateHash::compute(&vram, &cram, &vsram, &regs);
    /// assert_eq!(h.combined, 0xF160_1314_F59D_6B45);
    /// ```
    ///
    /// The same call with VRAM and CRAM swapped does not:
    ///
    /// ```compile_fail,E0308
    /// use oracle_core::state_hash::{StateHash, CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};
    /// let (vram, cram, vsram, regs) =
    ///     ([0u8; VRAM_SIZE], [0u8; CRAM_SIZE], [0u8; VSRAM_SIZE], [0u8; REG_COUNT]);
    /// let h = StateHash::compute(&cram, &vram, &vsram, &regs);
    /// assert_eq!(h.combined, 0xF160_1314_F59D_6B45);
    /// ```
    ///
    /// Nor with a region one byte short:
    ///
    /// ```compile_fail,E0308
    /// use oracle_core::state_hash::{StateHash, CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};
    /// let (vram, cram, vsram, regs) =
    ///     ([0u8; VRAM_SIZE], [0u8; CRAM_SIZE - 1], [0u8; VSRAM_SIZE], [0u8; REG_COUNT]);
    /// let h = StateHash::compute(&vram, &cram, &vsram, &regs);
    /// assert_eq!(h.combined, 0xF160_1314_F59D_6B45);
    /// ```
    ///
    /// Nor with a slice, whose length only a run can know (what this signature took until M70):
    ///
    /// ```compile_fail,E0308
    /// use oracle_core::state_hash::{StateHash, CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};
    /// let (vram, cram, vsram, regs) =
    ///     (vec![0u8; VRAM_SIZE], [0u8; CRAM_SIZE], [0u8; VSRAM_SIZE], [0u8; REG_COUNT]);
    /// let h = StateHash::compute(&vram[..], &cram, &vsram, &regs);
    /// assert_eq!(h.combined, 0xF160_1314_F59D_6B45);
    /// ```
    ///
    /// **What makes those three blocks mean anything is the control above them.** Stable rustdoc does not
    /// check a `compile_fail` block's error code (measured: a block marked `E0599` whose real error is
    /// E0308 passed), so each block passes on *any* compile error, a typo included. The control is the same
    /// program with the arguments right, and it compiles and asserts the golden. Run as plain doctests, the
    /// three fail with E0308 at exactly the argument the sentence above each names.
    ///
    /// The four `debug_assert_eq!` length checks this body opened with until M70 are gone rather than
    /// kept: a region's length is its type now, so none of them could fire.
    pub fn compute(
        vram: &[u8; VRAM_SIZE],
        cram: &[u8; CRAM_SIZE],
        vsram: &[u8; VSRAM_SIZE],
        regs: &[u8; REG_COUNT],
    ) -> Self {
        let mut hv = FNV_BASIS;
        let mut hc = FNV_BASIS;
        let mut hs = FNV_BASIS;
        let mut hr = FNV_BASIS;
        let mut hall = FNV_BASIS;
        for &b in vram {
            hv = fnv1a(hv, b);
            hall = fnv1a(hall, b);
        }
        for &b in cram {
            hc = fnv1a(hc, b);
            hall = fnv1a(hall, b);
        }
        for &b in vsram {
            hs = fnv1a(hs, b);
            hall = fnv1a(hall, b);
        }
        for &b in regs {
            hr = fnv1a(hr, b);
            hall = fnv1a(hall, b);
        }
        Self {
            vram: hv,
            cram: hc,
            vsram: hs,
            regs: hr,
            combined: hall,
        }
    }
}

/// Format a hash value exactly as Oracle does: `0x` + 16 uppercase zero-padded hex digits.
pub fn hex(value: u64) -> String {
    format!("0x{value:016X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same bytes this helper built as a `Vec` before lens M70, now as the array the signature takes;
    /// the region's size is the const parameter, inferred from the argument slot it fills.
    fn patt<const N: usize>(salt: usize) -> [u8; N] {
        std::array::from_fn(|i| ((i * 131 + 7 + salt) & 0xFF) as u8)
    }

    #[test]
    fn fnv1a_empty_is_basis() {
        assert_eq!(fnv1a_bytes(b""), FNV_BASIS);
    }

    #[test]
    fn fnv1a_matches_published_foobar_vector() {
        // Canonical FNV-1a-64 test vector — proves this is the standard algorithm.
        assert_eq!(fnv1a_bytes(b"foobar"), 0x8594_4171_F739_67E8);
    }

    #[test]
    fn all_zero_state_matches_oracle_byte_layout() {
        let h = StateHash::compute(
            &[0u8; VRAM_SIZE],
            &[0u8; CRAM_SIZE],
            &[0u8; VSRAM_SIZE],
            &[0u8; REG_COUNT],
        );
        assert_eq!(h.vram, 0xEB05_052E_A5B6_2325, "vram");
        assert_eq!(h.cram, 0x8421_AE12_6C7C_ED25, "cram");
        assert_eq!(h.vsram, 0xF14B_84B8_290B_8965, "vsram");
        assert_eq!(h.regs, 0x81D2_3FD7_003C_2305, "regs");
        assert_eq!(h.combined, 0xF160_1314_F59D_6B45, "combined");
    }

    #[test]
    fn distinct_per_region_pattern_pins_order_and_concatenation() {
        // Different bytes per region so a region-order or concatenation bug in `combined` is caught.
        let h = StateHash::compute(&patt(1), &patt(2), &patt(3), &patt(4));
        assert_eq!(h.vram, 0x7534_957F_70F1_2325, "vram");
        assert_eq!(h.cram, 0x9202_07A8_F1CE_8E25, "cram");
        assert_eq!(h.vsram, 0x277E_5A98_6DA7_0B35, "vsram");
        assert_eq!(h.regs, 0xDE54_078A_2CDF_8E65, "regs");
        assert_eq!(h.combined, 0xF7B4_9B14_367C_F495, "combined");
    }

    #[test]
    fn hex_matches_oracle_format() {
        assert_eq!(hex(0xCBF2_9CE4_8422_2325), "0xCBF29CE484222325");
        assert_eq!(hex(0), "0x0000000000000000");
        assert_eq!(hex(0xABC), "0x0000000000000ABC");
    }
}
