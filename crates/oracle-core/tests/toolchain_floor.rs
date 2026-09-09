//! THE RUST FLOOR IS DECLARED ONCE — this is what keeps it that way.
//!
//! Until 2026-09-09 the intended toolchain version lived nowhere in the project and was restated as a
//! literal in five places (three CI steps in `ci.yml`, one in `nightly-differential.yml`, one README
//! sentence). Nothing in the repo could detect those five disagreeing, and they already disagreed with
//! the development box. `Cargo.toml`'s `[workspace.package] rust-version` is now the one declaration
//! and everything else reads it; this file fails if that stops being true.
//!
//! WHY THIS TEST IS NOT VACUOUS. The easy version of this test — read a constant, assert it equals
//! itself — is the shape this repo has been burned by. Every assertion here compares two INDEPENDENTLY
//! OBTAINED values:
//!
//!   * `manifest_floor()` parses the committed `Cargo.toml` in Rust.
//!   * `script_floor()` EXECUTES `tools/rust-floor.sh` — the exact code both CI workflows run — and
//!     reads its stdout. So CI's extraction is exercised locally instead of being an untested shell
//!     snippet that only ever runs on a runner.
//!   * the workflow assertions GREP THE COMMITTED YAML, byte for byte, and are compared against the
//!     manifest parse.
//!   * `script_is_loud_when_the_floor_is_missing` is a negative control that runs the script against a
//!     manifest with the key deleted and requires a non-zero exit with empty stdout.
//!   * the two LIVE-SURFACE rules scan every file that tells a human or a machine which Rust to use —
//!     `README.md`, `.github/**`, `tools/**` and every `Cargo.toml` — see `live_surface_files()` for
//!     the scope argument and the `docs/**` exemption.
//!
//! The first four tests here guarded the workflows and the manifests, and shipped with a hole: the
//! README. It was found by someone mutating the README by hand and watching all five tests pass, which
//! is the whole reason the live-surface rules exist. The lesson is worth keeping: the sites a guard
//! covers are the sites its author was thinking about, and the one that had to be found by reading is
//! the one the guard did not cover.
//!
//! LOUD ON UNMEASURABLE. A scan that finds no workflows, no `toolchain:` sites, fewer than eight live
//! files, or a missing/empty `README.md`, FAILS. A grep that matches nothing exits 0, and "I could not
//! measure it" rendered as green is the failure mode this whole parcel exists to remove. Rule A goes
//! further and is SELF-VALIDATING: it hunts a string that must be present (the declaration), so zero
//! hits proves the scanner broken rather than the tree clean, and the hit it found is printed on fd 2
//! on every green run.
//!
//! RUNNER: `cargo test --workspace`, the `Test` step of the `build-test-lint` job in
//! `.github/workflows/ci.yml`. Targeted locally as
//! `cargo test -p oracle-core --test toolchain_floor`. It lives in `oracle-core` because that is the
//! cheapest member to build (one dependency), so the targeted run is seconds; `symbols_real_lst.rs`
//! and `aeon_dimensions.rs` are the precedent for a repo-file-reading test in this crate.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// The workspace root: `crates/oracle-core/../..`.
fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The sanctioned way for a workflow to name the floor: a GitHub Actions step-output reference, so the
/// value comes from `tools/rust-floor.sh` at run time. Whitespace inside `${{ }}` is normalised away
/// before comparison, because Actions accepts either form and a formatter may rewrite it.
const DERIVED_EXPR: &str = "${{steps.floor.outputs.toolchain}}";

/// Independent parse of the one declaration. Anchored at column 0 so it matches the table-level key
/// under `[workspace.package]` and never a member's `rust-version.workspace = true`.
fn manifest_floor() -> String {
    let manifest = repo_root().join("Cargo.toml");
    let text = fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", manifest.display()));

    let mut hits: Vec<String> = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("rust-version") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let value = rest.trim().trim_matches('"').to_string();
        hits.push(value);
    }

    assert_eq!(
        hits.len(),
        1,
        "{} must declare exactly one table-level `rust-version` (found {}: {hits:?}). \
         This is THE floor; a second one is a second source of truth.",
        manifest.display(),
        hits.len()
    );

    let floor = hits.remove(0);
    let parts: Vec<&str> = floor.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "declared floor {floor:?} is not MAJOR.MINOR.PATCH — `dtolnay/rust-toolchain` is handed this \
         string verbatim, so a malformed value is a CI failure nobody would trace back here"
    );
    floor
}

/// Runs the script CI runs, and returns its stdout. Panics loudly on a non-zero exit.
fn script_floor() -> String {
    let script = repo_root().join("tools/rust-floor.sh");
    assert!(
        script.is_file(),
        "{} is missing — both workflows invoke it to pick their toolchain, so its absence is a \
         broken CI, not a missing test fixture",
        script.display()
    );

    let out = Command::new(&script)
        .output()
        .unwrap_or_else(|e| panic!("cannot execute {} ({e}) — is it +x?", script.display()));

    assert!(
        out.status.success(),
        "{} exited {:?}\nstdout: {:?}\nstderr: {}",
        script.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("rust-floor.sh stdout is utf-8")
        .trim()
        .to_string()
}

/// THE LIVE SURFACE — every file in the tree that tells a human or a machine which Rust to use.
///
/// SCOPE, and why it is drawn here rather than wider or narrower. The defect is not "the README" and
/// it is not "documentation"; it is *a copy of the version in a place that is read as current
/// instruction*. Four such places exist:
///
///   * `README.md` — instructs a human. This is where the gap was found by hand.
///   * `.github/**` — instructs the runner. Already guarded at `toolchain:` keys, but a literal in a
///     comment, or a future `apt install` / `rustup` line, is the same copy in the same file and was
///     not covered.
///   * `tools/**` — executable configuration, and the single most likely home for the next copy: a
///     setup script that hardcodes a toolchain.
///   * every `Cargo.toml` — where the one legitimate statement lives.
///
/// DELIBERATELY EXEMPT, and how a future dated record stays clean:
///
///   * `docs/**` — the record archive, exempt WHOLESALE and by directory rather than by a list of
///     lines. This is principled, not a convenience: everything there is dated by construction —
///     `decisions.jsonl` and `lane-log.jsonl` are append-only ledgers, and the plan documents carry
///     the date in the filename (`2026-08-17-player-s3-lenses.md`). `docs/decisions.jsonl` (d-46) and
///     that plan document both legitimately state the floor as a record of what was true then, and
///     a guard that made someone edit either to get green would be worse than the gap it closed. So
///     the rule for a future historical note about the toolchain is simply: it belongs in `docs/`,
///     which is already where this repo puts such notes.
///   * `crates/**/*.rs` — a Rust source file cannot select a toolchain; the manifest beside it can,
///     and that IS scanned. Including half a million lines of source whose comments discuss
///     dependency versions would buy nothing and cost the first false positive.
///   * `target/`, `vendor/` — build output and pinned third-party corpora, not ours to edit.
///
/// The result needs NO exemption list at all: measured over the whole live surface, the only match is
/// the declaration itself.
fn live_surface_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files: Vec<PathBuf> = vec![root.join("README.md"), root.join("Cargo.toml")];
    for dir in [".github", "tools"] {
        collect_files(&root.join(dir), &mut files);
    }
    for member in workspace_members() {
        files.push(root.join(member).join("Cargo.toml"));
    }
    files.sort();
    files.dedup();
    files
}

/// Recursive walk. A directory that does not exist is a FINDING, not an empty result: `.github` and
/// `tools` are both load-bearing here, and silently scanning nothing is the vacuity this file exists
/// to prevent.
fn collect_files(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| {
        panic!(
            "cannot list {} ({e}) — refusing to scan nothing",
            dir.display()
        )
    });
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

/// Every `*.yml` / `*.yaml` under `.github/workflows`, sorted. Fails if there are none.
fn workflow_files() -> Vec<PathBuf> {
    let dir = repo_root().join(".github/workflows");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "yml" || e == "yaml")
        })
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no workflow files under {} — a scan that measured nothing must not report ok",
        dir.display()
    );
    files
}

/// `("path:line", "value")` for every `toolchain:` key in the workflows. Comment lines are skipped:
/// the prose above these steps mentions `toolchain:` and is not configuration.
fn toolchain_sites() -> Vec<(String, String)> {
    let mut sites = Vec::new();
    for file in workflow_files() {
        let text = fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                continue;
            }
            let Some(rest) = trimmed.strip_prefix("toolchain:") else {
                continue;
            };
            let value = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            sites.push((format!("{}:{}", file.display(), i + 1), value));
        }
    }
    sites
}

/// The script and an independent parse of the manifest must agree. If the script's `sed` ever stops
/// matching the key, CI would install the wrong toolchain (or none); this is where that shows up.
#[test]
fn rust_floor_script_reports_the_declared_floor() {
    let declared = manifest_floor();
    let printed = script_floor();
    assert_eq!(
        printed, declared,
        "tools/rust-floor.sh printed {printed:?} but Cargo.toml declares {declared:?} — CI hands the \
         script's output straight to dtolnay/rust-toolchain, so these disagreeing means CI builds on a \
         version the manifest does not describe"
    );
}

/// NEGATIVE CONTROL. Against a manifest with the key removed the script must exit non-zero and print
/// NOTHING on stdout. If it were quiet-and-empty instead, `v=$(...)` in CI would succeed with an empty
/// value and the toolchain action would install some default — green CI on an unknown compiler.
#[test]
fn script_is_loud_when_the_floor_is_missing() {
    let stamp = format!(
        "oracle-rust-floor-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let sandbox = std::env::temp_dir().join(stamp);
    let tools = sandbox.join("tools");
    fs::create_dir_all(&tools).expect("create sandbox");

    // A manifest that is valid TOML and simply does not declare the floor.
    fs::write(
        sandbox.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = []\n\n[workspace.package]\nedition = \"2021\"\n",
    )
    .expect("write sandbox manifest");
    // SYMLINKED, not copied — for two reasons, and the second one bit.
    //
    // 1. A symlink means the REAL committed script is what runs. A copy would let the script and the
    //    thing under test drift by exactly the mechanism this whole file exists to prevent.
    // 2. `fs::copy` + immediately exec'ing the copy is the classic ETXTBSY race: in a multithreaded
    //    process another test thread can fork while the copy's write descriptor is still open, and the
    //    exec then fails with "Text file busy". That is not hypothetical here — the first draft of this
    //    test copied, and failed with `ExecutableFileBusy` on a run where the assertion it exists to
    //    make was never reached. A guard that flakes is a guard that gets ignored.
    //
    // The script resolves its root from `dirname "$0"`, so it reads the SANDBOX manifest either way.
    std::os::unix::fs::symlink(
        repo_root().join("tools/rust-floor.sh"),
        tools.join("rust-floor.sh"),
    )
    .expect("symlink script into sandbox");

    let out = Command::new(tools.join("rust-floor.sh"))
        .output()
        .expect("run sandboxed rust-floor.sh");

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    fs::remove_dir_all(&sandbox).ok();

    assert!(
        !out.status.success(),
        "rust-floor.sh exited 0 against a manifest with no rust-version — a missing floor must be a \
         RED CI step, never an empty toolchain flowing into the action.\nstdout: {stdout:?}"
    );
    assert!(
        stdout.trim().is_empty(),
        "rust-floor.sh printed {stdout:?} on stdout while failing; CI captures stdout, so anything \
         here is a value that would be used"
    );
    assert!(
        !stderr.trim().is_empty(),
        "rust-floor.sh failed silently — the operator needs to be told which file is missing the key"
    );
}

/// No workflow may restate the version. A step-output reference is the sanctioned form; a literal is
/// tolerated ONLY while it equals the declaration, and is reported as drift the moment it does not.
#[test]
fn no_workflow_restates_the_rust_version() {
    let declared = manifest_floor();
    let sites = toolchain_sites();

    assert!(
        !sites.is_empty(),
        "found no `toolchain:` keys under .github/workflows — either the workflows stopped pinning a \
         toolchain or this scan is broken. Both are findings; neither is ok."
    );

    let mut bad: Vec<String> = Vec::new();
    for (where_, value) in &sites {
        let normalised: String = value.chars().filter(|c| !c.is_whitespace()).collect();
        if normalised == DERIVED_EXPR || value == &declared {
            continue;
        }
        bad.push(format!("  {where_}  ->  {value:?}"));
    }

    assert!(
        bad.is_empty(),
        "{} of {} `toolchain:` sites disagree with the declared floor {declared:?}:\n{}\n\
         Each must be `toolchain: {DERIVED_EXPR}` (fed by the `floor` step running \
         tools/rust-floor.sh). A literal here is the exact defect this test exists for: five copies of \
         a version and no way to notice them drifting.",
        bad.len(),
        sites.len(),
        bad.join("\n")
    );
}

/// A `${{ steps.floor.outputs.toolchain }}` reference is only as good as the step that sets it. If a
/// workflow uses the expression without running the script, the output is empty and the toolchain
/// action silently falls back.
#[test]
fn workflows_using_the_derived_toolchain_actually_run_the_script() {
    let mut bad: Vec<String> = Vec::new();
    let mut derived_sites = 0usize;

    for file in workflow_files() {
        let text = fs::read_to_string(&file).expect("read workflow");
        let uses_expr = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .any(|l| {
                l.trim_start().starts_with("toolchain:")
                    && l.chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>()
                        .contains(DERIVED_EXPR)
            });
        if !uses_expr {
            continue;
        }
        derived_sites += 1;
        if !text.contains("tools/rust-floor.sh") {
            bad.push(file.display().to_string());
        }
    }

    assert!(
        derived_sites > 0,
        "no workflow derives its toolchain from the manifest — the four literals are back"
    );
    assert!(
        bad.is_empty(),
        "these workflows reference {DERIVED_EXPR} but never run tools/rust-floor.sh, so the step \
         output is empty and the toolchain action installs a default:\n  {}",
        bad.join("\n  ")
    );
}

/// The workspace's member paths, derived from the root manifest so a new crate is covered the day it
/// is added.
fn workspace_members() -> Vec<String> {
    let root = repo_root();
    let text = fs::read_to_string(root.join("Cargo.toml")).expect("read workspace manifest");

    // `members = [ ... ]` — the literal list cargo itself reads. Parsed line-wise and by QUOTED
    // STRING rather than by splitting on commas: the real list is interleaved with prose comments that
    // contain commas, and a comma-splitter silently yields fragments of English as member paths.
    let mut in_list = false;
    let mut members: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if !in_list {
            if t.starts_with("members") && t.contains('[') {
                in_list = true;
            }
            continue;
        }
        if t.starts_with(']') {
            break;
        }
        if t.starts_with('#') {
            continue;
        }
        let mut rest = t;
        while let Some(open) = rest.find('"') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            members.push(after[..close].to_string());
            rest = &after[close + 1..];
        }
    }
    assert!(in_list, "workspace manifest declares no `members = [` list");

    assert!(
        members.len() >= 2,
        "parsed {} workspace members from Cargo.toml ({members:?}) — that is a parse failure, not a \
         small workspace; refusing to report ok on a list this test did not really read",
        members.len()
    );
    members
}

/// Cargo enforces MSRV PER PACKAGE. A `[workspace.package] rust-version` that no member inherits is a
/// declaration with no teeth — it would read as a fix and check nothing. The member list is derived
/// from the root manifest, never copied, so a new crate is covered the day it is added.
#[test]
fn every_workspace_member_inherits_the_floor() {
    let root = repo_root();
    let members = workspace_members();

    // On fd 2, where libtest's per-thread capture cannot swallow it: a passing test that prints
    // nothing cannot be told apart from one that checked an empty list.
    eprintln!(
        "FLOOR GUARD: {} declared, {} workspace members checked: {}",
        manifest_floor(),
        members.len(),
        members.join(" ")
    );

    let mut missing: Vec<String> = Vec::new();
    for m in &members {
        let manifest = root.join(m).join("Cargo.toml");
        let body = fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", manifest.display()));
        let inherits = body.lines().map(str::trim).any(|l| {
            l.starts_with("rust-version") && l.contains("workspace") && l.contains("true")
        });
        if !inherits {
            missing.push(manifest.display().to_string());
        }
    }

    assert!(
        missing.is_empty(),
        "{} of {} workspace members do not inherit the floor with `rust-version.workspace = true`, \
         so cargo does not enforce it for them:\n  {}",
        missing.len(),
        members.len(),
        missing.join("\n  ")
    );
}

/// Is `hay[at..]`'s match a standalone version rather than part of a longer token (`1.96.01`,
/// `v1.96.0-beta`)? Guards against a substring match reading as a finding.
fn standalone_at(hay: &str, at: usize, needle_len: usize) -> bool {
    let before_ok = hay[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_ascii_digit() && c != '.');
    let after_ok = hay[at + needle_len..]
        .chars()
        .next()
        .is_none_or(|c| !c.is_ascii_digit() && c != '.');
    before_ok && after_ok
}

/// RULE A — THE DECLARED VALUE APPEARS EXACTLY ONCE IN THE LIVE TREE.
///
/// This is the gap found by hand after the first four mutations: the parcel de-duplicated five sites
/// and guarded four, leaving `README.md` — the site that had to be found by reading — unprotected,
/// while the README's own new text promised that a number written there would go stale and be caught.
/// A doc that says *always* with nothing checking it is this repo's recurring shape.
///
/// SELF-VALIDATING, which is the anti-vacuity property that matters here. The scan searches for a
/// string KNOWN TO BE PRESENT: the declaration itself. A green run therefore proves the scanner can
/// see occurrences at all, and finding ZERO means a broken scanner (wrong root, unreadable files, a
/// needle that stopped matching) and FAILS rather than passing quietly. The legitimate hit is printed
/// on fd 2 every run, so the log names what was found instead of asserting silence.
#[test]
fn the_declared_version_is_stated_in_exactly_one_place() {
    let floor = manifest_floor();
    let files = live_surface_files();
    let root = repo_root();

    assert!(
        files.len() >= 8,
        "the live surface scan found only {} files ({files:?}) — README.md, both workflows, tools/*, \
         and six manifests are all expected. A short list is a broken walk, not a small repo, and \
         must not report ok",
        files.len()
    );
    let readme = root.join("README.md");
    assert!(
        files.contains(&readme) && fs::metadata(&readme).is_ok_and(|m| m.len() > 0),
        "README.md is missing or empty, so the guard covering it measured nothing. A file this test \
         is specifically responsible for cannot be allowed to vanish into a green run."
    );

    let mut hits: Vec<String> = Vec::new();
    for file in &files {
        // Non-utf8 files are skipped by `read_to_string`, which is correct: a version literal that
        // misleads anybody is text.
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            let mut from = 0;
            while let Some(off) = line[from..].find(&floor) {
                let at = from + off;
                if standalone_at(line, at, floor.len()) {
                    hits.push(format!("{}:{}  {}", file.display(), i + 1, line.trim()));
                }
                from = at + floor.len();
            }
        }
    }

    assert!(
        !hits.is_empty(),
        "scanned {} live files for the declared floor {floor:?} and found it NOWHERE — not even in \
         the manifest that declares it. That is this scan being broken, not the tree being clean; \
         refusing to report ok.",
        files.len()
    );
    eprintln!(
        "FLOOR GUARD: {} live files scanned for {floor:?}; {} occurrence(s):",
        files.len(),
        hits.len()
    );
    for h in &hits {
        eprintln!("  {h}");
    }

    let declaration_prefix = format!("{}:", root.join("Cargo.toml").display());
    let strays: Vec<&String> = hits
        .iter()
        .filter(|h| !h.starts_with(&declaration_prefix) || !h.contains("rust-version ="))
        .collect();

    assert!(
        strays.is_empty(),
        "the Rust version is stated in {} place(s) outside its declaration:\n  {}\n\
         Only `rust-version = \"{floor}\"` in the workspace Cargo.toml may carry this number. Every \
         other live site must DERIVE it — CI via `tools/rust-floor.sh`, prose by pointing at the \
         manifest instead of repeating the digits. If it is a dated historical note, it belongs in \
         `docs/`, which is exempt precisely so that records never have to be falsified to get green.",
        strays.len(),
        strays
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// RULE B — NO *OTHER* VERSION IS STATED AS THIS PROJECT'S RUST VERSION EITHER.
///
/// Rule A cannot see the worse failure. A copy that has DRIFTED no longer contains the declared
/// string: `CI pins Rust 1.94.0` is invisible to an exact-value scan, and it is precisely the
/// sentence that misleads a reader — an in-sync copy is merely redundant, a drifted one is false. So
/// this rule looks for the SHAPE of the claim: a toolchain keyword followed closely by a THREE-part
/// version, anywhere on the live surface except the declaration line itself.
///
/// Three parts are required, and the keyword must sit within `KEYWORD_WINDOW` characters BEFORE the
/// version. Both constraints keep this off the ordinary version chatter these files are full of —
/// `alsa-sys 0.3.1`, `clippy 0.1.98`, `egui 0.36`, `slice::as_chunks` (stable 1.88), `edition =
/// "2021"`.
///
/// THE WINDOW WIDTH IS MEASURED, and the first value was wrong. It started at 14, which caught
/// `CI pins Rust **1.94.0**` but NOT `rustup toolchain install 1.94.0` — there `toolchain` sits 18
/// characters back, and that line sailed through all seven tests while pinning a version the project
/// does not use. Sweeping the whole live surface for false positives at several widths:
///
///   window 14 / 25 / 40 / 80 chars -> 0 hits;  whole line -> 1 hit
///
/// The single whole-line hit is a legitimate sentence in `Cargo.toml` ("...has been on 1.98.0 since
/// 2026-08-18... this is cargo's own MSRV key"), where the keyword is real but unrelated to the
/// number. So the honest ceiling is "less than a line", and 60 is chosen: triple the phrasing that
/// defeated 14, with measured zero false positives, and still short of the width that produces one.
///
/// KNOWN LIMIT, stated rather than papered over: a bare literal with no toolchain word anywhere near
/// it (`echo 1.94.0 > /tmp/v`) is caught by neither rule. Rule A covers that case for the CURRENT
/// value; for a drifted one it is out of reach without a pattern so broad it would fire on every
/// dependency version in the tree.
#[test]
fn no_live_file_states_a_different_rust_version() {
    const KEYWORDS: [&str; 4] = ["rust", "rustc", "toolchain", "msrv"];
    const KEYWORD_WINDOW: usize = 60;
    let floor = manifest_floor();
    let root = repo_root();
    let declaration_line = format!("rust-version = \"{floor}\"");

    let files = live_surface_files();
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        scanned += 1;
        for (i, line) in text.lines().enumerate() {
            if line.trim() == declaration_line {
                continue;
            }
            let bytes = line.as_bytes();
            // Walk every `d.d.d` on the line and ask whether a toolchain keyword sits just before it.
            for (at, _) in line.match_indices('.') {
                let start = bytes[..at]
                    .iter()
                    .rposition(|b| !b.is_ascii_digit())
                    .map_or(0, |p| p + 1);
                if start == at {
                    continue;
                }
                let mut end = at;
                let mut dots = 0;
                while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
                    if bytes[end] == b'.' {
                        dots += 1;
                        if dots > 2 {
                            break;
                        }
                    }
                    end += 1;
                }
                if dots != 2 || !bytes[end - 1].is_ascii_digit() {
                    continue;
                }
                // SLICE THE ORIGINAL LINE, THEN LOWERCASE — never the other way round. These files
                // are full of em dashes, so two hazards go live at a 60-byte window that a 14-byte
                // one mostly dodged: `to_lowercase()` can change byte lengths, which would make
                // offsets taken from `line` wrong in a lowercased copy; and a fixed byte offset can
                // land inside a multi-byte character, which panics. `start` and `end` are always
                // boundaries (the digits and dots they delimit are ASCII), but `start - 60` need not
                // be, so it is snapped forward.
                let mut window_from = start.saturating_sub(KEYWORD_WINDOW);
                while !line.is_char_boundary(window_from) {
                    window_from += 1;
                }
                let window = line[window_from..start].to_lowercase();
                if !KEYWORDS.iter().any(|k| window.contains(k)) {
                    continue;
                }
                offenders.push(format!(
                    "  {}:{}  {}  (reads as: {})",
                    file.display(),
                    i + 1,
                    line.trim(),
                    &line[start..end]
                ));
                break;
            }
        }
    }

    assert!(
        scanned >= 8,
        "only {scanned} readable files on the live surface — a scan this thin measured nothing"
    );
    assert!(
        offenders.is_empty(),
        "{} live site(s) state a Rust version that is not the declaration:\n{}\n\
         The declared floor is {floor:?}. A number here that disagrees is the drift this parcel \
         exists to remove, and it is worse than an in-sync copy because it actively misleads. Derive \
         it (`tools/rust-floor.sh`, or point at {}), or move the sentence to `docs/` if it is a dated \
         record.",
        offenders.len(),
        offenders.join("\n"),
        root.join("Cargo.toml").display()
    );
}
