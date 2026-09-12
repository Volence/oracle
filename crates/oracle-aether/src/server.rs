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
//! table could be dumped"* (`aeon/docs/2026-09-09-BUGS-archived.md`, the `## BUG-005` entry) — has no
//! path to happen here: there is no socket write anywhere on the emulator thread to hang on.
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
use std::num::NonZeroUsize;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
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
    /// The `(device, inode)` of the socket file this bind created, taken while the listener is open. It
    /// becomes the accept thread's [`Listening`] claim, which removes that file, and only that file, when
    /// accepting stops (lens M13).
    bound: Option<(u64, u64)>,
}

/// A live server. Dropping it shuts everything down and unlinks the socket (its own, see
/// [`shutdown`](ServerHandle::shutdown)).
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

    /// **Block until the emulator thread ends, and say how it ended.** `Ok(())` when it was shut down;
    /// `Err` with the panic message when it died.
    ///
    /// The standalone binary parks on this instead of sleeping forever, so a dead machine ends the process
    /// with a non-zero status rather than leaving it parked behind a socket that answers nothing (lens M13).
    /// Called at most once: a second call, like [`shutdown`](Self::shutdown) after this, finds the thread
    /// already joined and answers `Ok(())`.
    pub fn wait(&mut self) -> Result<(), String> {
        match self.engine_thread.take() {
            Some(t) => t.join().map_err(|payload| panic_message(payload.as_ref())),
            None => Ok(()),
        }
    }

    /// Stop accepting, close every live connection, stop the emulator thread, unlink the socket.
    ///
    /// The unlink is the accept thread's, not this method's. That thread owns the [`Listening`] socket and
    /// removes this server's file as it stops, while the listener is still open (lens M13). Joining it
    /// below is what makes "the socket file is gone when this returns" true.
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
        // No unlink here (lens M13). The accept thread joined above removed this server's file while its
        // listener was still open. A `(device, inode)` check made now, with the listener closed, can match
        // a restarted server's socket that the filesystem gave the same inode number, and delete it: CI
        // red on `adcf239` and `85c1599`, see [`Listening`].
    }
}

/// The `(device, inode)` naming the file at `path` right now, or `None` when there is none. Not followed
/// through a symlink: the question is which file sits at this path.
fn socket_identity(path: &Path) -> Option<(u64, u64)> {
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.dev(), m.ino()))
}

/// **A listening socket, together with the claim to remove the file it is bound at** (lens M13). The
/// accept thread owns it, so the claim ends exactly when accepting ends: on [`ServerHandle::shutdown`], on
/// the emulator thread's death ([`HangUpIfPanicking`]), and on [`crate::host::Host::shutdown`] alike.
///
/// # Why the unlink lives here and not in the handle
///
/// The claim is released in this value's `Drop`, and Rust runs `Drop::drop` *before* it drops the fields,
/// so the file is removed while `listener` is still open. That ordering is the point. On Linux a bound
/// socket keeps a reference to its file for as long as it is open, so until the listener closes no other
/// file can be given that inode's number, and the `(device, inode)` comparison below is exact. Once the
/// listener closes the number is free, and ext4 hands it to the next file created: a restart's socket at
/// the same path. A comparison made after that point can take the restarted server's socket for this
/// one. That is what CI caught on `adcf239` and `85c1599`: [`ServerHandle::shutdown`] used to make this
/// comparison after the accept thread, and with it the listener, had gone.
///
/// Measured on a workstation's ext4: a socket file unlinked after its listener closed gave its inode
/// number to the next socket bound at the same path 20 times in 20, and one unlinked while its listener
/// was still open, 0 in 20. tmpfs mounted `inode64` never reuses a number, which is why the same test
/// passed locally with the old rule.
///
/// What the ordering buys, on both ways a server stops:
///
/// * **Before the unlink** the listener is open, so a `connect` still succeeds (it queues in the
///   backlog) and [`Server::bind`]'s probe refuses a restart with `AddrInUse`. No restart can have bound
///   the path yet.
/// * **After it** there is no file, so a restart binds without probing, and nothing of this server is
///   left that could touch the new file. A dead server's handle, dropped later, has nothing to unlink.
///
/// **One window remains**, between the comparison and the `remove_file`. Only something that removes a
/// live server's file and binds its own socket inside that interval could lose it, and [`Server::bind`]
/// cannot be that something, because its probe is answered by the live listener. Closing it would need
/// an unlink that names an inode rather than a path, and POSIX has none.
pub(crate) struct Listening {
    listener: UnixListener,
    /// The path and the `(device, inode)` of the file this value removes when it drops. `None` only when
    /// the bind could not stat the file it had just created. Then nothing is unlinked here, and the file
    /// outlives the server as a corpse that the next [`Server::bind`] clears.
    ///
    /// Every `Listening` is made by [`Server::listening`], so the standalone accept loop and the hosted
    /// one ([`crate::host::Host::serve`]) both carry their bind's claim, and nothing else in the crate
    /// unlinks a served socket (HOST-SHUTDOWN-UNLINK). There is deliberately no way to wrap a bare
    /// `UnixListener`: that door is how the hosted path came to unlink by path alone.
    claim: Option<(PathBuf, (u64, u64))>,
}

impl Drop for Listening {
    fn drop(&mut self) {
        // `self.listener` is still open here, so its inode number cannot have been given to another file.
        if let Some((path, bound)) = &self.claim {
            if socket_identity(path) == Some(*bound) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// **The emulator thread's last act when it dies by panicking** (lens M13): stop accepting and hang up on
/// every connection.
///
/// That thread is the only thing that can answer a request, so a server whose engine is gone must stop
/// looking alive. Before this, the accept loop and the socket outlived it: every new connection was
/// accepted and then closed without a reply, and a restart's incumbent probe was answered by the corpse.
/// Once the accept thread stops, it removes this server's socket file and only then closes the listener
/// ([`Listening`]), so a `connect` finds nothing at the path, which is what a client and
/// [`Server::bind`]'s probe both read as "nothing is serving here". The path is released at the last
/// moment it is provably this server's, so no restart can bind it before the release, and the dead
/// server's handle holds no claim afterwards that a restarted server's socket could be mistaken for.
///
/// A drop guard rather than `catch_unwind` so the panic still ends the thread as a panic: the default hook
/// prints it, and [`ServerHandle::wait`] receives the payload from `join`.
struct HangUpIfPanicking(AcceptCtx);

impl Drop for HangUpIfPanicking {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.close_all();
        }
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
/// Replies that *did* reach the engine carry the engine's own exact stamp instead; this snapshot stamps the
/// errors the engine never saw, and is at most one frame stale.
///
/// **It has a second reader, and that one cares about the staleness**: `running` is what
/// [`wait_for_stamp`] polls to decide whether `emulator/wait_for_break` waits at all. A snapshot that is
/// stale in the wrong direction — still `false` from a halt that a later `resume` undid — makes the
/// transport exit its wait instantly, and [`dispatch_call`] exists to reconcile that against what the
/// engine says. Publishers must therefore store **before** the reply that changed the state goes out; both
/// run drivers ([`engine_loop`], [`crate::host::Host::pump`]) do.
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

    /// The last-published run state. Relaxed, and **knowingly stale** — see the type's doc.
    pub(crate) fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
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
///
/// It does own the listener, and with it the claim to remove the socket file when accepting stops
/// ([`Listening`]), for both arrangements: [`Server::serve`] and [`crate::host::Host::serve`] each hand
/// over what [`Server::listening`] made of their bind. Joining this thread is therefore the unlink on both
/// paths, and neither [`ServerHandle`] nor [`crate::host::Host`] unlinks anything itself.
pub(crate) fn spawn_accept(
    listening: Listening,
    ctx: &AcceptCtx,
    engine_tx: Sender<EngineMsg>,
) -> std::thread::JoinHandle<()> {
    // The depth every connection's queue is built with (lens M71), checked once, here, on the caller's
    // thread, before any client exists. Unreachable as a failure: both callers hold a listener that
    // `Server::bind` produced, and bind refuses `event_queue_cap = 0` — the standalone server binds its own
    // config, and `Host::serve` builds its `ServerConfig` from this same `ctx` value and binds that.
    let queue_cap = NonZeroUsize::new(ctx.event_queue_cap).expect(
        "Server::bind refuses event_queue_cap = 0, and every caller of spawn_accept has bound",
    );
    let ctx = ctx.clone_handles();
    std::thread::Builder::new()
        .name("aether-accept".into())
        .spawn(move || {
            while !ctx.stop.load(Ordering::SeqCst) {
                match listening.listener.accept() {
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
                                    queue_cap,
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
            // Accepting has stopped. Dropping `listening` removes this server's socket file, when it holds
            // the claim, and only then closes the listener. That order is lens M13's fix: see `Listening`.
            drop(listening);
        })
        .expect("spawn accept thread")
}

impl Server {
    /// Bind the socket, enforcing mode `0600` (D8) and refusing to squat on a live server's path.
    pub fn bind(config: ServerConfig) -> std::io::Result<Self> {
        // Lens M71: a zero-depth outbound queue is a configuration error, refused here — before the
        // filesystem is touched, so a bad value leaves no socket file — rather than discovered by every
        // connection thread in turn while the socket stays bound. Both deployments pass through this bind
        // (`Host::serve` included), which is why it is the one place the check needs to live.
        if config.event_queue_cap == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "event_queue_cap is 0: every connection's outbound queue must hold at least one message, \
                 or it cannot carry a single reply",
            ));
        }
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
                                "{} exists and is not a socket ({ft:?}). Refusing to delete it. \
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
        let bound = socket_identity(path);
        Ok(Self {
            listener,
            config,
            bound,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.config.socket_path
    }

    /// **The bound listener with its claim on the socket file, ready for [`spawn_accept`]**, and the
    /// config it was bound under. The one way a [`Listening`] is made, used by the standalone server
    /// ([`Server::serve`]) and the hosted one ([`crate::host::Host::serve`]) alike, so both carry the
    /// `(device, inode)` this bind recorded and both unlink only their own file (lens M13;
    /// HOST-SHUTDOWN-UNLINK for the hosted half).
    ///
    /// Crate-private: binding is the only part of [`Server`] that is reusable, and exposing the listener
    /// publicly would let a caller serve on a socket whose 0600 check [`Server::bind`] performed and then
    /// bypass everything that check protects. It replaces `into_parts`, which returned the bare
    /// `UnixListener` and so dropped the claim: the hosted path then unlinked by path alone.
    pub(crate) fn listening(self) -> (Listening, ServerConfig) {
        let Server {
            listener,
            config,
            bound,
        } = self;
        let listening = Listening {
            listener,
            claim: bound.map(|id| (config.socket_path.clone(), id)),
        };
        (listening, config)
    }

    /// Start the emulator thread and the accept loop. Returns immediately; the returned handle owns the
    /// shutdown.
    pub fn spawn(self, machine: Machine) -> ServerHandle {
        let ctx = AcceptCtx::new(self.config.event_queue_cap);
        let mut engine = Engine::new(machine.system, self.config.engine.clone(), ctx.subs.clone());
        engine.set_rom_path(machine.rom_path);
        engine.set_symbols(machine.symbols, machine.symbols_path);
        self.serve(ctx, move |rx, shared| engine_loop(engine, rx, shared))
    }

    /// [`spawn`](Self::spawn) without the knowledge of what the emulator thread runs: start `engine_body`
    /// on the emulator thread and the accept loop beside it.
    ///
    /// Split out so the server's behaviour when that thread **dies** can be measured (lens M13) with a body
    /// that fails on purpose, instead of hunting for an input that makes the real engine panic. Every
    /// production path goes through [`spawn`](Self::spawn), which passes [`engine_loop`].
    fn serve<F>(self, ctx: AcceptCtx, engine_body: F) -> ServerHandle
    where
        F: FnOnce(mpsc::Receiver<EngineMsg>, &SharedStamp) + Send + 'static,
    {
        // The claim on the socket file travels with the listener it belongs to (lens M13, see `Listening`).
        let (listening, config) = self.listening();
        let (engine_tx, engine_rx) = mpsc::channel::<EngineMsg>();

        let engine_shared = Arc::clone(&ctx.shared);
        let on_death = ctx.clone_handles();
        let engine_thread = std::thread::Builder::new()
            .name("aether-engine".into())
            .spawn(move || {
                let _hang_up = HangUpIfPanicking(on_death);
                engine_body(engine_rx, &engine_shared)
            })
            .expect("spawn engine thread");

        let accept_thread = spawn_accept(listening, &ctx, engine_tx.clone());

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
    // `wait_for_stamp`, the only place a connection thread sleeps for longer than a socket read —
    // because a wait must not outlive the server it is waiting on.
    stop: &AtomicBool,
    queue_cap: NonZeroUsize,
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
                        // never on the engine's.** See [`dispatch_call`]. The engine handler that
                        // eventually runs is a pure poll; all this does is delay the forward until the
                        // machine has stopped or the caller's own deadline has passed — and reconcile
                        // the two halves before the reply goes out.
                        let Some(r) =
                            dispatch_call(&msg.method, &msg.params, &engine_tx, shared, stop)
                        else {
                            break;
                        };
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

/// How often [`wait_for_stamp`] re-reads the published run state. Short enough that a break is noticed
/// inside one emulated frame, long enough that a five-minute wait costs ~150,000 relaxed atomic loads and
/// nothing else.
const WAIT_POLL: Duration = Duration::from_millis(2);

/// **Forward one call to the engine — and, for `emulator/wait_for_break`, do the waiting first and
/// reconcile the two halves before the reply goes out.**
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
/// # ★ Why the forward is a LOOP and not a single shot ★
///
/// The two halves of this method read the same *quantity* — `Engine::is_running`, the free-run mode — but
/// they read it down **two different channels**, and that is the whole defect this loop exists to close.
/// The waiting half polls [`SharedStamp`], a snapshot **published by whoever owns the machine after the
/// fact**; the engine half re-reads the live flag at dispatch time. A published snapshot can be *stale in
/// the wrong direction*: it can still say `running: false` from a halt that a subsequent `emulator/resume`
/// has already undone. The waiting half then exits its poll **immediately**, the engine half finds the
/// machine running, and the caller who asked to wait ten seconds is told `{"timeoutReached": true}` after
/// approximately zero milliseconds — a wrong answer, not a slow one, and one that load makes *more*
/// likely because scheduling pressure is what widens the gap between the state change and its publication.
///
/// **Measured, not reasoned.** `tests/hosted.rs`'s `a_halted_window_resumes_past_its_own_breakpoint` — the
/// only test in that file that issues an `emulator/resume` immediately before a wait, and the only one
/// that ever went red — was run 30 times against the whole hosted suite under a loaded machine
/// (~300 spinners, load average ~250, 16 cores). Pre-fix: **7 of 30 red**, every one of them at the
/// *second* wait, every one with the same reply on the wire —
/// `{"frame":2,"mclk":1792182,"running":true,"timeoutReached":true,"waitedMs":0}` against
/// `timeoutMs: 10000`. Post-fix, identical load and run count: **0 of 30**. With the transport
/// reconciliation alone and `Host::pump`'s publish ordering put back the way it was: **0 of 30** — so this
/// loop, not the ordering, is what closes it; the ordering fix keeps the stamp honest for its other reader
/// and saves this loop a round trip. Unloaded, the pre-fix tree passed 30 of 30, which is why the defect
/// spent two lane-log entries booked as a flake.
///
/// (`Host::pump` used to publish at the *end* of a drain, after the reply to the `resume` that changed the
/// state had already gone out; `engine_loop` has always published before its reply. That ordering is now
/// the same on both drivers — but an ordering invariant in another file is not a guarantee this method can
/// rest on, and the stamp is documented as stale by construction. So the guarantee is made here.)
///
/// So a `timeoutReached: true` from the engine is treated as **evidence that the stamp was stale**, not as
/// an answer: while budget remains, this waits for the stamp to catch up to what the engine just said
/// (`wait_for_stamp(.., true)`), then goes back to waiting for the halt. The property the caller gets is
/// the one the method's name promises and the old code did not deliver:
///
/// > **A reply that says the wait expired has actually waited the caller's whole budget.**
///
/// The second wait — for the stamp to *agree* with the engine — is what keeps this bounded: without it a
/// permanently stale stamp would re-forward every 2 ms for the whole budget. With it, one extra forward is
/// spent per stale episode, and the pathological "the stamp never catches up" case spends the remaining
/// budget in a sleep and then answers honestly.
///
/// # What it reads, and what it deliberately does not
///
/// `timeoutMs` is parsed **leniently and only as a sleep bound**: anything [`wait_budget`] does not
/// recognise — a missing key, the wrong type, a value past the ceiling, or the snake_case `timeout_ms` a
/// legacy client might send — yields a zero delay, so the malformed request reaches the engine
/// *immediately* and is refused there. The engine is the authority on what is legal; this is only the
/// authority on how long to sleep, and it must never sleep on a request that is going to be refused. A
/// refusal comes back as `Err`, which is not `timeoutReached: true`, so it is never retried either.
///
/// Returns `None` only when the engine channel or its reply is gone — i.e. the connection is over.
fn dispatch_call(
    method: &str,
    params: &Value,
    engine_tx: &Sender<EngineMsg>,
    shared: &SharedStamp,
    stop: &AtomicBool,
) -> Option<CallResult> {
    // `None`, not a zero duration: the caller uses the distinction to decide whether the reply gets a
    // `waitedMs` at all. Emitting one on some other method's reply would be an undeclared key on the wire.
    let Some(budget) = wait_budget(method, params) else {
        return forward(method, params, engine_tx);
    };
    let started = Instant::now();
    loop {
        // The wait proper: block until the published state says the machine has stopped, the server is
        // going down, or the caller's own deadline has passed.
        wait_for_stamp(shared, stop, started, budget, false);
        let mut r = forward(method, params, engine_tx)?;
        let elapsed = started.elapsed();
        // The reconciliation. `timeoutReached: true` means the engine — which holds the machine and is the
        // authority — found it still running. If the budget has not been spent, the stamp we exited on was
        // stale, so this is not an answer yet.
        if says_timed_out(&r.result) && elapsed < budget && !stop.load(Ordering::SeqCst) {
            wait_for_stamp(shared, stop, started, budget, true);
            continue;
        }
        // **`waitedMs` has exactly one writer, and it is this line.** It is wall-clock time spent waiting,
        // which is a host-side fact about the WAIT rather than a machine coordinate (the fragment says so
        // in as many words), so it is knowable only here — the engine handler did not wait and would be
        // guessing. D11's emulated-clocks rule governs the stamp beside it, not this.
        if let Ok(Value::Object(m)) = &mut r.result {
            m.insert(
                "waitedMs".into(),
                json!(started.elapsed().as_millis() as u64),
            );
        }
        return Some(r);
    }
}

/// The wait bound for `method`, or `None` if this is not a call that waits at all.
///
/// Lenient by design — see [`dispatch_call`]. An absent `timeoutMs` takes the contract's own default;
/// anything unrecognised takes zero, so the engine gets to refuse it without a sleep in front of the
/// refusal.
fn wait_budget(method: &str, params: &Value) -> Option<Duration> {
    if method != "emulator/wait_for_break" {
        return None;
    }
    let budget_ms = match params.get("timeoutMs") {
        None => engine::DEFAULT_WAIT_TIMEOUT_MS,
        Some(v) => match v.as_u64() {
            Some(n) if n <= engine::MAX_WAIT_TIMEOUT_MS => n,
            _ => 0,
        },
    };
    Some(Duration::from_millis(budget_ms))
}

/// Block until the published run state reads `want`, the server is stopping, or `started + budget` has
/// passed. Returns as soon as any of the three holds.
///
/// `want: false` is the wait itself — a machine that is not running has already broken, which is also what
/// makes `timeoutMs: 0` (§11.24's *"0 polls once and returns"*) fall out without a branch of its own: the
/// budget is already spent on entry, so this returns at once and the single forward below is the poll.
///
/// `want: true` is the re-synchronisation after the engine has contradicted the stamp — see
/// [`dispatch_call`].
fn wait_for_stamp(
    shared: &SharedStamp,
    stop: &AtomicBool,
    started: Instant,
    budget: Duration,
    want: bool,
) {
    loop {
        if shared.is_running() == want || stop.load(Ordering::SeqCst) {
            return;
        }
        let elapsed = started.elapsed();
        if elapsed >= budget {
            return;
        }
        std::thread::sleep(WAIT_POLL.min(budget - elapsed));
    }
}

/// Whether an engine reply is the "still running, nothing observed" answer — the one
/// [`dispatch_call`] must not hand back until the budget really is gone.
fn says_timed_out(r: &Result<Value, RpcError>) -> bool {
    matches!(r, Ok(Value::Object(m)) if m.get("timeoutReached") == Some(&Value::Bool(true)))
}

/// Put one call on the engine's queue and block this connection thread on its reply. `None` means the
/// engine thread or the reply channel is gone, which ends the connection.
fn forward(method: &str, params: &Value, engine_tx: &Sender<EngineMsg>) -> Option<CallResult> {
    let (tx, rx) = mpsc::channel();
    engine_tx
        .send(EngineMsg::Call {
            method: method.to_string(),
            params: params.clone(),
            reply: tx,
        })
        .ok()?;
    rx.recv().ok()
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

// =====================================================================================================
// `emulator/wait_for_break` — the two halves, driven directly.
//
// **Why these are unit tests and not socket tests.** The defect they exist for is a disagreement between
// the connection thread's view of the run state (the published [`SharedStamp`]) and the engine's own, and
// over a socket it is a *race*: it needs the stamp to be stale at the instant the wait starts, which is a
// scheduling accident that peer load widens and an isolated run almost never produces. A test that needs
// the race to happen is a test that passes for the wrong reason. So the stamp and the engine's answers are
// both *inputs* here — constructed, not raced — and the ordering that used to be an accident is a fact of
// the fixture. Every one of these is deterministic on a loaded machine and an idle one alike.
// =====================================================================================================
#[cfg(test)]
mod wait_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// A stand-in engine thread: answers each `EngineMsg::Call` from a script indexed by call number, and
    /// counts them. It never touches a machine — what is under test is the *connection* half.
    struct FakeEngine {
        tx: Sender<EngineMsg>,
        calls: Arc<AtomicUsize>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl FakeEngine {
        fn new<F>(answer: F) -> Self
        where
            F: Fn(usize) -> Result<Value, RpcError> + Send + 'static,
        {
            let (tx, rx) = mpsc::channel::<EngineMsg>();
            let calls = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&calls);
            let thread = std::thread::spawn(move || {
                while let Ok(m) = rx.recv() {
                    match m {
                        EngineMsg::Shutdown => break,
                        EngineMsg::Call { reply, .. } | EngineMsg::Initialize { reply, .. } => {
                            let n = counter.fetch_add(1, Ordering::SeqCst);
                            let _ = reply.send(CallResult {
                                result: answer(n),
                                stamp: Map::new(),
                            });
                        }
                    }
                }
            });
            Self {
                tx,
                calls,
                thread: Some(thread),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl Drop for FakeEngine {
        fn drop(&mut self) {
            let _ = self.tx.send(EngineMsg::Shutdown);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    /// The engine's "still running, nothing observed" reply — no `pc`, by design.
    fn still_running() -> Result<Value, RpcError> {
        Ok(json!({"timeoutReached": true}))
    }

    /// The engine's "it stopped, here is where" reply.
    fn halted() -> Result<Value, RpcError> {
        Ok(json!({"pc": "0x0000020E", "timeoutReached": false}))
    }

    fn wait(
        fake: &FakeEngine,
        shared: &SharedStamp,
        params: Value,
    ) -> (Result<Value, RpcError>, Duration) {
        let stop = AtomicBool::new(false);
        let t = Instant::now();
        let r = dispatch_call("emulator/wait_for_break", &params, &fake.tx, shared, &stop)
            .expect("the fake engine answered");
        (r.result, t.elapsed())
    }

    /// ## ★ THE DEFECT ★ — **a reply that says the wait expired must have actually waited the budget.**
    ///
    /// The exact shape of the bug, reproduced as an ordering rather than as a race: the published stamp
    /// says `running: false` — stale, left over from a halt that a `resume` has already undone — while the
    /// engine, which holds the machine, says it is still running. The transport's poll therefore exits at
    /// once.
    ///
    /// Before the fix this returned `{"timeoutReached": true, "waitedMs": 0}` in under a millisecond
    /// against a 300 ms budget: the caller asked to wait and was told the wait expired without any wait
    /// having happened. `timeoutReached == false` is **not** the assertion that catches this — the honest
    /// answer here really is `timeoutReached: true`, because the machine really is running for the whole
    /// budget. The discriminator is the clock: a timeout that took no time is a wrong answer.
    ///
    /// Planting it back: make `says_timed_out` return `false`, which is exactly the single-shot forward
    /// the old `wait_for_break_delay` + one dispatch performed.
    #[test]
    fn a_reported_timeout_has_spent_the_whole_budget() {
        const BUDGET_MS: u64 = 300;
        // Stale in the wrong direction: the published snapshot still carries the halt.
        let shared = SharedStamp::default();
        shared.store(0, false);
        // …while the machine is, in fact, running for the whole of this test.
        let fake = FakeEngine::new(|_| still_running());

        let (r, elapsed) = wait(&fake, &shared, json!({"timeoutMs": BUDGET_MS}));
        let v = r.expect("a wait against a running machine is not an error");

        assert_eq!(
            v["timeoutReached"],
            json!(true),
            "the machine ran for the whole budget, so the timeout is the honest answer: {v}"
        );
        let waited = v["waitedMs"]
            .as_u64()
            .unwrap_or_else(|| panic!("a wait_for_break reply must carry waitedMs: {v}"));
        assert!(
            waited >= BUDGET_MS,
            "the reply says the wait expired after {waited} ms against a {BUDGET_MS} ms budget. A \
             timeout that did not wait is a WRONG answer, not a slow one: the connection thread exited \
             its poll on a stale `running: false` and the engine, re-reading the live flag, called that \
             a timeout. {v}"
        );
        assert!(
            elapsed >= Duration::from_millis(BUDGET_MS),
            "…and waitedMs must be the wall clock actually spent, not a number: {elapsed:?}"
        );
    }

    /// The other half of the same property, and the guard against "fix" it by always sleeping the budget:
    /// a stale stamp costs **one extra forward**, not the caller's whole ten seconds.
    ///
    /// The fixture is the real sequence: the stamp is stale-`false`, the engine says it is running, the
    /// stamp catches up to `true`, the machine then genuinely halts and the stamp goes `false` again. The
    /// reply must be the halt, promptly — the wait's entire purpose.
    #[test]
    fn a_stale_stamp_costs_one_extra_forward_not_the_budget() {
        const BUDGET_MS: u64 = 10_000;
        let shared = Arc::new(SharedStamp::default());
        shared.store(0, false); // stale: the resume has not been published yet
        let publisher = Arc::clone(&shared);
        let fake = FakeEngine::new(move |n| {
            if n == 0 {
                // The engine contradicts the stamp. Now let the publisher catch up, and halt shortly
                // after — exactly what a `resume` followed by a breakpoint looks like.
                publisher.store(0, true);
                let p = Arc::clone(&publisher);
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(40));
                    p.store(0, false);
                });
                still_running()
            } else {
                halted()
            }
        });

        let (r, elapsed) = wait(&fake, &shared, json!({"timeoutMs": BUDGET_MS}));
        let v = r.expect("the halt is not an error");
        assert_eq!(
            v["timeoutReached"],
            json!(false),
            "the machine halted well inside the budget and the wait must report the halt: {v}"
        );
        assert_eq!(v["pc"], json!("0x0000020E"), "…at its pc: {v}");
        assert_eq!(
            fake.calls(),
            2,
            "one forward per stale episode plus the answer. More means the reconciliation is spinning \
             the engine thread instead of waiting for the stamp to catch up"
        );
        assert!(
            elapsed < Duration::from_millis(BUDGET_MS / 2),
            "the wait sat out its budget instead of returning on the halt ({elapsed:?}) — a \
             reconciliation that always burns the budget passes the defect test above and breaks the \
             method"
        );
    }

    /// §11.24: **`timeoutMs: 0` polls once and returns.** The budget is spent on entry, so the
    /// reconciliation must not fire even though the engine says the machine is running.
    #[test]
    fn timeout_zero_polls_once_and_returns() {
        let shared = SharedStamp::default();
        shared.store(0, true); // accurate: the machine is running
        let fake = FakeEngine::new(|_| still_running());

        let (r, elapsed) = wait(&fake, &shared, json!({"timeoutMs": 0}));
        let v = r.expect("a zero-budget poll is not an error");
        assert_eq!(v["timeoutReached"], json!(true), "{v}");
        assert_eq!(fake.calls(), 1, "0 polls ONCE — one forward, no retry");
        assert!(
            elapsed < Duration::from_millis(100),
            "and returns: {elapsed:?}"
        );
    }

    /// A machine that has already stopped is answered immediately with its `pc`, and is **never resumed**
    /// (§5): one forward, no waiting, no retry.
    #[test]
    fn a_stopped_machine_answers_at_once_with_its_pc() {
        let shared = SharedStamp::default();
        shared.store(0, false);
        let fake = FakeEngine::new(|_| halted());

        let (r, elapsed) = wait(&fake, &shared, json!({"timeoutMs": 10_000}));
        let v = r.expect("a halted machine is not an error");
        assert_eq!(v["pc"], json!("0x0000020E"), "{v}");
        assert_eq!(v["timeoutReached"], json!(false), "{v}");
        assert_eq!(fake.calls(), 1);
        assert!(elapsed < Duration::from_millis(100), "{elapsed:?}");
    }

    /// A `timeoutMs` past the ceiling is **refused** (`-32602`), never clamped and never slept on — and an
    /// error is not a `timeoutReached`, so the reconciliation must not retry it into a storm of refusals.
    #[test]
    fn an_over_ceiling_timeout_is_refused_once_and_never_retried() {
        let shared = SharedStamp::default();
        shared.store(0, true);
        let fake =
            FakeEngine::new(|_| Err(RpcError::invalid_params("`timeoutMs` above the ceiling")));

        let (r, elapsed) = wait(
            &fake,
            &shared,
            json!({"timeoutMs": engine::MAX_WAIT_TIMEOUT_MS + 1}),
        );
        assert!(r.is_err(), "over the ceiling is a refusal, not a clamp");
        assert_eq!(fake.calls(), 1, "and it is refused once");
        assert!(
            elapsed < Duration::from_millis(100),
            "with no sleep in front of it: {elapsed:?}"
        );
    }

    /// Every other method is forwarded exactly once, with no `waitedMs` — an undeclared key on the wire is
    /// what this `None` branch exists to prevent.
    #[test]
    fn a_method_that_does_not_wait_is_forwarded_once_and_unstamped() {
        let shared = SharedStamp::default();
        shared.store(0, true);
        let fake = FakeEngine::new(|_| Ok(json!({"ok": true})));
        let stop = AtomicBool::new(false);
        let r = dispatch_call("emulator/status", &json!({}), &fake.tx, &shared, &stop)
            .expect("answered");
        let v = r.result.expect("status is not an error");
        assert!(
            v.get("waitedMs").is_none(),
            "waitedMs on a method that does not wait: {v}"
        );
        assert_eq!(fake.calls(), 1);
    }

    /// The server going down ends a wait rather than holding the shutdown for the caller's budget.
    #[test]
    fn a_shutdown_ends_a_wait_early() {
        let shared = Arc::new(SharedStamp::default());
        shared.store(0, true);
        let fake = FakeEngine::new(|_| still_running());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            flag.store(true, Ordering::SeqCst);
        });
        let t = Instant::now();
        let r = dispatch_call(
            "emulator/wait_for_break",
            &json!({"timeoutMs": 30_000}),
            &fake.tx,
            &shared,
            &stop,
        )
        .expect("answered");
        assert!(r.result.is_ok());
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "a wait must not outlive the server it is waiting on: {:?}",
            t.elapsed()
        );
    }
}

/// The text of a panic payload, for a report that has to say why a thread died.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with a non-string payload".to_string())
}

// =====================================================================================================
// What the server does when a thread it depends on cannot do its job — lens M13 and M71.
//
// Both findings have the same shape: a failure that kills the work while leaving the socket bound and
// accepting, so a client sees a live server that answers nothing and a restart's incumbent probe
// (`Server::bind` connects before it binds) is answered by it. Each row below measures the SOCKET's
// observable state, which is the part a client and a supervisor can see.
// =====================================================================================================
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::io::BufRead;

    fn socket(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ae-life-{tag}-{}.sock", std::process::id()))
    }

    fn config(path: &Path) -> ServerConfig {
        ServerConfig {
            socket_path: path.to_path_buf(),
            engine: EngineConfig {
                free_run_pace: None,
                ..EngineConfig::default()
            },
            ..ServerConfig::default()
        }
    }

    fn machine() -> Machine {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        Machine::new(sys)
    }

    /// Open a connection, send `initialize`, and report the first line back: `Some(line)`, or `None` when
    /// the server ended the connection (or said nothing for five seconds) instead of answering.
    fn first_reply(path: &Path) -> std::io::Result<Option<String>> {
        let mut s = UnixStream::connect(path)?;
        s.set_read_timeout(Some(Duration::from_secs(5)))?;
        s.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")?;
        let mut line = String::new();
        match std::io::BufReader::new(&s).read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(line)),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Whether `connect` to `path` is refused within `limit`, polled every 10 ms.
    fn stops_answering(path: &Path, limit: Duration) -> bool {
        let t = Instant::now();
        while t.elapsed() < limit {
            if UnixStream::connect(path).is_err() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// **Lens M13: a dead emulator thread must not leave a live-looking socket behind.**
    ///
    /// The emulator thread here dies on the first message it receives — a stand-in for any panic in the
    /// engine, injected through [`Server::serve`] so the row does not depend on finding one. Before the
    /// fix, the accept loop and the socket outlived it: every connection was accepted and then ended
    /// without a reply, `main` parked forever, and a restart's incumbent probe was answered, so the
    /// restart refused with `AddrInUse`. What has to be true instead, all measured on the socket:
    ///
    /// 1. the request the thread died on gets the end of the connection, not a hang;
    /// 2. the socket stops answering, promptly;
    /// 3. a restart can bind the same path (its probe finds a corpse);
    /// 4. [`ServerHandle::wait`] reports the death and its reason, which is what `main` exits on;
    /// 5. the dead server's handle, dropped after the restart, does not unlink the socket the restarted
    ///    server is serving on. That hazard is new with (3): before, nothing could rebind the path while
    ///    the old handle lived, so an unconditional unlink on drop could only ever remove its own file.
    #[test]
    fn a_dead_emulator_thread_releases_the_socket_and_reports_why() {
        let path = socket("dead-engine");
        let _ = std::fs::remove_file(&path);
        let server = Server::bind(config(&path)).expect("bind");
        let ctx = AcceptCtx::new(server.config.event_queue_cap);
        let mut dead = server.serve(ctx, |rx: mpsc::Receiver<EngineMsg>, _: &SharedStamp| {
            let _ = rx.recv();
            panic!("injected emulator-thread fault (lens M13 fixture)");
        });

        let reply = first_reply(&path).expect("the server accepts the first connection");
        assert_eq!(
            reply, None,
            "the request the emulator thread died on cannot have been answered"
        );

        assert!(
            stops_answering(&path, Duration::from_secs(5)),
            "lens M13: the emulator thread is dead and {} still accepts connections, so a client sees \
             a live server that answers nothing and a restart's probe calls it an incumbent",
            path.display()
        );

        let restarted = Server::bind(config(&path)).expect(
            "a restart must bind the dead server's path: its probe should find a corpse, not an incumbent",
        );

        let why = dead
            .wait()
            .expect_err("the emulator thread panicked, and wait() must say so");
        assert!(
            why.contains("injected emulator-thread fault"),
            "wait() must carry the reason, not only the fact: {why}"
        );

        let live = restarted.spawn(machine());
        drop(dead);
        assert!(
            UnixStream::connect(&path).is_ok(),
            "dropping the dead server's handle unlinked the socket the restarted server is serving on"
        );
        drop(live);
        assert!(
            !path.exists(),
            "and the live server still removes its own socket when it goes"
        );
    }

    /// **Lens M13, the half CI found: a dead server gives up its path at the moment it stops answering.**
    ///
    /// The row above checks step 5 (the dead handle's drop spares the restarted server's socket) by
    /// letting it happen. That check only bites where the restart's new socket gets the same inode number
    /// as the dead server's, because the old rule was "unlink at drop if the file's `(device, inode)` is the
    /// one this bind recorded". ext4 hands a freed inode number to the next file created in the directory,
    /// so on the CI runner the restart's socket *was* given the dead server's number, the check matched, and
    /// the drop deleted the live socket (CI red on `adcf239` and `85c1599`). tmpfs mounted `inode64` never
    /// reuses a number, so the same row passed on a workstation for the wrong reason.
    ///
    /// This row does not need the filesystem to reuse anything. It measures the claim directly: once the
    /// dead server has stopped answering, **there is no file at its path**. The first `connect` that fails
    /// finds nothing (`NotFound`), not a corpse that refuses (`ConnectionRefused`). A corpse is a file a
    /// restart has to replace while the dead server still claims the path, and that leftover claim is what
    /// an inode number reused by the filesystem turns against the live server.
    #[test]
    fn a_dead_server_gives_up_its_path_when_it_stops_answering() {
        let path = socket("dead-disowns");
        let _ = std::fs::remove_file(&path);
        let server = Server::bind(config(&path)).expect("bind");
        let ctx = AcceptCtx::new(server.config.event_queue_cap);
        let dead = server.serve(ctx, |rx: mpsc::Receiver<EngineMsg>, _: &SharedStamp| {
            let _ = rx.recv();
            panic!("injected emulator-thread fault (lens M13 disown fixture)");
        });
        assert_eq!(
            first_reply(&path).expect("the server accepts the first connection"),
            None,
            "the request the emulator thread died on cannot have been answered"
        );

        // The first connect that fails, and how it fails.
        let t = Instant::now();
        let refusal = loop {
            match UnixStream::connect(&path) {
                Ok(_) if t.elapsed() < Duration::from_secs(5) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(_) => panic!(
                    "lens M13: the emulator thread is dead and {} still accepts connections after 5 s",
                    path.display()
                ),
                Err(e) => break e,
            }
        };
        assert!(
            refusal.kind() == std::io::ErrorKind::NotFound && !path.exists(),
            "the dead server stopped answering but left its socket file at {} (connect: {refusal}, {:?}). \
             A server that no longer answers must no longer claim the path: a restart replaces the corpse, \
             and a claim still held on it is matched by (device, inode), which a filesystem that reuses \
             inode numbers (ext4, the CI runner's /tmp) gives to the restart's new socket. The claim then \
             deletes a live server's file (CI red on adcf239 and 85c1599).",
            path.display(),
            refusal.kind()
        );

        // No file at the path, so the restart's bind skips the incumbent probe and binds.
        let live = Server::bind(config(&path))
            .expect("a restart binds a path with no file at it")
            .spawn(machine());
        drop(dead);
        assert!(
            UnixStream::connect(&path).is_ok(),
            "dropping the dead server's handle touched the socket the restarted server is serving on"
        );
        drop(live);
        assert!(
            !path.exists(),
            "and the live server removes its own socket when it goes"
        );
    }

    /// **A server removes only the file it bound, on every filesystem.** Someone removes a live server's
    /// socket file and binds their own socket at the path. The server's listener is still open at that
    /// point and holds its inode, so the newcomer's file cannot be given the same inode number on any
    /// filesystem. The server's shutdown must then leave the newcomer's socket alone.
    ///
    /// This is the guard on the identity comparison itself. Replacing it with an unconditional unlink would
    /// pass every other row in this module.
    #[test]
    fn a_server_never_unlinks_a_socket_it_did_not_bind() {
        let path = socket("foreign");
        let _ = std::fs::remove_file(&path);
        let h = Server::bind(config(&path)).expect("bind").spawn(machine());
        std::fs::remove_file(&path).expect("take the live server's socket file away");
        let foreign =
            UnixListener::bind(&path).expect("bind a socket the server did not bind at its path");
        drop(h);
        let survived = path.exists() && UnixStream::connect(&path).is_ok();
        drop(foreign);
        let _ = std::fs::remove_file(&path);
        assert!(
            survived,
            "the server's shutdown unlinked {}, a socket bound there by someone else after the server's \
             own file was removed",
            path.display()
        );
    }

    /// **Lens M71: a zero-depth outbound queue is refused when the server is configured, not when each
    /// client connects.**
    ///
    /// `Outbound::new` asserted `capacity > 0`, and it runs on each connection's own thread
    /// (`connection_loop`), so `event_queue_cap = 0` bound the socket fine and then killed every
    /// connection at its first byte while the socket kept accepting: M13's shape again, from a
    /// configuration value. The refusal belongs where the value enters, `Server::bind`, which both
    /// deployments (the standalone server and `Host::serve`) pass through before any socket exists.
    ///
    /// On the old shape this row does not stop at "bind accepted it": it goes on to show what a client
    /// then saw, so its red run is the reproduction.
    #[test]
    fn a_zero_event_queue_cap_is_refused_at_bind_not_at_each_connection() {
        let path = socket("zero-cap");
        let _ = std::fs::remove_file(&path);
        let cfg = ServerConfig {
            event_queue_cap: 0,
            ..config(&path)
        };
        match Server::bind(cfg) {
            Ok(server) => {
                let h = server.spawn(machine());
                let reply = first_reply(&path);
                let still_accepting = UnixStream::connect(&path).is_ok();
                drop(h);
                panic!(
                    "lens M71: bind accepted event_queue_cap = 0. A client's first request then got \
                     {reply:?} (its connection thread died in Outbound::new) while the socket still \
                     accepted connections: {still_accepting}"
                );
            }
            Err(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{e}");
                assert!(
                    e.to_string().contains("event_queue_cap"),
                    "the refusal must name the setting a person has to change: {e}"
                );
                assert!(
                    !path.exists(),
                    "refused before anything touched the filesystem, so a bad value leaves no socket \
                     file behind"
                );
            }
        }
    }
}
