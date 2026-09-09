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
//!
//! LOUD ON UNMEASURABLE. A scan that finds no workflows, or no `toolchain:` sites, FAILS. A grep that
//! matches nothing exits 0, and "I could not measure it" rendered as green is the failure mode this
//! whole parcel exists to remove.
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
    // `fs::copy` carries the mode bits on unix, so the copy stays executable.
    fs::copy(
        repo_root().join("tools/rust-floor.sh"),
        tools.join("rust-floor.sh"),
    )
    .expect("copy script into sandbox");

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

/// Cargo enforces MSRV PER PACKAGE. A `[workspace.package] rust-version` that no member inherits is a
/// declaration with no teeth — it would read as a fix and check nothing. The member list is derived
/// from the root manifest, never copied, so a new crate is covered the day it is added.
#[test]
fn every_workspace_member_inherits_the_floor() {
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
