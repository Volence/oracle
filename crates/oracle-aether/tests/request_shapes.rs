//! **The non-numeric half of the request side** — `required`, `enum`, `minLength`, `pattern`,
//! `minItems`, and the `oneOf` disjunctions — asserted against the vendored fragments themselves.
//!
//! # What this adds to `request_bounds.rs`, and why it is a second file
//!
//! `request_bounds.rs` already closes the numeric half: it walks the vendored schema for
//! `minimum`/`maximum` and probes one step outside each end. That is **97 of the surface's declared
//! request obligations** and it is the precedent this file follows in every respect — derive from the
//! fragment, never transcribe; three outcomes kept apart; a coverage ledger that fails on an unruled
//! fragment; an in-bounds control against a false green.
//!
//! What it does not touch is everything that is not a number. Measured against the vendored
//! `tests/contract/bus-protocol.schema.json` on 2026-09-09, with `$ref`s resolved: **127 non-numeric
//! obligations** — 68 `required` names, 24 `pattern`, 22 `minLength`, 12 `enum`, 1 `minItems`. Nothing in
//! the tree asked the server to refuse outside any of them. This file does.
//!
//! It is a separate binary rather than more rows in `request_bounds.rs` because the *probe* is a
//! different shape. A numeric bound has an arithmetic neighbour — `min - 1` — and one line of code
//! produces it. A `pattern` has no such neighbour, and the way this file gets one is the subject of
//! [`illegal_value`]: it does not reason about the regex at all, it hands candidates to the **contract's
//! own compiled validator** and keeps the first one the fragment rejects. A probe value is therefore
//! proven out-of-contract by the same engine that judges conformance, never by a comment claiming it is.
//!
//! # The `oneOf` trap, which is the reason this file was dry-run before it was wired
//!
//! 36 of the 68 `required` names do **not** sit at the top of a `params` object. They sit inside `oneOf`
//! branches:
//!
//! ```text
//! "oneOf": [ { "required": ["addr"] }, { "required": ["symbol"] } ]
//! ```
//!
//! `addr` is listed as `required` there, and omitting it is **completely legal** — that is what the
//! disjunction says. A gate that walked for the keyword `required` and demanded a refusal for each name
//! it found would have demanded 36 refusals a **conformant** server must not give, and every one of them
//! would have presented as the server being broken. So the walk classifies by *context*, not by keyword:
//! an unconditional `required` becomes [`Kind::Required`], and a `oneOf` group becomes one
//! [`Kind::DisjunctionNone`] site testing the obligation the group actually carries — *supply none of the
//! branches and you are refused*. See [`the_oneof_branches_are_not_probed_as_plain_required`], which pins
//! that distinction so it cannot be flattened by a later edit.
//!
//! # The three outcomes, and the third one
//!
//! `request_bounds.rs` has four verdicts and no way to say **"the probe never finished"** — a hung read
//! panics out of `Client::recv` and takes the whole sweep with it. For a *refusal* differential that gap
//! is the dangerous one, because **"the server refused this" and "the probe never came back" both look
//! exactly like the absence of a successful reply.** Fold them together and a hang scores as the
//! obligation being met.
//!
//! So every probe here resolves to exactly one of:
//!
//! * **REFUSED** — `-32602`, message naming the offending key in backticks. The obligation, met.
//! * **ANSWERED** — a success reply to an out-of-contract request. The defect class. Fails the gate.
//! * **DID-NOT-FINISH** — [`common::Settled::DidNotFinish`]. No observation was made. **Never green**,
//!   printed by name, and it fails the gate rather than being absorbed by it.
//! * **UNMEASURED** — refused, but not for params (`-32601` unserved, a state refusal). The obligation
//!   was never reached; declared per method in [`UNCOVERED`] or it fails.
//!
//! [`a_probe_that_never_finishes_is_reported_as_such_and_never_as_a_refusal`] is the positive control for
//! the third: it drives a real server-side block and requires the verdict to be DID-NOT-FINISH.
//!
//! # What the request side is now covered by, and the two families STILL uncovered
//!
//! Written down because a differential's blind spots are invisible by construction, and because a green
//! run across three files reads like a closed surface when it is not one:
//!
//! | obligation family | count | covered by |
//! |---|---|---|
//! | `minimum` / `maximum` | 97 | `request_bounds.rs` |
//! | undeclared key refused (`unevaluatedProperties: false`) | 70 params objects | `params_closure.rs` |
//! | unconditional `required` | 30 | **this file** |
//! | `pattern` | 24 | **this file** (each hex site probed twice since M25: [`empty_payload_value`]) |
//! | `minLength` | 21 | **this file** |
//! | `oneOf` disjunction (supply none) | 17 | **this file** |
//! | `enum` | 10 | **this file** |
//! | `minItems` | 1 | **this file** |
//! | `dependentRequired` | 6 | **NOBODY** |
//! | `if`/`then` | 2 | **NOBODY** |
//!
//! The two uncovered families are conditional obligations, and they are not covered because the walk
//! here is unconditional by construction — it mutates one site of an otherwise-legal baseline, and a
//! conditional obligation is a statement about a COMBINATION of keys:
//!
//! * **`dependentRequired` (6).** `write_cram` declares `{r: [g,b], g: [r,b], b: [r,g]}` — the colour
//!   components travel as a trio — and `write_memory` declares `{value: [width], width: [value],
//!   disp: [symbol]}`. The obligation is *"this key present without its companions is refused"*, which
//!   needs a mutation that REMOVES a sibling of the key under test rather than touching the key itself.
//! * **`if`/`then` (2).** `watchpoint_add` declares, twice and symmetrically, that `censusKey` and
//!   `mode: "census"` imply each other. This server does enforce it — the refusal *"`censusKey` is only
//!   meaningful with `mode: \"census\"`"* was measured while building [`baseline`], which is the only
//!   reason it is known at all, and it is measured by nothing that would notice if it stopped.
//!
//! Both are tractable extensions of the same machinery — another [`Kind`] and another [`Mutation`] each
//! — and neither is done here. `F-COND-OBLIGATIONS-UNPROBED`.

mod common;

use common::{spawn_for_sweep, Client, Settled};
use oracle_aether::engine::METHODS;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

// ---------------------------------------------------------------------------------------------------
// Reading the authority
// ---------------------------------------------------------------------------------------------------

fn schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/contract/bus-protocol.schema.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("read the vendored schema"))
        .expect("the vendored schema parses")
}

/// Resolve a `$ref` against `$defs`, carrying any sibling keywords, and following chains.
///
/// The vendored schema puts the whole string surface behind `$defs` — `hex`, `symbolName`, `handle` —
/// so a walk that did not deref would find **zero** `pattern`s and report a clean sweep of nothing. The
/// `seen` set stops a cyclic `$ref` from hanging the walk rather than failing it.
fn deref(node: &Value, defs: &Value, seen: &mut Vec<String>) -> Value {
    let Some(r) = node.get("$ref").and_then(Value::as_str) else {
        return node.clone();
    };
    let key = r
        .strip_prefix("#/$defs/")
        .unwrap_or_else(|| panic!("the walk only understands `#/$defs/` refs, got {r:?}"));
    if seen.iter().any(|s| s == key) {
        return json!({});
    }
    seen.push(key.to_string());
    let target = defs
        .get(key)
        .unwrap_or_else(|| panic!("`{r}` names a $def the schema does not have"));
    let mut merged = target.clone();
    if let (Some(m), Some(o)) = (merged.as_object_mut(), node.as_object()) {
        for (k, v) in o {
            if k != "$ref" {
                m.insert(k.clone(), v.clone());
            }
        }
    }
    deref(&merged, defs, seen)
}

/// One step of the route from a `params` object down to a constrained value. Mirrors
/// `request_bounds.rs`'s type of the same name; the two files walk the same document for different
/// keywords.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seg {
    Key(String),
    /// The elements of an array. The probe writes element `0`.
    Item,
}

fn display_path(path: &[Seg]) -> String {
    let mut s = String::from("params");
    for seg in path {
        match seg {
            Seg::Key(k) => {
                s.push('.');
                s.push_str(k);
            }
            Seg::Item => s.push_str("[]"),
        }
    }
    s
}

/// Which obligation a site carries. The name is what gets printed, and it is also what decides the
/// mutation — there is no separate table saying "for an enum, do this".
#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// An unconditional `required` name: removing it must be refused.
    Required,
    /// A closed value set: a value outside it must be refused.
    Enum,
    /// A string floor: a shorter string must be refused.
    MinLength,
    /// A regex: a non-matching string must be refused.
    Pattern,
    /// An array floor: a shorter array must be refused.
    MinItems,
    /// A `oneOf` group of `required` branches: supplying **none** of them must be refused.
    DisjunctionNone,
}

impl Kind {
    fn tag(&self) -> &'static str {
        match self {
            Kind::Required => "required",
            Kind::Enum => "enum",
            Kind::MinLength => "minLength",
            Kind::Pattern => "pattern",
            Kind::MinItems => "minItems",
            Kind::DisjunctionNone => "oneOf/none",
        }
    }
}

/// What the probe does to the legal baseline to put it out of contract.
#[derive(Debug, Clone)]
enum Mutation {
    /// Delete the key at this path.
    Remove(Vec<Seg>),
    /// Delete every one of these keys (a disjunction's whole branch set).
    RemoveAll(Vec<Vec<Seg>>),
    /// Write an illegal value at this path.
    Set(Vec<Seg>, Value),
}

/// A single declared, machine-checkable request obligation.
#[derive(Debug, Clone)]
struct ShapeSite {
    method: String,
    kind: Kind,
    /// The name a refusal is required to carry, in backticks.
    field: String,
    /// For printing.
    display: String,
    mutation: Mutation,
}

/// **Derive an illegal value for a constrained subschema — by asking the contract, not by reasoning.**
///
/// This is the load-bearing function of the file. For a numeric bound the out-of-contract neighbour is
/// arithmetic; for `pattern` there is no arithmetic, and the obvious move — write a string that *looks*
/// wrong and assert the server refuses it — has a silent failure mode: if the candidate happens to
/// SATISFY the pattern, the probe sends a legal request, the server correctly answers it, and the gate
/// reports the server ANSWERED an out-of-bounds request. A false red, blamed on the server.
///
/// So no candidate is trusted on inspection. Each is validated against a one-keyword schema built from
/// the fragment's own subschema, using the same `jsonschema` engine `common::schema` judges replies
/// with. The first candidate the contract **rejects** is the probe value; if the contract accepts every
/// candidate, this returns `None` and the site is reported UNDERIVABLE — loudly, never skipped.
///
/// Candidate order is deliberate: structurally-wrong-but-non-empty comes before empty, so a `pattern`
/// site is probed with something that violates the *pattern* rather than with `""`, which would also
/// violate a `minLength` and blur the two obligations into one.
fn illegal_value(sub: &Value, kind: &Kind) -> Option<Value> {
    let keyword = match kind {
        Kind::Enum => "enum",
        Kind::MinLength => "minLength",
        Kind::Pattern => "pattern",
        Kind::MinItems => "minItems",
        _ => return None,
    };
    let mut one = serde_json::Map::new();
    one.insert(keyword.to_string(), sub.get(keyword)?.clone());
    let one = Value::Object(one);
    let validator = jsonschema::validator_for(&one).ok()?;
    const CANDIDATES: &[fn() -> Value] = &[
        || json!("zzz+$FF"),
        || json!("not-a-legal-value"),
        || json!(""),
        || json!(-1),
        || json!(4294967296i64),
        || json!([]),
    ];
    CANDIDATES
        .iter()
        .map(|f| f())
        .find(|c| !validator.is_valid(c))
}

/// **The second probe on a `pattern` site: the empty payload, `"0x"`** (lens row M25).
///
/// [`illegal_value`] keeps ONE violating candidate per site, and one candidate cannot see every way a
/// pattern can be broken. `"zzz+$FF"` breaks every hex pattern at its prefix, so a handler that checks
/// the prefix refuses it — and the probe never meets the other violation those patterns carry: a body of
/// zero digits. `^0x[0-9A-Fa-f]+$` and `^0x([0-9A-Fa-f]{2})+$` both reject `"0x"` by their `+` alone.
/// That is how `emulator/z80_write` came to answer `bytes: "0x"` while `write_memory` and `write_vram`
/// refuse it: nothing in the tree ever sent it.
///
/// Derived exactly like the first probe — the contract's own validator decides, never a reading of the
/// regex. A pattern that accepts `"0x"` (`$defs/symbolName` does: it is a legal name) gets no second
/// probe, and a site whose first probe already is `"0x"` gets no duplicate.
fn empty_payload_value(sub: &Value) -> Option<Value> {
    let mut one = serde_json::Map::new();
    one.insert("pattern".to_string(), sub.get("pattern")?.clone());
    let validator = jsonschema::validator_for(&Value::Object(one)).ok()?;
    let candidate = json!("0x");
    (!validator.is_valid(&candidate)).then_some(candidate)
}

/// Walk a fragment's `params` subtree and collect every non-numeric obligation under it.
///
/// `in_branch` is the whole `oneOf` correction: a `required` reached through a `oneOf`/`anyOf` arm is
/// **not** an obligation to refuse on omission, and is not collected as one.
#[allow(clippy::too_many_arguments)]
fn collect(
    node: &Value,
    defs: &Value,
    method: &str,
    path: &mut Vec<Seg>,
    in_branch: bool,
    out: &mut Vec<ShapeSite>,
) {
    let node = deref(node, defs, &mut Vec::new());
    let Some(obj) = node.as_object() else { return };

    let field_name = || match path.iter().rev().find_map(|s| match s {
        Seg::Key(k) => Some(k.clone()),
        Seg::Item => None,
    }) {
        Some(k) => k,
        None => "params".to_string(),
    };

    for kind in [Kind::Enum, Kind::MinLength, Kind::Pattern, Kind::MinItems] {
        let kw = kind.tag();
        if obj.contains_key(kw) {
            if let Some(bad) = illegal_value(&node, &kind) {
                let empty = match kind {
                    Kind::Pattern => empty_payload_value(&node).filter(|e| *e != bad),
                    _ => None,
                };
                out.push(ShapeSite {
                    method: method.to_string(),
                    field: field_name(),
                    display: display_path(path),
                    mutation: Mutation::Set(path.clone(), bad),
                    kind,
                });
                if let Some(e) = empty {
                    out.push(ShapeSite {
                        method: method.to_string(),
                        field: field_name(),
                        display: format!("{} = {e}", display_path(path)),
                        mutation: Mutation::Set(path.clone(), e),
                        kind: Kind::Pattern,
                    });
                }
            } else {
                // Loud, not skipped: a keyword whose violation this file cannot construct is a hole, and
                // it is reported as UNDERIVABLE by `every_declared_obligation_is_refused_by_name`.
                out.push(ShapeSite {
                    method: method.to_string(),
                    field: field_name(),
                    display: display_path(path),
                    mutation: Mutation::Set(path.clone(), Value::Null),
                    kind,
                });
            }
        }
    }

    // `required`, but only where it is unconditional.
    if let Some(req) = obj.get("required").and_then(Value::as_array) {
        if !in_branch {
            for r in req.iter().filter_map(Value::as_str) {
                let mut p = path.clone();
                p.push(Seg::Key(r.to_string()));
                out.push(ShapeSite {
                    method: method.to_string(),
                    kind: Kind::Required,
                    field: r.to_string(),
                    display: display_path(&p),
                    mutation: Mutation::Remove(p),
                });
            }
        }
    }

    // A `oneOf`/`anyOf` whose arms are `required` groups: one site for the whole group.
    for comb in ["oneOf", "anyOf"] {
        let Some(arms) = obj.get(comb).and_then(Value::as_array) else {
            continue;
        };
        let mut branch_keys: Vec<Vec<Seg>> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for arm in arms {
            let arm = deref(arm, defs, &mut Vec::new());
            for r in arm
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let mut p = path.clone();
                p.push(Seg::Key(r.to_string()));
                branch_keys.push(p);
                names.push(r.to_string());
            }
        }
        if branch_keys.len() >= 2 {
            out.push(ShapeSite {
                method: method.to_string(),
                kind: Kind::DisjunctionNone,
                // The refusal must name at least one of the alternatives; `field` carries the first and
                // `classify` is handed the whole set.
                field: names.join("|"),
                display: format!("{} one of {:?}", display_path(path), names),
                mutation: Mutation::RemoveAll(branch_keys),
            });
        }
        // The arms are still descended into, for the `pattern`/`minLength` they carry — but with
        // `in_branch` set, so their `required` is not re-collected as an obligation.
        for (i, arm) in arms.iter().enumerate() {
            let _ = i;
            collect(arm, defs, method, path, true, out);
        }
    }

    for (k, v) in obj
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        path.push(Seg::Key(k.clone()));
        collect(v, defs, method, path, in_branch, out);
        path.pop();
    }
    if let Some(items) = obj.get("items") {
        path.push(Seg::Item);
        collect(items, defs, method, path, in_branch, out);
        path.pop();
    }
    for arm in obj
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect(arm, defs, method, path, in_branch, out);
    }
}

/// Every non-numeric obligation the vendored schema declares, on methods this server advertises.
fn declared_obligations() -> Vec<ShapeSite> {
    let doc = schema();
    let defs = doc["$defs"].clone();
    let mut out = Vec::new();
    let methods = doc["methods"].as_object().expect("methods is an object");
    for (name, frag) in methods {
        let Some(params) = frag.get("params") else {
            continue;
        };
        collect(params, &defs, name, &mut Vec::new(), false, &mut out);
    }
    out.retain(|s| METHODS.iter().any(|m| m.name == s.method));
    out.sort_by(|a, b| {
        (&a.method, a.kind.tag(), &a.display).cmp(&(&b.method, b.kind.tag(), &b.display))
    });
    out.dedup_by(|a, b| {
        a.method == b.method && a.kind == b.kind && a.display == b.display && a.field == b.field
    });
    out
}

// ---------------------------------------------------------------------------------------------------
// The fixture side
// ---------------------------------------------------------------------------------------------------

/// The name the symbol-bearing baselines resolve against.
const PROBE_SYMBOL: &str = "Probe";

/// **Live, server-issued handles, because a made-up one is refused before the obligation is reached.**
///
/// The first version of this file spelled `"bp-1"`/`"wp-1"` into the baselines and the in-bounds control
/// caught it immediately: `watchpoint_hits`' `cursor` probe came back `-32602` naming **`watch`** — *"is
/// not a handle this server issued"* — so the refusal was about the fixture's fake handle and the
/// `cursor` obligation was never read. It would have counted as a refusal by any check looser than the
/// by-name one.
///
/// So the handles are obtained the way a client obtains them, from the methods that mint them.
struct Handles {
    breakpoint: String,
    watch: String,
    checkpoint: String,
}

impl Handles {
    /// **Structure only, for the coverage ledger, which asks a question about the code and not about a
    /// server.** [`every_obligated_method_is_covered_or_declared`] wants to know whether [`baseline`]
    /// *has an arm* for a method — a property of the match, not of any handle's value — and spinning a
    /// server to answer it would make a static question depend on a live machine. These strings must
    /// therefore never reach the wire; nothing that takes a `placeholder` sends anything.
    fn placeholder() -> Self {
        Self {
            breakpoint: String::new(),
            watch: String::new(),
            checkpoint: String::new(),
        }
    }

    fn mint(c: &mut Client) -> Self {
        let breakpoint = c.ok("emulator/breakpoint_add", json!({"addr": "0x00000200"}))
            ["breakpoint"]
            .as_str()
            .expect("breakpoint_add returns a handle")
            .to_string();
        let watch = c.ok(
            "emulator/watchpoint_add",
            json!({"addr": "0x00FF0500", "len": 2, "stopAfter": 1}),
        )["watch"]
            .as_str()
            .expect("watchpoint_add returns a handle")
            .to_string();
        let checkpoint = c.ok("emulator/checkpoint", json!({}))["id"]
            .as_str()
            .expect("checkpoint returns an id")
            .to_string();
        Self {
            breakpoint,
            watch,
            checkpoint,
        }
    }
}

/// **An otherwise-legal `params` object per method**, so a probe differs from a conformant request in
/// exactly the one place under test.
///
/// Hand-written, like `request_bounds.rs`'s equivalent and for the same reason: this is fixture data,
/// not contract data. Deriving a *valid* request from a schema is a different and much larger problem
/// than reading a constraint out of one. It is guarded from the far side by
/// [`the_baselines_are_legal_requests`], which sends each one untouched and requires it is not refused
/// for params — so a baseline that stops being legal is loud instead of silently making every probe on
/// its method refuse for the wrong reason.
fn baseline(method: &str, field: &str, h: &Handles) -> Option<Value> {
    let p = match method {
        "emulator/breakpoint_add" => json!({"addr": "0x00000200"}),
        // `all` and `breakpoint` are the two arms; the baseline carries whichever the site is about, so
        // a `minLength` probe on `breakpoint` is not also a disjunction violation.
        "emulator/breakpoint_clear" if field.contains("breakpoint") => {
            json!({"breakpoint": h.breakpoint.clone()})
        }
        "emulator/breakpoint_clear" => json!({"all": true}),
        "emulator/breakpoint_list" => json!({"cursor": h.breakpoint.clone()}),
        "emulator/breakpoint_set_enabled" => {
            json!({"breakpoint": h.breakpoint.clone(), "enabled": true})
        }
        "emulator/checkpoint_drop" if field.contains("id") => json!({"id": h.checkpoint.clone()}),
        "emulator/checkpoint_drop" => json!({"all": true}),
        "emulator/checkpoint_list" => json!({"cursor": h.checkpoint.clone()}),
        "emulator/hold" => json!({"buttons": ["a"], "down": true, "port": 0}),
        "emulator/load_symbols" => json!({"path": "/nonexistent/probe.lst"}),
        "emulator/lookup_equate" if field.contains("prefix") => json!({"prefix": "P"}),
        "emulator/lookup_equate" => json!({"name": PROBE_SYMBOL}),
        "emulator/lookup_symbol" if field.contains("addr") => json!({"addr": "0x00FF0600"}),
        "emulator/lookup_symbol" => json!({"name": PROBE_SYMBOL}),
        "emulator/memory_hash" if field.contains("symbol") => {
            json!({"symbol": PROBE_SYMBOL, "len": 8})
        }
        "emulator/memory_hash" => json!({"addr": "0x00FF0500", "len": 8}),
        "emulator/object_at" => json!({"x": 0, "y": 0}),
        "emulator/object_delete" if field.contains("slot") => json!({"slot": 0}),
        "emulator/object_delete" => json!({"handle": "0x00FFB000"}),
        "emulator/object_list" => json!({"fields": ["id"]}),
        "emulator/object_move" if field.contains("slot") => json!({"slot": 0, "x": 0, "y": 0}),
        "emulator/object_move" => json!({"handle": "0x00FFB000", "x": 0, "y": 0}),
        "emulator/object_slot" => json!({"slot": 0, "fields": ["id"]}),
        "emulator/object_spawn" if field == "defSymbol" => {
            json!({"defSymbol": PROBE_SYMBOL, "x": 0, "y": 0})
        }
        "emulator/object_spawn" => json!({"def": "0x00000200", "x": 0, "y": 0}),
        "emulator/pixel_attribution" => json!({"x": 0, "y": 0}),
        "emulator/play_input" => {
            json!({"rows": [{"start": 0, "end": 1, "buttons": ["a"], "port": 0}], "maxFrames": 1})
        }
        "emulator/player_state" => json!({"fields": ["x"]}),
        "emulator/press" => json!({"buttons": ["a"], "frames": 1, "port": 0}),
        "emulator/read" if field.contains("symbol") => json!({"symbol": PROBE_SYMBOL, "len": 1}),
        "emulator/read" => json!({"addr": "0x00FF0000", "len": 1, "space": "bus"}),
        "emulator/read_memory" if field.contains("symbol") => {
            json!({"symbol": PROBE_SYMBOL, "len": 1})
        }
        "emulator/read_memory" => json!({"addr": "0x00FF0000", "len": 1}),
        "emulator/read_vram" => json!({"addr": "0x0000", "len": 1}),
        "emulator/restore" => json!({"id": h.checkpoint.clone()}),
        "emulator/run_to" if field.contains("symbol") => {
            json!({"symbol": PROBE_SYMBOL, "maxFrames": 1})
        }
        "emulator/run_to" => json!({"addr": "0x00000200", "maxFrames": 1}),
        "emulator/run_to_scanline" => json!({"line": 0, "maxFrames": 1}),
        "emulator/set_layer_enabled" => json!({"layer": "planeA", "enabled": true}),
        "emulator/set_profiler" => json!({"enabled": true}),
        "emulator/watchpoint_add" if field == "censusKey" => {
            json!({"addr": "0x00FF0500", "len": 2, "stopAfter": 1, "mode": "census",
                   "censusKey": "addr"})
        }
        "emulator/watchpoint_add" if field.contains("symbol") => {
            json!({"symbol": PROBE_SYMBOL, "len": 2, "stopAfter": 1})
        }
        "emulator/watchpoint_add" => {
            json!({"addr": "0x00FF0500", "len": 2, "stopAfter": 1, "space": "bus",
                   "mode": "record", "censusKey": "addr"})
        }
        "emulator/watchpoint_clear" if field.contains("watch") => json!({"watch": h.watch.clone()}),
        "emulator/watchpoint_clear" => json!({"all": true}),
        "emulator/watchpoint_hits" if field.contains("watch") => json!({"watch": h.watch.clone()}),
        "emulator/watchpoint_hits" => json!({"cursor": "0"}),
        "emulator/watchpoint_list" => json!({"cursor": h.watch.clone()}),
        "emulator/write_cram" if field.contains("raw") => json!({"line": 0, "index": 0, "raw": 0}),
        "emulator/write_cram" => json!({"line": 0, "index": 0, "r": 0, "g": 0, "b": 0}),
        "emulator/write_memory" if field.contains("bytes") => {
            json!({"addr": "0x00FF0500", "bytes": "0x00"})
        }
        "emulator/write_memory" if field.contains("symbol") => {
            json!({"symbol": PROBE_SYMBOL, "value": 0, "width": 4})
        }
        "emulator/write_memory" => json!({"addr": "0x00FF0500", "value": 0, "width": 4}),
        "emulator/write_vram" => json!({"addr": "0x0000", "bytes": "0x0000"}),
        "emulator/z80_read" => json!({"addr": "0x0000", "len": 1}),
        "emulator/z80_write" if field.contains("bytes") => {
            json!({"addr": "0x0000", "bytes": "0x00"})
        }
        "emulator/z80_write" => json!({"addr": "0x0000", "value": 0}),
        _ => return None,
    };
    Some(p)
}

/// **Methods whose non-numeric obligations this differential does NOT probe, and why each.**
///
/// Declared rather than implicit: [`every_obligated_method_is_covered_or_declared`] requires this list
/// plus [`baseline`]'s arms to be exactly the obligated set, so a new fragment cannot quietly land
/// outside the differential. Empty is the correct state today: every advertised method that declares a
/// non-numeric obligation has a baseline.
const UNCOVERED: &[(&str, &str)] = &[];

/// **Sites the server refuses for something OTHER than params, with the reason each — an allowance
/// registry, not a skip list.**
///
/// Every row here was **measured**, never predicted: the entry exists because a probe was sent and came
/// back with a code that was not `-32602`. The request was refused in each case — nothing here is a site
/// where the server *served* an out-of-contract value — but the refusal came from a layer in front of
/// the params check, so **this file learned nothing about the declared obligation** and says so rather
/// than counting it.
///
/// The registry has the three properties `common::schema::KNOWN_CONTRACT_DIVERGENCES` has, and for the
/// same reasons: the suite stays green so a known state is not read as a regression; an entry that
/// **stops** being unmeasured fails [`the_registered_unmeasurable_sites_are_still_unmeasurable`], so it
/// cannot rot into a silent exemption; and the whole list is printed beside the coverage count, so a
/// green run is never mistaken for a fully-probed surface.
///
/// # The two families, and what they actually mean
///
/// **`F-SYMBOL-PARAMS-AS-STATE` (11 rows).** Every symbol-bearing field — `symbol`, `defSymbol`, and
/// `lookup_equate`'s `name` — carries `minLength: 1` and, through `$defs/symbolName`, the pattern
/// `^(?!.*\+\$[0-9A-Fa-f]+$).+$` that keeps a caller from smuggling a displacement in as a name. The
/// server refuses both violations, so no out-of-contract request is *served*. But it refuses them at
/// **symbol resolution** with `-32013` (*"no symbol named …"*), because the lookup runs before anything
/// validates the string. That is a params fault reported as a state fault, and the consequence is not
/// cosmetic: the message a caller who sent `""` gets back ends *"Check the spelling or the build, not
/// the freshness of the listing"* — a confident instruction to go and investigate the wrong thing.
/// **The obligation's outcome is met and its diagnosis is wrong.** Worth fixing at the five handlers,
/// out of scope for the parcel that found it.
///
/// **`F-OBJ-BOUNDS-UNPROBED` (4 rows).** The object surface needs a loaded act; on the test ROM every
/// call is refused for state (`-32012`, no object layout) before any param is read. This is the same
/// debt `request_bounds.rs` declares under the same tag for the same family — the numeric and
/// non-numeric differentials are blind to `emulator/object_*` for one shared reason, and it will be one
/// fixture that lifts both.
const UNMEASURABLE: &[(&str, &str, &str, &str)] = &[
    (
        "emulator/lookup_equate",
        "name",
        "minLength",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/memory_hash",
        "symbol",
        "minLength",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/memory_hash",
        "symbol",
        "pattern",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/object_spawn",
        "defSymbol",
        "minLength",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/object_spawn",
        "defSymbol",
        "pattern",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/read",
        "symbol",
        "minLength",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/read",
        "symbol",
        "pattern",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/watchpoint_add",
        "symbol",
        "minLength",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/watchpoint_add",
        "symbol",
        "pattern",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/write_memory",
        "symbol",
        "minLength",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/write_memory",
        "symbol",
        "pattern",
        "F-SYMBOL-PARAMS-AS-STATE",
    ),
    (
        "emulator/object_delete",
        "handle",
        "pattern",
        "F-OBJ-BOUNDS-UNPROBED",
    ),
    (
        "emulator/object_delete",
        "handle|slot",
        "oneOf/none",
        "F-OBJ-BOUNDS-UNPROBED",
    ),
    (
        "emulator/object_move",
        "handle",
        "pattern",
        "F-OBJ-BOUNDS-UNPROBED",
    ),
    (
        "emulator/object_move",
        "handle|slot",
        "oneOf/none",
        "F-OBJ-BOUNDS-UNPROBED",
    ),
];

/// **Out-of-contract requests this server still ANSWERS — a known-gap list, each row owned by a lens row
/// id pending its handler fix.** Not a pass, and not the same thing as [`UNMEASURABLE`].
///
/// This harness had no expected-failure form, and [`UNMEASURABLE`] cannot hold these rows: its anti-rot
/// check requires the probe to stay refused *for something other than params*, while a site the server
/// answers is the defect class itself. So this is a sibling registry with the same three properties: the
/// suite stays green so a known, owned defect is not read as a regression; a row that stops being
/// answered fails [`the_registered_answered_sites_are_still_answered`], which forces its deletion in the
/// commit that lands the fix; and every row is printed on every run beside the refusal count. Keyed by the
/// probe VALUE as well as the site, because one `pattern` site can carry two probes (see
/// [`empty_payload_value`]) and only one of them may be answered.
///
/// Each row: method, field, kind tag, probe value, and the lens row id with its reason.
const KNOWN_ANSWERED: &[(&str, &str, &str, &str, &str)] = &[];

fn registered_answered(site: &ShapeSite) -> Option<&'static str> {
    let Mutation::Set(_, sent) = &site.mutation else {
        return None;
    };
    KNOWN_ANSWERED
        .iter()
        .find(|(m, f, k, v, _)| {
            *m == site.method
                && *f == site.field
                && *k == site.kind.tag()
                && sent.as_str() == Some(*v)
        })
        .map(|(.., why)| *why)
}

fn registered_unmeasurable(site: &ShapeSite) -> Option<&'static str> {
    UNMEASURABLE
        .iter()
        .find(|(m, f, k, _)| *m == site.method && *f == site.field && *k == site.kind.tag())
        .map(|(_, _, _, why)| *why)
}

// ---------------------------------------------------------------------------------------------------
// The probe
// ---------------------------------------------------------------------------------------------------

/// Apply a mutation to a baseline. `Err` describes a baseline that does not have the shape the path
/// needs — a stale baseline must be loud, never a skipped probe.
fn apply(params: &mut Value, m: &Mutation) -> Result<(), String> {
    match m {
        Mutation::Set(path, v) => put(params, path, v.clone()),
        Mutation::Remove(path) => remove(params, path).map(|_| ()),
        Mutation::RemoveAll(paths) => {
            let mut hit = 0;
            for p in paths {
                if remove(params, p)? {
                    hit += 1;
                }
            }
            if hit == 0 {
                return Err(
                    "the baseline carried none of the disjunction's keys, so removing them all \
                     changed nothing and the probe would send a legal request"
                        .to_string(),
                );
            }
            Ok(())
        }
    }
}

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
                format!("baseline is missing `{k}`, which the path descends into")
            })?;
            put(sub, rest, value)
        }
        Seg::Item => {
            let arr = params
                .as_array_mut()
                .ok_or_else(|| "baseline is not an array where the path expects one".to_string())?;
            let first = arr.first_mut().ok_or_else(|| {
                "baseline array is empty; a nested site needs one element".to_string()
            })?;
            put(first, rest, value)
        }
    }
}

/// Delete the key at `path`. Returns whether anything was there.
fn remove(params: &mut Value, path: &[Seg]) -> Result<bool, String> {
    let Some((head, rest)) = path.split_first() else {
        return Err("a mutation path must end at a key".to_string());
    };
    match head {
        Seg::Key(k) => {
            let obj = params
                .as_object_mut()
                .ok_or_else(|| format!("baseline is not an object at `{k}`"))?;
            if rest.is_empty() {
                return Ok(obj.remove(k).is_some());
            }
            match obj.get_mut(k) {
                Some(sub) => remove(sub, rest),
                None => Ok(false),
            }
        }
        Seg::Item => {
            let arr = params
                .as_array_mut()
                .ok_or_else(|| "baseline is not an array where the path expects one".to_string())?;
            match arr.first_mut() {
                Some(first) => remove(first, rest),
                None => Ok(false),
            }
        }
    }
}

/// What one probe learned. **Four variants, and the fourth is the one this file exists to keep
/// separate.**
#[derive(Debug)]
enum Outcome {
    /// `-32602` and the message named the offending key. The obligation, met. The message is carried
    /// because the in-bounds control reports this variant as a FAILURE, and a control that says "your
    /// baseline is illegal" without quoting the server's reason makes the reader go and re-run it.
    RefusedByName(String),
    /// `-32602`, but the message never says which key. Loud, but not actionable.
    RefusedUnnamed(String),
    /// The server answered an out-of-contract request. The defect class.
    Answered,
    /// Refused for something other than params — unserved, a state gate. The obligation never ran.
    Unmeasured { code: i64, message: String },
    /// **No reply was read.** Nothing was observed. Never a pass, never folded into a refusal.
    DidNotFinish(String),
}

/// `field` may be a `|`-joined alternative set (a disjunction); naming ANY of them counts.
fn classify(settled: &Settled, field: &str) -> Outcome {
    let reply = match settled {
        Settled::DidNotFinish(why) => return Outcome::DidNotFinish(why.clone()),
        Settled::Reply(v) => v,
    };
    let Some(err) = reply.get("error") else {
        return Outcome::Answered;
    };
    let code = err["code"].as_i64().unwrap_or(0);
    let message = err["message"].as_str().unwrap_or("").to_string();
    if code != -32602 {
        return Outcome::Unmeasured { code, message };
    }
    // Backticks required, exactly as `request_bounds::classify` argues: `write_cram`'s components are
    // named `r`, `g` and `b`, and a bare substring match would call any English sentence a by-name
    // refusal for those three.
    if field
        .split('|')
        .any(|f| message.contains(&format!("`{f}`")))
    {
        Outcome::RefusedByName(message)
    } else {
        Outcome::RefusedUnnamed(message)
    }
}

/// The per-probe read deadline. Generous next to a params check, which never touches the machine, and
/// short enough that a hung sweep is a report rather than a wall-clock problem.
const PROBE_DEADLINE: Duration = Duration::from_secs(10);

/// A fresh connection posed so a params check is the FIRST thing a probe can hit: paused (several
/// handlers put their params checks behind a `require_paused`) and carrying the probe symbol.
fn client(h: &oracle_aether::server::ServerHandle) -> (Client, Handles) {
    let mut c = Client::connect(h);
    c.handshake(false);
    let _ = c.call("emulator/pause", json!({}));
    load_probe_symbol(&mut c);
    let handles = Handles::mint(&mut c);
    (c, handles)
}

/// **One file per call, because the tests in this binary run in parallel threads.** A single shared
/// `probe.lst` is truncated by `std::fs::write` while another thread's server is reading it, and the
/// server then refuses a zero-byte listing with a message about its content. See the twin of this
/// function in `request_bounds.rs`, where the same race was found and fixed.
fn load_probe_symbol(c: &mut Client) {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let lst =
        format!("  Symbol Table (* = unused):\n\n {PROBE_SYMBOL} : FF0600 C |\n\n   1 symbols\n");
    let dir = std::env::temp_dir().join(format!("oracle-rs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the symbol fixture dir");
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = dir.join(format!("probe-{n}.lst"));
    std::fs::write(&path, lst).expect("write the symbol fixture");
    c.ok(
        "emulator/load_symbols",
        json!({"path": path.to_str().unwrap()}),
    );
}

/// Send one probe, reconnecting if it did not finish.
///
/// **The reconnect is required, not tidy.** After a [`Settled::DidNotFinish`] a reply may still be in
/// flight on that socket; reusing the connection would hand the next probe a reply to the previous
/// request and every verdict after it would be about the wrong row.
fn send_probe(
    c: &mut Client,
    hs: &mut Handles,
    h: &oracle_aether::server::ServerHandle,
    method: &str,
    params: Value,
    field: &str,
) -> Outcome {
    let settled = c.try_call(method, params, PROBE_DEADLINE);
    let dead = matches!(settled, Settled::DidNotFinish(_));
    let outcome = classify(&settled, field);
    if dead {
        (*c, *hs) = client(h);
    }
    outcome
}

// ---------------------------------------------------------------------------------------------------
// The gates
// ---------------------------------------------------------------------------------------------------

/// **The differential.** Every declared non-numeric obligation, violated once, must come back `-32602`
/// naming the offending key.
#[test]
fn every_declared_obligation_is_refused_by_name() {
    let sites = declared_obligations();
    assert!(
        sites.len() >= 60,
        "the walk found only {} obligations; the vendored schema declares far more, so the walk is \
         broken rather than the schema being small",
        sites.len()
    );

    let h = spawn_for_sweep("request-shapes");
    let (mut c, mut hs) = client(&h);

    let uncovered: BTreeSet<&str> = UNCOVERED.iter().map(|(m, _)| *m).collect();
    let mut passed = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut unmeasured: Vec<String> = Vec::new();
    let mut known_gaps: Vec<String> = Vec::new();
    let mut skipped = 0usize;

    for site in &sites {
        if uncovered.contains(site.method.as_str()) {
            skipped += 1;
            continue;
        }
        let Some(mut params) = baseline(&site.method, &site.field, &hs) else {
            failures.push(format!(
                "{} {} [{}]: no baseline and not declared UNCOVERED",
                site.method,
                site.display,
                site.kind.tag()
            ));
            continue;
        };
        if let Mutation::Set(_, Value::Null) = &site.mutation {
            failures.push(format!(
                "{} {} [{}]: UNDERIVABLE — no candidate value violates this constraint, so the \
                 obligation is declared but unprobed. This is a hole in the instrument, not a pass.",
                site.method,
                site.display,
                site.kind.tag()
            ));
            continue;
        }
        if let Err(why) = apply(&mut params, &site.mutation) {
            failures.push(format!(
                "{} {} [{}]: the baseline could not be mutated: {why}",
                site.method,
                site.display,
                site.kind.tag()
            ));
            continue;
        }
        match send_probe(
            &mut c,
            &mut hs,
            &h,
            &site.method,
            params.clone(),
            &site.field,
        ) {
            Outcome::RefusedByName(_) => passed += 1,
            Outcome::Answered => {
                let row = format!(
                    "{} {} [{}]: ANSWERED an out-of-contract request. Sent: {params}",
                    site.method,
                    site.display,
                    site.kind.tag()
                );
                match registered_answered(site) {
                    Some(why) => known_gaps.push(format!("{row}\n      registered: {why}")),
                    // Answered and UNREGISTERED: the defect class, owned by nobody. Fails.
                    None => failures.push(row),
                }
            }
            Outcome::RefusedUnnamed(msg) => failures.push(format!(
                "{} {} [{}]: refused -32602 but the message never names `{}`: {msg:?}",
                site.method,
                site.display,
                site.kind.tag(),
                site.field
            )),
            Outcome::DidNotFinish(why) => failures.push(format!(
                "{} {} [{}]: DID-NOT-FINISH — {why}. NOTHING WAS OBSERVED about this obligation; \
                 this is not a refusal and must never be read as one.",
                site.method,
                site.display,
                site.kind.tag()
            )),
            Outcome::Unmeasured { code, message } => {
                let row = format!(
                    "{} {} [{}]: refused {code} (not a params refusal), so the obligation never ran: \
                     {message:?}",
                    site.method,
                    site.display,
                    site.kind.tag()
                );
                match registered_unmeasurable(site) {
                    Some(why) => unmeasured.push(format!("[{why}] {row}")),
                    // Unmeasured and UNDECLARED. Nobody has ruled on this site, so it fails — the
                    // alternative is a surface that quietly stops being probed one fragment at a time.
                    None => failures.push(format!(
                        "{row}\n    ...and it is not in UNMEASURABLE. An obligation nobody has ruled \
                         on is not a met one; add a row with a reason, or make the site reachable."
                    )),
                }
            }
        }
    }

    println!(
        "request shapes: {passed} refused by name, {} unmeasured (all registered), {} ANSWERED but \
         registered as known gaps, {} skipped as UNCOVERED, of {} declared obligations on advertised \
         methods",
        unmeasured.len(),
        known_gaps.len(),
        skipped,
        sites.len()
    );
    // Printed on every run, passing or not: a green suite must never be read as a fully-probed surface.
    for u in &unmeasured {
        println!("  UNMEASURED {u}");
    }
    for k in &known_gaps {
        println!("  KNOWN GAP {k}");
    }
    assert!(
        failures.is_empty(),
        "{} of {} declared non-numeric request obligations are not met:\n{}",
        failures.len(),
        sites.len(),
        failures.join("\n")
    );
}

/// **The registry's anti-rot check.** Every row in [`UNMEASURABLE`] must still be unmeasurable.
///
/// An allowance list nobody re-checks becomes an exemption list. If a handler starts validating its
/// params before resolving the symbol — the `F-SYMBOL-PARAMS-AS-STATE` fix — the site becomes probeable
/// and its row must go, or the surface silently keeps a hole it no longer has. So the loud state here is
/// a row that has **stopped** diverging, and it fails.
#[test]
fn the_registered_unmeasurable_sites_are_still_unmeasurable() {
    let sites = declared_obligations();
    let h = spawn_for_sweep("request-shapes-registry");
    let (mut c, mut hs) = client(&h);
    let mut retired: Vec<String> = Vec::new();
    let mut orphaned: Vec<String> = Vec::new();

    for (method, field, kind, why) in UNMEASURABLE {
        let Some(site) = sites
            .iter()
            .find(|s| &s.method == method && &s.field == field && s.kind.tag() == *kind)
        else {
            orphaned.push(format!(
                "[{why}] {method} `{field}` [{kind}] is registered unmeasurable but the schema no \
                 longer declares that obligation at all — the allowance has outlived its subject"
            ));
            continue;
        };
        let Some(mut params) = baseline(method, field, &hs) else {
            continue;
        };
        if apply(&mut params, &site.mutation).is_err() {
            continue;
        }
        let outcome = send_probe(&mut c, &mut hs, &h, method, params, field);
        if !matches!(outcome, Outcome::Unmeasured { .. }) {
            retired.push(format!(
                "[{why}] {method} `{field}` [{kind}] is registered as unmeasurable but the probe now \
                 resolves to {outcome:?}. Delete the row: the site is reachable, and leaving it \
                 registered turns a measured allowance into a silent exemption."
            ));
        }
    }
    assert!(
        retired.is_empty() && orphaned.is_empty(),
        "the UNMEASURABLE registry has rotted:\n{}\n{}",
        retired.join("\n"),
        orphaned.join("\n")
    );
}

/// **The known-gap list's anti-rot check.** Every row in [`KNOWN_ANSWERED`] must still be ANSWERED.
///
/// The day the handler is fixed, its probe comes back refused and this goes red, which is what forces the
/// row's deletion in the fixing commit rather than leaving an allowance for a defect that no longer
/// exists — an allowance that outlives its defect is where the next one hides. A row no probe sends any
/// more is red too. An empty list is no claim, exactly as for `request_bounds.rs`'s `KNOWN_UNSERVEABLE`.
#[test]
fn the_registered_answered_sites_are_still_answered() {
    let sites = declared_obligations();
    let h = spawn_for_sweep("request-shapes-known-answered");
    let (mut c, mut hs) = client(&h);
    let mut rotted: Vec<String> = Vec::new();

    for (method, field, kind, value, why) in KNOWN_ANSWERED {
        let Some(site) = sites.iter().find(|s| {
            &s.method == method
                && &s.field == field
                && s.kind.tag() == *kind
                && matches!(&s.mutation, Mutation::Set(_, v) if v.as_str() == Some(*value))
        }) else {
            rotted.push(format!(
                "{method} `{field}` [{kind}] = {value:?} is registered, but no probe sends it any more — \
                 the schema or the walk moved. Registered as: {why}"
            ));
            continue;
        };
        let Some(mut params) = baseline(method, field, &hs) else {
            rotted.push(format!(
                "{method} `{field}`: a registered site with no baseline"
            ));
            continue;
        };
        if let Err(e) = apply(&mut params, &site.mutation) {
            rotted.push(format!(
                "{method} `{field}`: the baseline cannot carry the probe: {e}"
            ));
            continue;
        }
        let outcome = send_probe(&mut c, &mut hs, &h, method, params, field);
        if !matches!(outcome, Outcome::Answered) {
            rotted.push(format!(
                "{method} `{field}` [{kind}] = {value:?} is registered as ANSWERED but now resolves to \
                 {outcome:?}. The handler was fixed: DELETE the row. Registered as: {why}"
            ));
        }
    }
    assert!(
        rotted.is_empty(),
        "the KNOWN_ANSWERED list has rotted:\n{}",
        rotted.join("\n")
    );
}

/// **The anti-false-green control.** Each baseline, sent UNTOUCHED, must not be refused for params.
///
/// Without it a baseline that had gone stale would make every probe on its method come back `-32602`
/// for the wrong reason, and the differential above would read that as the obligation being met — the
/// exact false green `request_bounds.rs` caught on `write_memory`'s `disp`.
#[test]
fn the_baselines_are_legal_requests() {
    let h = spawn_for_sweep("request-shapes-control");
    let (mut c, mut hs) = client(&h);
    let mut wrong: Vec<String> = Vec::new();
    let mut sites = declared_obligations();
    sites.dedup_by(|a, b| a.method == b.method && a.field == b.field);
    for site in &sites {
        let Some(params) = baseline(&site.method, &site.field, &hs) else {
            continue;
        };
        match send_probe(&mut c, &mut hs, &h, &site.method, params.clone(), &site.field) {
            Outcome::RefusedByName(msg) => wrong.push(format!(
                "{} (for the {} site): the UNMUTATED baseline is refused naming `{}` — the baseline \
                 is not a legal request, so every probe on this site refuses for the wrong reason.\n \
                    sent:   {params}\n    server: {msg:?}",
                site.method,
                site.display,
                site.field
            )),
            Outcome::DidNotFinish(why) => wrong.push(format!(
                "{}: the control probe DID NOT FINISH — {why}",
                site.method
            )),
            _ => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "{} baselines are not legal requests:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// **The timeout positive control, and the reason it is not optional.**
///
/// A probe that never comes back must report DID-NOT-FINISH and must never report REFUSED. Both look
/// identical from the outside — no success reply arrived — and the wrong mapping is silent, permanent,
/// and green.
///
/// The block is a real one rather than a simulated one: the machine is resumed with nothing due, and
/// `emulator/wait_for_break` is asked for a timeout an order of magnitude longer than the client's
/// deadline. `timeoutMs` here is well inside the fragment's declared ceiling, so this is a request the
/// contract permits and the server is right to sit on — which is exactly the situation that must not be
/// scored as a refusal.
#[test]
fn a_probe_that_never_finishes_is_reported_as_such_and_never_as_a_refusal() {
    let h = spawn_for_sweep("request-shapes-timeout-control");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok("emulator/resume", json!({}));

    let settled = c.try_call(
        "emulator/wait_for_break",
        json!({"timeoutMs": 5000}),
        Duration::from_millis(300),
    );
    let outcome = classify(&settled, "timeoutMs");
    println!("timeout positive control: {outcome:?}");
    match &outcome {
        Outcome::DidNotFinish(why) => {
            assert!(
                why.contains("300ms") || why.contains("no reply"),
                "the DID-NOT-FINISH reason must say what was waited for and for how long, got {why:?}"
            );
        }
        other => panic!(
            "a probe the server sat on for longer than the deadline was scored {other:?}. A null \
             result MUST NOT be mapped onto any verdict — least of all onto a refusal, which is what \
             this whole file measures."
        ),
    }
    assert!(
        !matches!(
            outcome,
            Outcome::RefusedByName(_) | Outcome::RefusedUnnamed(_) | Outcome::Answered
        ),
        "a timeout was mapped onto an observation"
    );
}

/// **The coverage ledger.** The obligated-method set is derived from the schema; `baseline ∪ UNCOVERED`
/// must equal it exactly, so a new fragment lands red until someone rules on it.
#[test]
fn every_obligated_method_is_covered_or_declared() {
    let sites = declared_obligations();
    let hs = Handles::placeholder();
    let obligated: BTreeSet<String> = sites.iter().map(|s| s.method.clone()).collect();
    let declared: BTreeSet<String> = UNCOVERED.iter().map(|(m, _)| m.to_string()).collect();

    let mut unruled: Vec<&String> = Vec::new();
    for m in &obligated {
        if declared.contains(m) {
            continue;
        }
        let has = sites
            .iter()
            .filter(|s| &s.method == m)
            .any(|s| baseline(m, &s.field, &hs).is_some());
        if !has {
            unruled.push(m);
        }
    }
    assert!(
        unruled.is_empty(),
        "these methods declare non-numeric request obligations and are neither probed nor declared \
         UNCOVERED — a fragment nobody has ruled on: {unruled:?}"
    );
    let stale: Vec<&String> = declared
        .iter()
        .filter(|m| !obligated.contains(*m))
        .collect();
    assert!(
        stale.is_empty(),
        "these methods are declared UNCOVERED but no longer declare any non-numeric obligation; the \
         allowance has outlived its subject: {stale:?}"
    );
}

/// **The `oneOf` distinction, pinned.**
///
/// This is the correction the dry run was for, and it is the one a later edit is most likely to undo —
/// "the walk skips some `required`s, that looks like a bug" is a very easy wrong conclusion. So the
/// property is asserted rather than left to the comment: a `required` reached through a `oneOf` arm is
/// NOT collected as a [`Kind::Required`] obligation, because omitting it is legal.
///
/// `emulator/read_memory` is the canonical instance — `oneOf: [{required:[addr]}, {required:[symbol]}]`
/// — and `{"symbol": …, "len": 1}` with no `addr` is a request the contract fully permits.
#[test]
fn the_oneof_branches_are_not_probed_as_plain_required() {
    let sites = declared_obligations();
    let bad: Vec<String> = sites
        .iter()
        .filter(|s| {
            s.kind == Kind::Required
                && s.method == "emulator/read_memory"
                && (s.field == "addr" || s.field == "symbol")
        })
        .map(|s| s.display.clone())
        .collect();
    assert!(
        bad.is_empty(),
        "`addr`/`symbol` on read_memory were collected as unconditional `required` obligations. They \
         sit inside a oneOf: omitting either is LEGAL, and probing them this way would demand a \
         refusal a CONFORMANT server must not give: {bad:?}"
    );
    assert!(
        sites
            .iter()
            .any(|s| s.kind == Kind::DisjunctionNone && s.method == "emulator/read_memory"),
        "the read_memory oneOf produced no disjunction obligation at all — the group's real \
         obligation (supply neither and be refused) has gone unprobed"
    );
}

/// The covered surface, enumerable rather than implied — printed so nobody has to read the walk to
/// know what it found.
#[test]
fn the_obligated_surface_is_enumerable() {
    let sites = declared_obligations();
    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_method: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for s in &sites {
        *by_kind.entry(s.kind.tag()).or_default() += 1;
        by_method
            .entry(s.method.as_str())
            .or_default()
            .push(format!("{} [{}]", s.display, s.kind.tag()));
    }
    println!(
        "\n{} non-numeric request obligations across {} advertised methods",
        sites.len(),
        by_method.len()
    );
    for (k, n) in &by_kind {
        println!("  {k:12} {n}");
    }
    for (m, rows) in &by_method {
        println!("  {m}");
        for r in rows {
            println!("      {r}");
        }
    }
    assert!(!sites.is_empty(), "the walk found nothing at all");
}
