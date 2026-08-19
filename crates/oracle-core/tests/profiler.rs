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
    self, ProfilerShape, StallKind, PROF_DISPATCH, PROF_LEAF, PROF_MID, PROF_MID_CALLS_LEAF,
    PROF_REC, PROF_STALL, PROF_TARGET, PROF_VINT_H,
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
