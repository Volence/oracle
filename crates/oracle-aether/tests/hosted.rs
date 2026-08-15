//! The **hosted** arrangement, over a real socket: a miniature of the player's run loop owns the `System`
//! and drains the capability layer once per iteration, and a real NDJSON client talks to it.
//!
//! These tests deliberately do not share `tests/common/mod.rs`. That harness spawns a
//! [`Server`](oracle_aether::server::Server), which is the other arrangement — the one where the bus owns the
//! machine on a thread of its own. What has to be checked here is precisely the difference: that a client
//! reaches the *player's* machine, that the two sides agree about who is running it, and that neither can
//! stall the other.

#![cfg(unix)]

use oracle_aether::host::{Host, HostConfig, MachineInfo, HOSTED_MAX_RUN_FRAMES};
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn temp_socket(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("ah-{tag}-{}-{n}.sock", std::process::id()))
}

/// A miniature of `oracle-frontend`'s run loop: it owns the machine, runs it through a scanline capture,
/// publishes the completed frame, drains the bus, and follows the bus's pause state. Every ordering decision
/// here matches the real loop, because the orderings are what these tests are about.
struct Player {
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    /// Wall-clock iterations completed — the "is the loop still turning?" probe a wedged emulator would
    /// freeze. Deliberately not the emulated frame count, which a *paused* player also freezes.
    iterations: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Player {
    fn start(tag: &str) -> Self {
        let socket = temp_socket(tag);
        let stop = Arc::new(AtomicBool::new(false));
        let iterations = Arc::new(AtomicU64::new(0));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let (t_socket, t_stop, t_iters) =
            (socket.clone(), Arc::clone(&stop), Arc::clone(&iterations));
        let thread = std::thread::Builder::new()
            .name("test-player".into())
            .spawn(move || {
                let mut sys = System::new(0x5EED);
                sys.load_rom(oracle_core::testrom::build());
                sys.reset();

                let mut host = Host::new(HostConfig::default());
                host.set_machine_info(MachineInfo {
                    rom_path: Some("testrom".into()),
                    ..MachineInfo::default()
                });
                host.serve(Some(t_socket)).expect("bind the hosted socket");
                ready_tx.send(()).ok();

                let mut cap = ScanlineCapture::new(Retain::LastFrame);
                let mut paused = false;
                while !t_stop.load(Ordering::SeqCst) {
                    if !paused {
                        sys.run_frames_with_sink(1, &mut cap);
                        host.publish_capture(&cap);
                        cap.clear();
                    }
                    host.set_paused(paused);
                    host.pump(&mut sys);
                    paused = host.is_paused();
                    t_iters.fetch_add(1, Ordering::SeqCst);
                    // Stand in for the window's 60 Hz present. Short enough that the tests are quick,
                    // long enough that a stalled drain would be obvious.
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
            .expect("spawn the test player");
        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the player bound its socket");
        Self {
            socket,
            stop,
            iterations,
            thread: Some(thread),
        }
    }

    fn iterations(&self) -> u64 {
        self.iterations.load(Ordering::SeqCst)
    }

    /// Block until the loop has turned over at least `n` more times, or fail. This is the liveness probe:
    /// anything that wedges the player fails here rather than hanging the suite forever.
    fn expect_progress(&self, n: u64, why: &str) {
        let from = self.iterations();
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.iterations() < from + n {
            assert!(
                Instant::now() < deadline,
                "the player stopped turning: {why}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// One NDJSON connection, hand-rolled so the tests exercise the wire.
struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: i64,
}

impl Client {
    fn connect(p: &Player) -> Self {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match UnixStream::connect(&p.socket) {
                Ok(s) => {
                    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
                    return Self {
                        reader: BufReader::new(s.try_clone().unwrap()),
                        writer: s,
                        next_id: 1,
                    };
                }
                Err(e) => {
                    assert!(Instant::now() < deadline, "connect: {e}");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    fn send_raw(&mut self, line: &str) {
        self.writer.write_all(line.as_bytes()).unwrap();
        self.writer.write_all(b"\n").unwrap();
        self.writer.flush().unwrap();
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send_raw(
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string(),
        );
        loop {
            let mut line = String::new();
            let n = self.reader.read_line(&mut line).expect("read");
            assert!(n > 0, "connection closed while a reply was expected");
            let v: Value = serde_json::from_str(&line).expect("bad JSON on the wire");
            if v.get("id").is_some_and(|i| !i.is_null()) {
                assert_eq!(v["id"], json!(id), "response id must correlate");
                return v;
            }
        }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params);
        assert!(v.get("error").is_none(), "{method} failed: {}", v["error"]);
        v["result"].clone()
    }

    fn err(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params);
        assert!(v.get("result").is_none(), "{method} unexpectedly succeeded");
        v["error"].clone()
    }

    fn handshake(&mut self, events: bool) -> Value {
        let r = self.ok(
            "initialize",
            json!({
                "clientId": "hosted-test",
                "clientName": "hosted",
                "clientVersion": "0",
                "protocolVersion": 1,
                "clientCapabilities": {"events": events},
            }),
        );
        self.send_raw(&json!({"jsonrpc":"2.0","method":"initialized"}).to_string());
        r
    }
}

/// The point of the whole exercise: a client on the socket is reading the machine the *player* is running,
/// stamped with that machine's own advancing clocks (D11).
#[test]
fn a_client_reads_the_players_live_machine() {
    let p = Player::start("live");
    let mut c = Client::connect(&p);
    c.handshake(false);

    let a = c.call("emulator/status", json!({}));
    assert_eq!(a["result"]["romPath"], json!("testrom"));
    assert_eq!(
        a["result"]["running"],
        json!(true),
        "an un-paused player is a free-running bus"
    );
    p.expect_progress(30, "reading status must not stop the loop");
    let b = c.call("emulator/status", json!({}));
    assert!(
        b["result"]["frame"].as_u64().unwrap() > a["result"]["frame"].as_u64().unwrap(),
        "the stamp is the player's own emulated clock, and it advanced"
    );
    assert!(
        b["result"]["mclk"].as_u64().unwrap() > a["result"]["mclk"].as_u64().unwrap(),
        "both clocks, and both emulated"
    );
}

/// **Conflict 1, both directions.** While the player is advancing the machine, a client-driven run is
/// refused with `-32005 machineRunning` per §6's run-control state rule; once the client pauses, the player
/// stops advancing and the same run is allowed.
#[test]
fn run_control_is_one_flag_shared_with_the_player() {
    let p = Player::start("runctl");
    let mut c = Client::connect(&p);
    c.handshake(false);

    let e = c.err("emulator/run_frames", json!({"frames": 1}));
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("machineRunning"));

    assert_eq!(c.ok("emulator/pause", json!({}))["wasRunning"], json!(true));
    // The player must actually stop, or "paused" would be a word the bus says and nothing obeys.
    p.expect_progress(20, "a paused player still turns its loop");
    let before = c.ok("emulator/status", json!({}))["frameToken"]
        .as_u64()
        .unwrap();
    p.expect_progress(30, "still turning");
    let after = c.ok("emulator/status", json!({}))["frameToken"]
        .as_u64()
        .unwrap();
    assert_eq!(before, after, "a client's pause really stops the player");

    // …and now the client owns the machine, so the bounded run is permitted and lands exactly.
    let r = c.ok("emulator/run_frames", json!({"frames": 3}));
    assert_eq!(r["frames"], json!(3));
    assert_eq!(r["frameToken"].as_u64().unwrap(), after + 3);

    // Resuming hands it back: the player starts advancing again.
    c.ok("emulator/resume", json!({}));
    p.expect_progress(30, "resumed");
    let running = c.ok("emulator/status", json!({}))["frameToken"]
        .as_u64()
        .unwrap();
    assert!(running > after + 3, "the player took the machine back");
}

/// **Conflict 3, and the screen fix.** The picture a client is served is the one the raster drew, published
/// by the player's own capture — not a post-hoc re-render of the VDP state.
#[test]
fn the_screen_served_is_the_frame_the_raster_drew() {
    let p = Player::start("screen");
    let mut c = Client::connect(&p);
    c.handshake(false);
    p.expect_progress(10, "let a frame or two be drawn");

    let path = std::env::temp_dir().join(format!("ah-shot-{}.ppm", std::process::id()));
    let r = c.ok(
        "emulator/screenshot",
        json!({"path": path.display().to_string()}),
    );
    assert_eq!(
        r["source"],
        json!("raster"),
        "the published frame is used, so this is a scanline capture"
    );
    assert!(
        r.get("caveat").is_none(),
        "and the not-scanline-accurate caveat must NOT be attached to a frame that is"
    );
    let bytes = std::fs::read(&path).expect("the PPM exists");
    let (w, h) = (
        r["width"].as_u64().unwrap() as usize,
        r["height"].as_u64().unwrap() as usize,
    );
    assert_eq!(bytes.len(), format!("P6\n{w} {h}\n255\n").len() + w * h * 3);
    let _ = std::fs::remove_file(&path);

    // The same frame backs the framebuffer fingerprint, and it says which picture it hashed.
    let h = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert_eq!(h["framebufferSource"], json!("raster"));
}

/// A client-driven run happens with the player's own sinks detached, so the frame it draws would otherwise
/// never reach the glass. It does: the bus latches it, and the player pulls it back.
#[test]
fn a_client_driven_run_leaves_a_presentable_frame_behind() {
    let p = Player::start("cdrun");
    let mut c = Client::connect(&p);
    c.handshake(false);
    c.ok("emulator/pause", json!({}));
    p.expect_progress(20, "paused");

    let before = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert_eq!(
        before["framebufferSource"],
        json!("raster"),
        "the player published a frame before it was paused"
    );

    // The player's own sinks are not attached to this run — it happens entirely inside the drain — so
    // without the engine's own capture there would be no frame to serve afterwards at all.
    let r = c.ok("emulator/run_frames", json!({"frames": 8}));
    assert_eq!(r["frames"], json!(8));
    let after = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert_eq!(
        after["framebufferSource"],
        json!("raster"),
        "the run left a raster-drawn frame behind, not a post-hoc state render"
    );
    // The player is still paused, so nothing has published over it since.
    p.expect_progress(20, "still paused and still turning");
    let again = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert_eq!(again["framebuffer"], after["framebuffer"]);
}

/// **The long-run bound.** Hosted, one command may not freeze the window for a minute, so the ceiling is
/// lowered — and it is a refusal that names the limit, with the limit advertised up front.
#[test]
fn the_hosted_run_ceiling_is_advertised_and_refused_not_clamped() {
    let p = Player::start("bound");
    let mut c = Client::connect(&p);
    let init = c.handshake(false);
    assert_eq!(
        init["limits"]["maxRunFrames"],
        json!(HOSTED_MAX_RUN_FRAMES),
        "a client discovers the bound before it hits it"
    );

    c.ok("emulator/pause", json!({}));
    let e = c.err(
        "emulator/run_frames",
        json!({"frames": HOSTED_MAX_RUN_FRAMES + 1}),
    );
    assert_eq!(e["code"], json!(-32602), "refused, never silently clamped");
    let r = c.ok(
        "emulator/run_frames",
        json!({"frames": HOSTED_MAX_RUN_FRAMES}),
    );
    assert_eq!(r["frames"], json!(HOSTED_MAX_RUN_FRAMES));
    // The player is still alive on the far side of the longest run it can be asked for.
    p.expect_progress(10, "the loop survives a full-length run");
}

/// **The slow-client guarantee, hosted.** A subscriber that never reads a byte cannot stall the player's
/// frame loop — the drain writes to no socket, and events go into a bounded drop-oldest queue.
#[test]
fn a_client_that_never_reads_cannot_stall_the_player() {
    let p = Player::start("slow");
    // A live client that subscribes to events and then goes silent. Nothing here ever reads.
    let mut dead = UnixStream::connect(&p.socket).expect("connect");
    let hs = json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"clientId":"dead","clientName":"dead","clientVersion":"0",
                  "protocolVersion":1,"clientCapabilities":{"events":true}}
    });
    dead.write_all(format!("{hs}\n").as_bytes()).unwrap();
    dead.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"initialized\"}\n")
        .unwrap();
    dead.flush().unwrap();

    // A second, healthy client generates a flood of events (each bounded run emits resumed + stopped) that
    // the dead one will never drain.
    let mut c = Client::connect(&p);
    c.handshake(false);
    c.ok("emulator/pause", json!({}));
    for _ in 0..600 {
        c.ok("emulator/run_frames", json!({"frames": 1}));
    }
    // The healthy client is still being answered, and the player is still turning.
    p.expect_progress(20, "a dead subscriber must not wedge the loop");
    assert!(c.ok("emulator/status", json!({}))["frameToken"]
        .as_u64()
        .is_some());
    drop(dead);
}
