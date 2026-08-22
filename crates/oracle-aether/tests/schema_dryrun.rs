//! **DRY-RUN SCAFFOLDING — not part of the default suite, not a conformance gate.**
//!
//! Written 2026-08-22 to answer one question about a set of contract fragments that had **not yet been
//! vendored and were not yet pushed upstream**: *would they accept or refuse the replies this server
//! actually emits today?* It is deliberately quarantined:
//!
//! * every test here is `#[ignore]`d, so `cargo test` never runs it;
//! * the runner is named and explicit —
//!   `AETHER_DRYRUN_SCHEMA=<path> cargo test -p oracle-aether --test schema_dryrun -- --ignored --nocapture`;
//! * it reads its candidate schema from a **path**, never from the vendored copy, so running it cannot
//!   change what the real suite validates against. The vendored copy stays the controller's call.
//!
//! # What it measures, and what it refuses to pretend it measured
//!
//! For every method the candidate schema schematizes and the vendored one does not, it asks this server
//! for a reply and validates that reply against the candidate's own fragment, closed per §8 item 20.
//! Three outcomes, kept separate on purpose:
//!
//! * **MEASURED/PASS** — a reply was obtained and the fragment accepted it.
//! * **MEASURED/REFUSED** — a reply was obtained and the fragment rejected it, with the errors named.
//! * **UNMEASURED** — no reply could be obtained. Most often `-32601`: the method is in the contract's
//!   §6 catalog but this server does not advertise or serve it, so there is no reply for a fragment to
//!   judge. **This is not a pass and must never be rendered as one.**
//!
//! The last bullet is the reason this file asserts the way it does: [`dry_run`] **fails** while anything
//! is UNMEASURED, and prints the whole table before it does. A dry run whose unmeasured rows sat quietly
//! inside a green result would be the single most dangerous output the exercise could produce — a reader
//! would take "green" for "these 21 fragments are fine", when nothing on this server ever exercised them.
//!
//! # The anti-vacuity control
//!
//! [`POSITIVE_CONTROLS`] are methods this server *does* serve and both schemas *do* cover. They go through
//! the identical procedure and must come back MEASURED. Without them, "21 UNMEASURED" would be equally
//! consistent with a broken harness that can reach nothing at all, and the whole table would prove
//! nothing. With them, UNMEASURED is a fact about the server's method set rather than about this file.
//!
//! The newly-covered set is **derived** — candidate fragments minus vendored fragments, by parse — never
//! transcribed from the empyrean commit message or from a nearby list. A hand-copied set is a count
//! waiting to disagree with the document it claims to describe.

mod common;

use common::{spawn, Client};
use jsonschema::Validator;
use oracle_aether::engine::METHODS;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

/// Methods this server serves and BOTH schemas cover. The instrument's own control: these must be
/// MEASURED, or the table's UNMEASURED rows describe this file rather than the server.
const POSITIVE_CONTROLS: &[&str] = &[
    "emulator/status",
    "emulator/registers",
    "emulator/read_cram",
];

/// Params for a control, chosen so the call succeeds on a freshly reset test ROM.
fn control_params(method: &str) -> Value {
    match method {
        // `line`, not `index`/`count`. Getting this wrong on the first run is why the control exists:
        // it came back UNMEASURED with `-32602 … accepted params: line`, and the assertion below stopped
        // the run rather than letting a mis-parameterised control sit in the table looking like evidence.
        "emulator/read_cram" => json!({"line": 0}),
        _ => json!({}),
    }
}

/// The candidate schema, from `AETHER_DRYRUN_SCHEMA`.
///
/// No default and no fallback. A dry run that silently measured the vendored copy against itself would
/// report a clean sweep and mean nothing, which is exactly the failure this whole file is built to avoid.
fn candidate() -> Value {
    let path = std::env::var("AETHER_DRYRUN_SCHEMA").unwrap_or_else(|_| {
        panic!(
            "AETHER_DRYRUN_SCHEMA is unset. This harness has NO default candidate on purpose: \
             falling back to the vendored copy would validate it against itself, report a clean \
             sweep, and mean nothing. Point it at the schema revision under test."
        )
    });
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read the candidate schema at {path}: {e}"));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("the candidate schema at {path} is not valid JSON: {e}"))
}

/// The method names a document gives a `result` fragment for.
fn fragments_with_result(doc: &Value) -> BTreeSet<String> {
    doc["methods"]
        .as_object()
        .expect("the schema's `methods` is an object")
        .iter()
        .filter(|(k, v)| !k.starts_with('$') && v.get("result").is_some())
        .map(|(k, _)| k.clone())
        .collect()
}

/// Lift a fragment into a standalone schema document and close it, exactly as `common::schema` does.
///
/// Reimplemented here rather than imported because those helpers are private to the real harness and
/// bound to the *vendored* root — this one must resolve `$ref`s against the **candidate** document, which
/// is the whole point. The two rules it reproduces are load-bearing and documented at length there:
/// `$defs` must travel with the fragment (every one refs `#/$defs/replyFields`), and closure is
/// `unevaluatedProperties`, never `additionalProperties`, because the latter is blind past the `allOf`
/// each fragment pulls its envelope in through.
fn closed_fragment(doc: &Value, method: &str) -> Value {
    let mut o: Map<String, Value> = doc["methods"][method]["result"]
        .as_object()
        .unwrap_or_else(|| panic!("{method}: the candidate's result fragment is not an object"))
        .clone();
    o.insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    let mut defs: Map<String, Value> = doc["$defs"]
        .as_object()
        .expect("the candidate's root $defs is an object")
        .clone();
    // A fragment-local `$defs` wins over the root's — never the other way round, or a shape only that
    // fragment defines is deleted and its `$ref` dangles.
    if let Some(local) = o.get("$defs").and_then(Value::as_object) {
        for (k, v) in local.clone() {
            defs.insert(k, v);
        }
    }
    o.insert("$defs".into(), Value::Object(defs));
    if Value::Object(o.clone())
        .to_string()
        .contains("\"#/methods/")
    {
        o.insert("methods".into(), doc["methods"].clone());
    }
    o.insert("unevaluatedProperties".into(), json!(false));
    Value::Object(o)
}

fn compile(doc: &Value, method: &str) -> Result<Validator, String> {
    jsonschema::validator_for(&closed_fragment(doc, method)).map_err(|e| e.to_string())
}

/// What happened when we asked this server for a reply to one method.
enum Verdict {
    Pass,
    Refused(Vec<String>),
    /// No reply obtainable. The string is *why*, in the server's own words.
    Unmeasured(String),
    /// The fragment does not even compile — a defect findable with no reply at all.
    FragmentBroken(String),
}

impl Verdict {
    fn label(&self) -> &'static str {
        match self {
            Verdict::Pass => "MEASURED/PASS",
            Verdict::Refused(_) => "MEASURED/REFUSED",
            Verdict::Unmeasured(_) => "UNMEASURED",
            Verdict::FragmentBroken(_) => "FRAGMENT-BROKEN",
        }
    }
    fn detail(&self) -> String {
        match self {
            Verdict::Pass => "the candidate fragment accepts this server's reply".into(),
            Verdict::Refused(e) => e.join(" | "),
            Verdict::Unmeasured(why) => why.clone(),
            Verdict::FragmentBroken(e) => e.clone(),
        }
    }
}

/// Ask the server for one reply and judge it against the candidate fragment.
fn probe(c: &mut Client, doc: &Value, method: &str, params: Value) -> Verdict {
    let validator = match compile(doc, method) {
        Ok(v) => v,
        Err(e) => return Verdict::FragmentBroken(e),
    };
    let reply = c.call(method, params);
    if let Some(err) = reply.get("error") {
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("<no message>");
        let why = if code == -32601 {
            format!(
                "NO REPLY EXISTS TO JUDGE: the server answered -32601 ({msg}). This method is in the \
                 contract's §6 catalog and is NOT in this server's advertised method set, so the \
                 fragment governs a reply this server has never emitted."
            )
        } else {
            format!("the call failed with {code} ({msg}) — no result to validate")
        };
        return Verdict::Unmeasured(why);
    }
    let result = &reply["result"];
    let errs: Vec<String> = validator
        .iter_errors(result)
        .map(|e| {
            let p = e.instance_path().to_string();
            let p = if p.is_empty() { "$".into() } else { p };
            format!("{p}: {e}")
        })
        .collect();
    if errs.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Refused(errs)
    }
}

/// **Anti-vacuity: the probe must be able to say REFUSED.**
///
/// [`dry_run`]'s controls prove it can reach a method and get a reply. They do **not** prove the
/// validation step does anything — a `probe` that returned `Pass` unconditionally would satisfy every
/// assertion in this file, and the whole table would be a validator accepting everything while wearing a
/// library's clothes. That failure has shipped in this repo before.
///
/// So: take a control the candidate genuinely accepts, plant one defect in the **candidate document, in
/// memory only**, and require the probe to reject the same live reply and to name the planted key. Two
/// defects, because they exercise different keywords — a `required` the reply cannot satisfy, and item
/// 20's `unevaluatedProperties` closure catching a key the fragment forbids. No file on disk is touched.
#[test]
#[ignore = "dry-run scaffolding; run explicitly with AETHER_DRYRUN_SCHEMA set"]
fn the_probe_rejects_a_reply_the_candidate_forbids() {
    let doc = candidate();
    let h = spawn("dryrun-vac");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // The positive half first, or the two rejections below are equally consistent with rejecting always.
    match probe(&mut c, &doc, "emulator/status", json!({})) {
        Verdict::Pass => {}
        other => panic!(
            "the unmodified candidate must ACCEPT a conformant emulator/status reply, or nothing \
             below distinguishes a working validator from one that rejects everything: {} — {}",
            other.label(),
            other.detail()
        ),
    }

    // (1) A `required` key this server does not emit. The fragment now demands something real replies
    //     lack, which is precisely the shape of a fragment that would refuse a conformant server.
    let mut demanding = doc.clone();
    demanding["methods"]["emulator/status"]["result"]["required"]
        .as_array_mut()
        .expect("the status fragment lists `required`")
        .push(json!("aKeyNoServerEmits"));
    match probe(&mut c, &demanding, "emulator/status", json!({})) {
        Verdict::Refused(e) => assert!(
            e.iter().any(|f| f.contains("aKeyNoServerEmits")),
            "refused, but not for the planted reason — the probe may be failing for something \
             unrelated, which would make this control prove nothing: {e:#?}"
        ),
        other => panic!(
            "THE PROBE ACCEPTED A REPLY THE FRAGMENT FORBIDS. Every MEASURED/PASS in the table is \
             worthless if this can happen: {} — {}",
            other.label(),
            other.detail()
        ),
    }

    // (2) Item 20's closure, from the other side: narrow the fragment so a key the server really does
    //     emit becomes unevaluated. `pc` is on every `emulator/status` reply.
    let mut narrowed = doc.clone();
    narrowed["methods"]["emulator/status"]["result"]["properties"]
        .as_object_mut()
        .expect("the status fragment declares `properties`")
        .remove("pc")
        .expect("`pc` is declared by the status fragment");
    if let Some(req) = narrowed["methods"]["emulator/status"]["result"]["required"].as_array_mut() {
        req.retain(|k| k != "pc");
    }
    match probe(&mut c, &narrowed, "emulator/status", json!({})) {
        Verdict::Refused(e) => assert!(
            e.iter().any(|f| f.contains("pc")),
            "refused, but `pc` is not named — the closure may not be what caught it: {e:#?}"
        ),
        other => panic!(
            "item 20's closure did not bite: a key the fragment no longer declares was accepted on \
             the wire, so the whole `unevaluatedProperties` half of this instrument is inert: {} — {}",
            other.label(),
            other.detail()
        ),
    }
}

#[test]
#[ignore = "dry-run scaffolding; run explicitly with AETHER_DRYRUN_SCHEMA set"]
fn dry_run() {
    let doc = candidate();
    let vendored = common::schema::schema_root();

    let vendored_set = fragments_with_result(vendored);
    let candidate_set = fragments_with_result(&doc);
    let newly: Vec<String> = candidate_set.difference(&vendored_set).cloned().collect();
    let lost: Vec<String> = vendored_set.difference(&candidate_set).cloned().collect();
    let advertised: BTreeSet<&str> = METHODS.iter().map(|m| m.name).collect();

    println!("--- SCHEMA FRAGMENT DRY RUN ---");
    println!(
        "vendored fragments: {}   candidate fragments: {}   newly covered: {}   coverage LOST: {}",
        vendored_set.len(),
        candidate_set.len(),
        newly.len(),
        lost.len()
    );
    assert!(
        lost.is_empty(),
        "the candidate DROPS fragments the vendored copy has: {lost:?} — that is a regression in \
         coverage, not an addition, and it is not what this dry run was pointed at"
    );

    let h = spawn("dryrun");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // --- the control, FIRST. Everything below is only readable if this passes. ---
    println!("\n  POSITIVE CONTROLS (served here, covered by both schemas):");
    let mut controls_measured = 0usize;
    for m in POSITIVE_CONTROLS {
        assert!(
            advertised.contains(m) && candidate_set.contains(*m),
            "{m} is not a valid control: it must be both advertised and schematized"
        );
        let v = probe(&mut c, &doc, m, control_params(m));
        println!("    {:<34} {:<17} {}", m, v.label(), v.detail());
        if matches!(v, Verdict::Pass | Verdict::Refused(_)) {
            controls_measured += 1;
        }
    }
    assert_eq!(
        controls_measured,
        POSITIVE_CONTROLS.len(),
        "THE INSTRUMENT IS BROKEN, NOT THE SERVER. A control this server serves came back \
         unmeasured, which means every UNMEASURED row below describes this harness rather than the \
         method set. The table underneath proves nothing until this is green."
    );

    // --- the subject ---
    println!("\n  NEWLY COVERED BY THE CANDIDATE ({}):", newly.len());
    let mut pass = Vec::new();
    let mut refused = Vec::new();
    let mut unmeasured = Vec::new();
    let mut broken = Vec::new();
    for m in &newly {
        let served = advertised.contains(m.as_str());
        let v = probe(&mut c, &doc, m, json!({}));
        println!(
            "    {:<34} advertised={:<5} {:<17} {}",
            m,
            served,
            v.label(),
            v.detail()
        );
        match v {
            Verdict::Pass => pass.push(m.clone()),
            Verdict::Refused(e) => refused.push((m.clone(), e)),
            Verdict::Unmeasured(w) => unmeasured.push((m.clone(), w)),
            Verdict::FragmentBroken(e) => broken.push((m.clone(), e)),
        }
    }

    println!("\n  === ACCOUNTING ===");
    println!("    subject fragments      : {}", newly.len());
    println!("    MEASURED / PASS        : {}", pass.len());
    println!("    MEASURED / REFUSED     : {}", refused.len());
    println!("    FRAGMENT-BROKEN        : {}", broken.len());
    println!("    UNMEASURED             : {}", unmeasured.len());
    assert_eq!(
        pass.len() + refused.len() + broken.len() + unmeasured.len(),
        newly.len(),
        "every subject fragment lands in exactly one bucket"
    );

    for (m, e) in &refused {
        println!("\n  REFUSED {m}:");
        for line in e {
            println!("      {line}");
        }
    }
    for (m, e) in &broken {
        println!("\n  FRAGMENT-BROKEN {m}: {e}");
    }

    // **The loud half.** An unmeasured fragment is not a passing one, and this is the only place the
    // distinction can be enforced rather than hoped for: while anything is unmeasured this test is RED,
    // so its result can never be read as "the candidate is fine". The table above is the deliverable;
    // the failure below is what stops the table being mistaken for a clean bill of health.
    if !unmeasured.is_empty() {
        let names: Vec<&str> = unmeasured.iter().map(|(m, _)| m.as_str()).collect();
        panic!(
            "\n\n  *** {} OF {} CANDIDATE FRAGMENTS WERE NOT MEASURED — THIS IS NOT A PASS ***\n\n  \
             Nothing this server emits was ever checked against them, so this run says NOTHING about \
             whether they would accept or refuse a conformant reply. They are listed here rather than \
             counted as zero, because a skipped row read as a pass is the one output of this exercise \
             that would actively mislead.\n\n  UNREACHED ({}):\n    {}\n\n  Reasons:\n    {}\n",
            unmeasured.len(),
            newly.len(),
            names.len(),
            names.join("\n    "),
            unmeasured
                .iter()
                .map(|(m, w)| format!("{m}: {w}"))
                .collect::<Vec<_>>()
                .join("\n    "),
        );
    }
}
