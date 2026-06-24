//! `state_hash` — FNV-1a-64 fingerprints of VDP memory + registers.
//!
//! **Byte-compatible with Oracle's `OpStateHash`** (`../oracle/linux-port/gui/ControlSocket.cpp`,
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
    pub fn compute(vram: &[u8], cram: &[u8], vsram: &[u8], regs: &[u8]) -> Self {
        debug_assert_eq!(vram.len(), VRAM_SIZE, "vram region size");
        debug_assert_eq!(cram.len(), CRAM_SIZE, "cram region size");
        debug_assert_eq!(vsram.len(), VSRAM_SIZE, "vsram region size");
        debug_assert_eq!(regs.len(), REG_COUNT, "vdp register count");

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

    fn patt(n: usize, salt: usize) -> Vec<u8> {
        (0..n)
            .map(|i| ((i * 131 + 7 + salt) & 0xFF) as u8)
            .collect()
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
        let h = StateHash::compute(
            &patt(VRAM_SIZE, 1),
            &patt(CRAM_SIZE, 2),
            &patt(VSRAM_SIZE, 3),
            &patt(REG_COUNT, 4),
        );
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
