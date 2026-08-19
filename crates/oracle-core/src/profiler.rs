//! **The CPU profiler: an exact per-invocation cycle accountant.**
//!
//! A [`BusEventSink`] that turns the retire stream ([`BusEventSink::on_step_retire`]) into per-routine
//! cycle rows and per-cause interrupt buckets. It is a pure accumulator: it reads `pc`, `opcode`, `sp`,
//! `ssp` and `cycles` off each retired step, and writes nothing back to the machine.
//!
//! # Why a shadow stack rather than sampling
//!
//! The consumers this exists for compare profiler output with `==` against constants derived from engine
//! source. A sampled figure cannot be equality-gated at all, so sampling would not merely be less accurate
//! — it would make the comparison impossible. Every cycle the CPU retires is charged to exactly one frame
//! on a shadow stack, so the totals are exact by construction rather than statistical.
//!
//! # What a row means
//!
//! A row is keyed by a **routine entry address** — the PC of the first instruction executed after a
//! `JSR`/`BSR`. `cycles` is **inclusive** of everything the routine called; `cyclesSelf` is the same span
//! with callee time subtracted; `calls` is the number of actual invocations.
//!
//! # The three attribution rules that are easy to get wrong
//!
//! 1. **A return only closes a frame whose stack pointer matches exactly.** `RTS` pops 4 bytes and `RTR`
//!    pops 6, so a frame entered at `entry_sp` closes on a return leaving `sp == entry_sp + 4` (or `+ 6`).
//!    The exactness is the point: the `move.l addr,-(sp)` / `rts` **dispatch idiom** — push a target, then
//!    "return" to it — leaves `sp == entry_sp`, which does not match, so it correctly does *not* close the
//!    caller. A tolerant "stack unwound past here" rule would silently close it and merge two routines.
//!
//! 2. **An interrupt is keyed by its cause, never by its handler's address.** The bucket opens on the
//!    fc = 7 interrupt-acknowledge that [`BusEventSink::on_event`] already carries — the level is encoded
//!    in the acknowledge address — and never on a guess about where a vector points. An address heuristic
//!    mis-buckets for any ROM whose vector points somewhere it did not anticipate, and a silently wrong
//!    number is worse than a missing one.
//!
//! 3. **An interrupt bucket closes at the `RTE` that unwinds *its own* exception frame**, matched on the
//!    **supervisor** stack pointer. Exception frames always live on the SSP, so matching on `ssp` (rather
//!    than the mode-selected active A7) is correct whether the interrupt preempted supervisor or user code
//!    — an `RTE` returning to user mode leaves the active A7 pointing at the *user* stack, and a match on
//!    `sp` would silently fail there. This one rule yields all three of the awkward cases for free: a
//!    `TRAP` taken inside a handler pushes its frame lower, so its `RTE` restores the SSP to a value that
//!    does not match the bucket and closes the trap instead; a nested HInt inside a VInt closes only
//!    itself; and an `RTE` that matches no open bucket closes nothing.
//!
//! # What the sample covers
//!
//! Cycles are accumulated into a **pending** frame and merged into the committed sample at each frame
//! boundary. The sample therefore **opens at the first boundary after arming and closes at the most recent
//! one**, so every frame it covers is whole and a partial frame is inexpressible. `frames` counts
//! boundaries observed *after* the one that opened the sample: *n* boundaries delimit **n − 1** frames, and
//! an instrument that has seen one boundary and no more reports `0`.
//!
//! # Cost when detached
//!
//! The profiler is caller-owned and attached per-run like every other instrument here; `System` never
//! stores it, so it is in no frozen currency and cannot move a state hash. With nothing attached the run
//! loop's retire hook is the empty default.

use crate::bus::{BusEvent, BusEventSink, BusOp, StepRetire};
use crate::m68000::decode::{control_flow_of, ControlFlow};
use std::collections::BTreeMap;

/// The bytes an `RTS` pops: the 32-bit return address a `JSR`/`BSR` pushed.
const RTS_POP: u32 = 4;
/// The bytes an `RTR` pops: the saved CCR word plus the 32-bit return address.
const RTR_POP: u32 = 6;
/// The bytes an `RTE` pops: the 68000's standard exception frame (SR word + 32-bit PC).
const RTE_POP: u32 = 6;
/// The `RTR` opcode — the one return whose frame is 6 bytes rather than 4.
const OPCODE_RTR: u16 = 0x4E77;

/// One accumulator's worth of counters. Every field is a raw, undivided sample total; the division into
/// per-frame figures happens once, in [`Profiler::report`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Cycles inclusive of everything called from here.
    pub cycles: u64,
    /// Cycles retired directly by this routine, callees excluded.
    pub self_cycles: u64,
    /// Actual invocations.
    pub calls: u64,
}

impl Counts {
    fn add(&mut self, other: Counts) {
        self.cycles += other.cycles;
        self.self_cycles += other.self_cycles;
        self.calls += other.calls;
    }
}

/// What a live shadow-stack frame is: a called routine, or a taken interrupt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameKind {
    /// A routine entered by `JSR`/`BSR`, keyed by its entry address, closed by a stack-matched return.
    Routine { addr: u32 },
    /// An interrupt taken at the acknowledge cycle, keyed by its level, closed by the `RTE` that unwinds
    /// the exception frame at `frame_ssp`.
    Interrupt { level: u8, frame_ssp: u32 },
}

/// One live frame on the shadow stack.
#[derive(Clone, Copy, Debug)]
struct Frame {
    kind: FrameKind,
    /// The active A7 immediately after entry — what a return must restore, exactly, to close this frame.
    /// Unused for [`FrameKind::Interrupt`], which matches on the supervisor stack instead.
    entry_sp: u32,
    /// Cycles retired directly in this frame.
    self_cycles: u64,
    /// Inclusive cycles of everything this frame **called**. Preemption is deliberately not included —
    /// see [`Profiler::pop_frame`].
    child_cycles: u64,
}

impl Frame {
    fn inclusive(&self) -> u64 {
        self.self_cycles + self.child_cycles
    }
}

/// The accumulated sample, divided into per-frame figures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Whole frames in the sample — the divisor. Counted, never derived from frame indices (which are
    /// positions on a clock `reset` restarts).
    pub frame_count: u64,
    /// The undivided cycle total for the whole sample.
    pub sample_cycles: u64,
    /// `sample_cycles` divided by `frame_count`.
    pub total_cycles: u64,
    /// `true` if and only if **every** divided figure in this report divided without remainder. A client
    /// gating with `==` must check it: when it is `false` every divided figure is floored, one-sided low.
    pub per_frame_exact: bool,
    /// Per-routine rows, keyed by entry address, each divided by `frame_count`.
    pub routines: BTreeMap<u32, Counts>,
    /// Per-cause interrupt buckets, keyed by level (4 = HInt, 6 = VInt), each divided by `frame_count`.
    pub interrupts: BTreeMap<u8, Counts>,
}

/// The exact per-invocation CPU-cycle accountant. See the module documentation.
#[derive(Clone, Debug, Default)]
pub struct Profiler {
    /// The live shadow stack, innermost last. Deliberately **not** reset at a frame boundary: a call that
    /// straddles a boundary is one call, not two.
    stack: Vec<Frame>,
    /// Rows merged from whole frames only.
    committed: BTreeMap<u32, Counts>,
    /// Rows accrued since the last frame boundary. Merged into `committed` at the next one, so the tail of
    /// a partial frame is never reported.
    pending: BTreeMap<u32, Counts>,
    committed_buckets: BTreeMap<u8, Counts>,
    pending_buckets: BTreeMap<u8, Counts>,
    committed_cycles: u64,
    pending_cycles: u64,
    /// Whole frames counted: boundaries seen after the one that opened the sample.
    frames: u64,
    /// Whether the opening boundary has been seen. Until it has, everything accrued is discarded.
    opened: bool,
    /// Set by a `JSR`/`BSR` retirement; the **next** retirement's `pc` is the callee's entry address. The
    /// payload is the caller's post-push `sp`, which becomes the callee frame's `entry_sp`.
    pending_call: Option<u32>,
    /// Set by an fc = 7 interrupt acknowledge seen during the step currently executing; consumed by that
    /// step's retirement.
    pending_iack: Option<u8>,
}

impl Profiler {
    /// A detached, empty accountant.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whole frames counted so far — the divisor a report will use.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Distinct routine rows in the committed sample.
    pub fn routine_count(&self) -> usize {
        self.committed.len()
    }

    /// The committed per-routine totals, **undivided** — the sample as accumulated, before
    /// [`Profiler::report`] divides it by the frame count.
    ///
    /// Exposed because the divided figures cannot express everything the raw ones can: an invocation that
    /// is open across every boundary it meets is always one pop "in flight", so its per-frame count floors
    /// to zero even though the sample plainly contains invocations. That flooring is correct — a count is
    /// never rounded up — but a caller checking *how many* rather than *how many per frame* needs this.
    pub fn sample_routines(&self) -> &BTreeMap<u32, Counts> {
        &self.committed
    }

    /// The committed per-cause interrupt totals, undivided. See [`Profiler::sample_routines`].
    pub fn sample_interrupts(&self) -> &BTreeMap<u8, Counts> {
        &self.committed_buckets
    }

    /// Charge `cycles` to the innermost open frame, and to the sample total.
    ///
    /// With no frame open — the sample armed in the middle of straight-line code that has not been called
    /// from anywhere the accountant saw — a **root frame keyed by the retiring PC** is opened first. That
    /// is honest rather than synthetic: the address is where execution actually was, and without it the
    /// cycles would belong to no row and the totals would not reconcile.
    fn charge(&mut self, pc: u32, sp: u32, cycles: u64) {
        if self.stack.is_empty() {
            self.stack.push(Frame {
                kind: FrameKind::Routine { addr: pc },
                entry_sp: sp,
                self_cycles: 0,
                child_cycles: 0,
            });
        }
        let top = self.stack.last_mut().expect("just ensured non-empty");
        top.self_cycles += cycles;
        self.pending_cycles += cycles;
    }

    /// Retire the innermost frame: fold its totals into its row, and its inclusive total into its parent's
    /// child time.
    ///
    /// **An interrupt's cost does not fold into the frame it preempted.** A preemption is not an
    /// invocation: charging a VBlank handler's cycles to whatever routine happened to be running would make
    /// that routine's inclusive figure vary with interrupt load, which is exactly the silently-wrong number
    /// a consumer gating with `==` cannot detect. The interrupt's cost is reported in its own bucket and in
    /// its handler's own row instead. The consequence, stated so nobody has to rediscover it: for a routine
    /// row, `cycles == cyclesSelf + (inclusive cycles of its callees)`, and preemption is in neither term.
    fn pop_frame(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let inclusive = frame.inclusive();
        let row = match frame.kind {
            FrameKind::Routine { addr } => self.pending.entry(addr).or_default(),
            FrameKind::Interrupt { level, .. } => self.pending_buckets.entry(level).or_default(),
        };
        row.cycles += inclusive;
        row.self_cycles += frame.self_cycles;
        row.calls += 1;
        if matches!(frame.kind, FrameKind::Routine { .. }) {
            if let Some(parent) = self.stack.last_mut() {
                parent.child_cycles += inclusive;
            }
        }
        // Deliberately no parent.child_cycles for an Interrupt frame. See the doc comment.
    }

    /// Close the innermost **interrupt** frame whose exception frame sits where this `RTE` just unwound
    /// from, if any. Frames inside it (a subroutine the handler called and never returned from) are
    /// discarded with it, which is the only sane reading of an `RTE` crossing them.
    fn close_interrupt(&mut self, ssp_after: u32) {
        let target = self.stack.iter().rposition(|f| {
            matches!(f.kind, FrameKind::Interrupt { frame_ssp, .. }
                if frame_ssp.wrapping_add(RTE_POP) == ssp_after)
        });
        let Some(idx) = target else {
            return; // An RTE that matches no open bucket closes nothing.
        };
        while self.stack.len() > idx {
            self.pop_frame();
        }
    }

    /// Divide the committed sample into per-frame figures.
    ///
    /// An empty sample is not an error: `frame_count: 0`, empty rows, every figure `0`, and
    /// `per_frame_exact: true` — there is nothing to divide and nothing was lost. A count is never floored
    /// **up**: a routine invoked fewer times than the sample has frames reports `calls: 0` rather than a
    /// fabricated `1`.
    pub fn report(&self) -> Report {
        let n = self.frame_count();
        if n == 0 {
            return Report {
                per_frame_exact: true,
                ..Report::default()
            };
        }
        let mut exact = true;
        let mut div = |v: u64| {
            exact &= v.is_multiple_of(n);
            v / n
        };
        let total_cycles = div(self.committed_cycles);
        let routines = self
            .committed
            .iter()
            .map(|(&addr, c)| {
                (
                    addr,
                    Counts {
                        cycles: div(c.cycles),
                        self_cycles: div(c.self_cycles),
                        calls: div(c.calls),
                    },
                )
            })
            .collect();
        let interrupts = self
            .committed_buckets
            .iter()
            .map(|(&level, c)| {
                (
                    level,
                    Counts {
                        cycles: div(c.cycles),
                        self_cycles: div(c.self_cycles),
                        calls: div(c.calls),
                    },
                )
            })
            .collect();
        Report {
            frame_count: n,
            sample_cycles: self.committed_cycles,
            total_cycles,
            per_frame_exact: exact,
            routines,
            interrupts,
        }
    }

    /// The divisor: whole frames in the sample.
    fn frame_count(&self) -> u64 {
        self.frames
    }
}

impl BusEventSink for Profiler {
    /// The only event that matters here: the **fc = 7 interrupt acknowledge**. The 68000 drives it as part
    /// of the interrupt exception entry, and the level is encoded in the acknowledge address
    /// (`0xFFFFFFF1 | level << 1`), so the cause is read from the bus rather than guessed from a handler
    /// address. Latched here and consumed by the same step's retirement.
    fn on_event(&mut self, event: BusEvent) {
        if event.fc == 7 && event.op == BusOp::Read {
            self.pending_iack = Some(((event.addr >> 1) & 0x07) as u8);
        }
    }

    fn on_step_retire(&mut self, r: StepRetire) {
        // 1. A call armed by the PREVIOUS retirement lands here: this step's PC is the callee's entry.
        if let Some(entry_sp) = self.pending_call.take() {
            self.stack.push(Frame {
                kind: FrameKind::Routine { addr: r.pc },
                entry_sp,
                self_cycles: 0,
                child_cycles: 0,
            });
        }
        // 2. An acknowledge seen during this step means this step IS the interrupt entry. `calls` counts
        //    the times an interrupt was TAKEN, so a raised-but-masked request appears nowhere: it never
        //    reaches an acknowledge.
        let was_interrupt_entry = if let Some(level) = self.pending_iack.take() {
            self.stack.push(Frame {
                kind: FrameKind::Interrupt {
                    level,
                    frame_ssp: r.ssp,
                },
                entry_sp: r.sp,
                self_cycles: 0,
                child_cycles: 0,
            });
            true
        } else {
            false
        };
        // 3. The entry cost belongs to the frame it opened, so charge after pushing.
        self.charge(r.pc, r.sp, u64::from(r.cycles));
        // 4. Classify — but NOT on an exception entry. An entry is dispatched before decode, so `opcode`
        //    is the instruction that did NOT run; treating it as executed would arm a phantom call from a
        //    `JSR` the CPU never reached. The same reasoning is why interrupts are keyed off the
        //    acknowledge and never off this field.
        if was_interrupt_entry {
            return;
        }
        match control_flow_of(r.opcode) {
            ControlFlow::Call => self.pending_call = Some(r.sp),
            ControlFlow::Return => {
                let pop = if r.opcode == OPCODE_RTR {
                    RTR_POP
                } else {
                    RTS_POP
                };
                // Exact match only — the dispatch idiom depends on it (rule 1 in the module docs).
                let matches = self.stack.last().is_some_and(|f| {
                    matches!(f.kind, FrameKind::Routine { .. })
                        && f.entry_sp.wrapping_add(pop) == r.sp
                });
                if matches {
                    self.pop_frame();
                }
            }
            ControlFlow::InterruptReturn => self.close_interrupt(r.ssp),
            ControlFlow::Jump | ControlFlow::None => {}
        }
    }

    /// The sample's frame clock. The **first** boundary opens the sample and is never counted: the span
    /// before it is not a frame the accountant saw whole, so whatever accrued during it is discarded
    /// rather than reported as a runt.
    fn on_frame_boundary(&mut self, _frame: u64) {
        if !self.opened {
            self.opened = true;
        } else {
            for (addr, c) in std::mem::take(&mut self.pending) {
                self.committed.entry(addr).or_default().add(c);
            }
            for (level, c) in std::mem::take(&mut self.pending_buckets) {
                self.committed_buckets.entry(level).or_default().add(c);
            }
            self.committed_cycles += self.pending_cycles;
            self.frames += 1;
        }
        self.pending.clear();
        self.pending_buckets.clear();
        self.pending_cycles = 0;
    }
}
