//! `emulator/write_memory` — `protocol.md` §6 (memory), adopted as CR-21
//! (`docs/2026-08-18-cr21-23-tier1-rows.md`, ruled in `docs/2026-08-18-ruling-cr21-23.md`, §11.13).
//!
//! Every reply is validated against the vendored schema on the way past. The adoption condition is
//! the shape of the file: closed happy path per payload spelling, plus one refusal per bound.

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

/// Happy path, `bytes` spelling — and the write is verified through the INDEPENDENT read path.
#[test]
fn bytes_land_in_ram_and_read_back() {
    let h = spawn_system("wm-bytes", machine(), 64);
    let mut c = client(&h);
    let r = c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0100", "bytes": "0xDEADBEEF"}),
    );
    assert_eq!(r["addr"], json!("0x00FF0100"));
    assert_eq!(r["len"], json!(4));
    let back = c.ok("emulator/read", json!({"addr": "0xFF0100", "len": 4}));
    assert_eq!(
        back["bytes"],
        json!("0xDEADBEEF"),
        "read back what was written"
    );
}

/// Happy path, `value`+`width` spelling — big-endian, as the 68000 stores.
#[test]
fn value_width_is_big_endian() {
    let h = spawn_system("wm-value", machine(), 64);
    let mut c = client(&h);
    for (width, value, expect) in [
        (1, 0xAB_u32, "0xAB"),
        (2, 0x1234, "0x1234"),
        (4, 0xCAFE_F00D, "0xCAFEF00D"),
    ] {
        let r = c.ok(
            "emulator/write_memory",
            json!({"addr": "0xFF0200", "value": value, "width": width}),
        );
        assert_eq!(r["len"], json!(width));
        let back = c.ok("emulator/read", json!({"addr": "0xFF0200", "len": width}));
        assert_eq!(
            back["bytes"],
            json!(expect),
            "width {width}: big-endian bytes"
        );
    }
}

/// The mirror window: an address in `$E00000` aliases the same RAM cell `$FF0000` sees.
#[test]
fn the_mirror_window_is_writable_and_aliases() {
    let h = spawn_system("wm-mirror", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/write_memory",
        json!({"addr": "0xE00300", "bytes": "0x5A"}),
    );
    let back = c.ok("emulator/read", json!({"addr": "0xFF0300", "len": 1}));
    assert_eq!(
        back["bytes"],
        json!("0x5A"),
        "$E00300 and $FF0300 are the same cell"
    );
}

/// Exactly one payload spelling — all wrong shapes are -32602, **before any write happens**.
///
/// The probe cell is load-bearing rather than decorative, and getting it there took two steps. Every
/// refusal below carries a `0x00`/`0x01`/`0x05`-shaped payload, and a leaked byte of that shape is
/// **indistinguishable from reset RAM** — so the cell is first poked with a sentinel through the
/// legitimate path, and the post-loop assertion is that the sentinel *survived*. A handler that wrote
/// before it refused would have overwritten it with the very payload it was refusing.
#[test]
fn payload_spelling_is_exactly_one_of_two() {
    const PROBE: &str = "0xFF0000";
    let h = spawn_system("wm-spelling", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/write_memory",
        json!({"addr": PROBE, "bytes": "0xA5"}),
    );
    let baseline = c.ok("emulator/read", json!({"addr": PROBE, "len": 1}));
    assert_eq!(
        baseline["bytes"],
        json!("0xA5"),
        "the sentinel is in place before a single refusal is issued"
    );

    for bad in [
        json!({"addr": PROBE, "bytes": "0x00", "value": 1, "width": 1}), // both
        json!({"addr": PROBE}),                                          // neither
        json!({"addr": PROBE, "bytes": "0x00", "width": 1}),             // width with bytes
        json!({"addr": PROBE, "value": 5}),                              // value sans width
        json!({"addr": PROBE, "value": 256, "width": 1}),                // value overflows width
        json!({"addr": PROBE, "bytes": "0xABC"}),                        // odd digit count
        json!({"addr": PROBE, "bytes": "0x"}),                           // empty payload
    ] {
        let e = c.err("emulator/write_memory", bad.clone());
        assert_eq!(e["code"], json!(-32602), "refused: {bad}");
    }

    // Not one of them touched the cell — and a leak would be visible, because the sentinel is a byte
    // none of the refused payloads carries.
    let back = c.ok("emulator/read", json!({"addr": PROBE, "len": 1}));
    assert_eq!(
        back["bytes"], baseline["bytes"],
        "a refusal that wrote first would have clobbered the sentinel"
    );
}

/// ROM and out-of-window targets are -32004 — refused, never clipped, and the end bound counts.
#[test]
fn rom_and_out_of_window_are_refused() {
    let h = spawn_system("wm-bounds", machine(), 64);
    let mut c = client(&h);
    for (addr, why) in [
        ("0x00000100", "ROM"),
        ("0x00400000", "unmapped"),
        ("0x00FFFFFF", "end runs past the window"), // len 2 below
    ] {
        let params = if why == "end runs past the window" {
            json!({"addr": addr, "bytes": "0x0102"})
        } else {
            json!({"addr": addr, "bytes": "0x01"})
        };
        let e = c.err("emulator/write_memory", params);
        assert_eq!(e["code"], json!(-32004), "{why} refused");
    }
    // The last legal byte IS writable:
    let r = c.ok(
        "emulator/write_memory",
        json!({"addr": "0x00FFFFFF", "bytes": "0x7E"}),
    );
    assert_eq!(r["len"], json!(1));
}

/// Run-state gated: -32005 machineRunning while free-running (named in §6's run-control rule).
#[test]
fn a_free_running_machine_refuses_the_poke() {
    let h = spawn_system("wm-gate", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));
    let e = c.err(
        "emulator/write_memory",
        json!({"addr": "0xFF0000", "bytes": "0x00"}),
    );
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("machineRunning"));
}

/// Symbol addressing refusals: no table loaded is -32012. The resolving half is deliberately not
/// re-exercised here — `resolve_target` is shared machinery, already pinned where it was introduced.
#[test]
fn symbol_addressing_refuses_without_a_table() {
    let h = spawn_system("wm-sym", machine(), 64);
    let mut c = client(&h);
    let e = c.err(
        "emulator/write_memory",
        json!({"symbol": "NoSuch", "bytes": "0x00"}),
    );
    assert_eq!(e["code"], json!(-32012), "no table loaded yet");
}

/// **A poke is a debugger access, not a guest access** — the contract's own words: it is never offered to
/// the watch surface, because a hit's `pc` names the instruction that drove the access and a poke has
/// none to name.
///
/// The assertion is two-sided on purpose. `matched == 0` alone would pass on a watch that was never
/// attached to anything, which is the exact failure `seen` exists to separate out — so the watch is first
/// proven live across a real run (`seen > 0`), and then the poke is proven to move **neither** counter.
#[test]
fn a_watch_does_not_see_the_poke() {
    let h = spawn_system("wm-watch", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": "0x00FFFFF0", "len": 1, "label": "poke target"}),
    );
    // Prove the instrument is live before asking it a negative question.
    c.ok("emulator/run_frames", json!({"frames": 2}));
    let before = c.ok("emulator/watchpoint_hits", json!({}));
    let seen = before["seen"].as_u64().unwrap();
    assert!(seen > 0, "the recorder rode the run: {before}");
    assert_eq!(before["matched"], json!(0), "the guest never wrote there");

    c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFFFFF0", "bytes": "0xA5"}),
    );
    // The poke landed...
    let back = c.ok("emulator/read", json!({"addr": "0xFFFFF0", "len": 1}));
    assert_eq!(back["bytes"], json!("0xA5"));
    // ...and the watch surface never heard about it.
    let after = c.ok("emulator/watchpoint_hits", json!({}));
    assert_eq!(after["seen"], json!(seen), "`seen` does not move on a poke");
    assert_eq!(after["matched"], json!(0), "no watch matches a poke");
    assert_eq!(after["total"], json!(0), "and no hit was recorded");
}

/// Over `limits.maxWriteLen` is `-32602` — **refused, never truncated**. A truncating server writes a
/// prefix and reports success, and the caller has no way to notice the tail never landed.
#[test]
fn an_over_cap_payload_is_refused_not_truncated() {
    let h = spawn_system("wm-cap", machine(), 64);
    let mut c = Client::connect(&h);
    // The cap is discoverable, so a client never has to learn it by being refused.
    let hello = c.handshake(false);
    let cap = hello["limits"]["maxWriteLen"]
        .as_u64()
        .expect("limits.maxWriteLen is advertised alongside the method");
    let over = usize::try_from(cap).unwrap() + 1;
    let payload = format!("0x{}", "5A".repeat(over));

    let base = c.ok("emulator/read", json!({"addr": "0xFF0500", "len": 1}));
    let e = c.err(
        "emulator/write_memory",
        json!({"addr": "0xFF0500", "bytes": payload}),
    );
    assert_eq!(e["code"], json!(-32602), "{over} bytes is over the cap");
    let after = c.ok("emulator/read", json!({"addr": "0xFF0500", "len": 1}));
    assert_eq!(
        after["bytes"], base["bytes"],
        "not one byte of a refused payload landed"
    );
    // The cap is real: exactly at it, the same shape succeeds.
    let at_cap = format!("0x{}", "5A".repeat(usize::try_from(cap).unwrap()));
    let r = c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0500", "bytes": at_cap}),
    );
    assert_eq!(r["len"], json!(cap));
}

/// §8 item 20 closure, asserted locally: the success key set is exact.
#[test]
fn the_key_set_is_exact() {
    use std::collections::BTreeSet;
    let h = spawn_system("wm-keys", machine(), 64);
    let mut c = client(&h);
    let r = c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0400", "bytes": "0x00"}),
    );
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    let want: BTreeSet<&str> = ["addr", "len", "frame", "mclk", "running", "droppedEvents"]
        .into_iter()
        .collect();
    assert_eq!(got, want, "no surplus keys, no constant caveat");
}
