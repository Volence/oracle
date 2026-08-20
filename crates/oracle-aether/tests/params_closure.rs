//! **Request params are closed** — `protocol.md` §2.5 and §8 item 22, added 2026-08-19 by §11.17
//! (CR-27c), plus the `disp` param that ships beside it.
//!
//! The two answer different halves of one complaint, and the CR is emphatic that they are different
//! halves: a client wanting to write `Player_1`+2 had no way to *say* so, reached for a parameter name,
//! and was told **OK** twice while the server wrote `Player_1`+0 and answered with an address the client
//! had never asked for. Closing params ends the silence; `disp` gives the request somewhere to go.
//!
//! The file's spine is the **authority test**: every advertised method's declared key set is derived by
//! parsing the vendored schema and compared against `MethodSpec.params`. The fragment is normative for
//! wire shapes (D14), so a hand-maintained mirror of it is a count waiting to go wrong — §11.10's
//! founding defect was exactly that, and clause 7 of §11.17's adoption now re-derives rather than
//! transcribes. This is the same discipline one layer down.

mod common;

use common::{spawn_system, Client};
use oracle_aether::engine::METHODS;
use oracle_core::system::System;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// Load a one-symbol listing naming `Probe` at `$FF0600`, and hand back the temp dir to clean up.
fn load_probe(c: &mut Client, tag: &str) -> std::path::PathBuf {
    let lst = "  Symbol Table (* = unused):\n\n Probe : FF0600 C |\n\n   1 symbols\n";
    let dir = std::env::temp_dir().join(format!("oracle-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("probe.lst");
    std::fs::write(&path, lst).unwrap();
    c.ok(
        "emulator/load_symbols",
        json!({"path": path.to_str().unwrap()}),
    );
    dir
}

/// The vendored schema, read directly rather than through the harness — this test is *about* the
/// schema's key sets, so it wants the document, not a compiled validator.
fn schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/contract/bus-protocol.schema.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("read the vendored schema"))
        .expect("the vendored schema parses")
}

fn fragment_params(schema: &Value, method: &str) -> Option<BTreeSet<String>> {
    let p = schema["methods"].get(method)?.get("params")?;
    Some(
        p.get("properties")
            .and_then(Value::as_object)
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default(),
    )
}

// ---------------------------------------------------------------------------------------------------
// The authority tie
// ---------------------------------------------------------------------------------------------------

/// **The fragment is the wire authority; the table must equal it, by parse.**
///
/// Both directions are failures and both are checked: a key in a fragment but not in the table is a param
/// the contract publishes and this server refuses, and a key in the table but not in the fragment is a
/// param this server accepts and the contract does not know about. §8 item 22 makes the first a
/// conformance failure; the second is how an undocumented extension gets in.
#[test]
fn every_advertised_method_declares_exactly_its_fragments_params() {
    let schema = schema();
    let mut checked = 0;
    for m in METHODS {
        let Some(declared) = fragment_params(&schema, m.name) else {
            panic!(
                "{} is advertised but has no params fragment — §2.5 has nothing to close against, and \
                 §8 item 20 already forbids serving it",
                m.name
            );
        };
        let ours: BTreeSet<String> = m.params.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(
            ours, declared,
            "{}: the accepted key set and its fragment disagree",
            m.name
        );
        checked += 1;
    }
    assert_eq!(checked, METHODS.len());
    assert!(
        checked >= 37,
        "only {checked} methods — did the table shrink?"
    );
}

/// The closure is spelled **mechanically in the published schema**, which is what distinguishes it from
/// item 20's result closure (kept out of the artifact on purpose). Every params object must declare it —
/// including the three profiler fragments, which arrived from a sibling amendment branch that could not
/// have carried the keyword and were closed at the merge.
#[test]
fn every_params_object_in_the_vendored_schema_is_closed() {
    let schema = schema();
    let methods = schema["methods"].as_object().expect("methods object");
    let mut open = Vec::new();
    let mut seen = 0;
    for (name, frag) in methods {
        if name.starts_with('$') {
            continue;
        }
        seen += 1;
        let p = &frag["params"];
        if p["unevaluatedProperties"] != json!(false) && p["additionalProperties"] != json!(false) {
            open.push(name.clone());
        }
    }
    assert!(open.is_empty(), "params objects left open: {open:?}");
    assert!(seen >= 37, "only {seen} fragments — did the schema shrink?");

    // §11.17 adoption clause 7, our side of it: the top-level `description`'s fragment count is
    // **re-derived by parsing** and compared, never trusted as a literal. §11.10's founding defect was a
    // count wrong on both ends, and a hardcoded expectation here reproduces exactly that failure — it
    // agrees with whatever the last person typed. The floor above is a floor; this is the gate.
    let desc = schema["description"].as_str().expect("a description");
    let claimed = desc
        .split_whitespace()
        .zip(desc.split_whitespace().skip(1))
        .zip(desc.split_whitespace().skip(2))
        .find_map(|((a, b), c)| {
            (b == "of" && c.starts_with("§6")).then(|| a.parse::<usize>().ok())?
        })
        .expect("the description must state its fragment count as `N of §6's ... methods`");
    assert_eq!(
        claimed, seen,
        "the description claims {claimed} fragments and the document holds {seen} — a count is parsed \
         or it is wrong (§11.17 clause 7)"
    );
}

/// **The closure is at the TOP level of `params`, and only there** (§2.5: *"objects nested inside a
/// params payload are closed only where their own subschema closes them"*).
///
/// The guard this needs is not about nesting, though — it is about **applicators**. A key contributed
/// from inside an `if`/`then`, `allOf` or `oneOf` branch is a declared key that
/// `params.properties.keys()` does not see, so `MethodSpec.params` would be missing it and the server
/// would refuse a param the contract declares. Today exactly one fragment contributes a property inside
/// an applicator (`watchpoint_add`'s `mode` const) and it is *also* declared at the parent, which is why
/// the authority test's simple key-set compare is sound. This asserts that premise instead of relying on
/// it: any `properties` object appearing below the top level of a params object must contribute no key
/// the parent does not already declare.
#[test]
fn no_params_key_is_declared_only_inside_an_applicator() {
    fn collect(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::Object(o) => {
                if let Some(Value::Object(p)) = o.get("properties") {
                    out.extend(p.keys().cloned());
                }
                for (k, child) in o {
                    // `properties`' own children are property *schemas*, not more params keys.
                    if k != "properties" {
                        collect(child, out);
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|c| collect(c, out)),
            _ => {}
        }
    }

    let schema = schema();
    let mut checked = 0;
    for m in METHODS {
        let params = &schema["methods"][m.name]["params"];
        let top: BTreeSet<String> = params["properties"]
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        // Everything except the top-level `properties` object.
        let mut nested = BTreeSet::new();
        if let Value::Object(o) = params {
            for (k, child) in o {
                if k != "properties" {
                    collect(child, &mut nested);
                }
            }
        }
        let hidden: Vec<&String> = nested.difference(&top).collect();
        assert!(
            hidden.is_empty(),
            "{}: {hidden:?} are declared only inside an applicator, so the accepted key set derived \
             from `params.properties` would refuse a param the contract declares",
            m.name
        );
        checked += 1;
    }
    assert_eq!(checked, METHODS.len());
}

/// **`initialize` is exempt, and exempt structurally.** It is the handshake: handled before dispatch and
/// absent from `METHODS`, so the closure's own spelling cannot reach it. A client built against a later
/// revision must still be able to hand its capabilities to an earlier server, and closing the one step
/// whose job is surviving version skew would make skew fail there first.
#[test]
fn the_handshake_is_exempt_from_the_closure() {
    assert!(
        !METHODS.iter().any(|m| m.name == "initialize"),
        "initialize must not be in the dispatch table — its exemption is structural"
    );

    let h = spawn_system("pc-handshake", machine(), 64);
    let mut c = Client::connect(&h);
    // An unknown capability flag and an unknown top-level key both survive.
    let r = c.ok(
        "initialize",
        json!({
            "clientId": "test",
            "clientName": "aether-tests",
            "clientVersion": "0",
            "protocolVersion": 1,
            "clientCapabilities": {"events": false, "somethingFromNextYear": true},
            "aKeyThisServerHasNeverHeardOf": 42,
        }),
    );
    assert!(r["methods"].is_array(), "the handshake answered normally");
}

// ---------------------------------------------------------------------------------------------------
// The refusal
// ---------------------------------------------------------------------------------------------------

/// **The measured case, now refused.** `{symbol, offset: 2}` used to succeed, write `Player_1`+0 and
/// answer with an address the caller never asked for. The refusal must name the offending key **and**
/// carry it typed, because a client acting on *which* key was rejected needs a field rather than prose —
/// and because "invalid params" with no name would satisfy a weaker assertion and help nobody.
#[test]
fn the_probe_that_used_to_pass_silently_is_refused_by_name() {
    let h = spawn_system("pc-offset", machine(), 64);
    let mut c = client(&h);

    let e = c.err(
        "emulator/write_memory",
        json!({"addr": "0xFF0100", "offset": 2, "bytes": "0xAA"}),
    );
    assert_eq!(e["code"], json!(-32602));
    assert_eq!(
        e["data"]["unknownParams"],
        json!(["offset"]),
        "the key must arrive typed, not only in prose"
    );
    let msg = e["message"].as_str().unwrap();
    assert!(
        msg.contains("offset"),
        "the message must name the key: {msg}"
    );
    assert!(
        msg.contains("bytes") && msg.contains("symbol"),
        "the message must list what IS accepted, so the refusal is also the fix: {msg}"
    );
}

/// **The refusal precedes any effect.** A write refused for an unknown param has written nothing — and
/// the sentinel is what makes that a measurement rather than an assumption, since a cell left at its
/// reset value is indistinguishable from a cell a refused write happened to zero.
#[test]
fn a_write_refused_for_an_unknown_param_writes_nothing() {
    let h = spawn_system("pc-noeffect", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0400", "bytes": "0x5A5A5A5A"}),
    );

    let e = c.err(
        "emulator/write_memory",
        json!({"addr": "0xFF0400", "bytes": "0xDEADBEEF", "offset": 0}),
    );
    assert_eq!(e["code"], json!(-32602));
    let back = c.ok("emulator/read", json!({"addr": "0xFF0400", "len": 4}));
    assert_eq!(
        back["bytes"],
        json!("0x5A5A5A5A"),
        "the sentinel survived — the refusal came before the write"
    );

    // The same is true of the CRAM poke, whose effect is on a different device entirely.
    c.ok(
        "emulator/write_cram",
        json!({"line": 0, "index": 3, "raw": 0x0246}),
    );
    let e = c.err(
        "emulator/write_cram",
        json!({"line": 0, "index": 3, "raw": 0x0EEE, "immediate": true}),
    );
    assert_eq!(e["data"]["unknownParams"], json!(["immediate"]));
    let pal = c.ok("emulator/read_cram", json!({"line": 0}));
    assert_eq!(pal["palette"][3]["raw"], json!("0x0246"), "CRAM untouched");
}

/// The closure binds **bus-wide**, not just on the method that prompted it — that is what §8 item 22
/// puts on the checklist. Every advertised method must refuse a key none of them declares, and the
/// sweep is over the whole table rather than a sampled few.
#[test]
fn every_advertised_method_refuses_a_key_it_does_not_declare() {
    let h = spawn_system("pc-sweep", machine(), 1024);
    let mut c = client(&h);

    // A name no fragment declares, and none plausibly will.
    const BOGUS: &str = "notARealParamName";
    for m in METHODS {
        assert!(
            !m.params.contains(&BOGUS),
            "{} declares the probe key — pick another",
            m.name
        );
        let e = c.err(m.name, json!({ BOGUS: 1 }));
        assert_eq!(
            e["code"],
            json!(-32602),
            "{} accepted an undeclared key (got {})",
            m.name,
            e
        );
        assert_eq!(
            e["data"]["unknownParams"],
            json!([BOGUS]),
            "{}: the offending key must be named",
            m.name
        );
    }
}

/// **The two keys the closure's own sweep found the CATALOG blessing** (§11.17 postscript, 2026-08-20).
///
/// `emulator/reload_rom` declared `wait` and `reset` — bare undescribed booleans inherited from the
/// legacy catalog — and no server had ever read either; ours reads `path` and nothing else. That is the
/// silent-ignore §2.5 exists to end, wearing the schema's clothes instead of a handler's, and it was
/// invisible until the closure arrived: while every other key was tolerated, a key nobody read looked
/// exactly like a key nobody had sent yet.
///
/// Struck rather than implemented, because their meaning was never stated anywhere and inventing one to
/// justify a declaration is §8's invention ban pointed backwards. So the assertion is that they are now
/// ordinary unknown keys — including `reset: false`, which is the shape most likely to be "obviously
/// harmless" to a future reader and is exactly as unspecified as `reset: true`.
#[test]
fn reload_rom_refuses_the_two_struck_keys() {
    let h = spawn_system("pc-reload", machine(), 64);
    let mut c = client(&h);

    for bad in [
        json!({"reset": false}),
        json!({"reset": true}),
        json!({"wait": true}),
    ] {
        let key = bad.as_object().unwrap().keys().next().unwrap().clone();
        let e = c.err("emulator/reload_rom", bad.clone());
        assert_eq!(e["code"], json!(-32602), "reload_rom accepted {bad}");
        assert_eq!(e["data"]["unknownParams"], json!([key]));
    }
    // Both at once, and the surviving key alongside them — the refusal is about the struck pair, not
    // about `path` having company.
    let e = c.err(
        "emulator/reload_rom",
        json!({"path": "/nonexistent", "reset": true, "wait": false}),
    );
    assert_eq!(e["data"]["unknownParams"], json!(["reset", "wait"]));

    // And the fragment really has stopped declaring them — the table check above would pass if BOTH
    // sides had kept them, so the schema is asserted directly.
    let declared = fragment_params(&schema(), "emulator/reload_rom").expect("a fragment");
    assert_eq!(declared, ["path".to_string()].into_iter().collect());
}

/// Several unknown keys are reported together. A client that guessed two names should learn both in one
/// round trip, not discover them one refusal at a time.
#[test]
fn every_unknown_key_is_reported_not_just_the_first() {
    let h = spawn_system("pc-multi", machine(), 64);
    let mut c = client(&h);
    let e = c.err(
        "emulator/read_cram",
        json!({"line": 0, "zeta": 1, "alpha": 2}),
    );
    assert_eq!(e["data"]["unknownParams"], json!(["alpha", "zeta"]));
}

/// The closure must not eat the params that DO exist: every known spelling still works. Without this the
/// suite would pass with a handler that refused everything.
#[test]
fn the_declared_params_all_still_work() {
    let h = spawn_system("pc-known", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0500", "bytes": "0x01"}),
    );
    c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0500", "value": 1, "width": 1}),
    );
    c.ok("emulator/read", json!({"addr": "0xFF0500", "len": 1}));
    c.ok("emulator/read_cram", json!({"line": 1}));
    c.ok("emulator/read_cram", json!({}));
    c.ok(
        "emulator/write_cram",
        json!({"line": 1, "index": 1, "r": 1, "g": 2, "b": 3}),
    );
    c.ok("emulator/run_frames", json!({"frames": 1}));
}

// ---------------------------------------------------------------------------------------------------
// `disp` — the other half
// ---------------------------------------------------------------------------------------------------

/// **`disp` round-trips.** Write at `symbol`+2, read back at the resolved address +2, and the reply
/// echoes the **displaced** address — the whole point, since answering with the symbol's own address is
/// the misexecution this replaces.
#[test]
fn disp_writes_at_the_displaced_address_and_the_reply_says_so() {
    let h = spawn_system("pc-disp", machine(), 64);
    let mut c = client(&h);
    let dir = load_probe(&mut c, "disp");

    // The base, so the displaced write is visibly NOT at the symbol.
    c.ok(
        "emulator/write_memory",
        json!({"symbol": "Probe", "bytes": "0x1111"}),
    );
    let r = c.ok(
        "emulator/write_memory",
        json!({"symbol": "Probe", "disp": 2, "bytes": "0x2222"}),
    );
    assert_eq!(
        r["addr"],
        json!("0x00FF0602"),
        "the reply echoes the DISPLACED address"
    );

    let back = c.ok("emulator/read", json!({"addr": "0xFF0600", "len": 4}));
    assert_eq!(
        back["bytes"],
        json!("0x11112222"),
        "the base write and the displaced write landed two bytes apart"
    );

    // `disp: 0` is the identity, not a special case.
    let zero = c.ok(
        "emulator/write_memory",
        json!({"symbol": "Probe", "disp": 0, "bytes": "0x33"}),
    );
    assert_eq!(zero["addr"], json!("0x00FF0600"));

    // And `symbolDisp` on the read side is the value that hands back in: D7's round trip, made literal.
    let read = c.ok(
        "emulator/read_memory",
        json!({"addr": "0xFF0602", "len": 1}),
    );
    assert_eq!(read["symbol"], json!("Probe"));
    assert_eq!(
        read["symbolDisp"],
        json!(2),
        "{{symbol, symbolDisp}} out..."
    );
    let again = c.ok(
        "emulator/write_memory",
        json!({"symbol": "Probe", "disp": 2, "bytes": "0x44"}),
    );
    assert_eq!(again["addr"], json!("0x00FF0602"), "...{{symbol, disp}} in");

    let _ = std::fs::remove_dir_all(&dir);
}

/// `disp` is valid **only** with `symbol`: with `addr` it is arithmetic the caller has already done, and
/// it must be non-negative because it mirrors a displacement from the nearest *preceding* symbol.
#[test]
fn disp_is_refused_with_addr_and_when_negative() {
    let h = spawn_system("pc-disp-bad", machine(), 64);
    let mut c = client(&h);

    let e = c.err(
        "emulator/write_memory",
        json!({"addr": "0xFF0700", "disp": 2, "bytes": "0xAA"}),
    );
    assert_eq!(e["code"], json!(-32602));
    assert!(
        e["message"].as_str().unwrap().contains("disp"),
        "the refusal must name the key: {}",
        e["message"]
    );
    // Not an unknown-param refusal — `disp` IS declared here; it is the pairing that is wrong.
    assert!(e["data"]["unknownParams"].is_null());

    // Negative, via a symbol that really resolves — otherwise `-32013 symbol not found` refuses first
    // and the assertion would be about symbol resolution rather than about `disp`.
    let dir = load_probe(&mut c, "disp-neg");
    let e = c.err(
        "emulator/write_memory",
        json!({"symbol": "Probe", "disp": -2, "bytes": "0xAA"}),
    );
    assert_eq!(e["code"], json!(-32602));
    assert!(
        e["message"].as_str().unwrap().contains("non-negative"),
        "the refusal must say why, not just that: {}",
        e["message"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The displaced address is the one that must land in the work-RAM window** (adoption gate 4, clause
/// 3). `disp` is not a way to reach past the window a bare `addr` cannot reach — the bound is applied
/// after resolution, not before it.
///
/// Two different refusals, and they are different on purpose: a displacement that leaves the writable
/// window is `-32004` (the address is real, it is just not writable), while one that leaves the 24-bit
/// bus entirely is also `-32004` but must report the **sum** it is complaining about rather than the
/// base — a `data.addr` naming a different number than the sentence beside it is a join a client cannot
/// make.
#[test]
fn a_displaced_target_outside_the_work_ram_window_is_refused() {
    let h = spawn_system("pc-disp-window", machine(), 64);
    let mut c = client(&h);
    let dir = load_probe(&mut c, "disp-window");

    // `Probe` is at $FF0600; the window ends at $FFFFFF. A displacement past the end leaves it.
    let past = 0x00FF_FFFF - 0x00FF_0600 + 1;
    let e = c.err(
        "emulator/write_memory",
        json!({"symbol": "Probe", "disp": past, "bytes": "0xAA"}),
    );
    assert_eq!(
        e["code"],
        json!(-32004),
        "the DISPLACED address is bounds-checked"
    );
    assert_eq!(
        e["data"]["addr"],
        json!("0x01000000"),
        "and the refusal names the displaced address, not the symbol's own"
    );

    // Far enough to leave the 24-bit bus: the sum is reported, un-truncated.
    let e = c.err(
        "emulator/write_memory",
        json!({"symbol": "Probe", "disp": 0xFFFF_FFFFu64, "bytes": "0xAA"}),
    );
    assert_eq!(e["code"], json!(-32004));
    assert_eq!(
        e["data"]["addr"],
        json!("0x100FF05FF"),
        "the sum the message complains about, not a truncation of it"
    );
    assert!(e["message"].as_str().unwrap().contains("0x100FF05FF"));

    // The control: one byte short of the edge still works, so the bound is a bound and not a blanket.
    let ok = c.ok(
        "emulator/write_memory",
        json!({"symbol": "Probe", "disp": past - 1, "bytes": "0xAA"}),
    );
    assert_eq!(ok["addr"], json!("0x00FFFFFF"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// `disp` is `write_memory`'s alone. The read side does not declare it, so it is an **unknown param**
/// there — which is the closure and the scoping decision agreeing with each other.
#[test]
fn disp_is_not_a_bus_wide_param() {
    let h = spawn_system("pc-disp-scope", machine(), 64);
    let mut c = client(&h);
    for method in ["emulator/read", "emulator/read_memory"] {
        let e = c.err(method, json!({"addr": "0xFF0800", "len": 1, "disp": 2}));
        assert_eq!(e["data"]["unknownParams"], json!(["disp"]), "{method}");
    }
}
