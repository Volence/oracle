//! `emulator/z80_read` / `emulator/z80_write` — `protocol.md` §6's Z80-pair blockquote, ruled in §11.22,
//! §11.24 and §11.28 (CR-B, adjudicated by the hub 2026-08-30 at empyrean `ec008ec`).
//!
//! Every test is a **wire** round trip, so `common::Client::recv` validates each reply against the vendored
//! contract schema — driving the rows at all is the schema-conformance pin.
//!
//! The two rows the ruling calls out as red-first are [`a_write_whose_end_runs_past_the_window_is_refused_whole`]
//! and [`a_value_above_a_byte_is_refused_rather_than_masked`]. The first is the **measured** legacy harm:
//! both legacy handlers bounded only the start address, then looped `addr + i` with no end check, so a
//! multi-byte write near `$3FFF` folded past the window, clobbered `$0000`, and replied **success**.

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

/// Paused, because `z80_write` is a paused-machine write under §6's run-control rule.
fn paused(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = client(h);
    c.ok("emulator/pause", json!({}));
    c
}

// ---------------------------------------------------------------------------------------------------
// The round trip
// ---------------------------------------------------------------------------------------------------

/// One byte in, one byte out, and `len` defaults to 1 on the read (§11.24, D-09).
#[test]
fn a_single_byte_round_trips_and_len_defaults_to_one() {
    let h = spawn_system("z80-rt", machine(), 64);
    let mut c = paused(&h);

    let w = c.ok(
        "emulator/z80_write",
        json!({"addr": "0x00000100", "value": 0xA5}),
    );
    assert_eq!(w["addr"], "0x00000100", "{w}");
    assert_eq!(w["len"], 1, "{w}");

    let r = c.ok("emulator/z80_read", json!({"addr": "0x00000100"}));
    assert_eq!(r["len"], 1, "`len` defaults to 1: {r}");
    assert_eq!(r["bytes"], "0xA5", "{r}");
}

/// A `bytes` payload is laid down **low-address-first**, which is what makes `width` unnecessary rather
/// than merely absent (§11.28's first bullet).
#[test]
fn a_bytes_payload_lands_low_address_first() {
    let h = spawn_system("z80-order", machine(), 64);
    let mut c = paused(&h);
    c.ok(
        "emulator/z80_write",
        json!({"addr": "0x00000200", "bytes": "0x11223344"}),
    );

    let r = c.ok("emulator/z80_read", json!({"addr": "0x00000200", "len": 4}));
    assert_eq!(r["bytes"], "0x11223344", "{r}");
    // Byte-addressed, so the third byte is readable on its own and is `0x33`. This is the assertion a
    // width/endianness bug fails: any byte-order confusion puts 0x44 or 0x22 here.
    let one = c.ok("emulator/z80_read", json!({"addr": "0x00000202", "len": 1}));
    assert_eq!(
        one["bytes"], "0x33",
        "low-address-first, byte by byte: {one}"
    );
}

/// ⚑ **The mirror is the machine, not a defect** (§11.28). `$2000`–`$3FFF` mirrors `$0000`–`$1FFF`; a
/// server MUST NOT "correct" it. Written through the mirror, read back at the low address.
#[test]
fn the_high_half_of_the_window_mirrors_the_low_half_and_is_not_corrected() {
    let h = spawn_system("z80-mirror", machine(), 64);
    let mut c = paused(&h);

    c.ok(
        "emulator/z80_write",
        json!({"addr": "0x00002001", "value": 0x5A}),
    );
    let low = c.ok("emulator/z80_read", json!({"addr": "0x00000001"}));
    assert_eq!(
        low["bytes"], "0x5A",
        "a write inside the window that lands on the mirror lands where the HARDWARE puts it — refusing \
         or redirecting it would be the server correcting the machine: {low}"
    );
    let high = c.ok("emulator/z80_read", json!({"addr": "0x00002001"}));
    assert_eq!(
        high["bytes"], "0x5A",
        "and it reads back through the mirror: {high}"
    );
}

// ---------------------------------------------------------------------------------------------------
// The refusals — both named red-first by the ruling
// ---------------------------------------------------------------------------------------------------

/// ⚑ **RED-FIRST, and this one is the measured legacy harm.** A range whose END runs past `$3FFF` is
/// `-32004`, refused **whole before any byte lands**, never wrapped and never clamped.
///
/// The legacy server bounded only the start, then looped `addr + i`: `z80_write {addr: 0x3FFC, bytes: 8}`
/// clobbered `$0000`–`$0003` and replied `len: 8`, success. The second half of this test is the part that
/// matters — it proves **nothing landed**, which a bare error assertion would not.
#[test]
fn a_write_whose_end_runs_past_the_window_is_refused_whole() {
    let h = spawn_system("z80-overrun", machine(), 64);
    let mut c = paused(&h);

    // A sentinel at the address a wrapped write would clobber.
    c.ok(
        "emulator/z80_write",
        json!({"addr": "0x00000000", "bytes": "0xEEEEEEEE"}),
    );

    let e = c.err(
        "emulator/z80_write",
        json!({"addr": "0x00003FFC", "bytes": "0x1122334455667788"}),
    );
    assert_eq!(
        e["code"], -32004,
        "the same code read/memory_hash/write_memory use for this refusal (§11.28 aligned it): {e}"
    );

    let after = c.ok("emulator/z80_read", json!({"addr": "0x00000000", "len": 4}));
    assert_eq!(
        after["bytes"], "0xEEEEEEEE",
        "REFUSED WHOLE: not one byte of the overrunning write may land. A partial write that reports an \
         error is still the legacy defect — it corrupted 0x0000 and the caller cannot tell: {after}"
    );
}

/// ⚑ **RED-FIRST.** `value` is 0–255 and refused **outside that range, never masked** — a masked `0x1FF`
/// writing `0xFF` is a wrong value reported as success.
#[test]
fn a_value_above_a_byte_is_refused_rather_than_masked() {
    let h = spawn_system("z80-value", machine(), 64);
    let mut c = paused(&h);
    c.ok(
        "emulator/z80_write",
        json!({"addr": "0x00000300", "value": 0x11}),
    );

    let e = c.err(
        "emulator/z80_write",
        json!({"addr": "0x00000300", "value": 0x1FF}),
    );
    assert_eq!(
        e["code"], -32602,
        "a SHAPE refusal, which §11.28 keeps distinct from the -32004 range refusal above: {e}"
    );
    let after = c.ok("emulator/z80_read", json!({"addr": "0x00000300"}));
    assert_eq!(
        after["bytes"], "0x11",
        "and nothing was written — a masked 0xFF here would be a wrong value reported as success: {after}"
    );
}

/// `len` above the `$2000` ceiling is refused, **never clamped**. The legacy server silently clamped
/// `10000` to `8192`: a short read reported as a whole one.
#[test]
fn a_read_length_above_the_ceiling_is_refused_rather_than_clamped() {
    let h = spawn_system("z80-len", machine(), 64);
    let mut c = client(&h);
    let e = c.err(
        "emulator/z80_read",
        json!({"addr": "0x00000000", "len": 10000}),
    );
    assert_eq!(e["code"], -32602, "{e}");
}

/// Two spellings of one payload is a shape error, not a precedence question.
#[test]
fn bytes_and_value_together_are_refused_rather_than_one_winning() {
    let h = spawn_system("z80-both", machine(), 64);
    let mut c = paused(&h);
    let e = c.err(
        "emulator/z80_write",
        json!({"addr": "0x00000400", "bytes": "0x11", "value": 0x22}),
    );
    assert_eq!(e["code"], -32602, "{e}");
}

/// `z80_write` is a paused-machine write; the read is not.
#[test]
fn the_write_needs_a_paused_machine_and_the_read_does_not() {
    let h = spawn_system("z80-runstate", machine(), 64);
    let mut c = client(&h);
    // The machine starts paused in this harness; resume explicitly, so the refusal below is about the RUN
    // STATE and not about a default. (Measured: without this the write succeeds, which is correct.)
    c.ok("emulator/resume", json!({}));
    let e = c.err(
        "emulator/z80_write",
        json!({"addr": "0x00000500", "value": 1}),
    );
    assert_eq!(e["code"], -32005, "§6's run-control state rule: {e}");
    // The read answers a free-running machine, like its pure-read siblings.
    let r = c.ok("emulator/z80_read", json!({"addr": "0x00000500"}));
    assert!(r["bytes"].is_string(), "{r}");
}

/// The handshake's `z80` group flag is **derived from the served set**, so serving one of the pair cannot
/// advertise the group as whole.
#[test]
fn the_z80_capability_is_true_now_that_both_rows_are_served() {
    let h = spawn_system("z80-cap", machine(), 64);
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    assert_eq!(init["capabilities"]["z80"], json!(true), "{init}");
    let methods = init["methods"].as_array().expect("methods");
    for m in ["emulator/z80_read", "emulator/z80_write"] {
        assert!(
            methods.iter().any(|v| v == m),
            "{m} must be advertised beside the flag: {init}"
        );
    }
}
