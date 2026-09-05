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
//! ## Closure: contract §8 item 20
//!
//! Every result is validated against its fragment **closed with `unevaluatedProperties: false`** ([`closed`]).
//! An unknown key on the wire is a change request, never a shipment. The keyword and its location are both
//! load-bearing and both were got wrong before they were got right — see [`closed`].
//!
//! ## Known divergences: [`KNOWN_CONTRACT_DIVERGENCES`]
//!
//! Wiring this in turned four existing tests red on two shapes where the server and the schema disagree.
//! Neither was resolved here — D14 calls a disagreement of that kind a **spec bug awaiting amendment** —
//! and both (CR-14, CR-15) have since been ruled on and retired, by the registry's own anti-rot test
//! rather than by anyone remembering. **CR-16 replaced them the same day**, found by turning §8 item 20's
//! closure on: five keys `protocol.md`'s prose registers by name and its schema forgot to declare.
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
//! account of the blind spots, and the coverage split (as of the `af434a2` re-vendor the schema has a
//! `result` for **all 25** methods we advertise — `tests/schema_conformance.rs` prints and pins it).
//!
//! The sharpest live blind spot gained a second instance with CR-9. `emulator/stopped`'s `buttons`/`port`
//! are REQUIRED *iff* `emulator/press` drove the advance, and the event deliberately carries no method
//! discriminator — `reason` names the stop CONDITION, so a press-driven advance and a `run_frames` one both
//! read `runFrames`. `dependentRequired` enforces the half that is expressible (the two travel together);
//! the rest is behavioural, and `control_buttons_without_port_is_rejected_and_that_is_all_the_schema_can_do`
//! asserts the gap so nobody reads the schema as covering more than it does.

#![allow(dead_code)]

use jsonschema::Validator;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The vendored schema, compiled in. Hermetic on purpose: the suite gives the same verdict on a machine
/// with no `empyrean/` checkout. `tests/schema_conformance.rs` proves the copy is fresh.
pub const VENDORED_SCHEMA: &str = include_str!("../contract/bus-protocol.schema.json");

/// **The contract's own wire vectors, vendored beside the schema** (first vendored 2026-09-05, §11.36).
///
/// The schema says what a shape *is*; the vectors say which concrete documents the hub asserts it accepts
/// and — the half that matters — which it asserts it **refuses**. Vendoring the schema alone left this
/// repo able to compile a fragment that accepts everything and call the result conformance. `PROVENANCE.md`
/// pins these bytes by blob exactly as it pins the schema's, and
/// `schema_conformance::the_contracts_own_vectors_pass_and_fail_exactly_as_declared` runs them.
pub const VENDORED_VECTORS: &str = include_str!("../contract/vectors.json");

/// The vectors document, parsed once.
pub fn vectors_root() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_str(VENDORED_VECTORS)
            .expect("the vendored contract vectors must be valid JSON")
    })
}

/// Compile one fragment out of the vendored schema, optionally under §8 item 20's closure.
///
/// The public door onto [`with_defs`]/[`closed`] for the vector runner, which needs `params` fragments
/// (never closed here — the closure is already *written into* every `params` fragment upstream, §2.5) as
/// well as `result` and event `params` fragments (closed, item 20).
pub fn compile_fragment(fragment: &Value, what: &str, close: bool) -> Validator {
    if close {
        compile(&closed(fragment), what)
    } else {
        compile(fragment, what)
    }
}

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

/// **Contract §8 item 20 — close a result fragment, at test time only.**
///
/// Item 20: *"A server's conformance suite MUST fail on any result key absent from that method's
/// fragment. An unknown key on the wire is a change request, never a shipment."* This is the keyword that
/// implements it, and both halves of where it goes were got wrong before they were got right — §11.5
/// reproduces the experiment, and so does `schema_conformance::the_strict_closure_needs_unevaluated…`:
///
/// * `additionalProperties: false` **rejects every conformant reply**. Every fragment pulls its envelope
///   in through `allOf: [{"$ref": "#/$defs/replyFields"}]`, and in draft 2020-12 `additionalProperties`
///   sees only the `properties` in its own schema object — never those an adjacent `allOf` contributes. So
///   it rejects `frame`, `mclk`, `running` and `droppedEvents` on every reply, which is to say it rejects
///   D11 and D17.
/// * `unevaluatedProperties` **does** see across applicators, and catches exactly the surplus.
///
/// And it is applied **here**, never written into the vendored schema: closure binds *servers*, additivity
/// (D5) protects *clients*, and a published closure would let a client's month-old schema reject a server
/// that added a registered key. The harness is the one place where only servers stand.
///
/// Closure is applied at the **top level of the result object**. That is item 20's literal subject ("any
/// result key"), and it is where the whole measured surplus lived. Nested objects are closed only where
/// the contract closes them itself — `otherMatches.items[]` carries its own `additionalProperties: false`
/// in the published schema, which is legal there because that subschema has no `allOf` to see past.
fn closed(fragment: &Value) -> Value {
    let mut o: Map<String, Value> = fragment
        .as_object()
        .cloned()
        .expect("a schema fragment must be an object");
    o.insert("unevaluatedProperties".into(), Value::Bool(false));
    Value::Object(o)
}

/// Splice the root `$defs` into a fragment so its `#/$defs/...` refs resolve.
///
/// The schema document is not itself a JSON Schema — `anyMessage`, `handshake`, `events` and `methods`
/// are plain object keys, not keywords — so a fragment has to be lifted out and compiled on its own.
/// Every fragment refs `#/$defs/replyFields` or `#/$defs/stamp`, and a `$ref` resolves against the root
/// of *the document being compiled*, so the `$defs` block must travel with it. Under 2020-12 a `$ref`
/// may have siblings, which is what makes this work for the fragments that are a bare
/// `{"$ref": "#/$defs/replyFields"}` (`emulator/release_all` and `emulator/log_clear`, the two left of
/// them — `emulator/restore` was the example named here until §11.39 gave it a `hitsDropped` of its own
/// and its `$ref` moved into an `allOf`).
fn with_defs(fragment: &Value) -> Value {
    let mut o: Map<String, Value> = fragment
        .as_object()
        .cloned()
        .expect("a schema fragment must be an object");
    o.insert(
        "$schema".into(),
        Value::String("https://json-schema.org/draft/2020-12/schema".into()),
    );
    // Root `$defs` MERGED UNDER the fragment's own, never over it. A fragment may carry a `$defs` of its
    // own for a shape only it uses — `get_profiler_frames` defines `interruptBucket` there so `hint` and
    // `vint` are provably the same shape rather than two copies that can drift. Clobbering the key would
    // delete that definition and leave its `$ref` dangling, which is a failure that looks like a contract
    // error and is not one.
    let mut defs: Map<String, Value> = schema_root()["$defs"]
        .as_object()
        .cloned()
        .expect("root $defs is an object");
    if let Some(local) = fragment.get("$defs").and_then(Value::as_object) {
        for (k, v) in local {
            defs.insert(k.clone(), v.clone());
        }
    }
    o.insert("$defs".into(), Value::Object(defs));

    // A fragment-local `$defs` is referenced from within the WHOLE document, so its `$ref` is an absolute
    // pointer (`#/methods/emulator~1get_profiler_frames/result/$defs/...`). Lifting the fragment to be its
    // own root breaks that path — the pointer is correct in the document it was written for and
    // unresolvable in ours. So when a fragment uses one, carry `methods` along: it is a plain data key
    // rather than a schema keyword (which is the same reason the document needs lifting at all), so it is
    // inert for validation and exists purely to give the pointer something to land on.
    //
    // Done conditionally, and cheaply, because splicing a copy of every method into every fragment would
    // multiply the compile input by the size of the document for the benefit of one fragment.
    if fragment.to_string().contains("\"#/methods/") {
        o.insert("methods".into(), schema_root()["methods"].clone());
    }
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
                    compile(&closed(result), &format!("methods.{name}.result")),
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
            // The handshake reply is a result too, and item 20 says *every* result.
            handshake_result: compile(
                &closed(&root["handshake"]["initialize"]["result"]),
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
    // **CR-16 was here for a few hours on 2026-08-15, and is the shortest-lived entry the registry has
    // held.** Its two entries said that five keys `protocol.md` registers by name — `initialize.limits`,
    // `.methodSummaries`, and `read_memory.region`/`.symbolDisp`/`.caveat` — were absent from the schema
    // fragments that §8 item 20's closure checks against, so a conformant reply was rejected by the
    // artifact meant to describe it. The contract adopted the fix (`empyrean` `d45dc87`,
    // `protocol.md` §11.6): five `properties` entries, `limits` added to `initialize`'s `required`,
    // `region` to `read_memory`'s, and **no prose changed, because the prose was already right**.
    //
    // Retiring it was not optional and not tidy-up. These entries *lift* their keys out of the payload
    // before validating, so the moment the schema **required** `limits`, lifting it made it missing and
    // every checkpoint test went red on the handshake. An allowance that outlives its divergence does not
    // merely go stale — it starts causing the failure it was written to suppress.
    // **The two entries the registry held before today were retired by the mechanism they were built
    // for, not by a tidy-up.** Kept as the record of what retirement looks like:
    //
    // **CR-14 was here until 2026-08-15.** Its entry said the schema typed
    // `lookup_symbol.otherMatches` as an array of strings while this server emitted a bounded object with
    // two different item shapes and a numeric continuation token. The contract ruled it (`empyrean`
    // `f309cc8`, `protocol.md` §4 rewritten + §2.4's new bounded-list rule) and ruled it *our way on the
    // container and against us on the token*: `otherMatches` is now `$defs/boundedList` with one pinned
    // item shape and **no `cursor`, no `nextCursor`**. The schema was re-vendored, and
    // `every_registered_divergence_is_still_live` went red — the canonical message it registered was no
    // longer rejected — which is the only reason this entry is deleted rather than quietly wrong.
    //
    // **CR-15 was here too, and was retired the same day it was raised** — the registry's anti-rot property
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

/// The checker a registered divergence substitutes for the schema, on one key. Takes the key's value and
/// a label for the failure message; returns one message per failure, empty when the key is well-shaped.
type KeyChecker = fn(&Value, &str) -> Vec<String>;

/// **The allowance seam** — see [`KNOWN_CONTRACT_DIVERGENCES`].
///
/// A registered divergence names result keys whose shape the schema and this server disagree about,
/// pending a ruling. Those keys are then **not left unchecked**, which is the difference between
/// registering a divergence and exempting a method from validation: this returns the checker that takes
/// the schema's place for one key, so an allowance swaps one authority for another. Everything else in the
/// result — including §8 item 20's closure over every key that is *not* listed here — still runs.
///
/// **It is empty today, and that is the point.** CR-16's five keys lived here for a few hours on
/// 2026-08-15 — `initialize.limits`/`.methodSummaries` and `read_memory.region`/`.symbolDisp`/`.caveat`,
/// all registered in `protocol.md`'s prose and none of them declared in the schema. The contract adopted
/// the declarations (`empyrean` `d45dc87`, §11.6) and the entries went the same day. Nothing is allowed
/// for implicitly: a key appears here only with a CR number, and only while its divergence is live.
///
/// **The retirement was forced, not remembered, and by a failure mode the registry was not designed
/// around.** A checker here *lifts its key out of the payload* before validating it, so the moment the
/// amended schema **required** `limits`, lifting it made it missing — and every checkpoint test went red
/// on the handshake, in tests with nothing to do with checkpoints. An allowance that outlives its
/// divergence does not go stale quietly; it starts causing the failure it was written to suppress.
///
/// To add one: restore the `match (method, key)` this function used to be, with an arm per key.
fn known_result_divergence(_method: &str, _key: &str) -> Option<KeyChecker> {
    None
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

    // 2. A success reply, keyed off the method of the request it answers. The handshake reply goes down
    //    the same path as any other result and differs only in which fragment it is validated against —
    //    it is a result, it is closed by item 20, and it can carry a registered divergence like any other.
    //    (It does: CR-16's `limits`/`methodSummaries`.)
    if let Some(result) = line.get("result") {
        let target = match method {
            Some("initialize") => Some((
                &s.handshake_result,
                "handshake.initialize.result".to_string(),
            )),
            // `None` on a method the schema has no `result` for is not a pass — it is an absence.
            // `tests/schema_conformance.rs` prints and pins that list so it cannot grow. As of the
            // 2026-08-15 re-vendor the list is EMPTY: every advertised method has a fragment.
            Some(m) => s
                .method_results
                .get(m)
                .map(|v| (v, format!("methods.{m}.result"))),
            None => None,
        };
        if let (Some((validator, label)), Some(owner)) = (target, method) {
            // An allowance lifts its key out and hands it to the registered checker instead, so this
            // swaps authorities rather than creating a hole. Everything left is validated against the
            // schema unchanged — including, since 2026-08-15, item 20's closure, so a key that is neither
            // in the fragment nor registered here fails.
            let mut subject = result.clone();
            if allow {
                if let Some(o) = subject.as_object_mut() {
                    let diverging: Vec<(String, KeyChecker)> = o
                        .keys()
                        .filter_map(|k| known_result_divergence(owner, k).map(|f| (k.clone(), f)))
                        .collect();
                    for (k, check_key) in diverging {
                        let val = o.remove(&k).expect("just enumerated");
                        out.extend(check_key(
                            &val,
                            &format!("registered divergence {label}.{k}"),
                        ));
                    }
                }
            }
            out.extend(errors(validator, &subject, &label));
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
