# oracle-next — 68000 micro-op framework (Push 1: framework + shape checkpoint)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:test-driven-development for every production
> change. Each behaviour gets a failing test *first*. Steps use checkbox (`- [ ]`) syntax for tracking.

Implements the ratified decision `docs/decisions/2026-06-24-cycle-granularity.md`: the **single-definition
hybrid** — each opcode written **once** as a resumable micro-op sequence, with a run-to-completion driver
(default fast path) and a step-one-micro-op driver (on-demand quiesce), default quiesce granularity =
**bus-access (transaction)**.

**Goal:** Stand up the micro-op opcode framework (the two drivers over one definition) and prove the shape
*generalizes* by re-expressing the **`ADD.w` family in two structurally-different forms** — `ADD.w Dn,(An)`
(memory destination, the prototype opcode) **and** `ADD.w <ea>,Dn` (register destination; source EA covers a
bus mode and a no-bus mode) — passing the vendored SingleStepTests through **both** drivers, with the drivers
agreeing and the in-flight state quiescable + bincode-serializable at every bus-access boundary. Then
**PAUSE** for a shape checkpoint before grinding all opcodes.

---

## Framework shape (decided 2026-06-24 — owner delegated the technical call)

**Representation: declarative micro-op recipe + one shared interpreter.** Each opcode's decode yields a small,
ordered, serializable sequence of `MicroOp`s; one shared interpreter performs a single `MicroOp`. The two
drivers are two loops over that *same* data, so the fast path and the quiesce path **cannot diverge** (no
per-opcode logic duplicated to keep in sync). The in-flight cursor is small fixed state — trivially
bincode-serializable, so the machine can snapshot/restore *mid-instruction* (the property that makes Oracle's
"break/snapshot anywhere" work *serializably*, where ares/Exodus rely on un-serializable coroutine/thread
stacks).

**Rejected alternatives:**
- **`async`/coroutine per opcode** — one flat definition, but the compiler-generated future is **not
  bincode-serializable mid-`await`**; breaks mid-instruction snapshot and the everything-is-bincode invariant.
- **Replay-to-cut-point** (fast monomorphized RTC + a stepping driver that re-runs to the Nth micro-op) —
  best perf, but replaying side-effecting reads (e.g. VDP data-port auto-increment) is unsafe without
  memoizing every read, which silently re-introduces the two-path divergence surface the decision exists to
  remove.
- **Per-opcode hand-written FSM** (generalize the prototype's `AddWFsm`) — most code per opcode and the
  highest fast-vs-quiesce divergence risk.

**Reserved optimization (not built now):** a macro that emits an *inlined* run-to-completion body from the
*same* recipe source, recovering near-instruction-stepped throughput **without** touching the recipes —
applied surgically *only if* the measured RTC number threatens the headless/N-instance perf budget. Measure
before optimizing.

---

## Scope of THIS push

**Step 1 only** (per the owner's sequencing): the framework + the `ADD.w` two-form proof + a perf
measurement, then the checkpoint. Explicitly deferred to **after** the checkpoint:
- **Step 2:** unify the FC-aware `Bus68k` with the generic `crate::bus::Bus` (add function code to
  `BusEvent`); retire `StubCpu`; wire the real `Cpu68000` into `System`.
- **Step 3:** grind full 68000 coverage against the complete SingleStepTests suite (extend
  `tools/fetch-tests.sh` + `tools/singlesteptests.sha256` to all mnemonic files; all EA modes; exceptions;
  address-error + A7 cases currently xfail'd; per-cycle bus-transaction gate + versioned xfail manifest).

This push stays on the existing word-granular, FC-aware **`Bus68k`** (`prototype.rs:40`) so the framework
shape is settled before the bus-unification refactor.

---

## The micro-op model (design)

- **`MicroOp`** — one resumable step. Bus-access steps emit a `Transaction` (the existing
  `prototype::Transaction`); compute/idle steps carry a cycle cost. Initial vocabulary (extended as coverage
  grows):
  - `ReadWord { addr, fc, dst }` — operand / extension-word read → scratch slot.
  - `WriteWord { addr, fc, src }` — result / stack write (deferred-seam aware once unified in Step 2).
  - `Prefetch` — refill the prefetch queue (read at `pc+4`), advance queue + `pc`.
  - `Internal { cycles }` — compute / idle (`n`) cycles, no bus access.
  - `Alu { op, .. }` — compute into scratch + set CCR (reuses `add_w_flags`); may fold into an `Internal`.
- **`MicroState`** — the serializable in-flight cursor: the decoded opcode identity + a `step` index + a small
  fixed `scratch` (in-flight operands/addresses/result). `None` between instructions; `Some` while quiesced.
  `bincode::Encode`/`Decode`, like the prototype's `AddWFsm`.
- **Effective-address modes = shared micro-op sub-sequences** (the Exodus `EffectiveAddress` pattern,
  re-expressed): e.g. `(An)` → one `ReadWord`; `(An)+` → read + post-inc `Internal`; `d16(An)` →
  extension-word read + add; `Dn`/`#imm` → no bus access (pure scratch fill). An opcode recipe is then just
  *"compute source EA → read operand → ALU → compute dest EA → write"*; the EA machinery, FC derivation,
  prefetch, **long = two word accesses** (`addr`, `addr+2`), cycle accounting, both drivers, and serialization
  all live **once** in the shared layer.
- **Cycle accounting:** each `MicroOp` contributes a cycle cost; the per-instruction sum equals the
  SingleStepTests `length`. Bus-access granularity needs *totals + transaction order/FC/addr/value* to match —
  intra-access (per-master-clock) placement is the deferred granularity-C option behind the same framework.
- **Two drivers, one definition:**
  - `run_to_completion(regs, bus) -> cycles` — default; interpret micro-ops to `Done`.
  - `step_micro_op(regs, bus) -> Outcome` — quiesce; perform one micro-op, pausable; in-flight `MicroState`
    held in the CPU struct so a debugger can stop at a bus boundary, snapshot, restore, resume.

---

## Tasks (TDD, logical commits)

- [ ] **P1 — `MicroOp` + `MicroState` + shared interpreter primitives.** Failing tests first: a `ReadWord`
  emits the expected `Transaction` and lands in the right scratch slot; `Prefetch` advances the queue + `pc`;
  `Internal` consumes cycles with no transaction; `MicroState` round-trips through bincode. Over `Bus68k`.
- [ ] **P2 — The two drivers + `Cpu68000` holder.** `run_to_completion` and `step_micro_op` over the same
  recipe; a `Cpu68000` owning `Registers` + optional in-flight `MicroState`. Test: on a hand recipe both
  drivers reach identical final state + identical transaction stream; `step_micro_op` pauses at each boundary.
- [ ] **P3 — Re-express `ADD.w Dn,(An)` as a recipe.** Replace the bespoke `step_instruction`/`AddWFsm` with
  one recipe (fold in `decode_add_w_dn_an` + `add_w_flags`). Port the prototype's per-cycle/per-boundary
  snapshot test to assert quiesce + serialize at every bus-access boundary. Both drivers green on the
  `db50` reference case.
- [ ] **P4 — `ADD.w <ea>,Dn` (the generalization proof).** Add `1101 ddd 001 mmm rrr` decode + recipe for a
  bus source EA (`(An)`) and a no-bus source EA (`Dn` and/or `#imm`), register destination. Exercises the
  *other* direction (read-modify-write of a register) + EA modes that touch and don't touch the bus —
  confirming the EA/operand abstraction generalizes. **No new test data** — these cases are already in the
  vendored `ADD.w.json`.
- [ ] **P5 — SingleStepTests gate.** Extend `tests/singlestep_m68000.rs` to run **both** `ADD.w` forms through
  **both** drivers (regs/SR/RAM/prefetch/cycles + per-cycle transaction stream), asserting driver agreement.
  Keep the A7/SP + odd-address (address-error) cases xfail'd as out-of-slice (unchanged scope).
- [ ] **P6 — Perf measurement.** Extend `examples/cycle_granularity_perf.rs` to measure the framework's
  run-to-completion vs the prototype's instruction-stepped baseline (`2.77 ns`/`8.34 ns` reference). Record
  the number + the macro-fast-path go/no-go threshold in a short note appended to the cycle-granularity
  decision doc (or a follow-on note).
- [ ] **P7 — CHECKPOINT.** Summarize: shape, both-form proof, driver agreement, mid-instruction
  serialize/restore, perf delta. **PAUSE for owner confirmation before full opcode coverage (Step 2/3).**

---

## How it generalizes (what the checkpoint must confirm)

- **Decode → recipe** scales to all ~88 families: per-opcode code shrinks to "decode operands + declare the
  recipe"; the heavy, repeated machinery is shared and tested once.
- **EA modes** plug in as sub-sequences (proved across a bus mode + a no-bus mode in P4); the remaining modes
  are more of the same shape (Step 3).
- **Long = two word accesses**, **RMW** (read+write with a flag), and **exceptions/address-errors** (a
  micro-op sub-sequence: stack-frame writes + vector fetch) all fit the same `MicroOp` stream — sketched here,
  built in Step 3 behind the versioned xfail manifest.
- **Granularity-C (per-master-clock)** stays reachable by sub-dividing bus-access micro-ops, without changing
  recipes — reserved for the VDP + deferred 68k accuracy.

## Risks / dependencies

- **RTC perf is the one open empirical question** (P6). Mitigation: measure; the macro-inlined fast path is a
  drop-in recovery that leaves recipes untouched. Even the slowest prototype model was ~160× real-time, so the
  risk is N-instance fuzzing throughput, not correctness or daily use.
- **Serialization invariant** — `MicroState` must stay small + fixed + bincode-clean (no `HashMap`/floats/
  `usize`-in-state); enforced by P1's round-trip test, mirroring the prototype.
- **Scope discipline** — bus unification + System wiring (Step 2) and full coverage (Step 3) stay *after* the
  checkpoint; this push must not silently expand into them.
- **Never modify `../oracle/`** (study-only); jgenesis is GPL-3 architecture reference only.
