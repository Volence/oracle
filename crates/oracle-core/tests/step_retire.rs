//! The retire hook: `BusEventSink::on_step_retire`, the counterpart of `on_step_boundary`.
//!
//! The boundary stamp says *which* instruction is about to run; the retirement says what it **cost**. That
//! cost — `step_cpu`'s return value, the exact CPU-cycle count the master clock is advanced by — is computed
//! inside the run loop, used once, and until this hook existed was dropped on the floor. Everything a
//! per-routine cycle accountant needs is downstream of it.
//!
//! This file is the neutrality slice's evidence, and it is deliberately the *empty* hook's evidence: there is
//! no profiler yet. The claim under test is that the hook exists, carries true values, and that a machine run
//! with one attached is **the same machine** as one run without — not merely the same state hash.

use oracle_core::bus::{BusEventSink, Fanout, StepRetire, StopWhen};
use oracle_core::system::{System, MCLK_PER_CPU_CYCLE};

/// A booted machine: the built-in test ROM loaded and the power-on reset driven (the `scanline_capture.rs`
/// idiom).
fn booted(seed: u64) -> System {
    boot(oracle_core::testrom::build(), seed)
}

/// Boot an arbitrary image.
fn boot(rom: Vec<u8>, seed: u64) -> System {
    let mut s = System::new(seed);
    s.load_rom(rom);
    s.reset();
    s
}

/// Run `rom` for `frames` frames and hand back every retirement.
fn retires_of(rom: Vec<u8>, frames: u64) -> Vec<StepRetire> {
    let mut s = boot(rom, 0x1234_5678);
    let mut sink = RetireLog::default();
    s.run_frames_with_sink(frames, &mut sink);
    sink.retires
}

/// Records both halves of a step: the boundary stamp and the retirement. Keeping both in one sink is what
/// lets the pairing be asserted (one retirement per step that actually ran, same PC, same order) rather than
/// taken on trust.
#[derive(Default)]
struct RetireLog {
    boundaries: Vec<u32>,
    retires: Vec<StepRetire>,
}

impl BusEventSink for RetireLog {
    fn on_event(&mut self, _event: oracle_core::bus::BusEvent) {}
    fn on_step_boundary(&mut self, pc: u32, _frame: u64) {
        self.boundaries.push(pc);
    }
    fn on_step_retire(&mut self, retire: StepRetire) {
        self.retires.push(retire);
    }
}

/// **The neutrality gate** — the four-assertion shape `scanline_capture.rs::frame_boundary_is_state_neutral`
/// established, applied to the retire hook. Clause 3 (liveness) is not optional: without it, deleting the
/// `sink.on_step_retire(...)` call from the run loop outright would leave this test green, because it would
/// then be comparing two runs of the same never-firing hook.
#[test]
fn retire_hook_is_state_neutral() {
    let mut plain = booted(9);
    let mut tapped = booted(9);
    plain.run_frames(3);
    let mut sink = RetireLog::default();
    tapped.run_frames_with_sink(3, &mut sink);

    // 1. The currency.
    assert_eq!(
        plain.export_state_hash(),
        tapped.export_state_hash(),
        "the retire hook must not move the machine"
    );
    // 2. The whole machine, not just the hash.
    assert_eq!(
        plain, tapped,
        "the WHOLE machine is identical, not just the hash"
    );
    // 3. The liveness control — the hook demonstrably fired.
    assert!(
        !sink.retires.is_empty(),
        "the sink did observe retirements — a hook that never fires cannot pass this test"
    );
    // 4. Structural: the retirements account for the ENTIRE clock. `scheduler.advance(cycles × 7)` at the
    //    single conversion site is the only thing that moves mclk, and the loop calls it once per step with
    //    the very `cycles` handed to this hook — so the sum is not "close to" the elapsed time, it IS the
    //    elapsed time. A hook that reported a plausible-looking but wrong cost fails here.
    //
    //    **This identity survived the stall slice unchanged, deliberately.** `stall_cycles` is a SUBSET of
    //    `cycles` — the part of the same number the CPU spent held off the bus, not an addition to it — so
    //    the clock still equals the sum of `cycles` alone. Had stall been threaded as a separate quantity
    //    beside the cost, this assertion would have started failing, and that failure would have been the
    //    correct alarm rather than a test to relax.
    let retired: u64 = sink.retires.iter().map(|r| u64::from(r.cycles)).sum();
    assert_eq!(
        retired * MCLK_PER_CPU_CYCLE,
        tapped.scheduler().now(),
        "the retired cycles are the clock: {} steps summing to {retired} CPU cycles",
        sink.retires.len()
    );
}

/// Every field, pinned against an independently driven twin. `step_instruction` is the same `step_cpu` the
/// run loop calls, reachable without the loop — so the twin's own before/after registers and its returned
/// cycle count are a source for the expectation that does not pass through the hook being tested.
#[test]
fn the_retired_fields_are_the_real_step() {
    let mut twin = booted(0x1234_5678);
    let expected = {
        let pc = twin.cpu_regs().pc;
        let opcode = twin.cpu_regs().prefetch[0];
        let cycles = twin.step_instruction();
        StepRetire {
            pc,
            opcode,
            sp: twin.cpu_regs().a7(),
            ssp: twin.cpu_regs().ssp,
            cycles,
            // The boot ROM's first instruction is `move.w #imm,SR` — it touches no VDP port, so it cannot
            // have been held off the bus. `step_instruction` does not report a stall of its own, so this
            // is pinned from what the instruction IS rather than from a measurement.
            stall_cycles: 0,
            // The first instruction runs: no exception is pending at the reset anchor.
            executed: true,
            // ...and it is a real instruction, not a `Stopped`/`Halted` idle slice: the CPU boots
            // `Normal`, and nothing before this anchor could have executed a `STOP`.
            idle: false,
            // The 68000 boots supervisor, and the ROM's first instruction has not left it.
            supervisor: twin.cpu_regs().supervisor(),
        }
    };

    let mut s = booted(0x1234_5678);
    let mut sink = RetireLog::default();
    s.run_frames_with_sink(1, &mut sink);

    assert_eq!(
        sink.retires[0], expected,
        "the first retirement is the first step: its pre-step PC and opcode, its post-step A7, its cost"
    );
    assert_ne!(expected.cycles, 0, "and that cost is a real number");
}

/// One retirement per step that ran, same PC, same order. This is what makes the two hooks a *pair*: a
/// consumer may latch context at the boundary and settle it at the retirement without keeping a queue.
#[test]
fn each_boundary_is_followed_by_exactly_one_retirement_with_the_same_pc() {
    let mut s = booted(0x1234_5678);
    let mut sink = RetireLog::default();
    s.run_frames_with_sink(1, &mut sink);

    assert_eq!(
        sink.retires.len(),
        sink.boundaries.len(),
        "no run-loop step is stamped without retiring, or retires without being stamped"
    );
    let retired_pcs: Vec<u32> = sink.retires.iter().map(|r| r.pc).collect();
    assert_eq!(
        retired_pcs, sink.boundaries,
        "the retirement reports the PC its own boundary stamped, in order"
    );
    // The opcode is read live, not latched once: a real run passes through more than one instruction.
    let mut distinct: Vec<u16> = sink.retires.iter().map(|r| r.opcode).collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() > 1,
        "the opcode field varies across a run (saw {} distinct)",
        distinct.len()
    );
}

/// The documented ordering edge: `stop_requested` is asked *between* the stamp and the step, so the step it
/// cancels is stamped but never runs — and therefore never retires. The counts differing by exactly one is
/// the observable form of "the retirement happens after the step commits".
#[test]
fn the_step_cancelled_by_an_early_stop_never_retires() {
    let mut s = booted(0x1234_5678);
    let mut log = RetireLog::default();
    let mut steps = 0usize;
    {
        // Stop at the 20th boundary — a PC-independent trigger, so this does not depend on what the ROM does.
        let stopper = StopWhen::new(|_pc, _frame| {
            steps += 1;
            steps >= 20
        });
        let mut sink = Fanout::new(&mut log, stopper);
        s.run_frames_with_sink(1, &mut sink);
    }
    assert_eq!(log.boundaries.len(), 20, "the run ended at the 20th stamp");
    assert_eq!(
        log.retires.len(),
        19,
        "the stamped-but-cancelled step retired nothing — the retirement follows the commit"
    );
}

/// The subset relation, per step rather than in aggregate: no step can wait longer than it took. This is
/// what makes `cycles - stall_cycles` meaningful at every level the figure is reported at, and it is the
/// property that keeps the clock identity above true.
///
/// Run on a ROM that ACTUALLY STALLS — a 68k→VDP DMA holds the bus — because a subset assertion over a
/// stall-free run says only that `0 <= cycles`, which is true of any implementation including one that
/// never reports a stall at all. The stall-free ROM is kept as the other half of the pair: together they
/// say the figure appears when it should and not when it shouldn't.
#[test]
fn the_stall_figure_is_a_subset_of_the_cycle_figure() {
    let stalling = retires_of(
        oracle_core::testrom::build_profiler(oracle_core::testrom::ProfilerShape::Stall {
            kind: oracle_core::testrom::StallKind::Dma,
        }),
        3,
    );
    let stalled: u64 = stalling.iter().map(|r| u64::from(r.stall_cycles)).sum();
    assert!(
        stalled > 0,
        "the DMA fixture must actually stall, or the subset check below is vacuous"
    );
    for r in &stalling {
        assert!(
            r.stall_cycles <= r.cycles,
            "a step cannot spend more time waiting than it took: {r:?}"
        );
    }

    // The other half: the built-in ROM only stirs work RAM and never touches a VDP port, so nothing can
    // hold it off. A non-zero total there would mean the accumulator was picking up something that is not
    // one of the three enumerated conditions.
    let quiet = retires_of(oracle_core::testrom::build(), 2);
    let quiet_stall: u32 = quiet.iter().map(|r| r.stall_cycles).sum();
    assert_eq!(
        quiet_stall, 0,
        "the stirring ROM drives no VDP access, so it has nothing to be held off by"
    );
}

/// **`executed` is wired to the machine, not asserted about it.** A ROM that only stirs work RAM takes no
/// exceptions and never stops, so every step runs the instruction at its own `pc`. Turn VBlank interrupts
/// on and entries start appearing — steps that cost cycles while executing nothing.
///
/// The difference is the test: a flag hardcoded either way fails one half of it. This matters because the
/// consumer that reads the flag (a call graph) has no other way to tell an entry from an instruction, and
/// the opcode it would otherwise trust is, on exactly these steps, one that never ran.
#[test]
fn an_exception_entry_reports_that_it_did_not_execute() {
    let quiet = retires_of(oracle_core::testrom::build(), 2);
    assert!(
        !quiet.is_empty() && quiet.iter().all(|r| r.executed),
        "a ROM that takes no exceptions executes every step it retires ({} of {} did not)",
        quiet.iter().filter(|r| !r.executed).count(),
        quiet.len()
    );

    let interrupted = retires_of(
        oracle_core::testrom::build_profiler(oracle_core::testrom::ProfilerShape::Interrupts {
            hint: false,
            vint: true,
        }),
        3,
    );
    let entries = interrupted.iter().filter(|r| !r.executed).count();
    assert!(
        entries >= 2,
        "one VBlank entry per frame at least, and each retires having executed nothing (saw {entries})"
    );
    assert!(
        interrupted.iter().any(|r| r.executed),
        "and the ordinary instructions in between still report that they ran"
    );
}
