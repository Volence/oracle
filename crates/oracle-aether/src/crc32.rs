//! CRC-32, IEEE 802.3 polynomial (the zlib/`crc32` one) — so a cartridge-window
//! `emulator/memory_hash` equals CRC32 over the same slice of the ROM file, which is what the
//! Aeon side's gates compare against. Table-driven, dependency-free, built at compile time.
//!
//! NOT in `oracle-core::state_hash`: that module is byte-compatible with Oracle's `OpStateHash`
//! and carries a do-not-touch warning; this is a bus convenience with a different job.
//!
//! [`crate::png`] deliberately keeps its own private streaming `Crc32` rather than calling this one,
//! and that is a decision rather than an oversight: it privately owns its deflate and its `adler32`
//! too, because a self-contained encoder is easier to trust than one wired through three modules —
//! and it needs an incremental `update` across chunk type + data, which this one-shot signature does
//! not offer. Both are pinned to the same outside-world check value (`0xCBF43926`), so they cannot
//! drift apart silently. Consolidate only if a third consumer appears.

const fn table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

const TABLE: [u32; 256] = table();

/// CRC-32 over a byte slice (init `0xFFFFFFFF`, reflected, final XOR — the zlib convention).
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::crc32;

    /// The standard check value every CRC-32 implementation must produce (ITU/zlib test vector) —
    /// an expectation from OUTSIDE this codebase, so it cannot be self-confirming.
    #[test]
    fn the_check_vector_holds() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(crc32(b""), 0);
    }
}
