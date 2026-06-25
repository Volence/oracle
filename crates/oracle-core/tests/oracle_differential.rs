//! Live-Oracle differential — proves `oracle-core`'s `state_hash` is byte-for-byte compatible with
//! Oracle's `OpStateHash` using **real VDP state captured from the running Oracle** (Exodus, server
//! `"2.1-linux"`).
//!
//! Capture: the s4 engine ROM, `emulator/reset` then `emulator/run_frames(30)`, 2026-06-24. Oracle
//! computed the sub-hashes below with its own C++ FNV-1a; this test feeds the *same captured bytes*
//! through `oracle-core` and asserts they match. CRAM/VSRAM/REGS are fully captured here. VRAM (64 KiB)
//! and `combined` use the identical FNV-1a code path (pinned by the unit goldens in `state_hash.rs`,
//! including the per-region ordering test); a full live VRAM/`combined` capture is left as future
//! hardening for the `oracle-bus` differential harness, which can pull whole-state cheaply.
//!
//! The data is captured, so this test needs neither a network nor a running Oracle — it always runs.

use oracle_core::state_hash::fnv1a_bytes;

/// VDP control registers 0..=23 (raw bytes), exactly as Oracle's `read_vdp_registers` returned them.
const REGS: [u8; 24] = [
    0x04, 0x34, 0x30, 0x3C, 0x07, 0x5C, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x02, 0x81, 0x2F, 0x00, 0x02,
    0x11, 0x00, 0x00, 0x00, 0x00, 0xBD, 0xC2, 0x7F,
];
const ORACLE_REGS_HASH: u64 = 0x40E9_6BAB_1A5B_F5BC;

/// CRAM: 64 colour words (4 lines × 16). Stored **big-endian** (high byte first) in the VDP buffer —
/// determined empirically against Oracle's reported `cram` hash (big-endian matched, little-endian did not).
const CRAM_WORDS: [u16; 64] = [
    0x0000, 0x0222, 0x0822, 0x0C42, 0x0ECC, 0x0E66, 0x0EEE, 0x0CAA, 0x0866, 0x0444, 0x08AE, 0x046A,
    0x000E, 0x0008, 0x00AE, 0x008E, 0x0000, 0x0000, 0x0E62, 0x0A86, 0x0E86, 0x0044, 0x0EEE, 0x0AAA,
    0x0888, 0x0444, 0x0666, 0x0E86, 0x00EE, 0x0088, 0x0EA8, 0x0ECA, 0x0000, 0x0002, 0x0800, 0x0224,
    0x0248, 0x026A, 0x048C, 0x06AE, 0x0000, 0x0020, 0x0240, 0x0460, 0x04A0, 0x0482, 0x02C6, 0x06EA,
    0x0000, 0x0000, 0x0240, 0x0460, 0x0680, 0x0202, 0x0624, 0x0828, 0x0848, 0x08A6, 0x0020, 0x0C8C,
    0x0C0E, 0x0E4E, 0x0E8E, 0x0ECE,
];
const ORACLE_CRAM_HASH: u64 = 0x22B1_0C22_23B1_6749;

/// VSRAM was all-zero (40 words = 80 bytes) in the captured state.
const ORACLE_VSRAM_HASH: u64 = 0xF14B_84B8_290B_8965;

#[test]
fn regs_hash_matches_live_oracle() {
    assert_eq!(fnv1a_bytes(&REGS), ORACLE_REGS_HASH);
}

#[test]
fn cram_hash_matches_live_oracle() {
    let mut bytes = Vec::with_capacity(128);
    for w in CRAM_WORDS {
        bytes.push((w >> 8) as u8); // big-endian: high byte first
        bytes.push((w & 0xFF) as u8);
    }
    assert_eq!(bytes.len(), 128);
    assert_eq!(fnv1a_bytes(&bytes), ORACLE_CRAM_HASH);
}

#[test]
fn vsram_all_zero_hash_matches_live_oracle() {
    let vsram = [0u8; 80];
    assert_eq!(fnv1a_bytes(&vsram), ORACLE_VSRAM_HASH);
}
