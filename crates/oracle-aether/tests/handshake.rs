//! Conformance of the envelope, the handshake and the transport itself against
//! `empyrean/contract/protocol.md` §2, §2.1, §5, §7.1.

mod common;

use common::{spawn, spawn_for_sweep, Client};
use oracle_aether::engine::{EVENTS, METHODS};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;

/// **The most frames one swept row may advance the machine.**
///
/// **It is a literal, and deliberately NOT `common::SWEEP_FRAME_BUDGET as i64`.** That spelling was
/// written and thrown away, because it makes the check vacuous in the one direction that matters: the
/// regression this row guards is somebody handing the sweep a default 3,600-frame server again, and a
/// ceiling derived from the budget rises with it — `emulator/step_out` goes back to 600 frames and the
/// assertion still passes. Verified by mutation rather than reasoned about: with the derived ceiling
/// the budget mutation came back **green in 6.62 s**, which is a vacuous check wearing the runtime of
/// the defect it failed to catch. A ratchet may not be adjustable by the thing it ratchets.
///
/// The number is 1 because 1 is the **measured** maximum across all ~60 rows, not because 1 happens to
/// be the budget today; there is no slack in it. Two candidates for a wider row were checked and
/// neither reaches here: `emulator/step_out` is clamped by `run_step` to `max_run_frames.min(600)`, and
/// `emulator/press` never advances at all in a sweep, because `{}` fails `parse_buttons` first. The
/// second one used to carry an open observation — `press`' `frames` default of 2 was read *past* the
/// `frame_cap` it computes from `max_run_frames`, so a bare press really could outrun a one-frame
/// server. That is now closed at the source (the default is `frame_cap.min(2)`) and gated by
/// `methods::a_bare_press_is_bounded_by_the_run_ceiling`, which measures the frames the machine
/// advanced rather than the ones the reply claims. Neither fact is something this row proves.
const SWEEP_ROW_FRAME_CEILING: i64 = 1;

/// The `frame` on a reply the caller has already unwrapped to its `result`.
fn frame_of(result: &Value) -> i64 {
    result["frame"].as_u64().expect("a stamped reply") as i64
}

#[test]
fn socket_is_mode_0600_per_d8() {
    let h = spawn("mode");
    let mode = std::fs::metadata(h.socket_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "protocol.md D8: the socket is created mode 0600"
    );
}

#[test]
fn initialize_advertises_a_generated_method_list_that_is_the_dispatch_table() {
    let h = spawn_for_sweep("init");
    let mut c = Client::connect(&h);
    let r = c.handshake(true);

    assert_eq!(r["protocolVersion"], json!(1));
    // **This line used to read `assert_eq!(r["serverName"], json!("oracle-next"))`, and §2.1 now bars
    // exactly that.** Since the 2026-08-26 amendment (§11.23): *"`serverName` remains a **deployment**
    // label a config may set — two processes of the same implementation on one machine want
    // distinguishable names — and MUST NOT be used to discriminate implementations."* `ServerConfig`
    // really can set it, so the old pin was asserting a default and calling it an identity; it would have
    // gone red on a rename that changed nothing about who was answering, and stayed green on a legacy
    // server configured to answer to the same string. The identity key is `implementation`, which no
    // config path reaches (`tests/server_build.rs` proves that at source level).
    assert_eq!(
        r["implementation"],
        json!("oracle-rs"),
        "§2.1 (§11.23): the registry value for the Rust `oracle-aether` server"
    );
    // `serverName` is still REQUIRED, so it is still checked — for presence, which is all a deployment
    // label warrants. (Its default is the pre-rename `"oracle-next"`; that is a display-string question,
    // and deliberately not this test's.)
    assert!(
        r["serverName"].is_string(),
        "§2.1: `serverName` is REQUIRED — as a deployment label, not an identity"
    );

    let advertised: Vec<String> = r["methods"]
        .as_array()
        .expect("methods is an array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let implemented: Vec<String> = METHODS.iter().map(|m| m.name.to_string()).collect();
    assert_eq!(
        advertised, implemented,
        "D4: the advertised list IS the dispatch table — there is no second list to drift"
    );

    // And the list is not merely equal to a constant: every advertised name must actually dispatch.
    // A name that is advertised but unwired would come back -32601.
    //
    // **The sweep is also held to a cost, and that half is `F-HANDSHAKE-LOAD-TIMEOUT`.** The row failed
    // with a bare socket-read `WouldBlock` under load and was reachable no other way; the cause was that
    // `emulator/step_out` has no frame to return out of on this fixture and therefore ran the engine's
    // entire 600-frame step budget — 6.68 s of the sweep's 7.44 s — inside one 20-second read. A wiring
    // probe must not hold a read deadline open running emulation, so the ceiling is asserted per row
    // rather than left to whatever the default budget happens to be. It is stated in **frames**, not in
    // wall clock, precisely because wall clock is the thing load moves: a machine at load average 65
    // makes 600 frames take longer, it does not make them fewer.
    //
    // Two things keep the number honest. The server is spawned with `SWEEP_FRAME_BUDGET`, so the run
    // ceiling is the engine's own config rather than a branch this test switched on. And the machine is
    // put back to paused whenever a row leaves it running — `emulator/resume` is in the table, and a
    // free-running unpaced engine taxes every one of the ~48 rows after it a whole frame each while
    // making the count depend on wall-clock scheduling. The re-pause is the harness's own call, not a
    // swept row, so its advance is deliberately not asserted.
    let mut frame = frame_of(&r);
    for name in &advertised {
        let v = c.call(name, common::sweep_params(name));
        if let Some(e) = v.get("error") {
            assert_ne!(
                e["code"],
                json!(-32601),
                "{name} is advertised but not wired"
            );
        }
        let stamp = common::reply_stamp(&v);
        let after = stamp["frame"]
            .as_u64()
            .expect("every reply carries `frame`") as i64;
        assert!(
            after - frame <= SWEEP_ROW_FRAME_CEILING,
            "{name} advanced the machine {} frames in one sweep row; the ceiling is \
             {SWEEP_ROW_FRAME_CEILING} (this sweep server was spawned with a frame budget of {}). A \
             wiring probe that runs emulation inside a socket read is F-HANDSHAKE-LOAD-TIMEOUT, where \
             `emulator/step_out` took 600 frames and blew the {:?} read deadline under load.",
            after - frame,
            common::SWEEP_FRAME_BUDGET,
            common::READ_TIMEOUT,
        );
        frame = after;
        if stamp["running"]
            .as_bool()
            .expect("every reply carries `running`")
        {
            frame = frame_of(&c.ok("emulator/pause", json!({})));
        }
    }

    // capabilities.events is the authoritative event set (D6).
    let events: Vec<String> = r["capabilities"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        events,
        EVENTS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn method_summaries_are_derived_from_the_same_registry_and_their_key_set_equals_methods() {
    // **§2.1's derivation clause, registered 2026-08-15 (§11.5) — and this test pins a property this
    // server already had rather than fixing a bug.** Both lists come from the one `METHODS` table
    // (`engine::initialize` walks it once, emitting a name into `methods` and a `name -> summary` pair
    // into `methodSummaries`), so equality is structural here and cannot drift while that stays true.
    //
    // It is pinned anyway because of what §2.1 says the clause is *for*: D4 retired `list_ops` because a
    // second hand-maintained inventory drifts from the first, and `list_ops` was advertising 34 of 47
    // before anyone noticed. `methodSummaries` is admissible only as long as it is not a second
    // inventory. The way this server would stop obeying that is not a wrong entry today but somebody
    // adding a method and giving it a bespoke summary somewhere else — which is precisely the edit this
    // assertion turns red. A structural property with a test is a property; without one it is a habit.
    let h = spawn("summaries");
    let mut c = Client::connect(&h);
    let r = c.handshake(true);

    let methods: BTreeSet<&str> = r["methods"]
        .as_array()
        .expect("methods is an array")
        .iter()
        .map(|v| v.as_str().expect("a method name is a string"))
        .collect();
    let summaries = r["methodSummaries"]
        .as_object()
        .expect("methodSummaries is an object");
    let summarised: BTreeSet<&str> = summaries.keys().map(String::as_str).collect();

    assert_eq!(
        summarised,
        methods,
        "§2.1 rule 2: methodSummaries' key set MUST equal `methods`, exactly — no extra key, no \
         missing one. Extra: {:?}; missing: {:?}",
        summarised.difference(&methods).collect::<Vec<_>>(),
        methods.difference(&summarised).collect::<Vec<_>>(),
    );
    // Rule 3 makes the values non-normative, so nothing *general* is asserted about their wording —
    // but non-normative is not the same as free to lie, and one summary is checked against its own
    // reply by `a_summary_that_names_a_format_must_name_the_format_the_reply_returns` below. Empty is
    // a different matter: a key with no summary is a key that failed to derive.
    for (name, summary) in summaries {
        let s = summary.as_str().expect("a summary is a string");
        assert!(!s.trim().is_empty(), "{name} has an empty summary");
    }
    // And the derivation is checked at its source, not only at its output — this is the half that would
    // still hold if the handshake ever grew a second producer.
    assert_eq!(
        methods,
        METHODS.iter().map(|m| m.name).collect::<BTreeSet<&str>>(),
        "the advertised set must be the dispatch table (D4)"
    );
}

/// **A summary that names a file format must name the format the handler actually emits.**
///
/// This exists because it already went wrong: `emulator/screenshot` advertised "render the active
/// display to a binary PPM file" for six days after the handler switched to PNG, and the change was
/// recorded only in a code comment five lines above the encoder — the one place nobody re-reads. The
/// wrong string shipped over the wire in every `initialize`, and it was a downstream consumer, not
/// this suite, that noticed: their capture harness had been silently writing nothing.
///
/// The assertion is deliberately *relational* rather than a ban on the word "PPM". It reads the
/// format out of the handler's own reply and requires the advertised summary to name that value, so
/// it stays live if the encoder is swapped again for something neither word covers. Nothing here is
/// copied from the source or from a pin — both sides of the comparison come off the wire.
///
/// Scope, stated so the guard is not mistaken for a general one: `emulator/screenshot` is the only
/// method that returns a `format` today, so this checks one row. If another method grows a `format`,
/// add it to `cases` — the loop is already shaped for it.
///
/// Two of the rows below exist because planting a violation cannot find every way a test goes green:
/// the empty-`format` stop closes a comparison that would have passed VACUOUSLY (the schema permits
/// `""`, and `contains("")` is always true), and `names_token` makes the match measure "the summary
/// names this format" rather than "these letters occur somewhere in it".
#[test]
fn a_summary_that_names_a_format_must_name_the_format_the_reply_returns() {
    // (method, params to invoke it with). Extend as more methods report a `format`.
    let cases = [(
        "emulator/screenshot",
        json!({"path": std::env::temp_dir()
            .join(format!("ae-summary-fmt-{}.png", std::process::id()))
            .display()
            .to_string()}),
    )];

    // Every image container this project could plausibly emit or has emitted. Used only for the
    // second, weaker assertion below; it is a heuristic list, not a derived one, and it is the one
    // part of this test that a genuinely novel format would need updating for.
    const IMAGE_FORMATS: &[&str] = &[
        "png", "ppm", "pgm", "pbm", "pnm", "bmp", "gif", "tga", "tiff", "webp", "jpeg", "jpg",
        "qoi",
    ];

    let h = spawn("summary-format");
    let mut c = Client::connect(&h);
    let init = c.handshake(false);

    for (method, params) in &cases {
        let summary = init["methodSummaries"][method]
            .as_str()
            .unwrap_or_else(|| {
                panic!("no advertised summary for {method} — cannot check it against anything")
            })
            .to_ascii_lowercase();

        let reply = c.ok(method, params.clone());
        // Loud on unmeasurable: a reply with no `format` is not a pass, it is a test that lost its
        // subject and must say so. Verified to be belt-and-braces rather than the live path — with
        // `format` deleted from the handler, `Client::ok`'s contract-schema validation fires first
        // ("`format` is a required property"), so the schema is what actually holds the key's
        // presence. This branch is what would catch it if the schema ever made `format` optional.
        let actual = reply["format"].as_str().unwrap_or_else(|| {
            panic!(
                "{method} returned no string `format`, so this guard has nothing to compare the \
                 advertised summary against; reply was {reply}"
            )
        });
        let actual = actual.to_ascii_lowercase();

        // **The vacuity stop, and it is not theoretical.** The contract schema types `format` as an
        // unconstrained `"type": "string"` with no const, enum or minLength, so `""` is a
        // schema-VALID reply — and `summary.contains("")` is unconditionally true. Without this
        // line the primary assertion below would report GREEN for a server that had stopped naming
        // its format at all, which is a worse failure than the one this test was written for.
        // Planting a wrong format could never have surfaced that; only asking what else could make
        // the row pass could.
        assert!(
            !actual.trim().is_empty(),
            "{method} reported an empty `format`, so there is no format name for the advertised \
             summary to be checked against — the comparison below would pass vacuously. An empty \
             format is a defect in the handler, not a pass here.",
        );

        assert!(
            names_token(&summary, &actual),
            "advertised summary for {method} does not name the format its own reply reports: the \
             reply says format={actual:?} but the summary reads {summary:?}. The summary ships to \
             every client in initialize.methodSummaries, so a summary that disagrees with the \
             reply misinforms every client. Fix the summary in engine::METHODS.",
        );

        // Weaker, heuristic half: a summary that names the right format AND a stale one is still a
        // summary a reader can act on wrongly. This is the shape a half-finished edit leaves behind.
        let strays: Vec<&&str> = IMAGE_FORMATS
            .iter()
            .filter(|f| **f != actual && names_token(&summary, f))
            .collect();
        assert!(
            strays.is_empty(),
            "advertised summary for {method} names image format(s) {strays:?} that its reply does \
             not emit (reply says format={actual:?}); a one-line summary should name exactly the \
             one format the handler writes. Summary reads {summary:?}.",
        );

        if let Some(p) = reply["path"].as_str() {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Does `haystack` name `token` as a WORD, rather than merely contain its letters?
///
/// A bare `contains` measures the wrong quantity: a three-letter format token is short enough to
/// fall inside an ordinary English word by accident, and the accident reads as a pass. "raw" sits
/// inside "drawn"; "tga" would sit inside a hypothetical "tgap". The summary this guard protects
/// already contains the word "display", and a future token "spl" or "isp" would match it. Requiring
/// non-alphanumeric neighbours makes the assertion measure "the summary names this format" instead
/// of "these letters occur somewhere", which is the property the rule is actually about.
///
/// Both callers use it, so the stray-format half tightens in the same direction: a summary saying
/// "compressed" no longer counts as naming "pgm"-like tokens by coincidence.
fn names_token(haystack: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    haystack.match_indices(token).any(|(i, _)| {
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let end = i + token.len();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        before_ok && after_ok
    })
}

#[test]
fn an_unknown_method_is_method_not_found() {
    let h = spawn("unknown");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let e = c.err("emulator/does_not_exist", json!({}));
    assert_eq!(e["code"], json!(-32601));
}

#[test]
fn list_ops_is_gone() {
    // protocol.md §6: "~~list_ops~~ — removed — replaced by initialize's methods".
    let h = spawn("listops");
    let mut c = Client::connect(&h);
    c.handshake(false);
    assert_eq!(c.err("list_ops", json!({}))["code"], json!(-32601));
    assert!(!METHODS.iter().any(|m| m.name.contains("list_ops")));
}

/// `F-TRACE-PAL`: the `frame` in every stamp is only meaningful with the basis that produced it, so the
/// server advertises it once — label *and* numbers, machine-readable, never prose. The numbers must be the
/// core's own, so a client can divide `mclk` by them and land on the `frame` the server reports.
#[test]
fn initialize_advertises_the_timing_basis_with_its_numbers() {
    let h = spawn("basis");
    let mut c = Client::connect(&h);
    let r = c.handshake(true);

    let basis = &r["timingBasis"];
    assert_eq!(basis["standard"], json!("ntsc"));
    assert_eq!(
        basis["mclkPerFrame"],
        json!(oracle_core::system::MCLK_PER_FRAME),
        "the advertised frame length is the core's own constant"
    );
    assert_eq!(basis["linesPerFrame"], json!(262));

    // Not a decoration: it is the divisor the server's own stamps are computed with.
    let mclk = r["mclk"].as_u64().expect("stamped reply");
    let frame = r["frame"].as_u64().expect("stamped reply");
    assert_eq!(frame, mclk / basis["mclkPerFrame"].as_u64().unwrap());
}

#[test]
fn an_incompatible_protocol_version_is_32015_and_names_what_we_support() {
    let h = spawn("ver");
    let mut c = Client::connect(&h);
    let e = c.err("initialize", json!({"protocolVersion": 99}));
    assert_eq!(e["code"], json!(-32015));
    assert_eq!(e["data"]["supported"], json!([1]));
    assert_eq!(e["data"]["requested"], json!(99));
}

#[test]
fn a_method_before_initialize_is_refused_on_the_wire() {
    let h = spawn("gate");
    let mut c = Client::connect(&h);
    let e = c.err("emulator/status", json!({}));
    assert_eq!(e["code"], json!(-32600));
    assert_eq!(e["data"]["expected"], json!("initialize"));
}

#[test]
fn invalid_json_is_32700_with_a_null_id() {
    let h = spawn("parse");
    let mut c = Client::connect(&h);
    c.send_raw("{this is not json");
    let v = c.recv();
    assert_eq!(v["error"]["code"], json!(-32700));
    assert_eq!(v["id"], json!(null));
}

#[test]
fn batches_are_refused_with_32600() {
    let h = spawn("batch");
    let mut c = Client::connect(&h);
    c.send_raw(r#"[{"jsonrpc":"2.0","id":1,"method":"emulator/status"}]"#);
    let v = c.recv();
    assert_eq!(v["error"]["code"], json!(-32600));
}

#[test]
fn a_notification_is_never_answered() {
    let h = spawn("notif");
    let mut c = Client::connect(&h);
    c.handshake(false);
    // A notification (no id) that would otherwise error must produce no line at all...
    c.send_raw(r#"{"jsonrpc":"2.0","method":"emulator/no_such_thing"}"#);
    // ...so the next line read must be the answer to the request that follows it.
    let v = c.call("emulator/status", json!({}));
    assert!(v.get("result").is_some(), "got {v}");
}

#[test]
fn an_over_long_line_is_refused_without_desyncing_the_connection() {
    let h = spawn("long");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let huge = format!(
        r#"{{"jsonrpc":"2.0","id":9,"method":"emulator/status","params":{{"pad":"{}"}}}}"#,
        "A".repeat(2 * 1024 * 1024)
    );
    c.send_raw(&huge);
    let v = c.recv();
    assert_eq!(v["error"]["code"], json!(-32600));
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("line limit"));
    // The framing survived: the next request still works.
    assert!(c.call("emulator/status", json!({})).get("result").is_some());
}

#[test]
fn ndjson_framing_holds_no_reply_ever_contains_a_raw_newline() {
    let h = spawn("frame");
    let mut c = Client::connect(&h);
    c.handshake(true);
    // A path that puts free text (a file path with an embedded newline) into a reply.
    let e = c.err(
        "emulator/load_symbols",
        json!({"path": "/nonexistent\npath"}),
    );
    assert_eq!(e["code"], json!(-32602));
    // Reaching here at all proves the framing held: `recv` reads exactly one line and parses it, so a
    // raw newline in the payload would have produced a JSON parse failure above.
    assert!(c.call("emulator/status", json!({})).get("result").is_some());
}

#[test]
fn two_clients_share_one_machine() {
    let h = spawn("multi");
    let mut a = Client::connect(&h);
    let mut b = Client::connect(&h);
    a.handshake(false);
    b.handshake(false);
    let before = b.ok("emulator/status", json!({}))["frame"]
        .as_u64()
        .unwrap();
    a.ok("emulator/run_frames", json!({"frames": 3}));
    let after = b.ok("emulator/status", json!({}))["frame"]
        .as_u64()
        .unwrap();
    assert_eq!(after, before + 3, "both connections see one machine");
}

/// **A blown read deadline must name the request it was waiting for.**
///
/// This is a gate on the *harness*, and it earns its place because the harness's failure message is
/// evidence. `F-HANDSHAKE-LOAD-TIMEOUT` was booked as "a socket read timeout under heavy load, cause
/// unknown" — and the cause was legible from the wire the whole time, in the one word the message did
/// not carry: the outstanding method. The same bare `WouldBlock` let the `wait_for_break` row be closed
/// as a flake twice before it was root-caused to a real ordering defect. So "the timeout says what it
/// was waiting for" is a property with a test, not a habit.
///
/// The blown deadline is manufactured rather than waited for, and the two numbers are chosen so that
/// **load can only ever make this test more true**. `emulator/step_out` has no frame to return out of on
/// this fixture, so it runs the server's whole step budget; the budget is set to 120 frames, which is
/// ~1.3 s of debug-build emulation here, and only then is the deadline dropped to 100 ms. That is a 13x
/// margin, and it is one-sided: a busy machine makes 120 frames slower, never fewer. The handshake in
/// front of it runs at the ordinary `READ_TIMEOUT`, so the row that exists to make a load-sensitive
/// failure legible cannot itself become one.
#[test]
fn a_read_that_times_out_names_the_request_it_was_waiting_for() {
    let h =
        common::spawn_with_frame_budget("timeout-msg", oracle_core::testrom::build(), 1024, 120);
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.set_read_timeout(std::time::Duration::from_millis(100));

    // The panic is the subject, so it is caught rather than propagated — and its payload is read, which
    // is the only way to assert on a message rather than on the mere fact of a failure.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        c.call("emulator/step_out", json!({}));
    }));
    std::panic::set_hook(hook);

    let err = outcome.expect_err(
        "emulator/step_out ran its whole 120-frame budget in under 100 ms, so this test observed no \
         deadline at all and proves nothing about the message a blown one carries",
    );
    let msg = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .expect("a panic payload the test can read")
        .to_string();

    assert!(
        msg.contains("emulator/step_out"),
        "a blown read deadline must name the outstanding request — that name is the diagnosis. \
         Got: {msg}"
    );
    assert!(
        msg.contains("100ms"),
        "a blown read deadline must quote the deadline it blew, so slowness can be told from a hang \
         without re-deriving the number. Got: {msg}"
    );
    // And it must commit to one of the two diagnoses rather than reporting an errno that fits both.
    assert!(
        msg.contains("LATE, not never") || msg.contains("nor within a further"),
        "a blown read deadline must say whether the server answered late or never answered — the two \
         are opposite defects and `WouldBlock` fires identically for both. Got: {msg}"
    );
}
