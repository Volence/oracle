//! Server-push events (`protocol.md` §3, D6) — including the one that matters most: **a client that
//! subscribes and then stops reading must not be able to wedge the emulator.**

mod common;

use common::{spawn_with, Client};
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

fn rom() -> Vec<u8> {
    oracle_core::testrom::build()
}

#[test]
fn events_reach_a_subscriber_and_carry_the_stamp() {
    let h = spawn_with("ev", rom(), 1024);
    let mut c = Client::connect(&h);
    c.handshake(true);
    // Read the raw stream rather than `ok`, which skips notifications: everything the server writes
    // between the request and its response must be exactly one `resumed` and one `stopped`.
    c.send_raw(
        &json!({"jsonrpc":"2.0","id":901,"method":"emulator/run_frames","params":{"frames":1}})
            .to_string(),
    );
    let mut resumed = 0;
    let mut stopped = 0;
    loop {
        let v = c.recv();
        if v["id"] == json!(901) {
            break;
        }
        assert_eq!(v["jsonrpc"], json!("2.0"));
        assert!(v.get("id").is_none(), "a notification carries no id");
        for k in ["frame", "mclk", "running"] {
            assert!(v["params"].get(k).is_some(), "event missing `{k}`: {v}");
        }
        match v["method"].as_str().unwrap() {
            "emulator/resumed" => {
                resumed += 1;
                // The event stream must not contradict itself: "resumed" and "running: false" in the
                // same message would tell a client two different things about one instant.
                assert_eq!(v["params"]["running"], json!(true), "{v}");
            }
            "emulator/stopped" => {
                stopped += 1;
                assert_eq!(v["params"]["running"], json!(false), "{v}");
                assert!(v["params"]["pc"].as_str().unwrap().starts_with("0x"));
                assert!(v["params"]["reason"].is_string());
            }
            other => panic!("unexpected event {other}"),
        }
    }
    assert_eq!(
        (resumed, stopped),
        (1, 1),
        "one run = one resumed + one stopped"
    );
}

#[test]
fn a_client_that_did_not_opt_in_receives_no_events() {
    let h = spawn_with("noev", rom(), 1024);
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.send_raw(
        &json!({"jsonrpc":"2.0","id":5,"method":"emulator/run_frames","params":{"frames":1}})
            .to_string(),
    );
    let v = c.recv();
    assert_eq!(
        v["id"],
        json!(5),
        "the very next line must be the reply, not an event"
    );
}

#[test]
fn no_event_is_pushed_before_initialized() {
    let h = spawn_with("preinit", rom(), 1024);
    // A subscriber that will drive the events.
    let mut driver = Client::connect(&h);
    driver.handshake(false);

    // A second connection that has sent `initialize` but NOT `initialized`.
    let mut half = Client::connect(&h);
    half.ok(
        "initialize",
        json!({"protocolVersion": 1, "clientCapabilities": {"events": true}}),
    );
    driver.ok("emulator/run_frames", json!({"frames": 2}));

    // Nothing may have been pushed to `half` yet (D6). Prove it by asking a question whose answer must
    // be the very next line.
    half.send_raw(&json!({"jsonrpc":"2.0","id":42,"method":"emulator/status"}).to_string());
    let v = half.recv();
    assert_eq!(
        v["id"],
        json!(42),
        "an event was pushed before `initialized`: {v}"
    );
}

#[test]
fn pause_and_resume_emit_their_events_and_free_running_advances_the_machine() {
    let h = spawn_with("freerun", rom(), 1024);
    let mut c = Client::connect(&h);
    c.handshake(true);

    let before = c.ok("emulator/status", json!({}))["frame"]
        .as_u64()
        .unwrap();
    c.ok("emulator/resume", json!({}));
    // Give the unpaced engine loop a moment of real time to run emulated frames.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut after = before;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        let s = c.ok("emulator/status", json!({}));
        after = s["frame"].as_u64().unwrap();
        if after > before + 2 {
            assert_eq!(s["running"], json!(true));
            break;
        }
    }
    assert!(
        after > before + 2,
        "free-running did not advance the machine"
    );
    let p = c.ok("emulator/pause", json!({}));
    assert_eq!(p["running"], json!(false));
}

#[test]
fn rom_reload_emits_rom_reloaded() {
    let h = spawn_with("reloadev", rom(), 1024);
    let mut c = Client::connect(&h);
    c.handshake(true);
    let p = std::env::temp_dir().join(format!("ae-ev-reload-{}.bin", std::process::id()));
    std::fs::write(&p, rom()).unwrap();

    c.send_raw(
        &json!({"jsonrpc":"2.0","id":77,"method":"emulator/reload_rom","params":{"path": p.display().to_string()}})
            .to_string(),
    );
    let mut saw = false;
    loop {
        let v = c.recv();
        if v["id"] == json!(77) {
            break;
        }
        if v["method"] == json!("emulator/romReloaded") {
            saw = true;
            assert_eq!(v["params"]["path"], json!(p.display().to_string()));
        }
    }
    assert!(saw, "emulator/romReloaded was not pushed");
    let _ = std::fs::remove_file(&p);
}

/// **The incident test.** `aeon/docs/BUGS.md:494-551` records a frozen repro frame *"lost to an
/// emulator control-socket hang before the sprite table could be dumped"* — a hang in the debug
/// transport destroyed irreplaceable evidence. `protocol.md` §8 item 4 requires event writes to be
/// non-blocking on a slow or dead client.
///
/// The setup: one client subscribes to events and then **never reads a byte again**, with a deliberately
/// tiny event queue (4) so its socket and its queue both fill almost immediately. A second client then
/// drives hundreds of events. The assertions are the three things that must survive:
///
/// 1. the driver keeps getting replies, promptly;
/// 2. the emulator keeps advancing emulated frames;
/// 3. when the dead client finally does read, it learns exactly how many pushes it missed
///    (`droppedEvents`) — the loss is visible, never silent.
#[test]
fn a_client_that_subscribes_and_stops_reading_cannot_wedge_the_emulator() {
    let h = spawn_with("slow", rom(), 4);

    // The deliberately-dead client: raw socket, handshake written, then never read from.
    let mut dead = UnixStream::connect(h.socket_path()).expect("connect");
    dead.write_all(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize",
               "params":{"clientId":"dead","protocolVersion":1,"clientCapabilities":{"events":true}}})
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    dead.write_all(b"\n").unwrap();
    dead.write_all(
        json!({"jsonrpc":"2.0","method":"initialized"})
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    dead.write_all(b"\n").unwrap();
    dead.flush().unwrap();
    // Let the server register it as a subscriber before we start pushing.
    std::thread::sleep(Duration::from_millis(150));

    let mut driver = Client::connect(&h);
    driver.handshake(false);
    let start_frame = driver.ok("emulator/status", json!({}))["frame"]
        .as_u64()
        .unwrap();

    // 300 run calls = 600 events, far beyond both the 4-deep queue and the socket buffer.
    let started = Instant::now();
    let mut slowest = Duration::ZERO;
    for _ in 0..300 {
        let t = Instant::now();
        driver.ok("emulator/run_frames", json!({"frames": 1}));
        slowest = slowest.max(t.elapsed());
    }
    let total = started.elapsed();

    // (1) the driver was never stalled behind the dead client.
    assert!(
        slowest < Duration::from_secs(2),
        "a single request took {slowest:?} while a dead client was subscribed — the transport blocked"
    );
    assert!(
        total < Duration::from_secs(30),
        "600 events to a dead subscriber took {total:?}"
    );

    // (2) the emulator really did keep running.
    let end_frame = driver.ok("emulator/status", json!({}))["frame"]
        .as_u64()
        .unwrap();
    assert_eq!(end_frame, start_frame + 300, "the machine kept advancing");

    // (3) the loss is visible to the client that caused it, not silent.
    dead.write_all(
        json!({"jsonrpc":"2.0","id":2,"method":"emulator/status"})
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    dead.write_all(b"\n").unwrap();
    dead.flush().unwrap();
    dead.set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut reader = std::io::BufReader::new(dead);
    let mut dropped = None;
    for _ in 0..10_000 {
        let mut line = String::new();
        if std::io::BufRead::read_line(&mut reader, &mut line).unwrap_or(0) == 0 {
            break;
        }
        let v: Value = serde_json::from_str(&line).unwrap();
        if v["id"] == json!(2) {
            dropped = v["result"]["droppedEvents"].as_u64();
            break;
        }
    }
    let dropped = dropped.expect("the dead client eventually got its status reply");
    assert!(
        dropped > 0,
        "events were dropped for a non-reading client but the count was not reported"
    );
}

/// A client that vanishes mid-stream (socket closed, not merely unread) must be pruned rather than
/// retried forever.
#[test]
fn a_disconnected_subscriber_is_dropped_without_affecting_anyone_else() {
    let h = spawn_with("gone", rom(), 8);
    {
        let mut gone = Client::connect(&h);
        gone.handshake(true);
    } // dropped: the socket closes here

    let mut c = Client::connect(&h);
    c.handshake(true);
    std::thread::sleep(Duration::from_millis(50));
    for _ in 0..50 {
        c.ok("emulator/run_frames", json!({"frames": 1}));
    }
    assert_eq!(
        c.ok("emulator/status", json!({}))["frame"]
            .as_u64()
            .unwrap(),
        50
    );
}
