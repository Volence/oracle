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
pub mod io;
pub mod m68000;
pub mod render;
pub mod rng;
/// Per-scanline capture (`F-SCANLINE-CAPTURE`): one configurable [`scanline_capture::ScanlineCapture`] sink
/// replacing the ad-hoc first-wins / last-frame collectors. Caller-owned, never part of `System` /
/// `state_hash` / `export_state`.
pub mod scanline_capture;
pub mod scheduler;
pub mod state_hash;
/// `<rom>.lst` symbol table — name↔address resolution and the `$`-mangled scope tree. Pure (`&str` in,
/// no filesystem); caller-owned metadata about a ROM, never part of `System` / `state_hash` /
/// `export_state`.
pub mod symbols;
/// Native, opt-in sound synthesis (Phase SY). Feature-gated (`synth`, default OFF); a caller-owned sink
/// that is never part of `System` / `state_hash` / `export_state`.
#[cfg(feature = "synth")]
pub mod synth;
pub mod system;
/// Hand-authored 68000 test ROM fixture (see [`testrom::build`]). Not part of the stable API.
#[doc(hidden)]
pub mod testrom;
pub mod vdp;
pub mod vgm;
pub mod watchpoints;
pub mod ym2612;
pub mod z80;
