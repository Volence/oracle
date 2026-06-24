//! The `Scheduler` — the core's **sole** master clock and **sole** seeded RNG.
//!
//! All time in the core is measured in master-clock (`mclk`) ticks (NTSC mclk ≈ 53.69 MHz; 68k = mclk/7,
//! Z80 = mclk/15). Pending hardware events live in an ordered map keyed by `(deadline_mclk, seq)` — a
//! `BTreeMap`, never a `HashMap`, so iteration/pop order is deterministic. The `seq` monotonic counter
//! gives a stable tiebreak when two events share a deadline, and is part of serialized state so it stays
//! unique across snapshot/restore.

use crate::rng::SplitMix64;
use std::collections::BTreeMap;

/// A scheduled hardware event. Phase 0 placeholder set; grows as the VDP/CPUs land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum EventKind {
    Scanline,
    HInt,
    VInt,
    FrameEnd,
}

/// Owns the master clock, the single seeded RNG, and the pending-event queue.
#[derive(Clone, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Scheduler {
    mclk: u64,
    seq: u64,
    rng: SplitMix64,
    events: BTreeMap<(u64, u64), EventKind>,
}

impl Scheduler {
    /// Create a scheduler at `mclk = 0` with its RNG seeded from `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            mclk: 0,
            seq: 0,
            rng: SplitMix64::new(seed),
            events: BTreeMap::new(),
        }
    }

    /// The current master-clock value.
    pub fn now(&self) -> u64 {
        self.mclk
    }

    /// Advance the master clock by `mclk` ticks (wrapping — identical in debug and release).
    pub fn advance(&mut self, mclk: u64) {
        self.mclk = self.mclk.wrapping_add(mclk);
    }

    /// Mutable access to the single RNG (used for power-on memory seeding).
    pub fn rng_mut(&mut self) -> &mut SplitMix64 {
        &mut self.rng
    }

    /// Schedule `kind` to fire at absolute `deadline_mclk`.
    pub fn schedule(&mut self, deadline_mclk: u64, kind: EventKind) {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.events.insert((deadline_mclk, seq), kind);
    }

    /// Remove and return the earliest pending event as `(deadline_mclk, kind)`, ties broken by
    /// insertion order. `None` if the queue is empty.
    pub fn pop_next(&mut self) -> Option<(u64, EventKind)> {
        let &key = self.events.keys().next()?;
        let kind = self.events.remove(&key).expect("key came from the map");
        Some((key.0, kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_master_clock() {
        let mut s = Scheduler::new(0);
        assert_eq!(s.now(), 0);
        s.advance(3420);
        s.advance(100);
        assert_eq!(s.now(), 3520);
    }

    #[test]
    fn pops_events_in_deadline_order() {
        let mut s = Scheduler::new(0);
        s.schedule(300, EventKind::VInt);
        s.schedule(100, EventKind::Scanline);
        s.schedule(200, EventKind::HInt);
        assert_eq!(s.pop_next(), Some((100, EventKind::Scanline)));
        assert_eq!(s.pop_next(), Some((200, EventKind::HInt)));
        assert_eq!(s.pop_next(), Some((300, EventKind::VInt)));
        assert_eq!(s.pop_next(), None);
    }

    #[test]
    fn ties_break_by_insertion_order() {
        let mut s = Scheduler::new(0);
        s.schedule(100, EventKind::Scanline);
        s.schedule(100, EventKind::HInt);
        s.schedule(100, EventKind::VInt);
        assert_eq!(s.pop_next(), Some((100, EventKind::Scanline)));
        assert_eq!(s.pop_next(), Some((100, EventKind::HInt)));
        assert_eq!(s.pop_next(), Some((100, EventKind::VInt)));
    }

    #[test]
    fn same_seed_yields_same_rng_stream() {
        let mut a = Scheduler::new(42);
        let mut b = Scheduler::new(42);
        for _ in 0..100 {
            assert_eq!(a.rng_mut().next_u64(), b.rng_mut().next_u64());
        }
    }
}
