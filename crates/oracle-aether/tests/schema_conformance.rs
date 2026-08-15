//! **The schema harness's own tests** — contract §8 item 15 (D14).
//!
//! Four jobs, and only the first is the obvious one:
//!
//! 1. **Freshness.** The vendored schema is byte-identical to the contract's copy.
//! 2. **Coverage, reported and pinned.** The schema used to be a SEED, covering 9 of the 21 methods we
//!    advertise; since the 2026-08-15 re-vendor (`empyrean` `f309cc8`) it covers all 21. The pin stays,
//!    now guarding the one direction that is left: a newly advertised method joining the unchecked pile
//!    silently.
//! 3. **The divergence registry, reported and kept live.** Shapes where the server and the schema
//!    disagree are registered rather than silenced, each with its CR number, and the report prints
//!    beside the coverage split so nobody reads a green suite as "fully conformant". An entry that stops
//!    firing fails `every_registered_divergence_is_still_live`, so the list cannot rot after a ruling —
//!    which is exactly how the first two entries it held came to be deleted. It holds CR-16 today, found
//!    by turning item 20's closure on for the first time.
//! 4. **Anti-vacuity.** Proof that the validator *rejects*. This repo has twice shipped an assertion that
//!    passed while testing nothing — a volatility test that was a name grep, an assertion that passed with
//!    zero enqueues. A validator that accepts everything is exactly that failure wearing a library's
//!    clothes, and it would be invisible: the suite would be green and the wire unchecked. Since §8 item
//!    20 landed, this carries the closure's own control too: the working keyword is proven to accept a
//!    conformant reply and catch a surplus key, and the obvious-but-wrong one is proven to do neither.

mod common;

use common::schema::{
    check_incoming, check_incoming_strict, divergence_report, schemas, KNOWN_CONTRACT_DIVERGENCES,
};
use oracle_aether::engine::METHODS;
use serde_json::{json, Value};
use std::path::PathBuf;

// ---------------------------------------------------------------------------------------------------
// 1. Freshness
// ---------------------------------------------------------------------------------------------------

/// Where the upstream contract schema might live. `$AETHER_CONTRACT_SCHEMA` wins; otherwise walk up from
/// this crate and probe each ancestor for `empyrean/contract/schema/…`, which finds it both from a normal
/// checkout and from a `.claude/worktrees/…` worktree, whose depth differs.
fn upstream_schema_path() -> Result<PathBuf, Vec<PathBuf>> {
    if let Ok(p) = std::env::var("AETHER_CONTRACT_SCHEMA") {
        let p = PathBuf::from(p);
        return if p.is_file() { Ok(p) } else { Err(vec![p]) };
    }
    let mut tried = Vec::new();
    let mut dir: Option<&std::path::Path> = Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let cand = d.join("empyrean/contract/schema/bus-protocol.schema.json");
        if cand.is_file() {
            return Ok(cand);
        }
        tried.push(cand);
        dir = d.parent();
    }
    Err(tried)
}

#[test]
fn the_vendored_schema_is_byte_identical_to_the_upstream_contract() {
    // WHY a vendored copy at all: the tests compile against a fixed schema, so the suite is hermetic and
    // reproducible. WHY this test: a hermetic copy is a copy that can rot silently. Byte-comparing it
    // against the contract makes a contract edit turn this suite red, which forces an explicit re-vendor
    // commit — and that commit is the auditable record of "we adopted contract revision X".
    let upstream = match upstream_schema_path() {
        Ok(p) => p,
        Err(tried) => {
            // NOT a silent skip. A missing `vendor` symlink once made whole conformance rows skip
            // unnoticed in this repo; the lesson was that "cannot check" must never look like "checked".
            // So the default is a loud failure naming every path tried, with a documented escape hatch
            // for a checkout that genuinely has no sibling contract repo.
            let msg = format!(
                "CANNOT VERIFY the vendored contract schema is fresh: no upstream copy found.\n\
                 Tried (in order):\n  {}\n\
                 Point AETHER_CONTRACT_SCHEMA at empyrean/contract/schema/bus-protocol.schema.json, \
                 or set AETHER_CONTRACT_OPTIONAL=1 to downgrade this to a warning.\n\
                 This does NOT pass silently on purpose (contract §8 item 15): a freshness check that \
                 cannot run must say so, or a stale vendored schema validates every message against \
                 last week's contract and the suite stays green.",
                tried
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n  ")
            );
            if std::env::var("AETHER_CONTRACT_OPTIONAL").is_ok() {
                eprintln!("WARNING: {msg}");
                return;
            }
            panic!("{msg}");
        }
    };

    let up = std::fs::read(&upstream).expect("read the upstream contract schema");
    let vendored = common::schema::VENDORED_SCHEMA.as_bytes();
    assert_eq!(
        up.len(),
        vendored.len(),
        "the vendored schema has drifted from {} (length {} vs {}). \
         Re-vendor it and update crates/oracle-aether/tests/contract/PROVENANCE.md — that commit is \
         the record of adopting the new contract revision.",
        upstream.display(),
        vendored.len(),
        up.len()
    );
    assert!(
        up == vendored,
        "the vendored schema is the same length as {} but differs byte-for-byte. Re-vendor it and \
         update crates/oracle-aether/tests/contract/PROVENANCE.md.",
        upstream.display()
    );
}

// ---------------------------------------------------------------------------------------------------
// 2. Coverage — reported, and pinned so it cannot shrink
// ---------------------------------------------------------------------------------------------------

/// The methods we advertise that the SEED schema gives **no** `result` schema for.
///
/// Pinned deliberately. Two ways this list can be wrong, and the test catches both:
///
/// * a **newly advertised** method would join it silently, so a new op would arrive on the wire with
///   nothing checking its result shape and nothing saying so;
/// * a **newly schematized** method would leave it, and that has to be a deliberate edit here rather
///   than a number quietly improving.
///
/// The list did **not** move when `emulator/pixel_attribution` was implemented, and that is the
/// mechanism working rather than a gap: CR-10 put the fragment in the contract *first*, so the method
/// arrived already schematized and the covered count went 8 → 9 while this list stayed at 12.
///
/// **It is now empty, and it emptied the right way round.** Writing the 12 missing fragments was
/// explicitly not this harness's job — writing schemas from what this server emits would encode the
/// implementation as the contract, the exact inversion of "the contract leads" (§8). So they were raised
/// as CR-13, the contract ruled every key on its merits (`empyrean` `f309cc8`, `protocol.md` §11.5:
/// registered, restructured, or **struck**), and the 12 fragments arrived upstream. Coverage went 9 → 21
/// because the contract moved first, which is the only direction that counts.
///
/// The pin stays, and it now guards the one remaining direction: a **newly advertised** method would join
/// this list silently, arriving on the wire with nothing checking its result shape and nothing saying so.
/// An empty expectation makes that failure loud on the first run.
const UNCOVERED_METHODS: &[&str] = &[];

#[test]
fn the_schema_covers_every_method_we_advertise_and_the_uncovered_list_is_pinned_empty() {
    let advertised: Vec<&str> = METHODS.iter().map(|m| m.name).collect();
    let schematized = schemas().methods_with_result();

    let mut covered: Vec<&str> = advertised
        .iter()
        .copied()
        .filter(|m| schematized.contains(m))
        .collect();
    let mut uncovered: Vec<&str> = advertised
        .iter()
        .copied()
        .filter(|m| !schematized.contains(m))
        .collect();
    covered.sort_unstable();
    uncovered.sort_unstable();

    // Schematized but not advertised: harmless in that direction — D4 makes the advertised list
    // authoritative — but worth printing, because it is the shape of a method we might be missing.
    let schema_only: Vec<&str> = schematized
        .iter()
        .copied()
        .filter(|m| !advertised.contains(m))
        .collect();

    println!("--- Aether wire-schema coverage (contract §8 item 15) ---");
    println!(
        "advertised methods: {}   result schema present: {}   absent: {}",
        advertised.len(),
        covered.len(),
        uncovered.len()
    );
    println!("  COVERED   ({}): {}", covered.len(), covered.join(", "));
    println!(
        "  UNCOVERED ({}): {}",
        uncovered.len(),
        uncovered.join(", ")
    );
    println!(
        "  schematized but not advertised ({}): {}",
        schema_only.len(),
        schema_only.join(", ")
    );
    println!(
        "  events with a params schema ({}): {}",
        schemas().events_with_params().len(),
        schemas().events_with_params().join(", ")
    );
    println!(
        "  => envelope coverage 100% of lines; result coverage {}/{} methods. \
         Every line is checked against `anyMessage`; a reply to an UNCOVERED method would get the \
         envelope and nothing more — there are {} of those.",
        covered.len(),
        advertised.len(),
        uncovered.len()
    );

    // The registry prints HERE, beside the coverage split, and not in a test of its own — because the
    // failure mode being guarded against is someone reading a green suite as "fully conformant". A
    // number that says what is checked has to sit next to the list of what is checked *differently*.
    println!(
        "  KNOWN CONTRACT DIVERGENCES ({}) — registered, ruling-pending, NOT conformant:",
        KNOWN_CONTRACT_DIVERGENCES.len()
    );
    for line in divergence_report() {
        println!("    {line}");
    }
    println!(
        "  => this server is NOT fully schema-conformant. A green suite means \"no UNREGISTERED \
         divergences\", which is a weaker and more useful claim — and since §8 item 20 landed it is a \
         much sharper one: every result is closed against its fragment, so an unknown key is a red \
         test rather than a shape nobody sampled."
    );

    let mut expected_uncovered = UNCOVERED_METHODS.to_vec();
    expected_uncovered.sort_unstable();
    assert_eq!(
        uncovered, expected_uncovered,
        "the set of methods with no `result` schema changed.\n\
         If a method was ADDED to `engine::METHODS`, it joined the unchecked pile — add it to \
         UNCOVERED_METHODS deliberately, or (better) get a fragment into the contract schema first.\n\
         If a schema fragment was COMPLETED, remove that method from UNCOVERED_METHODS in the same \
         commit that re-vendors the schema."
    );
    assert_eq!(
        covered.len() + uncovered.len(),
        advertised.len(),
        "every advertised method is in exactly one bucket"
    );
}

// ---------------------------------------------------------------------------------------------------
// 3. The divergence registry — kept live, so it cannot rot after a ruling
// ---------------------------------------------------------------------------------------------------

#[test]
fn every_registered_divergence_is_still_live() {
    // **The anti-rot property, and the reason the registry is a list of canonical MESSAGES rather than a
    // list of method names.** Each entry claims that the schema, as vendored, rejects a specific shape
    // this server puts on the wire. This test checks the claim both ways round:
    //
    //   * the schema must STILL reject it with no allowances — so when a CR is ruled on and the schema
    //     re-vendored, this goes red and the entry must be deleted in the same commit;
    //   * the harness must ACCEPT it with allowances — so the entry is actually doing the job it exists
    //     for, and is not a dead line of documentation next to an allowance that fires somewhere else.
    //
    // Liveness is keyed to the schema rather than to observed traffic on purpose. Counting live firings
    // would only see the messages the binary that owns the counter happened to produce — precisely the
    // sampling weakness that let CR-14 through a 33-message probe of a live server.
    //
    // The non-emptiness assertion that used to stand here is gone, deliberately: its own comment asked
    // for exactly that ("if the last divergence was ruled on and removed, delete this assertion"), CR-14's
    // ruling is the event it was written for, and a registry that is empty for a day should not have to
    // fake an entry to keep a test honest. The anti-vacuity job it was doing has moved somewhere it holds
    // whether the list is empty or not: `the_strict_closure_rejects_a_surplus_key_and_needs_the_…`
    // proves the validator still bites. (The list is not empty today — CR-16 landed the same afternoon.)
    for d in KNOWN_CONTRACT_DIVERGENCES {
        let (line, method) = (d.canonical)();

        let strict = check_incoming_strict(&line, method);
        assert!(
            strict.is_err(),
            "{} ({} {}) NO LONGER DIVERGES: the vendored schema now accepts the shape this entry was \
             registered for. That is good news — a ruling landed — but the entry is now a lie. Delete \
             it from KNOWN_CONTRACT_DIVERGENCES, delete its allowance in common::schema, and close the \
             CR in docs/2026-08-14-aether-change-requests.md.\n  canonical: {line}",
            d.cr,
            d.method,
            d.path
        );

        // A weak locator, and labelled as one rather than dressed up. For a divergence the schema
        // reports at a key (`/otherMatches`) this really does check the entry describes the right bug.
        // For one that falls out at the root `oneOf` (CR-15) the failure text embeds the whole instance,
        // so the key name is present either way and this proves little. The load-bearing assertions are
        // the two either side of it; this one only catches an entry whose path is outright unrelated.
        // An entry may name several keys (`"$.region, $.symbolDisp, $.caveat"`), and EVERY one of them
        // must show up — a divergence that listed three keys and tripped over one would otherwise pass
        // while two-thirds of it went unproven.
        let failures = strict.unwrap_err();
        for key in d.path.split(',').map(|k| k.trim().trim_start_matches("$.")) {
            assert!(
                failures.iter().any(|f| f.contains(key)),
                "{} ({} {}) diverges, but `{key}` appears nowhere in the failure — the entry may be \
                 describing one bug while its canonical message trips over another.\n  failures: {failures:#?}",
                d.cr,
                d.method,
                d.path
            );
        }

        check_incoming(&line, method).unwrap_or_else(|e| {
            panic!(
                "{} ({} {}) is registered but its allowance does not actually admit its own canonical \
                 message — the entry and the allowance have drifted apart.\n  failures: {e:#?}",
                d.cr, d.method, d.path
            )
        });
    }
}

#[test]
fn every_registered_divergence_names_a_real_change_request() {
    // A CR number that does not exist in the ledger makes the registry unauditable: the whole promise is
    // "registered, not silenced", and an entry pointing at nothing is silenced with extra steps.
    let ledger = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/2026-08-14-aether-change-requests.md"),
    )
    .expect("the change-request ledger must be readable from the crate");
    for d in KNOWN_CONTRACT_DIVERGENCES {
        assert!(
            ledger.contains(&format!("## {} —", d.cr)),
            "{} is registered in KNOWN_CONTRACT_DIVERGENCES but has no `## {} —` section in \
             docs/2026-08-14-aether-change-requests.md",
            d.cr,
            d.cr
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// 4. Anti-vacuity controls — proof the validator rejects
// ---------------------------------------------------------------------------------------------------

/// Assert the harness rejects `line`, and that some failure message mentions `needle`.
///
/// Naming the field is the point: an error that says only "invalid" would not distinguish a validator
/// that found the planted defect from one that rejects everything, and the positive control below is
/// what rules the latter out.
fn rejects(line: &Value, method: Option<&str>, needle: &str) -> Vec<String> {
    match check_incoming(line, method) {
        Ok(()) => panic!(
            "THE VALIDATOR ACCEPTED A MESSAGE IT MUST REJECT (expected a failure mentioning \
             `{needle}`). A validator that accepts everything is a vacuous assertion: the suite would \
             be green and the wire unchecked.\n  line: {line}"
        ),
        Err(failures) => {
            assert!(
                failures.iter().any(|f| f.contains(needle)),
                "rejected, but no failure mentions `{needle}` — the validator may be failing for an \
                 unrelated reason, which would make this control prove nothing.\n  failures: {failures:#?}"
            );
            failures
        }
    }
}

/// A well-formed `emulator/read_memory` reply, used as the base for the planted defects below.
fn good_read_memory_reply() -> Value {
    // `region` is REQUIRED as of the CR-16 re-vendor (`empyrean` `d45dc87`, §11.6) — §6's row lists it
    // un-parenthesised, and the server has always emitted it. This fixture omitted it and so stopped
    // being conformant the moment the fragment declared it, which is the positive control doing its job
    // on itself: a "conformant reply" fixture that drifts from the contract silently weakens every
    // rejection control underneath it.
    json!({"jsonrpc":"2.0","id":7,"result":{
        "addr":"0x00FFA144","len":4,"bytes":"0x02600000","symbol":"Camera_X","region":"work RAM",
        "frame":601,"mclk":538008040,"running":false,"droppedEvents":0}})
}

#[test]
fn positive_control_a_conformant_message_is_accepted() {
    // Without this, every control below is satisfied by a validator that rejects unconditionally.
    check_incoming(&good_read_memory_reply(), Some("emulator/read_memory"))
        .expect("a conformant reply must pass");
    check_incoming(
        &json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
            "reason":"runFrames","pc":"0x00012A4C","frames":1,"deadlineReached":true,
            "frame":1,"mclk":896040,"running":false}}),
        None,
    )
    .expect("a conformant event must pass");
}

#[test]
fn control_a_reply_without_droppedevents_is_rejected() {
    // §2.3 / D17 / §8 item 18: present even at zero — "0 is the answer, not the absence of one".
    let mut line = good_read_memory_reply();
    line["result"]
        .as_object_mut()
        .unwrap()
        .remove("droppedEvents");
    rejects(&line, Some("emulator/read_memory"), "droppedEvents");
}

#[test]
fn control_a_numeric_checkpoint_id_is_rejected() {
    // A REGRESSION GUARD, not a synthetic one: this exact shape was live in this tree until 2026-08-15,
    // and it is the one thing the probe found the live wire actually violating (F1). §8 item 16 / D9
    // category 4: an opaque handle is a JSON string.
    //
    // Note this control also proves the *keying* works. The envelope alone accepts it — `replyFields`
    // says nothing about a key called `id` inside `result` — so it can only be caught by having reached
    // for `methods["emulator/checkpoint"].result`, which means `recv` correctly attributed the reply to
    // the request that asked for it.
    let line = json!({"jsonrpc":"2.0","id":3,"result":{
        "id":1,"bytes":262144,"frame":10,"mclk":8960400,"running":false,"droppedEvents":0}});
    check_incoming(&line, None).expect("the envelope alone accepts it — that is the point");
    rejects(&line, Some("emulator/checkpoint"), "id");
}

#[test]
fn control_a_stopped_event_with_an_unknown_reason_is_rejected() {
    // §3's reason enum is closed. A server may not widen it unilaterally (§8).
    let line = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"frameAdvance","pc":"0x00012A4C","frame":1,"mclk":896040,"running":false}});
    rejects(&line, None, "reason");
}

#[test]
fn control_an_envelope_with_both_result_and_error_is_rejected() {
    // §2: success is signalled by the presence of `result`; an error response carries `error`. Both at
    // once tells a client two different things about one call.
    let line = json!({"jsonrpc":"2.0","id":9,
        "result":{"frame":1,"mclk":2,"running":false,"droppedEvents":0},
        "error":{"code":-32603,"message":"internal","data":{"frame":1,"mclk":2,"running":false,"droppedEvents":0}}});
    // This one names the keyword rather than a field, and necessarily so: `successResponse` and
    // `errorResponse` both close `additionalProperties`, so the message matches NEITHER arm and the
    // failure is reported at the root `oneOf` of `anyMessage`. Asserting on "oneOf" is asserting that
    // the envelope discriminant did its job.
    let failures = rejects(&line, None, "oneOf");
    assert!(
        failures.iter().any(|f| f.starts_with("anyMessage:")),
        "the failure must come from the envelope check: {failures:#?}"
    );
}

#[test]
fn control_an_error_without_data_is_rejected() {
    // §2 / §2.2 / §2.3: `error.data` is ALWAYS present, because it carries the stamp and the counter.
    // This is the arm of the funnel that reaches `$defs/errorObject` — it needs no separate dispatch,
    // `anyMessage` -> `errorResponse` -> `errorObject` gets there on its own.
    let line =
        json!({"jsonrpc":"2.0","id":11,"error":{"code":-32004,"message":"address out of range"}});
    rejects(&line, None, "oneOf");

    // And the stamp inside `data` is not optional either.
    let line = json!({"jsonrpc":"2.0","id":11,"error":{
        "code":-32004,"message":"address out of range","data":{"addr":"0x0","droppedEvents":0}}});
    rejects(&line, None, "oneOf");
}

#[test]
fn control_a_hex_field_that_is_not_hex_is_rejected() {
    // `$defs/hex` (D9 category 1) is a pattern, not just "a string": an address that lost its `0x` is a
    // client-side parse bug waiting to happen.
    let mut line = good_read_memory_reply();
    line["result"]["addr"] = json!("00FFA144");
    rejects(&line, Some("emulator/read_memory"), "addr");
}

// ---------------------------------------------------------------------------------------------------
// The blind spot, recorded as an executable fact
// ---------------------------------------------------------------------------------------------------

#[test]
fn the_schema_cannot_express_section_8_item_13_and_this_test_proves_it() {
    // Probe finding F2, promoted from a comment to an assertion so it cannot quietly stop being true.
    //
    // A completed `run_frames` mislabelled `reason:"step"` PASSES the schema, because `step` is a legal
    // member of the enum. §3 pins `step` as "one instruction, or one instruction-shaped unit … not the
    // value for a frame advance" — that is BEHAVIOUR, and D14 puts behaviour under the prose, not the
    // schema. So of the two mechanical conformance items in this arc the validator catches one (item 16,
    // the checkpoint id, controlled above) and is blind to the other by construction.
    //
    // The rule itself is asserted behaviourally in `tests/events.rs`
    // (`a_completed_run_frames_reports_runframes_not_step`, `a_press_reports_runframes_…`). If this test
    // ever starts FAILING, the schema grew a way to express item 13 and those tests have a mechanical
    // backstop they did not have before — which is good news, and means this test should be deleted and
    // the doc updated.
    let mislabelled = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"step","pc":"0x00012A4C","frames":8,"deadlineReached":true,
        "frame":8,"mclk":7168320,"running":false}});
    assert!(
        check_incoming(&mislabelled, None).is_ok(),
        "the schema now rejects a mislabelled run_frames stop — see this test's comment"
    );
}

#[test]
fn the_null_id_allowance_is_exactly_as_wide_as_json_rpc_2_0_requires_and_no_wider() {
    // FOUND BY THIS VALIDATOR, not by the probe — the probe's 33-message run drove six error paths, all
    // of which had a parseable request and therefore a real id, so it never saw a null one.
    //
    // JSON-RPC 2.0 §5 MANDATES `"id": null` when the id could not be detected, and our server obeys:
    // three handshake tests (invalid JSON, a batch, an over-long line) all produce it. `$defs/id` was
    // `["integer","string"]` and rejected it — a spec bug under D14, raised as CR-15, and originally
    // carried here as a narrow harness allowance.
    //
    // **CR-15 was ruled and the contract amended the same day** (`empyrean` `protocol.md` §11.4), at
    // exactly this width: `errorResponse.id` is nullable, narrowed by an `if`/`then` to the two codes
    // decided before a request object exists. So the four fences below are no longer OUR fences — they
    // are the schema's, and this test now checks that the schema's width and
    // `is_json_rpc_undetectable_id_error`'s still agree. If the contract ever widens (a nullable id on a
    // code that answers a parsed request), (a) goes red here rather than being discovered by a client
    // that cannot correlate its own failures.
    let parse_error = json!({"jsonrpc":"2.0","id":null,"error":{
        "code":-32700,"message":"invalid JSON",
        "data":{"frame":0,"mclk":0,"running":false,"droppedEvents":0}}});
    assert!(common::schema::is_json_rpc_undetectable_id_error(
        &parse_error
    ));
    check_incoming(&parse_error, None)
        .expect("the schema must accept the shape JSON-RPC 2.0 mandates");
    // ...and the *strict* verdict too: with CR-15 retired there is no allowance left to lean on, so the
    // mandated shape must pass with no allowances in play at all. This is what proves the entry was
    // retired because the contract moved, not because the harness got more permissive.
    check_incoming_strict(&parse_error, None)
        .expect("no allowance should be needed for this shape any more");

    // Now the fence, four sides of it. Each of these must still be rejected.

    // (a) A null id on a code that answers a request we DID parse. There is a real id to echo, so a
    //     null there is a correlation bug, not the protocol's mandate. The schema's `if`/`then` is what
    //     rejects this now.
    let mut wrong_code = parse_error.clone();
    wrong_code["error"]["code"] = json!(-32602);
    assert!(!common::schema::is_json_rpc_undetectable_id_error(
        &wrong_code
    ));
    rejects(&wrong_code, None, "oneOf");

    // (b) A null id on a SUCCESS. Nothing in JSON-RPC 2.0 permits it: a result always answers a request
    //     whose id was read. `$defs/id` was deliberately left unwidened, which is what keeps this shut.
    let null_id_success = json!({"jsonrpc":"2.0","id":null,"result":{
        "frame":0,"mclk":0,"running":false,"droppedEvents":0}});
    assert!(!common::schema::is_json_rpc_undetectable_id_error(
        &null_id_success
    ));
    rejects(&null_id_success, None, "oneOf");

    // (c) Legalising the null id does not legalise anything else about the message. A parse error that
    //     lost its `data` (and so its stamp and `droppedEvents`, §2.2/§2.3) is still caught.
    let mut no_data = parse_error.clone();
    no_data["error"].as_object_mut().unwrap().remove("data");
    assert!(common::schema::is_json_rpc_undetectable_id_error(&no_data));
    rejects(&no_data, None, "oneOf");

    // (d) ...and so is one whose `data` lost `droppedEvents`.
    let mut no_dropped = parse_error.clone();
    no_dropped["error"]["data"]
        .as_object_mut()
        .unwrap()
        .remove("droppedEvents");
    rejects(&no_dropped, None, "oneOf");
}

#[test]
fn the_strict_closure_rejects_a_surplus_key_and_needs_the_unevaluated_keyword_to_do_it() {
    // **Contract §8 item 20, and its own anti-vacuity control.** Two claims, and the second is the one
    // that was got wrong upstream before it was got right — so it is reproduced here rather than trusted.
    //
    // Claim 1: a conformant reply passes, and the same reply with one invented key does not. This is the
    // whole of item 20: "an unknown key on the wire is a change request, never a shipment".
    let good = good_read_memory_reply();
    check_incoming(&good, Some("emulator/read_memory")).expect("a conformant reply must pass");

    let mut surplus = good.clone();
    surplus["result"]["stoppedAtVibes"] = json!(7);
    rejects(&surplus, Some("emulator/read_memory"), "stoppedAtVibes");

    // Claim 2, the mechanics: `additionalProperties: false` would have rejected the CONFORMANT reply, not
    // the surplus one, because in draft 2020-12 it sees only its own `properties` and never those an
    // adjacent `allOf` contributes — and every fragment pulls the stamp (§2.2) and `droppedEvents` (§2.3)
    // in through `allOf: [{"$ref": "#/$defs/replyFields"}]`. Asserting this here means the harness cannot
    // be "simplified" to the obvious keyword without a red test explaining why not.
    let mut fragment = common::schema::schema_root()["methods"]["emulator/read_memory"]["result"]
        .as_object()
        .expect("the fragment is an object")
        .clone();
    fragment.insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    fragment.insert(
        "$defs".into(),
        common::schema::schema_root()["$defs"].clone(),
    );

    let mut wrong = fragment.clone();
    wrong.insert("additionalProperties".into(), json!(false));
    let wrong = jsonschema::validator_for(&Value::Object(wrong)).expect("compiles");
    let envelope_fields: Vec<String> = wrong
        .iter_errors(&good["result"])
        .map(|e| e.to_string())
        .collect();
    assert!(
        !envelope_fields.is_empty(),
        "additionalProperties:false was expected to reject the conformant reply — if it no longer does, \
         the fragments stopped composing their envelope through `allOf` and item 20's note is stale"
    );
    for f in ["frame", "mclk", "running", "droppedEvents"] {
        assert!(
            envelope_fields.iter().any(|e| e.contains(f)),
            "additionalProperties:false must reject `{f}` — that is the defect item 20 documents. \
             errors: {envelope_fields:#?}"
        );
    }

    let mut right = fragment;
    right.insert("unevaluatedProperties".into(), json!(false));
    let right = jsonschema::validator_for(&Value::Object(right)).expect("compiles");
    assert!(
        right.is_valid(&good["result"]),
        "unevaluatedProperties:false must ACCEPT the conformant reply — it is the keyword that sees \
         across applicators, which is the whole reason item 20 names it"
    );
    assert!(
        !right.is_valid(&surplus["result"]),
        "...and must still catch the surplus key"
    );
}

#[test]
fn othermatches_is_the_bounded_container_the_contract_pins_and_carries_no_cursor() {
    // **What is left of the CR-14 fence, re-aimed at the ruled shape.** The test this replaces asserted
    // the *un-ruled* shape under an allowance, which was the honest thing to do while a ruling was
    // pending and is the wrong thing to do now that one has landed (`empyrean` `f309cc8`, §4 + §2.4).
    //
    // The history is worth keeping because of how the defect was found: `otherMatches` is emitted only on
    // the partial-match paths, and the 33-message probe that opened this arc called `lookup_symbol` with a
    // name resolving to nothing, so it only ever drove the error path. Probe finding F1 — "the only
    // schema-level failure on the live wire is the checkpoint id" — was a floor, not a result. That is the
    // sampling weakness §8 item 20 replaces with a gate.
    let ok = json!({"jsonrpc":"2.0","id":3,"result":{
        "query":"Play","exact":false,
        "otherMatches":{"items":[{"name":"Player_2","addr":"0x00FF8D4A"}],
                        "total":2,"returned":1,"limit":5,"truncated":true},
        "frame":0,"mclk":0,"running":false,"droppedEvents":0}});
    check_incoming(&ok, Some("emulator/lookup_symbol"))
        .expect("the ruled container shape must pass");

    // A bare array of strings — what the schema asked for BEFORE the ruling — is now rejected. CR-14 was
    // the first divergence where the server's shape was the better one, and this is the ruling landing.
    let mut bare = ok.clone();
    bare["result"]["otherMatches"] = json!(["Player_2"]);
    rejects(&bare, Some("emulator/lookup_symbol"), "otherMatches");

    // §2.4 clause (a): losing `truncated` is losing the one field the whole rule exists for.
    let mut lost = ok.clone();
    lost["result"]["otherMatches"]
        .as_object_mut()
        .unwrap()
        .remove("truncated");
    rejects(&lost, Some("emulator/lookup_symbol"), "truncated");

    // §2.4 clause (b) / §8 item 16: `lookup_symbol` accepts no continuation param, so a token it emits is
    // one the client can never hand back. The schema spells this `"cursor": false` / `"nextCursor": false`
    // rather than leaving it to prose, and both spellings are fenced.
    for token in ["cursor", "nextCursor"] {
        let mut with_token = ok.clone();
        with_token["result"]["otherMatches"][token] = json!(1);
        rejects(&with_token, Some("emulator/lookup_symbol"), token);
    }

    // One item shape, closed. A branch-specific extra key is exactly the "which branch am I on?" problem
    // §4 struck, and `items[]` closes with `additionalProperties` legally — that subschema has no `allOf`
    // for the keyword to be blind past.
    let mut odd_item = ok.clone();
    odd_item["result"]["otherMatches"]["items"][0]["rawName"] = json!("Player_2");
    rejects(&odd_item, Some("emulator/lookup_symbol"), "rawName");

    // The rest of the result is still the schema's business. `addr` is `$defs/hex`.
    let mut bad_addr = ok.clone();
    bad_addr["result"]["addr"] = json!(4290743546_u64);
    rejects(&bad_addr, Some("emulator/lookup_symbol"), "addr");
}

#[test]
fn anymessage_is_bidirectional_so_a_stray_id_on_an_event_slips_through() {
    // A SECOND blind spot, found by building the controls rather than by reading the schema — it was
    // written as a control that must reject, and it passed.
    //
    // `anyMessage` is a `oneOf` over BOTH directions of the wire (`request`, `successResponse`,
    // `errorResponse`, `notification`). `$defs/notification` does carry `"not": {"required": ["id"]}`,
    // but a line with `jsonrpc` + `id` + `method` + `params` is a perfectly legal `$defs/request` — so
    // an event that grew an `id` is accepted, because from the schema's side of the fence it is
    // indistinguishable from a client request that happened to travel the wrong way.
    //
    // Consequence for THIS harness, which is the part that matters: `check_incoming` keys event-params
    // validation off "has no `id`", so such a line would also skip `events.<name>.params` entirely and
    // be checked by the envelope alone. The server does not do this today (`tests/events.rs` asserts a
    // notification carries no id, directly), and closing it in the schema would need `anyMessage` split
    // into a client-to-server and a server-to-client half — a contract change, not a server one, and so
    // out of this slice by §8's ban on unilateral invention. Recorded, not worked around.
    let stray = json!({"jsonrpc":"2.0","id":4,"method":"emulator/stopped","params":{
        "reason":"pause","pc":"0x0","frame":0,"mclk":0,"running":false}});
    assert!(
        check_incoming(&stray, None).is_ok(),
        "`anyMessage` gained a direction — see this test's comment, and tell the contract"
    );

    // The narrower half IS enforced, and this is what keeps the observation honest: the bad `reason`
    // is caught the moment the line is attributable as an event, i.e. once the `id` is gone.
    let mut as_event = stray.clone();
    as_event.as_object_mut().unwrap().remove("id");
    as_event["params"]["reason"] = json!("frameAdvance");
    assert!(check_incoming(&as_event, None).is_err());
}
