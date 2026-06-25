# Decision brief: 68000 cycle granularity

**Status: RATIFIED 2026-06-24.** (The one open Phase-0 decision from `docs/foundations.md`.)

Ratified choices:
1. **Single-definition hybrid** — each opcode written once as a resumable micro-op sequence; a
   run-to-completion driver is the default fast path, a step-one-micro-op driver provides on-demand quiesce.
2. **Default quiesce granularity = bus-access (transaction)** for the 68000. Per-master-clock is reserved
   for the VDP and as a deferred 68k accuracy option behind the same micro-op framework.

## The question

oracle-next wants "every cycle a valid 68k break/snapshot point," but jgenesis's 68k (the architecture
proof) is *instruction*-stepped. A truly cycle-stepped 68000 is more code and slower; a hybrid
(instruction fast-path + on-demand FSM-quiesce) adds a two-path correctness surface that must agree.
The charter said: resolve empirically — prototype one opcode both ways in Phase 0. This brief reports
that prototype.

## What was built and validated

`ADD.w Dn,(An)` (the recommended prototype opcode: fetch + memory-operand read + ALU + write-back),
implemented **two ways** over the same FC-aware bus (`crates/oracle-core/src/m68000/prototype.rs`):

- **instruction-stepped** (`step_instruction`) — the whole opcode executes atomically.
- **cycle-stepped FSM** (`AddWFsm`) — one `tick` = one master cycle; the bus access for each phase
  fires on the phase's final cycle, so every cycle boundary is coherent. The FSM is
  bincode-serializable, so the machine can be snapshot/restored *mid-instruction*.

Evidence (all green in CI):

- **Both paths are byte-identical to real hardware traces.** 179 clean `ADD.w Dn,(An)` cases from the
  pinned SingleStepTests suite pass through *both* paths — final regs/SR/RAM/prefetch, the cycle count,
  **and** the per-cycle bus-transaction stream (`tests/singlestep_m68000.rs`).
- **Both paths agree with each other** (same final state + same transaction stream).
- **The FSM is quiescable at every cycle.** Snapshot → restore at each of the 12 cycle boundaries
  yields an identical final result (`fsm_is_quiescable_and_serializable_at_every_cycle`).

## Empirical cost (measured, `examples/cycle_granularity_perf.rs`, release build)

| Model | Speed | Code |
|---|---|---|
| Instruction-stepped | **2.77 ns/op** (~361 M ops/s) | `step_instruction` body ≈ 17 lines |
| Cycle-stepped FSM (per-master-clock) | **8.34 ns/op** (~120 M ops/s) | `tick` ≈ 38 lines + struct/`Phase`/drivers |
| **Ratio** | **≈ 3.0× slower** | **≈ 2.5× more code per opcode** |

(The per-master-clock FSM is the *finest* granularity and thus the worst case; a per-bus-access FSM —
3 steps instead of 12 — would sit between the two.)

## The three granularities

- **A — instruction-stepped.** Break at instruction boundaries only. Fastest, least code (jgenesis).
- **B — bus-access (transaction) stepped.** Break between memory accesses. Watchpoint-precise (a
  watchpoint fires *on* an access; you almost never need to stop *inside* a 4-clock access). Moderate.
- **C — sub-access (per-master-clock) stepped.** Break at every clock. Needed only for sub-access timing
  precision (FIFO/DMA dot-accuracy) — which the charter explicitly defers to the Phase-3 accuracy tail
  ("MVP-debuggable, NOT passes VDPFIFOTesting").

## The key finding

The instruction-stepped path and the FSM in the prototype **share their decode and ALU** (`decode_add_w_dn_an`,
`add_w_flags`); they differ *only* in the driver loop that sequences the same bus accesses. So the
feared "two-path correctness surface" collapses if each opcode is written **once** as a *resumable
sequence of micro-ops* (bus access / internal step), with two drivers over that one definition:

- a **run-to-completion** driver (the instruction-stepped fast path — inlines to near-A speed), and
- a **step-one-micro-op** driver (the quiescable path — break/snapshot at each micro-op).

One definition ⇒ the two paths *cannot* diverge (no duplicated per-opcode logic to keep in sync), and
the granularity becomes a *driver choice*, not a reimplementation.

## Recommendation

Adopt the **hybrid via single-definition micro-op sequences**, matching foundations' "68k
instruction-stepped fast path but quiescable":

1. Write each 68000 opcode **once** as a micro-op sequence (shared decode + ALU + ordered accesses).
2. Default execution uses the **run-to-completion** driver — keeps ~3× the FSM's throughput, protecting
   the headless/N-instance perf budget.
3. On any debugger touch, **quiesce to bus-access (granularity B)** via the step-one-micro-op driver —
   BlastEm-style sync-on-demand with a catch-up window. This gives break/snapshot/watchpoint precision
   at every memory access, which covers agent-debugging needs for MVP.
4. **Reserve per-master-clock (granularity C)** for the VDP (where dot/FIFO timing is the product) and
   as a *deferred* 68k accuracy option behind the same micro-op framework — not built now.

Rationale: the prototype shows the hybrid is correct (179 cases, byte-identical both ways) and that the
single-definition structure removes its only real risk (path divergence), while the 3× perf gap makes a
full always-on cycle-stepped 68k an unjustified tax for MVP-debuggable.

## What ratification decides / next steps

- **If accepted:** the next 68000 push builds the micro-op opcode framework (run-to-completion +
  step-one-micro-op drivers) and grinds full opcode coverage against the full SingleStepTests suite;
  the prototype's `step_instruction`/`AddWFsm` are replaced by that framework. The FC-aware `Bus68k` is
  unified with the generic `crate::bus::Bus` (add FC to `BusEvent`).
- **Open sub-questions for the owner:** (a) is bus-access (B) the right *default* quiesce granularity, or
  do you want per-clock (C) available for the 68k from the start? (b) accept the one-definition/two-driver
  structure, or prefer a single always-cycle-stepped driver (simpler code, ~3× slower)?

**Ratified 2026-06-24** (single-definition hybrid + bus-access default quiesce). Phase 0 ends here; the
next push builds the micro-op opcode framework and full coverage per this decision.

---

## Post-implementation addendum (2026-06-24, framework push 1)

The micro-op framework was built (`m68000::microop` + `m68000::decode`) and validated: `ADD.w` in two
forms (`Dn,(An)` memory-dest + `<ea>,Dn` register-dest for Dn / (An) / #imm) passes **810** SingleStepTests
cases through **both drivers**, which agree on regs/SR/RAM/prefetch/cycles **and** the per-cycle transaction
stream, with snapshot/restore proven at every bus-access boundary. **Correctness, the single-definition
no-divergence property, and serializable mid-instruction quiesce all hold as designed.**

**But the perf premise did not.** Re-measuring with the *actual* framework (not the prototype's hand-written
paths), ratios stable across runs (`examples/cycle_granularity_perf.rs`):

| Model | Speed vs atomic baseline |
|---|---|
| 1. instruction-stepped (hand-written atomic `step_instruction`) | **1.0×** (baseline) |
| 2. **framework run-to-completion** (the intended default fast path) | **≈ 5.4×** slower |
| 3. framework step-one-micro-op (quiesce path) | ≈ 6× slower |
| 4. per-master-clock FSM (hand-written `AddWFsm`) | ≈ 3× slower |

The framework's "fast path" (2) is **~1.8× slower than even an always-on hand-written FSM** (4) — the
opposite of this brief's premise that "run-to-completion inlines to near-[atomic] speed / keeps ~3× the
FSM's throughput."

**Why the premise was wrong:** the prototype measured a *hand-written atomic* (`step_instruction`, 1×)
against a *hand-written FSM* (`AddWFsm`, 3×) — **both hard-code their operands**. Neither measured the
*generic micro-op interpreter*, which is the actual single-definition mechanism. That interpreter resolves
every operand symbolically at runtime (`match Operand`/`match Dest` per access) — irreducible indirection
the hand-written paths skip. `#[inline]` fixed cross-CU call overhead (helped the step path) but not this.

**This is not a correctness or design-soundness problem** — it is purely the throughput of the *data-driven
representation*. In absolute terms the interpreter still does ~45 M ADD.w/s (~60× real-time), fine for
MVP-debuggable and for differential testing (BlastEm, the accuracy oracle, runs ~50× real-time and is the
bottleneck). It is **not** best-in-class on the hot loop.

**The fix is the planned escape hatch (C — macro-inlined fast path), and it is already proven to work:**
`step_instruction` (the 1× baseline) *is* an example of the straight-line, operand-hard-coded code a macro
would generate per opcode from the same recipe. So a macro that emits an inlined run-to-completion body
from each recipe — while still emitting the data recipe for the quiesce path — recovers **baseline** RTC
speed with **zero** change to the recipes (the design's whole point). The interpreter stays as the
quiesce-path executor.

**Recommendation (owner's call at the push-1 checkpoint):** keep the interpreter (correct, clean, MVP-fast)
and grind full opcode coverage on it now; add the macro-inlined RTC fast path **later** as a dedicated perf
pass, once the micro-op vocabulary has stabilized across many opcode families (codegen over a moving target
is premature). Deferring C costs nothing architecturally. The alternative — build C now, before coverage —
locks in top-tier hot-loop perf from day one at the cost of carrying macro machinery through the grind.
