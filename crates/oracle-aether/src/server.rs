//! The reference transport (`protocol.md` §7.1): `AF_UNIX`/`SOCK_STREAM`, NDJSON in both directions,
//! one handler thread per connection, mode `0600` (D8).
//!
//! # Threading, and why the emulator cannot be wedged
//!
//! ```text
//!                    ┌── reader thread ──┐   Outbound   ┌── writer thread ──┐
//!   client socket ───┤                   ├─────────────▶│  (blocks freely)  ├──▶ client socket
//!                    └────────┬──────────┘   (bounded)  └───────────────────┘
//!                             │ Call{..., reply}              ▲
//!                             ▼                               │ push_event (never blocks)
//!                    ╔═══════════════════════════════════════════════╗
//!                    ║  engine thread — the ONLY owner of `System`   ║
//!                    ╚═══════════════════════════════════════════════╝
//! ```
//!
//! The emulator thread never touches a socket. It answers on a channel and it broadcasts through
//! [`Outbound::push_event`], which drops the oldest queued message rather than waiting. So the failure
//! that destroyed a frozen repro frame — *"lost to an emulator control-socket hang before the sprite
//! table could be dumped"* (`aeon/docs/BUGS.md:494-551`) — has no path to happen here: there is no
//! socket write anywhere on the emulator thread to hang on.
//!
//! `System` stays single-threaded throughout, as the core's charter requires; the channel is the only
//! way in.

use crate::engine::{self, Engine, EngineConfig};
use crate::outbound::{Outbound, Subscribers, DEFAULT_CAPACITY};
use crate::rpc::{self, LineRead, RpcError};
use crate::session::{Action, Session};
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use serde_json::{json, Map, Value};
use std::io::{BufReader, BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often the accept loop re-checks the shutdown flag. The listener is non-blocking so that a
/// shutdown never has to wait on a connection that may never arrive.
const ACCEPT_POLL: Duration = Duration::from_millis(5);

/// Server tunables.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub socket_path: PathBuf,
    pub engine: EngineConfig,
    /// Per-connection outbound queue depth. Events beyond it are dropped oldest-first and counted.
    pub event_queue_cap: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            engine: EngineConfig::default(),
            event_queue_cap: DEFAULT_CAPACITY,
        }
    }
}

/// The socket path, resolved exactly as `protocol.md` §7.1 specifies:
/// `$ORACLE_SOCKET` → (transitional) `$EXODUS_SOCKET` → `$XDG_RUNTIME_DIR/oracle.sock` →
/// `/tmp/oracle.sock`.
pub fn default_socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("ORACLE_SOCKET") {
        return PathBuf::from(p);
    }
    if let Some(p) = std::env::var_os("EXODUS_SOCKET") {
        return PathBuf::from(p);
    }
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir).join("oracle.sock");
    }
    PathBuf::from("/tmp/oracle.sock")
}

/// Everything the machine needs at startup. Kept separate from [`ServerConfig`] because it is *state*,
/// not configuration.
pub struct Machine {
    pub system: System,
    pub rom_path: Option<String>,
    pub symbols: Option<SymbolTable>,
    pub symbols_path: Option<String>,
}

impl Machine {
    pub fn new(system: System) -> Self {
        Self {
            system,
            rom_path: None,
            symbols: None,
            symbols_path: None,
        }
    }
}

/// A bound-but-not-yet-serving listener. Binding is separate from serving so a caller can learn the
/// real path (and fail fast on a busy socket) before handing over the machine.
pub struct Server {
    listener: UnixListener,
    config: ServerConfig,
}

/// A live server. Dropping it shuts everything down and unlinks the socket.
pub struct ServerHandle {
    socket_path: PathBuf,
    stop: Arc<AtomicBool>,
    accept_thread: Option<std::thread::JoinHandle<()>>,
    engine_thread: Option<std::thread::JoinHandle<()>>,
    engine_tx: Sender<EngineMsg>,
    conns: Arc<Mutex<Vec<Option<UnixStream>>>>,
}

impl ServerHandle {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Stop accepting, close every live connection, stop the emulator thread, unlink the socket.
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.engine_tx.send(EngineMsg::Shutdown);
        if let Ok(conns) = self.conns.lock() {
            for c in conns.iter().flatten() {
                let _ = c.shutdown(std::net::Shutdown::Both);
            }
        }
        if let Some(t) = self.accept_thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.engine_thread.take() {
            let _ = t.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The machine coordinate, published lock-free by the emulator thread so a connection thread can stamp
/// an envelope-level error (a parse failure, a handshake violation) without a round trip.
///
/// Replies that *did* reach the engine carry the engine's own exact stamp instead; this snapshot is only
/// ever used for errors the engine never saw, and is at most one frame stale.
#[derive(Default)]
pub(crate) struct SharedStamp {
    mclk: AtomicU64,
    running: AtomicBool,
}

impl SharedStamp {
    pub(crate) fn store(&self, mclk: u64, running: bool) {
        self.mclk.store(mclk, Ordering::Relaxed);
        self.running.store(running, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Map<String, Value> {
        let mclk = self.mclk.load(Ordering::Relaxed);
        let mut m = rpc::stamp_object(
            mclk / oracle_core::system::MCLK_PER_FRAME,
            mclk,
            self.running.load(Ordering::Relaxed),
        );
        // Honest about its own provenance: this stamp was cached, not taken at reply time.
        m.insert("stampCached".into(), json!(true));
        m
    }
}

pub(crate) enum EngineMsg {
    Call {
        method: String,
        params: Value,
        reply: Sender<CallResult>,
    },
    Initialize {
        params: Value,
        reply: Sender<CallResult>,
    },
    Shutdown,
}

pub(crate) struct CallResult {
    pub(crate) result: Result<Value, RpcError>,
    pub(crate) stamp: Map<String, Value>,
}

/// Everything the accept loop and its connection threads share. Bundled because there are two of them —
/// [`Server::spawn`] and [`crate::host::Host::serve`] — and a per-field parameter list is how the two drift
/// apart.
pub(crate) struct AcceptCtx {
    pub(crate) subs: Subscribers,
    pub(crate) shared: Arc<SharedStamp>,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) conns: Arc<Mutex<Vec<Option<UnixStream>>>>,
    /// Connections currently live. A host uses it to skip per-frame work (publishing the picture) that
    /// nobody is there to read; the standalone server ignores it.
    pub(crate) live: Arc<AtomicUsize>,
    pub(crate) event_queue_cap: usize,
}

impl AcceptCtx {
    pub(crate) fn new(event_queue_cap: usize) -> Self {
        Self {
            subs: Subscribers::new(),
            shared: Arc::new(SharedStamp::default()),
            stop: Arc::new(AtomicBool::new(false)),
            conns: Arc::new(Mutex::new(Vec::new())),
            live: Arc::new(AtomicUsize::new(0)),
            event_queue_cap,
        }
    }

    fn clone_handles(&self) -> Self {
        Self {
            subs: self.subs.clone(),
            shared: Arc::clone(&self.shared),
            stop: Arc::clone(&self.stop),
            conns: Arc::clone(&self.conns),
            live: Arc::clone(&self.live),
            event_queue_cap: self.event_queue_cap,
        }
    }

    /// Stop accepting and hang up on every live connection. Idempotent.
    pub(crate) fn close_all(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(conns) = self.conns.lock() {
            for c in conns.iter().flatten() {
                let _ = c.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

/// The accept loop, shared by the standalone server and the hosted one. It knows nothing about who owns the
/// `System`: every connection reaches the engine through `engine_tx` and nothing else, which is exactly why
/// the same loop serves both arrangements.
pub(crate) fn spawn_accept(
    listener: UnixListener,
    ctx: &AcceptCtx,
    engine_tx: Sender<EngineMsg>,
) -> std::thread::JoinHandle<()> {
    let ctx = ctx.clone_handles();
    std::thread::Builder::new()
        .name("aether-accept".into())
        .spawn(move || {
            while !ctx.stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        // Park a clone in a slot so a shutdown can unblock this connection's reader. The
                        // connection thread clears its own slot on exit, so the registry tracks *live*
                        // connections rather than growing forever — a long-lived bus will see thousands of
                        // short client sessions.
                        let slot = stream.try_clone().ok().map(|clone| {
                            let mut c = ctx
                                .conns
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            match c.iter().position(Option::is_none) {
                                Some(i) => {
                                    c[i] = Some(clone);
                                    i
                                }
                                None => {
                                    c.push(Some(clone));
                                    c.len() - 1
                                }
                            }
                        });
                        let tx = engine_tx.clone();
                        let conn = ctx.clone_handles();
                        conn.live.fetch_add(1, Ordering::SeqCst);
                        let spawned = std::thread::Builder::new()
                            .name("aether-conn".into())
                            .spawn(move || {
                                connection_loop(
                                    stream,
                                    tx,
                                    conn.subs.clone(),
                                    &conn.shared,
                                    &conn.stop,
                                    conn.event_queue_cap,
                                );
                                conn.live.fetch_sub(1, Ordering::SeqCst);
                                if let Some(i) = slot {
                                    let mut c = conn
                                        .conns
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                                    c[i] = None;
                                }
                            })
                            .is_ok();
                        if !spawned {
                            // The thread never started, so nothing will ever decrement for it.
                            ctx.live.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(ACCEPT_POLL);
                    }
                    Err(_) => std::thread::sleep(ACCEPT_POLL),
                }
            }
        })
        .expect("spawn accept thread")
}

impl Server {
    /// Bind the socket, enforcing mode `0600` (D8) and refusing to squat on a live server's path.
    pub fn bind(config: ServerConfig) -> std::io::Result<Self> {
        let path = &config.socket_path;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        // A leftover socket file from a crashed server must not block startup — but a *live* server on
        // the same path must, or two emulators would fight over one bus. Connecting is the only way to
        // tell the two apart.
        if path.exists() {
            match UnixStream::connect(path) {
                Ok(_) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!(
                            "another Aether server is already live on {}",
                            path.display()
                        ),
                    ))
                }
                Err(_) => {
                    // Nothing answered, so the path is a corpse — but only unlink it if it is a SOCKET.
                    // `bind` is given a path from a config file or `--socket`, and a typo that names a
                    // real file (a ROM, a listing, `~/.bashrc`) would otherwise be silently deleted by a
                    // server that never even started serving. Refusing is recoverable; deleting is not.
                    let ft = std::fs::metadata(path)?.file_type();
                    if !std::os::unix::fs::FileTypeExt::is_socket(&ft) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            format!(
                                "{} exists and is not a socket ({ft:?}) — refusing to delete it. \
                                 Point --socket somewhere else, or remove the file yourself.",
                                path.display()
                            ),
                        ));
                    }
                    std::fs::remove_file(path)?
                }
            }
        }
        let listener = UnixListener::bind(path)?;

        // D8: "SHOULD be created mode 0600". `bind` creates with 0777 & ~umask, so this is a
        // narrowing chmod immediately afterwards — there is a brief window in which the socket is
        // whatever the umask allowed. Documented rather than hidden; closing it properly needs a
        // pre-bind umask (libc) and this crate is `forbid(unsafe_code)`. The verification below at
        // least guarantees we never *serve* on a socket that is not 0600.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
        if mode != 0o600 {
            let _ = std::fs::remove_file(path);
            return Err(std::io::Error::other(format!(
                "refusing to serve: {} is mode {:o}, not 0600 (D8)",
                path.display(),
                mode
            )));
        }
        listener.set_nonblocking(true)?;
        Ok(Self { listener, config })
    }

    pub fn socket_path(&self) -> &Path {
        &self.config.socket_path
    }

    /// Take the bound listener apart, for a caller that runs its own accept wiring
    /// ([`crate::host::Host::serve`]). Crate-private: binding is the only part of [`Server`] that is
    /// reusable, and exposing the listener publicly would let a caller serve on a socket whose 0600 check
    /// [`Server::bind`] performed and then bypass everything that check protects.
    pub(crate) fn into_parts(self) -> (UnixListener, ServerConfig) {
        (self.listener, self.config)
    }

    /// Start the emulator thread and the accept loop. Returns immediately; the returned handle owns the
    /// shutdown.
    pub fn spawn(self, machine: Machine) -> ServerHandle {
        let Server { listener, config } = self;
        let ctx = AcceptCtx::new(config.event_queue_cap);
        let (engine_tx, engine_rx) = mpsc::channel::<EngineMsg>();

        let mut engine = Engine::new(machine.system, config.engine.clone(), ctx.subs.clone());
        engine.set_rom_path(machine.rom_path);
        engine.set_symbols(machine.symbols, machine.symbols_path);

        let engine_shared = Arc::clone(&ctx.shared);
        let engine_thread = std::thread::Builder::new()
            .name("aether-engine".into())
            .spawn(move || engine_loop(engine, engine_rx, &engine_shared))
            .expect("spawn engine thread");

        let accept_thread = spawn_accept(listener, &ctx, engine_tx.clone());

        ServerHandle {
            socket_path: config.socket_path,
            stop: Arc::clone(&ctx.stop),
            accept_thread: Some(accept_thread),
            engine_thread: Some(engine_thread),
            engine_tx,
            conns: Arc::clone(&ctx.conns),
        }
    }
}

/// The emulator thread. Owns `System` for its whole life; nothing else ever touches it.
fn engine_loop(mut engine: Engine, rx: mpsc::Receiver<EngineMsg>, shared: &SharedStamp) {
    let publish = |e: &Engine, s: &SharedStamp| {
        let stamp = e.stamp();
        s.store(
            stamp.get("mclk").and_then(Value::as_u64).unwrap_or(0),
            e.is_running(),
        );
    };
    publish(&engine, shared);
    loop {
        // While free-running, never park on the channel: poll it between frames so a `pause` lands
        // within one frame. While paused, park — an idle bus must not burn a core.
        let msg = if engine.is_running() {
            match rx.try_recv() {
                Ok(m) => Some(m),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => return,
            }
        };
        match msg {
            Some(EngineMsg::Shutdown) => return,
            Some(EngineMsg::Call {
                method,
                params,
                reply,
            }) => {
                let result = engine.dispatch(&method, &params);
                let stamp = engine.stamp();
                publish(&engine, shared);
                let _ = reply.send(CallResult { result, stamp });
            }
            Some(EngineMsg::Initialize { params, reply }) => {
                let result = engine.initialize_result(&params);
                let stamp = engine.stamp();
                let _ = reply.send(CallResult { result, stamp });
            }
            None => {
                let started = std::time::Instant::now();
                let pace = engine.free_run_step();
                publish(&engine, shared);
                // Sleep only the *remainder* of the interval. Sleeping the full interval on top of the
                // frame's own cost paces at ~53 Hz rather than 60 — measured, not assumed. Pacing is
                // wall-clock by nature and deliberately touches no emulated stamp (recon §5 C2), so
                // getting it wrong would cost nothing but a slow-looking game; getting it right costs
                // three lines.
                if let Some(rest) = pace.and_then(|p| p.checked_sub(started.elapsed())) {
                    std::thread::sleep(rest);
                }
            }
        }
    }
}

/// One connection: reads NDJSON, runs the handshake state machine, forwards to the engine, and hands
/// every outgoing line to its own writer thread.
pub(crate) fn connection_loop(
    stream: UnixStream,
    engine_tx: Sender<EngineMsg>,
    subs: Subscribers,
    shared: &SharedStamp,
    // `stop` is the server-wide shutdown flag. It is read by exactly one thing here —
    // `wait_for_break_delay`, the only place a connection thread sleeps for longer than a socket read —
    // because a wait must not outlive the server it is waiting on.
    stop: &AtomicBool,
    queue_cap: usize,
) {
    let Ok(write_half) = stream.try_clone() else {
        return;
    };
    let out = Arc::new(Outbound::new(queue_cap));
    let writer_out = Arc::clone(&out);
    let writer = std::thread::Builder::new()
        .name("aether-writer".into())
        .spawn(move || {
            let mut w = BufWriter::new(write_half);
            while let Some(line) = writer_out.pop() {
                if w.write_all(line.as_bytes()).is_err()
                    || w.write_all(b"\n").is_err()
                    || w.flush().is_err()
                {
                    break;
                }
            }
            writer_out.close();
            let _ = w.flush();
        })
        .ok();

    let mut reader = BufReader::new(stream);
    let mut session = Session::new();
    // Monotonic per-connection total, reported on every reply so a gap in the event stream is never
    // silent — the client learns exactly how many pushes it missed by not draining.
    let mut dropped_total: u64 = 0;

    loop {
        let line = match rpc::read_line_capped(&mut reader, rpc::MAX_LINE_BYTES) {
            Ok(LineRead::Eof) | Err(_) => break,
            Ok(LineRead::TooLong) => {
                dropped_total += out.take_dropped();
                let e = RpcError::invalid_request(format!(
                    "message exceeds the {}-byte line limit",
                    rpc::MAX_LINE_BYTES
                ));
                if !out.push_response(rpc::error_response(
                    None,
                    &e,
                    &with_dropped(shared.snapshot(), dropped_total),
                )) {
                    break;
                }
                continue;
            }
            Ok(LineRead::Payload(p)) => p,
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }

        dropped_total += out.take_dropped();
        // `None` = send nothing. JSON-RPC 2.0: a notification is never answered — not even when it
        // fails. A *parse* failure is the one exception, because a message we could not parse is a
        // message whose notification-ness we do not know, and silence would leave the client waiting
        // forever for a reply it will never get.
        let outgoing: Option<String> = match rpc::parse_line(&line) {
            Err((id, e)) => Some(rpc::error_response(
                id.as_ref(),
                &e,
                &with_dropped(shared.snapshot(), dropped_total),
            )),
            Ok(msg) => {
                let id = msg.id.clone();
                let is_notification = msg.is_notification();
                match session.on_message(&msg) {
                    Err(e) => (!is_notification).then(|| {
                        rpc::error_response(
                            id.as_ref(),
                            &e,
                            &with_dropped(shared.snapshot(), dropped_total),
                        )
                    }),
                    Ok(Action::Ignore) => None,
                    Ok(Action::Subscribe { events }) => {
                        // D6: registration happens here and nowhere earlier — no event may be pushed to
                        // a connection before it has sent `initialized`.
                        if events {
                            subs.add(Arc::clone(&out));
                        }
                        None
                    }
                    Ok(Action::Initialize) => {
                        let (tx, rx) = mpsc::channel();
                        if engine_tx
                            .send(EngineMsg::Initialize {
                                params: msg.params.clone(),
                                reply: tx,
                            })
                            .is_err()
                        {
                            break;
                        }
                        let Ok(r) = rx.recv() else { break };
                        Some(render(id.as_ref(), r, &mut dropped_total, &out))
                    }
                    Ok(Action::Dispatch) => {
                        // **`emulator/wait_for_break` waits HERE, on this connection's own thread, and
                        // never on the engine's.** See [`wait_for_break_delay`]. The engine handler that
                        // eventually runs is a pure poll; all this does is delay the forward until the
                        // machine has stopped or the caller's own deadline has passed.
                        let waited = wait_for_break_delay(&msg.method, &msg.params, shared, stop);
                        let (tx, rx) = mpsc::channel();
                        if engine_tx
                            .send(EngineMsg::Call {
                                method: msg.method.clone(),
                                params: msg.params.clone(),
                                reply: tx,
                            })
                            .is_err()
                        {
                            break;
                        }
                        let Ok(mut r) = rx.recv() else { break };
                        // **`waitedMs` has exactly one writer, and it is this line.** It is wall-clock
                        // time spent waiting, which is a host-side fact about the WAIT rather than a
                        // machine coordinate (the fragment says so in as many words), so it is knowable
                        // only here — the engine handler did not wait and would be guessing. D11's
                        // emulated-clocks rule governs the stamp beside it, not this.
                        if let (Some(w), Ok(v)) = (waited, &mut r.result) {
                            if let Value::Object(m) = v {
                                m.insert("waitedMs".into(), json!(w.as_millis() as u64));
                            }
                        }
                        (!is_notification).then(|| render(id.as_ref(), r, &mut dropped_total, &out))
                    }
                }
            }
        };
        if let Some(line) = outgoing {
            if !out.push_response(line) {
                break;
            }
        }
    }

    out.close();
    if let Some(w) = writer {
        let _ = w.join();
    }
}

/// How often [`wait_for_break_delay`] re-reads the published run state. Short enough that a break is
/// noticed inside one emulated frame, long enough that a five-minute wait costs ~150,000 relaxed atomic
/// loads and nothing else.
const WAIT_POLL: Duration = Duration::from_millis(2);

/// **Where `emulator/wait_for_break` actually waits — on the calling connection's thread, never the
/// engine's.**
///
/// # The transport problem this solves
///
/// `wait_for_break` is a poll-shaped method on an asynchronous transport, and the obvious implementation —
/// sleep inside the handler until a break arrives — is not merely rude here, it is **self-defeating**. The
/// engine thread is the only owner of `System` and it serialises: `engine_loop` takes one `EngineMsg`,
/// dispatches it to completion, and only then looks at the channel again. Its `None` branch — the branch
/// that calls `Engine::free_run_step` — is what advances a free-running machine. So a handler that slept
/// would:
///
/// 1. **stall every other client**, including the one that would call `emulator/pause` to end the wait, and
/// 2. **guarantee its own timeout**, because the frames that would have reached the breakpoint are exactly
///    the frames the sleeping thread is not running.
///
/// The same is true, worse, in the hosted arrangement: `Host::pump` checks its wall-clock budget *between*
/// commands and *"one that has started always finishes"*, so a 300-second handler would freeze the player's
/// window for 300 seconds.
///
/// # The answer
///
/// Waiting is a transport concern, not a machine concern, so it happens on the thread that is already
/// allowed to block. This function delays the *forward* of the call; the engine handler that eventually
/// runs does no waiting at all and simply reports the state it finds. Consequences, all of them wanted:
///
/// * **A concurrent request from another connection is completely unaffected** — different thread, and the
///   engine thread stays free to serve it. Two clients can wait at once.
/// * The machine keeps running throughout (free-run in the standalone server, the player's own loop when
///   hosted), which is what lets the breakpoint being waited for actually fire.
/// * A second request pipelined on the *same* connection queues behind the wait, because one connection is
///   one reader thread reading NDJSON in order. That is the client's own pipelining choice and is
///   unchanged by this.
///
/// # What it reads, and what it deliberately does not
///
/// The run state comes from [`SharedStamp`], published by the engine thread after every dispatch and every
/// free-run frame, so it is at most one frame stale — which costs a wait one frame of latency and nothing
/// else. `timeoutMs` is parsed **leniently and only as a sleep bound**: anything this function does not
/// recognise — a missing key, the wrong type, a value past the ceiling, or the snake_case `timeout_ms` a
/// legacy client might send — yields a zero delay, so the malformed request reaches the engine
/// *immediately* and is refused there. The engine is the authority on what is legal; this is only the
/// authority on how long to sleep, and it must never sleep on a request that is going to be refused.
fn wait_for_break_delay(
    method: &str,
    params: &Value,
    shared: &SharedStamp,
    stop: &AtomicBool,
) -> Option<Duration> {
    // `None`, not a zero duration: the caller uses the distinction to decide whether the reply gets a
    // `waitedMs` at all. Emitting one on some other method's reply would be an undeclared key on the wire.
    if method != "emulator/wait_for_break" {
        return None;
    }
    // Lenient by design — see the doc above. `None` (absent) takes the contract's own default; anything
    // unrecognised takes zero, so the engine gets to refuse it without a sleep in front of the refusal.
    let budget_ms = match params.get("timeoutMs") {
        None => engine::DEFAULT_WAIT_TIMEOUT_MS,
        Some(v) => match v.as_u64() {
            Some(n) if n <= engine::MAX_WAIT_TIMEOUT_MS => n,
            _ => 0,
        },
    };
    let started = Instant::now();
    let budget = Duration::from_millis(budget_ms);
    loop {
        // Already stopped is the answer, not a special case: a machine that is not running has already
        // broken. It is also what makes `timeoutMs: 0` — §11.24's "0 polls once and returns" — fall out of
        // this loop without a branch of its own.
        if !shared.running.load(Ordering::Relaxed) {
            return Some(started.elapsed());
        }
        if stop.load(Ordering::SeqCst) {
            return Some(started.elapsed());
        }
        let elapsed = started.elapsed();
        if elapsed >= budget {
            return Some(elapsed);
        }
        std::thread::sleep(WAIT_POLL.min(budget - elapsed));
    }
}

pub(crate) fn with_dropped(mut stamp: Map<String, Value>, dropped: u64) -> Map<String, Value> {
    stamp.insert("droppedEvents".into(), json!(dropped));
    stamp
}

fn render(id: Option<&Value>, r: CallResult, dropped_total: &mut u64, out: &Outbound) -> String {
    *dropped_total += out.take_dropped();
    let stamp = with_dropped(r.stamp, *dropped_total);
    match r.result {
        Ok(v) => rpc::success_response(id.unwrap_or(&Value::Null), rpc::stamp_result(v, &stamp)),
        Err(e) => rpc::error_response(id, &e, &stamp),
    }
}
