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
//! ## Spaces: bus (v1) + VDP-internal (v2)
//!
//! A watch lives in a [`WatchSpace`]. v1 [`add_watch`](Watchpoints::add_watch) watches the **68000 bus**
//! address space (work RAM, ROM, Z80 RAM, I/O, VDP *ports* — anywhere a `BusEvent` is emitted). v2
//! [`add_vdp_watch`](Watchpoints::add_vdp_watch) watches a **VDP-internal** byte-address space —
//! `Vram`/`Cram`/`Vsram` — the "who wrote this tile / palette entry?" case. A VDP-internal write happens
//! inside `vdp.rs` (after a data-port write decodes + autoincrements, and during DMA fills/copies), off the
//! bus stream, so it is delivered through a separate sink method: registering any VDP watch makes
//! [`wants_vdp_writes`](BusEventSink::wants_vdp_writes) true, which arms the VDP's write-capture buffer for the
//! run; each captured write arrives via [`on_vdp_write`](BusEventSink::on_vdp_write) and is attributed to the
//! same step-boundary PC as bus hits. A hit reports the resolved region address, old→new value, region,
//! driving PC, and [`WatchVia`] (Direct CPU write vs DMA step). Spaces never cross: a numeric address
//! collision between the bus space and a VDP space does not cross-trigger.
//!
//! This also **resolves v1's DMA-attribution gap**: a DMA writes VDP memory with `fc = 0` and never reaches
//! the bus event stream, so v1 could not attribute it; v2 captures it at the VDP write itself with
//! `via = Dma`, attributed to the instruction that triggered the transfer.
//!
//! Still **deferred**: break-on-hit / execution halt (the core runs frame-batched, not instruction-stepped
//! from a UI, so break-on-hit pairs with a future stepping frontend), and any frontend / MCP wiring. Recording
//! is bounded (drop-oldest ring with a drop count), never a halt.

use crate::bus::{BusEvent, BusEventSink, BusOp, Size};
use crate::vdp::{VdpTarget, VdpVia, VdpWrite};
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

/// Which address space a watch (and a hit) lives in. `Bus` is the v1 68000 bus address space; `Vram`/`Cram`/
/// `Vsram` are the v2 VDP-internal byte-address spaces (the "who wrote this tile" case). A watch only ever
/// matches accesses in its own space — a numeric address collision across spaces never cross-triggers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchSpace {
    Bus,
    Vram,
    Cram,
    Vsram,
}

/// How a recorded write reached its target. `Bus` is a v1 68000 bus access (the master is in [`WatchHit::fc`]:
/// 5/6 = CPU, 0 = a non-CPU master). `Direct`/`Dma` are v2 VDP-internal writes: `Direct` is a CPU data-port
/// write, `Dma` is a DMA step — attributed to the instruction that *triggered* the transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchVia {
    Bus,
    Direct,
    Dma,
}

/// One registered watch: the address space, an inclusive address range, an op filter, and a human label (for
/// the caller's own bookkeeping when several watches are active).
struct WatchSpec {
    space: WatchSpace,
    lo: u32,
    hi: u32,
    op: WatchOp,
    #[allow(dead_code)]
    // registered for the caller's reference; not propagated into hits (see module docs).
    label: String,
}

/// A recorded access that hit a watch. `pc` is the instruction that drove it (from the step-boundary stamp);
/// `fc` is the 68000 function code of the access (5 = supervisor data, 6 = supervisor program; a non-CPU
/// master such as DMA reports 0), so a hit attributes to both *which instruction* and *which master* touched
/// the address. `seq` is a monotonic id assigned to every matched access in order — stable across ring-buffer
/// drops, so a gap in `seq` marks dropped hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchHit {
    /// The address space this hit lives in (v1 bus hits are [`WatchSpace::Bus`]).
    pub space: WatchSpace,
    pub addr: u32,
    /// The value that was there **before** the access. Meaningful for VDP-internal writes (the pre-write
    /// value, so a hit reads old→new); `0` for bus accesses (the bus event stream carries no prior value).
    pub old: u32,
    /// The value read or written (for a VDP write, the new value).
    pub value: u32,
    /// The access width: [`Size::Byte`]/[`Size::Word`]/[`Size::Long`] for a bus access; a VDP write reports
    /// [`Size::Byte`] (a VRAM byte) or [`Size::Word`] (a CRAM/VSRAM entry).
    pub size: Size,
    /// The bus op for a bus access; always [`BusOp::Write`] for a VDP-internal write (a store).
    pub op: BusOp,
    /// The 68000 function code of a bus access (5/6 = CPU, 0 = non-CPU master); `0` for a VDP-internal write
    /// (there is no bus function code — the CPU-vs-DMA distinction is in [`WatchHit::via`]).
    pub fc: u8,
    /// How the write reached the target — the CPU-vs-DMA attribution for VDP writes ([`WatchVia::Bus`] for a
    /// v1 bus access).
    pub via: WatchVia,
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

    /// Register an inclusive **68000 bus** address range to watch for `op` accesses, tagged with `label` (v1).
    pub fn add_watch(&mut self, range: RangeInclusive<u32>, op: WatchOp, label: impl Into<String>) {
        self.specs.push(WatchSpec {
            space: WatchSpace::Bus,
            lo: *range.start(),
            hi: *range.end(),
            op,
            label: label.into(),
        });
    }

    /// Register an inclusive **VDP-internal** byte-address range in `space` (`Vram`/`Cram`/`Vsram`) to watch
    /// for `op` writes, tagged with `label` (v2 — the "who wrote this tile?" watch). Registering any VDP watch
    /// makes [`wants_vdp_writes`](Self::wants_vdp_writes) true, which arms the VDP write capture for the run.
    /// Passing [`WatchSpace::Bus`] here is a misuse (use [`add_watch`](Self::add_watch)); it is treated as a
    /// never-matching VDP watch.
    pub fn add_vdp_watch(
        &mut self,
        space: WatchSpace,
        range: RangeInclusive<u32>,
        op: WatchOp,
        label: impl Into<String>,
    ) {
        self.specs.push(WatchSpec {
            space,
            lo: *range.start(),
            hi: *range.end(),
            op,
            label: label.into(),
        });
    }

    /// Assign the next monotonic `seq` and record `hit` into the bounded drop-oldest ring (shared by the bus
    /// and VDP paths). Every matched access is counted in `seq` whether or not it fits.
    fn record(&mut self, mut hit: WatchHit) {
        hit.seq = self.seq;
        self.seq += 1;
        if self.hits.len() >= self.cap {
            // At capacity: drop the oldest to bound memory (or, for cap == 0, drop this hit outright).
            self.dropped += 1;
            if self.hits.is_empty() {
                return;
            }
            self.hits.remove(0);
        }
        self.hits.push(hit);
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
        // Match against every registered bus-space watch (immutable borrow ends before the record below).
        let matched = self.specs.iter().any(|s| {
            s.space == WatchSpace::Bus
                && s.op.matches(event.op)
                && (s.lo..=s.hi).contains(&event.addr)
        });
        if !matched {
            return;
        }
        self.record(WatchHit {
            space: WatchSpace::Bus,
            addr: event.addr,
            old: 0,
            value: event.value,
            size: event.size,
            op: event.op,
            fc: event.fc,
            via: WatchVia::Bus,
            pc: self.cur_pc,
            frame: self.cur_frame,
            seq: 0, // assigned by `record`
        });
    }

    fn on_step_boundary(&mut self, pc: u32, frame: u64) {
        self.cur_pc = pc;
        self.cur_frame = frame;
    }

    fn wants_vdp_writes(&self) -> bool {
        // Arm the (currency-sensitive) VDP capture only when at least one VDP-space watch is registered.
        self.specs.iter().any(|s| s.space != WatchSpace::Bus)
    }

    fn on_vdp_write(&mut self, w: VdpWrite) {
        // A VDP-internal write is a store: match `Write`/`Any` watches in the write's own space.
        let space = watch_space_of(w.target);
        let matched = self.specs.iter().any(|s| {
            s.space == space && s.op.matches(BusOp::Write) && (s.lo..=s.hi).contains(&w.addr)
        });
        if !matched {
            return;
        }
        self.record(WatchHit {
            space,
            addr: w.addr,
            old: w.old,
            value: w.new,
            size: if w.size >= 2 { Size::Word } else { Size::Byte },
            op: BusOp::Write,
            fc: 0, // a VDP-internal write has no bus function code; CPU-vs-DMA is in `via`
            via: match w.via {
                VdpVia::Direct => WatchVia::Direct,
                VdpVia::Dma => WatchVia::Dma,
            },
            pc: self.cur_pc,
            frame: self.cur_frame,
            seq: 0, // assigned by `record`
        });
    }
}

/// The [`WatchSpace`] a VDP-internal write lands in.
fn watch_space_of(target: VdpTarget) -> WatchSpace {
    match target {
        VdpTarget::Vram => WatchSpace::Vram,
        VdpTarget::Cram => WatchSpace::Cram,
        VdpTarget::Vsram => WatchSpace::Vsram,
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
                space: WatchSpace::Bus,
                addr: 0xFF_0000,
                old: 0,
                value: 0x0001,
                size: Size::Word,
                op: BusOp::Write,
                fc: 5,
                via: WatchVia::Bus,
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

    // --- VDP-internal watches (v2) -----------------------------------------------------------------------

    use crate::vdp::{VdpTarget, VdpVia, VdpWrite};

    fn vw(target: VdpTarget, addr: u32, old: u32, new: u32, size: u8, via: VdpVia) -> VdpWrite {
        VdpWrite {
            target,
            addr,
            old,
            new,
            size,
            via,
        }
    }

    /// A VRAM watch records a `VdpWrite` as a hit: resolved region address, old→new, byte size, `via`, and the
    /// step-boundary PC/frame — space is `Vram`, and a VDP write reports as a `Write` with `fc = 0`.
    #[test]
    fn vram_watch_records_a_vdp_write_hit() {
        let mut wp = Watchpoints::new(16);
        wp.add_vdp_watch(WatchSpace::Vram, 0x0100..=0x01FF, WatchOp::Write, "tile");
        wp.on_step_boundary(0x0400, 2);
        wp.on_vdp_write(vw(VdpTarget::Vram, 0x0100, 0xAB, 0xBE, 1, VdpVia::Direct));
        assert_eq!(
            wp.hits(),
            &[WatchHit {
                space: WatchSpace::Vram,
                addr: 0x0100,
                old: 0xAB,
                value: 0xBE,
                size: Size::Byte,
                op: BusOp::Write,
                fc: 0,
                via: WatchVia::Direct,
                pc: 0x0400,
                frame: 2,
                seq: 0,
            }]
        );
    }

    /// A DMA-driven write attributes `via = Dma` and a CRAM word maps to `Size::Word`.
    #[test]
    fn cram_watch_records_a_word_and_dma_via() {
        let mut wp = Watchpoints::new(16);
        wp.add_vdp_watch(WatchSpace::Cram, 0x0000..=0x007F, WatchOp::Any, "palette");
        wp.on_vdp_write(vw(VdpTarget::Cram, 0x0002, 0x0000, 0x0EEE, 2, VdpVia::Dma));
        let h = wp.hits()[0];
        assert_eq!(h.space, WatchSpace::Cram);
        assert_eq!(h.size, Size::Word, "a CRAM word is two bytes");
        assert_eq!(h.via, WatchVia::Dma, "DMA-driven");
        assert_eq!(h.old, 0x0000);
        assert_eq!(h.value, 0x0EEE);
    }

    /// Spaces are isolated: a VRAM watch ignores a CRAM/VSRAM write, a bus watch ignores every VDP write, and a
    /// VDP watch ignores every bus event.
    #[test]
    fn spaces_do_not_cross() {
        let mut wp = Watchpoints::new(16);
        wp.add_vdp_watch(WatchSpace::Vram, 0..=0xFFFF, WatchOp::Any, "vram");
        wp.on_vdp_write(vw(VdpTarget::Cram, 0x0000, 0, 1, 2, VdpVia::Direct));
        wp.on_vdp_write(vw(VdpTarget::Vsram, 0x0000, 0, 1, 2, VdpVia::Direct));
        wp.on_event(ev(BusOp::Write, 0x0000, 1)); // a bus write at the same numeric address
        assert_eq!(wp.hits().len(), 0, "the VRAM watch matches none of these");

        let mut bus = Watchpoints::new(16);
        bus.add_watch(0..=0xFFFF, WatchOp::Any, "bus");
        bus.on_vdp_write(vw(VdpTarget::Vram, 0x0000, 0, 1, 1, VdpVia::Direct));
        assert_eq!(
            bus.hits().len(),
            0,
            "a bus watch ignores VDP-internal writes"
        );
    }

    /// A VDP write is a store: it hits `Write`/`Any` watches, never a `Read` watch.
    #[test]
    fn vdp_write_is_a_store_for_op_filtering() {
        let mut read = Watchpoints::new(16);
        read.add_vdp_watch(WatchSpace::Vram, 0..=0xFFFF, WatchOp::Read, "r");
        read.on_vdp_write(vw(VdpTarget::Vram, 0x10, 0, 1, 1, VdpVia::Direct));
        assert_eq!(read.hits().len(), 0, "a Read watch ignores a VDP store");

        let mut write = Watchpoints::new(16);
        write.add_vdp_watch(WatchSpace::Vram, 0..=0xFFFF, WatchOp::Write, "w");
        write.on_vdp_write(vw(VdpTarget::Vram, 0x10, 0, 1, 1, VdpVia::Direct));
        assert_eq!(write.hits().len(), 1, "a Write watch catches it");
    }

    /// `wants_vdp_writes` gates the (currency-sensitive) VDP capture: false with only bus watches, true once a
    /// VDP watch is registered, false again after `clear`.
    #[test]
    fn wants_vdp_writes_tracks_vdp_watch_registration() {
        let mut wp = Watchpoints::new(16);
        assert!(!wp.wants_vdp_writes(), "no watches → capture stays off");
        wp.add_watch(0..=0xFF, WatchOp::Any, "bus");
        assert!(
            !wp.wants_vdp_writes(),
            "a bus watch does not arm VDP capture"
        );
        wp.add_vdp_watch(WatchSpace::Vram, 0..=0xFF, WatchOp::Any, "vram");
        assert!(wp.wants_vdp_writes(), "a VDP watch arms capture");
        wp.clear();
        assert!(!wp.wants_vdp_writes(), "clear disarms");
    }
}
