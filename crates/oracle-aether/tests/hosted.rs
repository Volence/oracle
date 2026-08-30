//! The **hosted** arrangement, over a real socket: a miniature of the player's run loop owns the `System`
//! and drains the capability layer once per iteration, and a real NDJSON client talks to it.
//!
//! These tests deliberately do not share `tests/common/mod.rs`. That harness spawns a
//! [`Server`](oracle_aether::server::Server), which is the other arrangement — the one where the bus owns the
//! machine on a thread of its own. What has to be checked here is precisely the difference: that a client
//! reaches the *player's* machine, that the two sides agree about who is running it, and that neither can
//! stall the other.

#![cfg(unix)]

use oracle_aether::engine::{ScreenSurface, ScreenSurfaceKind};
use oracle_aether::host::{Host, HostConfig, MachineInfo, HOSTED_MAX_RUN_FRAMES};
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
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
        Self::start_with(tag, None)
    }

    /// A player that also publishes screen text once per iteration, the way the real run loop publishes it
    /// once per present (`oracle-frontend/src/main.rs`, at the bottom of the present block).
    ///
    /// `None` is a player that never pushes — the state the real one is in before its first present, and
    /// the state a `--no-default-features` build is in forever.
    fn start_with(tag: &str, screen: Option<Vec<ScreenSurface>>) -> Self {
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
                        // The sink expression is `oracle-frontend/src/main.rs`'s, verbatim in shape: the
                        // capture, the two lent instruments, and the **bare** breakpoint sink in the outer
                        // `Fanout` where nothing can drop its stop signal. `resume_pc` is read before the
                        // run because the engine, holding its placeholder `System` outside a drain, cannot
                        // read it for itself.
                        let resume_pc = sys.cpu_regs().pc;
                        let (watch, prof, mut brk) = host.run_sinks(resume_pc);
                        {
                            let mut sink = oracle_core::bus::Fanout::new(
                                &mut cap,
                                oracle_core::bus::Fanout::new(
                                    &mut brk,
                                    oracle_core::bus::Fanout::new(watch, prof),
                                ),
                            );
                            sys.run_frames_with_sink(1, &mut sink);
                        }
                        // The loop's whole obligation to the breakpoint surface: hand the observation back
                        // so the halt is counted, the run flags cleared and the `stopped` emitted.
                        if let Some((_, addr)) = brk.and_then(|b| b.fired) {
                            host.record_break(addr);
                        }
                        host.publish_capture(&cap);
                        cap.clear();
                    }
                    // **Where the real loop pushes it**: after every surface has finished drawing and
                    // before the next drain, so what a client reads is the frame that is on the glass.
                    if let Some(s) = &screen {
                        host.set_screen_text(s.clone());
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
    /// Every notification seen while reading for a reply, kept rather than dropped.
    ///
    /// `call` used to discard these, which is fine when the tests are about replies. The breakpoint halt
    /// is about an **event**, and an event can arrive before the reply that provoked it — a `call` that
    /// discarded it would make "the stop was announced" untestable and, worse, make "the stop was
    /// announced 374,011 times" look identical to "once".
    events: Vec<Value>,
}

impl Client {
    fn connect(p: &Player) -> Self {
        Self::connect_path(&p.socket)
    }

    /// The same connect against a bare socket path — for the one test that runs the host loop itself
    /// rather than handing it to [`Player`], because what it has to observe is the `PumpReport`.
    fn connect_path(socket: &Path) -> Self {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match UnixStream::connect(socket) {
                Ok(s) => {
                    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
                    return Self {
                        reader: BufReader::new(s.try_clone().unwrap()),
                        writer: s,
                        next_id: 1,
                        events: Vec::new(),
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
            self.events.push(v);
        }
    }

    /// Every `emulator/stopped` seen so far, params only. The **count** is load-bearing: a halt that
    /// cleared one run flag and not the other re-broke once per frame forever, and the only thing that
    /// tells that apart from a correct halt is how many of these there are.
    fn stops(&self) -> Vec<&Value> {
        self.events
            .iter()
            .filter(|v| v["method"] == json!("emulator/stopped"))
            .map(|v| &v["params"])
            .collect()
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

    let path = std::env::temp_dir().join(format!("ah-shot-{}.png", std::process::id()));
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
    let bytes = std::fs::read(&path).expect("the PNG exists");
    let (w, h) = (
        r["width"].as_u64().unwrap() as u32,
        r["height"].as_u64().unwrap() as u32,
    );
    assert_eq!(&bytes[12..16], b"IHDR");
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), w);
    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), h);
    assert_eq!(r["bytes"].as_u64().unwrap() as usize, bytes.len());
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

/// **A client-driven `emulator/reset` reaches the player as `rom_changed`.** The machine a hosted
/// player is holding was replaced under it between two of its own frames, so everything it derives
/// from that machine — a symbol listing, a save-state fingerprint, its audio clock — is stale. That
/// is the same signal `restore` and `reload_rom` raise, and it is the whole reason the reset handler
/// bumps the ROM generation.
///
/// This test runs the host loop in the test thread rather than through [`Player`], because the thing
/// under test is the `PumpReport` itself and only the loop sees one.
#[test]
fn a_client_reset_reaches_the_player_as_a_rom_change() {
    let socket = temp_socket("reset-report");
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();

    let mut host = Host::new(HostConfig::default());
    host.set_machine_info(MachineInfo {
        rom_path: Some("testrom".into()),
        ..MachineInfo::default()
    });
    host.serve(Some(socket.clone())).expect("bind the socket");

    let client = std::thread::spawn(move || {
        let mut c = Client::connect_path(&socket);
        c.handshake(false);
        c.ok("emulator/reset", json!({}))
    });

    // Drain until the reset lands. A handler that did not bump the generation never sets the flag, and
    // this fails on the deadline rather than hanging the suite.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut rom_changed = false;
    let mut calls = 0;
    while !rom_changed {
        assert!(
            Instant::now() < deadline,
            "the reset never surfaced as rom_changed"
        );
        let r = host.pump(&mut sys);
        calls += r.calls;
        rom_changed = r.rom_changed;
        std::thread::sleep(Duration::from_millis(1));
    }

    let result = client.join().expect("the client thread");
    assert_eq!(result["deferred"], json!(false));
    assert!(calls >= 2, "initialize and the reset were both answered");
}

// ---------------------------------------------------------------------------------------------------
// The hosted breakpoint halt (`docs/2026-08-27-bp-hosted-halt.md`).
//
// These are the tests the parcel exists for. Until it shipped, everything below timed out: the player's
// loop carried no breakpoint sink, and every bounded run that does carry one is refused
// `-32005 machineRunning` while the player plays — so `resume` -> `wait_for_break`, the documented idiom,
// was exactly and only the broken path, and it failed by answering `{"timeoutReached": true}` with
// `hits: 0`, which is what "the ROM never reached that address" looks like.
// ---------------------------------------------------------------------------------------------------

/// The head of the fixture ROM's inner stirring loop, which it executes constantly.
///
/// Not copied from a neighbouring pin: [`assert_hot_pc_is_the_stirring_loop`] reads the opcode back out
/// of the ROM image this fixture actually loads, so the address is anchored to the *instruction* rather
/// than to a number, and a ROM change breaks it loudly instead of silently un-arming the fixture.
const HOT_PC: u32 = 0x0000_020E;

/// The negative control, taken from the core's own public constant rather than re-typed: the
/// illegal-instruction handler, reachable only through vector 4, which this ROM's main loop cannot take.
const COLD_PC: u32 = oracle_core::testrom::TRAP_HANDLER_ADDR;

fn hex_addr(v: u32) -> String {
    format!("0x{v:08X}")
}

/// `HOT_PC` names `move.w (A0), D0` in the ROM [`Player`] loads (`testrom.rs`: *"$00020E  inner: move.w
/// (A0), D0"*, encoding `$3010`). Checked rather than asserted in prose, because every test below is
/// vacuous if this address stopped being hot.
fn assert_hot_pc_is_the_stirring_loop() {
    let rom = oracle_core::testrom::build();
    let a = HOT_PC as usize;
    assert!(a + 1 < rom.len(), "HOT_PC is outside the fixture ROM");
    let op = u16::from_be_bytes([rom[a], rom[a + 1]]);
    assert_eq!(
        op, 0x3010,
        "{} is no longer `move.w (A0),D0` — the fixture ROM moved and every breakpoint test below is \
         armed at a dead address",
        hex_addr(HOT_PC)
    );
    let c = COLD_PC as usize;
    assert_ne!(
        u16::from_be_bytes([rom[c], rom[c + 1]]),
        0x3010,
        "the cold control must not be the hot loop"
    );
}

/// Ask the bus for the machine's clock, from a call rather than from the reply envelope, so the number
/// comes from a handler that read the real `System`.
fn mclk(c: &mut Client) -> u64 {
    c.ok("emulator/status", json!({}))["mclk"]
        .as_u64()
        .expect("status carries mclk")
}

fn hits(c: &mut Client, handle: &str) -> u64 {
    let l = c.ok("emulator/breakpoint_list", json!({}));
    let row = l["breakpoints"]
        .as_array()
        .expect("breakpoints[]")
        .iter()
        .find(|b| b["breakpoint"] == json!(handle))
        .unwrap_or_else(|| panic!("no row for {handle} in {l}"));
    row["hits"].as_u64().expect("hits is a number")
}

/// ## ★ THE PARCEL ★ — a breakpoint armed against a **playing** window halts it, exactly, once.
///
/// The whole documented consumer idiom, driven over a real socket against a player that owns the machine
/// and is free-running it: refuse a bounded run to prove the arrangement, arm, wait, and then check every
/// way this could look right while being wrong.
///
/// **The four things it is built to catch**, each with the mutation that produces it:
///
/// 1. **`Observe`-wrapping the sink** (hits counted, machine never halts) — caught by `pc`. An `Observe`
///    drops `stop_requested` only, so the sink still latches and the halt still lands; what changes is
///    that the frame ran to *completion* first, so the reported PC is wherever the frame ended and not
///    the breakpoint. `assert_eq!(pc, HOT_PC)` is the discriminator, not `timeoutReached`.
/// 2. **Clearing one run flag and not the other** — caught by the stop *count*. `free_run` is the mode
///    and `running` is "advancing right now"; clearing only `running` leaves the loop free-running and
///    re-breaks once per frame (374,011 measured, §5 of the breakpoints doc). One stop is asserted after
///    the player has been given twenty more iterations to produce a second.
/// 3. **A stop stamped from the placeholder `System`** — caught by `frame`/`mclk`, which must be the
///    machine's own and non-zero. Applying the halt where it is observed rather than at the top of the
///    next drain stamps `frame 0, mclk 0` (D11).
/// 4. **A halt that does not actually stop the window** — caught by the clock standing still across
///    twenty *wall-clock* iterations of the player's loop. A pause the player does not follow leaves the
///    emulated clock moving while the bus claims otherwise.
#[test]
fn a_breakpoint_halts_the_playing_window_exactly_once() {
    assert_hot_pc_is_the_stirring_loop();
    let p = Player::start("bp-halt");
    let mut c = Client::connect(&p);
    c.handshake(true);

    // The arrangement, stated as a fact rather than assumed: the player is free-running, so the bounded
    // runs that *always* carried a breakpoint are refused. This is the state in which the gap was total.
    let e = c.err("emulator/run_frames", json!({"frames": 1}));
    assert_eq!(
        e["data"]["reason"],
        json!("machineRunning"),
        "the player must really be free-running, or this test proves nothing: {e}"
    );

    let bp = c.ok("emulator/breakpoint_add", json!({"addr": hex_addr(HOT_PC)}))["breakpoint"]
        .as_str()
        .expect("a breakpoint handle")
        .to_string();

    // The documented idiom, unchanged: arm, then wait. Before this parcel this returned
    // `{"timeoutReached": true}` after the full timeout, with `hits: 0` corroborating it.
    let w = c.ok("emulator/wait_for_break", json!({"timeoutMs": 10000}));
    assert_eq!(
        w["timeoutReached"],
        json!(false),
        "the wait timed out against a playing window. Two causes produce this identical reply and \
         both are defects: the halt did not ride the player's loop at all, or it cleared `running` \
         without clearing `free_run` — `wait_for_break` reads the free-run MODE. {w}"
    );
    assert_eq!(
        w["pc"],
        json!(hex_addr(HOT_PC)),
        "the machine stopped somewhere other than the breakpoint: {w}"
    );

    // (4) The window really stopped. `expect_progress` proves the loop is still turning, so a frozen
    // clock is a paused player and not a dead thread.
    let halted_at = mclk(&mut c);
    assert!(halted_at > 0, "the fixture never ran");
    p.expect_progress(20, "the player must keep iterating while paused");
    assert_eq!(
        mclk(&mut c),
        halted_at,
        "the window kept emulating after a halt it was told about"
    );

    // (1)(2)(3) The event. One of them, naming this handle, at the breakpoint, stamped with the real
    // machine, on a machine that reports itself stopped.
    let stops = c.stops();
    assert_eq!(
        stops.len(),
        1,
        "one halt must announce itself once. More than one means the halt is being re-applied — a \
         latch that is peeked rather than taken, or a run driver that kept going and re-broke every \
         frame (374,011 measured, breakpoints doc §5): {stops:?}"
    );
    let s = stops[0];
    assert_eq!(s["reason"], json!("breakpoint"), "{s}");
    assert_eq!(s["breakpoint"], json!(bp), "{s}");

    // The D11 stamp is checked FIRST, so the two remaining mutations name themselves rather than both
    // landing on `pc`: a stop applied outside the drain window reads the placeholder's zeros here,
    // while an `Observe`-wrapped sink stamps this correctly and gets `pc` wrong below. (Measured: the
    // placeholder produces `frame 0, mclk 0, pc 0x00000000`; the `Observe` produces `pc 0x00000210`.)
    let stamped = s["mclk"].as_u64().unwrap_or_else(|| {
        panic!("the stop carried no numeric mclk, which is a D11 violation on its own: {s}")
    });
    assert_eq!(
        stamped, halted_at,
        "the stop was stamped from the placeholder System rather than the player's machine — the halt \
         was applied outside a pump drain window, where the engine holds the placeholder and every \
         clock reads 0: {s}"
    );
    assert!(
        s["frame"].as_u64().is_some_and(|f| f > 0),
        "and its frame with it: {s}"
    );
    assert_eq!(
        s["pc"],
        json!(hex_addr(HOT_PC)),
        "the stop is not AT the breakpoint. An `Observe`-wrapped sink produces exactly this: it drops \
         only `stop_requested`, so the hit is still counted and the halt still lands — after the frame \
         has run to completion. {s}"
    );
    assert_eq!(
        s["running"],
        json!(false),
        "the `running` flag must clear with `free_run`, not instead of it: {s}"
    );

    // …and the hit was counted exactly once, on the handle that stopped it.
    assert_eq!(hits(&mut c, &bp), 1, "one halt is one hit");
}

/// **A halted window resumes and makes progress** — the re-trigger suppression, which is the entire
/// reason `run_sinks` takes the caller's PC.
///
/// A machine halted *at* a breakpoint starts its next run on that same address. Without the suppression
/// the sink fires again before a single instruction retires, the run ends having advanced nothing, and
/// the window is unresumable: `resume` appears to work and the clock never moves. Poison this by passing
/// anything other than `sys.cpu_regs().pc` to `run_sinks` in `Player` and the clock below stops dead.
///
/// The second halt is not a defect — `HOT_PC` is a tight loop, so re-entering it *is* a real second hit,
/// and `hits: 2` is the honest count.
#[test]
fn a_halted_window_resumes_past_its_own_breakpoint() {
    assert_hot_pc_is_the_stirring_loop();
    let p = Player::start("bp-resume");
    let mut c = Client::connect(&p);
    c.handshake(true);
    let bp = c.ok("emulator/breakpoint_add", json!({"addr": hex_addr(HOT_PC)}))["breakpoint"]
        .as_str()
        .expect("a breakpoint handle")
        .to_string();

    let w = c.ok("emulator/wait_for_break", json!({"timeoutMs": 10000}));
    assert_eq!(w["timeoutReached"], json!(false), "first halt: {w}");
    let first = mclk(&mut c);
    assert_eq!(hits(&mut c, &bp), 1);

    c.ok("emulator/resume", json!({}));
    let w = c.ok("emulator/wait_for_break", json!({"timeoutMs": 10000}));
    assert_eq!(w["timeoutReached"], json!(false), "second halt: {w}");
    assert_eq!(
        w["pc"],
        json!(hex_addr(HOT_PC)),
        "and at the same address, because the loop comes back to it: {w}"
    );
    let second = mclk(&mut c);
    assert!(
        second > first,
        "the machine re-broke at its own resume PC without retiring an instruction — a window that can \
         never be resumed past a breakpoint ({first} -> {second})"
    );
    assert_eq!(hits(&mut c, &bp), 2, "and the second halt is a second hit");
}

/// The negative control: a breakpoint the ROM cannot reach never halts the window, and the machine keeps
/// running. Without this, everything above is also satisfied by a player that halts on any breakpoint at
/// all — or on none of them, if `timeoutReached` were the only thing asserted.
#[test]
fn a_breakpoint_the_rom_never_reaches_does_not_halt_the_window() {
    assert_hot_pc_is_the_stirring_loop();
    let p = Player::start("bp-cold");
    let mut c = Client::connect(&p);
    c.handshake(true);
    let bp = c.ok(
        "emulator/breakpoint_add",
        json!({"addr": hex_addr(COLD_PC)}),
    )["breakpoint"]
        .as_str()
        .expect("a breakpoint handle")
        .to_string();

    let before = mclk(&mut c);
    let w = c.ok("emulator/wait_for_break", json!({"timeoutMs": 250}));
    assert_eq!(
        w["timeoutReached"],
        json!(true),
        "a cold breakpoint must not stop the window: {w}"
    );
    assert!(
        w.get("pc").is_none(),
        "…and a PC sampled off a still-moving machine names an instruction that has gone: {w}"
    );
    p.expect_progress(5, "the player keeps running through a cold breakpoint");
    assert!(mclk(&mut c) > before, "the window must still be emulating");
    assert_eq!(hits(&mut c, &bp), 0, "and nothing was counted");
    assert!(
        c.stops().is_empty(),
        "and nothing was announced: {:?}",
        c.stops()
    );
}

// ------------------------------------------------------------------ screen text (§11.29, CR-H)

/// The snapshot a player would push, with every honest-reading case in one screen.
///
/// **Not invented shapes.** Each row is a string the real frontend actually composes, so what this drives
/// through the handler is the traffic the method exists for:
///
/// * the **title bar** — `main.rs`'s `format!("Oracle — frame {frame} [PAUSED]")`, drawn by the window
///   manager and therefore invisible to any OCR of the presented framebuffer;
/// * a **status line** that was cut by the player's own `fit`, which is the defect class
///   (`F-TOAST-TRUNCATES`) that `rendered` exists to make visible;
/// * the player's **very first toast**, ``PRESS ` FOR COMMANDS`` (`main.rs`), whose backtick this font has
///   no glyph for — so the window shows a hollow box where a character should be, and `unrenderable` is
///   the only field on the wire that can say so.
fn a_screen() -> Vec<ScreenSurface> {
    vec![
        ScreenSurface {
            kind: ScreenSurfaceKind::TitleBar,
            text: "Oracle — frame 12720 [PAUSED]".into(),
            rendered: "Oracle — frame 12720 [PAUSED]".into(),
            unrenderable: vec![],
        },
        ScreenSurface {
            kind: ScreenSurfaceKind::StatusLine,
            text: "AETHER ON 4:3 320X224 F12720".into(),
            rendered: "AETHER ON 4:3 320X2".into(),
            unrenderable: vec![],
        },
        ScreenSurface {
            kind: ScreenSurfaceKind::Toast,
            text: "PRESS ` FOR COMMANDS".into(),
            rendered: "PRESS ` FOR COMMANDS".into(),
            unrenderable: vec!["`".into()],
        },
    ]
}

/// **A player that has published its screen serves it, whole.**
///
/// Asserted as the **entire** reply object rather than field by field. The status line is a fixed-width
/// surface that truncates silently from the right, and a test that checks only the field it cares about is
/// blind to whatever it displaced — this repo's 2026-08-29 bar. So the shape, the derived flags and the
/// stamp keys are all pinned in one comparison, and a key that appeared or vanished fails here.
#[test]
fn a_player_that_published_its_screen_serves_every_surface_with_both_strings() {
    let p = Player::start_with("screentext", Some(a_screen()));
    let mut c = Client::connect(&p);
    c.handshake(false);
    // One full iteration, so the push has certainly landed before the read.
    p.expect_progress(
        2,
        "the player must present at least once before its text exists",
    );

    let r = c.ok("emulator/screen_text", json!({}));
    // Printed so the vectors handed to the hub can be shown to come from here — a real handler reached
    // through the real seam over a real socket — rather than from a cover note claiming they do.
    println!("REAL REPLY emulator/screen_text = {r}");

    // The stamp rides on every reply (§2.2) and is not this method's business; drop it so the comparison
    // below is about the method's own shape, and assert its presence separately rather than silently.
    for k in ["frame", "mclk", "running", "droppedEvents"] {
        assert!(
            r.get(k).is_some(),
            "the reply lost its stamp key `{k}`: {r}"
        );
    }
    let mut body = r.clone();
    let obj = body.as_object_mut().unwrap();
    for k in ["frame", "mclk", "running", "droppedEvents"] {
        obj.remove(k);
    }

    assert_eq!(
        body,
        json!({
            "surfaces": [
                {
                    "kind": "titleBar",
                    "text": "Oracle — frame 12720 [PAUSED]",
                    "rendered": "Oracle — frame 12720 [PAUSED]",
                    "truncated": false,
                    "unrenderable": [],
                },
                {
                    "kind": "statusLine",
                    "text": "AETHER ON 4:3 320X224 F12720",
                    "rendered": "AETHER ON 4:3 320X2",
                    "truncated": true,
                    "unrenderable": [],
                },
                {
                    "kind": "toast",
                    "text": "PRESS ` FOR COMMANDS",
                    "rendered": "PRESS ` FOR COMMANDS",
                    "truncated": false,
                    "unrenderable": ["`"],
                },
            ],
            "total": 3,
            "returned": 3,
            "truncated": false,
        }),
        "the whole reply, not just the fields this test added"
    );

    // The rider must agree with what the method just did. Either alone can be right while the pair is
    // wrong, and the pair being wrong is what a caller would trust first.
    let st = c.ok("emulator/status", json!({}));
    assert_eq!(
        st["display"],
        json!(true),
        "a hosted player has a display: {st}"
    );
}

/// **`truncated` is DERIVED at the wire, never carried.** A producer cannot publish a flag that disagrees
/// with the two strings beside it, because there is no flag to publish.
///
/// The poison this rules out is specific: a snapshot whose `rendered` equals its `text` but which *claims*
/// truncation (or the reverse) would let a caller trust a convenience field over the honest comparison the
/// fragment says is the real guard. Here the producer has no way to express that at all — which is the
/// point, and this test is what makes the absence observable.
#[test]
fn the_truncated_flag_is_derived_from_the_two_strings_and_not_from_the_producer() {
    let p = Player::start_with(
        "screentext-derived",
        Some(vec![
            // Equal strings — must be `false`.
            ScreenSurface {
                kind: ScreenSurfaceKind::Toast,
                text: "WHOLE".into(),
                rendered: "WHOLE".into(),
                unrenderable: vec![],
            },
            // A prefix — must be `true`.
            ScreenSurface {
                kind: ScreenSurfaceKind::Toast,
                text: "WHOLE".into(),
                rendered: "WHO".into(),
                unrenderable: vec![],
            },
            // Empty rendered against non-empty text: the picture was too narrow for even one glyph. Still
            // truncation, and the loudest case of it — the message is on no part of the glass.
            ScreenSurface {
                kind: ScreenSurfaceKind::Toast,
                text: "WHOLE".into(),
                rendered: String::new(),
                unrenderable: vec![],
            },
        ]),
    );
    let mut c = Client::connect(&p);
    c.handshake(false);
    p.expect_progress(
        2,
        "the player must present at least once before its text exists",
    );

    let r = c.ok("emulator/screen_text", json!({}));
    let s = r["surfaces"].as_array().expect("surfaces is an array");
    assert_eq!(s.len(), 3, "all three rows must survive: {r}");
    assert_eq!(s[0]["truncated"], json!(false), "equal strings: {}", s[0]);
    assert_eq!(s[1]["truncated"], json!(true), "a prefix: {}", s[1]);
    assert_eq!(
        s[2]["truncated"],
        json!(true),
        "nothing survived, which is truncation at its loudest: {}",
        s[2]
    );
}

/// **A window showing nothing is not the same artifact as no window** — the whole reason the refusal exists.
///
/// The pair is the assertion. A player that has published an *empty* screen succeeds with `surfaces: []`;
/// a player that has published nothing at all refuses. If either half changed to match the other, the two
/// states would become indistinguishable on the wire, which is the defect §11.29 names.
#[test]
fn a_blank_screen_succeeds_where_no_screen_refuses() {
    let blank = Player::start_with("screentext-blank", Some(vec![]));
    let mut c = Client::connect(&blank);
    c.handshake(false);
    blank.expect_progress(2, "the player must present at least once");
    let r = c.ok("emulator/screen_text", json!({}));
    println!("REAL REPLY emulator/screen_text (blank screen) = {r}");
    assert_eq!(
        r["surfaces"],
        json!([]),
        "F3 off, no toasts, nothing on: the default launch, and it is a SUCCESS: {r}"
    );
    assert_eq!(r["total"], json!(0), "{r}");
    assert_eq!(r["returned"], json!(0), "{r}");
    assert_eq!(
        r["truncated"],
        json!(false),
        "§2.4 clause (a): present even when false, so absent and false are not the same artifact: {r}"
    );
    assert_eq!(
        c.ok("emulator/status", json!({}))["display"],
        json!(true),
        "the window exists; it is merely blank"
    );

    let none = Player::start("screentext-none");
    let mut c2 = Client::connect(&none);
    c2.handshake(false);
    none.expect_progress(2, "the player must turn over");
    let e = c2.err("emulator/screen_text", json!({}));
    assert_eq!(e["code"], json!(-32005), "{e}");
    assert_eq!(e["data"]["reason"], json!("noDisplay"), "{e}");
    assert_eq!(
        c2.ok("emulator/status", json!({}))["display"],
        json!(false),
        "a player that has never presented has nothing to report yet, and says so"
    );
}
