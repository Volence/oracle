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
//! The opening boundary does two things beyond starting the clock. It **forgets what every live frame had
//! accrued** — a call in flight keeps its identity and its `entry_sp`, so its return still matches, but the
//! arbitrary stretch of execution before the sample does not land in the first reported frame. And it
//! **suppresses** any interrupt frame that is already open, because that bucket's entry was never observed
//! and the contract forbids opening one retroactively. That is the normal phase rather than a corner case:
//! a VBlank fires at line 224, which is the boundary line.
//!
//! # The reconciliation identity
//!
//! Every cycle the sample retired is charged to exactly one frame, and every frame's accrual reaches its
//! row — at a boundary checkpoint while it is still running, and at its pop for the remainder. So, over the
//! committed (undivided) sample:
//!
//! ```text
//! Σ routines[*].self_cycles  +  Σ interrupts[*].self_cycles  +  unattributed_cycles  ==  sample_cycles
//! ```
//!
//! exactly, with no tolerance. `unattributed_cycles` is the one escape hatch and it has exactly one source:
//! a suppressed pre-sample interrupt, whose cycles are real but belong to no bucket. Reporting it keeps the
//! identity **closed** rather than leaving a silent shortfall for a reader to discover.
//!
//! Two consequences worth stating plainly, because they are what a reader of a row needs to know:
//!
//! - **A routine still running when the sample closes has a row**, carrying the cycles it has accrued, with
//!   `calls: 0`. A main loop that never returns is the normal case, and reporting nothing for it while its
//!   cycles inflated the total would be the worst of both.
//! - **`calls` counts completed invocations only.** A frame that never returned, and one torn off the stack
//!   by an `RTE` that unwound past it, contribute cycles but no call — see
//!   [`Report::abandoned_frames`] for the latter, which is reported so the understatement is visible.
//!
//! Inclusive `cycles` is exact **in total** for every completed invocation. Its distribution *across*
//! frames lags where a callee straddles a boundary: the parent's checkpoint cannot see time still in flight
//! beneath it, and catches up when the callee pops. `self_cycles` has no such lag, which is why the
//! identity above is stated on self.
//!
//! # A note for the acceptance protocol
//!
//! A single 68k→VDP DMA can hold the bus across **more than one** frame boundary. When that happens the
//! frames it spans each receive a share of a cost that was, from the program's point of view, one
//! indivisible act — so a per-frame figure covering such a sample is diluted rather than wrong, and a
//! consumer comparing it against a per-frame ideal should read `sample_cycles` and `frame_count` instead of
//! the divided row. This is a property of dividing by frames at all, not of the accounting.
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
/// The deepest the shadow stack may go, **derived rather than picked**: the 68000's work RAM is 64 KiB
/// (`$FF0000-$FFFFFF`) and the smallest frame a call can push is a 4-byte return address, so no program
/// whose stack lives in RAM can nest more than this many calls without overrunning it. A correct program
/// stays orders of magnitude below; anything at this depth has already lost track of its own stack.
///
/// The cap exists because the shadow stack is driven by *observation*, and an observation can be missed —
/// a return the profiler could not match leaves its frame wedged, and every subsequent call stacks on top
/// of it. Unbounded, that is a slow memory leak whose only symptom is that the report quietly empties out.
/// Bounded, it is a number in [`Report::depth_exceeded`].
pub const MAX_DEPTH: usize = 0x1_0000 / 4;

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
    /// The part of [`cycles`](Counts::cycles) spent held off the bus rather than executing — a **subset**
    /// of it, keyed identically to it: same routine, same inclusive span, same division. That is what makes
    /// `cycles - stall_cycles` a well-formed subtraction rather than the difference of two quantities that
    /// were aggregated differently.
    pub stall_cycles: u64,
    /// Actual invocations.
    pub calls: u64,
}

impl Counts {
    fn add(&mut self, other: Counts) {
        self.cycles += other.cycles;
        self.self_cycles += other.self_cycles;
        self.stall_cycles += other.stall_cycles;
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
    /// The stall part of `self_cycles`.
    self_stall: u64,
    /// Inclusive cycles of everything this frame **called**. Preemption is deliberately not included —
    /// see [`Profiler::pop_frame`].
    child_cycles: u64,
    /// The stall part of `child_cycles`, carried alongside it so the subset relation survives the fold.
    child_stall: u64,
    /// How much of this frame's totals has already been written into its row by a boundary checkpoint.
    /// The eventual pop writes only the remainder, so nothing is counted twice and nothing is lost.
    reported_self: u64,
    reported_inclusive: u64,
    reported_inclusive_stall: u64,
    /// Set on an **interrupt** frame that was already open when the sample opened. Its entry was never
    /// observed, so it records neither calls nor cycles: the contract forbids retroactively opening a
    /// bucket, "since a bucket whose entry was never observed would report a cost the sample cannot
    /// account for". Its cycles are reported as [`Report::unattributed_cycles`] instead of being silently
    /// dropped — the sample really did spend them.
    ///
    /// Routine frames open across the boundary are NOT suppressed: a subroutine that was already running
    /// and then returns inside the sample completed inside the sample, and its post-boundary cycles are
    /// honestly its own. Only the bucket rule is retroactive-entry-sensitive.
    suppressed: bool,
}

impl Frame {
    fn inclusive(&self) -> u64 {
        self.self_cycles + self.child_cycles
    }
    fn inclusive_stall(&self) -> u64 {
        self.self_stall + self.child_stall
    }
    /// What this frame has accrued since its last checkpoint: `(self, inclusive, inclusive stall)`.
    fn unreported(&self) -> (u64, u64, u64) {
        (
            self.self_cycles - self.reported_self,
            self.inclusive() - self.reported_inclusive,
            self.inclusive_stall() - self.reported_inclusive_stall,
        )
    }
    /// Mark everything accrued so far as written.
    fn mark_reported(&mut self) {
        self.reported_self = self.self_cycles;
        self.reported_inclusive = self.inclusive();
        self.reported_inclusive_stall = self.inclusive_stall();
    }
    /// Forget everything accrued so far — the sample did not see it.
    fn zero_accrual(&mut self) {
        self.self_cycles = 0;
        self.self_stall = 0;
        self.child_cycles = 0;
        self.child_stall = 0;
        self.reported_self = 0;
        self.reported_inclusive = 0;
        self.reported_inclusive_stall = 0;
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
    /// The undivided stall total for the whole sample — the subset of `sample_cycles` spent held off the
    /// bus.
    pub sample_stall_cycles: u64,
    /// `sample_stall_cycles` divided by `frame_count`.
    pub total_stall_cycles: u64,
    /// `true` if and only if **every** divided figure in this report divided without remainder. A client
    /// gating with `==` must check it: when it is `false` every divided figure is floored, one-sided low.
    pub per_frame_exact: bool,
    /// Per-routine rows, keyed by entry address, each divided by `frame_count`.
    pub routines: BTreeMap<u32, Counts>,
    /// Per-cause interrupt buckets, keyed by level (4 = HInt, 6 = VInt), each divided by `frame_count`.
    pub interrupts: BTreeMap<u8, Counts>,
    /// Cycles the sample retired that belong to **no row** — undivided. Today there is exactly one source:
    /// an interrupt that was already in flight when the sample opened, whose bucket may not be opened
    /// retroactively. The cycles are real and are counted in `sample_cycles`, so reporting them separately
    /// is what keeps the reconciliation identity closed instead of leaving an unexplained shortfall.
    ///
    /// Normally `0`. Non-zero is the ordinary phase for a ROM whose VBlank handler straddles the opening
    /// boundary, which is most of them, so it is information rather than an alarm.
    pub unattributed_cycles: u64,
    /// Calls the accountant declined to track because the shadow stack was already at its bound, over the
    /// whole sample. Their cycles are charged to the innermost frame that IS tracked, so nothing is lost
    /// from the totals — but those routines get no rows of their own, and the frame they charge to is
    /// overstated. Non-zero means the profiler lost the thread of the program's stack, which is a fact
    /// about the measurement rather than about the program, and the reader should be told. Undivided.
    pub depth_exceeded: u64,
    /// Frames torn off the shadow stack without completing, over the whole sample — a routine an `RTE`
    /// unwound past, or one displaced by a stack the profiler could not follow. Their **cycles are still
    /// reported** (they ran); their `calls` are not (they never finished). Non-zero means some row's
    /// `calls` understates its invocations, which is the kind of thing a reader is entitled to know rather
    /// than have smoothed away. Undivided: it is a count of events, not a per-frame rate.
    pub abandoned_frames: u64,
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
    committed_stall: u64,
    pending_stall: u64,
    committed_unattributed: u64,
    pending_unattributed: u64,
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
    /// Frames torn off the stack without completing. See [`Report::abandoned_frames`].
    abandoned: u64,
    /// Pushes refused because the stack was already [`MAX_DEPTH`] deep. See [`Report::depth_exceeded`].
    depth_exceeded: u64,
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

    /// How many frames are open on the shadow stack right now.
    ///
    /// The depth is observable state, not a diagnostic afterthought: a frame that opens and never closes
    /// is invisible in the rows (a row is written when its invocation ends), so the only way to see one
    /// arrive is to look at the stack. A phantom call armed from an opcode the CPU never executed shows up
    /// here and nowhere else.
    pub fn open_frames(&self) -> usize {
        self.stack.len()
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

    /// Push a frame, unless the stack is already at [`MAX_DEPTH`].
    ///
    /// Refusing is the conservative failure: the callee's cycles then charge to its caller, which
    /// overstates one row, where growing without bound would eventually consume all memory and — long
    /// before that — bury every real row under a stack that never unwinds.
    fn push_frame(&mut self, frame: Frame) {
        if self.stack.len() >= MAX_DEPTH {
            self.depth_exceeded += 1;
            return;
        }
        self.stack.push(frame);
    }

    /// Charge `cycles` to the innermost open frame, and to the sample total.
    ///
    /// With no frame open — the sample armed in the middle of straight-line code that has not been called
    /// from anywhere the accountant saw — a **root frame keyed by the retiring PC** is opened first,
    /// without which those cycles would belong to no row and the totals would not reconcile.
    ///
    /// **Be honest about what that key is.** It is the address of whatever instruction happened to retire
    /// first, which is somewhere in the MIDDLE of a routine, not its entry. So the row is real cycles under
    /// an address that is not an entry point, and a symbol lookup will resolve it to "some label + a
    /// displacement" rather than to the routine a reader has in mind. It falls under the same rule as any
    /// unreturned frame — `calls: 0`, because nothing was observed to call it and nothing completes it —
    /// which is the honest signal that this row was inferred rather than seen. Flagging it as such on the
    /// wire is a question for the surface that publishes these rows, not for the accumulator.
    fn charge(&mut self, pc: u32, sp: u32, cycles: u64, stall: u64) {
        if self.stack.is_empty() {
            self.push_frame(Frame {
                kind: FrameKind::Routine { addr: pc },
                entry_sp: sp,
                self_cycles: 0,
                self_stall: 0,
                child_cycles: 0,
                child_stall: 0,
                reported_self: 0,
                reported_inclusive: 0,
                reported_inclusive_stall: 0,
                suppressed: false,
            });
        }
        let Some(top) = self.stack.last_mut() else {
            // The root push was refused (the stack is capped). The cycles still happened, so they stay in
            // the sample total and are reported as belonging to no row.
            self.pending_cycles += cycles;
            self.pending_stall += stall;
            self.pending_unattributed += cycles;
            return;
        };
        top.self_cycles += cycles;
        top.self_stall += stall;
        self.pending_cycles += cycles;
        self.pending_stall += stall;
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
    /// Move whatever `self.stack[idx]` has accrued since its last checkpoint into its row.
    ///
    /// This is the **checkpoint**, and it runs at every frame boundary as well as at the pop. Attributing
    /// a frame's cycles to the frames they were retired in — rather than banking them all against whichever
    /// frame the invocation happened to end in — is what makes two things true at once: a routine still
    /// running at the close of the sample has an honest row (its cycles are already in it, its `calls`
    /// still zero because it has not finished), and the totals add up exactly, because nothing waits on a
    /// completion that may never come.
    fn checkpoint(&mut self, idx: usize) {
        let frame = &mut self.stack[idx];
        let (d_self, d_incl, d_incl_stall) = frame.unreported();
        frame.mark_reported();
        let suppressed = frame.suppressed;
        let key = frame.kind;
        if suppressed {
            self.pending_unattributed += d_self;
            return;
        }
        let row = match key {
            FrameKind::Routine { addr } => self.pending.entry(addr).or_default(),
            FrameKind::Interrupt { level, .. } => self.pending_buckets.entry(level).or_default(),
        };
        row.self_cycles += d_self;
        row.cycles += d_incl;
        row.stall_cycles += d_incl_stall;
    }

    fn pop_frame(&mut self, completed: bool) {
        if self.stack.is_empty() {
            return;
        }
        // Flush the remainder through the same path a boundary uses, so the two can never disagree.
        self.checkpoint(self.stack.len() - 1);
        let frame = self.stack.pop().expect("just checked non-empty");
        if !completed {
            self.abandoned += 1;
        }
        // `calls` counts COMPLETED invocations. A frame torn off the stack without its own return — one an
        // `RTE` unwound past, or one displaced by a stack the profiler could not follow — really did run,
        // so its cycles are recorded; but it never completed, so counting it would report an invocation
        // that has no end. The cycles are the honest half; the call is the half that would be a fiction.
        if completed && !frame.suppressed {
            match frame.kind {
                FrameKind::Routine { addr } => self.pending.entry(addr).or_default().calls += 1,
                FrameKind::Interrupt { level, .. } => {
                    self.pending_buckets.entry(level).or_default().calls += 1
                }
            }
        }
        if matches!(frame.kind, FrameKind::Routine { .. }) {
            if let Some(parent) = self.stack.last_mut() {
                // The parent gets the child's FULL lifetime inclusive, not just the unreported remainder:
                // the parent's own checkpoints could not see time that was still in flight beneath it, and
                // this is where they catch up.
                parent.child_cycles += frame.inclusive();
                parent.child_stall += frame.inclusive_stall();
            }
        }
        // Deliberately no parent.child_cycles for an Interrupt frame. See the doc comment.
    }

    /// Close the routine frame this return unwound, if the stack still knows which one that is.
    ///
    /// The match is **exact** — a frame entered at `entry_sp` closes on a return leaving `entry_sp + 4`
    /// (or `+ 6` for `RTR`) — and that exactness is load-bearing: the `move.l #target,-(sp)` / `rts`
    /// dispatch idiom leaves the stack exactly where it started, so it matches nothing and correctly does
    /// not close its caller.
    ///
    /// The search runs **innermost-first through the whole stack**, not just the top. If a frame in the
    /// middle got wedged — a return the accountant could not match, so its frame was never popped — then
    /// every later return finds a stranger on top and, matching only the top, would give up forever: the
    /// stack grows without bound and every subsequent row is buried under a frame that never closes. One
    /// wedged frame silently emptying the rest of the report is a much worse failure than the wedge.
    ///
    /// Scanning deeper cannot loosen the rule, because what it looks for is the *same* exact match. It can
    /// only find a frame the top-only search would have missed, and everything above that frame is then
    /// unwound as ABANDONED — cycles kept, calls not, and counted in [`Report::abandoned_frames`] so the
    /// recovery is visible rather than silent.
    fn close_routine(&mut self, opcode: u16, sp_after: u32) {
        let pop = if opcode == OPCODE_RTR {
            RTR_POP
        } else {
            RTS_POP
        };
        let target = self.stack.iter().rposition(|f| {
            matches!(f.kind, FrameKind::Routine { .. }) && f.entry_sp.wrapping_add(pop) == sp_after
        });
        let Some(idx) = target else {
            return; // A return that matches nothing closes nothing.
        };
        // Everything above the match was left behind by a return we never saw.
        while self.stack.len() > idx + 1 {
            self.pop_frame(false);
        }
        self.pop_frame(true);
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
        // Unwind to the bucket. Two of the frames coming off completed and the rest did not: the bucket
        // itself (the interrupt ran to its `RTE`) and the **handler routine directly above it**, whose
        // return IS that `RTE` — a handler does not `rts`. Anything stacked above the handler is a
        // subroutine it entered and never returned from, which the `RTE` has just discarded.
        while self.stack.len() > idx {
            let completed = self.stack.len() <= idx + 2;
            self.pop_frame(completed);
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
                abandoned_frames: self.abandoned,
                depth_exceeded: self.depth_exceeded,
                unattributed_cycles: self.committed_unattributed,
                ..Report::default()
            };
        }
        let mut exact = true;
        let mut div = |v: u64| {
            exact &= v.is_multiple_of(n);
            v / n
        };
        let total_cycles = div(self.committed_cycles);
        let total_stall_cycles = div(self.committed_stall);
        let routines = self
            .committed
            .iter()
            .map(|(&addr, c)| {
                (
                    addr,
                    Counts {
                        cycles: div(c.cycles),
                        self_cycles: div(c.self_cycles),
                        stall_cycles: div(c.stall_cycles),
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
                        stall_cycles: div(c.stall_cycles),
                        calls: div(c.calls),
                    },
                )
            })
            .collect();
        Report {
            abandoned_frames: self.abandoned,
            depth_exceeded: self.depth_exceeded,
            unattributed_cycles: self.committed_unattributed,
            frame_count: n,
            sample_cycles: self.committed_cycles,
            total_cycles,
            sample_stall_cycles: self.committed_stall,
            total_stall_cycles,
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
            self.push_frame(Frame {
                kind: FrameKind::Routine { addr: r.pc },
                entry_sp,
                self_cycles: 0,
                self_stall: 0,
                child_cycles: 0,
                child_stall: 0,
                reported_self: 0,
                reported_inclusive: 0,
                reported_inclusive_stall: 0,
                suppressed: false,
            });
        }
        // 2. An acknowledge seen during this step means this step IS the interrupt entry. `calls` counts
        //    the times an interrupt was TAKEN, so a raised-but-masked request appears nowhere: it never
        //    reaches an acknowledge.
        //
        //    Opening the bucket also **arms a call**, so the next retire's `pc` — the handler's entry
        //    address — opens a routine frame nested inside it. A handler is code, and code gets a row: the
        //    contract requires the interrupt split to be *additive* to per-routine rows rather than a
        //    replacement, and the handler's own row is the load-bearing one for a consumer measuring, say,
        //    an HBlank trampoline. The bucket's inclusive total is unchanged, because the routine's
        //    inclusive folds into the bucket's child time when it closes.
        //
        //    Note where that arming comes FROM: the acknowledge, never the retired opcode. The phantom
        //    guard below is therefore untouched by it — this row is opened by a signal the CPU really drove.
        if let Some(level) = self.pending_iack.take() {
            self.push_frame(Frame {
                kind: FrameKind::Interrupt {
                    level,
                    frame_ssp: r.ssp,
                },
                entry_sp: r.sp,
                self_cycles: 0,
                self_stall: 0,
                child_cycles: 0,
                child_stall: 0,
                reported_self: 0,
                reported_inclusive: 0,
                reported_inclusive_stall: 0,
                suppressed: false,
            });
            self.pending_call = Some(r.sp);
        }
        // 3. The entry cost belongs to the frame it opened, so charge after pushing.
        self.charge(r.pc, r.sp, u64::from(r.cycles), u64::from(r.stall_cycles));
        // 4. Classify — but ONLY if the instruction at `pc` actually ran. Exception entries are dispatched
        //    before decode, idle slices retire a stale `pc` over and over, and an aborted instruction
        //    retires its own opcode having done nothing; on every one of those paths the opcode names an
        //    instruction the CPU never executed, and classifying it would arm a call that was never made
        //    and will never return. `executed` is the one flag that covers all of them.
        if !r.executed {
            return;
        }
        match control_flow_of(r.opcode) {
            ControlFlow::Call => self.pending_call = Some(r.sp),
            ControlFlow::Return => self.close_routine(r.opcode, r.sp),
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
            // **The sample starts here, and it starts clean.** Frames open across this boundary keep their
            // identity and their `entry_sp` — a call in flight is still one call, and its return must still
            // match — but everything they accrued BEFORE the sample is forgotten. Without this the first
            // reported frame is inflated by however long the machine had been running, which is unbounded.
            //
            // An interrupt in flight here is additionally SUPPRESSED: its acknowledge happened before the
            // sample, so opening a bucket for it now would be exactly the retroactive entry the contract
            // forbids. This is the ordinary phase, not a corner: a VBlank fires at line 224, which is the
            // boundary line.
            for frame in &mut self.stack {
                frame.zero_accrual();
                if matches!(frame.kind, FrameKind::Interrupt { .. }) {
                    frame.suppressed = true;
                }
            }
        } else {
            // Every live frame reports what it accrued during the frame just ended, so a routine that has
            // not returned still has an honest row.
            for idx in 0..self.stack.len() {
                self.checkpoint(idx);
            }
            for (addr, c) in std::mem::take(&mut self.pending) {
                self.committed.entry(addr).or_default().add(c);
            }
            for (level, c) in std::mem::take(&mut self.pending_buckets) {
                self.committed_buckets.entry(level).or_default().add(c);
            }
            self.committed_cycles += self.pending_cycles;
            self.committed_stall += self.pending_stall;
            self.committed_unattributed += self.pending_unattributed;
            self.frames += 1;
        }
        self.pending.clear();
        self.pending_buckets.clear();
        self.pending_cycles = 0;
        self.pending_stall = 0;
        self.pending_unattributed = 0;
    }
}
