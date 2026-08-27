//! `emulator/write_vram` — `protocol.md` §6 (*VRAM / CRAM / layers*), the row at line 1257.
//!
//! Served 2026-08-27 against contract revision **`091ac59`**. This row's fragment was **transcribed** in
//! empyrean's 2026-08-22 first-fragment pass rather than repaired, and its `$comment` names three
//! absences that are registered as audit **D-16**. All three are served **as written** — the deviation
//! that would have "fixed" them would be a server quietly better than its contract, which no client can
//! discover. `docs/2026-08-27-write-vram.md` carries the CR text filed upstream, and the handler's doc
//! comment carries the reasoning; this file is the evidence.
//!
//! Every test here is a **wire** round trip, so `common::Client::recv` validates each received line
//! against the vendored contract schema on the way past — driving the method *is* the schema-conformance
//! pin, and it cannot be forgotten.
//!
//! The shape follows `write_memory.rs` and `cram.rs`, which is the house shape for a poke row: closed
//! happy path, one refusal per catalogued bound with a sentinel proving nothing landed first, the
//! run-control question answered explicitly, and the two standing properties of a poke.

mod common;

use common::{spawn_system, spawn_with, Client};
use oracle_core::system::System;
use oracle_core::vdp::Vdp;
use serde_json::json;
use std::collections::BTreeSet;

/// SAT base: reg 5 = `$58` → `($58 & $7E) << 9` = `$B000` in H40. `sprites.rs`' fixture base, so the two
/// files describe the same table.
const SAT_BASE: u16 = 0xB000;

/// The last legal VRAM byte. Derived from the contract, not measured: `emulator/read`'s note fixes the
/// space sizes as *"bus 24-bit, VRAM `$FFFF`, CRAM `$7F`, VSRAM `$4F`"* — and cross-checked against the
/// core's own `VRAM_SIZE` by [`the_bound_is_the_cores_own_vram_size`].
const LAST_VRAM_BYTE: u32 = 0xFFFF;

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

fn set_reg(v: &mut Vdp, reg: u8, val: u8) {
    v.control_write(0x8000 | (u16::from(reg) << 8) | u16::from(val), 0);
}

fn set_addr(v: &mut Vdp, code: u8, addr: u16) {
    v.control_write(((u16::from(code) & 0x03) << 14) | (addr & 0x3FFF), 0);
    v.control_write(((u16::from(code) >> 2) << 4) | (addr >> 14), 0);
}

fn port_write_vram(v: &mut Vdp, addr: u16, words: &[u16]) {
    set_addr(v, 0x01, addr);
    for w in words {
        v.data_write(*w);
    }
}

// ---------------------------------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------------------------------

/// The payload lands, and it is verified through **two independent read paths** — the row's own
/// deprecated twin `read_vram` and the successor `read {space:"vram"}`. Byte order is linear: byte *i*
/// of the payload lands at `addr + i`. (The data port's odd-address byte-swap is a property of a *word*
/// written through the port; a byte-addressed poke has no counterpart to it, which is what makes
/// `write_vram` → `read_vram` an identity.)
#[test]
fn bytes_land_in_vram_and_read_back_through_both_read_paths() {
    let h = spawn_system("wv-bytes", machine(), 64);
    let mut c = client(&h);
    let r = c.ok(
        "emulator/write_vram",
        json!({"addr": "0x1234", "bytes": "0xDEADBEEF"}),
    );
    assert_eq!(r["addr"], json!("0x00001234"), "the base is echoed");
    assert_eq!(r["len"], json!(4), "bytes written");

    let old = c.ok("emulator/read_vram", json!({"addr": "0x1234", "len": 4}));
    assert_eq!(old["bytes"], json!("0xDEADBEEF"), "read_vram sees it");
    let new = c.ok(
        "emulator/read",
        json!({"space": "vram", "addr": "0x1234", "len": 4}),
    );
    assert_eq!(new["bytes"], json!("0xDEADBEEF"), "read sees it too");

    // Linear, not swapped: an odd base writes byte 0 at the odd address. VRAM comes up randomized at
    // power-on, so the three-byte window is cleared through the legitimate path first — otherwise the
    // untouched byte below would be whatever the RNG left there.
    c.ok(
        "emulator/write_vram",
        json!({"addr": "0x1300", "bytes": "0x000000"}),
    );
    c.ok(
        "emulator/write_vram",
        json!({"addr": "0x1301", "bytes": "0x1122"}),
    );
    let odd = c.ok("emulator/read_vram", json!({"addr": "0x1300", "len": 3}));
    assert_eq!(
        odd["bytes"],
        json!("0x001122"),
        "byte i lands at addr+i even from an odd base"
    );
}

/// §8 item 20's closure, asserted locally: the success key set is **exactly** `addr` + `len` beside the
/// envelope. In particular there is **no `caveat`** — the fragment declares it absent (the `write_memory`
/// precedent), so emitting one would fail item 20 at the schema and mislead a client here.
#[test]
fn the_key_set_is_exact_and_carries_no_caveat() {
    /// The four keys the envelope stamps on after the handler returns (§2.2 / D11, §2.3 / D17) —
    /// subtracted rather than listed among the method's own keys, as `cram.rs` and `scanlines.rs` do.
    const ENVELOPE_KEYS: &[&str] = &["frame", "mclk", "running", "droppedEvents"];

    let h = spawn_system("wv-keys", machine(), 64);
    let mut c = client(&h);
    let r = c.ok(
        "emulator/write_vram",
        json!({"addr": "0x0000", "bytes": "0x5A"}),
    );
    let mut keys: BTreeSet<String> = r.as_object().expect("an object").keys().cloned().collect();
    for e in ENVELOPE_KEYS {
        keys.remove(*e);
    }
    assert_eq!(
        keys,
        ["addr", "len"].iter().map(|s| s.to_string()).collect(),
        "the row's own keys are addr and len, and nothing else"
    );
}

// ---------------------------------------------------------------------------------------------------
// The address bound — absence (2), served as the read side already serves it
// ---------------------------------------------------------------------------------------------------

/// §6's row states **no** address bound; `emulator/read`'s note fixes VRAM's space size at `$FFFF`, and
/// `read_vram` already refuses on it. Adopting the read half's bound rather than inventing a second one
/// is the smallest choice available — some bound is physically unavoidable — and the refusal follows
/// every other write row in the catalog: `-32004`, **refused whole before any byte lands**.
///
/// The wrap case is the one that matters and is asserted directly: a server that masked `addr & 0xFFFF`
/// (as the core's guest write path legitimately does) would land the tail of an over-the-end payload at
/// VRAM `$0000`, silently corrupting a byte the caller never named.
#[test]
fn the_address_bound_is_refused_whole_and_never_wrapped() {
    let h = spawn_system("wv-bounds", machine(), 64);
    let mut c = client(&h);

    // Sentinels at both ends, through the legitimate path, so a leak is visible.
    c.ok(
        "emulator/write_vram",
        json!({"addr": "0x0000", "bytes": "0xA5A5"}),
    );
    c.ok(
        "emulator/write_vram",
        json!({"addr": format!("{LAST_VRAM_BYTE:#X}"), "bytes": "0xA5"}),
    );

    for (params, why) in [
        (
            json!({"addr": format!("{LAST_VRAM_BYTE:#X}"), "bytes": "0x0102"}),
            "the END runs one byte past the space",
        ),
        (
            json!({"addr": "0x10000", "bytes": "0x01"}),
            "the base is one past the space",
        ),
        (
            json!({"addr": "0xFF0000", "bytes": "0x01"}),
            "a 68000 work-RAM address is not a VRAM address",
        ),
    ] {
        let e = c.err("emulator/write_vram", params);
        assert_eq!(e["code"], json!(-32004), "{why}");
    }

    // Neither the far end nor the wrap target moved.
    let end = c.ok(
        "emulator/read_vram",
        json!({"addr": format!("{LAST_VRAM_BYTE:#X}"), "len": 1}),
    );
    assert_eq!(
        end["bytes"],
        json!("0xA5"),
        "the last byte kept its sentinel"
    );
    let wrap = c.ok("emulator/read_vram", json!({"addr": "0x0000", "len": 2}));
    assert_eq!(
        wrap["bytes"],
        json!("0xA5A5"),
        "an over-the-end payload must not wrap to $0000"
    );

    // The bound is real rather than a blanket refusal: the last legal byte IS writable.
    let r = c.ok(
        "emulator/write_vram",
        json!({"addr": format!("{LAST_VRAM_BYTE:#X}"), "bytes": "0x7E"}),
    );
    assert_eq!(r["len"], json!(1));
}

/// The bound this server enforces is the space's real size, **derived** rather than pinned twice: a full
/// `$10000`-byte payload from `$0000` is accepted, and one byte more is refused. If the core's VRAM ever
/// changed size, this test — not a copied literal — is what would say so.
#[test]
fn the_bound_is_the_cores_own_vram_size() {
    let h = spawn_system("wv-size", machine(), 64);
    let mut c = client(&h);
    let size = usize::try_from(LAST_VRAM_BYTE).unwrap() + 1;

    let whole = format!("0x{}", "00".repeat(size));
    let r = c.ok(
        "emulator/write_vram",
        json!({"addr": "0x0000", "bytes": whole}),
    );
    assert_eq!(
        r["len"],
        json!(size),
        "the whole space is one legal payload — this row declares no maxWriteLen"
    );

    let one_more = format!("0x{}", "00".repeat(size + 1));
    let e = c.err(
        "emulator/write_vram",
        json!({"addr": "0x0000", "bytes": one_more}),
    );
    assert_eq!(e["code"], json!(-32004), "one byte more is refused");
}

// ---------------------------------------------------------------------------------------------------
// The payload spelling — absence (3), served as written
// ---------------------------------------------------------------------------------------------------

/// `bytes` is this row's **only** payload spelling. `value`/`width` are therefore not a wrong spelling
/// but **undeclared params**, refused by §2.5's closure with `-32602` and the offending key named — which
/// is the loud failure a client can act on at its own call site.
///
/// The probe byte is load-bearing rather than decorative: every refused payload below is `0x00`- or
/// `0x01`-shaped and a leaked byte of that shape is indistinguishable from cleared VRAM, so a sentinel
/// goes in first through the legitimate path and the assertion is that it *survived*.
#[test]
fn bytes_is_the_only_payload_spelling_and_a_refusal_writes_nothing() {
    const PROBE: &str = "0x2000";
    let h = spawn_system("wv-spelling", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/write_vram",
        json!({"addr": PROBE, "bytes": "0xA5"}),
    );

    for (bad, code, why) in [
        (
            json!({"addr": PROBE, "value": 1, "width": 1}),
            -32602,
            "value+width is undeclared on this row",
        ),
        (
            json!({"addr": PROBE, "bytes": "0x00", "width": 1}),
            -32602,
            "width is undeclared even beside a legal payload",
        ),
        (json!({"addr": PROBE}), -32602, "no payload at all"),
        (json!({"bytes": "0x00"}), -32602, "no addr"),
        (
            json!({"addr": PROBE, "bytes": "0x0"}),
            -32602,
            "odd digit count",
        ),
        (
            json!({"addr": PROBE, "bytes": "0x"}),
            -32602,
            "empty payload",
        ),
        (
            json!({"addr": PROBE, "bytes": 1}),
            -32602,
            "a payload must be a hex string (D9 category 1)",
        ),
    ] {
        let e = c.err("emulator/write_vram", bad.clone());
        assert_eq!(e["code"], json!(code), "{why}: {bad}");
    }

    let back = c.ok("emulator/read_vram", json!({"addr": PROBE, "len": 1}));
    assert_eq!(
        back["bytes"],
        json!("0xA5"),
        "a refusal that wrote first would have clobbered the sentinel"
    );
}

/// The two undeclared keys are named on the refusal, not merely rejected — §2.5's whole point, and what
/// separates "this row has no `value`" from a generic malformed-params error.
#[test]
fn an_undeclared_payload_key_is_named() {
    let h = spawn_system("wv-named", machine(), 64);
    let mut c = client(&h);
    let e = c.err(
        "emulator/write_vram",
        json!({"addr": "0x0000", "value": 1, "width": 1}),
    );
    assert_eq!(
        e["data"]["unknownParams"],
        json!(["value", "width"]),
        "both offending keys are named, in order"
    );
}

// ---------------------------------------------------------------------------------------------------
// The named hazard: the SAT cache
// ---------------------------------------------------------------------------------------------------

/// **The reason this row does not write through `Vdp::vram_mut`.**
///
/// Every guest VRAM byte routes through `Vdp::write_vram_byte`, which mirrors the cached half of a
/// sprite-attribute entry (Y + size/link) into the SAT cache — the copy the VDP actually draws from.
/// `vram_mut` hands out the bare array and runs none of that, so a poke through it would leave the cache
/// describing the previous sprite while VRAM described the new one: `sprites` would report the **old**
/// `y` and `cacheDivergence: true` for a table nobody had left stale, and the emulator would draw a
/// picture the VRAM does not describe.
///
/// So the assertion is two-sided, and both halves are needed: the poked Y must be what `sprites` reports
/// (the cache moved), **and** `cacheDivergence` must stay `false` (the two halves still agree).
#[test]
fn a_poke_into_the_sprite_table_maintains_the_sat_cache() {
    let mut sys = machine();
    {
        let v = sys.vdp_mut();
        v.vram_mut().fill(0);
        set_reg(v, 0x01, 0x74); // display on, mode 5, DMA enable — reg 1 FIRST (mode-4 register mask)
        set_reg(v, 0x0C, 0x81); // H40
        set_reg(v, 0x05, 0x58); // SAT base $B000
        set_reg(v, 0x0F, 0x02); // autoincrement 2
                                // Entry 0 through the PORT path, so the fixture starts coherent:
                                // Y = $0100 (screen 128), size 2x3 + link 3, attr, X = $00C8 (screen 72).
        port_write_vram(v, SAT_BASE, &[0x0100, (0x06 << 8) | 3, 0x8000, 0x00C8]);
    }
    let h = spawn_system("wv-sat", sys, 64);
    let mut c = client(&h);

    let before = c.ok("emulator/sprites", json!({"limit": 1}))["sprites"][0].clone();
    assert_eq!(before["y"], json!(128), "screen Y = $0100 - 128 = 128");
    assert_eq!(
        before["cacheDivergence"],
        json!(false),
        "the fixture starts coherent, or this test proves nothing"
    );

    // Poke a new Y word ($00C8 = 200 → screen 200 - 128 = 72) at entry 0's cached half.
    let r = c.ok(
        "emulator/write_vram",
        json!({"addr": format!("{SAT_BASE:#X}"), "bytes": "0x00C8"}),
    );
    assert_eq!(r["len"], json!(2));

    let after = c.ok("emulator/sprites", json!({"limit": 1}))["sprites"][0].clone();
    assert_eq!(
        after["y"],
        json!(72),
        "the poke reached the SAT CACHE — `y` is read from the cache, so a `vram_mut` write would \
         still report 128 here"
    );
    assert_eq!(
        after["cacheDivergence"],
        json!(false),
        "VRAM and the cache still agree — a write path that skipped the write-through would flag a \
         divergence the caller never created"
    );

    // And the byte really is in VRAM too, through the independent read path.
    let back = c.ok(
        "emulator/read_vram",
        json!({"addr": format!("{SAT_BASE:#X}"), "len": 2}),
    );
    assert_eq!(back["bytes"], json!("0x00C8"));
}

// ---------------------------------------------------------------------------------------------------
// The two standing properties of a poke (§6, stated once rather than as a per-reply caveat)
// ---------------------------------------------------------------------------------------------------

/// **A poke is a debugger access, not a guest access**: it is never offered to the watch surface, and
/// `watchpoint_hits.seen` does not move for it — the fragment's `$comment` says so in terms.
///
/// The assertion is two-sided on purpose. `matched == 0` alone would pass on a watch that was never
/// attached to anything, which is exactly what `seen` exists to separate out — so the watch is first
/// proven live against a ROM that really does drive a VRAM write through the port (`build_vram_poke`
/// pokes a word at `$0100`), and only then is the poke proven to move **neither** counter.
///
/// # What this test does NOT prove, said out loud
///
/// It is the **end-to-end** pin: the property a client can observe, over the real wire, against a watch
/// that has demonstrably recorded something. It is **not** the guard on `Vdp::poke_vram`'s omission of
/// `capture`, and it was measured not to be: adding a `capture` call inside `poke_vram` leaves this test
/// **green**. The reason is structural — `System::run` arms the capture buffer for the duration of a run
/// and disarms it on return (`system.rs`, the `wants_writes`/`capture` pair), so a poke issued *between*
/// runs meets a disarmed buffer whatever it does. Nothing reachable from this surface can arm it around
/// a poke.
///
/// The sensitive guard is therefore `oracle_core::vdp::tests::a_vram_poke_is_never_offered_to_the_watch_
/// surface`, which arms the buffer directly and carries the port path as its control; that one was proven
/// red against exactly this poison. The two are kept as a pair rather than one being deleted, because
/// they answer different questions: "does the contract hold on the wire" and "does the write path
/// capture". *(The same insensitivity is pre-existing in `cram.rs`'s namesake, which `Vdp::poke_cram`'s
/// doc comment calls "the direct pin" — measured 2026-08-27 and reported as a follow-up, not fixed here.)*
#[test]
fn a_poke_is_never_offered_to_the_watch_surface() {
    let h = spawn_with("wv-watch", oracle_core::testrom::build_vram_poke(), 1024);
    let mut c = client(&h);
    c.ok(
        "emulator/watchpoint_add",
        json!({"space": "vram", "addr": "0x00000100", "len": 2}),
    );
    c.ok("emulator/run_frames", json!({"frames": 1}));

    let before = c.ok("emulator/watchpoint_hits", json!({}));
    let seen = before["seen"].as_u64().expect("seen");
    let matched = before["matched"].as_u64().expect("matched");
    assert!(
        seen > 0,
        "the recorder must have ridden the run, or this test proves nothing"
    );
    assert!(
        matched > 0,
        "the ROM must drive a real VRAM write through this watch, or the assertion below is vacuous \
         (seen={seen}, matched={matched})"
    );

    // The machine is paused after run_frames. Poke exactly the bytes the watch covers.
    c.ok(
        "emulator/write_vram",
        json!({"addr": "0x0100", "bytes": "0x1234"}),
    );

    let after = c.ok("emulator/watchpoint_hits", json!({}));
    assert_eq!(
        after["matched"], before["matched"],
        "a poke matched a vram watch — it went through the capturing byte choke"
    );
    assert_eq!(
        after["seen"], before["seen"],
        "a poke was OFFERED to the recorder — `seen` moved even though nothing matched"
    );

    // And the poke really did land, so the two assertions above are about the watch surface rather
    // than about a write that silently did nothing.
    let back = c.ok("emulator/read_vram", json!({"addr": "0x0100", "len": 2}));
    assert_eq!(back["bytes"], json!("0x1234"), "the poke landed");
}

// ---------------------------------------------------------------------------------------------------
// The run-control question — absence (1), served as written
// ---------------------------------------------------------------------------------------------------

/// **This row is NOT named in §6's run-control state rule**, and the server serves that silence rather
/// than repairing it. The rule names `run_to`, `run_to_scanline`, `run_frames`, `step*`, `press`,
/// `play_input`, `reload_rom`, `write_memory`, `write_cram` and `z80_write` — ten rows, and not this one.
///
/// The direction is the reversible one: §6 says *"relaxing a refusal later is additive (D5); introducing
/// one is not"*, so a server that invented the gate would have to break clients to give it up, while
/// this one can adopt the gate the day the contract states it.
///
/// **This test is a contract pin, not an endorsement.** A CR asking empyrean to name the row is filed
/// (`docs/2026-08-27-write-vram.md`), and the day it lands this assertion goes red and forces the
/// handler to grow a `require_paused` in the same commit — which is exactly what a pin is for.
#[test]
fn it_is_not_subject_to_the_run_control_state_rule() {
    let h = spawn_system("wv-running", machine(), 64);
    let mut c = client(&h);

    // The control: a row that IS named refuses, so a green here cannot be a machine that failed to
    // start running.
    c.ok("emulator/resume", json!({}));
    let gated = c.err(
        "emulator/write_memory",
        json!({"addr": "0xFF0000", "bytes": "0x00"}),
    );
    assert_eq!(
        gated["code"],
        json!(-32005),
        "the control row is gated, so the machine really is free-running"
    );
    assert_eq!(gated["data"]["reason"], json!("machineRunning"));

    let r = c.ok(
        "emulator/write_vram",
        json!({"addr": "0x3000", "bytes": "0x5A"}),
    );
    assert_eq!(
        r["len"],
        json!(1),
        "§6's run-control state rule does not name this row, so the free-running call is served"
    );
    assert_eq!(
        r["running"],
        json!(true),
        "and D11's stamp is what tells the caller what it acted on"
    );
}
