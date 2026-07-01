# m68000 micro-op vocabulary ledger

**Purpose:** the acceptance inventory for the macro-inlined RTC perf pass (policy 7 in
`docs/decisions/2026-07-01-audit-policies.md`). The codegen must reproduce every
mechanism here *exactly*; the both-drivers equivalence harness over the full SST sweep
is the gate. Maintained as families land — **every push that adds a mechanism adds a
row.** (Created 2026-07-01 from the audit; verified against the code that day.)

## The instruction vocabulary (`m68000/microop.rs`)

`MicroOp` variants (15 as of DIV): `Read`, `Write`, `Prefetch`, `Alu`, `Internal`,
`AdjustAddr`, `EaCalc`, `SetPc`, `TargetCalc`, `DecrementDnWord`, `LoadCcr`,
`EnterException`, `LoadImm`, `SetByte`, `TasRmw` — plus the `Operand`/`Dest` symbolic
operand families (scratch slots, sized register views, immediates/extension words,
`ShiftCount`, PC/SP-relative forms).

## Mechanisms beyond plain sequential micro-ops

| Mechanism | What codegen must reproduce | Introduced by / where |
|---|---|---|
| **Alu-returns-cycles, self-booked** | An `Alu` exec arm computes a *runtime data-dependent* cycle cost, adds it to `self.cycles` itself, and early-returns — **bypassing `exec_one`'s shared `self.step += 1; self.cycles += cycles` tail** (microop.rs ~1380–1480). Double-booking or missed booking here is the #1 codegen hazard. | MUL push (`38+2·count`), reused by DIV (`docs/plans/2026-06-27-m68000-mul.md`, `2026-06-28-m68000-div.md`) |
| **Decode-time data-dependent timing** | Cycle costs resolved *at decode* from operand data (e.g. bit ops' +2 register-form timing), baked into the recipe before execution. | Bit-ops push (`2026-06-26-m68000-bit-ops.md`) |
| **Shared `shift_recipe` + `Operand::ShiftCount(u8)`** | One recipe family parameterized over eight `AluOp`s; immediate/memory forms via `ShiftCount`, register forms via `DataRegFull`; the word `ea_dst` shift-by-1 memory RMW shape. | Shifts push (`2026-06-27-m68000-shifts-rotates.md`) |
| **`TasRmw` — indivisible RMW bus cycle** | One atomic read-modify-write bus transaction (the distinct `t` transaction kind in the stream), not a Read+Write pair (`bus68k.rs`). | Scc/TAS push (`2026-06-26-m68000-scc-tas.md`) |
| **Execution-time abort (address error)** | A faulting micro-op **rewrites its own `MicroState`** into the E3/E4 group-0 exception frame sequence mid-instruction (`exception.rs` header) — control flow codegen cannot straight-line through. | Exceptions push (`2026-06-25-m68000-exceptions.md`) |
| **Exception-frame ordering corrections** | DIV div0: `[Internal(idle=8), Prefetch]` order and saved-PC = *instruction start* (not post-extension PC). Any codegen of `EnterException` sequences must preserve recipe order verbatim. | DIV push (`2026-06-28-m68000-div.md`) |
| **SST runner coverage contract** | `covered(opcode, ini, fin)` (`tests/singlestep_m68000.rs`) gates by opcode *and* case data; the `ran >=` thresholds are floors the perf pass must not disturb. | Shifts push onward |

## Standing carve-out

The plain `(A7)` mode-2 indirect deferral in the pre-Scc families is tracked debt
(audit finding 5) — burn it down before the integration pivot so the codegen pass never
sees two coverage regimes.
