//! Golden-layout test for the frozen `export_state` image (integration pivot D8) — now at **v2** after the
//! SRAM go-live (a 64 KiB SRAM tail region was appended; §v2 in `docs/export-state-v1.md`).
//!
//! This is the anti-drift guard: a known seed + the vendored test ROM + a fixed number of frames produces
//! a byte-exact image whose total length, per-region offsets/sizes, and `export_state_hash` are all pinned
//! here. Any accidental layout change (a region resized, reordered, added, or removed) is a **hard test
//! failure**, not a silent version break. A *deliberate* layout change must bump `EXPORT_STATE_VERSION` and
//! regenerate the golden constants below in the same commit.
//!
//! The region offsets are written as independent literals from `docs/export-state-v1.md`, NOT recomputed
//! from the production constants, so a constant typo that still sums to the same total is still caught.
//!
//! Spec: `docs/export-state-v1.md`.

use oracle_core::system::System;

/// Fixed fixture inputs. Changing any of these changes the golden hash — regenerate deliberately.
const SEED: u64 = 0xD8_5EED_0F1C_ED01;
const FRAMES: u64 = 60;

/// Frozen v2 region offsets (byte, little-endian image). Independent of the production arithmetic.
/// Offsets 0–7 are unchanged from v1; region 8 (SRAM) is the new 64 KiB tail added at the go-live slice.
const OFF_VERSION: usize = 0x00000;
const OFF_M68K_REGS: usize = 0x00002;
const OFF_WORK_RAM: usize = 0x00050;
const OFF_Z80_RAM: usize = 0x10050;
const OFF_Z80_REGS: usize = 0x12050;
const OFF_VDP: usize = 0x12090;
const OFF_FM: usize = 0x22178;
const OFF_PSG: usize = 0x22378;
const OFF_SRAM: usize = 0x22388;
const TOTAL_LEN: usize = 0x32388; // 205704 = old 0x22388 + 0x10000 SRAM tail

/// Region sizes (bytes).
const SZ_M68K_REGS: usize = 78;
const SZ_WORK_RAM: usize = 0x10000;
const SZ_Z80_RAM: usize = 0x2000;
const SZ_Z80_REGS: usize = 0x40;
const SZ_VDP: usize = 0x100E8; // VRAM 0x10000 + CRAM 0x80 + VSRAM 0x50 + regs 24
const SZ_FM: usize = 0x200;
const SZ_PSG: usize = 0x10;
const SZ_SRAM: usize = 0x10000; // fixed 64 KiB tail; all-zero for this no-SRAM fixture ROM

/// The byte-exact `export_state_hash` of the fixture below. Pinned from a green run; a drift makes it fail.
/// Regenerated at the SRAM go-live (v1→v2): the image gained a 64 KiB all-zero tail (this fixture ROM has no
/// "RA" header → empty SRAM buffer → an all-zero region). The prior v1 value was `0x22F8_0ECF_29ED_3AD4`.
const GOLDEN_HASH: u64 = 0xBF5D_1E1A_A727_143B;

/// Boot the machine, run the vendored ROM for a fixed number of frames, and return the `export_state` image.
fn fixture() -> System {
    let mut sys = System::new(SEED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys.run_frames(FRAMES);
    sys
}

#[test]
fn v1_total_length_and_region_boundaries_are_frozen() {
    let sys = fixture();
    let img = sys.export_state();

    // Total length + the offset ladder are contiguous and gap-free.
    assert_eq!(img.len(), TOTAL_LEN, "export_state v1 total length");
    assert_eq!(OFF_M68K_REGS, OFF_VERSION + 2, "version region is 2 bytes");
    assert_eq!(
        OFF_WORK_RAM,
        OFF_M68K_REGS + SZ_M68K_REGS,
        "m68k regs region"
    );
    assert_eq!(OFF_Z80_RAM, OFF_WORK_RAM + SZ_WORK_RAM, "work RAM region");
    assert_eq!(OFF_Z80_REGS, OFF_Z80_RAM + SZ_Z80_RAM, "Z80 RAM region");
    assert_eq!(OFF_VDP, OFF_Z80_REGS + SZ_Z80_REGS, "Z80 regs region");
    assert_eq!(OFF_FM, OFF_VDP + SZ_VDP, "VDP region");
    assert_eq!(OFF_PSG, OFF_FM + SZ_FM, "FM region");
    assert_eq!(OFF_SRAM, OFF_PSG + SZ_PSG, "PSG region");
    assert_eq!(TOTAL_LEN, OFF_SRAM + SZ_SRAM, "SRAM region ends the image");
}

#[test]
fn v1_region_semantics_are_frozen() {
    let sys = fixture();
    let img = sys.export_state();

    // Region 0 — version: little-endian u16 == 1.
    assert_eq!(
        u16::from_le_bytes([img[OFF_VERSION], img[OFF_VERSION + 1]]),
        oracle_core::system::EXPORT_STATE_VERSION,
        "version field"
    );
    assert_eq!(
        oracle_core::system::EXPORT_STATE_VERSION,
        2,
        "this golden fixture pins v2 (after the SRAM go-live)"
    );

    // Region 1 — m68k regs: matches the live register file, little-endian.
    let r = sys.cpu_regs();
    let mut regs = Vec::new();
    for v in r.d {
        regs.extend_from_slice(&v.to_le_bytes());
    }
    for v in r.a {
        regs.extend_from_slice(&v.to_le_bytes());
    }
    regs.extend_from_slice(&r.usp.to_le_bytes());
    regs.extend_from_slice(&r.ssp.to_le_bytes());
    regs.extend_from_slice(&r.pc.to_le_bytes());
    regs.extend_from_slice(&r.sr.to_le_bytes());
    regs.extend_from_slice(&r.prefetch[0].to_le_bytes());
    regs.extend_from_slice(&r.prefetch[1].to_le_bytes());
    assert_eq!(regs.len(), SZ_M68K_REGS, "regs region is 78 bytes");
    assert_eq!(
        &img[OFF_M68K_REGS..OFF_M68K_REGS + SZ_M68K_REGS],
        &regs[..],
        "m68k regs region mirrors the register file"
    );

    // Region 2 — work RAM: mirrors System RAM (the ROM stirs it, so it is non-trivially non-zero).
    assert_eq!(
        &img[OFF_WORK_RAM..OFF_WORK_RAM + SZ_WORK_RAM],
        sys.ram(),
        "work RAM region mirrors System RAM"
    );
    assert!(
        img[OFF_WORK_RAM..OFF_WORK_RAM + SZ_WORK_RAM]
            .iter()
            .any(|&b| b != 0),
        "the ROM's RAM-stirring loop left non-zero work RAM"
    );

    // Region 3 — Z80 RAM: this ROM never writes $A00000, so it stays zero here (liveness is covered by the
    // `export_state_captures_live_z80_ram` unit test).
    assert!(
        img[OFF_Z80_RAM..OFF_Z80_RAM + SZ_Z80_RAM]
            .iter()
            .all(|&b| b == 0),
        "Z80 RAM region (untouched by this ROM)"
    );

    // Regions 4, 6, 7 — still reserved, all zero: Z80 regs, FM, PSG.
    // Region 8 (SRAM) — this fixture ROM has no "RA" header, so its SRAM buffer is empty and the fixed
    // 64 KiB tail region is all zero. (Liveness of a written SRAM cell is covered by the system.rs unit test
    // `export_state_sram_region_is_live_left_justified_and_zero_padded`.)
    for (name, off, sz) in [
        ("Z80 regs", OFF_Z80_REGS, SZ_Z80_REGS),
        ("FM", OFF_FM, SZ_FM),
        ("PSG", OFF_PSG, SZ_PSG),
        ("SRAM", OFF_SRAM, SZ_SRAM),
    ] {
        assert!(
            img[off..off + sz].iter().all(|&b| b == 0),
            "reserved {name} region is all zero"
        );
    }

    // Region 5 — VDP: now LIVE (the v1 content change). Mirrors the four Oracle-hashed regions in the
    // state_hash order VRAM → CRAM → VSRAM → regs, at unchanged offset/size.
    let mut vdp_bytes = Vec::with_capacity(SZ_VDP);
    vdp_bytes.extend_from_slice(sys.vdp().vram());
    vdp_bytes.extend_from_slice(sys.vdp().cram());
    vdp_bytes.extend_from_slice(sys.vdp().vsram());
    vdp_bytes.extend_from_slice(sys.vdp().regs());
    assert_eq!(vdp_bytes.len(), SZ_VDP, "VDP region size");
    assert_eq!(
        &img[OFF_VDP..OFF_VDP + SZ_VDP],
        &vdp_bytes[..],
        "VDP region mirrors VRAM/CRAM/VSRAM/regs"
    );
    assert!(
        img[OFF_VDP..OFF_VDP + SZ_VDP].iter().any(|&b| b != 0),
        "the VDP region is live (the power-on VRAM seed is non-zero)"
    );
}

#[test]
fn v1_golden_hash_is_frozen() {
    let sys = fixture();
    assert_eq!(
        sys.export_state_hash(),
        GOLDEN_HASH,
        "export_state v1 golden hash drifted — if this was a deliberate layout change, bump \
         EXPORT_STATE_VERSION and regenerate GOLDEN_HASH in the same commit"
    );
}
