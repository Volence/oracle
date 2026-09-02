//! Where a hand-run example's default ROM comes from, and how it says so out loud.
//!
//! **The problem this exists to fix.** Several examples defaulted to an absolute path inside
//! `/home/volence/sonic_hacks/aeon` — another lane's *live working tree*. That directory is rebuilt
//! without warning, so every such run read whatever happened to be on disk at that moment, and a run
//! against a rebuilt image was indistinguishable from a run against the one the numbers were written
//! for. `fixtures/aeon/` exists precisely to end that dependency for the artifacts we froze
//! (`fixtures/aeon/PROVENANCE.md`).
//!
//! **The rule, applied uniformly by the examples that use this module:**
//!
//! * **A frozen artifact exists** (`s4.bin`, `s4.debug.bin`, and the four `.lst` listings) — default to
//!   [`frozen`]. Deterministic, committed, always present, attributable to a chain.
//! * **No frozen artifact exists** (`s4.soundtest.bin`, and `demo.bin` which we chose not to freeze) —
//!   there is **no default at all**: [`unfrozen`] resolves the directory from named environment
//!   variables and otherwise **refuses**, naming every variable it consulted. When a path *is* resolved,
//!   [`announce`] states which tree was read, that it is not frozen, and how old the file is, so a stale
//!   read announces itself instead of passing silently as a measurement.
//!
//! **Why the live-tree default went away (2026-09-02).** Until then this module carried
//! `LIVE_AEON_DIR = "/home/volence/sonic_hacks/aeon"` and handed it out as a default. That was a *home
//! literal into another lane's working directory* — the shape `empyrean` `contract/SUITE_PATHS.md`
//! at `38f6df4` rules out in terms: precedence is the explicit checkout variable, then
//! `EMPYREAN_SUITE_ROOT` joined with the repo's directory name, then derivation, then *"refuse, naming
//! what was looked for and where. Never a home literal, and never a silent fallback to the live tree."*
//!
//! **And derivation is deliberately NOT implemented here**, which is the part worth reading twice. The
//! same contract makes step 3 legitimate for answering *which checkout* and refuses it for
//! **reference-dependent measurement**, "because it derives the owner's live tree whose revision moves
//! under a run". Reading a ROM to produce numbers is exactly that, and unlike a git-tracked contract
//! file there is nothing to pin these against: `s4.soundtest.bin` is a build output absent from sigil's
//! chain-attested goldens, so no revision names it. A walk would therefore hand these examples an
//! unattributable tree while looking like resolution. Two steps and a refusal is the honest shape.
//!
//! The cost is stated rather than hidden: `cargo run --example diag_soundqueue` with no arguments no
//! longer works on its own. It works with `ORACLE_AEON_DIR=…`, `AEON_DIR=…` or `EMPYREAN_SUITE_ROOT=…`
//! in front of it, and the refusal prints those three names, so the fix is readable from the message.
//!
//! `s4.soundtest.bin` cannot be frozen on the current recipe at all: sigil's committed goldens are the
//! authority for ROM bytes (`fixtures/aeon/PROVENANCE.md`, *"The ROMs — from sigil's committed golden
//! blobs"*) and that image is not among them — verified at the pinned freeze `39c34fd2`, whose golden
//! set is `s4.bin`, `s4.debug.bin`, `demo.bin`, `demo.debug.bin`, `config_a.bin`, `config_b.bin`,
//! `lean.bin`. There is nothing chain-attested to take it from.
//!
//! This module is `examples/common/rom_source.rs` rather than four copies of the same paragraph.
//! Cargo's example auto-discovery finds `examples/*.rs` and `examples/*/main.rs`, so a plain file in a
//! subdirectory is not itself built as an example; each consumer pulls it in with
//! `#[path = "common/rom_source.rs"] mod rom_source;`.

// Each consumer uses a different subset — this module is included by source path, so an unused helper
// is an artefact of that inclusion rather than dead code in the crate.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// This repo's own committed copy of Aeon's build artifacts. See `fixtures/aeon/PROVENANCE.md` for
/// which build is pinned, how the ROM/listing joint was proved, and how the pin moves.
pub const FROZEN_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/aeon");

/// A directory of Aeon build ARTIFACTS. `SUITE_PATHS.md` keeps this name distinct from a checkout
/// name on purpose: *"A variable that names a directory of artifacts rather than a checkout (oracle's
/// `ORACLE_AEON_DIR`, defaulting to a frozen copy) keeps its own name; it is not an alias of
/// `AEON_DIR`."* It is what `symbols_real_lst.rs` and `replay_real_artifacts.rs` already use.
pub const ENV_ARTIFACT_DIR: &str = "ORACLE_AEON_DIR";
/// Aeon's CHECKOUT, the contract's ratified spelling. Artifacts sit at its root (`build.sh` writes
/// `s4.bin` and friends in-tree), so the two coincide today — the fold `SUITE_PATHS.md` calls safe
/// "precisely while the build is in-tree".
pub const ENV_CHECKOUT_DIR: &str = "AEON_DIR";
/// The suite root every checkout hangs off.
pub const ENV_SUITE_ROOT: &str = "EMPYREAN_SUITE_ROOT";

/// Path to one of the six frozen artifacts.
pub fn frozen(name: &str) -> String {
    format!("{FROZEN_DIR}/{name}")
}

/// Where an Aeon build product that has **no** frozen copy may be read from — or a refusal saying why
/// not, ready to print.
///
/// Precedence, and a hard error rather than a fall-through when a variable is **set but wrong**: per
/// `SUITE_PATHS.md`, *"a wrong value is evidence of a wrong environment, and the next step would hide
/// it."* So a set variable is the answer, right or wrong, and its wrongness is named.
///
/// Every caller must pair a success with [`announce`] (or an equivalent per-row marker), or the
/// dependency on an unattributable tree goes back to being silent.
pub fn unfrozen(name: &str) -> Result<String, String> {
    let mut tried: Vec<String> = Vec::new();
    for (var, suffix) in [
        (ENV_ARTIFACT_DIR, ""),
        (ENV_CHECKOUT_DIR, ""),
        (ENV_SUITE_ROOT, "/aeon"),
    ] {
        match std::env::var(var) {
            Ok(dir) => {
                let path = format!("{dir}{suffix}/{name}");
                if Path::new(&path).is_file() {
                    return Ok(path);
                }
                return Err(format!(
                    "${var} is set to `{dir}`, so {path} is where `{name}` must be — and there is no \
                     readable file there.\nA variable that is set but wrong is a hard error, not a \
                     reason to keep looking: it is evidence of a wrong environment, and the next step \
                     would hide it (empyrean contract/SUITE_PATHS.md at 38f6df4)."
                ));
            }
            Err(_) => tried.push(format!("${var}{suffix}/{name} — {var} not set")),
        }
    }
    Err(format!(
        "`{name}` has no frozen copy in this repository and no Aeon directory was named.\n\
         Consulted, in order:\n  {}\n\
         There is deliberately no default and no filesystem walk. A home literal into another lane's \
         working tree is what this replaced, and a walk would hand this run an unattributable tree \
         while looking like resolution — `{name}` is a build output that no revision names, so nothing \
         could pin what the walk found (empyrean contract/SUITE_PATHS.md at 38f6df4).",
        tried.join("\n  ")
    ))
}

/// True when `path` resolves inside [`FROZEN_DIR`].
///
/// Canonicalised on both sides so a hand-typed path to the same file is recognised; falls back to a
/// lexical prefix test when either side cannot be resolved. The fallback errs towards reporting
/// *not* frozen, which is the safe direction: an over-loud warning costs a line, a missed one costs a
/// wrong measurement.
pub fn is_frozen(path: &str) -> bool {
    let canon = |p: &str| std::fs::canonicalize(p).ok();
    match (canon(path), canon(FROZEN_DIR)) {
        (Some(p), Some(dir)) => p.starts_with(dir),
        _ => Path::new(path).starts_with(PathBuf::from(FROZEN_DIR)),
    }
}

/// Print which image this run read and whether it is frozen.
///
/// Replaces the bare `ROM {path}: {n} bytes` line these examples used to print. The size alone cannot
/// distinguish a fresh image from a stale one — the chain-188 and chain-189 `s4.debug.bin` are both
/// exactly 736,315 bytes — so for an unfrozen path this also reports the file's age, which is the
/// cheapest signal available here that says *when* these bytes were produced. (A hash would say it
/// better; `oracle-core` deliberately carries one dependency, and a dev example is not the place to
/// add a second.)
pub fn announce(path: &str, len: usize) {
    println!("ROM {path}: {len} bytes");
    if is_frozen(path) {
        println!("  FROZEN — this repo's own committed copy (fixtures/aeon/PROVENANCE.md)");
        return;
    }
    let age = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
        .map(|d| {
            let s = d.as_secs();
            format!("last modified {} h {} min ago", s / 3600, (s % 3600) / 60)
        })
        .unwrap_or_else(|| "last-modified time UNREADABLE".to_string());
    println!(
        "  ⚠ NOT FROZEN — outside fixtures/aeon/, so these bytes are whatever was on disk when"
    );
    println!("    this run read them ({age}). If the path is in Aeon's working");
    println!(
        "    tree, that tree is rebuilt without warning and this run is not reproducible from"
    );
    println!("    the repository alone.");
}
