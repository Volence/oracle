//! A minimal Aether client, used by the integration tests. Hand-rolled on purpose: the tests must
//! exercise the *wire* (NDJSON framing, envelope shape, handshake ordering), so anything that shares
//! code with the server would test less than it appears to.

#![allow(dead_code)]

use oracle_aether::engine::EngineConfig;
use oracle_aether::server::{Machine, Server, ServerConfig, ServerHandle};
use oracle_core::system::System;
use serde_json::{json, Value};
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

    pub fn send_raw(&mut self, line: &str) {
        self.writer.write_all(line.as_bytes()).unwrap();
        self.writer.write_all(b"\n").unwrap();
        self.writer.flush().unwrap();
    }

    /// Read one NDJSON line as JSON. Panics on EOF or timeout — a hung transport is a test failure.
    pub fn recv(&mut self) -> Value {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).expect("read");
        assert!(n > 0, "connection closed while a reply was expected");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad JSON on the wire: {e}: {line}"))
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
