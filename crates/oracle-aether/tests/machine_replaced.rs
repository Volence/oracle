//! **`emulator/machineReplaced`** — §11.40 (CR-Q, 2026-09-05), and §8 item 28's third extension.
//!
//! The defect this closes, in one sentence: a save-state load at a WINDOW's own keys replaced the
//! emulated machine without moving the engine's `rom_generation`, so a client attached over the socket
//! was never told the machine underneath it had changed. It saw the clock jump in the next stamp and
//! nothing else — and the recorded watchpoint hits, the latched picture and the profiler's shadow stack
//! all survived a boundary they cannot describe.
//!
//! # Why this file is HOSTED and every other event row is not
//!
//! The gesture is a window's. `common::spawn` builds the standalone arrangement — the bus owns the
//! machine on a thread of its own — and there is nobody at that process to press F4. §11.40 M2 makes
//! that a normative distinction rather than an implementation detail: *"A headless server that has no
//! such gesture MUST NOT advertise it; the capability list describes what this process can emit, not
//! what the contract knows."* So the subject here is a miniature player loop that owns its `System`,
//! swaps it wholesale on command, and tells the bus.
//!
//! It uses [`common::Client`] rather than `tests/hosted.rs`'s hand-rolled one, because
//! [`Client::recv`](common::Client) validates **every** line against the vendored schema and that is
//! where most of the force in this file comes from: `events["emulator/machineReplaced"].params`
//! `require`s `reason` and `hitsDropped`, and types `reason` as a closed enum with one member. An event
//! missing the count, or carrying `"reset"`, is refused on the wire by the fragment before any assertion
//! in this file runs.
//!
//! # The rows
//!
//! 1. [`hits_recorded_before_a_window_state_load_are_absent_after_it_and_the_event_counts_them`] —
//!    item 28's extension, first clause.
//! 2. [`hits_dropped_is_zero_and_PRESENT_on_a_window_state_load_when_nothing_was_recorded`] — the
//!    control that stops a server reporting a made-up number, and the one that catches `max(1)`.
//! 3. [`the_latched_picture_is_invalidated_so_a_paused_subscriber_does_not_get_the_pre_load_raster`] —
//!    item 28's extension names this one by name.
//! 4. [`a_refused_load_moves_nothing_and_emits_nothing`] — the negative. A signal that fires on a refused
//!    load reports a replacement that provably did not happen.
//! 5. [`a_window_state_load_fires_machine_replaced_and_never_rom_reloaded`] and
//!    [`a_reload_rom_fires_rom_reloaded_and_never_machine_replaced`] — M4, both directions, because "one
//!    boundary, one signal" is two claims and a suite that asserts one of them has asserted half of it.
//! 6. [`a_deployment_with_the_window_gesture_advertises_the_event`] — M2's positive. The negative (a
//!    headless server must not) is in `tests/handshake.rs`, against the same constant.
//! 7. [`the_aggregates_survive_the_boundary_even_though_the_records_do_not`] — item 28's standing clause,
//!    at the new boundary.
//!
//! # Mutations proven red
//!
//! Every one of these was applied on disk and the named runner was watched go red, then the file was
//! restored from the committed baseline.
//!
//! | # | mutation | rows that go red |
//! |---|---|---|
//! | M1 | `note_machine_replaced`: drop the `self.emit(...)` line entirely | 1, 2, 3(partly), 5a, 7 — every row that waits for the event |
//! | M2 | `note_machine_replaced`: emit `hitsDropped` only when `> 0` | **2 only**, and it fails on the **vendored schema's** `required`, which is the fragment proving itself load-bearing rather than an assertion in this file |
//! | M3 | `note_machine_replaced`: `hits_dropped.max(1)` — schema-LEGAL, so the fragment is blind to it | **2 only**, on the row's own `== 0` |
//! | M4 | `note_machine_replaced`: drop `self.invalidate_screen()` | **3 only** |
//! | M5 | `note_machine_replaced`: drop `self.rom_generation += 1` | 5a's `rom_changed` half; the wire rows stay green, which is why 5a asserts the report and not only the wire |
//! | M6 | `EngineConfig::window_gestures` default `true` | `handshake.rs`'s events comparison — the M2 negative, one door over |
//! | M7 | `States::load`: delete the `bus.machine_replaced(...)` call | not caught here; caught by `oracle-player`'s `a_load_tells_the_bus_the_machine_was_replaced_and_a_refusal_does_not` |
//! | M8 | `States::load`: move that call **above** the `let loaded = match …`, so a refusal signals too | same row, its negative half |
//!
//! ⚑ **One mutation is caught NOWHERE, and it is named rather than left implicit.** Moving
//! `bus.machine_replaced` above `machine.adopt_system(loaded)` — still inside the success path, just
//! before the swap instead of after — leaves every row in both suites green. The picture is invalidated
//! and the hits are drained either way; what changes is the **stamp** the event carries, which would
//! describe the timeline the load just left. Catching it needs a subscriber reading `frame`/`mclk` off
//! the event against a machine whose clock demonstrably moved across the load, and the player's fixture
//! has no socket. The ordering is defended by the comment at that line and by nothing else.

#![cfg(unix)]

mod common;

use common::Client;
use oracle_aether::engine::MachineReplacedReason;
use oracle_aether::host::{Host, HostConfig, MachineInfo};
use oracle_core::system::System;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

static SEQ: AtomicU32 = AtomicU32::new(0);

/// The address `testrom::build`'s VInt handler writes `$1234` to, once per frame — `watch_hits_epoch.rs`'s
/// sentinel, and the same one for the same reason: a single writer, so a hit count is a fact rather than
/// a sample.
const SENTINEL: &str = "0x00FF8000";

/// What the miniature window is told to do. Both arms exist because §11.40 M4's negative is as normative
/// as its positive.
enum Gesture {
    /// The success path: a whole-value machine swap, then the signal. This is
    /// `oracle-player::states::States::load`'s shape exactly — `save_state::load` is a static constructor
    /// returning a complete `System` or an `Err`, so the running machine is replaced in one move.
    LoadState,
    /// The refusal path: an empty slot, a fingerprint mismatch, a corrupt payload. **Nothing happens at
    /// all** — not a swap, not a signal — which is what the loader does when its constructor returns
    /// `Err`, and what row 4 asserts from the wire.
    RefusedLoad,
}

/// A miniature of a window's run loop: it owns the machine, drains the bus once per iteration, and can be
/// told to replace the machine the way a person at the keys would.
///
/// It is deliberately a **paused** loop. Every row here is about a discrete boundary and its
/// accounting, so a machine advancing underneath would put `stopped` events and moving clocks between
/// the assertion and the thing it asserts. Frames are run when a row wants them, through
/// `emulator/run_frames`.
struct Window {
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    gestures: mpsc::Sender<Gesture>,
    /// Acknowledged **after** the gesture has completed inside the loop, so a row never races the thread
    /// it is driving.
    acks: mpsc::Receiver<()>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// The cartridge on disk, so `emulator/reload_rom` has a real file to load (row 5b).
    rom_path: PathBuf,
}

impl Window {
    fn start(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let socket = std::env::temp_dir().join(format!("mr-{tag}-{}-{n}.sock", std::process::id()));
        let rom_path =
            std::env::temp_dir().join(format!("mr-{tag}-{}-{n}.bin", std::process::id()));
        std::fs::write(&rom_path, oracle_core::testrom::build()).expect("write the cartridge");

        let stop = Arc::new(AtomicBool::new(false));
        let (gestures, gesture_rx) = mpsc::channel::<Gesture>();
        let (ack_tx, acks) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();

        let (t_socket, t_stop, t_rom) = (socket.clone(), Arc::clone(&stop), rom_path.clone());
        let thread = std::thread::Builder::new()
            .name("test-window".into())
            .spawn(move || {
                let mut sys = booted();

                let mut config = HostConfig::default();
                // §11.40 M2, the whole point of this fixture: this process HAS the gesture, so it
                // advertises the event and may emit it. `HostConfig::default()` leaves this false,
                // which is what `tests/hosted.rs`'s player is and what makes that player's handshake
                // indistinguishable from a headless one on this key.
                config.engine.window_gestures = true;
                let mut host = Host::new(config);
                host.set_machine_info(MachineInfo {
                    rom_path: Some(t_rom.display().to_string()),
                    ..MachineInfo::default()
                });
                host.serve(Some(t_socket))
                    .expect("bind the window's socket");
                ready_tx.send(()).ok();

                while !t_stop.load(Ordering::SeqCst) {
                    // The gesture is applied where a window applies it: on the loop's own thread,
                    // between drains, with the machine in hand.
                    match gesture_rx.try_recv() {
                        Ok(Gesture::LoadState) => {
                            sys = booted();
                            host.machine_replaced(&mut sys, MachineReplacedReason::StateLoad);
                            ack_tx.send(()).ok();
                        }
                        Ok(Gesture::RefusedLoad) => {
                            ack_tx.send(()).ok();
                        }
                        Err(_) => {}
                    }
                    host.set_paused(true);
                    host.pump(&mut sys);
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
            .expect("spawn the test window");
        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the window bound its socket");

        Window {
            socket,
            stop,
            gestures,
            acks,
            thread: Some(thread),
            rom_path,
        }
    }

    /// Perform a gesture and wait for the loop to finish it.
    fn gesture(&self, g: Gesture) {
        self.gestures.send(g).expect("the window loop is alive");
        self.acks
            .recv_timeout(Duration::from_secs(10))
            .expect("the window loop acknowledged the gesture");
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let _ = std::fs::remove_file(&self.rom_path);
    }
}

/// A machine that really **takes** its vertical interrupt, so the sentinel is written once a frame.
///
/// The IE0 poke is `tests/watchpoints.rs`'s and `watch_hits_epoch.rs`'s: the fixture ROMs lower the CPU
/// mask but never touch a VDP register, so without it the VInt latch is set every frame and never gated
/// into the IPL — the handler never runs, nothing writes the sentinel, and every "hits were dropped" row
/// below would pass by recording none.
fn booted() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys.vdp_mut().control_write(0x8120, 0); // reg 1 = $20 → IE0 (VINT enable)
    sys
}

/// A subscribed client on the window's socket.
fn attach(w: &Window) -> Client {
    let mut c = Client::connect_path(&w.socket);
    c.handshake(true);
    c
}

/// **Every line the server sends up to and including the reply to one marker call**, with the
/// notifications split out.
///
/// This is how both the positive and the negative rows read the event stream, and the reason it is sound
/// is a property of the transport rather than of timing: a connection has ONE writer thread draining ONE
/// `Outbound` queue (`server.rs`, `while let Some(line) = writer_out.pop()`), so replies and events share
/// a single FIFO. An event pushed by a gesture that has already completed is therefore **ahead of** the
/// reply to a request sent afterwards. A row can consequently assert an absence without a sleep and
/// without a timeout: if the event were going to come, it would already be in this vector.
///
/// The marker is `emulator/status` — the cheapest call that does not move the machine, so reading the
/// stream never becomes the thing that changes it.
fn stream_to_marker(c: &mut Client) -> Vec<Value> {
    let id = c.next_request_id();
    c.send_raw(
        &json!({"jsonrpc":"2.0","id":id,"method":"emulator/status","params":{}}).to_string(),
    );
    let mut events = Vec::new();
    loop {
        // `recv` validates every line against the vendored schema on the way past — notifications
        // included — which is where this file's fragment conformance comes from.
        let v = c.recv();
        if v.get("id").is_some_and(|i| !i.is_null()) {
            assert_eq!(v["id"], json!(id), "response id must correlate");
            return events;
        }
        events.push(v);
    }
}

/// The `params` of the one `emulator/machineReplaced` in a stream — asserting there is **exactly one**.
///
/// The count is load-bearing and not pedantry: a signal wired into a per-frame path instead of a per-
/// gesture one would emit correct-looking events forever, and "at least one arrived" cannot tell that
/// apart from a correct single push. It is the same argument `hosted.rs` records for counting `stopped`.
fn the_one_replacement(events: &[Value]) -> Value {
    let found: Vec<&Value> = events
        .iter()
        .filter(|v| v["method"] == json!("emulator/machineReplaced"))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one emulator/machineReplaced in the stream, got {}: {:?}",
        found.len(),
        events
    );
    found[0]["params"].clone()
}

/// `hitsDropped` off an event's params, having first asserted the key is **there**. `v["k"]` on a missing
/// key is `Null`, which fails every comparison below — but it fails naming the wrong thing.
fn hits_dropped(params: &Value) -> u64 {
    let v = params
        .as_object()
        .expect("event params are an object")
        .get("hitsDropped")
        .expect("`hitsDropped` is REQUIRED on emulator/machineReplaced (§11.40 M1)");
    v.as_u64()
        .unwrap_or_else(|| panic!("`hitsDropped` must be a non-negative integer, got {v}"))
}

/// Arm the sentinel watch, run until it has fired, and return the number of hits held — the count the
/// boundary is then obliged to report back.
fn record_some_hits(c: &mut Client) -> u64 {
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "len": 2, "label": "sentinel"}),
    );
    c.ok("emulator/run_frames", json!({"frames": 3}));
    let held = c.ok("emulator/watchpoint_hits", json!({}))["total"]
        .as_u64()
        .expect("`total` is required");
    assert!(
        held > 0,
        "the fixture must actually record hits, or every row below passes vacuously"
    );
    held
}

// ---------------------------------------------------------------------------------------------------
// Row 1 — item 28's extension, first clause
// ---------------------------------------------------------------------------------------------------

/// **Hits recorded before a window state load are absent afterwards, and the event's `hitsDropped`
/// counts them** (§8 item 28 as extended by §11.40).
///
/// The count is the half that matters. A server that emptied the ring and reported `0` would satisfy
/// "the hits are gone" and would be lying about what the client lost — which is the exact confusion
/// §11.38 was raised for, one boundary over.
///
/// ⚑ **A known gap, stated rather than left for the next reader.** This asserts the ENGINE half. It
/// cannot see mutation M7 — a window that signals *before* it swaps — because this fixture's loop does
/// the swap and the signal in two adjacent statements this file owns, so a reordering here would be a
/// change to the test, not to the subject. The ordering that matters is `oracle-player`'s
/// `States::load`, and its own suite is where that lives.
#[test]
fn hits_recorded_before_a_window_state_load_are_absent_after_it_and_the_event_counts_them() {
    let w = Window::start("hits");
    let mut c = attach(&w);

    let held = record_some_hits(&mut c);

    w.gesture(Gesture::LoadState);
    let params = the_one_replacement(&stream_to_marker(&mut c));

    assert_eq!(
        params["reason"],
        json!("stateLoad"),
        "§11.40 M1: the only adopted reason"
    );
    assert_eq!(
        hits_dropped(&params),
        held,
        "the event must report the hits it actually dropped, not a different number"
    );

    let after = c.ok("emulator/watchpoint_hits", json!({}));
    assert_eq!(
        after["total"],
        json!(0),
        "hits recorded before the load must not survive it"
    );
    assert_eq!(
        after["hits"].as_array().map(|a| a.len()),
        Some(0),
        "…and the ring itself must be empty, not merely counted as empty"
    );
}

// ---------------------------------------------------------------------------------------------------
// Row 2 — the control
// ---------------------------------------------------------------------------------------------------

/// **`hitsDropped` is `0`, and PRESENT, when nothing was recorded.**
///
/// Without this row a server could emit the key only when it had something to report, and absence and
/// zero would both mean "nothing was lost" — two states a client cannot tell apart, which is the shape
/// §11.38 and §11.39 both refused. It is also the only row that catches a `max(1)`, which the schema
/// cannot: `1` is a perfectly legal integer for that fragment.
#[test]
#[allow(non_snake_case)]
fn hits_dropped_is_zero_and_PRESENT_on_a_window_state_load_when_nothing_was_recorded() {
    let w = Window::start("zero");
    let mut c = attach(&w);

    w.gesture(Gesture::LoadState);
    let params = the_one_replacement(&stream_to_marker(&mut c));

    assert!(
        params
            .as_object()
            .expect("event params are an object")
            .contains_key("hitsDropped"),
        "`hitsDropped` is REQUIRED and present at zero (§11.40 M1)"
    );
    assert_eq!(params["hitsDropped"], json!(0));
    assert_eq!(hits_dropped(&params), 0);
}

// ---------------------------------------------------------------------------------------------------
// Row 3 — the latched picture
// ---------------------------------------------------------------------------------------------------

/// **The latched picture is invalidated, so a paused subscriber's next `emulator/screenshot` does not
/// hand back the pre-load raster.** Item 28's extension names this consequence explicitly.
///
/// The observable is `source`, which the row reads on both sides of the boundary rather than only after:
/// asserting `"stateRender"` afterwards witnesses nothing unless the machine was demonstrably serving a
/// real latched raster before, and on a paused machine that has never drawn a frame it would be
/// `"stateRender"` anyway. So the row runs frames first and asserts `"raster"` — its own anti-vacuity
/// clause — and only then performs the load.
#[test]
fn the_latched_picture_is_invalidated_so_a_paused_subscriber_does_not_get_the_pre_load_raster() {
    let w = Window::start("picture");
    let mut c = attach(&w);

    // `emulator/screenshot` writes a PNG, so the row names its own file and removes it. The default is
    // `temp_dir()/oracle-frame-<frame>.png`, which two rows running at the same frame would share.
    let shot = std::env::temp_dir().join(format!("mr-shot-{}.png", std::process::id()));
    let shot = json!({ "path": shot.display().to_string() });

    c.ok("emulator/run_frames", json!({"frames": 2}));
    let before = c.ok("emulator/screenshot", shot.clone());
    assert_eq!(
        before["source"],
        json!("raster"),
        "anti-vacuity: the machine must be serving a real latched frame before the load, or the \
         assertion after it is about nothing"
    );

    w.gesture(Gesture::LoadState);
    let _ = stream_to_marker(&mut c);

    let after = c.ok("emulator/screenshot", shot.clone());
    assert_eq!(
        after["source"],
        json!("stateRender"),
        "the pre-load raster survived the boundary and was handed back as a live frame"
    );
    let _ = std::fs::remove_file(after["path"].as_str().expect("the reply names the file"));
}

// ---------------------------------------------------------------------------------------------------
// Row 4 — the negative
// ---------------------------------------------------------------------------------------------------

/// **A refused load moves nothing and emits nothing** (§11.40 M4, §8 item 28's extension).
///
/// A signal that fires on a refused load reports a replacement that provably did not happen, which is
/// worse than no signal: a listener would drop its own derived state and re-derive it against a machine
/// that never changed. The absence is asserted through [`stream_to_marker`]'s ordering property rather
/// than through a sleep, so this row cannot pass by being fast.
///
/// The machine is asserted unmoved too, because "no event" and "no replacement" are two claims and only
/// the second one is what the user of a refused load cares about.
#[test]
fn a_refused_load_moves_nothing_and_emits_nothing() {
    let w = Window::start("refused");
    let mut c = attach(&w);

    let held = record_some_hits(&mut c);
    let hash_before = c.ok("emulator/state_hash", json!({}))["combined"].clone();
    assert!(
        hash_before.is_string(),
        "the control read a real fingerprint"
    );

    w.gesture(Gesture::RefusedLoad);
    let events = stream_to_marker(&mut c);

    assert!(
        !events
            .iter()
            .any(|v| v["method"] == json!("emulator/machineReplaced")),
        "a refused load emitted a replacement: {events:?}"
    );
    assert_eq!(
        c.ok("emulator/watchpoint_hits", json!({}))["total"],
        json!(held),
        "a refused load dropped recorded hits it never had the right to touch"
    );
    assert_eq!(
        c.ok("emulator/state_hash", json!({}))["combined"],
        hash_before,
        "a refused load moved the machine"
    );
}

// ---------------------------------------------------------------------------------------------------
// Row 5 — M4, both directions
// ---------------------------------------------------------------------------------------------------

/// **A window state load fires `machineReplaced` and never `romReloaded`** — §11.40 M4's first half.
///
/// The wrong answer here is a believable one, and the ruling says why it was refused: `romReloaded`'s
/// `path` is required and §11.26 M3 makes that event a symbol re-resolve trigger, so firing it for a load
/// that moved no cartridge is *"both a lie and an over-signal"*. A listener would re-resolve a listing
/// against an image that never changed.
#[test]
fn a_window_state_load_fires_machine_replaced_and_never_rom_reloaded() {
    let w = Window::start("one-signal-a");
    let mut c = attach(&w);

    w.gesture(Gesture::LoadState);
    let events = stream_to_marker(&mut c);

    let _ = the_one_replacement(&events);
    assert!(
        !events
            .iter()
            .any(|v| v["method"] == json!("emulator/romReloaded")),
        "a window state load emitted romReloaded, which claims a cartridge moved: {events:?}"
    );
}

/// **A `reload_rom` fires `romReloaded` and never `machineReplaced`** — §11.40 M4's second half, and the
/// direction that would double-count.
///
/// Both events carry `hitsDropped`, and both drain the same ring. A boundary that emitted both would
/// report the same loss twice to one listener — the second one necessarily as `0`, since the first drain
/// emptied the ring, which is a *wrong* number rather than a duplicate one.
#[test]
fn a_reload_rom_fires_rom_reloaded_and_never_machine_replaced() {
    let w = Window::start("one-signal-b");
    let mut c = attach(&w);

    let held = record_some_hits(&mut c);

    let id = c.next_request_id();
    c.send_raw(
        &json!({"jsonrpc":"2.0","id":id,"method":"emulator/reload_rom",
                "params":{"path": w.rom_path.display().to_string()}})
        .to_string(),
    );
    let mut events = Vec::new();
    let reply = loop {
        let v = c.recv();
        if v.get("id").is_some_and(|i| !i.is_null()) {
            assert_eq!(v["id"], json!(id));
            break v;
        }
        events.push(v);
    };

    let reloaded: Vec<&Value> = events
        .iter()
        .filter(|v| v["method"] == json!("emulator/romReloaded"))
        .collect();
    assert_eq!(
        reloaded.len(),
        1,
        "exactly one romReloaded is the existing §11.39 contract: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|v| v["method"] == json!("emulator/machineReplaced")),
        "a reload_rom also emitted machineReplaced, so the drop is signalled twice: {events:?}"
    );
    assert_eq!(
        reloaded[0]["params"]["hitsDropped"],
        json!(held),
        "and the one signal carries the whole count (§11.39)"
    );
    assert_eq!(
        reply["result"]["hitsDropped"],
        json!(held),
        "…the same number the reply carried"
    );
}

// ---------------------------------------------------------------------------------------------------
// Row 6 — M2's positive
// ---------------------------------------------------------------------------------------------------

/// **A deployment that has the gesture advertises the event** — §11.40 M2's positive half.
///
/// The negative is `tests/handshake.rs`, where a headless `common::spawn` server's `capabilities.events`
/// is compared against the base [`EVENTS`](oracle_aether::engine::EVENTS) constant. Two processes, one
/// binary, two answers: that is exactly the hazard `Engine::advertised_events` writes up as
/// `F-BANNER-INVITES-A-PIN` on a new surface, and it is asserted here rather than merely described.
///
/// The membership test is deliberately membership and not equality: a consumer that pins the array is
/// the defect being warned about, and a suite that pinned it here would be modelling the wrong client.
#[test]
fn a_deployment_with_the_window_gesture_advertises_the_event() {
    let w = Window::start("advertise");
    let mut c = Client::connect_path(&w.socket);
    let r = c.handshake(true);

    let events: Vec<String> = r["capabilities"]["events"]
        .as_array()
        .expect("capabilities.events is an array")
        .iter()
        .map(|v| v.as_str().expect("an event name").to_string())
        .collect();

    assert!(
        events.iter().any(|e| e == "emulator/machineReplaced"),
        "§11.40 M2: this process HAS the gesture and must advertise the event; it advertised {events:?}"
    );
    // The base set is still whole — the member is ADDED, and advertising it must not have cost anything.
    for base in oracle_aether::engine::EVENTS {
        assert!(
            events.iter().any(|e| e == base),
            "{base} left the advertised set: {events:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// Row 7 — the standing clause
// ---------------------------------------------------------------------------------------------------

/// **The aggregates survive the boundary even though the records do not** — §8 item 28's standing
/// sentence, at the third boundary.
///
/// §11.39 ratified the distinction for all four artifacts at once: a RECORD with epoch-relative fields
/// (`frame`, the cycle stamp, `pc`) is dropped because it is uninterpretable after the boundary; an
/// AGGREGATE over an observer's life is kept because it describes the *recorder*, whose life the boundary
/// did not end. A server that cleared `seen` here would be answering "the ring lost some at record time"
/// with a number that a cartridge swap had edited.
#[test]
fn the_aggregates_survive_the_boundary_even_though_the_records_do_not() {
    let w = Window::start("aggregates");
    let mut c = attach(&w);

    record_some_hits(&mut c);

    // Two aggregates at two scales, because they live in two places and only one of them is in the
    // reply the boundary edits. `seen`/`matched` on `watchpoint_hits` are the RECORDER's lifetime
    // counters; `matched` on a `watchpoint_list` entry is one WATCH's. A server that cleared either
    // would be answering "how much did the instrument observe" with a number a machine swap had edited.
    let hits_before = c.ok("emulator/watchpoint_hits", json!({}));
    let list_before = c.ok("emulator/watchpoint_list", json!({}));
    let seen_before = hits_before["seen"]
        .as_u64()
        .expect("`seen` is the recorder's lifetime counter");
    let matched_before = list_before["watches"][0]["matched"]
        .as_u64()
        .expect("a watch reports `matched`");
    assert!(
        seen_before > 0 && matched_before > 0,
        "anti-vacuity: both aggregates must be non-zero before the boundary, got \
         seen={seen_before} matched={matched_before}"
    );

    w.gesture(Gesture::LoadState);
    let _ = the_one_replacement(&stream_to_marker(&mut c));

    let hits_after = c.ok("emulator/watchpoint_hits", json!({}));
    let list_after = c.ok("emulator/watchpoint_list", json!({}));
    assert_eq!(
        list_after["watches"]
            .as_array()
            .map(|a| a.len())
            .expect("watches is an array"),
        1,
        "the watch itself is an observer and survives the boundary"
    );
    assert_eq!(
        hits_after["seen"], hits_before["seen"],
        "`seen` describes the recorder, not the epoch, and must not be cleared (§11.39)"
    );
    assert_eq!(
        hits_after["matched"], hits_before["matched"],
        "…nor `matched`"
    );
    assert_eq!(
        list_after["watches"][0]["matched"], list_before["watches"][0]["matched"],
        "…nor the per-watch aggregate"
    );
    assert_eq!(
        hits_after["total"],
        json!(0),
        "…while the RECORDS, which are epoch-relative, are gone"
    );
}
