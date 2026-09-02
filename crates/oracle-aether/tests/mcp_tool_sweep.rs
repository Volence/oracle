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
//! # It reads the peer through GIT OBJECTS at a pinned revision — never its working tree
//!
//! Until 2026-09-02 this file did `std::fs::read_to_string("/home/volence/sonic_hacks/oracle-old/…")`:
//! a home literal into **another repository's live working directory**, read by a test in the default
//! suite. Two things were wrong with that, and only the first is the obvious one.
//!
//! * A mid-edit save in `oracle-old` — a half-typed `TOOLS` row, a stashed experiment — turns *this*
//!   repo red for a reason that has nothing to do with our code. That is the complaint
//!   `F-SCHEMA-READS-LIVE-EMPYREAN` registered against `schema_conformance.rs` a week earlier, in a
//!   second file nobody had looked at.
//! * The half that matters more: a green here was a statement about *whatever that directory contained
//!   at that instant*, attributable to nothing, reproducible by no one else.
//!
//! `empyrean` `contract/SUITE_PATHS.md` at `38f6df4` rules it suite-wide — *"A gate that proves a
//! vendored copy of a peer's CONTENT is fresh reads the peer through git objects at a named revision,
//! never through the peer's working tree"* — and, in the same section, names the population as **what
//! READS a peer's tree, not what NAMES one**. So the sweep now:
//!
//! 1. **locates** an `oracle-old` checkout (env first, then a marker walk; never a home literal), and
//! 2. **reads** the client out of that checkout's *object store* at the pinned blob below, which is
//!    immutable and cannot move under a run.
//!
//! # Why a pinned revision here, and not a tip read
//!
//! This is a **recovery** read, not a currency read, and the distinction decides the mechanism.
//! It reaches a *known artifact* — the legacy client as it stands — to sweep against our vendored
//! fragments. `oracle-old` is a frozen reference repo (this workspace's `CLAUDE.md`: *"the legacy C++
//! Exodus port it replaced (reference only)"*), it carries **no remote at all**, so there is no "has
//! upstream moved" question to ask and nothing to ask it of. Pointing a currency check at a pinned blob
//! would be vacuous; pinning a *recovery* read is exactly right.
//!
//! What that costs, stated rather than buried: a tool row added to `oracle-old` after [`PIN_REV`] is
//! **not** swept until someone re-pins. That is a deliberate trade of one detection for reproducibility,
//! and it is cheap precisely because the repo is legacy. **Re-pin recipe:** set [`PIN_REV`] to the new
//! commit and [`PIN_BLOB`] to `git -C <oracle-old> rev-parse <rev>:linux-port/mcp/oracle_mcp.py`; the
//! test asserts those two agree, so a half-done re-pin fails rather than sweeping the wrong bytes.
//!
//! # It SKIPS loudly when the sibling checkout is absent
//!
//! `oracle-old/` is a different repository and is not present in a fresh clone or on CI, so this test
//! prints what it could not check — every variable consulted and every path tried — and returns; the
//! house pattern (`symbols_real_lst.rs`, `schema_conformance.rs`). It never passes silently: "cannot
//! check" must not look like "checked", the lesson a missing `vendor` symlink taught this repo when
//! whole conformance rows skipped unnoticed. `ORACLE_MCP_PY` still overrides with a plain file path, and
//! that override is announced as reading an unpinned file, because it is.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A path to the client's source FILE. An explicit operator choice; read as-is, unpinned, and said so.
const ENV_MCP_FILE: &str = "ORACLE_MCP_PY";
/// A path to an `oracle-old` git CHECKOUT. Read only through its object store.
const ENV_OLD_DIR: &str = "ORACLE_OLD_DIR";
/// The suite root every checkout hangs off (`SUITE_PATHS.md`, "Two levels, two names").
const ENV_SUITE_ROOT: &str = "EMPYREAN_SUITE_ROOT";

/// The `oracle-old` revision this sweep reads at. See the module docs for the re-pin recipe.
const PIN_REV: &str = "1eb09a989effad1ea42839e877a1dbf2b418b68d";
/// The blob [`PIN_REV`] carries at [`PIN_PATH`] — the object actually fetched. Content-addressed by
/// git itself, so no hash implementation lives here and a coincidentally similar file cannot satisfy it.
const PIN_BLOB: &str = "11db11f639a79963fbc9cb2e6c8161e17474b06f";
/// Used only as an argument to `git rev-parse`, never joined onto a checkout and read.
const PIN_PATH: &str = "linux-port/mcp/oracle_mcp.py";
/// Length of [`PIN_BLOB`]. A second, independent way for a wrong re-pin to be caught.
const PIN_BYTES: usize = 78_856;

/// The directory name `oracle-old` hangs off the suite root under.
const OLD_REPO_DIR: &str = "oracle-old";

/// One `git -C <repo> …`. `None` on any failure — never a panic, never a silent empty string, because a
/// pipeline that treats a failed command's empty output as content is how "measured nothing" gets
/// rendered as a result.
fn git_in(repo: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// Walk up from `anchor` to the first directory that has an `oracle-old` checkout in it.
///
/// **Deliberately not `git rev-parse --git-common-dir`.** That command returns three different shapes
/// (`.git`; an absolute path from a linked worktree; a *relative* `../../.git` from a main-checkout
/// subdirectory), and normalising its answer lexically is how sigil walked onto the wrong directory —
/// a failure invisible to agents, who run in worktrees, while the suite runs from the main checkout.
/// This repo had zero `--git-common-dir` call sites before this change and still has zero. A marker
/// walk needs none of it: it asks the filesystem the question it actually has.
///
/// The anchor is a **parameter**, not `env!("CARGO_MANIFEST_DIR")` read inside, so the walk itself is
/// exercisable against a constructed anchor — `SUITE_PATHS.md`'s "general form, and the compiled-language
/// reading": in a language without runtime module loading, the test parameterises the anchor. There is
/// no cache, for the same reason.
fn suite_root_from(anchor: &Path) -> Option<PathBuf> {
    let mut cur = Some(anchor);
    while let Some(dir) = cur {
        if dir.join(OLD_REPO_DIR).join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// The client's source, and a one-line statement of where it came from.
struct McpSource {
    text: String,
    origin: String,
}

/// Fetch the pinned blob out of a candidate checkout, verifying that the pin names something real there.
///
/// Returns `Err(reason)` rather than panicking on a directory that is not the repo we meant: a wrong
/// guess from the walk must degrade to a named skip, not to a red on someone else's layout.
fn read_pinned_from(repo: &Path, step: &str) -> Result<McpSource, String> {
    let at_rev = git_in(repo, &["rev-parse", &format!("{PIN_REV}:{PIN_PATH}")])
        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .ok_or_else(|| {
            format!(
                "{}: `git rev-parse {PIN_REV}:{PIN_PATH}` failed — not an oracle-old checkout, or the \
                 pinned revision is absent from it",
                repo.display()
            )
        })?;
    // The pin is internally consistent: the revision really carries the blob we are about to fetch.
    // A half-done re-pin (revision moved, blob not) dies here by name instead of sweeping stale bytes.
    if at_rev != PIN_BLOB {
        return Err(format!(
            "{}: {PIN_REV}:{PIN_PATH} is blob {at_rev}, not the pinned {PIN_BLOB}. The pin in \
             mcp_tool_sweep.rs is half-updated — see the re-pin recipe in this file's module docs.",
            repo.display()
        ));
    }
    let bytes = git_in(repo, &["cat-file", "blob", PIN_BLOB]).ok_or_else(|| {
        format!(
            "{}: `git cat-file blob {PIN_BLOB}` failed even though rev-parse resolved it",
            repo.display()
        )
    })?;
    if bytes.len() != PIN_BYTES {
        return Err(format!(
            "{}: blob {PIN_BLOB} is {} bytes, and this file pins {PIN_BYTES}",
            repo.display(),
            bytes.len()
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|e| format!("{}: blob is not UTF-8: {e}", repo.display()))?;
    Ok(McpSource {
        text,
        origin: format!(
            "step={step} repo={} object-store blob {PIN_BLOB} at {PIN_REV} ({PIN_BYTES} bytes) — the \
             peer's WORKING TREE was not read",
            repo.display()
        ),
    })
}

/// Resolve the legacy client's source. `Err` carries the whole refusal text, ready to print.
///
/// Precedence is `SUITE_PATHS.md`'s, minus a home literal at every step: an explicit file, an explicit
/// checkout, the suite root joined with the repo's directory name, then derivation. Step 4's derivation
/// is safe here in a way it would not be for an unpinned read — what it derives is *where the checkout
/// is*, and what is then read is a fixed object, so the answer cannot move under a run.
fn mcp_source() -> Result<McpSource, String> {
    let mut tried: Vec<String> = Vec::new();

    // Step 1 — an explicit FILE. Unpinned by construction: the operator asked for this exact file.
    if let Ok(p) = std::env::var(ENV_MCP_FILE) {
        let path = PathBuf::from(&p);
        return std::fs::read_to_string(&path)
            .map(|text| McpSource {
                text,
                origin: format!(
                    "step=1-env-file ${ENV_MCP_FILE}={p} — read as a plain FILE, NOT pinned: if that \
                     path is inside a live working tree, this sweep's verdict is about whatever it \
                     contained at this instant"
                ),
            })
            .map_err(|e| format!("${ENV_MCP_FILE} points at {p}, which cannot be read: {e}"));
    }
    tried.push(format!(
        "${ENV_MCP_FILE} (a path to the client's source FILE) — not set"
    ));

    // Step 2 — an explicit CHECKOUT.
    //
    // **Set-but-wrong stops here; it does not fall through.** `SUITE_PATHS.md`: *"A variable that is
    // set but wrong … is a hard error at that step, the aeon semantic, not a null that lets the next
    // step run: a wrong value is evidence of a wrong environment, and the next step would hide it."*
    // Measured, not assumed: before this arm existed, `ORACLE_OLD_DIR=<this repo>` printed
    // `RESULT ok step=4-derived` and the operator's wrong variable left no trace in the output at all.
    match std::env::var(ENV_OLD_DIR) {
        Ok(d) => {
            return read_pinned_from(Path::new(&d), "2-env-repo").map_err(|why| {
                format!("${ENV_OLD_DIR} is set to `{d}`, and it does not answer: {why}")
            })
        }
        Err(_) => tried.push(format!(
            "${ENV_OLD_DIR} (a path to an oracle-old git CHECKOUT) — not set"
        )),
    }

    // Step 3 — the suite root, joined with the repo's directory name. Set-but-wrong stops here too,
    // for the same reason.
    match std::env::var(ENV_SUITE_ROOT) {
        Ok(r) => {
            let cand = Path::new(&r).join(OLD_REPO_DIR);
            return read_pinned_from(&cand, "3-suite-root").map_err(|why| {
                format!("${ENV_SUITE_ROOT} is set to `{r}`, and {OLD_REPO_DIR} under it does not answer: {why}")
            });
        }
        Err(_) => tried.push(format!(
            "${ENV_SUITE_ROOT}/{OLD_REPO_DIR} — {ENV_SUITE_ROOT} not set"
        )),
    }

    // Step 4 — derivation from this crate's own location.
    let anchor = Path::new(env!("CARGO_MANIFEST_DIR"));
    match suite_root_from(anchor) {
        Some(root) => {
            let cand = root.join(OLD_REPO_DIR);
            match read_pinned_from(&cand, "4-derived") {
                Ok(s) => return Ok(s),
                Err(why) => tried.push(format!("derived suite root {} -> {why}", root.display())),
            }
        }
        None => tried.push(format!(
            "derivation: no ancestor of {} contains {OLD_REPO_DIR}/.git",
            anchor.display()
        )),
    }

    // Step 5 — refuse, naming what was looked for and where.
    Err(format!(
        "the legacy MCP client could not be reached. Consulted, in order:\n  {}",
        tried.join("\n  ")
    ))
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
    let source = match mcp_source() {
        Ok(s) => s,
        Err(why) => {
            println!(
                "\n=========================================================================\n\
                 SKIPPED: the MCP tool sweep (CR-27 ruling S1) did NOT run.\n\
                 {why}\n\
                 `oracle-old` is a different repository, absent on CI and in a fresh clone, so this is\n\
                 an expected state there — but it is PRINTED, never silent: a green log and an absent\n\
                 run are the same artifact (SUITE_PATHS.md, protocol bar 25).\n\
                 There is no home literal and no fallback into a peer's working tree on purpose\n\
                 (empyrean contract/SUITE_PATHS.md at 38f6df4).\n\
                 ========================================================================="
            );
            return;
        }
    };
    println!("RESULT ok {}", source.origin);
    let src = source.text;

    let tools = parse_tools(&src);
    assert!(
        tools.len() >= 60,
        "parsed only {} MCP tools from [{}] — the parser has lost the table shape, and a sweep over an \
         empty set reports success while checking nothing",
        tools.len(),
        source.origin
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
    // server and client move together or not at all, and the scheduling is the owner's call. `eecce95`'s
    // own words: *"Nothing in the ruling changes code today."* That is why this is registered rather than
    // fixed here — neither artifact is ours, and unilaterally renaming either half is the breakage the
    // inversion was caught to prevent.
    //
    // **The migration mechanism was REVISED after the above was written, and the revision is the part
    // that matters — do not restore the earlier wording from a stale doc.** The first ruling said
    // *dual-accept first, retire the alias last*. Measuring both servers inverted it, because they have
    // opposite failure modes for a stale spelling. Ours **refuses** an unknown param with `-32602`
    // naming the key and listing the accepted set, at the single dispatch choke *before the handler runs*
    // (`engine.rs:4290`, called from `:999`). The legacy server **ignores** it: `ControlSocket.cpp:127`
    // `getInt(k, d = 0)` and `:149` `getU32` are defaulting accessors with no closure anywhere. So
    // retiring the alias *on the legacy server* silently hands `30000` to every caller still sending
    // `timeout_ms` — an aeon gate that believes it waits 120s waits 30, and still pastes a verdict into
    // merge evidence. **Revised ruling (empyrean, bar 15): never retire the alias on the legacy server —
    // retire it by REPLACING the server**, where closure turns each stale caller into a named error at
    // its own call site. Generalised: *when two implementations of one contract disagree about unknown
    // keys, sequence the cutover onto the strict one* — a permissive implementation can only ever report
    // success. Corollary, found the same day: **dual-accept does not exist on the SEND side.** You cannot
    // accept both spellings of a key you are *sending*, so it covers exactly zero of the param sites
    // pinned below; it is a remedy for *results* only. That is why the mechanism clause above is now
    // silent on dual-accept rather than recommending it.
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

// ---------------------------------------------------------------------------------------------------
// The resolver's own proof
// ---------------------------------------------------------------------------------------------------

/// The marker walk answers the same suite root from a **deeper, constructed anchor** as it does from the
/// crate's own directory — so the derivation is exercised rather than merely present.
///
/// Why this shape. `SUITE_PATHS.md`'s step-3 clauses are written against
/// `git rev-parse --show-toplevel` vs `--git-common-dir`, a distinction only observable from a linked
/// worktree; [`suite_root_from`] uses **neither**, so that particular trap does not apply to it. What
/// does apply is the general form the same section states for compiled languages: *"the test must
/// exercise the resolver's DERIVATION against the bed's path"*, with the anchor parameterised and no
/// cache in the way. This calls the real walk — the one production uses — with an anchor it constructed,
/// and asserts on the RETURNED value.
///
/// It is invariant to where the runner stands, which is the property that matters here: this file's
/// production anchor is `env!("CARGO_MANIFEST_DIR")`, which is the **worktree's** own crate directory
/// when an agent runs from a linked worktree and the main checkout's when the suite runs from the main
/// checkout. Both are descendants of the suite root, so both walk to it.
#[test]
fn the_marker_walk_finds_the_suite_root_from_a_deeper_anchor() {
    let anchor = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(from_crate) = suite_root_from(anchor) else {
        println!(
            "SKIPPED: no ancestor of {} contains {OLD_REPO_DIR}/.git, so there is no suite root to \
             derive and this row cannot discriminate. Expected on CI and in a fresh clone; printed \
             rather than passed, because an absent run and a green one are the same artifact.",
            anchor.display()
        );
        return;
    };

    // A deeper bed inside this crate. Nothing is created on disk: the walk asks about
    // `<dir>/oracle-old/.git` at each ancestor, and every ancestor of this path is a real directory the
    // moment the crate exists.
    let deep = anchor.join("tests").join("contract");
    let from_deep = suite_root_from(&deep)
        .unwrap_or_else(|| panic!("the walk found nothing from {}", deep.display()));
    assert_eq!(
        from_deep,
        from_crate,
        "the walk answers {} from {} but {} from {} — the derivation is anchor-sensitive in a way it \
         must not be",
        from_deep.display(),
        deep.display(),
        from_crate.display(),
        anchor.display()
    );

    // And it is a real answer, not an artefact of the loop: the directory it named actually holds the
    // checkout. Without this the row would pass for a walk that returned its own argument.
    assert!(
        from_crate.join(OLD_REPO_DIR).join(".git").exists(),
        "the walk returned {} but {}/{OLD_REPO_DIR}/.git does not exist",
        from_crate.display(),
        from_crate.display()
    );

    // The refusal arm is reachable: a path with no such ancestor yields None rather than a guess. If `/`
    // ever did hold an `oracle-old` checkout, this fails loudly rather than the row quietly ceasing to
    // discriminate.
    assert!(
        suite_root_from(Path::new("/")).is_none(),
        "/ resolved as a suite root, so this row cannot tell a found answer from a fabricated one"
    );

    println!(
        "RESULT ok step=4-derived suite_root={} (from anchor {} and from {})",
        from_crate.display(),
        anchor.display(),
        deep.display()
    );
}
