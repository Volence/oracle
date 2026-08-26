//! **§8 item 23 — every advertised method dispatches** (contract `protocol.md`, added 2026-08-26 by
//! §11.23).
//!
//! *"A name present in `initialize`'s `methods` MUST be dispatchable: a server MUST NOT answer `-32601`
//! for a name it advertised … Correspondingly, a name absent from `methods` MUST answer `-32601` if
//! called. `methods` is therefore a **warranty**, not an advertisement, and a client MAY treat membership
//! as sufficient to decide servedness **without issuing a call**."*
//!
//! This server satisfies it structurally — `engine::METHODS` **is** the dispatch table, and the
//! advertised list is built by walking it — so this file is a guard rather than a fix, and being a guard
//! is the point: the property holds by construction only until someone builds the list a second way,
//! which is the exact defect D4 retired `list_ops` over (it was advertising 34 of 47 before anyone
//! noticed).
//!
//! It sits in a file of its own rather than inside `handshake.rs`'s list test, because the two ask
//! different questions. That one asserts the advertised list **equals** `METHODS` — which a server could
//! satisfy while answering `-32601` for half of them. This one calls every name.

mod common;

use common::spawn;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------------------------------
// §8 item 23 — every advertised method dispatches
// ---------------------------------------------------------------------------------------------------

/// One NDJSON connection that validates replies **against the envelope only**.
///
/// `common::Client` picks the `methods.<name>.result` fragment for every reply it reads, and that is
/// exactly right for every other test in this repo. It is wrong *here*, and the reason is a live one
/// rather than a hypothetical: on 2026-08-26 the re-vendor brought §11.24, which gave
/// `emulator/step_over` a required `pc` this server does not yet emit — and an item-23 sweep built on
/// `Client` promptly died on `step_over`'s **result shape**, a clause item 23 has nothing to do with.
/// An item-23 regression test that goes red whenever any *other* conformance item is outstanding
/// reports the wrong thing, and would be read as "dispatch is broken".
///
/// So this reads the envelope — `$defs/replyFields`, the JSON-RPC frame, the stamp — through the same
/// validator (`check_incoming(line, None)`), and asserts on `error.code`. Nothing is skipped: the result
/// shapes are checked by every other file in this suite, and by `schema_conformance` against the
/// fragments themselves.
struct EnvelopeOnlyClient {
    reader: std::io::BufReader<std::os::unix::net::UnixStream>,
    writer: std::os::unix::net::UnixStream,
    next_id: i64,
}

impl EnvelopeOnlyClient {
    fn connect(h: &oracle_aether::server::ServerHandle) -> Self {
        use std::os::unix::net::UnixStream;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match UnixStream::connect(h.socket_path()) {
                Ok(s) => {
                    s.set_read_timeout(Some(std::time::Duration::from_secs(20)))
                        .unwrap();
                    return Self {
                        reader: std::io::BufReader::new(s.try_clone().unwrap()),
                        writer: s,
                        next_id: 1,
                    };
                }
                Err(e) if std::time::Instant::now() < deadline => {
                    let _ = e;
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(e) => panic!("connect: {e}"),
            }
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        use std::io::{BufRead, Write};
        let id = self.next_id;
        self.next_id += 1;
        let line = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
        self.writer.write_all(line.as_bytes()).unwrap();
        self.writer.write_all(b"\n").unwrap();
        self.writer.flush().unwrap();
        loop {
            let mut buf = String::new();
            let n = self.reader.read_line(&mut buf).expect("read");
            assert!(n > 0, "connection closed while a reply was expected");
            let v: Value = serde_json::from_str(&buf).expect("bad JSON on the wire");
            // The envelope IS still checked — "cannot check" must not look like "checked".
            if let Err(f) = common::schema::check_incoming(&v, None) {
                panic!("envelope violation on the wire for {method}: {f:#?}\n  line: {buf}");
            }
            if v.get("id").is_some_and(|i| !i.is_null()) {
                assert_eq!(v["id"], json!(id), "response id must correlate");
                return v;
            }
        }
    }
}

#[test]
fn every_advertised_method_dispatches_and_every_unadvertised_one_does_not() {
    // §8 item 23 (added 2026-08-26 by §11.23), a **MUST** in both directions:
    //   * a name present in `methods` MUST be dispatchable — a server MUST NOT answer -32601 for it;
    //   * a name absent from `methods` MUST answer -32601 if called.
    //
    // Item 23 governs NAME RESOLUTION ONLY and does not require the call to succeed: a handler that
    // dispatches and then refuses on its own domain terms (-32005 `machineRunning`, a bound exceeded, no
    // symbol table) "is conformant and is answering truthfully". So the assertion is on the code, and
    // deliberately not on `error`-vs-`result` — nor on the result's shape; see `EnvelopeOnlyClient`.
    let h = spawn("item23");
    let mut c = EnvelopeOnlyClient::connect(&h);
    let r = c.call(
        "initialize",
        json!({
            "clientId": "test", "clientName": "aether-tests", "clientVersion": "0",
            "protocolVersion": 1, "clientCapabilities": {"events": true},
        }),
    );
    let advertised: Vec<String> = r["result"]["methods"]
        .as_array()
        .expect("methods is an array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        !advertised.is_empty(),
        "the handshake advertised no methods, so the sweep below would prove nothing"
    );

    let mut undispatched = Vec::new();
    for name in &advertised {
        let v: Value = c.call(name, json!({}));
        if let Some(e) = v.get("error") {
            if e["code"] == json!(-32601) {
                undispatched.push(format!("{name}: {}", e["message"]));
            }
        }
    }
    assert!(
        undispatched.is_empty(),
        "§8 item 23: `methods` is a WARRANTY, not an advertisement — a client MAY treat membership as \
         sufficient to decide servedness without issuing a call. These names were advertised and \
         answered -32601, which the item calls a server defect:\n  {}",
        undispatched.join("\n  ")
    );

    // The other direction, which the item states as a MUST of its own. The name is built rather than
    // borrowed so it cannot accidentally be a real row, and it is asserted absent before it is called.
    let absent = "emulator/__no_such_method_item23__";
    assert!(!advertised.contains(&absent.to_string()));
    let v: Value = c.call(absent, json!({}));
    assert_eq!(
        v["error"]["code"],
        json!(-32601),
        "§8 item 23: a name absent from `methods` MUST answer -32601 if called — otherwise the cheap \
         pre-check the item exists to make sound is not sound.\n  got: {v}"
    );
}
