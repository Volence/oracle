//! A minimal Aether client, used by the integration tests. Hand-rolled on purpose: the tests must
//! exercise the *wire* (NDJSON framing, envelope shape, handshake ordering), so anything that shares
//! code with the server would test less than it appears to.

#![allow(dead_code)]

pub mod schema;

use oracle_aether::engine::EngineConfig;
use oracle_aether::server::{Machine, Server, ServerConfig, ServerHandle};
use oracle_core::system::System;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

static SEQ: AtomicU32 = AtomicU32::new(0);

/// Params for a **method sweep** — a test that calls every advertised name in turn to check some
/// property of the *reply envelope* rather than of any one handler.
///
/// Empty for all but one row. `emulator/wait_for_break` is the only method in the catalog whose default
/// behaviour is to **block**: §6 gives `timeoutMs` a default of `30000` (§11.24 D-07, *"the legacy
/// server's measured default, preserved because a retained deprecated method keeps its behaviour"*), and
/// on a machine that is running with no breakpoint due it waits the whole of it. A sweep that has already
/// called `emulator/resume` — which every sweep over the whole table does, because `resume` is in the
/// table — therefore stalls for 30 seconds on that one row and trips its own read timeout.
///
/// `{"timeoutMs": 0}` is the contract's own non-blocking spelling (§11.24: *"`0` polls once and
/// returns"*), so this weakens nothing: the call still dispatches, still runs the handler, and still
/// returns a stamped reply — which is the entire subject of every sweep that uses this. What it avoids is
/// making those sweeps depend on the order the table happens to be in.
pub fn sweep_params(method: &str) -> Value {
    match method {
        "emulator/wait_for_break" => json!({"timeoutMs": 0}),
        _ => json!({}),
    }
}

/// **`resume`, then the halt it runs into — read as one operation, because the two race.**
///
/// This lives here, and not beside its one current caller, because the race it closes is a property of
/// *this client* rather than of any one test: [`Client::ok`] reads through to a reply and **discards**
/// every event it passes, so any test that resumes a machine and then waits for the halt on the **same
/// connection** has the same bug waiting for it. One home, so the next such test inherits the fix instead
/// of rediscovering it.
///
/// A breakpoint on a tight fixture loop fires within microseconds of the resume, and the `stopped` event
/// is broadcast from the engine thread while the `resume` reply is written by the connection thread.
/// Either can reach the socket first — and unlike the bounded run-control methods (`run_frames`,
/// `run_to`, `run_to_scanline`, `step`), which call `emit_stopped` *inside* the handler and therefore
/// always put the event ahead of their reply, `emulator/resume` returns the instant it flips the free-run
/// flag and the halt is enqueued later by whichever thread gets there first. So the obvious spelling —
/// `ok("emulator/resume")` then a loop reading for the event — throws the halt away roughly half the time
/// and then blocks to the socket read timeout waiting for it. Measured at trial 4 of 8, after three clean
/// passes; a single-shot test would have called it green.
///
/// So both lines are read before either is acted on. This is a property of the *harness*, not of the
/// server: the wire carried both messages in a legitimate order.
///
/// `id` is written by the caller rather than drawn from [`Client`]'s own counter because the request is
/// hand-framed; use a value well clear of the sequential ids [`Client::call`] hands out.
pub fn resume_and_wait_for_stop(c: &mut Client, id: i64) -> Value {
    c.send_raw(
        &json!({"jsonrpc":"2.0","id":id,"method":"emulator/resume","params":{}}).to_string(),
    );
    let mut stopped = None;
    let mut replied = false;
    loop {
        let line = c.recv();
        if line["method"] == json!("emulator/stopped") {
            stopped = Some(line["params"].clone());
        }
        if line["id"] == json!(id) {
            assert!(
                line.get("error").is_none(),
                "resume failed: {}",
                line["error"]
            );
            replied = true;
        }
        if replied {
            if let Some(p) = stopped.take() {
                return p;
            }
        }
    }
}

/// **The frame budget a method sweep gives its server**, and the ceiling every sweep row is held to.
///
/// A sweep calls every advertised name to check something about the *reply envelope*. Not one of them
/// is asking a run-control method to run: the subject is that the name dispatches and comes back
/// stamped. So the honest budget is the smallest one that still lets a bounded run happen at all.
///
/// On a default server it is 3,600, and `run_step` clamps the three `step*` rows to 600 of those.
/// `emulator/step_out` on the test fixture has no frame to return out of, so it takes **every one of the
/// 600** — measured at 6.68 s of the sweep's 7.44 s total wall clock, inside a single socket read whose
/// deadline is [`READ_TIMEOUT`]. That is what `F-HANDSHAKE-LOAD-TIMEOUT` is: not a flake and not a
/// deadlock, but a wiring probe holding a 20-second read open while it runs ten seconds of emulation,
/// with only a 3x margin to spend on a busy machine. Reproduced 5 times out of 5 under 64 CPU spinners
/// on a 16-core box (load average 43-65); it passes 15/15 unloaded, which is exactly how a defect with a
/// narrow window looks.
///
/// This is the *same* defect [`sweep_params`] already fixed once, in the row above: `wait_for_break`
/// blocked 30 seconds on its own default and tripped the same read timeout. `step_out` is the sibling
/// that was missed, because it does not block on a timer — it blocks on a budget, and `params: &[]`
/// means no wire knob can shorten it. The engine's own config is the only seam, which is what
/// [`spawn_with_frame_budget`] is for.
pub const SWEEP_FRAME_BUDGET: u64 = 1;

/// **A server for a method sweep**: [`spawn`], with [`SWEEP_FRAME_BUDGET`] instead of the 3,600-frame
/// default.
///
/// Every sweep should use this rather than [`spawn`]. Nothing a sweep asserts can tell the two apart —
/// a name still dispatches, a handler still runs, a reply still comes back stamped — and the difference
/// is whether the probe costs milliseconds or costs the read deadline.
pub fn spawn_for_sweep(tag: &str) -> ServerHandle {
    spawn_with_frame_budget(tag, oracle_core::testrom::build(), 1024, SWEEP_FRAME_BUDGET)
}

/// The stamp on a reply, **wherever it rides**: in `result` when the call succeeded, in `error.data`
/// when it was refused. Both are replies, and `methods::every_reply_from_every_method_carries_frame_\
/// mclk_and_running` is the row that says so — a sweep reading `frame` must read it from either.
pub fn reply_stamp(reply: &Value) -> &Value {
    match reply.get("result") {
        Some(r) => r,
        None => &reply["error"]["data"],
    }
}

/// A unique socket path per test. `AF_UNIX` paths are capped near 108 bytes, so this stays short.
pub fn temp_socket(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("ae-{tag}-{}-{n}.sock", std::process::id()))
}

/// A server on a private socket with the pacing disabled (tests must not wait on wall-clock).
pub fn spawn(tag: &str) -> ServerHandle {
    spawn_with(tag, oracle_core::testrom::build(), 1024)
}

pub fn spawn_with(tag: &str, rom: Vec<u8>, queue_cap: usize) -> ServerHandle {
    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    spawn_system(tag, sys, queue_cap)
}

/// [`spawn_with`], with the engine's **frame budget** set by the caller.
///
/// The seam exists for exactly one property no other spawn can reach: the run-control methods are all
/// frame-bounded, and a test that wants to observe what a server does *when the bound is hit* has to be
/// able to make the bound cheap. `EngineConfig::max_run_frames` defaults to 3600 and `step` clamps its own
/// budget to 600 of them, so the honest way to see a bounded step is a fixture that would need more than
/// 600 frames — which is minutes of emulation per assertion. One frame is the same event, measured in
/// milliseconds, and the bound is the engine's own rather than a branch a test switched on.
///
/// Everything else matches [`spawn_with`], pacing included, so a server spawned here differs from one
/// spawned there in exactly the one number named.
pub fn spawn_with_frame_budget(
    tag: &str,
    rom: Vec<u8>,
    queue_cap: usize,
    max_run_frames: u64,
) -> ServerHandle {
    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    let config = ServerConfig {
        socket_path: temp_socket(tag),
        engine: EngineConfig {
            free_run_pace: None,
            max_run_frames,
            ..EngineConfig::default()
        },
        event_queue_cap: queue_cap,
    };
    Server::bind(config)
        .expect("bind aether socket")
        .spawn(Machine::new(sys))
}

/// A server around a `System` the test has already configured — the seam for a test that needs a
/// specific *machine* rather than a specific ROM (a VDP posed by hand, say). Everything else is
/// identical to [`spawn`], pacing included.
pub fn spawn_system(tag: &str, sys: System, queue_cap: usize) -> ServerHandle {
    let config = ServerConfig {
        socket_path: temp_socket(tag),
        engine: EngineConfig {
            free_run_pace: None,
            ..EngineConfig::default()
        },
        event_queue_cap: queue_cap,
    };
    Server::bind(config)
        .expect("bind aether socket")
        .spawn(Machine::new(sys))
}

/// **A server whose ROM really is on disk** — [`spawn`], plus the `romPath` a launched server always
/// has, pointing at a file whose bytes are the image the machine booted.
///
/// The seam exists because of §11.37 (CR-N): `emulator/status` now carries a ROM-freshness verdict, and
/// its unmeasurable state — *"this server holds no path for the image"* — is exactly what [`spawn`]
/// produces, since it loads `testrom::build()` from memory and never writes it anywhere. That is honest
/// and it is the right default for the hundred-odd tests that do not care. It is **not** right for a test
/// whose subject is some *other* caveat on the same key, which would otherwise be reading a composed
/// string with an unrelated sentence in front of it.
///
/// Returns the handle and the ROM file's path; the caller owns the file and may rewrite it to make the
/// image stale on purpose.
pub fn spawn_with_rom_file(tag: &str) -> (ServerHandle, PathBuf) {
    let rom = oracle_core::testrom::build();
    let path = std::env::temp_dir().join(format!(
        "ae-{tag}-{}-{}.bin",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&path, &rom).expect("write the ROM fixture to disk");
    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    let mut machine = Machine::new(sys);
    machine.rom_path = Some(path.display().to_string());
    let h = Server::bind(ServerConfig {
        socket_path: temp_socket(tag),
        engine: EngineConfig {
            free_run_pace: None,
            ..EngineConfig::default()
        },
        event_queue_cap: 1024,
    })
    .expect("bind aether socket")
    .spawn(machine);
    (h, path)
}

/// One NDJSON connection.
pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: i64,
    /// The method each outstanding request asked for, keyed by the request's `id` rendered as JSON.
    ///
    /// This is what lets [`Client::recv`] — the single funnel for every line the server sends — pick the
    /// right `methods.<name>.result` schema for a reply it is only handed *after* the fact. Populated by
    /// both [`Client::call`] and [`Client::send_raw`]: many tests write the request line by hand, and a
    /// reply the harness cannot attribute to a method gets envelope validation only.
    pending: HashMap<String, String>,
    /// **The request this connection is waiting on**, as `method (id N)`, or `None` when the last line
    /// read settled it. Set by [`Client::send_raw`], cleared by [`Client::recv`].
    ///
    /// Distinct from `pending`, which is a permanent `id -> method` ledger for schema selection and
    /// therefore grows to the whole sweep. This is the one thing a timeout has to be able to say, and it
    /// is why [`Client::read_line_or_explain`] can name a method instead of an errno.
    awaiting: Option<String>,
    /// The read deadline this connection was armed with, kept so a failure can quote the number it blew.
    read_timeout: Duration,
}

/// **The socket read deadline.** Named rather than spelled inline because a failure now quotes it, and a
/// deadline a test reports has to be a deadline the test can name.
///
/// Twenty seconds is not "slow", it is starvation or a hang — see [`Client::read_line_or_explain`] for
/// what the client does before it is willing to say which.
pub const READ_TIMEOUT: Duration = Duration::from_secs(20);

impl Client {
    pub fn connect(handle: &ServerHandle) -> Self {
        Self::connect_path(handle.socket_path())
    }

    /// The same connection against a **bare socket path**, for a server this harness did not spawn.
    ///
    /// The one caller is `tests/machine_replaced.rs`, whose subject is a HOSTED deployment: §11.40 M2
    /// makes `emulator/machineReplaced` a window's event, and [`spawn`] builds the standalone
    /// arrangement, which by that ruling must never advertise or emit it. `tests/hosted.rs` solved the
    /// same problem by hand-rolling a second client — and that client does **not** validate lines against
    /// the vendored schema, which is precisely the check an event fragment's `required` and its closed
    /// `reason` enum need. So the seam is here rather than a third copy of a client: one funnel,
    /// [`Client::recv`], validates every line whichever arrangement produced it.
    pub fn connect_path(socket: &std::path::Path) -> Self {
        // The accept loop polls, so a connect immediately after spawn may beat it by a few ms.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match UnixStream::connect(socket) {
                Ok(s) => {
                    s.set_read_timeout(Some(READ_TIMEOUT)).unwrap();
                    return Self {
                        reader: BufReader::new(s.try_clone().unwrap()),
                        writer: s,
                        next_id: 1,
                        pending: HashMap::new(),
                        awaiting: None,
                        read_timeout: READ_TIMEOUT,
                    };
                }
                Err(e) if std::time::Instant::now() < deadline => {
                    let _ = e;
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("connect: {e}"),
            }
        }
    }

    /// **Re-arm the read deadline mid-connection**, and remember the new number so a failure quotes it.
    ///
    /// The seam is for one test —
    /// `handshake::a_read_that_times_out_names_the_request_it_was_waiting_for` — which has to *observe* a
    /// blown deadline, and observing the real one costs [`READ_TIMEOUT`]. It is a setter rather than a
    /// second constructor on purpose: the handshake in front of that test keeps the ordinary, generous
    /// deadline, and only the one call being watched runs against a short one. A test that pins the
    /// timeout message must not itself become the load-sensitive row it exists to make legible.
    pub fn set_read_timeout(&mut self, d: Duration) {
        self.reader
            .get_ref()
            .set_read_timeout(Some(d))
            .expect("re-arm the read deadline");
        self.read_timeout = d;
    }

    /// **Claim the next request id**, for a test that writes its request line by hand and then reads the
    /// stream itself.
    ///
    /// [`Client::call`] hides the id because it also hides the events — `recv_response` discards every
    /// notification queued ahead of the reply. A row whose subject IS those notifications cannot use it,
    /// and hand-rolling the id would let a test's counter drift from `send_raw`'s `pending` ledger, which
    /// is what picks the per-method result schema. So the counter is handed out from the one place that
    /// owns it.
    pub fn next_request_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Write one line verbatim.
    ///
    /// The line is **not** validated against the schema on its way out, and that is a decision rather
    /// than an omission — see the module doc on `common::schema`. Several tests deliberately send
    /// malformed params to assert `-32602`, so outgoing validation would need a per-call opt-out at every
    /// such site; the server, not the test client, is the conformance subject.
    ///
    /// It is still *parsed*, purely to learn `id -> method` for a well-formed request, so a hand-written
    /// request gets its reply checked against the right per-method result schema.
    pub fn send_raw(&mut self, line: &str) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let (Some(id), Some(m)) = (v.get("id"), v.get("method").and_then(Value::as_str)) {
                if !id.is_null() {
                    self.pending.insert(id.to_string(), m.to_string());
                    self.awaiting = Some(format!("{m} (id {id})"));
                }
            }
        }
        self.writer.write_all(line.as_bytes()).unwrap();
        self.writer.write_all(b"\n").unwrap();
        self.writer.flush().unwrap();
    }

    /// Read one NDJSON line as JSON. Panics on EOF or timeout — a hung transport is a test failure.
    ///
    /// **This is the single funnel for every line the server sends, and therefore where contract §8
    /// item 15 lands**: every line is validated against the schema here, so no test can receive an
    /// off-contract shape without failing. See `common::schema` for what that covers and what it
    /// structurally cannot.
    pub fn recv(&mut self) -> Value {
        let line = self.read_line_or_explain();
        let v: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("bad JSON on the wire: {e}: {line}"));
        let method = v
            .get("id")
            .filter(|i| !i.is_null())
            .and_then(|i| self.pending.get(&i.to_string()))
            .cloned();
        if v.get("id").is_some_and(|i| !i.is_null()) {
            self.awaiting = None;
        }
        schema::assert_incoming(&v, method.as_deref());
        v
    }

    /// One line off the wire, or **a panic that says what the client was waiting for and for how long.**
    ///
    /// This used to be `read_line(&mut line).expect("read")`, and the message it produced was
    /// `read: Os { code: 11, kind: WouldBlock }` — an errno, at a `common/mod.rs` line number, naming
    /// neither the method nor the deadline. That message is the reason `F-HANDSHAKE-LOAD-TIMEOUT` was
    /// booked as "a socket read timeout, cause unknown", and it is the same message that let the
    /// `wait_for_break` row be written off as a flake **twice** (2026-09-03 and 2026-09-04) before it was
    /// root-caused to a real ordering defect. A load-sensitive failure whose only evidence is `EAGAIN` is
    /// indistinguishable from noise, so it gets read as noise.
    ///
    /// Two things are added, and each answers a question the bare errno could not.
    ///
    /// 1. **What was outstanding.** `awaiting` carries `method (id N)`, so the panic names the row. In the
    ///    booked failure that one word — `emulator/step_out` — is the whole diagnosis.
    /// 2. **Whether the server answered late, or not at all.** These are opposite defects — the first is a
    ///    slow or starved machine, the second is a hang or a deadlock — and the deadline alone cannot tell
    ///    them apart, because it fires identically for both. So the deadline is *not* treated as final:
    ///    the socket is re-armed for a grace window and read once more, purely so the panic can state
    ///    which of the two happened. A line that arrives in the grace window proves the server was alive
    ///    and merely behind; silence through both windows is the stronger claim, and only then is it made.
    ///
    /// The grace read costs nothing on a passing run — it is only ever reached after a deadline has
    /// already blown, i.e. on a run that is failing either way.
    fn read_line_or_explain(&mut self) -> String {
        // One buffer across both attempts: `read_line`'s contents are unspecified on error, and a partial
        // line already drained out of the socket must not be dropped on the floor by the retry.
        let mut line = String::new();
        let started = std::time::Instant::now();
        match self.reader.read_line(&mut line) {
            Ok(0) => panic!(
                "connection closed while a reply was expected. {}",
                self.awaiting_note()
            ),
            Ok(_) => return line,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => panic!("read failed: {e}. {}", self.awaiting_note()),
        }
        let blown = started.elapsed();
        let grace = self.read_timeout / 2;
        self.reader
            .get_ref()
            .set_read_timeout(Some(grace))
            .expect("re-arm the read deadline for the grace window");
        match self.reader.read_line(&mut line) {
            Ok(n) if n > 0 => panic!(
                "the server answered LATE, not never. Nothing arrived within the {:?} read deadline, \
                 but a line did arrive {:?} after it — so the server is alive and behind (a slow or \
                 starved machine), NOT hung. {} Late line: {}",
                self.read_timeout,
                started.elapsed() - blown,
                self.awaiting_note(),
                line.trim(),
            ),
            // **This branch states what it ruled out, and stops.** An earlier wording said "so this is a
            // hang or a deadlock rather than slowness", and the F-HANDSHAKE-LOAD-TIMEOUT repro caught it
            // lying: at load average 47 a server that was merely starved failed to finish inside the
            // deadline *and* the grace, and the message called it a deadlock. Silence through both
            // windows rules out "late, but within the grace" and rules out nothing else. Only the branch
            // above gets to make a positive claim, because only it has a line to show for it.
            _ => panic!(
                "the server did not answer within the {:?} read deadline, nor within a further {:?} of \
                 grace. That rules out a server that was only a little behind, and leaves two: it is \
                 hung, or it is starved badly enough not to finish inside {:?}. Check the machine's load \
                 before concluding the first. {}",
                self.read_timeout,
                grace,
                self.read_timeout + grace,
                self.awaiting_note(),
            ),
        }
    }

    /// The "what was it waiting for" clause of a transport failure message.
    fn awaiting_note(&self) -> String {
        match &self.awaiting {
            Some(req) => format!("The outstanding request was {req}."),
            None => "No request was outstanding on this connection — the read was waiting for a \
                     server-pushed event."
                .to_string(),
        }
    }

    /// Read lines until one has an `id` (i.e. skip any events queued ahead of the reply).
    pub fn recv_response(&mut self) -> Value {
        loop {
            let v = self.recv();
            if v.get("id").is_some_and(|i| !i.is_null()) {
                return v;
            }
        }
    }

    /// Send a request and read its response, skipping intervening events.
    pub fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send_raw(
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string(),
        );
        let v = self.recv_response();
        assert_eq!(v["id"], json!(id), "response id must correlate");
        v
    }

    /// `call`, asserting success and returning `result`.
    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params);
        assert!(v.get("error").is_none(), "{method} failed: {}", v["error"]);
        v["result"].clone()
    }

    /// `call`, asserting failure and returning the error object.
    pub fn err(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params);
        assert!(v.get("result").is_none(), "{method} unexpectedly succeeded");
        v["error"].clone()
    }

    /// The full `initialize` + `initialized` handshake. Returns the `initialize` result.
    pub fn handshake(&mut self, events: bool) -> Value {
        let r = self.ok(
            "initialize",
            json!({
                "clientId": "test",
                "clientName": "aether-tests",
                "clientVersion": "0",
                "protocolVersion": 1,
                "clientCapabilities": {"events": events},
            }),
        );
        self.send_raw(&json!({"jsonrpc":"2.0","method":"initialized"}).to_string());
        r
    }
}
