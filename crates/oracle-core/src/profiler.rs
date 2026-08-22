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
//! # The caller lens (§11.18 / CR-28)
//!
//! Armed with [`Profiler::with_lenses`], the accountant also keeps a second map keyed
//! `(callee entry address, caller)` — one entry per distinct **call edge**. The caller is read **once, at
//! the push**, from the frame directly beneath, so the added cost is proportional to *calls* rather than
//! to instructions and `charge` — the hot path — is untouched.
//!
//! Two properties follow from every invocation having exactly one caller, and they are what the wire's
//! `==` gates rest on: a callee's edges **partition** its row, so their `self_cycles` sum to the row's
//! `self_cycles` and their `calls` sum to the row's `calls`, undivided on both sides. They hold because the
//! edge is folded from the *same* checkpoint delta and incremented from the *same* branch as the row, never
//! from a second tally that could drift.
//!
//! An absence of caller address is never a fabricated zero: [`CallerKey`] enumerates the three ways a call
//! edge can have no calling *routine* — an interrupt context (split by cause, never collapsed), the frame
//! the sample opened on, and a frame the depth cap declined.
//!
//! The lens is opt-in for the **reply's** sake and not the accumulator's, which is the opposite of why the
//! per-frame ring is opt-in. See [`Profiler::with_lenses`].
//!
//! # The three attribution rules that are easy to get wrong
//!
//! 1. **A return only closes a frame whose stack pointer matches exactly.** `RTS` pops 4 bytes and `RTR`
//!    pops 6, so a frame entered at `entry_sp` closes on a return leaving `sp == entry_sp + 4` (or `+ 6`).
//!    The exactness is the point: the `move.l addr,-(sp)` / `rts` **dispatch idiom** — push a target, then
//!    "return" to it — leaves `sp == entry_sp`, which does not match, so it correctly does *not* close the
//!    caller. A tolerant "stack unwound past here" rule would silently close it and merge two routines.
//!
//!    The match also requires the **privilege mode** to agree. The user and supervisor stacks are
//!    independent, so a user frame and a supervisor frame can sit at the same numeric pointer while having
//!    nothing to do with each other; closing one on the other's return is a coincidence with no symptom.
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
use crate::m68000::decode::{control_flow_of, return_pop_bytes, ControlFlow};
use std::collections::{BTreeMap, VecDeque};

// The bytes each return opcode pops now come from [`return_pop_bytes`], next to the `control_flow_of`
// classifier whose classes select them. They were three private constants here until the Aether server's
// `step_over`/`step_out` became a second consumer of the same shadow-stack rule: one definition, or two that
// can drift silently while both look right. `RTE_POP` keeps a name because the interrupt path matches a
// constant frame rather than a variable opcode.
/// The bytes an `RTE` pops: the 68000's standard exception frame (SR word + 32-bit PC).
const RTE_POP: u32 = return_pop_bytes(0x4E73);
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

/// The 68000 interrupt levels the VDP drives, named because a bucket key is a cause and not a number
/// somebody has to look up.
pub const LEVEL_HINT: u8 = 4;
/// Level 6 — VBlank.
pub const LEVEL_VINT: u8 = 6;

// `OPCODE_RTR` lived here to select the 6-byte frame from the 4-byte one. `return_pop_bytes` makes that
// selection itself, from the opcode, so the constant no longer had a reader.

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

/// **Who called a routine**, as the accountant observed it at the push (§11.18 / CR-28).
///
/// One value per *genuinely different fact*, which is the whole reason this is an enum rather than an
/// `Option<u32>`: interrupt-entered, root-entered and depth-capped are three different things and a single
/// absence would make them one. The interrupt case is split **by cause** — this family keys interrupt
/// accounting by the acknowledged level everywhere else, and collapsing `hint` and `vint` here would be the
/// one place in the surface where the two are indistinguishable.
///
/// The derived `Ord` is what makes an edge list's tie-break total and reproducible boot to boot: routines
/// first (by address), then the two interrupt causes (by level), then root, then the depth cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallerKey {
    /// The **entry address** of the calling routine — the same key space a row uses, so an edge joins to
    /// its caller's row by equality.
    ///
    /// One caveat that is a property of the accountant rather than of the program: when the caller is the
    /// frame the sample *opened* on, this address is whatever PC retired first, which is mid-routine. It is
    /// real and it is where the call came from, but it is **not** an entry point.
    Routine(u32),
    /// Reached from inside an interrupt's context, keyed by the acknowledged **level** (4 = HBlank,
    /// 6 = VBlank). There is no calling routine address to give: the caller is a *bucket*.
    Interrupt(u8),
    /// No call into this routine was ever observed — it **is** the frame the sample opened on.
    Root,
    /// The calling frame was one [`Profiler::push_frame`] declined at [`MAX_DEPTH`], the same event
    /// [`Report::depth_exceeded`] counts.
    DepthCap,
}

/// One **call edge**'s worth of counters: what a routine cost when reached from one particular caller.
///
/// Three figures rather than the row's four, and the missing one is deliberate: **there is no per-edge
/// stall figure**. The client this lens was built for declined one on measured grounds (flat and
/// VBlank-owned at every state they took), and the wire fragment bars the key outright rather than leaving
/// it undeclared — so a type that could carry it would be an invitation to publish something the contract
/// refuses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeCounts {
    /// Cycles inclusive of everything called from here, on invocations reached from this caller.
    pub cycles: u64,
    /// The same span with callee time subtracted. **This** is the figure that partitions the row: every
    /// invocation has exactly one caller, so a row's edges' `self_cycles` sum to the row's exactly.
    pub self_cycles: u64,
    /// Completed invocations made from this caller, on the row field's own rule.
    pub calls: u64,
}

impl EdgeCounts {
    fn add(&mut self, other: EdgeCounts) {
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
    /// **Who called this frame**, read once at the push from the frame directly beneath it — an `O(1)`
    /// look at `stack.last()`, not a walk and not a per-instruction key. That is what keeps the per-caller
    /// cost proportional to **calls** rather than to instructions: the hot path (`charge`) never touches
    /// it, and the fold happens at the invocation boundaries that already exist.
    ///
    /// Captured on every frame whether or not the lens is armed, because it is one enum copy at a push; the
    /// thing that is allocated only when armed is the edge *map*, which is what actually costs memory.
    caller: CallerKey,
    /// The active A7 immediately after entry — what a return must restore, exactly, to close this frame.
    /// Unused for [`FrameKind::Interrupt`], which matches on the supervisor stack instead.
    entry_sp: u32,
    /// The privilege mode this frame was entered in. A return must match it as well as the pointer: the
    /// user and supervisor stacks are independent, so two unrelated frames can share a numeric `entry_sp`,
    /// and closing one on the other's return would be a mis-attribution with no visible symptom.
    supervisor: bool,
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
    /// A freshly entered frame: nothing accrued, nothing reported, not suppressed.
    ///
    /// `caller` is a **placeholder** here and is overwritten unconditionally by
    /// [`Profiler::push_frame`], which is the only place that can see what sits beneath this frame. It is
    /// not a default anything is expected to observe.
    fn opened(kind: FrameKind, entry_sp: u32, supervisor: bool) -> Self {
        Self {
            kind,
            caller: CallerKey::Root,
            entry_sp,
            supervisor,
            self_cycles: 0,
            self_stall: 0,
            child_cycles: 0,
            child_stall: 0,
            reported_self: 0,
            reported_inclusive: 0,
            reported_inclusive_stall: 0,
            suppressed: false,
        }
    }
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

/// One whole frame's totals, as recorded by the opt-in per-frame ring.
///
/// Undivided by construction — a per-frame row is already per frame — and carrying no per-routine
/// breakdown, which is what keeps the ring cheap enough to leave running across a long sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameRow {
    /// The machine's own frame index at this row's closing boundary — a position on a clock `reset`
    /// restarts, so a consumer correlates with it and never subtracts two of them to count frames.
    pub frame: u64,
    /// CPU cycles the machine retired in this one frame.
    pub cycles: u64,
    /// Of those, the ones spent held off the bus.
    pub stall_cycles: u64,
    /// What this frame's level-4 (HBlank) interrupts cost, inclusive of everything their handlers called.
    pub hint_cycles: u64,
    /// What this frame's level-6 (VBlank) interrupt cost, likewise.
    pub vint_cycles: u64,
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
    /// **Call edges**, keyed `(callee entry address, caller)`, each divided by `frame_count`. Empty unless
    /// the caller lens was armed (§11.18 / CR-28).
    ///
    /// Arming the lens **adds divided figures to the report**, so `per_frame_exact` ranges over these too:
    /// a sample that reported `true` without the lens may report `false` with it, and nothing became less
    /// exact — more figures are being reported on. Their undivided partners are
    /// [`Profiler::sample_callers`].
    pub callers: BTreeMap<(u32, CallerKey), EdgeCounts>,
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
    /// The opt-in per-frame ring, oldest first, capped at [`Profiler::per_frame_depth`]. Empty and never
    /// written when the ring was not armed — the absence is the signal, so a consumer cannot mistake "not
    /// recording" for "recorded nothing".
    per_frame: VecDeque<FrameRow>,
    /// Ring depth; `0` means the ring is not armed at all.
    per_frame_depth: usize,
    /// Whether the **caller lens** is armed (§11.18 / CR-28). The two maps below are populated only while
    /// it is, which is what makes an unarmed sample byte-identical to one taken before the lens existed.
    callers_armed: bool,
    /// Call edges accrued since the last frame boundary, keyed `(callee entry address, caller)`.
    pending_callers: BTreeMap<(u32, CallerKey), EdgeCounts>,
    /// Call edges merged from whole frames only — the same commit discipline the rows follow, for the same
    /// reason: the tail of a partial frame is never reported.
    committed_callers: BTreeMap<(u32, CallerKey), EdgeCounts>,
    /// **A push was refused at [`MAX_DEPTH`] and nothing has come off the stack since.**
    ///
    /// Set by the refusal, consumed by the next successful push (which then carries
    /// [`CallerKey::DepthCap`]), and **cleared by any pop** — because once the stack has unwound, the frame
    /// on top is one the accountant really did track and naming it a lost caller would be a lie.
    ///
    /// **Not reachable today, and that is stated rather than hidden.** A refusal implies the stack is *at*
    /// the cap, so the next push is refused too, and the only way to get below the cap is a pop — which
    /// clears the latch. So the honest reading is that this arm exists for correctness rather than for
    /// coverage: it is the shape the wire enumerates, it is cleared in the one direction that could make it
    /// wrong, and `depth_cap_is_never_attributed_to_a_caller_we_did_track` pins the negative half. If a
    /// later change ever makes a push refusable *below* the cap, the attribution is already right.
    depth_capped: bool,
}

impl Profiler {
    /// A detached, empty accountant, with the per-frame ring **off**.
    pub fn new() -> Self {
        Self::default()
    }

    /// A detached, empty accountant that also records a bounded ring of whole-frame totals.
    ///
    /// Opt-in because a per-frame record costs memory across a long sample while the aggregate does not —
    /// the aggregate accumulates in place. `depth` of `0` is the same as [`Profiler::new`].
    pub fn with_per_frame(depth: usize) -> Self {
        Self::with_lenses(depth, false)
    }

    /// A detached, empty accountant in a named **lens** pose: the per-frame ring at `per_frame_depth`
    /// (`0` = off) and the caller lens on or off.
    ///
    /// The two lenses are opt-in for **opposite** reasons, and the distinction is worth keeping straight
    /// because it decides which half a later optimisation may touch. The ring costs *memory across a long
    /// sample* while the aggregate accumulates in place. Per-caller accounting costs almost nothing to
    /// accumulate — the work is proportional to calls, not to instructions — and is opt-in because the
    /// **reply** grows by rows × edges.
    pub fn with_lenses(per_frame_depth: usize, callers: bool) -> Self {
        Self {
            per_frame_depth,
            callers_armed: callers,
            ..Self::default()
        }
    }

    /// Whether the per-frame ring is armed.
    pub fn per_frame_armed(&self) -> bool {
        self.per_frame_depth > 0
    }

    /// Whether the caller lens is armed (§11.18 / CR-28).
    pub fn callers_armed(&self) -> bool {
        self.callers_armed
    }

    /// The per-frame rows, oldest first. Empty when the ring is not armed.
    pub fn per_frame(&self) -> &VecDeque<FrameRow> {
        &self.per_frame
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

    /// The committed **call edges**, undivided — keyed `(callee entry address, caller)`. Empty when the
    /// lens was not armed, which is the signal: an armed row always acquires at least one edge, so an empty
    /// edge list under an armed lens is a defect in the accountant rather than an ordinary answer.
    ///
    /// This is the term the two normative sums are stated on. Every invocation has exactly one caller, so
    /// the edges of one callee **partition** its row: their `self_cycles` sum to the row's `self_cycles`
    /// and their `calls` to the row's `calls`, undivided on both sides.
    pub fn sample_callers(&self) -> &BTreeMap<(u32, CallerKey), EdgeCounts> {
        &self.committed_callers
    }

    /// Push a frame, unless the stack is already at [`MAX_DEPTH`].
    ///
    /// Refusing is the conservative failure: the callee's cycles then charge to its caller, which
    /// overstates one row, where growing without bound would eventually consume all memory and — long
    /// before that — bury every real row under a stack that never unwinds.
    fn push_frame(&mut self, mut frame: Frame) {
        if self.stack.len() >= MAX_DEPTH {
            self.depth_exceeded += 1;
            // The frame we just declined is a caller we will not be able to name if anything is pushed
            // beneath it before the stack unwinds. See `depth_capped`.
            self.depth_capped = true;
            return;
        }
        // **The caller, read once, here.** This is the `O(1)` look the whole lens rests on: one glance at
        // the frame beneath, at the moment of entry, rather than a key derived per retired instruction.
        frame.caller = match self.stack.last() {
            _ if self.depth_capped => {
                self.depth_capped = false;
                CallerKey::DepthCap
            }
            None => CallerKey::Root,
            Some(f) => match f.kind {
                FrameKind::Routine { addr } => CallerKey::Routine(addr),
                FrameKind::Interrupt { level, .. } => CallerKey::Interrupt(level),
            },
        };
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
    fn charge(&mut self, pc: u32, sp: u32, supervisor: bool, cycles: u64, stall: u64) {
        if self.stack.is_empty() {
            self.push_frame(Frame::opened(
                FrameKind::Routine { addr: pc },
                sp,
                supervisor,
            ));
        }
        // UNCONDITIONAL, and that is a proof rather than an optimism: the only way to reach here with an
        // empty stack is for the push above to have been refused, and `push_frame` refuses only at
        // `MAX_DEPTH` — which an empty stack is, by definition, below. The branch that once handled a
        // refused root was dead code carrying a second `pending_unattributed` write, i.e. a second source
        // of truth for a number whose whole job is to make one sum close. It is gone; the impossibility is
        // stated here instead.
        let top = self
            .stack
            .last_mut()
            .expect("a root push is never refused: an empty stack is below MAX_DEPTH");
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
        let caller = frame.caller;
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
        // The **same** delta, through the same checkpoint, into the edge — so a row's figure and the sum
        // of its edges' figures cannot be two measurements that disagree. Interrupt buckets take no edge:
        // a bucket is keyed by cause and what it preempted is not a call.
        if self.callers_armed {
            if let FrameKind::Routine { addr } = key {
                let edge = self.pending_callers.entry((addr, caller)).or_default();
                edge.self_cycles += d_self;
                edge.cycles += d_incl;
            }
        }
    }

    fn pop_frame(&mut self, completed: bool) {
        if self.stack.is_empty() {
            return;
        }
        // Flush the remainder through the same path a boundary uses, so the two can never disagree.
        self.checkpoint(self.stack.len() - 1);
        let frame = self.stack.pop().expect("just checked non-empty");
        // The stack has unwound, so whatever `push_frame` last declined is no longer plausibly the caller
        // of the next thing pushed. See `Profiler::depth_capped`.
        self.depth_capped = false;
        if !completed {
            self.abandoned += 1;
        }
        // `calls` counts COMPLETED invocations. A frame torn off the stack without its own return — one an
        // `RTE` unwound past, or one displaced by a stack the profiler could not follow — really did run,
        // so its cycles are recorded; but it never completed, so counting it would report an invocation
        // that has no end. The cycles are the honest half; the call is the half that would be a fiction.
        if completed && !frame.suppressed {
            match frame.kind {
                FrameKind::Routine { addr } => {
                    self.pending.entry(addr).or_default().calls += 1;
                    // Counted on the edge from the same branch as the row's, so the two counts can only
                    // ever move together — which is what makes "the edges' calls sum to the row's calls"
                    // a property of the code rather than an agreement between two tallies.
                    if self.callers_armed {
                        self.pending_callers
                            .entry((addr, frame.caller))
                            .or_default()
                            .calls += 1;
                    }
                }
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
    fn close_routine(&mut self, opcode: u16, sp_after: u32, supervisor: bool) {
        let pop = return_pop_bytes(opcode);
        let target = self.stack.iter().rposition(|f| {
            matches!(f.kind, FrameKind::Routine { .. })
                && f.entry_sp.wrapping_add(pop) == sp_after
                && f.supervisor == supervisor
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
        // Divided through the SAME `div`, so `per_frame_exact` covers the edge figures as well — the one
        // consequence of arming the lens that a client has to be told rather than left to discover.
        let callers = self
            .committed_callers
            .iter()
            .map(|(&key, e)| {
                (
                    key,
                    EdgeCounts {
                        cycles: div(e.cycles),
                        self_cycles: div(e.self_cycles),
                        calls: div(e.calls),
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
            callers,
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
            self.push_frame(Frame::opened(
                FrameKind::Routine { addr: r.pc },
                entry_sp,
                r.supervisor,
            ));
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
            self.push_frame(Frame::opened(
                FrameKind::Interrupt {
                    level,
                    frame_ssp: r.ssp,
                },
                r.sp,
                r.supervisor,
            ));
            self.pending_call = Some(r.sp);
        }
        // 3. The entry cost belongs to the frame it opened, so charge after pushing.
        self.charge(
            r.pc,
            r.sp,
            r.supervisor,
            u64::from(r.cycles),
            u64::from(r.stall_cycles),
        );
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
            ControlFlow::Return => self.close_routine(r.opcode, r.sp, r.supervisor),
            ControlFlow::InterruptReturn => self.close_interrupt(r.ssp),
            ControlFlow::Jump | ControlFlow::None => {}
        }
    }

    /// The sample's frame clock. The **first** boundary opens the sample and is never counted: the span
    /// before it is not a frame the accountant saw whole, so whatever accrued during it is discarded
    /// rather than reported as a runt.
    fn on_frame_boundary(&mut self, frame: u64) {
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
            // The ring row is cut from the SAME pending figures the sample is about to commit, so a
            // per-frame row and the aggregate can never describe different frames. It MUST be cut here —
            // after the checkpoints have flushed every live frame into `pending_*`, and before the drains
            // below empty them.
            if self.per_frame_depth > 0 {
                let bucket = |level: u8| {
                    self.pending_buckets
                        .get(&level)
                        .map_or(0, |c: &Counts| c.cycles)
                };
                if self.per_frame.len() == self.per_frame_depth {
                    self.per_frame.pop_front();
                }
                self.per_frame.push_back(FrameRow {
                    frame,
                    cycles: self.pending_cycles,
                    stall_cycles: self.pending_stall,
                    hint_cycles: bucket(LEVEL_HINT),
                    vint_cycles: bucket(LEVEL_VINT),
                });
            }
            for (addr, c) in std::mem::take(&mut self.pending) {
                self.committed.entry(addr).or_default().add(c);
            }
            for (level, c) in std::mem::take(&mut self.pending_buckets) {
                self.committed_buckets.entry(level).or_default().add(c);
            }
            for (key, e) in std::mem::take(&mut self.pending_callers) {
                self.committed_callers.entry(key).or_default().add(e);
            }
            self.committed_cycles += self.pending_cycles;
            self.committed_stall += self.pending_stall;
            self.committed_unattributed += self.pending_unattributed;
            self.frames += 1;
        }
        self.pending.clear();
        self.pending_buckets.clear();
        self.pending_callers.clear();
        self.pending_cycles = 0;
        self.pending_stall = 0;
        self.pending_unattributed = 0;
    }
}
