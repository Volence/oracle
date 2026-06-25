//! The determinism gate — the most-guarded CI job.
//!
//! Native Rust port of the *logic* of Oracle's `determinism_gate.py`
//! (`../oracle/linux-port/harness/determinism_gate.py`): two fresh instances, reset to the stopped
//! power-on anchor, then a 120-frame loop capturing `state_hash.combined` after each frame; the two
//! sequences must be byte-identical.
//!
//! Oracle's version spawns two separate *processes* over the bus to also catch process-level
//! nondeterminism; that over-the-bus port lands with `oracle-bus`. This in-process version catches
//! logic-level nondeterminism — the only kind this core can have, since it has no globals, no
//! wall-clock, deterministic collections (no `HashMap` in state), and a single seeded RNG.

use oracle_core::system::System;

const FRAMES: usize = 120;
const SEED: u64 = 0xA5A5_5A5A_DEAD_BEEF;

/// One cold-boot run: power on, reset to the stopped anchor, then capture `combined` after each frame.
fn fresh_run(seed: u64) -> Vec<u64> {
    let mut sys = System::new(seed);
    sys.reset(); // stopped anchor (matches the gate's `reset {run:false}`)
    let mut seq = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        sys.run_frames(1);
        seq.push(sys.state_hash().combined);
    }
    seq
}

#[test]
fn two_fresh_instances_are_byte_identical() {
    let a = fresh_run(SEED);
    let b = fresh_run(SEED);
    assert_eq!(a.len(), FRAMES);
    assert_eq!(
        a, b,
        "determinism gate FAILED: per-frame state_hash sequences diverged"
    );
}

#[test]
fn gate_detects_divergence() {
    // The comparison has teeth: different power-on seeds must produce different sequences.
    assert_ne!(fresh_run(1), fresh_run(2));
}
