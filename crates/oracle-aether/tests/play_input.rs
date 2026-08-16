//! `emulator/play_input` — `protocol.md` §6 (input), adopted as CR-19
//! (`docs/2026-08-16-cr19-pad-timeline.md`, ruled in `docs/2026-08-16-ruling-cr19.md`, §11.11).
//!
//! Every test is a wire round trip, so `common::Client::recv` validates each line against the vendored
//! contract schema on the way past — driving the method *is* the schema-conformance pin.
//!
//! The property under test is the one the contract makes normative and the schema cannot express: **the
//! pad at frame N is a pure function of `rows`, and of nothing else.** The two tests that matter most are
//! therefore the ones proving what does *not* leak in — a prior `hold` set, and a port no row covers.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::{json, Value};

/// The pad-**log** fixture: it writes what it reads from both controller ports, in both TH phases, to
/// `PAD_LOG_ADDR` on every poll — so a test can assert the buttons the machine actually saw.
///
/// `build_pad_poll` was the first choice and it is blind for this purpose: it exposes only **Start**, and
/// only as a backdrop colour. Four of five mutations survived against it, including the two that matter
/// most (merging the held set into the timeline, and leaving an un-driven port alone), because no
/// assertion could see the pad at all.
fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build_pad_log());
    sys.reset();
    sys
}

/// The buttons the machine saw on `port` at the last poll of the run, read back over the wire.
///
/// The machine is paused when a call returns, so the log still holds the final frame's poll: restoring the
/// held set afterwards writes the pad but cannot drive another poll.
fn seen(c: &mut Client, port: usize) -> Vec<&'static str> {
    let addr = oracle_core::testrom::PAD_LOG_ADDR + 2 * port as u32;
    let r = c.ok(
        "emulator/read_memory",
        json!({"addr": format!("0x{addr:08X}"), "len": 2}),
    );
    let hex = r["bytes"]
        .as_str()
        .unwrap()
        .trim_start_matches("0x")
        .to_string();
    let th1 = u8::from_str_radix(&hex[0..2], 16).unwrap();
    let th0 = u8::from_str_radix(&hex[2..4], 16).unwrap();
    // Active-low: a clear bit is a held button.
    let mut out = Vec::new();
    for (bit, name) in [
        (0, "up"),
        (1, "down"),
        (2, "left"),
        (3, "right"),
        (4, "b"),
        (5, "c"),
    ] {
        if th1 & (1 << bit) == 0 {
            out.push(name);
        }
    }
    for (bit, name) in [(4, "a"), (5, "start")] {
        if th0 & (1 << bit) == 0 {
            out.push(name);
        }
    }
    out.sort_unstable();
    out
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

fn play(c: &mut Client, params: Value) -> Value {
    c.ok("emulator/play_input", params)
}

// -------------------------------------------------------------------------------------------------
// The property
// -------------------------------------------------------------------------------------------------

/// **The purity property, on the source most likely to leak.** A `hold` set armed before the call MUST
/// NOT be merged into the timeline's frames — and MUST still be there afterwards, because suspending is
/// not clearing.
#[test]
fn a_prior_hold_does_not_leak_in_and_is_restored_after() {
    let h = spawn_system("pi-hold", machine(), 64);
    let mut c = client(&h);

    let held = c.ok("emulator/hold", json!({"buttons": ["right"], "down": true}));
    assert_eq!(held["held"], json!(["right"]), "armed the held set");

    let r = play(
        &mut c,
        json!({"rows": [{"start": 0, "end": 3, "buttons": ["a"]}]}),
    );
    assert_eq!(r["frames"], json!(3));
    // THE assertion: the machine saw the timeline and ONLY the timeline. A server that merged the held
    // set — the easier implementation the contract forbids by name — would show `right` here too.
    assert_eq!(
        seen(&mut c, 0),
        vec!["a"],
        "the held `right` must not leak into a timeline frame"
    );

    // Restored, unchanged — suspended for the duration, not cleared. `hold` with an empty button list
    // would be a mutation, so the read-back rides a no-op `down` toggle of a button already released.
    let after = c.ok("emulator/hold", json!({"buttons": ["left"], "down": false}));
    assert_eq!(
        after["held"],
        json!(["right"]),
        "the client's held set survives the call untouched — a button the client is holding is not \
         this method's to release"
    );
}

/// **A port no row covers is fully released** for every frame of the run — not left holding whatever it
/// had. Port 1 is given a held button and then left out of the timeline entirely.
#[test]
fn an_unrowed_port_is_released_for_the_whole_run() {
    let h = spawn_system("pi-port", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/hold",
        json!({"buttons": ["start"], "down": true, "port": 1}),
    );

    let r = play(
        &mut c,
        json!({"rows": [{"start": 0, "end": 2, "buttons": ["a"], "port": 0}]}),
    );
    assert_eq!(r["frames"], json!(2));
    assert_eq!(
        seen(&mut c, 1),
        Vec::<&str>::new(),
        "port 1 is fully released for the run — not left holding what it had"
    );
    assert_eq!(seen(&mut c, 0), vec!["a"], "port 0 played its row");

    let after = c.ok(
        "emulator/hold",
        json!({"buttons": ["b"], "down": false, "port": 1}),
    );
    assert_eq!(
        after["held"],
        json!(["start"]),
        "port 1's held set is restored afterwards even though no row named it"
    );
}

/// **Union, observed.** Two overlapping rows on one port, and the machine must see *both* contributions
/// on the overlapping frame. A later-row-wins server would show only the later row's button.
#[test]
fn overlapping_rows_union_on_the_machine() {
    let h = spawn_system("pi-union", machine(), 64);
    let mut c = client(&h);
    play(
        &mut c,
        json!({"rows": [
            {"start": 0, "end": 6, "buttons": ["right"]},
            {"start": 5, "end": 6, "buttons": ["a"]}
        ]}),
    );
    assert_eq!(
        seen(&mut c, 0),
        vec!["a", "right"],
        "the overlapping frame carries BOTH rows' buttons — later-row-wins would show only `a`"
    );
}

/// **Order-independence, on the machine.** The same row *set* in two orders must leave the machine having
/// seen the same buttons — which is what "the pad depends on the row set, not the row order" means.
///
/// Two *fresh* servers, because a second run on one server starts from the first run's state. An earlier
/// version compared `frames` from two calls on one server and would have passed for any implementation
/// whatsoever: it asserted that two 6-frame timelines both ran 6 frames.
#[test]
fn the_same_row_set_in_any_order_is_seen_identically() {
    let rows_sorted = json!([
        {"start": 0, "end": 6, "buttons": ["right"]},
        {"start": 5, "end": 6, "buttons": ["a"]}
    ]);
    let rows_reversed = json!([
        {"start": 5, "end": 6, "buttons": ["a"]},
        {"start": 0, "end": 6, "buttons": ["right"]}
    ]);

    let ha = spawn_system("pi-order-a", machine(), 64);
    let mut a = client(&ha);
    play(&mut a, json!({"rows": rows_sorted}));
    let seen_a = seen(&mut a, 0);

    let hb = spawn_system("pi-order-b", machine(), 64);
    let mut b = client(&hb);
    play(&mut b, json!({"rows": rows_reversed}));
    let seen_b = seen(&mut b, 0);

    assert_eq!(seen_a, vec!["a", "right"]);
    assert_eq!(seen_a, seen_b, "row order is not load-bearing");
}

// -------------------------------------------------------------------------------------------------
// Bounds, truncation, and the frame count
// -------------------------------------------------------------------------------------------------

/// `frames` is the timeline's length by default, and `maxFrames` **truncates** it.
#[test]
fn max_frames_truncates_and_defaults_to_the_largest_end() {
    let h = spawn_system("pi-max", machine(), 64);
    let mut c = client(&h);

    let full = play(
        &mut c,
        json!({"rows": [{"start": 0, "end": 5, "buttons": ["a"]}]}),
    );
    assert_eq!(
        full["frames"],
        json!(5),
        "absent maxFrames = the largest end"
    );

    let cut = play(
        &mut c,
        json!({"rows": [{"start": 0, "end": 5, "buttons": ["a"]}], "maxFrames": 2}),
    );
    assert_eq!(
        cut["frames"],
        json!(2),
        "maxFrames below the largest end truncates"
    );

    // **Adoption condition, branch 2: a reply carrying `frames: 0`.** CR-17 made the zero reachable and
    // the schema had to be amended to allow it; here it arrives by truncation rather than by a watch.
    let none = play(
        &mut c,
        json!({"rows": [{"start": 0, "end": 5, "buttons": ["a"]}], "maxFrames": 0}),
    );
    assert_eq!(
        none["frames"],
        json!(0),
        "exact, including zero — never rounded up to a frame that did not run"
    );
}

/// Every refusal the contract pins, each `-32602` and each naming what was wrong.
#[test]
fn malformed_timelines_are_refused_and_never_silently_dropped() {
    let h = spawn_system("pi-refuse", machine(), 64);
    let mut c = client(&h);

    let cases: Vec<(&str, Value)> = vec![
        ("empty rows", json!({"rows": []})),
        (
            "end <= start",
            json!({"rows": [{"start": 5, "end": 5, "buttons": ["a"]}]}),
        ),
        (
            "bad port",
            json!({"rows": [{"start": 0, "end": 1, "buttons": ["a"], "port": 2}]}),
        ),
        (
            "6-button name with sixButtonPad false",
            json!({"rows": [{"start": 0, "end": 1, "buttons": ["x"]}]}),
        ),
        ("rows missing", json!({})),
    ];
    for (what, params) in cases {
        let e = c.err("emulator/play_input", params);
        assert_eq!(e["code"], json!(-32602), "{what} must be refused");
    }
}

/// It is run control: refused on a free-running machine rather than pausing implicitly (§5).
#[test]
fn it_is_refused_while_the_machine_is_free_running() {
    let h = spawn_system("pi-running", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));
    let e = c.err(
        "emulator/play_input",
        json!({"rows": [{"start": 0, "end": 1, "buttons": ["a"]}]}),
    );
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("machineRunning"));
}

/// **Adoption condition, branch 1: the completed-timeline reply, key set exact.** The schema's `result`
/// carries no `additionalProperties: false`, so a surplus key passes the validator and only this catches
/// it — and the four keys struck by the ruling (`reason`, `stoppedAt`, `rowsApplied`, `ports`) must not
/// have crept back.
#[test]
fn the_key_set_is_exact_and_the_struck_keys_stayed_struck() {
    use std::collections::BTreeSet;
    let h = spawn_system("pi-keys", machine(), 64);
    let mut c = client(&h);
    let r = play(
        &mut c,
        json!({"rows": [{"start": 0, "end": 2, "buttons": ["a"]}]}),
    );
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    let want: BTreeSet<&str> = [
        "frames",
        "frameToken",
        "pc",
        "frame",
        "mclk",
        "running",
        "droppedEvents",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want, "run_frames' own shape, and nothing more");
    for struck in ["reason", "stoppedAt", "rowsApplied", "ports"] {
        assert!(
            r.get(struck).is_none(),
            "`{struck}` was struck by the ruling and must not return"
        );
    }
}

/// The row bound is discoverable rather than learned by being refused.
#[test]
fn the_row_bound_is_advertised_in_limits() {
    let h = spawn_system("pi-limits", machine(), 64);
    let mut c = Client::connect(&h);
    let hello = c.handshake(false);
    assert!(
        hello["limits"]["maxInputRows"].as_u64().unwrap() >= 1,
        "limits.maxInputRows is advertised: a client that must hit a limit to learn it loses the work \
         it was doing when it found out"
    );
}

/// **Adoption condition, the branch the first version of this file never exercised: a watch cuts the run
/// short.** A watch armed on the address the fixture writes every poll fires inside frame 0, so the reply
/// carries `frames: 0` from a *watch* rather than from truncation — which is the case CR-17 amended the
/// schema for, and the case the `frames` accounting has a separate code path for.
#[test]
fn a_watch_can_cut_the_timeline_short_and_frames_is_exact() {
    let h = spawn_system("pi-watch", machine(), 64);
    let mut c = client(&h);
    let w = c.ok(
        "emulator/watchpoint_add",
        json!({
            "addr": format!("0x{:08X}", oracle_core::testrom::PAD_LOG_ADDR),
            "write": true,
            "stopAfter": 1
        }),
    );
    assert!(w["watch"].is_string(), "armed a watch handle");

    let r = play(
        &mut c,
        json!({"rows": [{"start": 0, "end": 30, "buttons": ["a"]}]}),
    );
    assert_eq!(
        r["frames"],
        json!(0),
        "the watch fired inside the first frame, so no whole frame completed — exact, not rounded to 1"
    );
    assert!(
        r["frames"].as_u64().unwrap() < 30,
        "the timeline did not run to its end"
    );
}
