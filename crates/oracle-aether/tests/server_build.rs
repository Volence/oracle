//! **Server identity — contract `protocol.md` §2.1, registered 2026-08-26 by §11.23 (CR-C).**
//!
//! `initialize` now says *which implementation answered* and *which build of it*. The wire shape is a
//! schema fragment and is checked by the ordinary handshake path; what lives here is everything §2.1
//! requires that **no fragment can express**, which is most of the clause:
//!
//! 1. **Non-forgeability.** *"A server MUST NOT make either settable by config file, config struct,
//!    environment variable, command-line flag or bus method."* A schema sees a string; whether that
//!    string could have come from a flag is a property of the source. So this file reads the source.
//! 2. **Structural emission.** *"`serverBuild.id` … MUST be fixed when the binary is produced and MUST
//!    NOT be read at run time."* Asserting the value is *correct* is a different test from asserting it
//!    has no run-time source, and only the second one catches a server that reads a file and gets it
//!    right.
//! 3. **Invalidation** (ruling M2). *"The constant must be invalidated by what it names."* A cached
//!    build-script product that survives a commit is non-conformant even though every value it emits is
//!    well-formed. The check is that the compiled-in id equals what the tree says **right now** — which
//!    is self-enforcing: if `build.rs` ever stops re-running, this goes red on the next commit rather
//!    than on the next audit.
//! 4. **M1's fold-in.** Under `"vcs"` the id is the revision *extended by* the build configuration that
//!    changes the served surface. That the extension is present is checkable; that it is **sufficient**
//!    is checked by `no_cfg_feature_in_this_crate_escapes_the_build_id`, which fails if this crate ever
//!    reads a feature `build.rs` cannot see.
//!
//! Nothing here pins a hash, a count or a path as a literal: every expectation is re-derived from git,
//! from Cargo's own metadata, or from the source tree.

mod common;

use common::{spawn, Client};
use oracle_aether::build_info::{
    IMPLEMENTATION, SERVER_BUILD_DIRTY, SERVER_BUILD_DIRTY_SCOPE, SERVER_BUILD_ID,
    SERVER_BUILD_SOURCE,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` under this crate's `src/`, as (path, contents). The population is walked, never listed.
fn src_files() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for e in std::fs::read_dir(dir).expect("read src dir").flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let body = std::fs::read_to_string(&p).expect("read a source file");
                out.push((p, body));
            }
        }
    }
    let mut out = Vec::new();
    walk(&crate_root().join("src"), &mut out);
    assert!(
        !out.is_empty(),
        "no source files were found under {}/src — this test would then be vacuous, and a source-level \
         conformance check that scans nothing is exactly the failure §8 item 15 was written about",
        crate_root().display()
    );
    out
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(crate_root())
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

// ---------------------------------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------------------------------

#[test]
fn initialize_names_the_implementation_and_the_build() {
    let h = spawn("identity");
    let mut c = Client::connect(&h);
    let r = c.handshake(true);

    // §2.1's registry. The value is the contract's, not a display string: an unregistered value is
    // non-conformant, "which is the whole point of a registry".
    assert_eq!(
        r["implementation"],
        json!("oracle-rs"),
        "§2.1 (§11.23): this is the Rust `oracle-aether` server, and a consumer's \
         `implementation === 'oracle-rs'` must have exactly one meaning"
    );

    let b = &r["serverBuild"];
    assert!(
        b.is_object(),
        "§2.1: `serverBuild` is an object, not a bare string"
    );
    let id = b["id"].as_str().expect("serverBuild.id is a string");
    assert!(!id.is_empty(), "§2.1 / schema `minLength: 1`");
    let source = b["source"]
        .as_str()
        .expect("serverBuild.source is a string");
    assert!(
        ["vcs", "content", "declared"].contains(&source),
        "§2.1's `source` table has exactly three values; got {source:?}"
    );
    assert_eq!(
        b.get("dirty").is_some(),
        source == "vcs",
        "§2.1: `dirty` is REQUIRED when `source` is \"vcs\" and meaningless otherwise — the schema's \
         if/then is conditional on purpose, so emitting it unconditionally is as wrong as omitting it"
    );

    // §12.1 answer 4: `serverVersion` is now REQUIRED (it was in the example and in `properties` from
    // the beginning, and in neither `required` array). `serverName` survives as a DEPLOYMENT label — so
    // it is checked for presence and deliberately not for value; see `handshake.rs`.
    assert!(
        r["serverVersion"].is_string(),
        "§2.1 (§11.23): `serverVersion` is REQUIRED"
    );
    assert!(
        r["serverName"].is_string(),
        "§2.1: `serverName` remains REQUIRED — as a deployment label"
    );
}

// ---------------------------------------------------------------------------------------------------
// Non-forgeability (§2.1) — a source-level property, so read the source
// ---------------------------------------------------------------------------------------------------

#[test]
fn neither_identity_value_is_reachable_from_configuration() {
    // The names that carry the two values. Every place either is *mentioned* in this crate's source is
    // enumerated, and the set of files is what is asserted — because the violation §2.1 describes is not
    // a bad value, it is a value with a second way in.
    const NAMES: &[&str] = &[
        "IMPLEMENTATION",
        "SERVER_BUILD_ID",
        "SERVER_BUILD_SOURCE",
        "SERVER_BUILD_DIRTY",
    ];
    // `build_info.rs` declares them; `engine.rs` emits them into the handshake. Nothing else may touch
    // them — and `server.rs` (`ServerConfig`), `host.rs` and `main.rs` (argv) are precisely the files
    // §2.1 is talking about.
    const ALLOWED: &[&str] = &["build_info.rs", "engine.rs"];

    let mut offenders: Vec<String> = Vec::new();
    let mut touched: Vec<String> = Vec::new();
    for (path, body) in src_files() {
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        for (i, line) in body.lines().enumerate() {
            // Skip the doc-comment prose, which names these constants on purpose.
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if NAMES.iter().any(|n| line.contains(n)) {
                touched.push(format!("{file}:{}", i + 1));
                if !ALLOWED.contains(&file.as_str()) {
                    offenders.push(format!("{file}:{}: {}", i + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        !touched.is_empty(),
        "the identity constants are referenced nowhere in this crate's code — the scan matched nothing, \
         so every assertion below it would be vacuous"
    );
    assert!(
        offenders.is_empty(),
        "§2.1 (§11.23): `implementation` / `serverBuild` MUST NOT be settable by config file, config \
         struct, environment variable, command-line flag or bus method. These references live outside \
         {ALLOWED:?}, which is where a configuration path would have to appear:\n  {}",
        offenders.join("\n  ")
    );

    // The other half: the module that declares them must not read anything at run time. `build_info.rs`
    // is checked in, so this is a claim about the file in the repository, not about the generated one.
    //
    // Comments are stripped first, and that is not a convenience: this file's doc comment *quotes* the
    // barred forms while explaining why they are barred, so a raw `contains` would fail on the prose that
    // documents the rule. The claim is about code.
    let decl_raw =
        std::fs::read_to_string(crate_root().join("src/build_info.rs")).expect("build_info.rs");
    let decl: String = decl_raw
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        decl.contains("pub const IMPLEMENTATION"),
        "stripping comments removed the declarations too — the filter is wrong and every check below it \
         would pass on an empty string"
    );
    for forbidden in ["env::var", "std::env", "read_to_string", "File::open"] {
        assert!(
            !decl.contains(forbidden),
            "src/build_info.rs contains `{forbidden}` — §2.1's ⚑ clause bars reading the identity at run \
             time from a file, an environment variable, a generated config, a flag or a sibling process"
        );
    }
    assert!(
        decl.contains("include!(concat!(env!(\"OUT_DIR\")"),
        "src/build_info.rs must take the build values by `include!`ing the build script's product — the \
         arrangement §2.1's ⚑ clause names as conformant. `env!` is a COMPILE-time macro and is the one \
         permitted use here; `env::var` (barred above) is the run-time one."
    );
}

/// **Structural emission, proven by the compiler rather than by inspection.**
///
/// A value read at run time cannot appear in a `const` initialiser. So this item is not an assertion at
/// all — it is a compile-time obligation, and it fails as a build error rather than a test failure, which
/// is the strongest form the check can take.
const _COMPILE_TIME_OR_NOTHING: (&str, &str, &str) =
    (IMPLEMENTATION, SERVER_BUILD_ID, SERVER_BUILD_SOURCE);

// ---------------------------------------------------------------------------------------------------
// Invalidation (ruling M2) and M1's fold-in
// ---------------------------------------------------------------------------------------------------

#[test]
fn the_compiled_in_build_id_still_names_this_tree() {
    let Some(head) = git(&["rev-parse", "HEAD"]) else {
        // Loud, never silent: "cannot check" must not look like "checked".
        assert_eq!(
            SERVER_BUILD_SOURCE, "declared",
            "there is no git revision for this tree, so §2.1 requires `source: \"declared\"` — a `vcs` \
             id here would be a fabricated hash, which is the case §2.1 typed `declared` to prevent"
        );
        assert!(
            SERVER_BUILD_ID.starts_with("no-vcs+"),
            "the documented no-git fallback emits an id that cannot be mistaken for a revision"
        );
        assert!(
            SERVER_BUILD_DIRTY.is_none(),
            "§2.1: `dirty` is meaningful only under `vcs`"
        );
        return;
    };

    assert_eq!(SERVER_BUILD_SOURCE, "vcs", "this tree has a revision");
    assert_eq!(
        SERVER_BUILD_ID.split('+').next().unwrap(),
        head,
        "**M2: the constant must be invalidated by what it names.** The compiled-in build id names a \
         different commit than HEAD, which means `build.rs` did not re-run when the revision moved — a \
         cached build-script product surviving a change to its inputs, which §2.1's ⚑ clause calls a \
         self-report by another route and non-conformant. Check the `cargo:rerun-if-changed` \
         declarations on `.git/HEAD`, on the ref it resolves to, on `packed-refs` and on the index."
    );
    assert_eq!(
        head.len(),
        40,
        "§2.1 SHOULD: the revision part is the FULL identifier, not an abbreviation"
    );

    // The dirty half of the same property, re-derived over the scope `build.rs` measured (which it emits
    // for this purpose, so the two cannot drift apart).
    assert!(
        !SERVER_BUILD_DIRTY_SCOPE.is_empty(),
        "the build script declared no dirty scope, so the flag below means nothing"
    );
    let mut args: Vec<&str> = vec!["--no-optional-locks", "status", "--porcelain", "--"];
    args.extend(SERVER_BUILD_DIRTY_SCOPE.iter().copied());
    let now = git(&args).expect("git status over the build-input scope");
    assert_eq!(
        SERVER_BUILD_DIRTY,
        Some(!now.is_empty()),
        "the compiled-in `dirty` disagrees with the working tree over the very paths the build script \
         measured. Same M2 defect as above, on the other input.\n  git status said:\n{now}"
    );
}

#[test]
fn the_build_id_folds_in_the_configuration_that_changes_the_served_surface() {
    if SERVER_BUILD_SOURCE != "vcs" {
        eprintln!("NOTE: no version control for this tree; the `vcs` fold-in rule does not apply");
        return;
    }
    let (rev, ext) = SERVER_BUILD_ID.split_once('+').expect(
        "M1: a bare revision is not a conformant `vcs` id for a tree with build-time selection",
    );
    assert!(
        rev.len() == 40 && rev.chars().all(|c| c.is_ascii_hexdigit()),
        "the revision part comes FIRST and whole (§2.1's SHOULD), so a consumer can resolve it back to \
         a commit; got {rev:?}"
    );
    for key in ["profile=", "target=", "features="] {
        assert!(
            ext.contains(key),
            "M1: the id must be the revision EXTENDED BY the build-time selection that changes the \
             served surface. `{key}` is missing from {ext:?}."
        );
    }
    // Derived cross-checks, so the extension is the *actual* configuration and not three constant
    // strings that would agree with themselves forever.
    let expected_profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    assert!(
        ext.contains(&format!("profile={expected_profile}")),
        "the id's profile does not match the profile this test was compiled under ({expected_profile}). \
         The profile is load-bearing rather than decorative: `engine.rs`'s checkpoint_restore carries a \
         `#[cfg(debug_assertions)] debug_assert!`, so two builds of one clean commit answer \
         `emulator/restore` differently — the differ-rule's own case.\n  id: {SERVER_BUILD_ID}"
    );
    assert!(
        ext.contains(&format!("target={}", std::env::consts::ARCH)),
        "the id's target does not name the architecture this test runs on. The whole crate is \
         `#![cfg(unix)]`, so the target gates the entire served surface.\n  id: {SERVER_BUILD_ID}"
    );
}

#[test]
fn no_cfg_feature_in_this_crate_escapes_the_build_id() {
    // **The completeness check behind M1's `features=` component.** Cargo tells a build script this
    // crate's OWN features (`CARGO_FEATURE_*`) and nothing about a dependency's features turned on by
    // workspace unification. That is only a gap if this crate reads a cfg the build script cannot see —
    // so the gap is closed here rather than described in a comment: every `cfg(feature = "X")` in this
    // crate's source must name a feature THIS crate declares.
    let manifest = std::fs::read_to_string(crate_root().join("Cargo.toml")).expect("Cargo.toml");
    let declared: Vec<String> = manifest
        .split("[features]")
        .nth(1)
        .map(|s| {
            s.split("\n[")
                .next()
                .unwrap()
                .lines()
                .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim().to_string()))
                .filter(|k| !k.is_empty() && !k.starts_with('#'))
                .collect()
        })
        .unwrap_or_default();

    let mut used: Vec<String> = Vec::new();
    for (path, body) in src_files() {
        for (i, line) in body.lines().enumerate() {
            let mut rest = line;
            while let Some(at) = rest.find("cfg(feature") {
                rest = &rest[at + "cfg(feature".len()..];
                let Some(open) = rest.find('"') else { break };
                let after = &rest[open + 1..];
                let Some(close) = after.find('"') else { break };
                used.push(format!("{}|{}:{}", &after[..close], path.display(), i + 1));
                rest = &after[close + 1..];
            }
        }
    }
    println!(
        "cfg(feature) sites in oracle-aether/src: {} ; features declared by this crate: {declared:?}",
        used.len()
    );
    for entry in &used {
        let (name, whence) = entry.split_once('|').unwrap();
        assert!(
            declared.iter().any(|d| d == name),
            "{whence} reads `cfg(feature = \"{name}\")`, but `{name}` is not a feature THIS crate \
             declares — so Cargo sets no `CARGO_FEATURE_{}` for the build script, the feature never \
             reaches `serverBuild.id`, and two builds with different served surfaces would carry the \
             SAME id. That is the §2.1 differ-rule violation ruling M1 was written about. Declare a \
             feature of this crate that forwards to it (`{name} = [\"oracle-core/{name}\"]`).",
            name.to_ascii_uppercase().replace('-', "_")
        );
    }
}
