//! The determinism gate — the most-guarded CI job.
//!
//! Native Rust port of the *logic* of Oracle's `determinism_gate.py`
//! (`../oracle-old/linux-port/harness/determinism_gate.py`): two fresh instances, reset to the stopped
//! power-on anchor, then a 120-frame loop capturing `state_hash.combined` after each frame; the two
//! sequences must be byte-identical.
//!
//! Oracle's version spawns two separate *processes* over the bus to also catch process-level
//! nondeterminism; that over-the-bus port lands with `oracle-bus`. This in-process version catches
//! logic-level nondeterminism — the only kind this core can have, since it has no globals, no
//! wall-clock, deterministic collections (no `HashMap` in state), and a single seeded RNG.
//!
//! Currency: with the stub gone and no VDP, the Oracle-compatible VDP `state_hash` would be near-constant,
//! so the gate hashes `export_state` (CPU regs + work RAM + fixed placeholders, integration-pivot D8) —
//! strictly stronger. The Oracle `state_hash` is unchanged and still drives the live-Oracle differential.

use oracle_core::system::System;

const FRAMES: usize = 120;
const SEED: u64 = 0xA5A5_5A5A_DEAD_BEEF;

/// One cold-boot run: power on, load the vendored test ROM, drive the power-on reset, then capture
/// `export_state_hash` after each frame. The real CPU runs the ROM's RAM-stirring loop, so the sequence
/// genuinely evolves frame to frame while staying byte-identical across instances.
fn fresh_run(seed: u64) -> Vec<u64> {
    let mut sys = System::new(seed);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset(); // power-on anchor (primes the CPU from the ROM vector table)
    let mut seq = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        sys.run_frames(1);
        seq.push(sys.export_state_hash());
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
