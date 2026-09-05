//! **§8 item 28 / §11.38 (CR-O, 2026-09-05): watchpoint hits do not survive a reload or a reset.**
//!
//! The report is aeon's. A first `emulator/watchpoint_hits` read after an `emulator/reload_rom` handed
//! them hits stamped **frames 397 and 655**, recorded against a *previous build*, and nothing on the wire
//! distinguished them from the new run's toggles. A hit is epoch-relative in three fields at once —
//! `frame` and the cycle stamp restart at the boundary, and `pc` is resolved against whatever symbol
//! table is loaded now — so a survivor is not a stale datum, it is an uninterpretable one wearing a live
//! one's shape.
//!
//! Four rows, and the fourth is the one item 28's closing sentence demands:
//!
//! 1. [`hits_recorded_before_a_reload_are_absent_after_it_and_hits_dropped_counts_them`]
//! 2. [`hits_recorded_before_a_reset_are_absent_after_it_and_hits_dropped_counts_them`]
//! 3. [`hits_dropped_is_zero_and_PRESENT_when_nothing_was_recorded`] — the control that stops the
//!    feature passing by always reporting a number it invented.
//! 4. [`a_client_action_keeps_hits_and_only_the_machines_boundary_drops_them`] — *"the suite must not
//!    conflate the two"*. Without it, an implementation that dropped hits on **every** operation would
//!    satisfy rows 1-3 perfectly while destroying the property the CR promised to preserve:
//!    `watchpoint_clear` keeps recorded hits (a destructive clear would let one client erase another's
//!    evidence) and reads use `hits()`, never `take_hits()`.
//!
//! **Every row asserts the COUNT, not merely the presence of a number.** `hitsDropped` is compared
//! against the `total` read back from the ring immediately before the boundary, and the post-boundary
//! ring is asserted empty by count as well as by array. A reply that reported a plausible constant, or a
//! clear that dropped a different number of hits than it claimed, reads identically to a correct one on
//! any assertion weaker than that.

#![cfg(unix)]

mod common;

use common::{spawn_system, Client};
use oracle_aether::server::ServerHandle;
use oracle_core::system::System;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The address `testrom::build`'s VInt handler writes `$1234` to, once per frame — the same clean
/// single-writer target `tests/watchpoints.rs` uses, and for the same reason.
const SENTINEL: &str = "0x00FF8000";

/// A paused server whose machine really **takes** its vertical interrupt, plus the image on disk so
/// `emulator/reload_rom` has something to load.
///
/// The IE0 poke is `tests/watchpoints.rs`'s: the fixture ROMs lower the CPU mask but never touch a VDP
/// register, so without it the VInt latch is set every frame and never gated into the IPL — the handler
/// never runs, the sentinel is never written, and a "no hits were recorded" row would pass for the wrong
/// reason.
fn armed(tag: &str) -> (ServerHandle, Client, PathBuf) {
    let rom = oracle_core::testrom::build();
    let path = std::env::temp_dir().join(format!("ae-{tag}-{}.bin", std::process::id()));
    std::fs::write(&path, &rom).expect("write the ROM fixture to disk");
    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    sys.vdp_mut().control_write(0x8120, 0); // reg 1 = $20 → IE0 (VINT enable)
    let h = spawn_system(tag, sys, 1024);
    let mut c = Client::connect(&h);
    c.handshake(true);
    (h, c, path)
}

/// Arm the sentinel watch, run until it has fired, and return `(handle, hits_held)` — the number the
/// boundary is then obliged to report back.
fn record_some_hits(c: &mut Client) -> (String, u64) {
    let handle = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "len": 2, "label": "sentinel"}),
    )["watch"]
        .as_str()
        .expect("a watch handle")
        .to_string();
    c.ok("emulator/run_frames", json!({"frames": 3}));
    let got = c.ok("emulator/watchpoint_hits", json!({}));
    let held = got["total"].as_u64().expect("`total` is required");
    assert!(
        held > 0,
        "the fixture must actually record hits, or every row below passes vacuously"
    );
    (handle, held)
}

/// The ring, read back the way a client reads it.
fn hits(c: &mut Client) -> Value {
    c.ok("emulator/watchpoint_hits", json!({}))
}

/// `hitsDropped` as a number, having first asserted the key is **there** — `r["k"]` on a missing key is
/// `Null`, which every comparison below would fail, but the failure would name the wrong thing.
fn hits_dropped(reply: &Value) -> u64 {
    let v = reply
        .as_object()
        .expect("a result object")
        .get("hitsDropped")
        .expect("`hitsDropped` is REQUIRED on this reply (§11.38)");
    v.as_u64()
        .unwrap_or_else(|| panic!("`hitsDropped` must be a non-negative integer, got {v}"))
}

// ---------------------------------------------------------------------------------------------------
// Item 28, row 1 — the reload
// ---------------------------------------------------------------------------------------------------

/// **Hits recorded before an `emulator/reload_rom` are absent afterwards, and the reply's `hitsDropped`
/// counts them.** The count is the load-bearing half: a server that emptied the ring and reported `0`
/// would pass an "are they gone?" assertion and still be lying about what it destroyed.
#[test]
fn hits_recorded_before_a_reload_are_absent_after_it_and_hits_dropped_counts_them() {
    let (_h, mut c, path) = armed("wh-reload");
    let (_handle, held) = record_some_hits(&mut c);

    let reload = c.ok(
        "emulator/reload_rom",
        json!({"path": path.display().to_string()}),
    );
    assert_eq!(reload["reloaded"], json!(true), "the reload really ran");
    assert_eq!(
        hits_dropped(&reload),
        held,
        "the reply counts exactly the hits the ring was holding, not a plausible constant"
    );

    let after = hits(&mut c);
    assert_eq!(after["total"], json!(0), "the ring is empty by count");
    assert_eq!(
        after["hits"].as_array().map(Vec::len),
        Some(0),
        "and by array — a page that is short for a paging reason is not the same finding"
    );

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------------------
// Item 28, row 2 — the reset
// ---------------------------------------------------------------------------------------------------

/// **The same across `emulator/reset`.** The image and the symbols survive a reset, so this is the weaker
/// case on paper — and it is the one the consumer actually met: the frame counter restarts, so a hit
/// stamped frame 397 from the previous epoch is indistinguishable from frame 397 of the epoch now
/// running.
#[test]
fn hits_recorded_before_a_reset_are_absent_after_it_and_hits_dropped_counts_them() {
    let (_h, mut c, path) = armed("wh-reset");
    let (_handle, held) = record_some_hits(&mut c);

    let reset = c.ok("emulator/reset", json!({}));
    assert_eq!(reset["deferred"], json!(false), "the reset really ran");
    assert_eq!(
        hits_dropped(&reset),
        held,
        "the reply counts exactly the hits the ring was holding"
    );

    let after = hits(&mut c);
    assert_eq!(after["total"], json!(0));
    assert_eq!(after["hits"].as_array().map(Vec::len), Some(0));

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------------------
// Item 28, row 3 — the zero, and it is PRESENT
// ---------------------------------------------------------------------------------------------------

/// **`hitsDropped` is `0`, present, when nothing was recorded.** This is the control: without it a server
/// that hard-coded the count — or one that only emitted the key when it had something to report — passes
/// rows 1 and 2 and tells a client nothing it can rely on. Absence and `0` must not both mean "nothing
/// was lost", which is `symbolsDropped`'s own argument on this same reply.
///
/// Asserted on **both** boundaries, and on a machine that has run frames with no watch armed — so the
/// zero is "the ring was empty", not "the machine never moved".
#[test]
#[allow(non_snake_case)]
fn hits_dropped_is_zero_and_PRESENT_when_nothing_was_recorded() {
    let (_h, mut c, path) = armed("wh-zero");
    c.ok("emulator/run_frames", json!({"frames": 3}));
    let cold = hits(&mut c);
    assert_eq!(
        cold["total"],
        json!(0),
        "no watch was armed, so nothing was recorded"
    );

    let reset = c.ok("emulator/reset", json!({}));
    assert!(
        reset.as_object().unwrap().contains_key("hitsDropped"),
        "present, not omitted, when it is zero"
    );
    assert_eq!(reset["hitsDropped"], json!(0));
    assert_eq!(hits_dropped(&reset), 0);

    let reload = c.ok(
        "emulator/reload_rom",
        json!({"path": path.display().to_string()}),
    );
    assert!(
        reload.as_object().unwrap().contains_key("hitsDropped"),
        "present, not omitted, when it is zero"
    );
    assert_eq!(reload["hitsDropped"], json!(0));
    assert_eq!(hits_dropped(&reload), 0);

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------------------
// Item 28's closing sentence — the two must NOT be conflated
// ---------------------------------------------------------------------------------------------------

/// ## ★ The row that stops rows 1-3 being satisfied by the wrong implementation ★
///
/// Item 28 closes: *"Hits stay durable against CLIENT actions (`watchpoint_clear` keeps them, reads never
/// drain); the boundary is a discontinuity in the machine, not a client action, and the suite must not
/// conflate the two."*
///
/// A server that dropped the ring on **any** watchpoint operation would satisfy the three rows above
/// perfectly and destroy two live design commitments:
///
/// * `watchpoint_clear` keeps recorded hits — *"a destructive clear would let one client erase another's
///   evidence"* (`engine.rs`), which is also what makes a retired handle legible on its own hits;
/// * reads use `hits()` and never `take_hits()` — *"a draining read is one client stealing another's
///   evidence"*.
///
/// So this row walks a client through **every** action that might plausibly be mistaken for a boundary —
/// two reads, a filtered read, a single clear, a clear-all — asserting the count is **unmoved** after
/// each, and only then crosses a real boundary and asserts the count goes to zero and is reported. The
/// discriminator is what the machine did, not what the client did.
#[test]
fn a_client_action_keeps_hits_and_only_the_machines_boundary_drops_them() {
    let (_h, mut c, path) = armed("wh-durable");
    let (handle, held) = record_some_hits(&mut c);

    // 1. A read does not drain. Twice, because a drain is invisible on the first read.
    assert_eq!(hits(&mut c)["total"].as_u64(), Some(held), "first re-read");
    assert_eq!(hits(&mut c)["total"].as_u64(), Some(held), "second re-read");

    // 2. A *filtered* read does not drain either — it walks the same ring.
    let filtered = c.ok("emulator/watchpoint_hits", json!({"watch": &handle}));
    assert_eq!(filtered["total"].as_u64(), Some(held), "filtered read");
    assert_eq!(
        hits(&mut c)["total"].as_u64(),
        Some(held),
        "and the unfiltered ring is untouched by it"
    );

    // 3. Clearing the watch that recorded them keeps them, still naming the retired handle.
    assert_eq!(
        c.ok("emulator/watchpoint_clear", json!({"watch": &handle}))["removed"],
        json!(1),
        "the watch really was registered"
    );
    let after_clear = hits(&mut c);
    assert_eq!(
        after_clear["total"].as_u64(),
        Some(held),
        "`watchpoint_clear` keeps recorded hits — one client must not erase another's evidence"
    );
    assert_eq!(after_clear["hits"][0]["watch"], json!(handle));

    // 4. `clear all` is not a boundary either.
    c.ok("emulator/watchpoint_clear", json!({"all": true}));
    assert_eq!(
        hits(&mut c)["total"].as_u64(),
        Some(held),
        "`watchpoint_clear all` keeps them too"
    );

    // 5. NOW the machine discontinues. Same ring, same client, opposite answer — and the count it
    //    reports is the count that survived every client action above.
    let reset = c.ok("emulator/reset", json!({}));
    assert_eq!(
        hits_dropped(&reset),
        held,
        "the boundary drops exactly what four client actions had preserved"
    );
    assert_eq!(
        hits(&mut c)["total"],
        json!(0),
        "and this time the ring really is empty"
    );

    let _ = std::fs::remove_file(&path);
}
