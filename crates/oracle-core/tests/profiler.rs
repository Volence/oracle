//! The CPU profiler's accumulator: per-routine cycle rows and per-cause interrupt buckets.
//!
//! Every expectation here is derived from a constant the fixture builder itself used
//! (`testrom::build_profiler`), never from a number read off a passing run. The fixtures share a skeleton
//! whose outer loop is gated on the V counter so the body executes **exactly once per frame** — that is
//! what turns "how many times was this called" into a constant a test can compare with `==`, which is the
//! same bar the consumers of this surface hold it to.

use oracle_core::bus::{BusEvent, BusEventSink, BusOp, Size, StepRetire};
use oracle_core::profiler::{Counts, Profiler, Report};
use oracle_core::system::System;
use oracle_core::testrom::{
    self, ProfilerShape, PROF_DISPATCH, PROF_LEAF, PROF_MID, PROF_MID_CALLS_LEAF, PROF_REC,
    PROF_TARGET,
};

/// Interrupt levels, as the 68000 numbers them.
const HINT: u8 = 4;
const VINT: u8 = 6;

/// Boot a fixture and run it for `frames` frames with the profiler attached.
///
/// Note the divisor this implies: `frames` boundaries are observed, the first opens the sample and is
/// never counted, so the report divides by `frames - 1`. Tests that want a divisor of 1 ask for 2 frames.
fn profile(shape: ProfilerShape, frames: u64) -> Report {
    profiler_of(shape, frames).report()
}

/// As [`profile`], but hands back the accountant itself so a test can read the **undivided** sample.
fn profiler_of(shape: ProfilerShape, frames: u64) -> Profiler {
    let mut sys = System::new(0x1234_5678);
    sys.load_rom(testrom::build_profiler(shape));
    sys.reset();
    let mut prof = Profiler::new();
    sys.run_frames_with_sink(frames, &mut prof);
    prof
}

/// The row for `addr`, or a failure naming what rows there were — a missing row is the most common way
/// for one of these tests to fail, and "None" on its own says nothing about why.
fn row(r: &Report, addr: u32) -> Counts {
    *r.routines.get(&addr).unwrap_or_else(|| {
        panic!(
            "no row for {addr:#06X}; rows present: {:#06X?}",
            r.routines.keys().collect::<Vec<_>>()
        )
    })
}

// --- The frame clock ------------------------------------------------------------------------------

/// The divisor rule, stated as a test because every other expectation in this file rests on it: *n*
/// boundaries delimit **n − 1** whole frames. The boundary that opens the sample is not counted, because
/// the span before it is not a frame the accountant saw whole — which is precisely what makes a partial
/// first frame inexpressible instead of a runt that callers have to compensate for by hand.
#[test]
fn n_boundaries_delimit_n_minus_one_whole_frames() {
    for frames in 1..=4u64 {
        let r = profile(ProfilerShape::CallsLeaf { k: 1 }, frames);
        assert_eq!(
            r.frame_count,
            frames - 1,
            "{frames} boundaries must count {} frames",
            frames - 1
        );
    }
}

/// An empty sample is answered, not refused: nothing to divide, nothing lost, and no fabricated `1`
/// substituted for the zero denominator.
#[test]
fn an_empty_sample_is_all_zeroes_and_exact() {
    let r = profile(ProfilerShape::CallsLeaf { k: 1 }, 1);
    assert_eq!(r.frame_count, 0);
    assert_eq!(r.sample_cycles, 0);
    assert_eq!(r.total_cycles, 0);
    assert!(r.routines.is_empty(), "no rows from a sample of no frames");
    assert!(
        r.per_frame_exact,
        "an empty sample divided nothing, so nothing was truncated"
    );
}

// --- Routine rows ---------------------------------------------------------------------------------

/// `calls == k`, where *k* is the constant handed to the fixture builder. The fixture calls the leaf `k`
/// times per frame and the report is per-frame, so the number the profiler reports and the number the
/// builder was told are the same number.
#[test]
fn a_routine_called_k_times_reports_calls_equal_to_k() {
    for k in [1u16, 3, 7] {
        let r = profile(ProfilerShape::CallsLeaf { k }, 3);
        assert_eq!(r.frame_count, 2);
        assert_eq!(
            row(&r, PROF_LEAF).calls,
            u64::from(k),
            "the leaf is called {k} times per frame by construction"
        );
    }
}

/// Self plus children equals inclusive, on a tree whose shape is known: MID calls LEAF twice and does a
/// little work of its own, so neither term is zero and the identity is not satisfied by accident.
#[test]
fn self_plus_children_equals_inclusive_for_a_two_level_tree() {
    let r = profile(ProfilerShape::TwoLevel, 3);
    let mid = row(&r, PROF_MID);
    let leaf = row(&r, PROF_LEAF);

    assert_eq!(mid.calls, 1, "MID is called once per frame");
    assert_eq!(
        leaf.calls, PROF_MID_CALLS_LEAF,
        "and MID calls LEAF twice per invocation"
    );
    assert_eq!(
        mid.cycles,
        mid.self_cycles + leaf.cycles,
        "MID's inclusive time is its own work plus everything LEAF cost"
    );
    assert!(
        mid.self_cycles > 0 && leaf.cycles > 0,
        "both terms must be non-zero or the identity above proves nothing \
         (self {}, children {})",
        mid.self_cycles,
        leaf.cycles
    );
    assert_eq!(
        leaf.cycles, leaf.self_cycles,
        "a leaf has no children, so its two figures coincide"
    );
}

/// Recursion is **one row**, with every invocation counted and no total larger than the run that
/// produced it. A depth-first accountant that summed nested spans would report an inclusive figure
/// several times the sample — the classic double-count this shape exists to catch.
#[test]
fn recursion_is_one_row_with_every_invocation_counted() {
    let depth = 3u16;
    let r = profile(ProfilerShape::Recursive { depth }, 3);
    let rec = row(&r, PROF_REC);
    assert_eq!(
        rec.calls,
        u64::from(depth) + 1,
        "the outer call plus one per decrement of d6"
    );
    assert!(
        rec.cycles <= r.total_cycles,
        "no row may claim more cycles than the frame it ran in: {} > {}",
        rec.cycles,
        r.total_cycles
    );
    assert!(
        rec.self_cycles <= rec.cycles,
        "self is part of inclusive, never more than it"
    );
}

/// **W3.** `move.l #target,-(sp)` then `rts` is a jump wearing a return's clothes: the stack pointer it
/// leaves does not match the frame the routine was entered on, so it must not close it. If it did, the
/// dispatcher and everything it dispatched to would silently merge into the caller.
#[test]
fn the_move_l_rts_dispatch_does_not_close_its_caller() {
    let r = profile(ProfilerShape::Dispatch, 3);
    let dispatch = row(&r, PROF_DISPATCH);
    let leaf = row(&r, PROF_LEAF);
    assert_eq!(
        dispatch.calls, 1,
        "the dispatcher is entered once per frame, by a real JSR"
    );
    // The load-bearing assertion. The target calls LEAF, so LEAF's cost lands in the dispatcher's CHILD
    // time — but only if the dispatcher's frame was still open when the target ran. A return matched
    // loosely ("the stack unwound to at least here") closes the dispatcher on its own `rts`, and then
    // LEAF's cost belongs to whatever was underneath instead, leaving `cycles == self_cycles` here. So
    // this equation, and not the mere existence of the row, is what pins the exact match.
    assert!(
        leaf.cycles > 0,
        "the leaf must have cost something or the equation below is vacuous"
    );
    assert_eq!(
        dispatch.cycles,
        dispatch.self_cycles + leaf.cycles,
        "the dispatcher stayed open across its own rts, so everything the target called is its child \
         time (self {}, leaf {}, inclusive {})",
        dispatch.self_cycles,
        leaf.cycles,
        dispatch.cycles
    );
    assert!(
        !r.routines.contains_key(&PROF_TARGET),
        "the dispatch target was never CALLED, so it gets no row of its own — its cycles belong to \
         the dispatcher that jumped to it; rows present: {:#06X?}",
        r.routines.keys().collect::<Vec<_>>()
    );
}

// --- Interrupts: the conflation regression ---------------------------------------------------------

/// **W8, half one.** An HBlank-only run puts nothing in the VBlank bucket.
#[test]
fn an_hint_only_run_puts_nothing_in_vint() {
    let r = profile(
        ProfilerShape::Interrupts {
            hint: true,
            vint: false,
        },
        3,
    );
    let hint = r.interrupts.get(&HINT).copied().unwrap_or_default();
    assert!(
        hint.calls > 0,
        "HBlank interrupts were enabled and must have been taken; buckets: {:?}",
        r.interrupts
    );
    assert!(hint.cycles > 0, "and they cost something");
    assert!(
        !r.interrupts.contains_key(&VINT),
        "VBlank was disabled, so its bucket must not exist at all; buckets: {:?}",
        r.interrupts
    );
}

/// **W8, half two.** A VBlank-only run puts nothing in the HBlank bucket. The reference this replaces
/// reported HBlank + VBlank in `hint` and a structural zero in `vint`, so this direction is the one that
/// catches a straight transcription of it.
#[test]
fn a_vint_only_run_puts_nothing_in_hint() {
    let r = profile(
        ProfilerShape::Interrupts {
            hint: false,
            vint: true,
        },
        3,
    );
    let vint = r.interrupts.get(&VINT).copied().unwrap_or_default();
    assert_eq!(
        vint.calls, 1,
        "exactly one VBlank per frame, and `calls` counts the times it was TAKEN; buckets: {:?}",
        r.interrupts
    );
    assert!(vint.cycles > 0, "and it cost something");
    assert!(
        !r.interrupts.contains_key(&HINT),
        "HBlank was disabled, so its bucket must not exist at all; buckets: {:?}",
        r.interrupts
    );
}

/// **W8, the sharp form.** With both interrupts live the fixture points BOTH vectors at ONE handler, so
/// the two causes are indistinguishable by address. An accountant that keys an interrupt by where its
/// handler lives — which is exactly what the instrument this replaces did, testing the handler PC against
/// vector-table constants — cannot separate these no matter how carefully it compares. Only the
/// acknowledged cause can.
#[test]
fn two_causes_sharing_one_handler_address_still_land_in_separate_buckets() {
    let r = profile(
        ProfilerShape::Interrupts {
            hint: true,
            vint: true,
        },
        3,
    );
    let hint = r.interrupts.get(&HINT).copied().unwrap_or_default();
    let vint = r.interrupts.get(&VINT).copied().unwrap_or_default();
    assert!(
        hint.calls > 0,
        "HBlank must be counted; buckets: {:?}",
        r.interrupts
    );
    assert_eq!(
        vint.calls, 1,
        "exactly one VBlank per frame; buckets: {:?}",
        r.interrupts
    );
    assert!(
        hint.calls > vint.calls,
        "HBlank fires once a line and VBlank once a frame, so a single conflated total would be \
         unmistakably wrong: hint {} vs vint {}",
        hint.calls,
        vint.calls
    );
}

// --- The edges slice 1 wrote down ------------------------------------------------------------------

/// **W4.** A call that is still open when a frame boundary passes is **one** call, not one per frame it
/// touches. The leaf here `stop`s until a VBlank wakes it, and a VBlank arrives at the start of vblank —
/// which is the boundary — so the routine's frame is guaranteed to be open across it.
///
/// This is also why the shadow stack is deliberately *not* reset at a boundary.
#[test]
fn a_call_straddling_a_frame_boundary_is_one_call() {
    let prof = profiler_of(ProfilerShape::IdleInRoutine, 5);
    let r = prof.report();
    let raw = prof.sample_routines()[&PROF_LEAF];

    // The claim, on the undivided sample: the fixture makes exactly ONE call per frame, so the sample
    // holds one invocation per counted frame — less the one still in flight, because this leaf is
    // *always* mid-call when a boundary passes. Never two per frame, which is what a shadow stack torn
    // down and rebuilt at each boundary would produce.
    assert_eq!(
        raw.calls,
        r.frame_count - 1,
        "one invocation per counted frame, minus the one open across the closing boundary \
         (frames {}, calls {})",
        r.frame_count,
        raw.calls
    );
    // And it CLOSED. Counting the call is the easy half — a boundary cannot un-count a push that already
    // happened. That the invocation still completes is what pins the surviving stack: the `rts` arrives
    // after the boundary and can only be matched against a frame that is still there. Clear the stack at
    // each boundary and this collapses to nothing.
    assert!(
        raw.cycles > 0,
        "the straddling invocation was closed and recorded, not discarded"
    );
    assert_eq!(
        raw.cycles, raw.self_cycles,
        "and a childless routine's inclusive is exactly its self time (inclusive {}, self {})",
        raw.cycles, raw.self_cycles
    );
    // The divided view of the same thing, which is the CR's flooring rule in action: fewer invocations
    // than frames reports `0` rather than a fabricated `1`, and says so through `per_frame_exact`.
    assert_eq!(
        row(&r, PROF_LEAF).calls,
        0,
        "a routine invoked fewer times than the sample has frames floors to zero, never up"
    );
    assert!(
        !r.per_frame_exact,
        "and the report admits the truncation rather than absorbing it"
    );
}

/// Cycles retired while the CPU is `Stopped` are real cycles — the clock advances through them — so they
/// must land somewhere sane rather than vanish or pile onto whatever ran next. They belong to the
/// innermost open frame, which here is the routine that executed the `stop`.
#[test]
fn stopped_idle_cycles_land_on_the_open_routine_frame() {
    let r = profile(ProfilerShape::IdleInRoutine, 4);
    let leaf = row(&r, PROF_LEAF);
    // The leaf is three instructions plus a long wait. If the idle went anywhere else this would be tiny.
    assert!(
        leaf.self_cycles > 1_000,
        "the leaf spends most of the frame stopped, so its self time dominates: {}",
        leaf.self_cycles
    );
    assert!(
        leaf.self_cycles < r.total_cycles,
        "but not the whole frame — the outer loop and the interrupt run too"
    );
}

/// The mode-selected stack pointer, which is the trap the retire hook's two pointers exist to avoid. Here
/// a routine is called in **user** mode while VBlank interrupts are live, so each interrupt pushes its
/// frame on the *supervisor* stack and the `RTE` restores user mode before the profiler ever sees the
/// step. Matching an exception frame against the active A7 would silently fail to close the bucket and
/// leak every later routine into it; matching against the supervisor stack works either way.
#[test]
fn a_user_mode_routine_survives_a_supervisor_interrupt() {
    let r = profile(ProfilerShape::ModeSwitch, 4);
    let leaf = row(&r, PROF_LEAF);
    let vint = r.interrupts.get(&VINT).copied().unwrap_or_default();

    assert_eq!(
        leaf.calls, 1,
        "the user-mode call is seen and counted once per frame"
    );
    assert!(
        leaf.cycles > 0,
        "and it CLOSED — a frame that never closed would report no inclusive time, which is what a \
         stack corrupted by the mode switch would look like"
    );
    assert_eq!(
        vint.calls, 1,
        "the interrupt that interleaved with it is counted separately, once per frame"
    );
    assert!(
        vint.cycles > 0 && vint.cycles < r.total_cycles,
        "the bucket closed too: {} of {}",
        vint.cycles,
        r.total_cycles
    );
    assert!(
        leaf.cycles < r.total_cycles,
        "and the preempted routine did NOT absorb the handler's cost — an interrupt is not a callee"
    );
}

// --- Determinism ------------------------------------------------------------------------------------

/// Two identical runs produce identical output. The instrument is part of a surface whose consumers gate
/// on exact equality across boots, so a spread of zero is the requirement, not a nice property.
#[test]
fn two_identical_runs_produce_identical_output() {
    for shape in [
        ProfilerShape::CallsLeaf { k: 3 },
        ProfilerShape::TwoLevel,
        ProfilerShape::Recursive { depth: 3 },
        ProfilerShape::Interrupts {
            hint: true,
            vint: true,
        },
        ProfilerShape::ModeSwitch,
    ] {
        let a = profile(shape, 4);
        let b = profile(shape, 4);
        assert_eq!(a, b, "{shape:?} must profile identically twice");
    }
}

// --- The exception rules, driven synthetically ------------------------------------------------------
//
// The accumulator is a pure function of three inputs — the acknowledge event, the retire stream and the
// frame boundary — so it can be driven directly, without an emulator. That is not a shortcut: these are
// cases a ROM fixture cannot reach *reliably*. An interrupt has to land on one specific instruction, or a
// handler has to take an exception of its own at a controlled stack depth, and timing a real machine onto
// those is a coin flip. Here the sequence is the test.

/// The interrupt-acknowledge the 68000 drives on exception entry. The level is carried in the address
/// (`0xFFFFFFF1 | level << 1`), which is where the profiler reads the cause from.
fn iack(level: u8) -> BusEvent {
    BusEvent {
        op: BusOp::Read,
        fc: 7,
        addr: 0x00FF_FFF1 | (u32::from(level) << 1),
        size: Size::Word,
        value: 0,
    }
}

/// One retired step, costing [`STEP_CYCLES`]. `ssp` is spelled out at every call site because it is the
/// whole subject here.
fn step(pc: u32, opcode: u16, sp: u32, ssp: u32) -> StepRetire {
    StepRetire {
        pc,
        opcode,
        sp,
        ssp,
        cycles: STEP_CYCLES as u32,
    }
}

/// What every synthetic step costs, so the totals below are `n * STEP_CYCLES` and the arithmetic is
/// visible rather than magic.
const STEP_CYCLES: u64 = 10;
const OP_NOP: u16 = 0x4E71;
const OP_JSR_ABS_W: u16 = 0x4EB8;
const OP_RTE: u16 = 0x4E73;

/// A stack pointer to hang the arithmetic off. Exception frames are six bytes, so the interesting values
/// are `S`, `S - 6` and `S - 12`.
const S: u32 = 0x00FF_FF00;

/// **The not-executed opcode.** An exception entry is dispatched before the instruction at `pc` decodes,
/// so the retirement carries the opcode of an instruction that did **not** run. Here that instruction is
/// a `JSR`. Classifying it would arm a call the CPU never made, and the handler's first instruction would
/// then be recorded as a routine nobody invoked — a row for code that was never called.
///
/// This is why an interrupt is keyed off the acknowledge and never off the opcode field.
#[test]
fn the_not_executed_opcode_of_an_exception_entry_is_not_classified() {
    const PHANTOM: u32 = 0x0000_3000;
    let mut p = Profiler::new();
    p.on_frame_boundary(0); // open the sample
    p.on_step_retire(step(0x1000, OP_NOP, S, S));
    // The interrupt preempts a JSR: the acknowledge arrives during the step, and the step retires with
    // the JSR's opcode even though the JSR never executed.
    p.on_event(iack(VINT));
    p.on_step_retire(step(0x2000, OP_JSR_ABS_W, S - 6, S - 6));
    // The handler's first instruction. If the phantom call had been armed, THIS pc becomes a routine.
    p.on_step_retire(step(PHANTOM, OP_NOP, S - 6, S - 6));
    p.on_step_retire(step(PHANTOM + 2, OP_RTE, S, S));
    p.on_frame_boundary(1);

    assert!(
        !p.sample_routines().contains_key(&PHANTOM),
        "the handler's entry must not be recorded as a called routine; rows: {:#06X?}",
        p.sample_routines().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        p.sample_interrupts()[&VINT].calls,
        1,
        "and the interrupt itself was still counted, from its acknowledge"
    );
}

/// **The matching `RTE`.** An exception taken *inside* a handler — a `TRAP`, an address error — pushes its
/// own frame below the interrupt's, and its `RTE` unwinds only that one. Closing the bucket there would
/// end it early and hand the rest of the handler's cost to whatever it preempted.
///
/// Matching on the supervisor stack gets this right without knowing anything about traps: the trap's
/// `RTE` restores the SSP to a value that is not the bucket's frame plus six, so it does not match.
#[test]
fn an_rte_inside_a_handler_closes_the_trap_and_not_the_bucket() {
    let mut p = Profiler::new();
    p.on_frame_boundary(0);
    p.on_event(iack(VINT));
    p.on_step_retire(step(0x2000, OP_NOP, S - 6, S - 6)); // entry: the bucket's frame sits at S-6
    p.on_step_retire(step(0x3000, OP_NOP, S - 6, S - 6)); // handler
    p.on_step_retire(step(0x3002, OP_NOP, S - 12, S - 12)); // a TRAP entry: a second frame, at S-12
    p.on_step_retire(step(0x4000, OP_NOP, S - 12, S - 12)); // the trap's handler
    p.on_step_retire(step(0x4002, OP_RTE, S - 6, S - 6)); // the trap's RTE: back to S-6, NOT to S
    p.on_step_retire(step(0x3004, OP_NOP, S - 6, S - 6)); // still inside the interrupt handler
    p.on_step_retire(step(0x3006, OP_RTE, S, S)); // the interrupt's own RTE: S-6 + 6 == S
    p.on_frame_boundary(1);

    let vint = p.sample_interrupts()[&VINT];
    assert_eq!(vint.calls, 1, "one interrupt, opened once and closed once");
    // Seven steps ran inside the bucket: the entry, the handler either side of the trap, the whole trap,
    // and the closing RTE. If the trap's RTE had closed the bucket it would be three.
    assert_eq!(
        vint.cycles,
        7 * STEP_CYCLES,
        "the bucket spans the trap it contains, because the trap's RTE did not close it"
    );
}

/// An `RTE` that matches no open bucket closes nothing — and, in particular, does not pop somebody else's
/// frame. A handler entered before the profiler was armed must not retroactively open or close anything:
/// a bucket whose entry was never observed would report a cost the sample cannot account for.
#[test]
fn an_orphan_rte_closes_nothing() {
    let mut p = Profiler::new();
    p.on_frame_boundary(0);
    p.on_step_retire(step(0x1000, OP_NOP, S, S));
    p.on_step_retire(step(0x1002, OP_RTE, S + 6, S + 6)); // returning from an entry never observed
    p.on_step_retire(step(0x1004, OP_NOP, S + 6, S + 6));
    p.on_frame_boundary(1);

    assert!(
        p.sample_interrupts().is_empty(),
        "no bucket was ever opened, so none may be reported: {:?}",
        p.sample_interrupts()
    );
    // Nor may it close anything else. The frame underneath is whatever was running when the sample
    // opened; an unmatched RTE that popped it would end that span early and attribute the rest of the
    // frame to whatever happened to be beneath.
    assert!(
        p.sample_routines().is_empty(),
        "an unmatched RTE closes NOTHING, not merely no bucket; rows: {:#06X?}",
        p.sample_routines().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        p.report().frame_count,
        1,
        "and the sample is otherwise unharmed"
    );
}

/// **Nesting.** Cycles retired while a nested HBlank runs inside a VBlank handler belong to the HBlank
/// bucket alone; the suspended VBlank accrues nothing until the nested one closes. Both are still counted
/// exactly once.
#[test]
fn a_nested_hint_inside_a_vint_charges_the_inner_bucket_alone() {
    let mut p = Profiler::new();
    p.on_frame_boundary(0);
    p.on_event(iack(VINT));
    p.on_step_retire(step(0x2000, OP_NOP, S - 6, S - 6)); // VInt entry, frame at S-6
    p.on_step_retire(step(0x3000, OP_NOP, S - 6, S - 6)); // VInt handler
    p.on_event(iack(HINT));
    p.on_step_retire(step(0x3002, OP_NOP, S - 12, S - 12)); // HInt entry, frame at S-12
    p.on_step_retire(step(0x5000, OP_NOP, S - 12, S - 12)); // HInt handler
    p.on_step_retire(step(0x5002, OP_RTE, S - 6, S - 6)); // HInt's RTE: S-12 + 6
    p.on_step_retire(step(0x3004, OP_NOP, S - 6, S - 6)); // back in the VInt handler
    p.on_step_retire(step(0x3006, OP_RTE, S, S)); // VInt's RTE: S-6 + 6
    p.on_frame_boundary(1);

    let hint = p.sample_interrupts()[&HINT];
    let vint = p.sample_interrupts()[&VINT];
    assert_eq!((hint.calls, vint.calls), (1, 1), "each taken exactly once");
    // Three steps ran with the HInt open — its entry, its handler, its RTE.
    assert_eq!(
        (hint.self_cycles, hint.cycles),
        (3 * STEP_CYCLES, 3 * STEP_CYCLES),
        "the inner bucket's own time, and it called nothing"
    );
    // The other four are the VInt's, and its inclusive is NOT inflated by the bucket that preempted it —
    // an interrupt is not a callee of the interrupt it interrupted.
    assert_eq!(
        (vint.self_cycles, vint.cycles),
        (4 * STEP_CYCLES, 4 * STEP_CYCLES),
        "the outer bucket accrued nothing while the inner one was open"
    );
}
