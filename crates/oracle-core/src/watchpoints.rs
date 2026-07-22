//! Bus-level recording watchpoints — the "who wrote this?" root-causing primitive
//! (`docs/2026-07-20-diagnostic-tooling-ideas.md`).
//!
//! A [`Watchpoints`] is a pure **consumer** of the bus event stream ([`crate::bus::BusEventSink`]): register
//! one or more address ranges to watch, pass it as the sink to a sink-generic run
//! (`System::run_frames_with_sink` / `run_until_with_sink`), and read back a log of every access that hit a
//! watched range — each hit attributed to the instruction that drove it (its PC) and to the master that drove
//! it (CPU vs DMA/other, via the event's function code) plus the value, size, op, and frame.
//!
//! It observes only: it never touches CPU or memory state, is stored by the *caller* (never by `System`), and
//! so sits in neither frozen currency and can never move a state hash. The null-sink hot path is untouched —
//! attaching a `Watchpoints` is what makes the machine observable, detaching it makes it a black box again.
//!
//! ## Attribution (how a hit learns its PC)
//!
//! A [`crate::bus::BusEvent`] is emitted deep inside a CPU access and carries no PC. The sink-generic run loop
//! instead calls [`BusEventSink::on_step_boundary`] once immediately before each CPU step, stamping the PC of
//! the instruction about to execute (and the current frame); `Watchpoints` latches that context, so every
//! event that follows — until the next boundary — is attributed to that one instruction. An instruction that
//! drives several accesses (a `MOVEM`, a read-modify-write) attributes them all to its own PC, which is
//! exactly right.
//!
//! ## v1 scope / v2
//!
//! v1 watches the 68000 bus address space (work RAM, ROM, Z80 RAM, I/O, VDP *ports* — anywhere a
//! `BusEvent` is emitted). It **records**, it does not break: hits land in a bounded ring buffer (drop-oldest,
//! with a drop count); there is no execution halt (the core runs frame-batched, not instruction-stepped from a
//! UI, so break-on-hit pairs with a future stepping frontend — a v2). Also deferred to v2: **VDP-internal**
//! VRAM/CRAM/VSRAM *byte-address* watchpoints (the "who wrote this tile" case) — that needs the event stream
//! extended into `vdp.rs` and the CPU-PC/DMA context carried into the VDP, and it touches currency-sensitive
//! code; it is designed after v1 proves this API + attribution shape.

use crate::bus::{BusEvent, BusEventSink, BusOp, Size};
use std::ops::RangeInclusive;

/// Which bus operations a watch matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchOp {
    /// Match reads only.
    Read,
    /// Match writes only. The 68000 TAS is an indivisible read-modify-write whose *write* is the point of a
    /// "who modified this?" watch, so a `Write` watch also matches [`BusOp::Tas`] (the RMW write cycle);
    /// a `Read` watch does not (TAS is fundamentally a store).
    Write,
    /// Match any access (read, write, or TAS).
    Any,
}

impl WatchOp {
    /// Whether this filter matches a bus operation.
    fn matches(self, op: BusOp) -> bool {
        match self {
            WatchOp::Any => true,
            WatchOp::Read => op == BusOp::Read,
            WatchOp::Write => op == BusOp::Write || op == BusOp::Tas,
        }
    }
}

/// One registered watch: an inclusive address range, an op filter, and a human label (for the caller's own
/// bookkeeping when several watches are active).
struct WatchSpec {
    lo: u32,
    hi: u32,
    op: WatchOp,
    #[allow(dead_code)]
    // registered for the caller's reference; not propagated into v1 hits (see module docs).
    label: String,
}

/// A recorded access that hit a watch. `pc` is the instruction that drove it (from the step-boundary stamp);
/// `fc` is the 68000 function code of the access (5 = supervisor data, 6 = supervisor program; a non-CPU
/// master such as DMA reports 0), so a hit attributes to both *which instruction* and *which master* touched
/// the address. `seq` is a monotonic id assigned to every matched access in order — stable across ring-buffer
/// drops, so a gap in `seq` marks dropped hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchHit {
    pub addr: u32,
    pub value: u32,
    pub size: Size,
    pub op: BusOp,
    pub fc: u8,
    pub pc: u32,
    pub frame: u64,
    pub seq: u64,
}

/// A recording watchpoint facility — a [`BusEventSink`] the caller owns and passes to a sink-generic run.
pub struct Watchpoints {
    specs: Vec<WatchSpec>,
    hits: Vec<WatchHit>,
    cap: usize,
    dropped: u64,
    seq: u64,
    cur_pc: u32,
    cur_frame: u64,
}

impl Watchpoints {
    /// A facility whose hit log holds at most `cap` entries (drop-oldest past that, counted by
    /// [`dropped`](Self::dropped)). `cap` should be ≥ 1; `cap = 0` records nothing (every hit is counted as a
    /// drop).
    pub fn new(cap: usize) -> Self {
        Self {
            specs: Vec::new(),
            hits: Vec::new(),
            cap,
            dropped: 0,
            seq: 0,
            cur_pc: 0,
            cur_frame: 0,
        }
    }

    /// Register an inclusive address range to watch for `op` accesses, tagged with `label`.
    pub fn add_watch(&mut self, range: RangeInclusive<u32>, op: WatchOp, label: impl Into<String>) {
        self.specs.push(WatchSpec {
            lo: *range.start(),
            hi: *range.end(),
            op,
            label: label.into(),
        });
    }

    /// Remove all registered watches (the inverse of [`add_watch`](Self::add_watch)). Recorded hits are left
    /// intact — drain them with [`take_hits`](Self::take_hits).
    pub fn clear(&mut self) {
        self.specs.clear();
    }

    /// The recorded hits, oldest first.
    pub fn hits(&self) -> &[WatchHit] {
        &self.hits
    }

    /// Drain and return the recorded hits, oldest first, leaving the log empty.
    pub fn take_hits(&mut self) -> Vec<WatchHit> {
        std::mem::take(&mut self.hits)
    }

    /// How many hits were dropped (oldest-first) because the log was at capacity.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

impl BusEventSink for Watchpoints {
    fn on_event(&mut self, event: BusEvent) {
        // Match against every registered watch first (immutable borrow ends here, before the record below).
        let matched = self
            .specs
            .iter()
            .any(|s| s.op.matches(event.op) && (s.lo..=s.hi).contains(&event.addr));
        if !matched {
            return;
        }
        // Every matched access gets the next sequence id in order, whether or not it fits in the ring.
        let seq = self.seq;
        self.seq += 1;
        if self.hits.len() >= self.cap {
            // At capacity: drop the oldest to bound memory (or, for cap == 0, drop this hit outright).
            self.dropped += 1;
            if self.hits.is_empty() {
                return;
            }
            self.hits.remove(0);
        }
        self.hits.push(WatchHit {
            addr: event.addr,
            value: event.value,
            size: event.size,
            op: event.op,
            fc: event.fc,
            pc: self.cur_pc,
            frame: self.cur_frame,
            seq,
        });
    }

    fn on_step_boundary(&mut self, pc: u32, frame: u64) {
        self.cur_pc = pc;
        self.cur_frame = frame;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed one write event to a Write watch on its address; it is recorded with the stamped pc/frame.
    #[test]
    fn records_a_write_hit_with_stamped_pc_and_frame() {
        let mut wp = Watchpoints::new(16);
        wp.add_watch(0xFF_0000..=0xFF_0001, WatchOp::Write, "target");
        wp.on_step_boundary(0x0212, 3);
        wp.on_event(BusEvent {
            op: BusOp::Write,
            fc: 5,
            addr: 0xFF_0000,
            size: Size::Word,
            value: 0x0001,
        });
        assert_eq!(
            wp.hits(),
            &[WatchHit {
                addr: 0xFF_0000,
                value: 0x0001,
                size: Size::Word,
                op: BusOp::Write,
                fc: 5,
                pc: 0x0212,
                frame: 3,
                seq: 0,
            }]
        );
        assert_eq!(wp.dropped(), 0);
    }

    fn ev(op: BusOp, addr: u32, value: u32) -> BusEvent {
        BusEvent {
            op,
            fc: 5,
            addr,
            size: Size::Byte,
            value,
        }
    }

    /// A Write watch ignores reads of the watched address; a Read watch ignores writes.
    #[test]
    fn op_filter_selects_reads_or_writes() {
        let mut write_only = Watchpoints::new(16);
        write_only.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Write, "w");
        write_only.on_event(ev(BusOp::Read, 0xFF_0000, 0));
        assert_eq!(
            write_only.hits().len(),
            0,
            "a read does not hit a Write watch"
        );
        write_only.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        assert_eq!(write_only.hits().len(), 1, "the write hits");

        let mut read_only = Watchpoints::new(16);
        read_only.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Read, "r");
        read_only.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        assert_eq!(
            read_only.hits().len(),
            0,
            "a write does not hit a Read watch"
        );
        read_only.on_event(ev(BusOp::Read, 0xFF_0000, 0));
        assert_eq!(read_only.hits().len(), 1, "the read hits");
    }

    /// A Write watch also catches the 68000 TAS (its RMW write cycle); a Read watch does not.
    #[test]
    fn write_watch_catches_tas_read_watch_does_not() {
        let mut write_only = Watchpoints::new(16);
        write_only.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Write, "w");
        write_only.on_event(ev(BusOp::Tas, 0xFF_0000, 0x80));
        assert_eq!(write_only.hits().len(), 1, "TAS is a write to the address");

        let mut read_only = Watchpoints::new(16);
        read_only.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Read, "r");
        read_only.on_event(ev(BusOp::Tas, 0xFF_0000, 0x80));
        assert_eq!(
            read_only.hits().len(),
            0,
            "a Read watch ignores the TAS store"
        );
    }

    /// An access outside every watched range records nothing.
    #[test]
    fn access_outside_the_range_is_not_recorded() {
        let mut wp = Watchpoints::new(16);
        wp.add_watch(0xFF_0000..=0xFF_00FF, WatchOp::Any, "range");
        wp.on_event(ev(BusOp::Write, 0xFF_0100, 1)); // one past the top
        wp.on_event(ev(BusOp::Write, 0xFE_FFFF, 1)); // one below the bottom
        assert_eq!(wp.hits().len(), 0);
        assert_eq!(wp.dropped(), 0);
    }

    /// The hit log is a bounded drop-oldest ring: past `cap`, the oldest hit is dropped, the drop is counted,
    /// and the retained hits keep their original monotonic `seq` (so a `seq` gap marks the drop).
    #[test]
    fn ring_buffer_drops_oldest_and_counts_and_keeps_seq() {
        let mut wp = Watchpoints::new(2);
        wp.add_watch(0xFF_0000..=0xFF_00FF, WatchOp::Any, "range");
        for i in 0..5u32 {
            wp.on_event(ev(BusOp::Write, 0xFF_0000, i));
        }
        let seqs: Vec<u64> = wp.hits().iter().map(|h| h.seq).collect();
        let vals: Vec<u32> = wp.hits().iter().map(|h| h.value).collect();
        assert_eq!(wp.hits().len(), 2, "log bounded at cap");
        assert_eq!(vals, vec![3, 4], "the two most recent hits are retained");
        assert_eq!(seqs, vec![3, 4], "retained hits keep their original seq");
        assert_eq!(wp.dropped(), 3, "the first three hits were dropped");
    }

    /// `Any` matches every op; `take_hits` drains the log; `clear` removes the registered watches.
    #[test]
    fn any_matches_all_ops_take_hits_drains_clear_removes_watches() {
        let mut wp = Watchpoints::new(16);
        wp.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Any, "any");
        wp.on_event(ev(BusOp::Read, 0xFF_0000, 0));
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        wp.on_event(ev(BusOp::Tas, 0xFF_0000, 0x80));
        let drained = wp.take_hits();
        assert_eq!(drained.len(), 3, "Any matched read, write, and TAS");
        assert_eq!(wp.hits().len(), 0, "take_hits left the log empty");

        wp.clear();
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 2));
        assert_eq!(
            wp.hits().len(),
            0,
            "clear removed the watch — nothing matches now"
        );
    }
}
