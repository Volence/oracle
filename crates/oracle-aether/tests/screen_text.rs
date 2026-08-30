//! **`emulator/screen_text` on a server that has no window** — contract §11.29 (CR-H).
//!
//! The headless half of the method, and it is the half that can be checked end to end here: a standalone
//! `oracle-aether` has no frontend in the process at all, so `-32005` / `noDisplay` is not a stub answer —
//! it is the only true one. The populated half needs a host pushing a snapshot and lives in `hosted.rs`,
//! beside the rest of the player-shaped tests.
//!
//! Every line these tests receive is validated against the vendored contract schema by
//! [`common::Client::recv`], so the refusal's envelope is checked against the fragment as a side effect of
//! reading it.

#![cfg(unix)]

mod common;

use common::{spawn, Client};
use serde_json::json;

/// **The refusal, in full.** Not merely "it failed": the code, the machine-readable `reason`, and the
/// absence of any `surfaces` key — because the whole argument for refusing is that an empty list would be
/// indistinguishable from a blank screen, and a refusal that smuggled one back would give that away.
#[test]
fn a_server_with_no_window_refuses_rather_than_serving_an_empty_screen() {
    let h = spawn("screentext-refuse");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let e = c.err("emulator/screen_text", json!({}));
    assert_eq!(
        e["code"],
        json!(-32005),
        "§5: a good request in the wrong state is INVALID_STATE, not INVALID_REQUEST: {e}"
    );
    assert_eq!(
        e["data"]["reason"],
        json!("noDisplay"),
        "§5 requires a machine-readable discriminant, and §11.29 pins this one's spelling. Clients \
         branch on `reason` everywhere else on this bus; a boolean flag here would be a second \
         convention: {e}"
    );
    assert!(
        e["data"].get("surfaces").is_none(),
        "a refusal must not carry a list at all — an empty one is exactly what §11.29 forbids: {e}"
    );
    // The message is for a human and is not pinned, but it must exist and must name the condition rather
    // than restating the code.
    let msg = e["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("window"),
        "the message should say what is missing, not just that something is: {msg:?}"
    );
}

/// **The rider (§11.29): `emulator/status.display` lets a caller ask instead of probing by failing.**
///
/// The two are asserted together on purpose. Either one alone could go green while the pair disagreed —
/// `display: true` beside a `noDisplay` refusal is precisely the "ask" being wrong, and it is the failure
/// a caller would trust before it trusted the error.
#[test]
fn status_says_there_is_no_display_and_agrees_with_the_refusal() {
    let h = spawn("screentext-status");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let st = c.ok("emulator/status", json!({}));
    assert_eq!(
        st["display"],
        json!(false),
        "a headless server serves false — and serves the key, so that absent and false are not the \
         same artifact: {st}"
    );
    // Present, not merely falsey: `json!(null) == json!(false)` is false, but `st["display"]` on a missing
    // key yields Null, and a reader skimming the assertion above could mistake one for the other. This is
    // the row that makes the absence loud.
    assert!(
        st.get("display").is_some_and(|v| v.is_boolean()),
        "`display` must be a real boolean on the wire, not absent: {st}"
    );

    let e = c.err("emulator/screen_text", json!({}));
    assert_eq!(
        e["data"]["reason"],
        json!("noDisplay"),
        "the ask and the probe must give the same answer: {st} vs {e}"
    );
}

// ------------------------------------------------------------------ the vectors, RUN

/// **The CR-H wire vectors, run against the vendored fragment by the suite's own validator.**
///
/// This exists because "these conform" is not evidence. The bar was earned the hard way in this lane: a
/// previous submission shipped eleven hand-built cases of which nine could not have validated, and nobody
/// found out until someone ran them. So the artifact handed to the hub is checked **by the same
/// `check_incoming_strict` every reply in this suite goes through**, on every run, and a vector that stops
/// matching its own `expect` turns this red rather than being noticed at the hub.
///
/// `expect: "fail"` rows are the red-first half. Each is a document a well-meaning implementer would
/// actually produce, and the fragment must refuse it — a schema that accepts everything is the vacuous
/// assertion wearing a validator's clothes.
///
/// The failure list is **printed**, so the run's output is the evidence rather than the exit code.
#[test]
fn the_cr_h_vectors_validate_the_way_the_file_says_they_do() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/proposed/2026-08-30-cr-h-vectors.json"
    );
    let doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(path).expect("read the CR-H vectors beside this crate"),
    )
    .expect("the vectors file parses");
    let cases = doc["cases"].as_array().expect("`cases` is an array");
    // Not `> 0`: an empty file and a file that failed to load are the same observation, and both would
    // sail past a loop that never iterates. The count is pinned to what the file is documented to hold.
    assert_eq!(
        cases.len(),
        7,
        "the vectors file holds 7 cases; if that changed, change this number deliberately"
    );

    let mut passes = 0;
    let mut fails = 0;
    println!("--- CR-H vectors against the vendored contract fragment ---");
    for (i, c) in cases.iter().enumerate() {
        let method = c["method"].as_str().expect("each case names its method");
        let expect = c["expect"].as_str().expect("each case declares expect");
        let kind = c["kind"].as_str().expect("each case declares its kind");
        // Wrap the body in the envelope the validator checks, exactly as it arrives on the wire.
        let line = match kind {
            "result" => json!({"jsonrpc": "2.0", "id": 1, "result": c["doc"].clone()}),
            "error" => json!({"jsonrpc": "2.0", "id": 1, "error": c["doc"].clone()}),
            other => panic!("case {i}: unknown kind {other:?}"),
        };
        let verdict = common::schema::check_incoming_strict(&line, Some(method));
        match (expect, &verdict) {
            ("pass", Ok(())) => {
                passes += 1;
                println!("  case {i} [{kind}] expect=pass  -> PASS");
            }
            ("fail", Err(errs)) => {
                fails += 1;
                println!(
                    "  case {i} [{kind}] expect=fail  -> REFUSED: {}",
                    errs.join(" | ")
                );
            }
            ("pass", Err(errs)) => panic!(
                "case {i} is declared passing and the schema REFUSED it: {}\n{}",
                errs.join(" | "),
                serde_json::to_string_pretty(&line).unwrap()
            ),
            ("fail", Ok(())) => panic!(
                "case {i} is declared failing and the schema ACCEPTED it — the fragment does not \
                 constrain what this case claims it does:\n{}",
                serde_json::to_string_pretty(&line).unwrap()
            ),
            (e, _) => panic!("case {i}: expect must be \"pass\" or \"fail\", got {e:?}"),
        }
    }
    println!(
        "  => {passes} accepted, {fails} refused, {} total",
        cases.len()
    );
    // Both halves must be exercised. A file of nothing but passing cases proves the fragment accepts;
    // only the refusals prove it rejects, and this repo has shipped a validator that did neither.
    assert!(
        passes > 0 && fails > 0,
        "the vectors must exercise both directions: {passes} pass, {fails} fail"
    );
}
