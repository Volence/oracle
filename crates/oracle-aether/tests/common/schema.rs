//! **Contract §8 item 15 — validate real messages against the schema, not against a reading of prose.**
//!
//! `schema/bus-protocol.schema.json` is normative for wire *shapes* (D14): which keys exist, their JSON
//! types and ranges, whether they are required. `protocol.md` stays normative for *behaviour*. This
//! module is the mechanical half of that split: every line the test client receives is checked here, so
//! a shape regression fails a test instead of being noticed by a reader.
//!
//! ## What is validated, and against what
//!
//! | line | schema |
//! |---|---|
//! | every line, no exceptions | `anyMessage` (the envelope `oneOf`) |
//! | a success reply | `methods.<name>.result`, keyed off the method of the request it answers |
//! | the handshake reply | `handshake.initialize.result` |
//! | an error reply | `$defs/errorObject` — reached *through* `anyMessage`, so it needs no separate arm |
//! | a notification | `events.<name>.params`, keyed off the notification's own method name |
//!
//! ## What is deliberately NOT validated: what the client *sends*
//!
//! Decided, not missed. Several tests send intentionally malformed params to assert the server answers
//! `-32602` — a bad hex string, a `label` of the wrong type, `frames: 0`. Validating outgoing requests
//! would need a per-call opt-out threaded through every one of those call sites, and the value is low:
//! the *server* is the conformance subject here, and a request the server rejects correctly has already
//! been judged by the thing that matters. If outgoing validation is ever wanted, the seam is
//! `Client::send_raw`, and the opt-out belongs there.
//!
//! ## Known divergences: [`KNOWN_CONTRACT_DIVERGENCES`]
//!
//! Wiring this in turned four existing tests red on two shapes where the server and the schema disagree.
//! Neither is resolved here — D14 calls a disagreement of that kind a **spec bug awaiting amendment**,
//! and both server shapes are arguably the better ones, so the ruling is the owner's.
//!
//! They are therefore **registered, not silenced**. Each entry names its CR, its method, its JSON path
//! and a canonical instance of the diverging shape, and the registry has three properties that matter
//! more than the allowance itself:
//!
//! 1. the suite stays green, so a known ruling-pending divergence is not read as a regression;
//! 2. an entry that **stops firing** fails
//!    `schema_conformance::every_registered_divergence_is_still_live` — so when a CR is ruled on and the
//!    schema re-vendored, the list cannot silently rot;
//! 3. the list is **printed alongside the coverage report**, so nobody reads a green suite as "fully
//!    conformant". That property is the whole point: CR-13 and CR-14 exist because undocumented wire
//!    shapes were invisible, and an allowance list that hid them would rebuild the problem inside the
//!    instrument built to find it.
//!
//! ## What a validator structurally CANNOT catch
//!
//! D14 puts behaviour under the prose. The sharpest example is live in this tree: an `emulator/stopped`
//! carrying `reason: "step"` for a completed `run_frames` **passes this validator**, because `step` is a
//! legal member of the enum — while §3 and §8 item 13 say the value is `runFrames` and that `step` is a
//! knowing mislabel. Nothing schema-shaped protects that rule. It has its own behavioural assertions in
//! `tests/events.rs`; see the comment there. Read `docs/2026-08-15-schema-validator.md` for the full
//! account of the blind spots, and the coverage split (the schema has a `result` for 8 of the 20 methods
//! we advertise — `tests/schema_conformance.rs` prints and pins it).

#![allow(dead_code)]

use jsonschema::Validator;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The vendored schema, compiled in. Hermetic on purpose: the suite gives the same verdict on a machine
/// with no `empyrean/` checkout. `tests/schema_conformance.rs` proves the copy is fresh.
pub const VENDORED_SCHEMA: &str = include_str!("../contract/bus-protocol.schema.json");

/// The schema root, parsed once.
pub fn schema_root() -> &'static Value {
    static ROOT: OnceLock<Value> = OnceLock::new();
    ROOT.get_or_init(|| {
        serde_json::from_str(VENDORED_SCHEMA)
            .expect("the vendored contract schema must be valid JSON")
    })
}

/// Every compiled validator, built once for the whole test binary.
pub struct Schemas {
    /// `anyMessage` — the envelope. Applied to every line.
    any_message: Validator,
    /// `handshake.initialize.result`.
    handshake_result: Validator,
    /// `methods.<name>.result`, for the methods that have one.
    method_results: BTreeMap<String, Validator>,
    /// `events.<name>.params`, for every event the schema knows.
    event_params: BTreeMap<String, Validator>,
}

pub fn schemas() -> &'static Schemas {
    static S: OnceLock<Schemas> = OnceLock::new();
    S.get_or_init(Schemas::compile)
}

/// Splice the root `$defs` into a fragment so its `#/$defs/...` refs resolve.
///
/// The schema document is not itself a JSON Schema — `anyMessage`, `handshake`, `events` and `methods`
/// are plain object keys, not keywords — so a fragment has to be lifted out and compiled on its own.
/// Every fragment refs `#/$defs/replyFields` or `#/$defs/stamp`, and a `$ref` resolves against the root
/// of *the document being compiled*, so the `$defs` block must travel with it. Under 2020-12 a `$ref`
/// may have siblings, which is what makes this work for the fragments that are a bare
/// `{"$ref": "#/$defs/replyFields"}` (e.g. `emulator/restore`'s result).
fn with_defs(fragment: &Value) -> Value {
    let mut o: Map<String, Value> = fragment
        .as_object()
        .cloned()
        .expect("a schema fragment must be an object");
    o.insert(
        "$schema".into(),
        Value::String("https://json-schema.org/draft/2020-12/schema".into()),
    );
    o.insert("$defs".into(), schema_root()["$defs"].clone());
    Value::Object(o)
}

fn compile(fragment: &Value, what: &str) -> Validator {
    jsonschema::validator_for(&with_defs(fragment))
        .unwrap_or_else(|e| panic!("the contract schema fragment `{what}` does not compile: {e}"))
}

impl Schemas {
    fn compile() -> Self {
        let root = schema_root();

        let mut method_results = BTreeMap::new();
        for (name, spec) in root["methods"]
            .as_object()
            .expect("schema.methods is an object")
        {
            if let Some(result) = spec.get("result") {
                method_results.insert(
                    name.clone(),
                    compile(result, &format!("methods.{name}.result")),
                );
            }
        }

        let mut event_params = BTreeMap::new();
        for (name, spec) in root["events"]
            .as_object()
            .expect("schema.events is an object")
        {
            // `$comment` sits beside the event entries; it is a string, not an event.
            let Some(params) = spec.get("params") else {
                continue;
            };
            event_params.insert(
                name.clone(),
                compile(params, &format!("events.{name}.params")),
            );
        }

        Self {
            any_message: compile(&root["anyMessage"], "anyMessage"),
            handshake_result: compile(
                &root["handshake"]["initialize"]["result"],
                "handshake.initialize.result",
            ),
            method_results,
            event_params,
        }
    }

    /// The methods the schema gives a `result` schema for. Sorted.
    pub fn methods_with_result(&self) -> Vec<&str> {
        self.method_results.keys().map(String::as_str).collect()
    }

    /// The events the schema gives a `params` schema for. Sorted.
    pub fn events_with_params(&self) -> Vec<&str> {
        self.event_params.keys().map(String::as_str).collect()
    }
}

// ---------------------------------------------------------------------------------------------------
// The divergence registry
// ---------------------------------------------------------------------------------------------------

/// One place where this server's wire shape and the normative schema disagree, **registered rather than
/// silenced**, pending an owner ruling on the change request named here.
///
/// See the module doc for the three properties the registry has to hold. The load-bearing field is
/// [`Divergence::canonical`]: it is a message that the schema must **still** reject, so
/// `schema_conformance::every_registered_divergence_is_still_live` fails the moment a ruling lands and the
/// schema is re-vendored. Liveness is keyed to the *schema* rather than to observed traffic on purpose —
/// each test binary is its own process, so a counter of live firings could only see the messages its own
/// binary happened to produce, which is the sampling weakness that let CR-14 through a 33-message probe.
pub struct Divergence {
    /// The change request in `docs/2026-08-14-aether-change-requests.md` this is registered under.
    pub cr: &'static str,
    /// The method it appears on, or `"<envelope>"` when it is not method-specific.
    pub method: &'static str,
    /// JSON path within the message.
    pub path: &'static str,
    /// One line, for the report the coverage test prints.
    pub summary: &'static str,
    /// A canonical message carrying the diverging shape, and the method to attribute it to.
    /// Must fail [`check_incoming_strict`] and pass [`check_incoming`].
    pub canonical: fn() -> (Value, Option<&'static str>),
}

/// **Every known divergence, and nothing else.** Adding an entry here is a deliberate, reviewable act;
/// nothing is allowed for implicitly, and no entry may be added without a CR number.
pub const KNOWN_CONTRACT_DIVERGENCES: &[Divergence] = &[
    Divergence {
        cr: "CR-14",
        method: "emulator/lookup_symbol",
        path: "$.otherMatches",
        summary: "schema says an array of strings; we emit the house bounded-array object with \
                  {name,demangled,addr} items — wrong container AND wrong element type",
        canonical: || {
            (
                json!({"jsonrpc":"2.0","id":3,"result":{
                    "name":"Player_1","addr":"0x00FF8CFA",
                    "otherMatches":{"items":[{"name":"Player_2","demangled":"Player_2","addr":"0x00FF8D4A"}],
                                    "total":2,"returned":1,"cursor":0,"limit":5,
                                    "truncated":true,"nextCursor":1},
                    "frame":0,"mclk":0,"running":false,"droppedEvents":0}}),
                Some("emulator/lookup_symbol"),
            )
        },
    },
    // **CR-15 was here, and was retired the same day it was raised** — the registry's anti-rot property
    // working on real traffic rather than in a drill. Its entry said `$defs/id` is `[integer,string]`
    // while JSON-RPC 2.0 §5 mandates `null` when the id could not be detected. The contract was amended
    // (`empyrean` §11.4), the vendored copy refreshed, and `every_registered_divergence_is_still_live`
    // went red on the next run because the canonical message it registered was no longer rejected. That
    // failure is the only reason this entry is gone rather than quietly wrong — which is exactly the
    // property the list was built for and the reason it is worth more than a comment.
];

/// The registry, rendered for the report the coverage test prints.
pub fn divergence_report() -> Vec<String> {
    KNOWN_CONTRACT_DIVERGENCES
        .iter()
        .map(|d| format!("{} {} {} — {}", d.cr, d.method, d.path, d.summary))
        .collect()
}

/// Whether a line is the JSON-RPC 2.0 §5 *"id could not be detected"* reply: `error`, `id: null`, and one
/// of the two **transport-level** codes — `-32700` (parse error) and `-32600` (invalid request). Both are
/// decided *before* a request object exists, which is exactly when the standard mandates the null; every
/// other code answers a request the server did parse and therefore has a real id to echo.
///
/// **This is no longer an allowance.** It was CR-15's, when the schema's `$defs/id` was
/// `["integer","string"]` and rejected the reply the standard requires. The contract adopted the fix on
/// 2026-08-15 (`protocol.md` §11.4) at *exactly* this width — `errorResponse.id` is nullable, narrowed by
/// an `if`/`then` to these two codes — so the schema now enforces what this predicate used to assert.
/// It survives as the oracle for
/// `schema_conformance::the_null_id_allowance_is_exactly_as_wide_as_json_rpc_2_0_requires_and_no_wider`,
/// which now checks the *schema's* width against it rather than our own: the two must agree, and if the
/// contract ever widens, that test says so.
pub fn is_json_rpc_undetectable_id_error(line: &Value) -> bool {
    line.get("result").is_none()
        && line.get("id").is_some_and(Value::is_null)
        && line
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(Value::as_i64)
            .is_some_and(|c| c == -32700 || c == -32600)
}

/// **CR-14's allowance.** `methods["emulator/lookup_symbol"].result.otherMatches` is
/// `{"type":"array","items":{"type":"string"}}` in the schema and §4's prose agrees; this server emits
/// `rpc::bounded_array` there — an object `{items, total, returned, cursor, limit, truncated,
/// nextCursor}` with `{name, demangled, addr}` items. Wrong container *and* wrong element type, because
/// *"every array is bounded, cursored, and flags truncation"* is a standing non-negotiable here, asserted
/// by name in `tests/methods.rs::arrays_are_bounded_cursored_and_flag_truncation`.
///
/// The key is **not left unchecked**, which is the difference between registering a divergence and
/// exempting a method from validation. It is lifted out before the schema runs and checked against the
/// house shape by [`bounded_array_failures`] instead — so the allowance swaps one authority for another,
/// and everything else in the result is still the schema's business. Pinned by
/// `schema_conformance::the_othermatches_divergence_is_swapped_for_the_house_shape_not_left_unchecked`.
///
/// Note what is deliberately **not** allowed for, because CR-14 folds it in: `cursor`/`nextCursor` are
/// JSON numbers while §8 item 16 says every list cursor is a string, and `lookup_symbol` accepts no
/// `cursor` param at all. That is CR-14's to rule on, not this harness's to check.
fn known_result_divergence(method: &str, key: &str) -> bool {
    matches!((method, key), ("emulator/lookup_symbol", "otherMatches"))
}

/// The house bounded-array envelope, as `rpc::bounded_array` builds it.
pub fn bounded_array_failures(v: &Value, what: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(o) = v.as_object() else {
        return vec![format!("{what}: not an object")];
    };
    for k in ["items", "total", "returned", "cursor", "limit", "truncated"] {
        if !o.contains_key(k) {
            out.push(format!("{what}: bounded array is missing `{k}`"));
        }
    }
    if !o.contains_key("nextCursor") {
        out.push(format!("{what}: bounded array is missing `nextCursor`"));
    }
    if o.get("items").is_some_and(|i| !i.is_array()) {
        out.push(format!("{what}: `items` is not an array"));
    }
    if o.get("truncated").is_some_and(|t| !t.is_boolean()) {
        out.push(format!("{what}: `truncated` is not a boolean"));
    }
    for k in ["total", "returned", "limit"] {
        if o.get(k).is_some_and(|n| !n.is_u64()) {
            out.push(format!("{what}: `{k}` is not a non-negative integer"));
        }
    }
    out
}

/// One validation failure, rendered for a panic message.
fn errors(v: &Validator, instance: &Value, what: &str) -> Vec<String> {
    v.iter_errors(instance)
        .map(|e| {
            let path = e.instance_path().to_string();
            let path = if path.is_empty() { "$".into() } else { path };
            format!("{what}: {path}: {e}")
        })
        .collect()
}

/// **The check.** Validate one server→client line.
///
/// `method` is the method of the request this line answers, when the caller knows it (`Client::call`
/// does). `None` means "an unattributed line" — the envelope is still checked, and so are event params,
/// which key off the notification's own `method` field and need no help.
///
/// Returns the list of failures rather than panicking, so the anti-vacuity controls in
/// `tests/schema_conformance.rs` can assert that this function *rejects* — a validator that accepts
/// everything is exactly the vacuous assertion this repo has twice been burned by.
///
/// Honours [`KNOWN_CONTRACT_DIVERGENCES`]. Use [`check_incoming_strict`] for the unallowed verdict.
pub fn check_incoming(line: &Value, method: Option<&str>) -> Result<(), Vec<String>> {
    check(line, method, true)
}

/// [`check_incoming`] with **no allowances at all** — the schema's own verdict.
///
/// This is what makes the registry unrottable: `schema_conformance` asserts that every registered
/// divergence's canonical message still fails *this* function. When a CR is ruled on and the schema
/// re-vendored, that assertion goes red and the entry must be deleted, so an allowance cannot outlive
/// the disagreement it was raised for.
pub fn check_incoming_strict(line: &Value, method: Option<&str>) -> Result<(), Vec<String>> {
    check(line, method, false)
}

fn check(line: &Value, method: Option<&str>, allow: bool) -> Result<(), Vec<String>> {
    let s = schemas();
    let mut out = Vec::new();

    // 1. The envelope, for every line, no exceptions. This arm is also what validates `error.data`:
    //    `anyMessage` -> `errorResponse` -> `$defs/errorObject` -> `data` -> `$defs/replyFields`.
    //
    //    **CR-15 used to need an allowance here and no longer does.** The schema rejected the `"id": null`
    //    that JSON-RPC 2.0 §5 mandates on `-32700`/`-32600`, so this arm substituted a placeholder id to
    //    keep checking the rest of the message. The contract was amended the same day (§11.4) — and
    //    amended *narrowly*, restricting null to exactly those two codes via `if`/`then`, so all four
    //    fences the allowance carried are now enforced by the schema itself rather than by our harness.
    //    There is nothing left to patch, so the envelope is now checked verbatim.
    out.extend(errors(&s.any_message, line, "anyMessage"));

    // 2. A success reply, keyed off the method of the request it answers.
    if let Some(result) = line.get("result") {
        match method {
            Some("initialize") => out.extend(errors(
                &s.handshake_result,
                result,
                "handshake.initialize.result",
            )),
            Some(m) => {
                if let Some(v) = s.method_results.get(m) {
                    // CR-14's allowance: lift the diverging key out and check it against the house shape
                    // instead, so this swaps authorities rather than creating a hole. Everything left is
                    // validated against the schema unchanged.
                    let mut subject = result.clone();
                    if allow {
                        if let Some(o) = subject.as_object_mut() {
                            let diverging: Vec<String> = o
                                .keys()
                                .filter(|k| known_result_divergence(m, k))
                                .cloned()
                                .collect();
                            for k in diverging {
                                let val = o.remove(&k).expect("just enumerated");
                                out.extend(bounded_array_failures(
                                    &val,
                                    &format!("CR-14 divergence methods.{m}.result.{k}"),
                                ));
                            }
                        }
                    }
                    out.extend(errors(v, &subject, &format!("methods.{m}.result")));
                }
                // else: one of the 12 methods the SEED schema has no `result` for. Not a pass — an
                // absence. `tests/schema_conformance.rs` prints and pins that list so it cannot grow.
            }
            None => {}
        }
    }

    // 3. A notification, keyed off its own method name.
    if line.get("id").is_none() {
        if let Some(name) = line.get("method").and_then(Value::as_str) {
            if let Some(v) = s.event_params.get(name) {
                let params = line.get("params").cloned().unwrap_or(Value::Null);
                out.extend(errors(v, &params, &format!("events.{name}.params")));
            }
        }
    }

    if out.is_empty() {
        Ok(())
    } else {
        Err(out)
    }
}

/// `check_incoming`, panicking with every failure listed. This is what `Client::recv` calls.
pub fn assert_incoming(line: &Value, method: Option<&str>) {
    if let Err(failures) = check_incoming(line, method) {
        panic!(
            "contract schema violation on the wire ({} failure(s)); \
             the schema is normative for wire shapes (protocol.md D14, §8 item 15)\n  \
             answering method: {}\n  line: {}\n  {}\n\
             \n  If this is a divergence the owner has yet to rule on, it belongs in \
             common::schema::KNOWN_CONTRACT_DIVERGENCES with a CR number — never silenced here.",
            failures.len(),
            method.unwrap_or("<unattributed>"),
            line,
            failures.join("\n  "),
        );
    }
}
