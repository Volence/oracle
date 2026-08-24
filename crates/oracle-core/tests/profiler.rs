//! The CPU profiler's accumulator: per-routine cycle rows and per-cause interrupt buckets.
//!
//! Every expectation here is derived from a constant the fixture builder itself used
//! (`testrom::build_profiler`), never from a number read off a passing run. The fixtures share a skeleton
//! whose outer loop is gated on the V counter so the body executes **exactly once per frame** — that is
//! what turns "how many times was this called" into a constant a test can compare with `==`, which is the
//! same bar the consumers of this surface hold it to.

use oracle_core::bus::{BusEvent, BusEventSink, BusOp, Size, StepRetire};
use oracle_core::m68000::bus68k::Bus68k;
use oracle_core::profiler::{CallerKey, Counts, EdgeCounts, Profiler, Report, MAX_DEPTH};
use oracle_core::system::System;
use oracle_core::testrom::{
    self, ProfilerShape, StallKind, PROF_CPU_CYCLES_PER_FRAME, PROF_DISPATCH, PROF_LEAF, PROF_MID,
    PROF_MID_CALLS_LEAF, PROF_PREEMPT_CA, PROF_PREEMPT_CB, PROF_PREEMPT_FLAG, PROF_PREEMPT_R,
    PROF_PREEMPT_VINT_LIVE, PROF_PREEMPT_VINT_MASKED, PROF_REC, PROF_STALL, PROF_TARGET,
    PROF_VINT_H,
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

/// The row for `addr` on the **undivided** sample, or a failure naming what rows there were.
fn raw(p: &Profiler, addr: u32) -> Counts {
    *p.sample_routines().get(&addr).unwrap_or_else(|| {
        panic!(
            "no row for {addr:#06X}; rows present: {:#06X?}",
            p.sample_routines().keys().collect::<Vec<_>>()
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
    // The bucket is ADDITIVE to per-routine rows, not a replacement: a handler is code, and it gets its own
    // row keyed by its entry address. This is the row a consumer measuring the handler itself reads, and
    // the reason it exists is that the acknowledge armed it — nothing in the opcode stream could have.
    let handler = row(&r, PROF_VINT_H);
    assert_eq!(
        handler.calls,
        1,
        "the handler's own row, entered once per frame; rows: {:#06X?}",
        r.routines.keys().collect::<Vec<_>>()
    );
    assert!(
        handler.cycles > 0 && handler.cycles <= vint.cycles,
        "and its cost is part of the bucket's, never more ({} vs {})",
        handler.cycles,
        vint.cycles
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
    // The `- 1` is this FIXTURE'S PHASE, not a rule of the accountant: because the leaf is always
    // mid-call when a boundary passes, exactly one invocation is perpetually in flight and so has not
    // been recorded when the sample closes. A fixture whose calls complete inside their own frame reports
    // one per frame with no subtraction — see `a_routine_called_k_times_reports_calls_equal_to_k`.
    assert_eq!(
        raw.calls,
        r.frame_count - 1,
        "one invocation per counted frame, less the one still open at the close \
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
        ProfilerShape::Dispatch,
        ProfilerShape::IdleInRoutine,
        ProfilerShape::Interrupts {
            hint: true,
            vint: true,
        },
        ProfilerShape::ModeSwitch,
        ProfilerShape::Stall {
            kind: StallKind::Dma,
        },
    ] {
        let a = profile(shape, 4);
        let b = profile(shape, 4);
        assert_eq!(a, b, "{shape:?} must profile identically twice");
    }
}

// --- Stall attribution ------------------------------------------------------------------------------

/// A 68k→VDP DMA halts the CPU for the whole transfer, and that halt belongs to the routine that armed it
/// — not to the frame at large and not to whatever ran next. `stallCycles` is keyed identically to
/// `cycles`: same routine, same inclusive span, same division, which is what makes `cycles - stallCycles`
/// a well-formed subtraction rather than the difference of two differently-aggregated numbers.
#[test]
fn a_dma_stall_lands_on_the_routine_that_armed_it() {
    let r = profile(
        ProfilerShape::Stall {
            kind: StallKind::Dma,
        },
        3,
    );
    let stall_routine = row(&r, PROF_STALL);
    assert!(
        stall_routine.stall_cycles > 0,
        "the DMA held the bus, so the routine that triggered it carries stall time"
    );
    assert!(
        stall_routine.stall_cycles <= stall_routine.cycles,
        "and the stall is a SUBSET of the routine's cycles, never a quantity beside them ({} > {})",
        stall_routine.stall_cycles,
        stall_routine.cycles
    );
    assert_eq!(
        r.total_stall_cycles, stall_routine.stall_cycles,
        "nothing else in the frame stalled, so the routine's share is the whole frame's"
    );
    assert!(
        r.total_stall_cycles < r.total_cycles,
        "a stall is part of the frame, not the whole of it"
    );
}

/// **The boundary of the field.** A VRAM fill and a VRAM copy let the 68000 keep running, so a routine
/// that triggers one costs cycles but stalls for none. Without this, `stallCycles` could quietly become
/// "time the VDP was busy", which is a different and much less useful number — and one a consumer
/// subtracting it to recover an ideal-cycle figure would be actively misled by.
#[test]
fn a_fill_or_copy_costs_cycles_but_no_stall() {
    for kind in [StallKind::Fill, StallKind::Copy] {
        let r = profile(ProfilerShape::Stall { kind }, 3);
        let stall_routine = row(&r, PROF_STALL);
        assert!(
            stall_routine.cycles > 0,
            "{kind:?}: the routine ran and cost something"
        );
        assert_eq!(
            stall_routine.stall_cycles, 0,
            "{kind:?}: but the 68000 never waited, so it stalled for nothing"
        );
        assert_eq!(
            r.total_stall_cycles, 0,
            "{kind:?}: and neither did anything else in the frame"
        );
    }
}

// --- The sample window ------------------------------------------------------------------------------

/// **The reconciliation identity.** Every cycle the sample retired is charged to exactly one frame, and
/// every frame's accrual reaches its row — at a boundary while it is still running, at its pop for the
/// remainder. So the rows account for the sample exactly, with no tolerance:
///
/// ```text
/// Σ routines.self + Σ interrupts.self + unattributed == sample_cycles
/// ```
///
/// This is the assertion that makes every other number here trustworthy: a leak anywhere — a frame whose
/// cycles never reach a row, a checkpoint that double-counts, an interrupt suppressed without being
/// accounted for — shows up as a mismatch in one line. It is checked across shapes deliberately, because
/// each one stresses a different path (nested calls, recursion, preemption, a stalled DMA, a routine that
/// is always mid-call at the boundary).
#[test]
fn the_rows_account_for_the_sample_exactly() {
    for shape in [
        ProfilerShape::CallsLeaf { k: 3 },
        ProfilerShape::TwoLevel,
        ProfilerShape::Recursive { depth: 3 },
        ProfilerShape::Dispatch,
        ProfilerShape::IdleInRoutine,
        ProfilerShape::Interrupts {
            hint: true,
            vint: true,
        },
        ProfilerShape::ModeSwitch,
        ProfilerShape::Stall {
            kind: StallKind::Dma,
        },
    ] {
        let prof = profiler_of(shape, 4);
        let r = prof.report();
        let rows: u64 = prof.sample_routines().values().map(|c| c.self_cycles).sum();
        let buckets: u64 = prof
            .sample_interrupts()
            .values()
            .map(|c| c.self_cycles)
            .sum();
        assert_eq!(
            rows + buckets + r.unattributed_cycles,
            r.sample_cycles,
            "{shape:?}: rows {rows} + buckets {buckets} + unattributed {} must equal the sample {}",
            r.unattributed_cycles,
            r.sample_cycles
        );
        assert!(
            r.sample_cycles > 0,
            "{shape:?}: a sample of nothing proves nothing"
        );
    }
}

/// **A routine that never returns still gets a row.** The outer loop of any real program is exactly this:
/// entered once (or never observed being entered at all) and running when the sample closes. Reporting
/// nothing for it while its cycles inflate the total is the worst of both — the reader sees a large
/// `totalCycles` and rows that do not add up to it.
///
/// `calls` stays `0`, which is the honest signal: cycles were observed, a completed invocation was not.
#[test]
fn a_routine_still_running_at_the_close_reports_cycles_with_no_completed_call() {
    let prof = profiler_of(ProfilerShape::CallsLeaf { k: 3 }, 4);
    let r = prof.report();

    // The fixture's outer loop was never called from anywhere the profiler saw, so it is the synthesized
    // root: a row at whatever address retired first, which is somewhere inside the loop rather than its
    // entry. It is the only row with no completed call.
    let open: Vec<(u32, u64)> = prof
        .sample_routines()
        .iter()
        .filter(|(_, c)| c.calls == 0)
        .map(|(&a, c)| (a, c.self_cycles))
        .collect();
    assert_eq!(
        open.len(),
        1,
        "exactly one never-completed row — the loop the sample opened inside; saw {open:#06X?}"
    );
    assert!(
        open[0].1 > 0,
        "and it carries the cycles it actually burned, not a placeholder"
    );
    // It dominates: the leaf is three short calls, the loop is the rest of the frame.
    assert!(
        open[0].1 > row(&r, PROF_LEAF).self_cycles,
        "the loop outweighs the leaf it calls ({} vs {})",
        open[0].1,
        row(&r, PROF_LEAF).self_cycles
    );
}

/// **Nothing from before the sample leaks into it.** Arming mid-run leaves frames open whose accrued time
/// is however long the machine had already been running — unbounded, and nothing to do with the frames
/// being measured. The opening boundary forgets it, keeping the frame itself (a call in flight is still one
/// call, and its return must still match) while discarding what it accrued.
///
/// Stated exactly, which a ROM cannot do: a real fixture's per-frame cost jitters by a few cycles depending
/// where in its wait loop the sample opens, so "the same to the cycle" is not a claim a ROM can support.
/// Here the stream is the specification.
#[test]
fn cycles_from_before_the_opening_boundary_do_not_leak_in() {
    let mut p = Profiler::new();
    // Three steps BEFORE the sample opens: 30 cycles the sample must never see.
    for i in 0..3 {
        p.on_step_retire(step(0x1000 + i * 2, OP_NOP, S, S));
    }
    p.on_frame_boundary(0); // the sample opens here
    p.on_step_retire(step(0x1006, OP_NOP, S, S));
    p.on_step_retire(step(0x1008, OP_NOP, S, S));
    p.on_frame_boundary(1);

    let r = p.report();
    assert_eq!(
        r.sample_cycles,
        2 * STEP_CYCLES,
        "only the two in-sample steps count; the three before the opening boundary are gone"
    );
    // The frame that straddled the boundary is the same frame — kept, not rebuilt — carrying only its
    // post-boundary accrual.
    assert_eq!(
        p.sample_routines().get(&0x1000).map(|c| c.self_cycles),
        Some(2 * STEP_CYCLES),
        "the straddling frame reports what it did INSIDE the sample, not what it did before; \
         rows: {:#06X?}",
        p.sample_routines().keys().collect::<Vec<_>>()
    );
    assert_eq!(p.open_frames(), 1, "and it is still the same open frame");
}

/// **An interrupt in flight across the opening boundary opens no bucket.** Its acknowledge happened before
/// the sample, so counting it would report a cost whose entry the sample never saw — the contract's
/// explicit MUST NOT. This is the ordinary phase, not a corner: a VBlank fires at line 224, which is the
/// boundary line, so a handler is very often mid-flight exactly here.
///
/// Note what is NOT suppressed. The handler's own routine frame is ordinary code that demonstrably ran, so
/// its post-boundary cycles land in its row as usual; only the *cause* accounting is retroactive-sensitive.
/// That is why the `unattributed_cycles` escape hatch is normally zero — with a handler row absorbing the
/// work, a suppressed bucket's own self time is just its entry step, which the boundary already discarded.
/// The second half below constructs the case where it is not zero, so the identity is closed there too.
#[test]
fn an_interrupt_straddling_the_opening_boundary_is_not_bucketed_retroactively() {
    let mut p = Profiler::new();
    // Before the sample: an interrupt is taken and its handler starts running.
    p.on_event(iack(VINT));
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6));
    p.on_step_retire(step(0x3000, OP_NOP, S - 6, S - 6));
    // The sample opens with that handler still on the stack.
    p.on_frame_boundary(0);
    p.on_step_retire(step(0x3002, OP_NOP, S - 6, S - 6));
    p.on_step_retire(step(0x3004, OP_RTE, S, S)); // it finishes INSIDE the sample
    p.on_step_retire(step(0x1000, OP_NOP, S, S));
    p.on_frame_boundary(1);

    assert!(
        p.sample_interrupts().is_empty(),
        "the bucket must not be opened after the fact; buckets: {:?}",
        p.sample_interrupts()
    );
    assert_eq!(
        p.sample_routines().get(&0x3000).map(|c| c.self_cycles),
        Some(2 * STEP_CYCLES),
        "but the handler's own code still gets its row: it ran, and the sample watched it run"
    );
    // A LATER interrupt, whose acknowledge the sample DID see, is bucketed normally — so this is the
    // suppression of one unobserved entry, not the bucket machinery being broken.
    p.on_event(iack(VINT));
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6));
    p.on_step_retire(step(0x3000, OP_RTE, S, S));
    p.on_frame_boundary(2);
    assert_eq!(
        p.sample_interrupts()[&VINT].calls,
        1,
        "the interrupt the sample DID see taken is counted"
    );
}

/// The `unattributed_cycles` escape hatch, on the one shape that produces it: a suppressed bucket that
/// accrues time of its OWN inside the sample, because its handler routine has already returned and the
/// bucket itself is what is left on the stack. Deliberately constructed — the point is not that this is
/// common (it is not), but that the reconciliation identity stays closed when it happens instead of
/// quietly losing the cycles.
#[test]
fn a_suppressed_buckets_own_time_is_reported_as_unattributed() {
    let mut p = Profiler::new();
    p.on_event(iack(VINT));
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6)); // bucket, frame at S-6
    p.on_step_retire(step(0x3000, OP_NOP, S - 6, S - 6)); // the handler's routine frame opens
    p.on_step_retire(step(0x3002, OP_RTS, S - 2, S - 2)); // ...and returns, leaving the bucket on top
    p.on_frame_boundary(0); // the sample opens: the bucket is suppressed
    p.on_step_retire(step(0x3004, OP_NOP, S - 2, S - 2)); // 10 cycles charged to the bucket itself
    p.on_step_retire(step(0x3006, OP_RTE, S, S)); // 10 more, then it closes
    p.on_frame_boundary(1);

    let r = p.report();
    assert!(
        p.sample_interrupts().is_empty(),
        "still no retroactive bucket; buckets: {:?}",
        p.sample_interrupts()
    );
    assert_eq!(
        r.unattributed_cycles,
        2 * STEP_CYCLES,
        "the suppressed bucket's own in-sample time is reported, not dropped"
    );
    let rows: u64 = p.sample_routines().values().map(|c| c.self_cycles).sum();
    assert_eq!(
        rows + r.unattributed_cycles,
        r.sample_cycles,
        "and the identity closes: rows {rows} + unattributed {} == sample {}",
        r.unattributed_cycles,
        r.sample_cycles
    );
}

// --- Bounding the damage ----------------------------------------------------------------------------

/// **One frame the accountant loses track of must not empty the rest of the report.** If a return goes
/// unmatched its frame stays on the stack, and a search that only ever looks at the top will never match
/// again: every later call piles on, the stack grows without bound, and every row after the wedge silently
/// disappears. The failure is far worse than the wedge that caused it.
///
/// Searching innermost-first through the whole stack recovers, and it cannot loosen anything, because what
/// it looks for is the SAME exact `entry_sp` match. Frames above the one it finds are unwound as abandoned
/// — cycles kept, calls not — and counted, so the recovery is visible.
#[test]
fn an_unmatched_return_deeper_in_the_stack_is_still_found() {
    const OUTER: u32 = 0x0000_2000;
    const INNER: u32 = 0x0000_3000;
    let mut p = Profiler::new();
    p.on_frame_boundary(0);
    p.on_step_retire(step(0x1000, OP_NOP, S, S)); // the root
    p.on_step_retire(step(0x1002, OP_JSR_ABS_W, S - 4, S - 4));
    p.on_step_retire(step(OUTER, OP_NOP, S - 4, S - 4)); // OUTER, entry_sp = S-4
    p.on_step_retire(step(OUTER + 2, OP_JSR_ABS_W, S - 8, S - 8));
    p.on_step_retire(step(INNER, OP_NOP, S - 8, S - 8)); // INNER, entry_sp = S-8
                                                         // INNER leaves by a route the accountant cannot match — its stack pointer lands somewhere no frame
                                                         // was entered at — so its frame is WEDGED on top of OUTER's.
    p.on_step_retire(step(INNER + 2, OP_RTS, S - 6, S - 6));
    assert_eq!(p.open_frames(), 3, "the unmatched return left INNER wedged");
    // OUTER now returns properly. The top of the stack is a stranger; the match is one deeper.
    p.on_step_retire(step(OUTER + 4, OP_RTS, S, S));
    p.on_frame_boundary(1);

    assert_eq!(
        p.open_frames(),
        1,
        "the wedge is cleared and only the root remains"
    );
    assert_eq!(
        p.sample_routines().get(&OUTER).map(|c| c.calls),
        Some(1),
        "OUTER's return was found and its invocation completed"
    );
    assert_eq!(
        p.sample_routines().get(&INNER).map(|c| c.calls),
        Some(0),
        "INNER ran, so it has cycles — but it never completed, so it has no call"
    );
    assert!(
        p.sample_routines()[&INNER].self_cycles > 0,
        "and those cycles are real, not a placeholder row"
    );
    assert_eq!(
        p.report().abandoned_frames,
        1,
        "the recovery is reported, not silent"
    );
}

/// **The stack is bounded, and the bound is derived.** 64 KiB of work RAM divided by the 4-byte return
/// address a call pushes is the deepest any program whose stack lives in RAM could nest; past that the
/// accountant is certainly following something that is not a call stack. Refusing the push keeps the
/// damage to one overstated row instead of unbounded memory, and the refusals are counted.
#[test]
fn the_shadow_stack_is_capped_and_says_so() {
    const OVERSHOOT: usize = 5;
    let mut p = Profiler::new();
    p.on_frame_boundary(0);
    // Drive strictly more calls than the cap allows, none of them returning.
    let mut sp = 0x00FF_FFFC_u32;
    for i in 0..(MAX_DEPTH + OVERSHOOT) as u32 {
        p.on_step_retire(step(0x1_0000 + i * 8, OP_JSR_ABS_W, sp, sp));
        sp = sp.wrapping_sub(4);
        p.on_step_retire(step(0x2_0000 + i * 8, OP_NOP, sp, sp));
    }
    p.on_frame_boundary(1);

    assert_eq!(
        p.open_frames(),
        MAX_DEPTH,
        "the stack stops growing at the bound"
    );
    let r = p.report();
    assert_eq!(
        r.depth_exceeded,
        OVERSHOOT as u64 + 1,
        "every refused call is counted, so the reader knows rows are missing — one more than the \
         overshoot because the synthesized root holds a slot of its own"
    );
    // The refused calls' cycles are not lost — they charge to the deepest frame still tracked, which the
    // identity below proves is still whole.
    let rows: u64 = p.sample_routines().values().map(|c| c.self_cycles).sum();
    assert_eq!(
        rows + r.unattributed_cycles,
        r.sample_cycles,
        "the totals still reconcile at the bound"
    );
}

/// **The cross-mode coincidence.** `sp` is mode-selected, and the user and supervisor stacks are
/// independent — so a user-mode frame and a supervisor-mode return can meet at the same numeric stack
/// pointer while having nothing whatever to do with each other. Matching on the pointer alone closes the
/// user frame on the supervisor's return: the wrong routine's invocation ends, its cycles stop, and the
/// rest of it is charged to whatever was underneath. Nothing about that is visible in the output.
///
/// Constructed synthetically because it must be: the coincidence needs `usp` and `ssp` to line up to the
/// byte, which a ROM cannot be asked to arrange on demand — the `ModeSwitch` fixture exercises the mode
/// change but never the collision.
#[test]
fn a_supervisor_return_cannot_close_a_user_mode_frame_that_merely_shares_its_pointer() {
    const USER_ROUTINE: u32 = 0x0000_2000;
    let user = |pc: u32, opcode: u16, sp: u32| StepRetire {
        supervisor: false,
        ..step(pc, opcode, sp, S)
    };
    let mut p = Profiler::new();
    p.on_frame_boundary(0);
    // In USER mode, on the user stack, which happens to sit at the same address as the supervisor stack.
    p.on_step_retire(user(0x1000, OP_NOP, S));
    p.on_step_retire(user(0x1002, OP_JSR_ABS_W, S - 4));
    p.on_step_retire(user(USER_ROUTINE, OP_NOP, S - 4)); // entry_sp = S-4, user
                                                         // A SUPERVISOR return whose stack pointer is exactly what would close that user frame.
    p.on_step_retire(step(0x9000, OP_RTS, S, S)); // supervisor: true
    p.on_frame_boundary(1);

    assert_eq!(
        p.sample_routines().get(&USER_ROUTINE).map(|c| c.calls),
        Some(0),
        "the user routine has NOT completed — a supervisor return is not its return; rows: {:#06X?}",
        p.sample_routines().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        p.open_frames(),
        2,
        "and it is still open, under the root, exactly where it was"
    );
}

// --- The opt-in per-frame ring ----------------------------------------------------------------------

/// The ring records **one row per counted frame**, cut from the same figures the aggregate commits, so a
/// per-frame row and the sample can never describe different frames. It is undivided by construction — a
/// per-frame row is already per frame — which is what lets a consumer see variance *inside* a sample
/// instead of inferring it from repeated whole-boot runs.
#[test]
fn the_per_frame_ring_records_one_row_per_counted_frame() {
    let mut sys = System::new(0x1234_5678);
    sys.load_rom(testrom::build_profiler(ProfilerShape::Interrupts {
        hint: false,
        vint: true,
    }));
    sys.reset();
    let mut prof = Profiler::with_per_frame(64);
    sys.run_frames_with_sink(5, &mut prof);
    let r = prof.report();

    assert!(prof.per_frame_armed(), "the ring is armed");
    assert_eq!(
        prof.per_frame().len() as u64,
        r.frame_count,
        "one row per counted frame — the same divisor, not a second count of its own"
    );
    // The rows are the sample, decomposed: their cycles sum to it exactly.
    let summed: u64 = prof.per_frame().iter().map(|f| f.cycles).sum();
    assert_eq!(
        summed, r.sample_cycles,
        "the per-frame rows ARE the sample: {summed} vs {}",
        r.sample_cycles
    );
    // Every frame of this fixture takes exactly one VBlank and no HBlank, which is what makes the ring's
    // cause split checkable rather than merely present. The bucket is INCLUSIVE of the handler it armed a
    // row for, so it is the entry plus the bare `rte` — small, and decidedly not zero.
    for row in prof.per_frame() {
        assert!(row.vint_cycles > 0, "each frame took its VBlank: {row:?}");
        assert_eq!(row.hint_cycles, 0, "and no HBlank was enabled: {row:?}");
        assert!(
            row.vint_cycles < row.cycles,
            "the interrupt is part of the frame, not the whole of it: {row:?}"
        );
    }
    // Frame indices advance by one and are the machine's own coordinate, not a tally.
    let frames: Vec<u64> = prof.per_frame().iter().map(|f| f.frame).collect();
    for w in frames.windows(2) {
        assert_eq!(w[1], w[0] + 1, "consecutive boundaries: {frames:?}");
    }
}

/// **Off by default, and its absence is the signal.** A ring that silently recorded nothing would be
/// indistinguishable from one that recorded a sample with no frames in it, so the unarmed instrument
/// reports an empty ring AND says it is unarmed.
#[test]
fn the_ring_is_off_unless_asked_for() {
    let prof = profiler_of(ProfilerShape::CallsLeaf { k: 3 }, 4);
    assert!(!prof.per_frame_armed(), "not armed unless asked");
    assert!(prof.per_frame().is_empty(), "and it recorded nothing");
    // The aggregate is unaffected — the ring is additive, not a mode.
    assert!(prof.report().frame_count > 0, "the sample still ran");
}

/// The ring is **bounded**, keeping the most recent frames. A profiler left armed across a long session
/// must not grow without limit, and the frames a consumer wants are the ones nearest the symptom.
#[test]
fn the_ring_keeps_the_most_recent_frames_and_no_more() {
    const DEPTH: usize = 2;
    let mut sys = System::new(0x1234_5678);
    sys.load_rom(testrom::build_profiler(ProfilerShape::CallsLeaf { k: 1 }));
    sys.reset();
    let mut prof = Profiler::with_per_frame(DEPTH);
    sys.run_frames_with_sink(6, &mut prof);
    let r = prof.report();

    assert!(
        r.frame_count > DEPTH as u64,
        "the run must outlast the ring or this proves nothing ({} frames)",
        r.frame_count
    );
    assert_eq!(
        prof.per_frame().len(),
        DEPTH,
        "capped at the depth asked for"
    );
    // Most-recent, not first: the last row's frame index is the sample's last boundary.
    let last = prof.per_frame().back().expect("non-empty").frame;
    let first = prof.per_frame().front().expect("non-empty").frame;
    assert_eq!(
        last - first,
        DEPTH as u64 - 1,
        "the rows are the final {DEPTH} frames, contiguous"
    );
}

// --- The caller lens (§11.18 / CR-28) ---------------------------------------------------------------

/// As [`profiler_of`], but with the **caller lens** armed.
fn profiler_with_callers(shape: ProfilerShape, frames: u64) -> Profiler {
    let mut sys = System::new(0x1234_5678);
    sys.load_rom(testrom::build_profiler(shape));
    sys.reset();
    let mut prof = Profiler::with_lenses(0, true);
    sys.run_frames_with_sink(frames, &mut prof);
    prof
}

/// Every committed edge whose **callee** is `addr`, undivided.
fn edges_of(p: &Profiler, addr: u32) -> Vec<(CallerKey, EdgeCounts)> {
    p.sample_callers()
        .iter()
        .filter(|((callee, _), _)| *callee == addr)
        .map(|((_, caller), e)| (*caller, *e))
        .collect()
}

/// **The two normative sums, in the accumulator that produces them.**
///
/// Every invocation has exactly one caller, so a callee's edges *partition* its row: their `self_cycles`
/// sum to the row's and their `calls` to the row's, **undivided on both sides**. Asserted with `==` rather
/// than a bound, which is the whole reason the wire carries undivided partners on the edge — a divided sum
/// would fall short by up to one unit per edge and read exactly like agreement.
///
/// Two fixtures, because one of them alone would prove less than it looks. `TwoLevel` gives a leaf reached
/// through a middle routine — a single edge, where a partition is trivially true — while `CallsLeaf` has
/// the leaf reached repeatedly from the frame the sample opened on. The `inclusive` figure is deliberately
/// **not** summed: it double-counts by construction, which is why the contract states the sum on self.
#[test]
fn a_rows_edges_partition_it_exactly() {
    for (shape, callee) in [
        (ProfilerShape::TwoLevel, PROF_LEAF),
        (ProfilerShape::CallsLeaf { k: 3 }, PROF_LEAF),
        (ProfilerShape::TwoLevel, PROF_MID),
    ] {
        let p = profiler_with_callers(shape, 6);
        let r = raw(&p, callee);
        let edges = edges_of(&p, callee);
        assert!(
            !edges.is_empty(),
            "{shape:?}: an armed row always acquires at least one edge — an empty list is a defect in \
             the accountant, not an ordinary answer"
        );
        assert!(
            r.calls > 0 && r.self_cycles > 0,
            "{shape:?}: the partition must have something to partition: {r:?}"
        );
        assert_eq!(
            edges.iter().map(|(_, e)| e.calls).sum::<u64>(),
            r.calls,
            "{shape:?}: the edges' calls sum EXACTLY to the row's: {edges:?} vs {r:?}"
        );
        assert_eq!(
            edges.iter().map(|(_, e)| e.self_cycles).sum::<u64>(),
            r.self_cycles,
            "{shape:?}: and their self cycles likewise: {edges:?} vs {r:?}"
        );
    }
}

/// **An interrupt-entered edge is keyed by CAUSE, and the two causes stay apart** — even when the two
/// vectors point at one handler, which is exactly when an accountant keying by handler address cannot tell
/// them apart at all.
///
/// This fixture is the sharpest form of the conflation regression, one level down: `PROF_VINT_H` is the
/// handler for both levels, so its row is one row — and its edge list must still be **two** edges, one per
/// acknowledged cause. A single collapsing `interrupt` value would make this assertion unwritable, which is
/// why the contract ships four enum values rather than the three the demand side asked for.
#[test]
fn a_handler_reached_from_two_causes_has_two_edges_keyed_by_cause() {
    let p = profiler_with_callers(
        ProfilerShape::Interrupts {
            hint: true,
            vint: true,
        },
        6,
    );
    let edges = edges_of(&p, PROF_VINT_H);
    let kinds: Vec<CallerKey> = edges.iter().map(|(k, _)| *k).collect();
    assert!(
        kinds.contains(&CallerKey::Interrupt(HINT)) && kinds.contains(&CallerKey::Interrupt(VINT)),
        "one handler address, two acknowledged causes, two distinct edges: {edges:?}"
    );
    for (kind, e) in &edges {
        assert!(
            e.calls > 0 && e.self_cycles > 0,
            "neither cause is a vacuous zero row: {kind:?} {e:?}"
        );
    }
    // And the handler is never given a fabricated calling address: its caller IS a bucket.
    assert!(
        !kinds.iter().any(|k| matches!(k, CallerKey::Routine(_))),
        "an interrupt handler has no calling routine — a routine key here would be an invention: {edges:?}"
    );
}

/// **The frame the sample opened on is `Root`, and the routines it calls carry its address.**
///
/// Both halves of the one shape that carries two senses of the word *root*: the opening frame's own edge is
/// `Root` (nothing was ever observed calling it), while an edge *from* it carries a real `callerAddr` that
/// is mid-routine and is **not** an entry point. A client that assumes every caller address resolves like a
/// row key mis-renders exactly this edge.
#[test]
fn the_opening_frame_is_root_and_its_callees_carry_its_mid_routine_address() {
    let p = profiler_with_callers(ProfilerShape::CallsLeaf { k: 3 }, 6);
    let roots: Vec<(u32, CallerKey)> = p
        .sample_callers()
        .keys()
        .copied()
        .filter(|(_, caller)| *caller == CallerKey::Root)
        .collect();
    assert_eq!(
        roots.len(),
        1,
        "exactly one frame was opened on rather than called into: {roots:#06X?}"
    );
    let (root_addr, _) = roots[0];

    let leaf_callers: Vec<CallerKey> = edges_of(&p, PROF_LEAF).iter().map(|(k, _)| *k).collect();
    assert_eq!(
        leaf_callers,
        vec![CallerKey::Routine(root_addr)],
        "the leaf is reached from the opening frame, by its real address: {leaf_callers:#06X?}"
    );
    assert_ne!(
        root_addr, PROF_LEAF,
        "and that address is the main loop's, not the leaf's"
    );
}

/// **The lens is off unless asked for, and off means the second map was never populated.**
///
/// The aggregate is asserted **byte-identical** between an armed and an unarmed run of the same fixture —
/// the accumulator-level form of the amendment's central claim, that a client which never arms the lens
/// reads exactly the reply this surface already sent. An always-on accumulator would break this first.
#[test]
fn the_lens_is_off_unless_asked_for_and_changes_nothing_when_it_is_on() {
    let off = profiler_of(ProfilerShape::CallsLeaf { k: 3 }, 6);
    assert!(!off.callers_armed(), "not armed unless asked");
    assert!(
        off.sample_callers().is_empty(),
        "and the second map was never populated: {:?}",
        off.sample_callers()
    );
    assert!(
        off.report().callers.is_empty(),
        "so the report carries no edges either"
    );

    let on = profiler_with_callers(ProfilerShape::CallsLeaf { k: 3 }, 6);
    assert!(on.callers_armed() && !on.sample_callers().is_empty());
    let (mut a, mut b) = (off.report(), on.report());
    assert!(
        !b.callers.is_empty(),
        "the armed run really produced edges, or the comparison below is vacuous"
    );
    // Compare everything EXCEPT the two fields arming is *allowed* to move. Cleared and normalised rather
    // than skipped field-by-field, so a field added to `Report` later is caught by this `==` rather than
    // quietly excluded from it.
    //
    // `per_frame_exact` is the documented exception and the only one: arming ADDS divided figures, so a
    // sample that divided evenly without the lens may not with it. Nothing became less exact — more figures
    // are being reported on — and the direction it may move is asserted immediately below rather than
    // waved past.
    assert!(
        !b.per_frame_exact || a.per_frame_exact,
        "arming the lens may only ever turn perFrameExact true->false, never false->true: {} -> {}",
        a.per_frame_exact,
        b.per_frame_exact
    );
    a.callers.clear();
    b.callers.clear();
    b.per_frame_exact = a.per_frame_exact;
    assert_eq!(
        a, b,
        "arming the lens moved an aggregate figure — it is a second lens on the same rows, not a mode"
    );
}

/// **Arming the lens can turn `perFrameExact` from `true` to `false`, and that is not a defect.**
///
/// The one behaviour a client could reasonably have assumed was unaffected, so it is pinned rather than
/// described. Driven synthetically because it has to be *constructed*: the property needs a sample where
/// every row and aggregate figure divides evenly while an **edge** figure does not, and no ROM fixture can
/// be asked to arrange that on demand.
///
/// The construction, with a divisor of **2** (three boundaries, the first of which opens the sample):
/// `A` calls both `B1` and `B2` in **every** frame — so their rows' counts are even — but only *one* of
/// them calls the leaf in each frame, alternating. The leaf's row is therefore `calls: 2` (exact) while
/// each of its two edges is `calls: 1` (not). Every other figure is arranged to be even, so the flag can
/// only move for the reason under test.
#[test]
fn arming_the_lens_can_turn_per_frame_exact_from_true_to_false() {
    const A: u32 = 0x0001_0000;
    const B1: u32 = 0x0002_0000;
    const B2: u32 = 0x0003_0000;
    const CALLEE: u32 = 0x0004_0000;

    let drive = |p: &mut Profiler| {
        // Establish the root frame BEFORE the sample opens, so the two frames below are step-for-step
        // identical and every aggregate divides evenly.
        p.on_step_retire(step(A, OP_NOP, S, S));
        p.on_frame_boundary(0);
        for frame in 0..2u64 {
            for b in [B1, B2] {
                let calls_leaf = (b == B1) == (frame == 0);
                p.on_step_retire(step(A + 2, OP_JSR_ABS_W, S - 4, S - 4)); // charged to the root
                p.on_step_retire(step(b, OP_NOP, S - 4, S - 4)); // b's frame opens
                if calls_leaf {
                    p.on_step_retire(step(b + 2, OP_JSR_ABS_W, S - 8, S - 8));
                    p.on_step_retire(step(CALLEE, OP_NOP, S - 8, S - 8)); // the leaf, from b
                    p.on_step_retire(step(CALLEE + 2, OP_RTS, S - 4, S - 4));
                } else {
                    // A no-op in b's place, so both arms cost b the same three steps and its ROW divides
                    // evenly however the leaf's edges fall.
                    p.on_step_retire(step(b + 2, OP_NOP, S - 4, S - 4));
                }
                p.on_step_retire(step(b + 4, OP_RTS, S, S));
            }
            p.on_frame_boundary(frame + 1);
        }
    };

    let mut off = Profiler::new();
    drive(&mut off);
    let mut on = Profiler::with_lenses(0, true);
    drive(&mut on);

    let (a, b) = (off.report(), on.report());
    assert_eq!(a.frame_count, 2, "the divisor this rests on: {a:?}");
    assert_eq!(
        raw(&off, CALLEE).calls,
        2,
        "the row is called twice across the sample, so it divides evenly"
    );
    let edges = edges_of(&on, CALLEE);
    assert_eq!(
        edges.len(),
        2,
        "…once from each of two callers, so each edge does NOT: {edges:?}"
    );
    assert!(
        edges.iter().all(|(_, e)| e.calls == 1),
        "each edge saw exactly one invocation: {edges:?}"
    );
    assert!(
        a.per_frame_exact,
        "without the lens this sample divides without remainder: {a:?}"
    );
    assert!(
        !b.per_frame_exact,
        "and arming it reports on figures that do not — one flag, ranging over every divided figure \
         in the reply: {b:?}"
    );
}

/// **The depth cap never becomes a caller we really did track.**
///
/// `CallerKey::DepthCap` says *the calling frame was one the accountant declined to track*. That is a
/// claim about a frame we lost, and the failure mode worth guarding is the opposite one: attributing it to
/// a call whose caller was sitting on the stack all along. Driving strictly more calls than
/// [`MAX_DEPTH`] allows produces the refusals — `depth_exceeded` proves it — and **no edge may carry
/// `DepthCap`**, because the only way below the cap is a pop, and a pop means the frame on top is one we
/// tracked.
///
/// That makes the arm unreachable from this accumulator today, which is stated in `Profiler::depth_capped`
/// rather than hidden: the value exists on the wire and in the enum, the latch is cleared in the one
/// direction that could make it wrong, and this is the negative half.
#[test]
fn the_depth_cap_is_never_attributed_to_a_caller_we_did_track() {
    const OVERSHOOT: usize = 5;
    let mut p = Profiler::with_lenses(0, true);
    p.on_frame_boundary(0);
    let mut sp = 0x00FF_FFFC_u32;
    for i in 0..(MAX_DEPTH + OVERSHOOT) as u32 {
        p.on_step_retire(step(0x1_0000 + i * 8, OP_JSR_ABS_W, sp, sp));
        sp = sp.wrapping_sub(4);
        p.on_step_retire(step(0x2_0000 + i * 8, OP_NOP, sp, sp));
    }
    p.on_frame_boundary(1);

    assert!(
        p.report().depth_exceeded > 0,
        "the cap must actually have been hit or this proves nothing"
    );
    assert_eq!(p.open_frames(), MAX_DEPTH, "and the stack is at its bound");

    // **The step that makes this test bite.** Refusals alone prove nothing: no frame can be pushed while
    // the stack is at the cap, so the attribution is never exercised. So unwind ONE frame with a matched
    // return and call again. The new frame's caller is the routine that really is beneath it — the latch
    // was cleared by the pop — and attributing it to a caller the accountant *declined to track* would be
    // inventing a lost frame for a call whose caller was on the stack all along.
    let innermost_entry_sp = 0x00FF_FFFC_u32.wrapping_sub(4 * (MAX_DEPTH as u32 - 2));
    let after_return = innermost_entry_sp.wrapping_add(4);
    p.on_step_retire(step(0x3_0000, OP_RTS, after_return, after_return));
    assert_eq!(
        p.open_frames(),
        MAX_DEPTH - 1,
        "the matched return really popped a frame, or nothing below is exercised"
    );
    const NEW_CALLEE: u32 = 0x0005_0000;
    p.on_step_retire(step(
        0x3_0002,
        OP_JSR_ABS_W,
        innermost_entry_sp,
        innermost_entry_sp,
    ));
    p.on_step_retire(step(
        NEW_CALLEE,
        OP_NOP,
        innermost_entry_sp - 4,
        innermost_entry_sp - 4,
    ));
    assert_eq!(
        p.open_frames(),
        MAX_DEPTH,
        "…and the call after it WAS tracked, so it has an edge to get wrong"
    );
    p.on_frame_boundary(2);

    assert!(
        !p.sample_callers().is_empty(),
        "edges must have been recorded, or the check below is vacuous"
    );
    let new_edges: Vec<_> = p
        .sample_callers()
        .keys()
        .filter(|(callee, _)| *callee == NEW_CALLEE)
        .collect();
    assert_eq!(
        new_edges.len(),
        1,
        "the post-unwind call has exactly one edge: {new_edges:#06X?}"
    );
    assert!(
        matches!(new_edges[0].1, CallerKey::Routine(_)),
        "its caller is the routine really beneath it, not a frame we declined to track: {:#06X?}",
        new_edges[0]
    );
    let capped: Vec<_> = p
        .sample_callers()
        .keys()
        .filter(|(_, caller)| *caller == CallerKey::DepthCap)
        .collect();
    assert!(
        capped.is_empty(),
        "a frame pushed after a refusal can only be pushed once the stack has unwound, and then its \
         caller is one we tracked: {capped:#06X?}"
    );
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
        stall_cycles: 0,
        executed: true,
        // Supervisor unless a test says otherwise: that is where a 68000 boots, and where every handler
        // and every exception frame lives.
        supervisor: true,
    }
}

/// A step that did **not** execute the instruction at `pc` — an exception entry, an idle slice, or an
/// aborted instruction. Its `opcode` names something the CPU never ran.
fn entry_step(pc: u32, opcode: u16, sp: u32, ssp: u32) -> StepRetire {
    StepRetire {
        executed: false,
        ..step(pc, opcode, sp, ssp)
    }
}

/// What every synthetic step costs, so the totals below are `n * STEP_CYCLES` and the arithmetic is
/// visible rather than magic.
const STEP_CYCLES: u64 = 10;
const OP_NOP: u16 = 0x4E71;
const OP_JSR_ABS_W: u16 = 0x4EB8;
const OP_RTE: u16 = 0x4E73;
const OP_RTS: u16 = 0x4E75;

/// A stack pointer to hang the arithmetic off. Exception frames are six bytes, so the interesting values
/// are `S`, `S - 6` and `S - 12`.
const S: u32 = 0x00FF_FF00;

/// **The not-executed opcode, as a difference.** An exception entry is dispatched before the instruction
/// at `pc` decodes, an idle slice retires a stale `pc`, and an aborted instruction retires its own opcode
/// having done nothing — on all of them `executed` is false and the opcode names something that never ran.
/// Here that something is a `JSR`.
///
/// The two streams below are IDENTICAL except for the acknowledge. With it, the handler's entry gets a row
/// because the acknowledge armed one (a handler is code, and code gets a row). Without it, nothing may open
/// a frame there — the only remaining candidate is the unexecuted `JSR`, and classifying that would arm a
/// call the CPU never made and that will never return.
///
/// Asserting the difference rather than the absence is what keeps this honest: an implementation that
/// simply never opened handler rows would pass an absence test while failing the contract.
#[test]
fn a_handler_row_comes_from_the_acknowledge_and_never_from_the_unexecuted_opcode() {
    const HANDLER: u32 = 0x0000_3000;

    // With the acknowledge: the bucket opens and arms the handler's own row.
    let mut with_ack = Profiler::new();
    with_ack.on_frame_boundary(0);
    with_ack.on_step_retire(step(0x1000, OP_NOP, S, S));
    with_ack.on_event(iack(VINT));
    with_ack.on_step_retire(entry_step(0x2000, OP_JSR_ABS_W, S - 6, S - 6));
    with_ack.on_step_retire(step(HANDLER, OP_NOP, S - 6, S - 6));
    with_ack.on_step_retire(step(HANDLER + 2, OP_RTE, S, S));
    with_ack.on_frame_boundary(1);

    // Without it: the same steps, the same unexecuted `JSR`, no acknowledge.
    let mut without_ack = Profiler::new();
    without_ack.on_frame_boundary(0);
    without_ack.on_step_retire(step(0x1000, OP_NOP, S, S));
    without_ack.on_step_retire(entry_step(0x2000, OP_JSR_ABS_W, S - 6, S - 6));
    without_ack.on_step_retire(step(HANDLER, OP_NOP, S - 6, S - 6));
    without_ack.on_step_retire(step(HANDLER + 2, OP_RTE, S, S));
    without_ack.on_frame_boundary(1);

    assert_eq!(
        with_ack.sample_routines().get(&HANDLER).map(|c| c.calls),
        Some(1),
        "the acknowledge opens the handler's own row, once per entry; rows: {:#06X?}",
        with_ack.sample_routines().keys().collect::<Vec<_>>()
    );
    assert!(
        !without_ack.sample_routines().contains_key(&HANDLER),
        "with no acknowledge the unexecuted JSR must arm NOTHING; rows: {:#06X?}",
        without_ack.sample_routines().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        with_ack.sample_interrupts()[&VINT].calls,
        1,
        "and the interrupt was counted from its acknowledge"
    );
    assert!(
        without_ack.sample_interrupts().is_empty(),
        "no acknowledge, no bucket"
    );
    // And the stack is left in the same shape by both streams — one synthesized root, nothing else.
    // A frame opened and never closed writes no row (a row is recorded when its invocation ends), so the
    // depth is the ONLY place a phantom call is visible at all. Without this, a classified `JSR` that
    // pushed a frame nothing ever returns from would leak silently.
    assert_eq!(
        (with_ack.open_frames(), without_ack.open_frames()),
        (1, 1),
        "neither stream may leave a frame behind: the unexecuted JSR pushed nothing"
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
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6)); // entry: the bucket's frame sits at S-6
    p.on_step_retire(step(0x3000, OP_NOP, S - 6, S - 6)); // handler
    p.on_step_retire(entry_step(0x3002, OP_NOP, S - 12, S - 12)); // a TRAP entry: a second frame, at S-12
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
    //
    // "Closed nothing" is visible in two places at once, and both are needed. The frame is still OPEN —
    // it has a row, because a running routine's cycles are reported as they accrue, but `calls` is zero
    // because nothing completed it. And the stack still holds it.
    assert_eq!(
        p.sample_routines().get(&0x1000).map(|c| c.calls),
        Some(0),
        "the frame the RTE crossed is still running: cycles yes, a completed call no; rows: {:#06X?}",
        p.sample_routines().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        p.open_frames(),
        1,
        "and it is still on the stack — the unmatched RTE popped nothing"
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
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6)); // VInt entry, frame at S-6
    p.on_step_retire(step(0x3000, OP_NOP, S - 6, S - 6)); // VInt handler
    p.on_event(iack(HINT));
    p.on_step_retire(entry_step(0x3002, OP_NOP, S - 12, S - 12)); // HInt entry, frame at S-12
    p.on_step_retire(step(0x5000, OP_NOP, S - 12, S - 12)); // HInt handler
    p.on_step_retire(step(0x5002, OP_RTE, S - 6, S - 6)); // HInt's RTE: S-12 + 6
    p.on_step_retire(step(0x3004, OP_NOP, S - 6, S - 6)); // back in the VInt handler
    p.on_step_retire(step(0x3006, OP_RTE, S, S)); // VInt's RTE: S-6 + 6
    p.on_frame_boundary(1);

    let hint = p.sample_interrupts()[&HINT];
    let vint = p.sample_interrupts()[&VINT];
    assert_eq!((hint.calls, vint.calls), (1, 1), "each taken exactly once");
    // Three steps ran with the HInt open — its entry, its handler, its RTE — so its INCLUSIVE total is 30.
    // Only the entry is the bucket's own time: the acknowledge armed a routine row for the handler, so the
    // handler's two steps are the bucket's CHILD time. That split is the whole point of the additive rule.
    assert_eq!(
        (hint.self_cycles, hint.cycles),
        (STEP_CYCLES, 3 * STEP_CYCLES),
        "the inner bucket: its own entry, plus its handler as a child"
    );
    // The other four are the VInt's, and its inclusive is NOT inflated by the bucket that preempted it —
    // an interrupt is not a callee of the interrupt it interrupted.
    assert_eq!(
        (vint.self_cycles, vint.cycles),
        (STEP_CYCLES, 4 * STEP_CYCLES),
        "the outer bucket accrued nothing while the inner one was open"
    );
    // Both handlers have rows of their own, which is what makes the buckets additive rather than a
    // replacement — a consumer measuring the HBlank routine itself reads this, not the bucket.
    assert_eq!(
        p.sample_routines().get(&0x5000).map(|c| c.calls),
        Some(1),
        "the HBlank handler's own row; rows: {:#06X?}",
        p.sample_routines().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        p.sample_routines().get(&0x3000).map(|c| c.calls),
        Some(1),
        "and the VBlank handler's"
    );
}

// --- C1: attribution under preemption ---------------------------------------------------------------
//
// The witness for `docs/2026-08-19-streaming-asks-recon.md` §2.5. The instrument this replaces loses
// **20.6%** of a frame when a tick straddles a VBlank, because its shadow stack is declared inside a
// per-frame loop (`ControlSocket.cpp:1972`) and cycles are charged only at an exit event
// (`:1986-1990`): a routine entered in frame N and returning in frame N+1 meets a stack that never saw
// its entry, so its whole post-boundary segment reaches no row while still counting toward the total.
//
// Our accumulator charges continuously and never tears the stack down, so the defect is not expressible
// here. That is a claim until something checks it — which is what this fixture is for, and why its M1
// mutation (`self.stack.clear()` at the boundary: a faithful reproduction of the defect, applied to OUR
// accumulator) is the one that must turn it red.

/// How long the witness runs. R starts at line 0 of frame 1 — `main` synchronises on the V counter, so
/// the sample's opening boundary (frame 0, line 224) is already behind it — and R's delay is sized for
/// `testrom::PROF_PREEMPT_DELAY_FRAMES` frames, so a run of this length closes the sample several whole
/// frames after R returned. **Both ends are load-bearing:** R's invocation must lie strictly INSIDE the
/// sample, or the opening boundary would forget part of its accrual and the money assertion below would
/// be comparing two truncations rather than two measurements.
const PREEMPT_FRAMES: u64 = 8;

/// The 68000 function code for a supervisor data access — what a write made on the guest's behalf
/// carries.
const FC_SUPERVISOR_DATA: u8 = 5;

/// One run of the preemption witness. `flag` is the single byte that decides whether R lowers the
/// interrupt mask; the ROM image, the run length and the VDP arming are identical between the two.
fn preemption_run(flag: u8) -> Profiler {
    let mut sys = System::new(0x1234_5678);
    sys.load_rom(testrom::build_profiler(ProfilerShape::Preempted));
    sys.reset();
    // Written through the machine's own bus, after the reset (which does not touch work RAM) and before
    // the first instruction retires. This is what keeps "one ROM, run twice" literal: the two runs load
    // byte-identical images and differ by one byte of RAM.
    let mut sink = ();
    let mut bus = sys.mega_bus(&mut sink);
    bus.write8(PROF_PREEMPT_FLAG, FC_SUPERVISOR_DATA, flag);
    let mut prof = Profiler::new();
    sys.run_frames_with_sink(PREEMPT_FRAMES, &mut prof);
    prof
}

/// **★ C1, in one equality: a routine's own cost does not depend on interrupt load.**
///
/// One ROM, run twice — VBlank live, VBlank masked — and `cyclesSelf` for the preempted routine must be
/// **exactly** equal between them. That is precisely the property the old instrument violated by 20.6%,
/// and it is asserted as an equality rather than a tolerance because the consumers of this surface gate
/// with `==`.
///
/// Everything else here is the scaffolding that keeps the equality from being vacuous:
///
/// - **The liveness control.** Run A must actually have been preempted (a VBlank bucket, more than once)
///   and run B must not have been (no bucket at all). Without it the equality is two identical
///   unpreempted runs agreeing with each other, which is a test that cannot fail.
/// - **The sizing check, derived.** R's own retired cycles must exceed two whole frames — computed from
///   `MCLK_PER_FRAME / MCLK_PER_CPU_CYCLE`, the machine's own constants, never from a passing run —
///   because a single invocation that outlasts two frames of CPU time necessarily spanned at least two
///   frame boundaries, which is what makes the mid-invocation checkpoint fold the thing under test.
/// - **`calls == 1`.** The L3 regression: the old instrument's end-of-frame flush increments `calls`, so
///   one invocation open across *k* boundaries books *k + 1* calls there.
///
/// The mutation record (recon §2.5, each run against this test before it was recorded):
/// **M1** — `self.stack.clear()` in the `else` branch of `on_frame_boundary`, i.e. the per-frame stack —
/// breaks the money assertion, `calls == 1` and the inclusive relation. Its breaking the *money*
/// assertion is the proof that R really does span a boundary; a fixture whose delay were too short would
/// leave it green. **M2** — dropping `pop_frame`'s `FrameKind::Routine` guard so an interrupt folds into
/// its parent — breaks the inclusive relation ALONE and leaves the money assertion green, which is
/// exactly the discrimination §2.5 asks for (a fold moves inclusive time, never self time). **M3** —
/// checkpointing the frame's full accrual instead of `unreported()` — breaks the inclusive relation and
/// the identity.
#[test]
fn a_routines_own_cycles_are_identical_whether_or_not_an_interrupt_preempts_it() {
    let a = preemption_run(PROF_PREEMPT_VINT_LIVE);
    let b = preemption_run(PROF_PREEMPT_VINT_MASKED);
    let a_r = raw(&a, PROF_PREEMPT_R);
    let b_r = raw(&b, PROF_PREEMPT_R);

    // --- The fixture is live: R spans boundaries, and only run A was preempted ---------------------
    let two_frames = 2 * PROF_CPU_CYCLES_PER_FRAME;
    for (tag, r) in [("A (VInt live)", a_r), ("B (VInt masked)", b_r)] {
        assert!(
            r.self_cycles > two_frames,
            "{tag}: R's own retired cycles ({}) must outlast two whole frames ({two_frames}), or its \
             single invocation never spanned two boundaries and everything below is vacuous",
            r.self_cycles
        );
    }
    let a_vint = a
        .sample_interrupts()
        .get(&VINT)
        .copied()
        .unwrap_or_default();
    assert!(
        a_vint.calls >= 2,
        "run A must really have been preempted, more than once, INSIDE R — `main` runs masked and R \
         lowers the mask only between its two child calls, so every interrupt this run took was taken \
         there; buckets: {:?}",
        a.sample_interrupts()
    );
    assert!(
        b.sample_interrupts().is_empty(),
        "run B must NOT have been preempted at all — otherwise the equality below compares two \
         preempted runs and says nothing; buckets: {:?}",
        b.sample_interrupts()
    );

    // --- ★ The money assertion ---------------------------------------------------------------------
    assert_eq!(
        a_r.self_cycles, b_r.self_cycles,
        "R's OWN cycles must not move when {} VBlanks preempt it: {} preempted vs {} not",
        a_vint.calls, a_r.self_cycles, b_r.self_cycles
    );
    // And its inclusive figure too, because this fixture's children are unpreemptible by construction
    // (they run with the mask raised). Two equalities, not one, and they fail for different reasons: the
    // first breaks if preemption leaks into R's own accrual, the second if it leaks into the child fold.
    assert_eq!(
        a_r.cycles, b_r.cycles,
        "and neither does its inclusive figure: {} vs {}",
        a_r.cycles, b_r.cycles
    );

    // --- One invocation, not one per boundary crossed (the L3 regression) --------------------------
    assert_eq!(
        (a_r.calls, b_r.calls),
        (1, 1),
        "R is called exactly once by construction, and a boundary crossed mid-invocation must not \
         fabricate a second call"
    );

    // --- The exact inclusive relation, in BOTH runs ------------------------------------------------
    // The real gate, and an EQUALITY rather than an inequality precisely because of the non-folding
    // rule: preemption is in neither term, so there is no interrupt cost to leave room for.
    for (tag, p, r) in [("A (VInt live)", &a, a_r), ("B (VInt masked)", &b, b_r)] {
        let ca = raw(p, PROF_PREEMPT_CA);
        let cb = raw(p, PROF_PREEMPT_CB);
        assert_eq!(
            (ca.calls, cb.calls),
            (1, 1),
            "{tag}: each child is called once, by R's one invocation"
        );
        assert!(
            ca.cycles > 0 && cb.cycles > 0,
            "{tag}: both children must have cost something or the relation below is vacuous"
        );
        assert_eq!(
            r.cycles,
            r.self_cycles + ca.cycles + cb.cycles,
            "{tag}: R's inclusive time is its own work plus exactly its two children — nothing the \
             interrupt did is in either term (self {}, Ca {}, Cb {}, inclusive {})",
            r.self_cycles,
            ca.cycles,
            cb.cycles,
            r.cycles
        );
        // The direct negation of aeon's impossibility signature: a boundary-spanning parent reported
        // SMALLER than the children that complete inside one frame.
        assert!(
            r.cycles >= ca.cycles + cb.cycles,
            "{tag}: the parent cannot cost less than the children it contains ({} < {} + {})",
            r.cycles,
            ca.cycles,
            cb.cycles
        );
    }

    // --- The reconciliation identity closes in BOTH runs -------------------------------------------
    for (tag, p) in [("A (VInt live)", &a), ("B (VInt masked)", &b)] {
        let report = p.report();
        let rows: u64 = p.sample_routines().values().map(|c| c.self_cycles).sum();
        let buckets: u64 = p.sample_interrupts().values().map(|c| c.self_cycles).sum();
        assert_eq!(
            rows + buckets + report.unattributed_cycles,
            report.sample_cycles,
            "{tag}: rows {rows} + buckets {buckets} + unattributed {} must equal the sample {}",
            report.unattributed_cycles,
            report.sample_cycles
        );
        assert_eq!(
            report.unattributed_cycles, 0,
            "{tag}: nothing may escape into the hatch here — `main` runs masked, so no interrupt can \
             be in flight across the opening boundary and there is nothing to suppress"
        );
        assert_eq!(
            (report.abandoned_frames, report.depth_exceeded),
            (0, 0),
            "{tag}: and no frame was torn off or refused — either would mean some row understates"
        );
    }

    // --- Where the difference went, stated ---------------------------------------------------------
    // R's row is identical across the two runs; run A additionally carries a VBlank bucket and a handler
    // row that run B has no key for at all. So the whole cost of the preemption IS the bucket: it did
    // not come out of R (the 20.6% loss) and it did not go into R (the W5 conflation). Both directions
    // are stated, because our `cycles` differs from the old instrument's in BOTH of them — so a row that
    // happened to agree with it would be evidence of nothing.
    assert!(
        a_vint.cycles > 0,
        "the preemption cost something, and that something is in the bucket"
    );
    let a_handler = raw(&a, PROF_VINT_H);
    assert!(
        a_handler.cycles > 0 && a_handler.cycles <= a_vint.cycles,
        "the handler's own row is part of the bucket's inclusive total, never more ({} vs {})",
        a_handler.cycles,
        a_vint.cycles
    );
    assert!(
        !b.sample_routines().contains_key(&PROF_VINT_H),
        "and run B never entered the handler at all; rows: {:#06X?}",
        b.sample_routines().keys().collect::<Vec<_>>()
    );
    assert!(
        !a.sample_interrupts().contains_key(&HINT),
        "only VBlank was armed, in both runs; buckets: {:?}",
        a.sample_interrupts()
    );
}

// --- The per-frame ring across a boundary the interrupt straddles (Q-PROF-STRADDLE) -----------------
//
// Every fixture above closes its bucket between two boundaries, so nothing here was ever put across one.
// These two streams do exactly that, and they are synthetic for the reason the section above gives: a
// boundary has to land on one chosen instruction, which no ROM can be asked to arrange.
//
// The expectations are counted off the streams themselves — *n* steps at [`STEP_CYCLES`] each — and never
// read back off a run. Each stream states, next to every step, which frame it retires in and whether the
// bucket is open over it, so the two figures below are a transcription of the fixture rather than an
// observation of the code under test.

/// **A VBlank handler that straddles a frame boundary must be charged to the frames it ran in.**
///
/// The handler here calls nothing at all, which is the whole point of this first case: the profiler opens
/// a *routine* frame for the handler's entry address beneath the bucket (the acknowledge arms one), so the
/// bucket's own `self_cycles` is the **exception entry alone** and every cycle the handler retires is
/// already child time. A bucket therefore straddles badly whether or not a callee is in flight — which is
/// the case a "only an in-flight callee is displaced" reading would miss entirely.
///
/// The stream: 4 steps in frame 1 (three of them under the bucket), 3 in frame 2 (two under it).
#[test]
fn a_straddling_vblank_handler_is_charged_to_the_frames_it_ran_in() {
    const HANDLER: u32 = 0x0000_3000;
    let mut p = Profiler::with_per_frame(8);
    p.on_frame_boundary(0); // opens the sample; the span before it is not a frame

    // --- frame 1: 4 steps, of which 3 are under the bucket ---
    p.on_step_retire(step(0x1000, OP_NOP, S, S)); // main
    p.on_event(iack(VINT));
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6)); // entry     — bucket
    p.on_step_retire(step(HANDLER, OP_NOP, S - 6, S - 6)); //      handler   — bucket
    p.on_step_retire(step(HANDLER + 2, OP_NOP, S - 6, S - 6)); //  handler   — bucket
    p.on_frame_boundary(1); // <<< the boundary lands mid-handler

    // --- frame 2: 3 steps, of which 2 are under the bucket ---
    p.on_step_retire(step(HANDLER + 4, OP_NOP, S - 6, S - 6)); //  handler   — bucket
    p.on_step_retire(step(HANDLER + 6, OP_RTE, S, S)); //          rte       — bucket, and closes it
    p.on_step_retire(step(0x1002, OP_NOP, S, S)); //               main
    p.on_frame_boundary(2);

    let rows: Vec<_> = p.per_frame().iter().copied().collect();
    assert_eq!(rows.len(), 2, "two counted frames: {rows:?}");
    assert_eq!(
        (rows[0].cycles, rows[1].cycles),
        (4 * STEP_CYCLES, 3 * STEP_CYCLES),
        "the frames themselves are 4 steps and 3 steps: {rows:?}"
    );
    assert_eq!(
        (rows[0].vint_cycles, rows[1].vint_cycles),
        (3 * STEP_CYCLES, 2 * STEP_CYCLES),
        "the bucket ran 3 steps in frame 1 and 2 in frame 2, and each frame is charged its own: {rows:?}"
    );
    // The ring's own stated invariant, which a displaced row breaks: an interrupt is part of a frame.
    for r in &rows {
        assert!(
            r.vint_cycles <= r.cycles,
            "a bucket cannot cost more than the frame that contains it: {r:?}"
        );
    }
    // Displacement, not loss: the ring still decomposes the sample exactly.
    let ring_vint: u64 = rows.iter().map(|r| r.vint_cycles).sum();
    assert_eq!(
        ring_vint,
        p.sample_interrupts()[&VINT].cycles,
        "the per-frame bucket figures sum to the undivided bucket — redistributed, never invented"
    );
    let ring_cycles: u64 = rows.iter().map(|r| r.cycles).sum();
    assert_eq!(
        ring_cycles,
        p.report().sample_cycles,
        "and the rows are still the sample"
    );
}

/// **The §5 shape: a callee still open beneath the handler when the boundary lands.** The handler `jsr`s,
/// the boundary falls inside the callee, the callee `rts`es and only then does the `RTE` arrive — so the
/// displaced span is a whole subroutine's lifetime rather than the handler's own retirement.
///
/// The stream: 6 steps in frame 1 (five under the bucket), 5 in frame 2 (four under it).
#[test]
fn a_callee_straddling_a_boundary_beneath_a_handler_is_charged_to_the_frames_it_ran_in() {
    const HANDLER: u32 = 0x0000_3000;
    const CALLEE: u32 = 0x0000_4000;
    let mut p = Profiler::with_per_frame(8);
    p.on_frame_boundary(0);

    // --- frame 1: 6 steps, of which 5 are under the bucket ---
    p.on_step_retire(step(0x1000, OP_NOP, S, S)); // main
    p.on_event(iack(VINT));
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6)); // entry   — bucket
    p.on_step_retire(step(HANDLER, OP_NOP, S - 6, S - 6)); //      handler — bucket
    p.on_step_retire(step(HANDLER + 2, OP_JSR_ABS_W, S - 10, S - 10)); // jsr, pushing 4 — bucket
    p.on_step_retire(step(CALLEE, OP_NOP, S - 10, S - 10)); //     callee  — bucket
    p.on_step_retire(step(CALLEE + 2, OP_NOP, S - 10, S - 10)); // callee  — bucket
    p.on_frame_boundary(1); // <<< the boundary lands inside the callee

    // --- frame 2: 5 steps, of which 4 are under the bucket ---
    p.on_step_retire(step(CALLEE + 4, OP_NOP, S - 10, S - 10)); // callee  — bucket
    p.on_step_retire(step(CALLEE + 6, OP_RTS, S - 6, S - 6)); //   rts: S-10 + 4 closes the callee
    p.on_step_retire(step(HANDLER + 4, OP_NOP, S - 6, S - 6)); //  handler — bucket
    p.on_step_retire(step(HANDLER + 6, OP_RTE, S, S)); //          rte: S-6 + 6 closes the bucket
    p.on_step_retire(step(0x1002, OP_NOP, S, S)); //               main
    p.on_frame_boundary(2);

    // The callee really did straddle: one invocation, four steps, spanning both frames.
    let callee = p.sample_routines()[&CALLEE];
    assert_eq!(
        (callee.calls, callee.cycles),
        (1, 4 * STEP_CYCLES),
        "one call, four steps — the straddle is the fixture, not an accident of matching"
    );

    let rows: Vec<_> = p.per_frame().iter().copied().collect();
    assert_eq!(rows.len(), 2, "two counted frames: {rows:?}");
    assert_eq!(
        (rows[0].cycles, rows[1].cycles),
        (6 * STEP_CYCLES, 5 * STEP_CYCLES),
        "the frames themselves are 6 steps and 5 steps: {rows:?}"
    );
    assert_eq!(
        (rows[0].vint_cycles, rows[1].vint_cycles),
        (5 * STEP_CYCLES, 4 * STEP_CYCLES),
        "frame 1 keeps the two callee steps it retired; frame 2 gets only its own four: {rows:?}"
    );
    for r in &rows {
        assert!(
            r.vint_cycles <= r.cycles,
            "a bucket cannot cost more than the frame that contains it: {r:?}"
        );
    }
    let ring_vint: u64 = rows.iter().map(|r| r.vint_cycles).sum();
    assert_eq!(
        ring_vint,
        p.sample_interrupts()[&VINT].cycles,
        "the per-frame bucket figures sum to the undivided bucket — redistributed, never invented"
    );
    assert_eq!(
        p.per_frame().iter().map(|r| r.hint_cycles).sum::<u64>(),
        0,
        "no HBlank was acknowledged anywhere in this stream"
    );
}

/// **Nesting survives the split.** An HBlank taken inside a VBlank handler, with the boundary landing
/// inside the HBlank's own handler: the inner bucket takes every cycle beneath it and the outer takes
/// none of them, in **both** frames. An interrupt is not a callee of the interrupt it preempted, so the
/// two figures partition rather than nest — which is what makes `hint_cycles + vint_cycles <= cycles` a
/// property of the split rather than a coincidence of this fixture.
///
/// The stream is symmetric on purpose: 5 steps per frame, 2 under each bucket in each frame. A rule that
/// charged the inner bucket's time to the outer as well would double one of the columns.
#[test]
fn a_nested_hint_straddling_a_boundary_is_charged_to_the_inner_bucket_alone() {
    const VINT_H: u32 = 0x0000_3000;
    const HINT_H: u32 = 0x0000_5000;
    let mut p = Profiler::with_per_frame(8);
    p.on_frame_boundary(0);

    // --- frame 1: 5 steps — 1 main, 2 under the VInt, 2 under the HInt ---
    p.on_step_retire(step(0x1000, OP_NOP, S, S)); // main
    p.on_event(iack(VINT));
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6)); // VInt entry     — vint
    p.on_step_retire(step(VINT_H, OP_NOP, S - 6, S - 6)); //       VInt handler   — vint
    p.on_event(iack(HINT));
    p.on_step_retire(entry_step(VINT_H + 2, OP_NOP, S - 12, S - 12)); // HInt entry   — hint
    p.on_step_retire(step(HINT_H, OP_NOP, S - 12, S - 12)); //         HInt handler — hint
    p.on_frame_boundary(1); // <<< the boundary lands inside the INNER handler

    // --- frame 2: 5 steps — 2 under the HInt, 2 under the VInt, 1 main ---
    p.on_step_retire(step(HINT_H + 2, OP_NOP, S - 12, S - 12)); //  HInt handler — hint
    p.on_step_retire(step(HINT_H + 4, OP_RTE, S - 6, S - 6)); //    HInt rte: S-12 + 6 closes it
    p.on_step_retire(step(VINT_H + 4, OP_NOP, S - 6, S - 6)); //    VInt handler — vint
    p.on_step_retire(step(VINT_H + 6, OP_RTE, S, S)); //            VInt rte: S-6 + 6 closes it
    p.on_step_retire(step(0x1002, OP_NOP, S, S)); //                main
    p.on_frame_boundary(2);

    let rows: Vec<_> = p.per_frame().iter().copied().collect();
    assert_eq!(rows.len(), 2, "two counted frames: {rows:?}");
    for r in &rows {
        assert_eq!(
            (r.cycles, r.hint_cycles, r.vint_cycles),
            (5 * STEP_CYCLES, 2 * STEP_CYCLES, 2 * STEP_CYCLES),
            "each frame ran 5 steps, 2 under each bucket: {rows:?}"
        );
        assert!(
            r.hint_cycles + r.vint_cycles <= r.cycles,
            "the two causes partition part of the frame; they cannot overrun it: {r:?}"
        );
    }
    // And the halves still sum to the undivided buckets, which the nesting rule keeps disjoint.
    assert_eq!(
        (
            rows.iter().map(|r| r.hint_cycles).sum::<u64>(),
            rows.iter().map(|r| r.vint_cycles).sum::<u64>()
        ),
        (
            p.sample_interrupts()[&HINT].cycles,
            p.sample_interrupts()[&VINT].cycles
        ),
        "each column sums to its own bucket — redistributed, never invented and never shared"
    );
}

/// **The ring inherits the retroactive-entry rule, and must.** A bucket already open when the sample
/// opened is suppressed: its acknowledge was never observed, so the contract forbids opening it. The
/// frames *beneath* it are not suppressed — a handler that ran inside the sample really did run — so the
/// per-frame accumulator has to check the bucket rather than the frame it is charging, or it would credit
/// a bucket the sample is not allowed to have.
///
/// The handler's own row is asserted alongside, because "the ring shows nothing" would also be satisfied
/// by an accountant that had simply stopped counting.
#[test]
fn the_ring_credits_no_bucket_for_an_interrupt_the_sample_never_saw_entered() {
    const HANDLER: u32 = 0x0000_3000;
    let mut p = Profiler::with_per_frame(8);
    // Before the sample: the interrupt is taken and its handler starts running.
    p.on_event(iack(VINT));
    p.on_step_retire(entry_step(0x2000, OP_NOP, S - 6, S - 6));
    p.on_step_retire(step(HANDLER, OP_NOP, S - 6, S - 6));
    p.on_frame_boundary(0); // the sample opens with that handler still on the stack

    // --- frame 1: 1 step, inside the suppressed handler ---
    p.on_step_retire(step(HANDLER + 2, OP_NOP, S - 6, S - 6));
    p.on_frame_boundary(1);

    // --- frame 2: 2 steps, one of them the `rte` that finishes it inside the sample ---
    p.on_step_retire(step(HANDLER + 4, OP_RTE, S, S));
    p.on_step_retire(step(0x1000, OP_NOP, S, S));
    p.on_frame_boundary(2);

    let rows: Vec<_> = p.per_frame().iter().copied().collect();
    assert_eq!(
        rows.iter().map(|r| (r.cycles, r.hint_cycles, r.vint_cycles)).collect::<Vec<_>>(),
        vec![(STEP_CYCLES, 0, 0), (2 * STEP_CYCLES, 0, 0)],
        "the frames ran, and not one of their cycles may be credited to a bucket the sample never saw \
         entered: {rows:?}"
    );
    assert!(
        p.sample_interrupts().is_empty(),
        "the aggregate says the same thing, which is where this rule was already pinned: {:?}",
        p.sample_interrupts()
    );
    assert_eq!(
        p.sample_routines().get(&HANDLER).map(|c| c.self_cycles),
        Some(2 * STEP_CYCLES),
        "but the handler's own code still gets its row for the two steps it retired in the sample; \
         rows: {:#06X?}",
        p.sample_routines().keys().collect::<Vec<_>>()
    );
}
