//! `emulator/read` — `protocol.md` §6 (memory), adopted as CR-20
//! (`docs/2026-08-16-cr20-unified-read.md`, ruled in `docs/2026-08-16-ruling-cr20.md`, §11.12).
//!
//! Every test is a wire round trip, so each reply is validated against the vendored contract schema on the
//! way past — which for this row is doing real work: the fragment enforces `region`/`symbol`/`symbolDisp`
//! **iff** `space` is `bus`, in both directions, so a reply that leaked `region` into a VRAM read would be
//! rejected by the schema before any assertion here ran.
//!
//! The adoption condition is the shape of the file: **one closed reply per space, plus one refusal per
//! bound.**

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::{json, Value};

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

/// Every space answers, and the reply is self-describing — `addr` means nothing without its space.
#[test]
fn every_space_answers_and_echoes_which_one() {
    let h = spawn_system("read-spaces", machine(), 64);
    let mut c = client(&h);
    for (space, addr) in [
        ("bus", "0x00FF0000"),
        ("vram", "0x00000100"),
        ("cram", "0x00000010"),
        ("vsram", "0x00000004"),
    ] {
        let r = c.ok(
            "emulator/read",
            json!({"space": space, "addr": addr, "len": 4}),
        );
        assert_eq!(r["space"], json!(space), "the reply names its own space");
        assert_eq!(r["len"], json!(4));
        let bytes = r["bytes"].as_str().unwrap();
        assert_eq!(
            bytes.trim_start_matches("0x").len(),
            8,
            "{space}: 4 bytes is 8 hex digits"
        );
    }
}

/// `space` defaults to `bus`, and the default read is one byte — not `read_vram`'s 32, and not
/// `read_memory`'s implicit shape. One rule for the unified row.
#[test]
fn space_defaults_to_bus_and_len_defaults_to_one() {
    let h = spawn_system("read-defaults", machine(), 64);
    let mut c = client(&h);
    let r = c.ok("emulator/read", json!({"addr": "0x00FF0000"}));
    assert_eq!(r["space"], json!("bus"));
    assert_eq!(r["len"], json!(1));
    assert!(r.get("region").is_some(), "bus reads carry their region");
}

/// **`region`/`symbol`/`symbolDisp` appear iff the space is `bus`.** The schema enforces this in both
/// directions; this asserts the server actually behaves that way, since a schema can only reject what the
/// server emits.
#[test]
fn bus_only_fields_appear_only_on_bus_reads() {
    let h = spawn_system("read-iffbus", machine(), 64);
    let mut c = client(&h);

    let bus = c.ok("emulator/read", json!({"addr": "0x00000200", "len": 2}));
    assert!(bus.get("region").is_some(), "bus read carries region");

    for space in ["vram", "cram", "vsram"] {
        let r = c.ok(
            "emulator/read",
            json!({"space": space, "addr": "0x00000000", "len": 2}),
        );
        for key in ["region", "symbol", "symbolDisp"] {
            assert!(
                r.get(key).is_none(),
                "{space} read must not carry `{key}` — it is meaningless outside the 68000 map"
            );
        }
    }
}

/// **One refusal per bound.** A range whose *end* runs past its space is refused, never clipped: a clipped
/// read reports bytes it never looked at, which is the one answer a read must never give.
#[test]
fn a_range_that_runs_past_its_space_is_refused_not_clipped() {
    let h = spawn_system("read-bounds", machine(), 64);
    let mut c = client(&h);
    // VRAM $10000, CRAM $80, VSRAM $50 — read the last legal byte, then one past it.
    for (space, last) in [("vram", 0xFFFFu32), ("cram", 0x7F), ("vsram", 0x4F)] {
        let ok = c.ok(
            "emulator/read",
            json!({"space": space, "addr": format!("0x{last:08X}"), "len": 1}),
        );
        assert_eq!(ok["len"], json!(1), "{space}: the last byte is readable");

        let e = c.err(
            "emulator/read",
            json!({"space": space, "addr": format!("0x{last:08X}"), "len": 2}),
        );
        assert_eq!(
            e["code"],
            json!(-32004),
            "{space}: a range running one byte past the end is refused"
        );
    }
}

/// A symbol resolves on the bus and is refused everywhere else — `watchpoint_add`'s rule, verbatim,
/// because a VDP-internal byte address has no symbol.
#[test]
fn symbol_is_bus_only() {
    let h = spawn_system("read-symbol", machine(), 64);
    let mut c = client(&h);
    for space in ["vram", "cram", "vsram"] {
        let e = c.err(
            "emulator/read",
            json!({"space": space, "symbol": "anything", "len": 1}),
        );
        assert_eq!(
            e["code"],
            json!(-32602),
            "{space} with `symbol` is refused before resolution, so the error names the real mistake"
        );
    }
}

/// An unknown space is refused rather than silently defaulting to `bus` — a read answered from the wrong
/// space is a wrong answer wearing a right one's shape.
#[test]
fn an_unknown_space_is_refused() {
    let h = spawn_system("read-badspace", machine(), 64);
    let mut c = client(&h);
    let e = c.err(
        "emulator/read",
        json!({"space": "z80", "addr": "0x00000000", "len": 1}),
    );
    assert_eq!(
        e["code"],
        json!(-32602),
        "`z80` is deliberately NOT a space here — z80_read keeps its own row and its own bounds"
    );
}

/// It is a pure read: answered on a free-running machine rather than refused, as `pixel_attribution` and
/// `sprites` are.
#[test]
fn it_answers_a_free_running_machine() {
    let h = spawn_system("read-running", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));
    let r = c.ok("emulator/read", json!({"addr": "0x00FF0000", "len": 2}));
    assert_eq!(r["running"], json!(true));
    assert_eq!(r["len"], json!(2));
}

/// **The deprecated rows are exact aliases and keep answering.** Removing them is barred by D5 and by a
/// live client that drives both by name — which is also why adopting this row costs that client nothing.
#[test]
fn the_deprecated_rows_still_answer_and_agree_byte_for_byte() {
    let h = spawn_system("read-alias", machine(), 64);
    let mut c = client(&h);

    let via_read = c.ok(
        "emulator/read",
        json!({"space": "bus", "addr": "0x00000200", "len": 8}),
    );
    let via_old = c.ok(
        "emulator/read_memory",
        json!({"addr": "0x00000200", "len": 8}),
    );
    assert_eq!(
        via_read["bytes"], via_old["bytes"],
        "read with space bus and read_memory are the same read"
    );

    let via_read = c.ok(
        "emulator/read",
        json!({"space": "vram", "addr": "0x00000040", "len": 8}),
    );
    let via_old = c.ok(
        "emulator/read_vram",
        json!({"addr": "0x00000040", "len": 8}),
    );
    assert_eq!(
        via_read["bytes"], via_old["bytes"],
        "read with space vram and read_vram are the same read"
    );
}

/// The key set on a non-bus reply, exact — §8 item 20's closure applied where it binds.
#[test]
fn the_key_set_is_exact() {
    use std::collections::BTreeSet;
    let h = spawn_system("read-keys", machine(), 64);
    let mut c = client(&h);
    let r: Value = c.ok(
        "emulator/read",
        json!({"space": "cram", "addr": "0x00000000", "len": 4}),
    );
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    let want: BTreeSet<&str> = [
        "space",
        "addr",
        "len",
        "bytes",
        "frame",
        "mclk",
        "running",
        "droppedEvents",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        got, want,
        "no region/symbol/symbolDisp off the bus, and no constant caveat — a caveat every reply carries \
         is one nobody reads"
    );
}
