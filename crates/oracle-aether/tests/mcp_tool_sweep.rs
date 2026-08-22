//! **The legacy MCP client's tool schemas, swept against the contract fragments** — CR-27's ruling S1.
//!
//! §11.17's *What does not change* said the MCP's two CRAM tool rows *"match this row after the
//! amendment exactly as they matched it before"*. The adjudicator qualified it: they match the row's
//! **names and kinds**, not its constraints, and asked for the claim to be discharged *by parse* rather
//! than by the recon's `write_memory`-only spot check — *"sweep every MCP tool's property set against
//! its fragment's declared key set"*. Both sides are machine-readable, so the sweep is a comparison and
//! not a reading.
//!
//! The direction that matters is **MCP-declares-what-the-fragment-does-not**. Since §2.5 the bus refuses
//! an undeclared top-level param by name, so a tool advertising a property no fragment declares is a
//! call the client will happily compose and the server will refuse — a client-side bug that only shows
//! up at runtime, on someone else's machine. The reverse (a fragment key the tool omits) is *not* a
//! failure: the MCP surfaces a subset on purpose, and those omissions are reported rather than asserted.
//!
//! # Why this is a test and not a note in a doc
//!
//! The sweep was run by hand once during review and came back clean. A hand-run sweep protects the
//! revision it was run against and nothing after it; the next re-vendor, or the next tool row, is
//! exactly when it stops being true. This file is that sweep, checked in.
//!
//! # It SKIPS loudly when the sibling checkout is absent
//!
//! `oracle-old/` is a different repository and is not present in a fresh clone or on CI, so this test
//! prints what it could not check and returns — the house pattern (`symbols_real_lst.rs`,
//! `symbols_as_dialect.rs`). It never passes silently: "cannot check" must not look like "checked", the
//! lesson a missing `vendor` symlink taught this repo when whole conformance rows skipped unnoticed.
//! `ORACLE_MCP_PY` overrides the path.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Where the legacy MCP client lives.
fn mcp_py() -> PathBuf {
    std::env::var("ORACLE_MCP_PY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from("/home/volence/sonic_hacks/oracle-old/linux-port/mcp/oracle_mcp.py")
        })
}

fn vendored_schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/contract/bus-protocol.schema.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("read the vendored schema"))
        .expect("the vendored schema parses")
}

/// Parse `oracle_mcp.py`'s `TOOLS` table into `op -> {declared property names}`.
///
/// A **parser, not an importer**: running Python from a Rust test would drag in an interpreter, a venv
/// and an import graph to learn four-tuples of literals. The table is a list of
/// `(op, description, {props}, [required])` where every key is a string literal, so the shape needed
/// here — the op name and its property names — is recoverable by scanning brace depth.
///
/// The parse is deliberately brittle in the safe direction: if it cannot find the table, or finds fewer
/// tools than the file plainly contains, it says so and the test fails rather than sweeping an empty set
/// and reporting success. A sweep that silently checks nothing is the exact failure this file exists to
/// prevent one layer up.
fn parse_tools(src: &str) -> BTreeMap<String, BTreeSet<String>> {
    let marker = src
        .find("TOOLS")
        .expect("oracle_mcp.py must contain a TOOLS table");
    // Start at the `[` that opens the LIST, not at `TOOLS` — the type annotation
    // (`list[tuple[str, str, dict[str, Any], list[str]]]`) is full of brackets that would otherwise be
    // counted as structure. The assignment's `=` is what separates the two.
    let eq = src[marker..]
        .find('=')
        .expect("the TOOLS table must be an assignment")
        + marker;
    let start = eq + src[eq..].find('[').expect("the TOOLS table must be a list");
    let body = &src[start..];

    let mut out = BTreeMap::new();
    let mut depth = 0i32; // bracket depth relative to the TOOLS list
    let mut entry_start_depth = None::<i32>;
    let mut op: Option<String> = None;
    let mut props: BTreeSet<String> = BTreeSet::new();
    let mut in_props = false;
    let mut props_depth = 0i32;

    let mut chars = body.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '#' => {
                // Skip to end of line — a comment can contain brackets and quotes.
                while let Some(&(_, n)) = chars.peek() {
                    if n == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '"' | '\'' => {
                // Read a (possibly adjacent-concatenated) string literal.
                let quote = c;
                let lit_start = i + 1;
                let mut lit_end = lit_start;
                while let Some((j, n)) = chars.next() {
                    if n == '\\' {
                        chars.next();
                        continue;
                    }
                    if n == quote {
                        lit_end = j;
                        break;
                    }
                }
                let lit = &body[lit_start..lit_end];
                if entry_start_depth.is_some() && op.is_none() && !in_props {
                    // First string in the tuple is the op name.
                    op = Some(lit.to_string());
                } else if in_props && depth == props_depth {
                    // A key directly inside the props dict. Its value is a dict one level deeper, whose
                    // own keys ("type", "description") sit at `props_depth + 1` and are not properties.
                    props.insert(lit.to_string());
                }
            }
            '[' | '{' | '(' => {
                depth += 1;
                if depth == 2 && entry_start_depth.is_none() && c == '(' {
                    entry_start_depth = Some(depth);
                    op = None;
                    props.clear();
                    in_props = false;
                }
                if entry_start_depth.is_some() && c == '{' && !in_props {
                    in_props = true;
                    props_depth = depth;
                }
            }
            ']' | '}' | ')' => {
                if in_props && c == '}' && depth == props_depth {
                    in_props = false;
                }
                depth -= 1;
                if depth == 1 && entry_start_depth == Some(2) && c == ')' {
                    if let Some(name) = op.take() {
                        out.insert(name, std::mem::take(&mut props));
                    }
                    entry_start_depth = None;
                }
                if depth == 0 {
                    break; // end of the TOOLS list
                }
            }
            _ => {}
        }
    }
    out
}

/// Tool properties the fragment does not declare — **the one comparison this whole file performs.**
///
/// Extracted so the sweep and its positive control call the *same* code. When the control computed its
/// own difference inline, a mutant that made the sweep's difference self-referential (always empty) went
/// undetected: the control still passed, because it was checking a copy of the logic rather than the
/// logic. A control that does not share the path it vouches for vouches for nothing.
fn surplus(tool: &BTreeSet<String>, declared: &BTreeSet<String>) -> Vec<String> {
    tool.difference(declared).cloned().collect()
}

#[test]
fn every_mcp_tool_property_is_declared_by_its_contract_fragment() {
    let path = mcp_py();
    let Ok(src) = std::fs::read_to_string(&path) else {
        println!(
            "SKIP: {} not present — the legacy MCP client is a different repository and is absent on \
             CI and in a fresh clone. Set ORACLE_MCP_PY to sweep a checkout elsewhere. This is a \
             printed skip, never a silent pass: the sweep it performs (CR-27 ruling S1) is unrun here.",
            path.display()
        );
        return;
    };

    let tools = parse_tools(&src);
    assert!(
        tools.len() >= 60,
        "parsed only {} MCP tools from {} — the parser has lost the table shape, and a sweep over an \
         empty set reports success while checking nothing",
        tools.len(),
        path.display()
    );

    // **Anti-vacuity, part one: the parser really extracts PROPERTIES, not just tool names.**
    // A `parse_tools` that returned every tool with an empty property set would sail through the whole
    // sweep below — every difference would be empty and the test would report success having compared
    // nothing. A tool count alone does not catch that, because the names parse from a different position
    // in the tuple than the properties do. So one tool's full property set is pinned exactly.
    let write_cram = tools
        .get("write_cram")
        .expect("write_cram must be in the MCP tool table");
    assert_eq!(
        write_cram,
        &["b", "g", "index", "line", "r", "raw"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>(),
        "the parser is not reading property names out of the tool's schema dict"
    );
    assert!(
        tools.values().filter(|p| !p.is_empty()).count() >= 20,
        "almost every tool parsed with no properties — the parse is degenerate"
    );

    let schema = vendored_schema();
    let methods = schema["methods"].as_object().expect("methods");

    let mut undeclared: Vec<String> = Vec::new();
    let mut omissions: Vec<String> = Vec::new();
    let mut unschematized: Vec<&str> = Vec::new();
    let mut swept = 0usize;

    for (op, tool_props) in &tools {
        let method = format!("emulator/{op}");
        let Some(frag) = methods.get(&method) else {
            // A tool for a method this contract does not schematize yet (the legacy server serves a
            // wider surface than we do). Nothing to compare against — reported, not failed.
            unschematized.push(op);
            continue;
        };
        swept += 1;
        let declared: BTreeSet<String> = frag["params"]["properties"]
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();

        for extra in surplus(tool_props, &declared) {
            undeclared.push(format!("{op}.{extra}"));
        }
        let missing: Vec<&String> = declared.difference(tool_props).collect();
        if !missing.is_empty() {
            omissions.push(format!("{op}: {missing:?}"));
        }
    }

    println!("--- MCP tool sweep (CR-27 ruling S1) ---");
    println!(
        "  {} tools parsed, {swept} with a contract fragment, {} without one",
        tools.len(),
        unschematized.len()
    );
    println!("  properties the fragment does NOT declare: {undeclared:?}");
    println!(
        "  fragment keys the tool omits ({}, harmless — the MCP surfaces a subset on purpose):",
        omissions.len()
    );
    for o in &omissions {
        println!("    {o}");
    }

    // **Anti-vacuity, part two: the comparison can actually FAIL.** The sweep's whole value is that it
    // detects a tool property no fragment declares, and every assertion above is satisfied by a
    // comparison that never flags anything. This runs the identical difference over a real tool's real
    // fragment with one fabricated surplus key, and requires it to be caught. (Proven necessary: a
    // mutant that made the difference self-referential — always empty — passed every other check here.)
    {
        let declared: BTreeSet<String> = methods["emulator/write_cram"]["params"]["properties"]
            .as_object()
            .expect("write_cram declares properties")
            .keys()
            .cloned()
            .collect();
        let mut doctored = write_cram.clone();
        doctored.insert("brightness".to_string());
        assert_eq!(
            surplus(&doctored, &declared),
            vec!["brightness".to_string()],
            "the sweep's comparison does not detect a surplus property, so a clean result above means \
             nothing"
        );
        // …and the undoctored set is clean through the same call, so the control is not simply
        // "this function returns something".
        assert!(surplus(write_cram, &declared).is_empty());
    }

    // **D-33, registered — the divergence the 58-fragment schema made visible.**
    //
    // Until 2026-08-22 this list was empty and asserted empty, because the two methods below had no
    // fragment to disagree with: `audio_spectrum` and `wait_for_break` were §6 rows nobody had
    // schematized. empyrean's §9 mechanical-completion pass wrote both fragments FROM THEIR §6 ROWS, and
    // a spelling conflict that had been latent since the legacy client was written became measurable for
    // the first time. **Nothing regressed; an instrument arrived.**
    //
    // The dry run that found this concluded "the fragments are right and the client is wrong". empyrean
    // checked the other half and **inverted it** (`eecce95`): the legacy *server* reads `fft_size`,
    // `max_hz` and `timeout_ms` too — `req.getU32("fft_size")` into a variable named `fftSize`, which is
    // why it hid so long — so client and server agree with each other and both diverge from §6. "Fix the
    // client" would have broken the owner's working MCP tooling, since the server reads what the client
    // sends.
    //
    // **The ruling is direction-only.** camelCase is the stated convention and D14 makes the schema
    // normative for wire shapes, so §6 does not move. The load-bearing half is the migration constraint:
    // server and client move together or not at all, via a dual-accept transition, and the scheduling is
    // the owner's call. `eecce95`'s own words: *"Nothing in the ruling changes code today."* That is why
    // this is registered rather than fixed here — neither artifact is ours, and unilaterally renaming
    // either half is the breakage the inversion was caught to prevent.
    //
    // **Registered, not silenced**, and the registry is anti-rot in both directions by using `assert_eq!`
    // on the whole set rather than subtracting an allowlist:
    //
    //   * a NEW undeclared property — a genuine client bug of the kind this file exists to catch — is red,
    //     because it is not in the pin;
    //   * a registered one going away, because the migration happened or a fragment was amended, is ALSO
    //     red, so the entry is deleted by the commit that resolves it instead of outliving its divergence.
    //     (That failure mode is not hypothetical here: `schema_conformance.rs` records an allowance that
    //     outlived its divergence and started *causing* the failure it was written to suppress.)
    //   * and it cannot pass vacuously — the expectation is three names, so a sweep that compared nothing
    //     produces an empty set and fails, where `is_empty()` would have called that success.
    //
    // **Bound, and the bound is not ours to state loosely.** empyrean measured the divergence across the
    // whole legacy surface — 38 wire keys the server reads, against 49 param names in the fragments — and
    // found **four** genuine conflicts, not three: `fftSize`, `maxHz`, `timeoutMs`, and `maxFrames`. The
    // fourth is `call_stack.max_frames`, and it is invisible to *this* sweep for a reason worth writing
    // down rather than leaving as a discrepancy: `call_stack` is one of the eight §6 rows the contract
    // leaves deliberately unschematized, so it has no fragment and this loop reports it as
    // `unschematized` and moves on. It will appear here, as a fourth entry, on the day that eighth row is
    // schematized — and this comment is what stops that arrival being read as a new defect.
    const D33_WIRE_SPELLING: &[&str] = &[
        "audio_spectrum.fft_size",
        "audio_spectrum.max_hz",
        "wait_for_break.timeout_ms",
    ];

    let mut expected = D33_WIRE_SPELLING
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    expected.sort();
    let mut found = undeclared.clone();
    found.sort();
    assert_eq!(
        found, expected,
        "the set of MCP tool properties no contract fragment declares changed.\n\
         A NEW entry is a real client-side bug: since §2.5 the bus refuses an undeclared top-level param \
         BY NAME, so it is a call this client will compose and the server will refuse, at runtime, on \
         someone else's machine.\n\
         A MISSING entry means a registered D-33 divergence was resolved — delete it here in the same \
         commit, because an allowance that outlives its divergence starts causing failures of its own."
    );

    // **The registry's own claim, re-derived from the schema rather than trusted.** Each entry above
    // asserts something specific and falsifiable: *this is a spelling divergence, not an unknown
    // parameter* — the fragment declares the same field under its camelCase name. Checking that is what
    // separates D-33 from a general-purpose amnesty: a genuinely undeclared param smuggled into the list
    // has no camelCase partner in the fragment and is rejected here, so the pin cannot be used to hide
    // the very bug this file was written to find.
    for entry in D33_WIRE_SPELLING {
        let (op, snake) = entry
            .split_once('.')
            .expect("registry entries are `op.property`");
        // snake_case -> camelCase, derived from the entry rather than tabulated beside it.
        let mut camel = String::new();
        let mut upper_next = false;
        for ch in snake.chars() {
            if ch == '_' {
                upper_next = true;
            } else if upper_next {
                camel.extend(ch.to_uppercase());
                upper_next = false;
            } else {
                camel.push(ch);
            }
        }
        assert_ne!(
            camel, *snake,
            "{entry} is registered as a *spelling* divergence but has no underscore to respell — if it \
             is a genuinely undeclared param it is a client bug and does not belong in this registry"
        );
        let declared: BTreeSet<String> = methods[&format!("emulator/{op}")]["params"]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("emulator/{op} declares params properties"))
            .keys()
            .cloned()
            .collect();
        assert!(
            declared.contains(&camel),
            "{entry} is registered as a D-33 wire-spelling divergence, which claims the fragment \
             declares the same field as `{camel}` — it does not. emulator/{op} declares {declared:?}. \
             Either the registry entry is wrong or this is an undeclared param, i.e. a real client bug."
        );
    }
}
