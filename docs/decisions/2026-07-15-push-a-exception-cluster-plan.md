# Push A plan: the exception/async cluster (D1–D4)

**Status: IN PROGRESS 2026-07-15.** Implements Push A of the integration-pivot design
(`2026-07-15-integration-pivot-design.md`, sections D1–D4): the asynchronous exception
cluster — orchestrator + processor states, decode totality, trace, interrupts/STOP. Pure CPU
work over `FlatBus`; **no `System` changes** (those are Push B/C).

## The safety-net inversion (why this push is different)

The grind so far gated every commit on ≥976k vendored SingleStepTests cases. **Push A has no
SST data** — that is audit finding 3. There are no vendored files for interrupts, trace,
illegal, line-A/F, privilege, STOP, or RESET. Recon therefore changes shape:

- **Ground truth** = M68000UM exception-processing cycle tables + Yacht.txt bus-stream
  timing (permissive documentation, clean-room policy), cross-checked **behaviorally** against
  BlastEm *over the bus* (never its source).
- **Discipline unchanged**: hand-authored vectors for every new frame, run through **both
  drivers** (`run_to_completion` fast path + `step_micro_op` quiesce path), snapshot/restore
  asserted at every micro-op boundary — the SST rigor, minus SST.
- **Tiebreak**: where UM and Yacht are silent or disagree, BlastEm-over-the-bus is the
  arbiter; residual divergences go to the versioned xfail manifest.

## Recon: the pieces already proven

The exception *tail* is TDD-pinned and reused verbatim (`exception.rs`):

- `push_standard_frame(saved_pc, save_sr)` — the 6-byte group-1/2 frame in the 68000 on-bus
  write order (`PCL @ B+4`, `SR @ B+0`, `PCH @ B+2`, all FC=5).
- `vector_fetch_and_reload(vector_addr)` — two FC=5 vector reads, assemble the handler,
  `SetPc` + two FC=6 prefetches with the `n2` idle between.
- `MicroOp::EnterException { save_sr }` — captures live SR, sets S, clears T.

Every new frame in this push is the **TRAP recipe with a different vector and saved-PC**
(`trap_recipe`, decode.rs): `[TargetCalc(saved_pc), EnterException, Internal(idle),
push_standard_frame, vector_fetch_and_reload]`, all length 34 = the `34(4/3)` UM timing shared
by illegal / privilege / trace / line-A / line-F / TRAP.

**Pinned invariant** (from TRAP saving `pc+2` for a 1-word op): at decode, `regs.pc` = the
address of the instruction word in `prefetch[0]`. So:

| Exception | Vector | Stacked PC | Timing |
|---|---|---|---|
| Illegal (`0x4AFC` + unassigned) | 4 | `pc + 0` (the faulting instruction) | 34(4/3) |
| Privilege violation | 8 | `pc + 0` (the offending instruction) | 34(4/3) |
| Trace | 9 | `pc + 0` (next-instruction start; T sampled pre-exec) | 34(4/3) |
| Line-A (`0xAxxx`) | 10 | `pc + 0` | 34(4/3) |
| Line-F (`0xFxxx`) | 11 | `pc + 0` | 34(4/3) |
| Interrupt (autovectored) | 24 + level | `pc + 0` (next-instruction start) | 44(5/3) + IACK |

**SR bits** (`registers.rs`): `SR_TRACE = 0x8000`, `SR_SUPERVISOR = 0x2000`, interrupt mask
I2–I0 = `0x0700`, `SR_IMPLEMENTED = 0xA71F`.

## Recon: PINNED from `docs/reference/` (Yacht.txt v1.1 + M68000UM §6)

The references are now in-tree (`docs/reference/`, Yacht tracked, PDF untracked). Everything the
A/B question flagged is pinned with a citation:

- **Vectors** (UM Table 6-2, PDF p.92): Illegal=4 (`0x010`), Privilege=8 (`0x020`), Trace=9
  (`0x024`), Line-1010=10 (`0x028`), Line-1111=11 (`0x02C`) — all **supervisor-data space**
  (FC=5 vector fetch, matching the machinery).
- **Priority** (UM §6.2.3 + Table 6-3, PDF p.93-94): group 0 > group 1 (**trace > interrupt >
  illegal/privilege**) > group 2. Illegal/privilege are decode-time ("detected when next to
  execute"); trace/interrupt are boundary events. = the `begin_next` order exactly.
- **Frame** (UM Fig 6-5, PDF p.95): SR @ SSP+0, PCH @ SSP+2, PCL @ SSP+4 = `push_standard_frame`.
  SR copied *before* S-set/T-clear (§6.2.5 step 1) = `EnterException`.
- **Illegal/Privilege bus stream** (Yacht L1551/L1550): both `34(4/3)`, `nn ns ns nS nV nv np n
  np` — **byte-identical to TRAP** (Yacht L956). Leading idle = `nn` (= `Internal{4}`, not a
  guess). Cycle check: 4 + 3·4(writes) + 2·4(vector) + 2·4(reload) + 2(n2) = 34. ✓
- **Privilege stacked PC** (UM §6.3.7, PDF p.100): *"the address of the first word of the
  instruction causing the privilege violation"* = `pc+0` (`TargetCalc(PcPlus(0))`). **Behavioral
  pin, cited — not xfail.**
- **Privileged set** (UM §6.3.7): `ANDI/ORI/EORItoSR`, `MOVE to SR`, `MOVE USP`, `RESET`, `RTE`,
  `STOP`. **`MOVE from SR` is NOT privileged on the 68000** (68010-only) — do not gate it.
- **Trace** (UM §6.3.8, PDF p.100-101): T latched at instruction *start*; fires *after*
  completion; stacked PC = *address of the next instruction*; suppressed if the instruction was
  illegal/privileged/interrupt-taken/reset/bus/addr-error; trace before interrupt. Stream
  `nn ns nS ns nV nv np n np` (Yacht L1548) — **write order differs from TRAP** (`ns nS ns`).
- **Interrupt** (Yacht L1549): `44(5/3)`, `n nn ns ni n- n nS ns nV nv np n np` — the `ni` IACK
  vector-number-acquisition cycle. Mega Drive autovectors (VPA) → vector = 24 + level (Table 6-2
  Level-n Autovector). For A4.

### Residual unknowns (legitimately deferred — reference itself is silent)

- **STOP second microcycle** (Yacht L908-911): `4(0/0)`, stream `n`; Yacht states *"there is no
  real clues of how the second microcycle of this instruction is spent."* This is a **timing**
  ambiguity — defer to the BlastEm cross-check / xfail manifest. STOP's **behavior** (loads
  `prefetch[1]`→SR masked to `SR_IMPLEMENTED`, privileged, then `Stopped`) is not ambiguous and
  is pinned; only the idle placement of cycle 2 is open.
- **RESET `#`** (`0x4E70`): already implemented (`reset_recipe` = `[Internal(4), Internal(124),
  Prefetch]`); UM §6.2.5 note confirms the 124-cycle reset-line assertion (PDF p.96).

## Slicing (proven small-commit cadence; each keeps SST green)

### A1 — orchestrator + `CpuState` + STOP + privilege
- `enum CpuState { Normal, Stopped, Halted }` as a plain serialized field on `Cpu68000`.
- `begin_next` / `step()` skeleton (D1). Event priority coded once: reset > address/bus error
  (already in-flight) > trace > interrupt > decode-time (illegal/privilege) > plain decode.
  Trace and interrupt latches are wired but only *serviced* from A3/A4.
- **Correction (2026-07-15 recon):** NOP (`0x4E71`, `nop_recipe`) and RESET (`0x4E70`,
  `reset_recipe` = `[Internal(4), Internal(124), Prefetch]`) are **already decoded**. The only
  remaining legal straggler is **STOP** (`0x4E72`, → `Stopped`; wake in A4). It loads
  `prefetch[1]` → SR (masked to `SR_IMPLEMENTED`), then stops; privileged.
- **Privilege violation** (vector 8): user-mode gate on `MOVEtoSR`, `ANDI/ORI/EORItoSR`,
  `MOVE USP` (`usp_recipe`), `RESET`, `RTE`, `STOP`. Decode-time check (regs available) →
  frame recipe. These arms exist today but their privilege trap is explicitly *ungated*
  ("correctness-only, not gated") — A1 adds the gate.
- **Why A1 before A2**: the totality blanket must not misclassify the legal STOP as illegal.
- **SST stays green by construction**: every gated op is supervisor-only in every vendored
  case (`recon-sr-moves`, `recon-itoccr` — "privilege trap unexercised"); the new ops are new
  opcode points outside the ADD/SUB/logic spaces.
- Gate: hand vectors for RESET/STOP/privilege, both drivers, snapshot every boundary; full SST.

### A2 — decode totality (ILLEGAL / line-A / line-F)
- Terminal `todo!` (decode.rs:1207) + every reachable EA-builder `todo!` (ea.rs 440/626/830/
  1007/1111/1309/1520/1646, decode.rs:2857) → the correct exception recipe.
- **Illegal-EA gates**: legal instructions with illegal mode/reg (e.g. `AND.b An,Dn`) route to
  ILLEGAL instead of silently flowing through an arm that trusted the harness `covered()`
  filter.
- Vectors: ILLEGAL = 4 (incl. official `0x4AFC`), line-A = 10 (`0xAxxx`), line-F = 11 (`0xFxxx`).
- Gate: **65536-opcode totality sweep** — every opcode yields a recipe (no panic), with spot
  checks that known-illegal encodings map to the right vector; SST green.

### A3 — trace (T-latch, vector 9)
- Latch T at instruction start (sampled before execution). After the recipe completes,
  `begin_next` services the vector-9 frame (T already cleared for the handler by
  `EnterException`). Trace outranks interrupt. One serialized bool.
- Gate: hand vectors both drivers; priority test (trace beats a pending interrupt).

### A4 — interrupts + STOP wake
- `Cpu68000::set_ipl(level)` latched field; take when `ipl > SR.I` (level 7 = edge NMI).
- Autovector everything (Mega Drive VPA): vector = `24 + level`. New IACK micro-op so the bus
  stream shows the cycle; reuse `push_standard_frame`/`vector_fetch_and_reload`.
- STOP wake: `Stopped` CPU wakes on `ipl > SR.I` (or reset); idle consumed in slice-sized
  chunks so `run_until` (Push C) terminates.
- Gate: hand vectors both drivers; priority (trace > int > decode) + STOP-wake tests; SST green.

## Opportunistic cleanups (per design doc's review notes)
- `m68000/mod.rs` module doc still says "Phase-0 vertical slice" — rewrite at A1.
- Note in the harness header that `'n'` idle-token *placement* is asserted only in aggregate.
- Document the "recipes bake decode-time state → not cacheable per opcode" invariant where the
  macro-RTC pass will look.
