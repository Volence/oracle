//! `oracle-core` — the deterministic, no-I/O Sega Genesis / Mega Drive emulation core.
//!
//! Architectural invariants, enforced from commit one (see `docs/foundations.md`):
//! - One `System` struct owns all memory + chips + the `Scheduler`.
//! - The `Scheduler` owns the *sole* master clock and *one* seeded RNG. Deterministic is the only mode.
//! - Chips are generic over `&mut impl Bus` (split-borrow); no `Rc`/`RefCell`/`unsafe` on the hot path.
//! - The whole machine is plain owned data: `Clone` + bincode-serializable, O(struct) snapshot.
//! - No `HashMap` or floats in hashed/serialized state; zero threads in core.

#![forbid(unsafe_code)]

pub mod bus;
pub mod m68000;
pub mod rng;
pub mod scheduler;
pub mod state_hash;
pub mod stub_cpu;
pub mod system;
