//! `emulator/memory_hash` — `protocol.md` §6 (memory), adopted as CR-23 / §11.13
//! (`docs/2026-08-18-cr21-23-tier1-rows.md`, `docs/2026-08-18-ruling-cr21-23.md`).
//! The gap `state_hash` leaves: nothing else on this bus hashes 68000 memory.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::json;

fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// The suite's established symbol fixture, verbatim from `tests/methods.rs`'s `LST_UNBOUND`: no
/// `EndOfRom` row, so it loads *unverified* with a caveat rather than being refused. `EntryPoint`
/// sits at `$200`, inside `testrom::build()`'s `$300` image — a real address in the fixture ROM, not
/// a number aimed at nowhere.
const LST: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 Player_1 : FFFF8CFA C |

    2 symbols
    0 unused symbols
";

fn write_lst(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ae-{tag}-{}.lst", std::process::id()));
    std::fs::write(&p, LST).unwrap();
    p
}

/// The hash agrees with hashing the SAME range fetched through the independent `read` path —
/// poked to known bytes first so the input is controlled, not incidental.
#[test]
fn the_hash_matches_the_bytes_read_back() {
    let h = spawn_system("mh-agree", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0500", "bytes": "0x0123456789ABCDEF"}),
    );
    let r = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0xFF0500", "len": 8}),
    );
    assert_eq!(r["region"], json!("work RAM"));
    assert_eq!(r["len"], json!(8));
    assert_eq!(r["addr"], json!("0x00FF0500"));
    // Independent derivations: the algorithm modules' own fns over the known payload.
    let payload = [0x01u8, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];
    assert_eq!(
        r["fnv1a64"],
        json!(oracle_core::state_hash::hex(
            oracle_core::state_hash::fnv1a_bytes(&payload)
        ))
    );
    assert_eq!(
        r["crc32"],
        json!(format!("0x{:08X}", oracle_aether::crc32::crc32(&payload)))
    );
    // And the bytes the hash claims to cover are the bytes the independent read path returns.
    let back = c.ok("emulator/read", json!({"addr": "0xFF0500", "len": 8}));
    assert_eq!(back["bytes"], json!("0x0123456789ABCDEF"));
}

/// A cartridge-window hash equals CRC32 over the same slice of the ROM image — the property the
/// legacy MCP row promises and the Aeon gates rely on.
#[test]
fn a_cart_window_hash_matches_the_rom_slice() {
    let sys = machine();
    let expect = format!(
        "0x{:08X}",
        oracle_aether::crc32::crc32(&sys.rom()[0x100..0x200])
    );
    let h = spawn_system("mh-rom", sys, 64);
    let mut c = client(&h);
    let r = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0x00000100", "len": 256}),
    );
    assert_eq!(r["region"], json!("cartridge ROM"));
    assert_eq!(r["crc32"], json!(expect));
}

/// `len` is required, bounded, and a range past the region end is -32004 refused never clipped.
#[test]
fn bounds_and_the_required_len() {
    let h = spawn_system("mh-bounds", machine(), 64);
    let mut c = client(&h);
    let e = c.err("emulator/memory_hash", json!({"addr": "0xFF0000"}));
    assert_eq!(e["code"], json!(-32602), "len is required");
    let e = c.err(
        "emulator/memory_hash",
        json!({"addr": "0xFF0000", "len": 4_194_305}),
    );
    assert_eq!(e["code"], json!(-32602), "len above limits.maxHashLen");
    let e = c.err(
        "emulator/memory_hash",
        json!({"addr": "0x00FFFFFF", "len": 2}),
    );
    assert_eq!(
        e["code"],
        json!(-32004),
        "end past the window: refused, never clipped"
    );
}

/// A pure read: answers a free-running machine (like emulator/read, unlike write_memory).
#[test]
fn it_answers_a_free_running_machine() {
    let h = spawn_system("mh-freerun", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));
    let r = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0xFF0000", "len": 16}),
    );
    assert!(r.get("fnv1a64").is_some());
}

/// The FNV parameters on the wire are the contract's, not whatever the core happens to hold: §6 pins
/// the offset basis and the prime by value, so a silent drift in either becomes a red suite here
/// rather than two servers that quietly never agree.
#[test]
fn the_fnv_parameters_are_the_contracts() {
    assert_eq!(
        oracle_core::state_hash::FNV_BASIS,
        0xCBF2_9CE4_8422_2325,
        "FNV-1a-64 offset basis, pinned by protocol.md §6"
    );
    assert_eq!(
        oracle_core::state_hash::FNV_PRIME,
        0x0000_0100_0000_01B3,
        "FNV-1a-64 prime, pinned by protocol.md §6"
    );
    // The one vector derivable by hand: folding nothing leaves the basis untouched.
    assert_eq!(
        oracle_core::state_hash::fnv1a_bytes(b""),
        oracle_core::state_hash::FNV_BASIS
    );
}

/// The work-RAM window is mirror-masked, so an address in the mirror hashes the same 64 KiB of RAM
/// as its canonical alias — the routing rule made observable rather than asserted.
#[test]
fn a_mirror_window_hash_equals_its_canonical_alias() {
    let h = spawn_system("mh-mirror", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0500", "bytes": "0x000102030405060708090A0B0C0D0E0F"}),
    );
    let canonical = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0xFF0500", "len": 16}),
    );
    let mirror = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0xE00500", "len": 16}),
    );
    assert_eq!(mirror["region"], json!("work RAM"));
    assert_eq!(mirror["fnv1a64"], canonical["fnv1a64"]);
    assert_eq!(mirror["crc32"], canonical["crc32"]);
    // The reply still reports the address that was asked for, not the masked one.
    assert_eq!(mirror["addr"], json!("0x00E00500"));
}

/// The `symbol` spelling is the schema's other `oneOf` alternative, and D7 is the reason it exists:
/// clients resolve names, they never hardcode a RAM literal. Hashing by name must therefore land on
/// exactly the range hashing by address lands on — the same two fingerprints, the resolved address
/// echoed back — or symbol-first addressing is a promise this row does not keep.
#[test]
fn the_symbol_spelling_hashes_the_same_range_as_the_address() {
    let h = spawn_system("mh-symbol", machine(), 64);
    let mut c = client(&h);
    let lst = write_lst("mh-symbol");
    c.ok(
        "emulator/load_symbols",
        json!({"path": lst.display().to_string()}),
    );

    let by_name = c.ok(
        "emulator/memory_hash",
        json!({"symbol": "EntryPoint", "len": 16}),
    );
    let by_addr = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0x00000200", "len": 16}),
    );

    // The name resolved to the address the fixture declares, and says so on the wire.
    assert_eq!(by_name["addr"], json!("0x00000200"));
    assert_eq!(by_name["region"], json!("cartridge ROM"));
    assert_eq!(by_name["len"], json!(16));
    // Same range, so the same two fingerprints — not merely "both present".
    assert_eq!(by_name["fnv1a64"], by_addr["fnv1a64"]);
    assert_eq!(by_name["crc32"], by_addr["crc32"]);
    // The agreement above is only worth something if these fingerprints discriminate at all: a
    // neighbouring window of the same length must NOT match, or two ranges agreeing would prove
    // nothing about which range was hashed.
    let elsewhere = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0x00000210", "len": 16}),
    );
    assert_ne!(
        by_name["fnv1a64"], elsewhere["fnv1a64"],
        "a different 16 bytes must hash differently, or the equality above is vacuous"
    );
    assert_ne!(by_name["crc32"], elsewhere["crc32"]);

    // The unknown-name refusal keeps its own code, so a client can tell "no such symbol" from
    // "no symbols loaded" and from a malformed param.
    let e = c.err(
        "emulator/memory_hash",
        json!({"symbol": "Nope_Nope", "len": 16}),
    );
    assert_eq!(e["code"], json!(-32013));
}

/// §8 item 20 closure, asserted locally: the key set is exact.
#[test]
fn the_key_set_is_exact() {
    use std::collections::BTreeSet;
    let h = spawn_system("mh-keys", machine(), 64);
    let mut c = client(&h);
    let r = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0xFF0000", "len": 4}),
    );
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    let want: BTreeSet<&str> = [
        "addr",
        "len",
        "region",
        "fnv1a64",
        "crc32",
        "frame",
        "mclk",
        "running",
        "droppedEvents",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
}
