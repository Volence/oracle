//! **`serverBuild` is computed here, at compile time** — contract `protocol.md` §2.1 (registered
//! 2026-08-26 by §11.23), and the ⚑ clause in particular.
//!
//! The contract's rule is not "embed a hash". It is two rules that pull against each other, and this
//! script exists because satisfying one of them by hand is what makes the other fail silently:
//!
//! 1. **Structural emission.** `serverBuild.id` MUST be fixed when the binary is produced and MUST NOT be
//!    read at run time from a file, an environment variable, a generated config, a flag or a sibling
//!    process. *"A process which reads its identity has an opinion about it, and an opinion can be stale,
//!    mismatched, or copied from a neighbour."* A build-generated file `include!`d into the binary is
//!    explicitly on the compile-time side of that line and is conformant, which is exactly what this
//!    script produces (`$OUT_DIR/build_info.rs`, `include!`d by `src/build_info.rs`).
//! 2. **The constant must be invalidated by what it names** (ruling M2). A build MUST recompute `id`
//!    whenever the revision, the dirty state, or the surface-changing configuration changes. Cargo caches
//!    a build script's output; a cached product that survives such a change *is* a self-report by another
//!    route, and is non-conformant. So every input this script reads is declared with
//!    `cargo:rerun-if-changed` below, and the completeness of that declaration is the whole point of the
//!    exercise rather than hygiene around it.
//!
//! **What goes into `id`, and why** (ruling M1: under `"vcs"` the id is the revision identifier *extended
//! by whatever build-time selection changes the served surface*; a bare commit hash does not satisfy the
//! differ-rule for a tree with compile-time-optional surfaces). Derived from what this crate's source
//! actually keys off, not from a wish-list:
//!
//! * **The full 40-character commit hash**, first, and unabbreviated (§2.1's SHOULD: abbreviations collide
//!   over a repository's life, and resolving the hash back to a commit is the use the field was added
//!   for).
//! * **`profile`** — `debug` vs `release`. This is not decorative here: `engine.rs`'s
//!   `checkpoint_restore` carries a `#[cfg(debug_assertions)] debug_assert!` on the restored symbol
//!   table's ROM binding. Two builds of one clean commit therefore answer `emulator/restore` differently
//!   — one replies, one aborts the process — which is *precisely* "two builds whose observable behaviour
//!   on this bus can differ". `PROFILE` is the only signal Cargo gives a build script for this; it is a
//!   proxy for `debug_assertions` rather than the flag itself, and a custom profile that flips
//!   `debug-assertions` away from its `PROFILE` default would not be distinguished. Recorded, not hidden.
//! * **`target`** — the whole crate is `#![cfg(unix)]` (`lib.rs:64`), so the target triple gates the
//!   entire served surface, not a row of it.
//! * **`features`** — this crate's own enabled feature set, read from Cargo's `CARGO_FEATURE_*`. It is
//!   **empty today** and that is correct rather than vestigial: `oracle-aether/src` contains zero
//!   `cfg(feature = …)` (`tests/server_build.rs` re-derives that claim rather than trusting this comment),
//!   so no feature of this crate changes what it serves. The moment one does — the shape ruling M1 names,
//!   a feature-gated handler — the name appears here and the id moves without anyone remembering to make
//!   it. **Known limit:** a *dependency's* feature turned on by workspace unification (`oracle-core`'s
//!   `synth`, say) is invisible to `CARGO_FEATURE_*`. That is only a gap if this crate ever reads such a
//!   cfg without declaring a feature of its own that mirrors it, and `tests/server_build.rs` fails if it
//!   does.
//!
//! `dirty` is a separate field on purpose — §2.1: *"`dirty` covers uncommitted source; it does not cover
//! configuration"* — so it stays out of `id`.
//!
//! **No git, no lie.** A source tarball has no `.git`. §2.1 types that case rather than forcing it to
//! fabricate a hash: `source: "declared"`, *"the build system was told a string and embedded it… a
//! packager rebuilding from a tarball has no version control, and naming that case beats forcing it to
//! lie."* So the fallback emits a `"declared"` id that cannot be mistaken for a revision (it starts
//! `no-vcs+`), omits `dirty` (the schema requires it only under `"vcs"`), and prints a `cargo:warning`
//! so the degradation is on the build log rather than silent.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".into());
    let features = enabled_features();
    let config = format!("+profile={profile}+target={target}+features={features}");

    // Declared BEFORE the git probe, so the dirty half of the invalidation holds even when the probe
    // fails: these are the sources that compile into this binary, and a change to any of them is a change
    // to what `id` claims to name.
    for p in build_input_paths(&manifest) {
        rerun_if_changed(&p);
    }

    let (id, source, dirty) = match probe_git(&manifest, &build_input_paths(&manifest)) {
        Some((hash, dirty)) => (format!("{hash}{config}"), "vcs", Some(dirty)),
        None => {
            println!(
                "cargo:warning=oracle-aether: no usable git revision for this tree, so \
                 serverBuild.source is \"declared\" rather than \"vcs\" (contract protocol.md §2.1). \
                 This is the documented source-tarball fallback; it never fabricates a hash."
            );
            let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0".into());
            (
                format!("no-vcs+pkg=oracle-aether-{pkg}{config}"),
                "declared",
                None,
            )
        }
    };

    let dirty_literal = match dirty {
        Some(d) => format!("Some({d})"),
        None => "None".to_string(),
    };
    // Emitted so `tests/server_build.rs` can RE-DERIVE `dirty` over the same paths this script measured,
    // instead of pinning a duplicate list that would agree with the script only until someone edited one
    // of them. The scope is part of what the flag means, so it travels with the flag.
    let scope_literal = build_input_paths(&manifest)
        .iter()
        .map(|p| format!("{:?}", p.display().to_string()))
        .collect::<Vec<_>>()
        .join(", ");

    let generated = format!(
        "// @generated by crates/oracle-aether/build.rs — do not edit.\n\
         //\n\
         // Compile-time `serverBuild` (contract protocol.md §2.1 / §11.23). `include!`d, which §2.1's ⚑\n\
         // clause names as conformant: the line it draws is compile time versus run time, not file\n\
         // versus constant.\n\
         pub const SERVER_BUILD_ID: &str = {id:?};\n\
         pub const SERVER_BUILD_SOURCE: &str = {source:?};\n\
         pub const SERVER_BUILD_DIRTY: Option<bool> = {dirty_literal};\n\
         pub const SERVER_BUILD_DIRTY_SCOPE: &[&str] = &[{scope_literal}];\n"
    );
    std::fs::write(out.join("build_info.rs"), generated).expect("write build_info.rs");
}

/// This crate's own enabled features, canonicalised: lowercase, `_` → `-`, sorted, comma-joined.
///
/// Cargo hands a build script one `CARGO_FEATURE_<NAME>` per enabled feature, uppercased with `-`
/// replaced by `_`. The round trip back is lossy for a feature whose real name contains `_`, which is why
/// the value is documented as an opaque id component rather than a parseable list — §2.1 says `id` is
/// opaque and MUST NOT be parsed, so lossiness here costs nothing a consumer is allowed to rely on.
fn enabled_features() -> String {
    let mut v: Vec<String> = std::env::vars()
        .filter_map(|(k, _)| k.strip_prefix("CARGO_FEATURE_").map(str::to_string))
        .map(|f| f.to_ascii_lowercase().replace('_', "-"))
        .collect();
    v.sort();
    v.join(",")
}

/// Everything that compiles into this binary, as paths.
///
/// Two jobs, and the second is the one the ruling cares about. They are the `rerun-if-changed` set, so an
/// edit to any of them re-runs this script and recomputes `dirty`; and they are the scope `dirty` is
/// measured over, so the flag means "uncommitted changes in the sources that built *this binary*" and the
/// two agree by construction.
///
/// **Why scope `dirty` rather than ask the whole repo.** A repo-wide `git status` reports a modified
/// `docs/*.md` as dirty — but that build is byte-identical to the clean-commit build, so under §2.1's
/// differ-rule (*"`id` MUST differ between any two builds whose observable behaviour on this bus can
/// differ"*) the honest answer is `false`. More decisively: nothing can declare `rerun-if-changed` over a
/// whole repository without either naming `target/` (an unconditional rebuild loop) or leaving the
/// declaration incomplete — and an incomplete declaration is the exact M2 defect this script exists to
/// avoid. A scope that can be declared **completely** is worth more than a wider one that goes stale.
///
/// `oracle-frontend` and `oracle-replay` are deliberately absent: neither links into this binary.
fn build_input_paths(manifest: &Path) -> Vec<PathBuf> {
    let ws = manifest.join("../..");
    vec![
        manifest.join("src"),
        manifest.join("Cargo.toml"),
        manifest.join("build.rs"),
        ws.join("crates/oracle-core/src"),
        ws.join("crates/oracle-core/Cargo.toml"),
        ws.join("Cargo.toml"),
        ws.join("Cargo.lock"),
    ]
}

fn rerun_if_changed(p: &Path) {
    println!("cargo:rerun-if-changed={}", p.display());
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

/// The full commit hash and the dirty flag, plus the `rerun-if-changed` declarations M2 requires.
///
/// Every path is resolved with `git rev-parse --git-path`, never spelled `.git/…` by hand: this repo is
/// routinely built from a **linked worktree**, where `.git` is a file and `HEAD` lives at
/// `<common>/worktrees/<name>/HEAD`. Hand-spelling the path would declare a file that does not exist,
/// which Cargo accepts silently — a `rerun-if-changed` on a nonexistent path is not an error — and the
/// constant would then survive exactly the commit it is supposed to track. The M2 defect, reintroduced by
/// a string literal.
fn probe_git(manifest: &Path, dirty_scope: &[PathBuf]) -> Option<(String, bool)> {
    let hash = git(manifest, &["rev-parse", "HEAD"])?;
    if hash.len() != 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    // (a) HEAD itself — moves on checkout, and on commit when HEAD is detached.
    if let Some(p) = git(manifest, &["rev-parse", "--git-path", "HEAD"]) {
        rerun_if_changed(Path::new(&p));
    }
    // (b) The ref HEAD resolves to — this is the file a plain `git commit` rewrites, and the one M2 names
    //     beside HEAD. Absent when HEAD is detached, which is why it is conditional rather than assumed.
    if let Some(refname) = git(manifest, &["symbolic-ref", "-q", "HEAD"]) {
        if let Some(p) = git(manifest, &["rev-parse", "--git-path", &refname]) {
            rerun_if_changed(Path::new(&p));
        }
    }
    // (c) `packed-refs` — a branch whose loose ref file has been packed away has no file at (b), so
    //     without this the ref half of M2's requirement has a hole exactly on repositories old enough to
    //     have been gc'd.
    if let Some(p) = git(manifest, &["rev-parse", "--git-path", "packed-refs"]) {
        rerun_if_changed(Path::new(&p));
    }
    // (d) The index. This is the *staged* half of `dirty`: it is rewritten by `git add`, `git rm`,
    //     `git reset`, `git checkout` and `git stash`, none of which need touch a file in (a)-(c) or in
    //     the source scope. The working-tree half is covered by the source paths declared in `main`, and
    //     neither covers the other — a `git add` of an already-edited file changes no source mtime, and an
    //     editor save changes no index. Both are needed; that is the justification for including it.
    if let Some(p) = git(manifest, &["rev-parse", "--git-path", "index"]) {
        rerun_if_changed(Path::new(&p));
    }

    // `--no-optional-locks` keeps `git status` from writing back a refreshed index. Without it this call
    // can touch `.git/index`, which (d) has just declared as an input — so every build would dirty its own
    // trigger and the next build would re-run this script for no reason, forever.
    let mut args = vec!["--no-optional-locks", "status", "--porcelain", "--"];
    let scope: Vec<String> = dirty_scope
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    args.extend(scope.iter().map(String::as_str));
    let dirty = !git(manifest, &args)?.is_empty();

    Some((hash, dirty))
}
