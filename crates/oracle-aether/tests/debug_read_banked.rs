//! **F-DEBUGREAD-BANKED** — the bus-space debug read answers what the CPU sees NOW, through the cartridge
//! bank mapper and the SRAM overlay, and says which one answered (`docs/2026-09-11-debugread-banked.md`).
//!
//! Before this, `engine::debug_read` indexed the ROM image flat: with a window re-pointed, a debug read at
//! `$080000` returned the image's bank 1 whatever window 1 showed, under the region label `"cartridge ROM"`
//! — a plausible wrong answer served identically by `emulator/read`, `emulator/read_memory`,
//! `emulator/memory_hash` and the player's Memory panel, because all four are one function.
//!
//! Every test builds its image in-process. **Every expectation is DERIVED** from the image and the mapper
//! formula `bank * $80000 + (addr & $7FFFF)` — never read back through `CartBanks::rom_offset` (the shared
//! derivation under test) and never compared against `CartBanks::IDENTITY` (the mapper parcel's reset test
//! did that and stayed green with the constant mutated). And the fixture's premise — that the banks differ
//! — is asserted before anything that depends on it.

mod common;

use common::{spawn_system, Client};
use oracle_aether::engine;
use oracle_aether::rpc::code;
use oracle_core::m68000::bus68k::Bus68k;
use oracle_core::system::System;
use serde_json::{json, Value};

const BANK: usize = 0x8_0000;
const BANKS: usize = 10;

/// The image byte at offset `i` names both its bank and its in-bank offset: `label(bank) ^ mix(offset)`.
/// Two banks at the same in-bank offset always differ (XOR by the same `mix` is a bijection on the label),
/// and neighbouring offsets in one bank differ, so both a window error and an in-window offset error move
/// the value.
fn image() -> Vec<u8> {
    (0..BANKS * BANK)
        .map(|i| {
            let o = i % BANK;
            ((i / BANK) as u8 ^ 0x5A) ^ ((o ^ (o >> 8) ^ (o >> 16)) as u8)
        })
        .collect()
}

/// The image offset the mapper formula gives for `a` when its window shows `bank`. Written out, not
/// delegated to `rom_offset`: the ruler must not be the thing being measured.
fn derived(bank: u8, a: u32) -> usize {
    usize::from(bank) * BANK + (a as usize & (BANK - 1))
}

/// The label a banked window's reads report. Derived from the bank number, not copied from the server.
fn bank_label(bank: u8) -> String {
    format!("cartridge ROM bank {bank}")
}

/// A reset machine over [`image`] with each `(window, bank)` written through the **real bus write path**
/// (the odd byte `$A130F1 + 2k`), after the reset — a reset re-seeds the identity mapping.
fn machine(remap: &[(usize, u8)]) -> System {
    let mut sys = System::new(0xDB6);
    sys.load_rom(image());
    sys.reset();
    for &(k, bank) in remap {
        sys.mega_bus(&mut ())
            .write8(0xA1_30F1 + 2 * k as u32, 5, bank);
    }
    for &(k, bank) in remap {
        assert_eq!(
            sys.cart_banks().bank(k),
            bank,
            "the remap of window {k} did not land"
        );
    }
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

fn bytes_of(v: &Value) -> Vec<u8> {
    let s = v["bytes"].as_str().expect("a `bytes` hex string");
    let d = s.trim_start_matches("0x");
    (0..d.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&d[i..i + 2], 16).unwrap())
        .collect()
}

/// Addresses sampled in window `k`: both ends, the second byte, an interior point.
fn samples(k: usize) -> [u32; 4] {
    let base = (k * BANK) as u32;
    [base, base + 1, base + 0x1_2345, base + 0x7_FFFF]
}

#[test]
fn the_fixture_banks_differ_before_anything_else() {
    let img = image();
    for o in [0usize, 1, 0x1_2345, BANK - 1] {
        for b1 in 0..BANKS {
            for b2 in (b1 + 1)..BANKS {
                assert_ne!(
                    img[b1 * BANK + o],
                    img[b2 * BANK + o],
                    "banks {b1} and {b2} agree at in-bank offset {o:#X}: identity and remapped reads \
                     would look alike and every test in this file would be vacuous"
                );
            }
        }
    }
}

/// Every live window re-pointed at once, window `k` -> bank `k + 2` (3..=9: all distinct, none identity).
/// Moving all seven together is what exposes a window off-by-one: window `k` reading its neighbour's bank
/// reads `k + 1` or `k + 3`, never the `k + 2` asserted here.
#[test]
fn every_remapped_window_debug_reads_the_bank_it_shows() {
    let img = image();
    let remap: Vec<(usize, u8)> = (1..8).map(|k| (k, (k + 2) as u8)).collect();
    let mut sys = machine(&remap);
    for k in 0..8 {
        let bank = if k == 0 { 0 } else { (k + 2) as u8 };
        for a in samples(k) {
            let off = derived(bank, a);
            let (bytes, region) = engine::debug_read(&sys, a, 1)
                .unwrap_or_else(|e| panic!("{a:#08X} refused: {}", e.message));
            assert_eq!(
                bytes,
                vec![img[off]],
                "{a:#08X}: window {k} shows bank {bank}, so a debug read is image[{off:#X}]"
            );
            if k == 0 {
                assert_eq!(
                    region.to_string(),
                    "cartridge ROM",
                    "window 0 is fixed and unbanked"
                );
            } else {
                assert_ne!(
                    img[off], img[a as usize],
                    "{a:#08X}: the banked byte equals the flat one — this sample proves nothing"
                );
                assert_eq!(region.to_string(), bank_label(bank), "{a:#08X}: provenance");
            }
            // The parity half: what the CPU reads at the same address.
            assert_eq!(
                sys.mega_bus(&mut ()).read8(a, 6).0,
                img[off],
                "{a:#08X}: the debug read and a CPU read disagree"
            );
        }
        // A multi-byte read inside the window is the contiguous slice of the bank it shows.
        let base = (k * BANK) as u32 + 0x100;
        let (bytes, _) = engine::debug_read(&sys, base, 64).expect("inside one window");
        let off = derived(bank, base);
        assert_eq!(bytes, img[off..off + 64], "window {k}: a 64-byte read");
    }
}

/// `read`, `read_memory` and `memory_hash` over the wire: one answer, and it is the derived one — not merely
/// three surfaces that agree with each other (they share one function, so their agreement alone is blind to
/// a defect in it).
#[test]
fn three_wire_surfaces_agree_on_a_banked_window() {
    let img = image();
    let sys = machine(&[(1, 9), (2, 8)]);
    let direct = engine::debug_read(&sys, 0x08_0040, 32).expect("readable").0;
    let h = spawn_system("drb-three", sys, 64);
    let mut c = client(&h);
    let a = "0x00080040";
    let rm = c.ok("emulator/read_memory", json!({"addr": a, "len": 32}));
    let rd = c.ok(
        "emulator/read",
        json!({"space": "bus", "addr": a, "len": 32}),
    );
    let mh = c.ok("emulator/memory_hash", json!({"addr": a, "len": 32}));

    let off = derived(9, 0x08_0040);
    let want = &img[off..off + 32];
    assert_ne!(
        want,
        &img[0x08_0040..0x08_0040 + 32],
        "the fixture cannot tell bank 9 from bank 1"
    );
    assert_eq!(
        bytes_of(&rm),
        want,
        "read_memory is the bank the window shows"
    );
    assert_eq!(bytes_of(&rd), want, "read is the bank the window shows");
    assert_eq!(direct, want, "the in-process function the panel calls");
    assert_eq!(
        mh["crc32"],
        json!(format!("0x{:08X}", oracle_aether::crc32::crc32(want))),
        "memory_hash hashes the bytes the CPU sees"
    );
    assert_eq!(
        mh["fnv1a64"],
        json!(oracle_core::state_hash::hex(
            oracle_core::state_hash::fnv1a_bytes(want)
        ))
    );
    for (name, r) in [("read_memory", &rm), ("read", &rd), ("memory_hash", &mh)] {
        assert_eq!(
            r["region"],
            json!(bank_label(9)),
            "{name}: provenance names the bank"
        );
    }
    // A banked answer is weaker than it looks to anyone who takes the address for an image offset, so the
    // two read rows say so; memory_hash's fragment declares no caveat, and its region carries it instead.
    for (name, r) in [("read_memory", &rm), ("read", &rd)] {
        let cav = r["caveat"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: a banked read carries a caveat"));
        assert!(
            cav.contains("bank 9"),
            "{name}: the caveat names the bank: {cav}"
        );
    }
    assert!(
        mh.get("caveat").is_none(),
        "memory_hash's fragment declares caveat absent"
    );
}

/// Under the identity mapping (every non-banking cartridge, and every cartridge at reset) nothing a client
/// can observe moves: the `"cartridge ROM"` spelling, the image bytes, and the contract's image promise —
/// a cartridge-window hash equals CRC32 over the same slice of the ROM file, here across all eight windows.
/// And the read rows no longer carry a constant caveat (§2.4's advisory named `read_memory`'s as the shape
/// to avoid).
#[test]
fn an_unbanked_cartridge_answers_exactly_as_before() {
    let img = image();
    let h = spawn_system("drb-identity", machine(&[]), 64);
    let mut c = client(&h);
    let rm = c.ok(
        "emulator/read_memory",
        json!({"addr": "0x00080000", "len": 16}),
    );
    let rd = c.ok("emulator/read", json!({"addr": "0x00080000", "len": 16}));
    for (name, r) in [("read_memory", &rm), ("read", &rd)] {
        assert_eq!(r["region"], json!("cartridge ROM"), "{name}");
        assert_eq!(
            bytes_of(r),
            &img[0x08_0000..0x08_0010],
            "{name}: the flat image bytes"
        );
        assert!(
            r.get("caveat").is_none(),
            "{name}: an unbanked cartridge read is exactly what the CPU sees; no caveat: {r}"
        );
    }
    let mh = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0x000000", "len": 0x40_0000}),
    );
    assert_eq!(mh["region"], json!("cartridge ROM"));
    assert_eq!(
        mh["crc32"],
        json!(format!(
            "0x{:08X}",
            oracle_aether::crc32::crc32(&img[..0x40_0000])
        )),
        "under identity, a whole-cartridge-space hash is CRC32 over the file's first 4 MiB"
    );
}

/// One region per read. A range from an unbanked window into a banked one would be two provenances under
/// one label, so it is refused (`-32004`, never stitched) — and the same range under identity is one
/// region and answers, which is what makes the refusal a statement about the mapping.
#[test]
fn a_read_may_not_straddle_into_a_remapped_window() {
    let identity = machine(&[]);
    assert!(
        engine::debug_read(&identity, 0x07_FFFE, 4).is_ok(),
        "unbanked, the range is one region"
    );
    let banked = machine(&[(1, 9)]);
    let e = engine::debug_read(&banked, 0x07_FFFE, 4).expect_err("window 0 into banked window 1");
    assert_eq!(e.code, code::ADDRESS_OUT_OF_RANGE, "{}", e.message);
    let h = spawn_system("drb-straddle", banked, 64);
    let mut c = client(&h);
    let e = c.err(
        "emulator/memory_hash",
        json!({"addr": "0x000000", "len": 0x40_0000}),
    );
    assert_eq!(
        e["code"],
        json!(-32004),
        "a whole-space hash straddles the banked window"
    );
}

/// SRAM answers first (window 4 shares its span), and its span is its own region while it is mapped in.
#[test]
fn the_sram_overlay_answers_first_and_is_its_own_region() {
    let img = image();
    let mut sys = machine(&[(4, 9)]);
    let save: Vec<u8> = (0..64u8).map(|i| i.wrapping_mul(37) ^ 0xC3).collect();
    sys.load_sram(&save);

    // Latch off: window 4's bank answers at the SRAM lane.
    let (b, r) = engine::debug_read(&sys, 0x20_0001, 1).unwrap();
    assert_eq!(b, vec![img[derived(9, 0x20_0001)]]);
    assert_eq!(r.to_string(), bank_label(9));

    sys.mega_bus(&mut ()).write8(0xA1_30F1, 5, 1); // $A130F1 bit0: SRAM mapped in
    let w = sys.sram_window().expect("mapped in");
    assert_eq!(
        (w.base, w.odd),
        (0x20_0001, true),
        "the fixture's SRAM page moved"
    );
    assert_ne!(
        save[0],
        img[derived(9, 0x20_0001)],
        "precedence would be unobservable"
    );

    let (b, r) = engine::debug_read(&sys, 0x20_0001, 1).unwrap();
    assert_eq!(b, vec![save[0]], "the SRAM byte, not the ROM under it");
    assert_eq!(r.to_string(), "cartridge SRAM");

    // A multi-byte read inside the span is what the CPU reads byte for byte: the chip's lane is the save
    // RAM, the other lane falls through to window 4's bank.
    let (b, r) = engine::debug_read(&sys, 0x20_0001, 8).unwrap();
    let cpu: Vec<u8> = (0..8u32)
        .map(|i| sys.mega_bus(&mut ()).read8(0x20_0001 + i, 6).0)
        .collect();
    assert_eq!(
        b, cpu,
        "the debug read and the CPU disagree inside the SRAM span"
    );
    let want: Vec<u8> = (0..8usize)
        .map(|i| {
            if i % 2 == 0 {
                save[i / 2]
            } else {
                img[derived(9, 0x20_0001 + i as u32)]
            }
        })
        .collect();
    assert_eq!(b, want, "derived: SRAM on the odd lane, bank 9 on the even");
    assert_eq!(r.to_string(), "cartridge SRAM");

    // Outside the span, window 4 again; and a range crossing either edge of the span is refused.
    let (b, r) = engine::debug_read(&sys, 0x20_0000, 1).unwrap();
    assert_eq!(b, vec![img[derived(9, 0x20_0000)]]);
    assert_eq!(r.to_string(), bank_label(9));
    for (a, len) in [(0x20_0000u32, 2usize), (0x20_FFFE, 4)] {
        let e = engine::debug_read(&sys, a, len).expect_err("crosses an edge of the SRAM span");
        assert_eq!(
            e.code,
            code::ADDRESS_OUT_OF_RANGE,
            "{a:#08X}: {}",
            e.message
        );
    }
}

/// Cartridge space ends at `$3FFFFF` whatever the image's length: past it the CPU reads open bus, so a
/// debug read there is refused, even though a 5 MiB image has bytes at those offsets. The bytes of banks 8
/// and 9 are reached the way the CPU reaches them — through a re-pointed window.
#[test]
fn cartridge_space_ends_at_3fffff_even_when_the_image_does_not() {
    let sys = machine(&[]);
    assert!(
        sys.rom().len() > 0x40_0000,
        "the image must reach past $3FFFFF for this to mean anything"
    );
    for (a, len) in [(0x40_0000u32, 1usize), (0x3F_FFFE, 4)] {
        let e = engine::debug_read(&sys, a, len).expect_err("not cartridge space");
        assert_eq!(
            e.code,
            code::ADDRESS_OUT_OF_RANGE,
            "{a:#08X}: {}",
            e.message
        );
    }
    let h = spawn_system("drb-past4m", sys, 64);
    let mut c = client(&h);
    let e = c.err("emulator/read_memory", json!({"addr": "0x400000"}));
    assert_eq!(e["code"], json!(-32004));
}

/// A window pointed at a bank the image does not have reads open bus on the CPU; a debug read refuses
/// rather than inventing a byte.
#[test]
fn a_window_pointed_past_the_image_is_refused_not_invented() {
    let sys = machine(&[(3, 20)]);
    let e = engine::debug_read(&sys, 0x18_0000, 1).expect_err("bank 20 of 10");
    assert_eq!(e.code, code::ADDRESS_OUT_OF_RANGE, "{}", e.message);
}

/// Lens finding L2: `debug_read` is `pub` and cross-crate, and its `len == 0` domain restriction lived only
/// in its callers — `end = addr + len - 1` underflowed at `addr 0`. Every served caller bounds `len >= 1`
/// before reaching it; the function itself now refuses rather than relying on that.
#[test]
fn a_zero_length_read_is_refused_not_underflowed() {
    let sys = machine(&[]);
    for a in [0u32, 0x00FF_0000] {
        let e = engine::debug_read(&sys, a, 0).expect_err("zero bytes has no region and no answer");
        assert_eq!(e.code, code::INVALID_PARAMS, "{a:#08X}: {}", e.message);
    }
}
