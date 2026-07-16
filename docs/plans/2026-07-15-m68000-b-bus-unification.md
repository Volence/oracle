# Push B — bus unification, the Mega Drive memory map, and the power-on reset sequence

**Status: PLAN (2026-07-15).** Design of record: `docs/decisions/2026-07-15-integration-pivot-design.md`
(D5 bus unification, D6 memory map + reset). This is the third push of the integration pivot
(Push 0 + Push A are code-complete; HEAD `4630800`, SST **1,000,058** green). Push B is pure
bus/adapter/reset work over the existing proven CPU core — **no `System` run-loop wiring** (that is
Push C) and **no `StubCpu` deletion** (also Push C). The one prime directive carries over: the
micro-op single-definition property is preserved — the reset sequence is a micro-op recipe run by the
same `exec_one` reached by both drivers; nothing here adds a second execution path.

## Goal

Take the 68000 from "runs one SST case in a flat 16 MiB harness" toward "runs real code on the real
machine's bus" by:

1. **Unifying the two bus vocabularies without churning the proven CPU trait** (D5): `Bus68k` stays
   exactly as the CPU-facing trait; the *generic* `crate::bus` layer absorbs the 68000's vocabulary
   (`fc`, `Tas`), and the duplicate `Size` enums collapse to one.
2. **Adding the wait-cycle return channel** — the seam VDP-port waits / Z80 contention / DMA-halt
   timing plug into later — with `FlatBus` returning 0 so SST behavior is bit-identical.
3. **Giving every 24-bit address a deterministic answer** (D6): a new `MegaDriveBus` split-borrow
   adapter over `System`'s fields, the full memory map, open-bus = last-bus-word, and the TAS
   write-drop quirk.
4. **The real power-on reset sequence** as a micro-op recipe (fetch SSP/PC from the vector table,
   S=1/T=0/I=7, no stacking), which also **wires the shaped-but-unwired reset-wake arm** from A4.2.
5. **Deleting `prototype.rs`** — the cycle-granularity decision it existed to settle is ratified.

## What is NOT in Push B (deferred, per the design of record)

- Wiring `Cpu68000` into `System` / replacing `StubCpu` / `run_until` / the ×7 clock — **Push C.**
- The VDP proper (its ports get a deterministic stub here) — the ratified VDP design.
- Bus-arbitration *timing* (Z80 BUSREQ stalls, DMA steal) — the seam is defined, the model lands
  with the VDP.
- `export_state` v1 freeze — **Push D.**
- The BlastEm differential harness (A3.2 + the differential docket) — deliberately scheduled *after*
  Push B, immediately before Push C, since it is the same infrastructure C's nightly differential
  needs (owner's sequencing call). **Do not pick docket items up early.**

---

## Recon (pinned)

### The two bus layers today (code map)

- **`m68000::bus68k`** — the CPU-facing trait `Bus68k { read16/write16/read8/write8/tas }`, each taking
  an `fc: u8` and masking `addr` to `ADDR_MASK` (24-bit). `Transaction { kind, fc, addr, size, value }`
  + `TxKind { Read, Write, Tas }` is the recorded stream. `FlatBus` is the SST harness's private flat
  16 MiB recording bus. **This trait encodes real electrical semantics 1,000,058 SST cases pinned —
  it does not change shape except for the additive wait-cycle return (B4).**
- **`crate::bus`** — the generic typed protocol `Bus { read/write }` + `BusEvent { op, addr, size, value }`
  + `BusOp { Read, Write }` + `BusEventSink`, and the `SystemBus<'a, S>` split-borrow adapter (RAM +
  synthetic VRAM window, deferred-write seam via `apply_writes`). This is the single instrumentation
  surface (watchpoints, tracers, profiler) for *all* masters — it absorbs the 68000 vocabulary.
- **Two identical `Size` enums** — `microop::Size` (bincode `Encode`/`Decode`; used by every recipe,
  `Transaction`, `ea.rs`, `decode.rs`, and the SST harness) and `bus::Size` (has `.bytes()`; used by
  `SystemBus` + `StubCpu`). Both are exactly `{ Byte, Word, Long }`.
- **`prototype.rs`** — the two-way `ADD.w Dn,(An)` cycle-granularity prototype. Its only *external*
  consumer is `examples/cycle_granularity_perf.rs`; the SST harness imports the durable bus types from
  `bus68k` **directly** (not through the prototype re-export), so deleting the prototype is gate-safe.
- **`StubCpu`** — the Phase-0 placeholder chip `System` drives via `SystemBus`. **Stays** until Push C.

### The power-on reset sequence — pinned from M68000UM §6.3.1 + §6.2.4 + Yacht L1546

M68000UM §6.3.1 (verbatim facts):
- "The processor is forced into the supervisor state, and the trace state is forced off. The interrupt
  priority mask is set at level 7." → **SR = `0x2700`** (S=1, T=0, I=7; CCR undefined/zeroed).
- "Because no assumptions can be made about the validity of register contents, in particular the SSP,
  **neither the program counter nor the status register is saved.**" → **no stacking** (§6.2.4 lists
  reset among group-0 but "the reset exception does not stack any information").
- "The address in the **first two words** of the reset exception vector is fetched as the **initial
  SSP**, and the address in the **last two words** ... as the **initial program counter**." → read
  `$0`/`$2` → A7 (SSP); read `$4`/`$6` → PC.
- §6.2.1 (verbatim): "All exception vectors reside in the supervisor **data** space, **except for the
  reset vector, which is in the supervisor program space.**" → **all six reset reads are FC=6**
  (supervisor program), NOT FC=5. This is why the reset recipe cannot reuse `vector_fetch_and_reload`
  (whose vector reads are FC=5 supervisor-data).

Yacht L1546 (the timing/bus-stream ground truth):
```
  /RESET              | 40(6/0)  |    (n-)*5   nn       nF nf nV nv np  n np
```
- `40(6/0)` = 40 clocks, **6 reads, 0 writes** — SSP MSW (`F`)@$0, SSP LSW (`f`)@$2, PC MSW (`V`)@$4,
  PC LSW (`v`)@$6, then the two prefetches (`np np`) at PC / PC+2.
- Leading idle `(n-)*5 nn` (the reset-line-sampling idle) before the first fetch; the `n` between the
  two prefetches is the same `n2` inter-prefetch idle every taken-branch tail already emits.
- Legend (Yacht L145–149): `V`/`v` = fetch vector MSW/LSW; `F`/`f` = fetch SSP MSW/LSW (reset only).

The exact sub-allocation of the 40 clocks across the leading idles is **timing, not behavior** — it is
pinned to Yacht's stream for the bus-stream test but is not a correctness invariant (per
[[feedback-unknowns-timing-vs-behavior]]: only the reads' addresses/FC/values, the SR value, the
no-stacking property, and the final PC/SSP are behavioral and must match reference exactly).

### The `begin_next` reset-wake arm (shaped in A4.2, wired here)

`microop.rs` `step()` already carries the shape (the `CpuState::Stopped` arm): "(reset-wake is shaped
here but unwired until Push B — do not fake a reset)". Wiring it means: a `Stopped` **or** `Halted` CPU
exits its state on an external reset assertion and runs the reset recipe. The design's `Halted`
terminal state is documented as "only reset leaves it" — this push makes that true.

### Micro-op vocabulary available for the reset recipe

`Read` (fc-tagged), `Combine32` (assemble hi/lo → scratch), `SetPc`, `Prefetch`, `Internal{cycles}`,
`LoadImm`, `LoadSr{Operand}`, plus the `Dest::AddrReg(n)` full-32 register write-back (routed through
`Registers::addr_reg_set` so `n==7` hits the active stack pointer — the MOVEA machinery). The reset
recipe composes these; the two small new needs (set A7 from a combined scratch as a *32-bit* store, and
set SR to the constant `0x2700`) are settled in B6 — reuse the existing `Dest::AddrReg` write-back for
A7 and either a constant-Operand `LoadSr` or a tiny `EnterReset` micro-op for SR; the build slice picks
the lowest-churn option and documents it.

---

## Slices (TDD, one coherent commit each; full triplet + fmt/clippy per commit; SST re-run per slice)

Linear/gated order — each dependency is real:

### B1 — delete `prototype.rs` + the perf example (housekeeping; ratified job done)
Delete `src/m68000/prototype.rs`, its `pub mod prototype;` in `mod.rs`, and
`examples/cycle_granularity_perf.rs` (its sole consumer). The cycle-granularity brief is ratified, so
the two-way prototype has no remaining purpose. Verify the SST harness + all of `m68000` still build
(the durable bus types are imported from `bus68k` directly). **Gate: SST green, workspace builds.**
Pure deletion first so later slices refactor a smaller surface.

### B2 — one shared `Size` enum
`microop::Size` becomes the single definition (it is bincode-serialized and has hundreds of consumers
incl. the SST harness); give it the `.bytes()` method; `crate::bus` re-exports it
(`pub use crate::m68000::microop::Size;`) so `SystemBus`/`StubCpu`/`BusEvent` all name the same type.
Delete the standalone `bus::Size` definition. No cyclic dependency (`bus` → `m68000::microop` is
one-directional; `microop` does not import `crate::bus`). **Gate: SST green.**

### B3 — `BusEvent` gains `fc` + `BusOp::Tas`
Add `fc: u8` to `BusEvent` (0 for non-CPU masters; the CPU adapter fills the real FC) and a
`BusOp::Tas` kind (the indivisible RMW, distinct from a Read+Write pair). Update every `BusEvent`
construction/assertion site (`SystemBus`, the `bus.rs`/`system.rs` tests). `SystemBus` passes `fc: 0`
(it has no CPU FC concept). **Gate: SST green.**

### B4 — the wait-cycle return channel
`Bus68k`'s access methods gain a wait-cycle return — the access's *extra* cycles beyond the base 4.
`read16/read8/tas` return `(value, u32)`; `write16/write8` return `u32`. **`FlatBus` always returns
0**, so the SST transaction streams + cycle counts are bit-identical (the invariant that keeps the
1,000,058 gate meaningful). `exec_one` adds the returned wait in its bus arms (cycle costs live only
there, never in recipes — no recipe changes). Nothing *uses* a non-zero wait in this push; it is the
seam. **Gate: SST green (bit-identical), plus a unit test that a stub wait-returning bus adds cycles in
`exec_one`.**

### B5 — `MegaDriveBus` + the memory map + open bus + the TAS write-drop quirk
New `MegaDriveBus` (split-borrow adapter, same shape as `SystemBus`) implementing `Bus68k` over the
`System`'s memory fields, emitting a `BusEvent` (with the real `fc`) per access. `System` gains
`rom: Vec<u8>` (owned, in `Clone`/bincode for now — snapshot-cost refinement is a deferred free change
per snapshot policy 5) and `last_bus_word: u16` (open-bus latch). The map (D6), every range
deterministic:

| Range | Behavior |
|---|---|
| `$000000–$3FFFFF` | ROM (read-only; past a short ROM's end → open bus, no mirroring) |
| `$400000–$7FFFFF` | open bus |
| `$A00000–$A0FFFF` | Z80 RAM (8 KiB) as plain bytes (68k side; nothing executes) |
| `$A10000–$A1001F` | I/O: version reg = fixed NTSC/no-disk; controller ports = documented constant |
| `$A11100/$A11200` | Z80 BUSREQ/RESET latches: writable/readable, bus always "granted" |
| `$C00000–$C0000F` | VDP ports: deterministic stub (writes latch a placeholder; status = fixed empty-FIFO/VBlank-clear) |
| `$E00000–$FFFFFF` | 64 KiB work RAM, mirrored across the window |

- **Open bus** returns `last_bus_word` (updated on every access). Cheap, deterministic, closer to
  hardware than a constant.
- **TAS write-drop** (the Gargoyles/Ex-Mutants quirk): on the Mega Drive the RMW *write* cycle of `TAS`
  is not honored — `MegaDriveBus::tas()` performs the read, **skips the store**, still logs the `Tas`
  event. `FlatBus::tas()` keeps the datasheet behavior (SST pins it). One focused test pins each side.

**Gate: a map test per region (read/write behavior + open-bus + mirroring); the TAS quirk pair
(`MegaDriveBus` drops, `FlatBus` stores); SST green (harness untouched — the CPU core cannot tell
`MegaDriveBus` from `FlatBus`, which is the point).**

### B6 — the power-on reset recipe + wiring the reset-wake arm
`reset_exception_recipe()` (a new decode/exception builder): idle (`(n-)*5 nn`) → read SSP MSW@$0 (FC=6)
→ SSP LSW@$2 → `Combine32` → set A7 → read PC MSW@$4 → PC LSW@$6 → `Combine32` → `SetPc` → set
SR=`0x2700` → prefetch reload (two FC=6 reads at PC/PC+2 with the `n2` idle between). No stacking.
Then wire `begin_next`: a `Cpu68000::assert_reset()` (or a `reset_pending` latch mirroring `ipl`) that,
when a `Stopped`/`Halted`/`Normal` CPU is polled, runs the reset recipe and returns to `Normal`. The
A4.2 comment "reset-wake is shaped here but unwired until Push B" is resolved.

**Gate: hand-authored bus-stream tests (both drivers, snapshot at every boundary — the SST discipline
without SST): the six reads at $0/$2/$4/$6 + two prefetches with correct FC=6 and values; final
A7=SSP, PC=handler, SR=0x2700; no stack writes; a `Halted` and a `Stopped` CPU both wake to `Normal`
via reset. SST green.** (This push does NOT change `System::reset`, which still resets `StubCpu` until
the CPU is wired in Push C — the reset recipe is exercised standalone over a bus with a vector table.)

---

## Anti-cheating / invariants

- **The SST gate stays exactly 1,000,058 and is re-run per slice.** No assert weakened/removed/ignored;
  no `ran>=` lowered. B4's `FlatBus`-returns-0 invariant is what keeps every SST stream bit-identical —
  if any SST case's transaction log or cycle count shifts, the slice is wrong.
- **`Bus68k` shape is additive-only** (the wait-cycle return). Its read/write/tas *semantics* are
  frozen by SST; only `MegaDriveBus` (a new impl) introduces the map/open-bus/TAS-drop divergences, and
  those are on the `System` side, never in the CPU core.
- **Behavioral facts pinned from `docs/reference/` only** (M68000UM + Yacht.txt): reset FC=6, SR=0x2700,
  no-stacking, the six read addresses. The Genesis memory-map ranges + open-bus + TAS-drop are pinned
  from the design of record (D6); they are hardware-platform facts, not 68000-core behavior.
- **Clean-room:** no jgenesis/BlastEm/GPGX source, in any form. The TAS-drop and open-bus behaviors
  enter via the ratified design doc's documentation, not another emulator's code.
- **Timing vs behavior** ([[feedback-unknowns-timing-vs-behavior]]): the reset stream's exact idle
  sub-allocation and any wait-state numbers are deferrable/pinned-to-Yacht; the reads' addresses/FC/
  values, SR, no-stacking, and final regs are behavioral and pinned exactly.

## Risks

- **B4 trait-shape change is the widest churn** (every `Bus68k` call site in `exec_one` + both impls).
  Mitigation: the return is additive and `FlatBus` returns 0 → SST bit-identical is the tripwire; run
  the full SST sweep immediately.
- **`Size` unification (B2) touches many files** but is mechanical (one type, two names → one). The
  re-export keeps import churn minimal.
- **`System` gaining `rom`/`last_bus_word` fields** perturbs bincode layout → any snapshot fixture must
  be regenerated; snapshot policy 5 already voids cross-version compat so this is free.
- **Reset FC=6 subtlety** — easy to reflexively reuse the FC=5 `vector_fetch_and_reload`. Pinned above;
  B6 uses a distinct FC-6 path.

## Opportunistic cleanups (per the design's review notes, do while in the area)

- `m68000/mod.rs` module doc still says "Phase-0 vertical slice" / references the prototype — rewrite
  when B1 removes the prototype.
- Note in the SST harness header (if not already) that `'n'` idle tokens are pinned in aggregate length,
  not by position.
