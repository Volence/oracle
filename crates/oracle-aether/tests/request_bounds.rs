//! **The request side of the wire, differentially** — the server's duty to REFUSE what the vendored
//! fragments declare out of bounds, asserted against the fragments themselves rather than against a
//! hand-written table of what they say.
//!
//! # Why this file exists, and what the conformance suite structurally cannot see
//!
//! `schema_conformance.rs` validates **replies**. That is the right subject for a document schema and it
//! is only half the wire. A fragment's `params` block describes what a conformant CLIENT may send; the
//! obligation it puts on a SERVER is the mirror image — *refuse everything else* — and no amount of
//! validating outbound messages can observe whether an inbound one was refused. A schema cannot watch a
//! server ignore it.
//!
//! That gap has already shipped a defect. `emulator/step` enforced neither end of its `count` bounds for
//! ten days while the handler's own comment claimed it transcribed them verbatim. Handler, comment and
//! test agreed with each other; the fragment was correctly vendored; the suite was green. **Every
//! artifact healthy and the obligation unmet** — because nothing in the repo ever sent `count: 0` and
//! looked at what came back. The fix landed by hand. The hole it came out of did not close.
//!
//! # The shape, and why it is not a table
//!
//! Every bound probed here is **parsed out of `tests/contract/bus-protocol.schema.json` at test time**.
//! A hand-maintained list of 60 bounds is the same defect one level up: it is a second reading of the
//! authority, and it goes stale the first time a fragment moves without anyone noticing. So the walk
//! finds `minimum`/`maximum` wherever they sit — including nested under `rows/items` — and the probe
//! values are arithmetic on what it found, never literals typed here.
//!
//! What *is* hand-written is [`BASELINES`]: an otherwise-valid `params` object per method, so the probe
//! differs from a legal request in exactly the one field under test. That is fixture data, not contract
//! data, and it is guarded from the far side by [`the_in_bounds_control_is_never_refused_by_name`].
//!
//! # The three outcomes, kept apart on purpose
//!
//! For each bound, one request is sent carrying `min - 1` (or `max + 1`) in that field and nothing else
//! changed:
//!
//! * **REFUSED** — `-32602`, and the message NAMES the field. The obligation met, loudly. The naming is
//!   not decoration: a refusal that does not say which field it refused is indistinguishable, to the
//!   agent reading it, from a refusal about the machine's state, and the queue fills with plausible
//!   wrong answers. It is also this file's only defence against a false green, since a request refused
//!   for some *other* reason would otherwise read as a bound being enforced.
//! * **ACCEPTED** — the server answered. This is the `emulator/step` defect class, and it fails here.
//! * **UNMEASURED** — refused, but with a code that is not `-32602` (a state refusal, a missing ROM).
//!   The bound check never ran, so this file learned nothing about it. **It is not a pass**, it is
//!   printed by name, and the set of methods allowed to land here is declared in [`UNCOVERED`] with a
//!   reason each. A site that goes unmeasured outside that set fails the run.
//!
//! # The coverage ledger
//!
//! [`every_bounded_method_is_either_covered_or_declared_uncovered`] derives the bounded-method set from
//! the schema and requires it to equal `BASELINES ∪ UNCOVERED`, exactly. A new bounded fragment lands
//! red until someone rules on it, and the uncovered set is enumerable rather than implicit — which is
//! the only honest way to ship a partial differential.

mod common;

use common::{spawn_for_sweep, Client};
use oracle_aether::engine::METHODS;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------------------------------
// Reading the authority
// ---------------------------------------------------------------------------------------------------

/// The vendored schema as a document. Read directly, like `params_closure.rs` does and for the same
/// reason: the subject here is what the fragment *declares*, so this wants the JSON, not a compiled
/// validator over it.
fn schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/contract/bus-protocol.schema.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("read the vendored schema"))
        .expect("the vendored schema parses")
}

/// One step of the route from a `params` object down to a bounded scalar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seg {
    /// A named property.
    Key(String),
    /// The elements of an array. The probe writes element `0`, so a baseline for a method with an
    /// array-nested bound MUST carry at least one element (asserted, not assumed).
    Item,
}

/// A single declared bound: where it lives, what it is called on the wire, and its two ends.
#[derive(Debug, Clone)]
struct BoundSite {
    method: String,
    path: Vec<Seg>,
    /// The last `Key` on the path — the name a refusal is required to carry.
    field: String,
    min: Option<i64>,
    max: Option<i64>,
}

impl BoundSite {
    /// `params.rows[].start`, for printing.
    fn display_path(&self) -> String {
        let mut s = String::new();
        for seg in &self.path {
            match seg {
                Seg::Key(k) => {
                    if !s.is_empty() {
                        s.push('.');
                    }
                    s.push_str(k);
                }
                Seg::Item => s.push_str("[]"),
            }
        }
        s
    }
}

/// Walk a fragment's `params` subtree and collect every numeric bound under it.
///
/// `exclusiveMinimum`/`exclusiveMaximum` are collected too — the vendored schema uses neither today, and
/// a walk that silently skipped a keyword it did not expect is how a bound stops being tested without
/// anyone finding out. They are converted to their inclusive equivalents on integers.
fn collect(node: &Value, method: &str, path: &mut Vec<Seg>, out: &mut Vec<BoundSite>) {
    let Some(obj) = node.as_object() else { return };

    let num = |k: &str| obj.get(k).and_then(Value::as_i64);
    let min = num("minimum").or_else(|| num("exclusiveMinimum").map(|n| n + 1));
    let max = num("maximum").or_else(|| num("exclusiveMaximum").map(|n| n - 1));
    if min.is_some() || max.is_some() {
        let field = path
            .iter()
            .rev()
            .find_map(|s| match s {
                Seg::Key(k) => Some(k.clone()),
                Seg::Item => None,
            })
            .expect("a bound always sits under a named property");
        out.push(BoundSite {
            method: method.to_string(),
            path: path.clone(),
            field,
            min,
            max,
        });
    }

    if let Some(props) = obj.get("properties").and_then(Value::as_object) {
        for (k, sub) in props {
            path.push(Seg::Key(k.clone()));
            collect(sub, method, path, out);
            path.pop();
        }
    }
    if let Some(items) = obj.get("items") {
        path.push(Seg::Item);
        collect(items, method, path, out);
        path.pop();
    }
    // The composition keywords. None of them carries a bound in today's vendored schema; walking them
    // anyway means a fragment that later puts one inside a `oneOf` is probed rather than skipped.
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(arr) = obj.get(key).and_then(Value::as_array) {
            for sub in arr {
                collect(sub, method, path, out);
            }
        }
    }
}

/// Every declared request bound on every **advertised** method, derived from the vendored schema.
///
/// Fragments for methods this server does not advertise are excluded: there is no handler to hold to
/// them, and `-32601` is the correct answer to all of them. `emulator/audio_spectrum` is the one such
/// fragment carrying bounds today; `schema_dryrun.rs` already owns that class of gap.
fn declared_bounds() -> Vec<BoundSite> {
    let schema = schema();
    let advertised: BTreeSet<&str> = METHODS.iter().map(|m| m.name).collect();
    let mut out = Vec::new();
    let methods = schema["methods"]
        .as_object()
        .expect("`methods` is an object");
    for (name, frag) in methods {
        if !advertised.contains(name.as_str()) {
            continue;
        }
        let Some(params) = frag.get("params") else {
            continue;
        };
        let mut path = Vec::new();
        collect(params, name, &mut path, &mut out);
    }
    out.sort_by(|a, b| (&a.method, a.display_path()).cmp(&(&b.method, b.display_path())));
    out
}

// ---------------------------------------------------------------------------------------------------
// The fixture side: an otherwise-valid request per method
// ---------------------------------------------------------------------------------------------------

/// **A legal `params` object per bounded method**, into which the probe substitutes one out-of-bounds
/// value. Hand-written on purpose — this is fixture data, not contract data, and deriving a *valid*
/// request from a schema is a different and much larger problem than reading a bound out of one.
///
/// The risk a hand-written baseline carries is a stale one: if the rest of the object stops being legal,
/// every probe on that method starts being refused for the wrong reason, and a file that only looked for
/// "was it refused" would call that green forever. Two things stop that here. The by-name assertion
/// discriminates a bound refusal from any other refusal, and
/// [`the_in_bounds_control_is_never_refused_by_name`] re-sends each baseline with a LEGAL value in the
/// same field and requires that it is not refused by name.
///
/// Addresses are work RAM (`$FF….`) or the fixture ROM's reset region, matching the conventions in
/// `read.rs` and `memory_hash.rs`.
fn baseline(method: &str, field: &str) -> Option<Value> {
    let params = match method {
        "emulator/breakpoint_list" => json!({}),
        "emulator/checkpoint_list" => json!({}),
        "emulator/get_profiler_frames" => json!({}),
        "emulator/hold" => json!({"buttons": ["a"], "down": true, "port": 0}),
        "emulator/memory_hash" => json!({"addr": "0x00FF0500", "len": 8}),
        "emulator/pixel_attribution" => json!({"x": 0, "y": 0}),
        "emulator/play_input" => {
            json!({"rows": [{"start": 0, "end": 1, "buttons": ["a"]}], "maxFrames": 1})
        }
        "emulator/press" => json!({"buttons": ["a"], "frames": 1, "port": 0}),
        "emulator/read" => json!({"addr": "0x00FF0000", "len": 1}),
        "emulator/read_cram" => json!({"line": 0}),
        "emulator/read_memory" => json!({"addr": "0x00FF0000", "len": 1}),
        "emulator/read_vram" => json!({"addr": "0x0000", "len": 1}),
        "emulator/run_frames" => json!({"frames": 1}),
        "emulator/run_to" => json!({"addr": "0x00000200", "maxFrames": 1}),
        "emulator/run_to_scanline" => json!({"line": 0, "maxFrames": 1}),
        "emulator/scanlines" => json!({"startLine": 0, "count": 1}),
        "emulator/sprites" => json!({"limit": 1}),
        "emulator/step" => json!({"count": 1}),
        "emulator/wait_for_break" => json!({"timeoutMs": 0}),
        "emulator/watchpoint_add" => json!({"addr": "0x00FF0500", "len": 2, "stopAfter": 1}),
        "emulator/watchpoint_hits" => json!({}),
        "emulator/watchpoint_list" => json!({}),
        // `raw` and the `r`/`g`/`b` triple are alternative spellings of one colour, so the baseline for
        // a `raw` probe must not also carry components.
        "emulator/write_cram" => {
            if field == "raw" {
                json!({"line": 0, "index": 0, "raw": 0})
            } else {
                json!({"line": 0, "index": 0, "r": 0, "g": 0, "b": 0})
            }
        }
        // **`disp` needs a `symbol`, and the control is what proved it.** The first baseline written
        // here paired `disp` with `addr`, and the server answered
        // *"`disp` is valid only with `symbol`: with `addr` it is arithmetic the caller has already
        // done"* — `-32602`, naming `disp`. The out-of-bounds probe therefore came back REFUSED BY NAME
        // and the main differential went green **without ever reaching the bound**: a refusal about the
        // param's company, read as a refusal about its value. That is precisely the false green this
        // file exists to prevent, caught by the in-bounds control and by nothing else, which is the
        // strongest argument for keeping the control that could be made.
        "emulator/write_memory" if field == "disp" => {
            json!({"symbol": PROBE_SYMBOL, "disp": 0, "value": 0, "width": 1})
        }
        "emulator/write_memory" => json!({"addr": "0x00FF0500", "value": 0, "width": 1}),
        "emulator/z80_read" => json!({"addr": "0x0000", "len": 1}),
        "emulator/z80_write" => json!({"addr": "0x0000", "value": 0}),
        _ => return None,
    };
    Some(params)
}

/// **Methods whose bounds this differential does NOT probe, and why each.**
///
/// Declared rather than implicit: `every_bounded_method_is_either_covered_or_declared_uncovered`
/// requires this list plus [`baseline`]'s arms to be *exactly* the bounded set, so a new bounded
/// fragment cannot quietly land outside the differential. Each entry is a debt with an owner, not a
/// dismissal.
const UNCOVERED: &[(&str, &str)] = &[
    (
        "emulator/object_at",
        "the object surface needs a loaded act; on the test ROM every call is refused for state before \
         any bound is read, so a probe here measures nothing (F-OBJ-BOUNDS-UNPROBED)",
    ),
    (
        "emulator/object_delete",
        "same: needs a loaded act and a live slot (F-OBJ-BOUNDS-UNPROBED)",
    ),
    (
        "emulator/object_list",
        "same: needs a loaded act (F-OBJ-BOUNDS-UNPROBED)",
    ),
    (
        "emulator/object_move",
        "same: needs a loaded act and a live slot (F-OBJ-BOUNDS-UNPROBED)",
    ),
    (
        "emulator/object_slot",
        "same: needs a loaded act (F-OBJ-BOUNDS-UNPROBED)",
    ),
    (
        "emulator/object_spawn",
        "same: needs a loaded act and an archetype pointer (F-OBJ-BOUNDS-UNPROBED)",
    ),
];

/// **Bounds whose LEGAL end this file cannot send, because no conformant reply exists for it.**
///
/// One entry today, and finding it is the first thing this differential did that the reply-only suite
/// structurally could not. `emulator/z80_read`'s fragment declares `len` with `minimum: 0` — alone among
/// every `len` on the surface, which all floor at 1 — so `len: 0` is a request the contract **permits**.
/// The server serves it, and answers `bytes: "0x"`. Its own `result.bytes` is `$ref: #/$defs/hex`, whose
/// pattern is `^0x[0-9A-Fa-f]+$` and therefore requires at least one hex digit. So:
///
/// > `emulator/z80_read {"addr": "0x0000", "len": 0}` is a request the schema allows and for which **no
/// > reply the schema accepts exists**. The server is not free to refuse it (the fragment says 0 is in
/// > bounds) and is not able to answer it conformantly.
///
/// Nothing in the tree had ever sent it. Every `z80_read` test uses a positive length, so the reply
/// validator — which is a funnel on `Client::recv` and would have caught this instantly — was never
/// handed the shape. That is the shape of the whole H3 gap: **the suite was green because of what it
/// never asked, not because of what the server answered.**
///
/// This is a **contract** question, not a server one, and this parcel is deliberately hermetic (no live
/// peer read, no cross-repo edit). Both plausible fixes are the contract owner's: widen the `hex` pattern
/// to `^0x([0-9A-Fa-f]{2})*$` so the empty blob is spellable, or raise `z80_read.len`'s floor to 1 and
/// make 0 a refusal like its siblings. So the entry is REGISTERED here, in the idiom
/// `common::schema::KNOWN_CONTRACT_DIVERGENCES` already establishes for this repo — never silenced — and
/// [`the_registered_unserveable_bound_is_still_live`] fails the day it stops being true, so it cannot rot
/// after a re-vendor.
const KNOWN_UNSERVEABLE: &[(&str, &str, i64, &str)] = &[(
    "emulator/z80_read",
    "len",
    0,
    "F-Z80READ-LEN0-UNSERVEABLE: the fragment permits `len: 0`; the server answers `bytes: \"0x\"`; \
     `$defs/hex` requires >=1 hex digit. No conformant reply exists. Contract ruling needed.",
)];

// ---------------------------------------------------------------------------------------------------
// The probe
// ---------------------------------------------------------------------------------------------------

/// Write `value` at `path` inside `params`. Returns `Err` with a description if the baseline does not
/// have the shape the path needs — a stale baseline must be loud, never a skipped probe.
fn put(params: &mut Value, path: &[Seg], value: Value) -> Result<(), String> {
    let Some((head, rest)) = path.split_first() else {
        *params = value;
        return Ok(());
    };
    match head {
        Seg::Key(k) => {
            let obj = params
                .as_object_mut()
                .ok_or_else(|| format!("baseline is not an object at `{k}`"))?;
            if rest.is_empty() {
                obj.insert(k.clone(), value);
                return Ok(());
            }
            let sub = obj.get_mut(k).ok_or_else(|| {
                format!("baseline is missing `{k}`, which the bound path descends into")
            })?;
            put(sub, rest, value)
        }
        Seg::Item => {
            let arr = params.as_array_mut().ok_or_else(|| {
                "baseline is not an array where the bound path expects one".to_string()
            })?;
            let first = arr.first_mut().ok_or_else(|| {
                "baseline array is empty; a nested bound needs one element".to_string()
            })?;
            put(first, rest, value)
        }
    }
}

/// What one probe learned. Ordered from best to worst so a table sorts usefully.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// `-32602` and the message named the field. The obligation, met.
    RefusedByName,
    /// `-32602`, but the message never says which field. Loud, but not loud enough to act on.
    RefusedUnnamed(String),
    /// The server answered. This is the defect class.
    Accepted,
    /// Refused for something other than params. The bound check never ran.
    Unmeasured { code: i64, message: String },
}

fn classify(reply: &Value, field: &str) -> Outcome {
    let Some(err) = reply.get("error") else {
        return Outcome::Accepted;
    };
    let code = err["code"].as_i64().unwrap_or(0);
    let message = err["message"].as_str().unwrap_or("").to_string();
    if code != -32602 {
        return Outcome::Unmeasured { code, message };
    }
    // **Backticks required, and that is not style policing.** A bare substring match cannot be used
    // here: `write_cram`'s components are named `r`, `g` and `b`, and the letter `r` occurs in almost
    // every English sentence a server could emit. A loose match would call any refusal at all a
    // by-name refusal for those three fields — the exact vacuity this file exists to rule out. Every
    // refusal this server writes already spells the field in backticks (`hex::parse_count`,
    // `engine.rs`'s hand-rolled arms), so the strict form costs nothing and means something.
    if message.contains(&format!("`{field}`")) {
        Outcome::RefusedByName
    } else {
        Outcome::RefusedUnnamed(message)
    }
}

/// One probe end: a name for the report and the value to send.
struct Probe {
    end: &'static str,
    value: i64,
}

fn probes(site: &BoundSite) -> Vec<Probe> {
    let mut v = Vec::new();
    if let Some(min) = site.min {
        v.push(Probe {
            end: "below minimum",
            value: min - 1,
        });
    }
    if let Some(max) = site.max {
        v.push(Probe {
            end: "above maximum",
            value: max + 1,
        });
    }
    v
}

/// A fresh connection posed so that a bound check is the FIRST thing a probe can hit.
///
/// Two preconditions, and both were found by running this file rather than by reading the handlers:
///
/// * **Paused.** Bound checks in several handlers sit *behind* a `require_paused`, so a probe on a
///   running machine measures the state gate and reports UNMEASURED.
/// * **The profiler armed with every lens.** `emulator/get_profiler_frames` refuses `frames` and
///   `topCallers` with `-32005` when the sample was not armed with `perFrame`/`callers` — and it does so
///   *before* reading their values, so those two bounds were unmeasurable until the sample was armed.
///   §11.18's rule that every arming flag resets together is why all three go on in one call.
fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    // Already-paused is not an error worth failing on; the point is only that we are not running.
    let _ = c.call("emulator/pause", json!({}));
    c.ok(
        "emulator/set_profiler",
        json!({"enabled": true, "perFrame": true, "callers": true}),
    );
    load_probe_symbol(&mut c);
    c
}

/// The name `write_memory`'s `disp` baseline resolves against.
const PROBE_SYMBOL: &str = "Probe";

/// Load a one-symbol listing naming [`PROBE_SYMBOL`] in work RAM — the `params_closure.rs` fixture,
/// because `write_memory`'s `disp` is refused outright unless it travels with a `symbol`.
fn load_probe_symbol(c: &mut Client) {
    let lst =
        format!("  Symbol Table (* = unused):\n\n {PROBE_SYMBOL} : FF0600 C |\n\n   1 symbols\n");
    let dir = std::env::temp_dir().join(format!("oracle-rb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the symbol fixture dir");
    let path = dir.join("probe.lst");
    std::fs::write(&path, lst).expect("write the symbol fixture");
    c.ok(
        "emulator/load_symbols",
        json!({"path": path.to_str().unwrap()}),
    );
}

// ---------------------------------------------------------------------------------------------------
// The gates
// ---------------------------------------------------------------------------------------------------

/// **The differential.** Every declared bound on every covered method, probed one step outside each
/// end, must come back `-32602` naming the field.
#[test]
fn every_declared_bound_is_refused_by_name_one_step_outside() {
    let sites = declared_bounds();
    assert!(
        sites.len() >= 40,
        "the walk found only {} bounds; the vendored schema declares far more, so the walk is broken \
         rather than the schema being small",
        sites.len()
    );

    let h = spawn_for_sweep("rb-outside");
    let mut c = client(&h);

    let uncovered: BTreeSet<&str> = UNCOVERED.iter().map(|(m, _)| *m).collect();
    let mut failures: Vec<String> = Vec::new();
    let mut unmeasured: Vec<String> = Vec::new();
    let mut passed = 0usize;
    let mut skipped = 0usize;

    for site in &sites {
        if uncovered.contains(site.method.as_str()) {
            skipped += 1;
            continue;
        }
        let Some(base) = baseline(&site.method, &site.field) else {
            failures.push(format!(
                "{} {}: no baseline and not declared UNCOVERED",
                site.method,
                site.display_path()
            ));
            continue;
        };
        for probe in probes(site) {
            let mut params = base.clone();
            if let Err(why) = put(&mut params, &site.path, json!(probe.value)) {
                failures.push(format!(
                    "{} {} ({}): baseline cannot carry the probe: {why}",
                    site.method,
                    site.display_path(),
                    probe.end
                ));
                continue;
            }
            let reply = c.call(&site.method, params.clone());
            match classify(&reply, &site.field) {
                Outcome::RefusedByName => passed += 1,
                Outcome::Accepted => failures.push(format!(
                    "ACCEPTED {} {} = {} ({}): the fragment declares min={:?} max={:?} and the server \
                     answered instead of refusing. params = {params}",
                    site.method,
                    site.display_path(),
                    probe.value,
                    probe.end,
                    site.min,
                    site.max
                )),
                Outcome::RefusedUnnamed(msg) => failures.push(format!(
                    "UNNAMED {} {} = {} ({}): refused -32602 but the message never names the field: \
                     {msg:?}",
                    site.method,
                    site.display_path(),
                    probe.value,
                    probe.end
                )),
                Outcome::Unmeasured { code, message } => unmeasured.push(format!(
                    "UNMEASURED {} {} = {} ({}): refused {code}, not -32602 — the bound check never \
                     ran. {message:?}",
                    site.method,
                    site.display_path(),
                    probe.value,
                    probe.end
                )),
            }
        }
    }

    let report = format!(
        "{passed} bound probes refused by name; {} failed; {} unmeasured; {skipped} sites skipped as \
         declared UNCOVERED (of {} declared bound sites on advertised methods)\n\nFAILURES:\n{}\n\n\
         UNMEASURED (not a pass):\n{}",
        failures.len(),
        unmeasured.len(),
        sites.len(),
        if failures.is_empty() {
            "  (none)".to_string()
        } else {
            failures.join("\n")
        },
        if unmeasured.is_empty() {
            "  (none)".to_string()
        } else {
            unmeasured.join("\n")
        },
    );
    // Printed on the way past, not only on the way down: a gate whose totals are visible only when it
    // fails cannot be read as "how much is covered" on a green run, which is the number a reader of a
    // partial differential most needs.
    println!("\n=== request-bounds differential ===\n{report}");
    assert!(failures.is_empty() && unmeasured.is_empty(), "{report}");
    assert!(passed > 0, "no probe was measured at all: {report}");
}

/// **The anti-vacuity control.** The same baseline with a LEGAL value in the same field must NOT be
/// refused by name.
///
/// Without it, a handler that rejected `count` unconditionally — or a baseline that had gone stale in
/// some *other* field and was being refused for that instead — would pass the differential above while
/// proving nothing. The legal value used is the fragment's own `minimum` wherever there is one, because
/// the maximum end of several of these bounds (`step` at a million instructions, `memory_hash` at four
/// megabytes, `wait_for_break` at five minutes) is a legal request this suite should not be making.
#[test]
fn the_in_bounds_control_is_never_refused_by_name() {
    let sites = declared_bounds();
    let h = spawn_for_sweep("rb-inside");
    let mut c = client(&h);
    let uncovered: BTreeSet<&str> = UNCOVERED.iter().map(|(m, _)| *m).collect();
    let mut failures = Vec::new();
    let mut checked = 0usize;

    for site in &sites {
        if uncovered.contains(site.method.as_str()) {
            continue;
        }
        let Some(base) = baseline(&site.method, &site.field) else {
            continue;
        };
        let legal = match (site.min, site.max) {
            (Some(min), _) => min,
            (None, Some(max)) => max,
            (None, None) => continue,
        };
        if let Some((_, _, _, why)) = KNOWN_UNSERVEABLE
            .iter()
            .find(|(m, f, v, _)| *m == site.method && *f == site.field && *v == legal)
        {
            println!(
                "  control SKIPPED at {} {} = {legal} — {why}",
                site.method,
                site.display_path()
            );
            continue;
        }
        let mut params = base.clone();
        if let Err(why) = put(&mut params, &site.path, json!(legal)) {
            failures.push(format!("{} {}: {why}", site.method, site.display_path()));
            continue;
        }
        let reply = c.call(&site.method, params.clone());
        checked += 1;
        if let Outcome::RefusedByName = classify(&reply, &site.field) {
            failures.push(format!(
                "{} {} = {legal} is INSIDE the fragment's bounds ({:?}..={:?}) and was still refused by \
                 name: {} — either the handler is stricter than the contract, or this file's probe of \
                 that field proves nothing. params = {params}",
                site.method,
                site.display_path(),
                site.min,
                site.max,
                reply["error"]["message"]
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {checked} in-bounds controls were refused by name:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked > 0, "the control measured nothing");
}

/// **The coverage ledger.** The bounded-method set is derived from the schema; `BASELINES ∪ UNCOVERED`
/// must equal it exactly, in both directions.
///
/// A bounded method in neither is a fragment nobody ruled on. A method in `UNCOVERED` that no longer
/// carries a bound is a debt still being paid on a closed account, and it hides the fact that the
/// differential's boundary moved.
#[test]
fn every_bounded_method_is_either_covered_or_declared_uncovered() {
    let bounded: BTreeSet<String> = declared_bounds().into_iter().map(|s| s.method).collect();
    let declared_uncovered: BTreeSet<String> =
        UNCOVERED.iter().map(|(m, _)| m.to_string()).collect();
    let baselined: BTreeSet<String> = bounded
        .iter()
        .filter(|m| baseline(m, "").is_some())
        .cloned()
        .collect();

    let unruled: Vec<&String> = bounded
        .iter()
        .filter(|m| !baselined.contains(*m) && !declared_uncovered.contains(*m))
        .collect();
    assert!(
        unruled.is_empty(),
        "these methods declare request bounds and are neither probed nor declared UNCOVERED — a \
         fragment landed and nobody ruled on it: {unruled:?}"
    );

    let stale: Vec<&String> = declared_uncovered
        .iter()
        .filter(|m| !bounded.contains(*m))
        .collect();
    assert!(
        stale.is_empty(),
        "these methods are declared UNCOVERED but no longer declare any request bound; the \
         differential's boundary moved and the note did not: {stale:?}"
    );

    let both: Vec<&String> = declared_uncovered
        .iter()
        .filter(|m| baselined.contains(*m))
        .collect();
    assert!(
        both.is_empty(),
        "declared UNCOVERED and also baselined — one of the two is wrong: {both:?}"
    );
}

/// **The boundary, printed.** Not an assertion about the server: a report of what this file covers, so
/// the differential's own edge is readable without reading its source.
///
/// Run with `--nocapture` to see it. It asserts only the two facts a reader would otherwise have to
/// take on trust: that the walk found bounds on more than one method, and that every site it found
/// belongs to a method that is either covered or declared.
#[test]
fn the_covered_surface_is_enumerable() {
    let sites = declared_bounds();
    let uncovered: BTreeMap<&str, &str> = UNCOVERED.iter().copied().collect();
    let mut by_method: BTreeMap<&str, Vec<&BoundSite>> = BTreeMap::new();
    for s in &sites {
        by_method.entry(s.method.as_str()).or_default().push(s);
    }

    let mut covered_sites = 0;
    let mut uncovered_sites = 0;
    println!("\n=== request-bounds differential: covered surface ===");
    for (method, sites) in &by_method {
        let note = uncovered.get(method);
        for s in sites {
            let ends = format!("min={:?} max={:?}", s.min, s.max);
            match note {
                None => {
                    covered_sites += 1;
                    println!("  COVERED   {method} {} {ends}", s.display_path());
                }
                Some(why) => {
                    uncovered_sites += 1;
                    println!("  UNCOVERED {method} {} {ends} — {why}", s.display_path());
                }
            }
        }
    }
    println!(
        "=== {covered_sites} covered / {uncovered_sites} uncovered, {} sites over {} methods ===\n",
        sites.len(),
        by_method.len()
    );

    assert!(by_method.len() > 1, "the walk found bounds on one method");
    for method in by_method.keys() {
        assert!(
            uncovered.contains_key(method) || baseline(method, "").is_some(),
            "{method} is neither covered nor declared"
        );
    }
}

/// **Anti-rot for [`KNOWN_UNSERVEABLE`].** The registered entry must STILL be unserveable.
///
/// An allowance that outlives its divergence does not merely go stale — `common::schema`'s own history
/// records it starting to *cause* the failure it was written to suppress. So the day the contract widens
/// `$defs/hex`, or raises `z80_read.len`'s floor, this test goes red and the skip above must be deleted.
///
/// The reply validator lives inside `Client::recv` and reports by panicking, so the observation is made
/// under `catch_unwind` — the same instrument `handshake.rs` uses for the same reason. A fresh client,
/// because the panic happens after the offending line has been taken off the socket.
#[test]
fn the_registered_unserveable_bound_is_still_live() {
    for (method, field, value, why) in KNOWN_UNSERVEABLE {
        let h = spawn_for_sweep("rb-unserveable");
        let mut c = client(&h);
        let base = baseline(method, field).expect("a registered entry names a baselined method");
        let mut params = base.clone();
        put(&mut params, &[Seg::Key((*field).to_string())], json!(value))
            .expect("the registered field is a top-level param");

        // The default hook would print a full panic banner for something this test EXPECTS; quiet it for
        // the duration and put it back, so a real panic elsewhere still reports normally.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            c.call(method, params.clone())
        }));
        std::panic::set_hook(prev);

        assert!(
            outcome.is_err(),
            "{method} {field} = {value} is registered as unserveable and the server now answers it \
             conformantly. If the contract was amended, DELETE the KNOWN_UNSERVEABLE entry and the \
             control's skip — an allowance that outlives its divergence starts causing failures of its \
             own. Registered as: {why}\n  reply = {:?}",
            outcome.ok()
        );
    }
}

/// **A hard-wired witness that the walk reads the document.**
///
/// Everything else in this file is derived, which is the point — and it also means a walk that returned
/// an empty list, or read the wrong subtree, would make every other test in the file vacuously green.
/// This one pins the shape of the answer against the fragment for `emulator/step`, the row whose ten-day
/// unenforced `count` is the reason the file exists. If the schema moves this row, this is the test that
/// says so, and it says it about one named row rather than about a total.
#[test]
fn the_walk_finds_the_step_count_bound_the_defect_was_about() {
    let sites = declared_bounds();
    let step: Vec<&BoundSite> = sites
        .iter()
        .filter(|s| s.method == "emulator/step")
        .collect();
    assert_eq!(
        step.len(),
        1,
        "expected exactly one bounded field on emulator/step, found {step:?}"
    );
    assert_eq!(step[0].field, "count");
    assert!(
        step[0].min.is_some() && step[0].max.is_some(),
        "the fragment declares both ends of `count`; the walk found {:?}",
        step[0]
    );

    // And the derived probe values really are one step outside, not the bound itself.
    let p = probes(step[0]);
    assert_eq!(p.len(), 2);
    assert_eq!(p[0].value, step[0].min.unwrap() - 1);
    assert_eq!(p[1].value, step[0].max.unwrap() + 1);
}
