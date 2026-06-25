//! Property tests for the core determinism invariants (the gate's siblings).
//!
//! - `run_frames(N) == N × run_frames(1)` — frame stepping composes.
//! - snapshot → restore preserves state exactly.
//! - two fresh instances from the same seed are identical.

use oracle_core::system::System;
use proptest::prelude::*;

fn combined_after(seed: u64, frames: u64) -> u64 {
    let mut s = System::new(seed);
    s.run_frames(frames);
    s.state_hash().combined
}

proptest! {
    #[test]
    fn run_frames_n_equals_n_times_one(seed: u64, n in 0u64..16) {
        let bulk = combined_after(seed, n);
        let mut step = System::new(seed);
        for _ in 0..n {
            step.run_frames(1);
        }
        prop_assert_eq!(bulk, step.state_hash().combined);
    }

    #[test]
    fn snapshot_restore_preserves_state(seed: u64, n in 0u64..16) {
        let mut s = System::new(seed);
        s.run_frames(n);
        let back = System::restore(&s.snapshot()).expect("snapshot should decode");
        prop_assert_eq!(s.state_hash(), back.state_hash());
        prop_assert!(s == back);
    }

    #[test]
    fn two_fresh_instances_identical(seed: u64, n in 0u64..16) {
        prop_assert_eq!(combined_after(seed, n), combined_after(seed, n));
    }
}
