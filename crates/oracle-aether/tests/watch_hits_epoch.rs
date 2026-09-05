//! **§8 item 28: watchpoint hits do not survive a reload, a reset or a restore** — §11.38 (CR-O,
//! 2026-09-05) for the first two, **EXTENDED by §11.39 (CR-P, 2026-09-05)** for the third, for the
//! `romReloaded` event's copy of the count, and for the survival of the aggregates.
//!
//! The report is aeon's. A first `emulator/watchpoint_hits` read after an `emulator/reload_rom` handed
//! them hits stamped **frames 397 and 655**, recorded against a *previous build*, and nothing on the wire
//! distinguished them from the new run's toggles. A hit is epoch-relative in three fields at once —
//! `frame` and the cycle stamp restart at the boundary, and `pc` is resolved against whatever symbol
//! table is loaded now — so a survivor is not a stale datum, it is an uninterpretable one wearing a live
//! one's shape.
//!
//! Eight rows. Four are §11.38's, and the fourth of those is the one item 28's closing sentence demands:
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
//! Four are §11.39's, and they are **the same idea reaching two more surfaces**, not a second mechanism:
//!
//! 5. [`hits_recorded_after_the_capture_point_are_absent_after_a_restore_and_hits_dropped_counts_them`]
//!    — note *after*: a restore rewinds **to** the capture point, so it is the hits recorded past it that
//!    describe a discarded future. Capture, record, restore. The other order measures something else.
//! 6. [`hits_dropped_is_zero_and_PRESENT_on_a_restore_when_nothing_was_recorded`] — row 3 at the third
//!    boundary.
//! 7. [`the_rom_reloaded_event_carries_the_same_hits_dropped_as_the_reply_that_caused_it`] — an
//!    EQUALITY between two messages, asserted as one. A client that learns of reloads by *listening* had
//!    no route to the count at all before this; a reply and an event carrying two different numbers would
//!    be legal on both schema fragments and would be worse than the silence.
//! 8. [`the_aggregates_survive_every_boundary_while_the_epoch_relative_records_are_dropped`] — ★ the
//!    positive obligation the adjudicator added, and the mirror of row 4. §11.39 ratifies the distinction
//!    for all four artifacts: a RECORD with epoch-relative fields is dropped, an AGGREGATE over an
//!    observer's life (a breakpoint's `hits`, this recorder's `seen`/`matched`/`dropped`) is KEPT.
//!    **Without row 8, an implementation that cleared everything at a boundary satisfies rows 1-7.**
//!
//! **Every row asserts the COUNT, not merely the presence of a number.** `hitsDropped` is compared
//! against the `total` read back from the ring immediately before the boundary, and the post-boundary
//! ring is asserted empty by count as well as by array. A reply that reported a plausible constant, or a
//! clear that dropped a different number of hits than it claimed, reads identically to a correct one on
//! any assertion weaker than that.
//!
//! ## Red-first, measured — which mutation reddened which row
//!
//! Each mutation was applied to a **committed, clean** tree at `d7bc6ae`, quoted back from disk before
//! the run, and restored with `git checkout --` from that commit. Recorded here rather than in a report
//! nobody will find, because the value of a row is what it is the *only* thing that catches.
//!
//! | # | mutation, in `engine.rs` | rows that went RED |
//! |---|---|---|
//! | M1 | `reload_rom`: `take_hits()` → `hits()` — count still correct, ring **kept** | 1 only |
//! | M2 | `reset`: `take_hits()` → `hits()` | 2 and 4 |
//! | M3 | `reset`: emit `hitsDropped` only when `> 0` | 3 only — and it failed on the **vendored schema's** `required`, which is the re-vendor proving itself load-bearing |
//! | M3b | `reset`: `hits_dropped.max(1)` — schema-LEGAL, so the fragment cannot see it | 3 only, on the row's own `== 0` |
//! | M4 | `watchpoint_clear` drains the ring — *the exact defect item 28's last sentence forbids* | **4 only** |
//! | M5 | both replies report a constant `0` while really draining | 1, 2 and 4 (3 stays green, correctly) |
//!
//! M3b and M5 are the two that matter for the trap this repo has hit twice. M3b is invisible to the
//! schema — a non-negative integer is a non-negative integer — so only row 3's own comparison sees it.
//! M5 leaves every "are the hits gone?" assertion true and is caught **solely** by comparing the count
//! against what the ring was holding, which is why every row does.
//!
//! M4 is the whole justification for row 4's existence: it reddens **that row and no other**. Rows 1-3
//! are green under an implementation that lets any client erase any other client's evidence.
//!
//! ### …and the seven for §11.39's four rows
//!
//! Same method: applied to a **committed, clean** tree at `342428e`, quoted back from disk before each
//! run, restored with `git checkout --` from that commit, and the run below is the run that was made.
//!
//! | # | mutation, in `engine.rs` | rows that went RED |
//! |---|---|---|
//! | M6 | `restore`: `take_hits()` → `hits()` — count still correct, ring **kept** | 5 and 8 |
//! | M7 | `restore`: return `Ok(json!({}))`, the shape it had yesterday | 5, 6 and 8 — on the **vendored schema's** `methods.emulator/restore.result: "hitsDropped" is a required property`, and it takes **9 of 20 `checkpoints.rs` rows** with it |
//! | M7b | `restore`: `hits_dropped.max(1)` — schema-LEGAL, so the fragment is blind to it | **6 only**, on the row's own `== 0` |
//! | M8 | `reload_rom`: drop `hitsDropped` from the EVENT params, keep it on the reply | 1, 3, 7 and 8 — on `events.emulator/romReloaded.params: "hitsDropped" is a required property`, plus a row in `events.rs`: `Client::recv` validates every line, so this is not one row but every test that reloads |
//! | M9 | `reload_rom`: the event recomputes the count (`take_hits().len()` again) instead of reading the one binding | **7 only** — `left: 0, right: 3` |
//! | M11 | `restore`: report a constant `0` while really draining | 5 and 8 (6 stays green, correctly) |
//! | M12 | `restore`: clear the breakpoints' `hits`, leave the watch aggregates alone | **8 only** — `left: 0, right: 1` |
//! | M10 | `restore`: clear **everything** — fresh `Watchpoints`, breakpoint `hits` zeroed | **8 only** — `seen` `left: 0, right: 179199` |
//!
//! **M10 is the whole justification for row 8, and it is the sharpest measurement in this file.** It is
//! the implementation §11.39's ratification exists to forbid — one that treats every counter as
//! epoch-relative — and it passes **rows 1 through 7 and every vector in the vendored schema**. Only row
//! 8 sees it. M12 is the same argument narrowed to the breakpoint half, so neither half of that row is
//! carrying the other.
//!
//! **M9 is the trap the two-place shape creates and the reason the count is one binding.** A recomputed
//! count is a legal non-negative integer on both fragments and identical to a correct one on every
//! assertion that reads a single message; it is caught **solely** by comparing the event to the reply
//! that caused it. M7b is §11.38's `max(1)` trap arriving at the new boundary, and it is likewise
//! invisible to the schema.

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

// ---------------------------------------------------------------------------------------------------
// Item 28 as EXTENDED by §11.39 — the restore
// ---------------------------------------------------------------------------------------------------

/// **Hits recorded AFTER the capture point are absent after an `emulator/restore`, and `hitsDropped`
/// counts them.**
///
/// ⚑ **The order of the steps is dictated by what a restore is, and getting it wrong measures something
/// else.** A restore rewinds *to* the capture point, so the only hits it can be said to drop are the ones
/// recorded **after** that point — those are the ones stamped with coordinates in a future this restore
/// has just discarded. A test that recorded hits and *then* captured would be asserting that a restore
/// drops hits belonging to the very machine it restored: a different claim, and one that happens to go
/// green here only because the ring is not part of the snapshot. Hence capture, record, restore, assert
/// absent and counted, in that order.
#[test]
fn hits_recorded_after_the_capture_point_are_absent_after_a_restore_and_hits_dropped_counts_them() {
    let (_h, mut c, path) = armed("wh-restore");

    // 1. The capture point, taken with the ring empty — everything below it is the discarded future.
    let cp = c.ok("emulator/checkpoint", json!({"label": "before the hits"}));
    let id = cp["id"].as_str().expect("a checkpoint handle").to_string();
    assert_eq!(
        hits(&mut c)["total"],
        json!(0),
        "nothing is recorded yet, so every hit below is recorded AFTER the capture point"
    );

    // 2. …and now they exist, on the far side of it.
    let (_handle, held) = record_some_hits(&mut c);

    // 3. Back to the capture point.
    let restore = c.ok("emulator/restore", json!({ "id": &id }));
    assert_eq!(
        restore["frame"], cp["frame"],
        "the restore really went back to the capture coordinate — which is what makes the hits above a \
         discarded future rather than this machine's own past"
    );
    assert_eq!(
        hits_dropped(&restore),
        held,
        "the reply counts exactly the hits the ring was holding, not a plausible constant"
    );

    // 4. Absent, by count and by array.
    let after = hits(&mut c);
    assert_eq!(after["total"], json!(0), "the ring is empty by count");
    assert_eq!(
        after["hits"].as_array().map(Vec::len),
        Some(0),
        "and by array"
    );

    let _ = std::fs::remove_file(&path);
}

/// **`hitsDropped` is `0`, present, on a restore when nothing was recorded** — row 3's control at the
/// third boundary. The machine really moves between the capture and the restore, so the zero says *"the
/// ring was empty"* rather than *"nothing happened"*.
#[test]
#[allow(non_snake_case)]
fn hits_dropped_is_zero_and_PRESENT_on_a_restore_when_nothing_was_recorded() {
    let (_h, mut c, path) = armed("wh-restore-zero");
    let id = c.ok("emulator/checkpoint", json!({}))["id"]
        .as_str()
        .expect("a checkpoint handle")
        .to_string();
    c.ok("emulator/run_frames", json!({"frames": 3}));
    assert_eq!(
        hits(&mut c)["total"],
        json!(0),
        "no watch was armed, so nothing was recorded"
    );

    let restore = c.ok("emulator/restore", json!({ "id": &id }));
    assert!(
        restore.as_object().unwrap().contains_key("hitsDropped"),
        "present, not omitted, when it is zero"
    );
    assert_eq!(restore["hitsDropped"], json!(0));
    assert_eq!(hits_dropped(&restore), 0);

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------------------
// Item 28 as EXTENDED by §11.39 — the event says the SAME number as the reply
// ---------------------------------------------------------------------------------------------------

/// **The `emulator/romReloaded` event carries the same `hitsDropped` as the reply that caused it.**
///
/// The obligation is an EQUALITY between two messages, so this asserts the two values against each other
/// as well as against the count read out of the ring a moment earlier — three ways of saying one number.
/// Asserting merely that each message carries *a* count is the weaker check the two-place shape exists to
/// defeat: a reply saying 7 beside an event saying 0 passes it, is legal on both schema fragments, and is
/// exactly the drift §11.39 warns about ("derive it once rather than computing it twice").
///
/// Both messages are read off **one** connection in the order the server wrote them, rather than through
/// `Client::ok` — which skips notifications on its way to the reply, and would leave the message under
/// test unread.
///
/// The zero case follows on the same connection: `0 == 0` still fails a server that omitted the key from
/// the event (it must be *present*), while a server that hard-coded either side passes it and fails the
/// non-zero half above.
#[test]
fn the_rom_reloaded_event_carries_the_same_hits_dropped_as_the_reply_that_caused_it() {
    let (_h, mut c, path) = armed("wh-event");
    let (_handle, held) = record_some_hits(&mut c);
    assert!(
        held > 0,
        "the interesting half of the equality needs a non-zero number on both sides"
    );

    let (reply, event) = reload_reading_the_event(&mut c, &path, 101);
    assert_eq!(
        hits_dropped(&reply),
        held,
        "the reply counts what the ring was holding"
    );
    let on_the_event = event
        .get("hitsDropped")
        .expect("`hitsDropped` is REQUIRED on the romReloaded event (§11.39)");
    assert_eq!(
        on_the_event,
        &json!(held),
        "★ the event carries the SAME count as the reply that caused it — a client that learns of \
         reloads by listening sees what the caller saw"
    );
    assert_eq!(
        on_the_event, &reply["hitsDropped"],
        "asserted against the reply directly too, so this row fails the day the two drift apart while \
         both stay schema-legal"
    );

    // And the zero, present on both — the ring is empty now, so this reload drops nothing.
    let (reply0, event0) = reload_reading_the_event(&mut c, &path, 102);
    assert_eq!(reply0["hitsDropped"], json!(0));
    assert!(
        event0.as_object().unwrap().contains_key("hitsDropped"),
        "present on the event, not omitted, when it is zero"
    );
    assert_eq!(event0["hitsDropped"], reply0["hitsDropped"]);

    let _ = std::fs::remove_file(&path);
}

/// One `emulator/reload_rom`, returning `(result, event params)` — the reply and the notification it
/// caused, read off the same connection. Hand-rolled rather than `Client::ok` because that helper skips
/// notifications, and the notification is half of what this row asserts.
fn reload_reading_the_event(c: &mut Client, path: &std::path::Path, id: i64) -> (Value, Value) {
    c.send_raw(
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "emulator/reload_rom",
            "params": {"path": path.display().to_string()},
        })
        .to_string(),
    );
    let mut event: Option<Value> = None;
    loop {
        let v = c.recv();
        if v["method"] == json!("emulator/romReloaded") {
            assert_eq!(v["params"]["path"], json!(path.display().to_string()));
            event = Some(v["params"].clone());
        }
        if v["id"] == json!(id) {
            assert!(
                v.get("error").is_none(),
                "the reload failed: {}",
                v["error"]
            );
            let event = event.expect("emulator/romReloaded was not pushed before the reply");
            return (v["result"].clone(), event);
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// §11.39's NEW positive obligation — the aggregates SURVIVE
// ---------------------------------------------------------------------------------------------------

/// ## ★ Record versus aggregate, asserted rather than assumed ★
///
/// §11.39 ratifies the distinction once, for all four artifacts: a **RECORD** with epoch-relative fields
/// (a watchpoint hit — `frame`, the cycle stamp, `pc`) is dropped at a boundary because it is
/// *uninterpretable* afterwards; an **AGGREGATE** over an observer's life (a breakpoint's `hits`, this
/// recorder's `seen`/`matched`/`dropped`) is **kept**, because it describes the recorder, whose life the
/// boundary did not end.
///
/// Item 28's extension makes that a suite obligation in its own right, and it is the mirror of the
/// client-durability row above. Without it, **an implementation that cleared everything at a boundary
/// satisfies every other row in this file**: the hits are gone, the count is right, the zero is present.
/// What such a server destroys — *"this breakpoint has fired 1,691,410 times"*, *"this instrument saw
/// 40,000 accesses and matched 12"* — is invisible to every assertion that only looks at the ring.
///
/// So this row carries a **live** breakpoint and a **live** watch across all three boundaries, and at each
/// one asserts two opposite things at the same instant: the epoch-relative records were dropped (a
/// non-zero `hitsDropped`, an empty ring), and all four aggregates are exactly what they were the line
/// before. Records are re-earned between boundaries, so every crossing is one with something real to
/// drop — a boundary that dropped nothing could not demonstrate that it kept the right things.
#[test]
fn the_aggregates_survive_every_boundary_while_the_epoch_relative_records_are_dropped() {
    let (_h, mut c, path) = armed("wh-aggregates");

    // The capture point for the third boundary, taken first so a restore is available at the end.
    let cp = c.ok("emulator/checkpoint", json!({"label": "aggregates"}))["id"]
        .as_str()
        .expect("a checkpoint handle")
        .to_string();

    // ---- one observer of each kind, both with a life longer than one epoch ------------------------
    // A breakpoint in the fixture's main loop, earned the way `tests/breakpoints.rs` earns one.
    let bp = c.ok("emulator/breakpoint_add", json!({"addr": HOT_PC}))["breakpoint"]
        .as_str()
        .expect("a breakpoint handle")
        .to_string();
    c.ok("emulator/resume", json!({}));
    c.ok("emulator/wait_for_break", json!({"timeoutMs": 5000}));
    // Disabled once it has fired, so its count is frozen and the `run_frames` below cannot move it —
    // §6: *"`hits` counts firings while enabled and is never reset by this surface"*. The number under
    // test is then the same number at every boundary, and any change to it is a real change.
    c.ok(
        "emulator/breakpoint_set_enabled",
        json!({"breakpoint": &bp, "enabled": false}),
    );
    let bp_hits = breakpoint_hits(&mut c, &bp);
    assert!(
        bp_hits > 0,
        "the breakpoint must really have fired, or 'unchanged' is a claim about zero"
    );

    // A watch wide enough to overrun the ring inside one frame, which is what makes `dropped` — the
    // aggregate easiest to clear by accident, and the one whose truth a cartridge cannot change — a
    // non-zero number.
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": "0x00FF0000", "len": 0x10000, "label": "the whole of work RAM"}),
    );

    // ---- boundary 1: the reload -------------------------------------------------------------------
    let before = earn(&mut c);
    let reload = c.ok(
        "emulator/reload_rom",
        json!({"path": path.display().to_string()}),
    );
    assert_eq!(
        hits_dropped(&reload),
        before.total,
        "the records went, and were counted"
    );
    assert_aggregates_survived(&mut c, &bp, bp_hits, &before, "emulator/reload_rom");

    // ---- boundary 2: the reset --------------------------------------------------------------------
    let before = earn(&mut c);
    let reset = c.ok("emulator/reset", json!({}));
    assert_eq!(
        hits_dropped(&reset),
        before.total,
        "the records went, and were counted"
    );
    assert_aggregates_survived(&mut c, &bp, bp_hits, &before, "emulator/reset");

    // ---- boundary 3: the restore ------------------------------------------------------------------
    let before = earn(&mut c);
    let restore = c.ok("emulator/restore", json!({ "id": &cp }));
    assert_eq!(
        hits_dropped(&restore),
        before.total,
        "the records went, and were counted"
    );
    assert_aggregates_survived(&mut c, &bp, bp_hits, &before, "emulator/restore");

    let _ = std::fs::remove_file(&path);
}

/// The PC the fixture ROM's main loop reaches every pass — `tests/breakpoints.rs`'s `HOT_PC`, quoted
/// rather than re-derived so the two files break together if the ROM moves.
const HOT_PC: &str = "0x0000020E";

/// The three lifetime aggregates of the recorder, plus the ring's current occupancy, read in one call.
struct Aggregates {
    seen: u64,
    matched: u64,
    dropped: u64,
    total: u64,
}

fn aggregates(c: &mut Client) -> Aggregates {
    let r = hits(c);
    let n = |k: &str| {
        r.get(k)
            .and_then(Value::as_u64)
            .unwrap_or_else(|| panic!("`{k}` is required on a watchpoint_hits reply, got {r}"))
    };
    Aggregates {
        seen: n("seen"),
        matched: n("matched"),
        dropped: n("dropped"),
        total: n("total"),
    }
}

/// A breakpoint's lifetime `hits`, read out of `breakpoint_list`.
fn breakpoint_hits(c: &mut Client, handle: &str) -> u64 {
    c.ok("emulator/breakpoint_list", json!({}))["breakpoints"]
        .as_array()
        .expect("breakpoints[]")
        .iter()
        .find(|b| b["breakpoint"] == json!(handle))
        .unwrap_or_else(|| panic!("{handle} is not in the list — it must survive the boundary too"))
        ["hits"]
        .as_u64()
        .expect("`hits` is required on a breakpoint row")
}

/// Run frames until the ring is holding records and every aggregate is non-zero, then snapshot them.
/// Called before each boundary so that no crossing is a crossing with nothing to drop.
fn earn(c: &mut Client) -> Aggregates {
    c.ok("emulator/run_frames", json!({"frames": 2}));
    let a = aggregates(c);
    assert!(a.total > 0, "the ring must be holding records to drop");
    assert!(a.seen > 0, "`seen` is the instrument's negative control");
    assert!(a.matched > 0, "`matched` counts accesses");
    assert!(
        a.dropped > 0,
        "`dropped` must be non-zero, or 'it survived' is a claim about zero — the wide watch exists to \
         overrun the ring within a frame"
    );
    a
}

/// The assertion §11.39's new obligation names, made at the instant after a boundary: the records are
/// gone, and **all four** aggregates are exactly what they were before it.
fn assert_aggregates_survived(
    c: &mut Client,
    bp: &str,
    bp_hits: u64,
    before: &Aggregates,
    boundary: &str,
) {
    let after = aggregates(c);
    assert_eq!(after.total, 0, "{boundary}: the record ring really emptied");
    assert_eq!(
        after.seen, before.seen,
        "{boundary}: `seen` describes the recorder, not the epoch — it must NOT be cleared"
    );
    assert_eq!(
        after.matched, before.matched,
        "{boundary}: `matched` must NOT be cleared"
    );
    assert_eq!(
        after.dropped, before.dropped,
        "{boundary}: `dropped` answers 'the ring lost some at record time', whose true answer does not \
         change because a cartridge did — it must NOT be cleared"
    );
    assert_eq!(
        breakpoint_hits(c, bp),
        bp_hits,
        "{boundary}: a breakpoint's `hits` spans every boundary (§11.39: it describes the recorder)"
    );
}
