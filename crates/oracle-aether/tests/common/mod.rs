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
}

impl Client {
    pub fn connect(handle: &ServerHandle) -> Self {
        // The accept loop polls, so a connect immediately after spawn may beat it by a few ms.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match UnixStream::connect(handle.socket_path()) {
                Ok(s) => {
                    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
                    return Self {
                        reader: BufReader::new(s.try_clone().unwrap()),
                        writer: s,
                        next_id: 1,
                        pending: HashMap::new(),
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
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).expect("read");
        assert!(n > 0, "connection closed while a reply was expected");
        let v: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("bad JSON on the wire: {e}: {line}"));
        let method = v
            .get("id")
            .filter(|i| !i.is_null())
            .and_then(|i| self.pending.get(&i.to_string()))
            .cloned();
        schema::assert_incoming(&v, method.as_deref());
        v
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
