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
//!   keep the live-tree default, because these are hand-run tools whose whole ergonomic value is
//!   `cargo run --example …` with no arguments, but make the dependency **loud**: [`announce`] states
//!   at startup which tree was read, that it is not frozen, and how old the file is, so a stale read
//!   announces itself instead of passing silently as a measurement.
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

/// Aeon's live working tree — another lane's working directory. Named once, here, so that every site
/// that still has to read it is greppable from a single identifier rather than from a string literal
/// repeated across files.
pub const LIVE_AEON_DIR: &str = "/home/volence/sonic_hacks/aeon";

/// Path to one of the six frozen artifacts.
pub fn frozen(name: &str) -> String {
    format!("{FROZEN_DIR}/{name}")
}

/// Path to an Aeon build product that has **no** frozen copy. Every caller must pair this with
/// [`announce`] (or an equivalent per-row marker), or the dependency goes back to being silent.
pub fn live_aeon(name: &str) -> String {
    format!("{LIVE_AEON_DIR}/{name}")
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
    println!("  ⚠ NOT FROZEN — outside fixtures/aeon/, so these bytes are whatever was on disk when");
    println!("    this run read them ({age}). If the path is in Aeon's working");
    println!("    tree, that tree is rebuilt without warning and this run is not reproducible from");
    println!("    the repository alone.");
}
