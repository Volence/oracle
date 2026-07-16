//! Property tests for the core determinism invariants (the gate's siblings).
//!
//! - `run_frames(N) == N × run_frames(1)` — frame stepping composes.
//! - snapshot → restore preserves state exactly.
//! - two fresh instances from the same seed are identical.

use oracle_core::system::System;
use proptest::prelude::*;

/// A booted machine running the vendored test ROM, exercised for `frames`; returns the `export_state_hash`
/// (CPU regs + work RAM — the gate currency). The real CPU actually executes, so these invariants have
/// teeth beyond the near-constant VDP `state_hash`.
fn export_after(seed: u64, frames: u64) -> u64 {
    let mut s = System::new(seed);
    s.load_rom(oracle_core::testrom::build());
    s.reset();
    s.run_frames(frames);
    s.export_state_hash()
}

fn booted(seed: u64) -> System {
    let mut s = System::new(seed);
    s.load_rom(oracle_core::testrom::build());
    s.reset();
    s
}

proptest! {
    #[test]
    fn run_frames_n_equals_n_times_one(seed: u64, n in 0u64..16) {
        // The overshoot-carry invariant on the real CPU: absolute frame deadlines make bulk stepping
        // identical to single-frame stepping.
        let bulk = export_after(seed, n);
        let mut step = booted(seed);
        for _ in 0..n {
            step.run_frames(1);
        }
        prop_assert_eq!(bulk, step.export_state_hash());
    }

    #[test]
    fn snapshot_restore_preserves_state(seed: u64, n in 0u64..16) {
        let mut s = booted(seed);
        s.run_frames(n);
        let back = System::restore(&s.snapshot()).expect("snapshot should decode");
        prop_assert_eq!(s.export_state_hash(), back.export_state_hash());
        prop_assert!(s == back);
    }

    #[test]
    fn two_fresh_instances_identical(seed: u64, n in 0u64..16) {
        prop_assert_eq!(export_after(seed, n), export_after(seed, n));
    }
}
