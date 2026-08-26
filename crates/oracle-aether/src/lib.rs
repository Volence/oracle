//! `oracle-aether` — the **Aether control surface** for `oracle-core`.
//!
//! JSON-RPC 2.0 carried as NDJSON over an `AF_UNIX` socket, implementing
//! `empyrean/contract/protocol.md`. That document is normative and says so: *"This document is the
//! source of truth; the emulator conforms to it, not the reverse."* Nothing here is a design; where the
//! contract could not be followed as written, the deviation is filed as a change request in
//! `docs/2026-08-14-aether-change-requests.md`, never taken silently (§8).
//!
//! # Why this is not in `oracle-core`
//!
//! The core's charter is *deterministic, no-I/O* with a one-crate dependency list (`bincode`). A socket
//! server is threads, filesystem and JSON — all three live here instead, and the core's dependency set
//! is unchanged (`cargo tree -p oracle-core`). The core does not know this crate exists.
//!
//! # The four properties this transport is built around
//!
//! 1. **Every reply carries `{frame, mclk, running}`** — success, error, and every pushed event. Both
//!    clocks are *emulated*, never wall-clock. Without this an agent stitching four reads into one
//!    conclusion may be reading four different machine states with no way to detect it. Structural, not
//!    per-handler: [`rpc::stamp_result`] merges the stamp after the handler returns and overwrites any
//!    key of the same name, so a handler cannot omit or shadow it.
//! 2. **Every array is bounded and flags truncation** ([`rpc::bounded_array`]). No unbounded dumps.
//!    Out-of-range bounds are a loud refusal, never a silent clamp. *Cursored* used to be part of this
//!    sentence and contract §2.4 clause (b) took it out: continuation belongs to the methods that accept
//!    a cursor param, and a token the client can never hand back is worse than none.
//! 3. **Anything approximate carries a `caveat` string** in the payload — a nearest-preceding symbol
//!    match, a debug read that bypasses the bus, a whole-frame render that is not scanline-accurate, a
//!    symbol listing that could not be bound to the ROM, a run that ended on its bound instead of its
//!    condition.
//! 4. **A slow or dead client cannot wedge the emulator.** The emulator thread never writes to a socket;
//!    it broadcasts into per-connection bounded queues that drop oldest-first and count the loss. See
//!    [`outbound`] for the incident that makes this a requirement rather than polish.
//!
//! # Two arrangements, one engine
//!
//! The same [`engine`] serves the bus in two shapes, and which one is right is decided by **who owns the run
//! loop**:
//!
//! * [`server`] — the bus owns the machine, on a thread of its own. The headless arrangement; the shape
//!   below.
//! * [`host`] — **something else owns the machine and its loop** (the player process, with a window, an
//!   audio device and a keyboard), and drains the bus once per iteration. There cannot be two things
//!   advancing one `System`, so the capability layer becomes a thing the owner pumps rather than a thread
//!   that runs.
//!
//! Neither is a wrapper around the other: they share the accept loop, the connection threads and the engine
//! verbatim, and differ only in where the `System` lives.
//!
//! # Shape
//!
//! ```no_run
//! use oracle_aether::server::{Machine, Server, ServerConfig};
//! use oracle_core::system::System;
//!
//! let mut sys = System::new(0x5EED);
//! sys.load_rom(std::fs::read("game.bin")?);
//! sys.reset();
//! let server = Server::bind(ServerConfig::default())?;
//! let handle = server.spawn(Machine::new(sys));
//! # Ok::<(), std::io::Error>(())
//! ```

#![forbid(unsafe_code)]
#![cfg(unix)]

// Public, not `pub(crate)`, for the reason `oracle_core::state_hash` exports its primitives: the tests
// derive their expectations by running the algorithm themselves, and a hash a test cannot recompute
// independently is a hash only its own implementation vouches for.
pub mod build_info;
pub mod crc32;
pub mod engine;
pub mod hex;
pub mod host;
pub mod outbound;
pub mod png;
pub mod rpc;
pub mod server;
pub mod session;
