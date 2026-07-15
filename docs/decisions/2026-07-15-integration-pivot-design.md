# Decision brief: the integration pivot — wiring `Cpu68000` into `System`

**Status: PROPOSED 2026-07-15 (Fable).** Design authority delegated by the owner; treat as
ratified unless overridden. This is the design for step 5 of the audit's recommended sequence
(`docs/2026-07-01-plan-audit.md`): retire `StubCpu`, unify `Bus68k` with the system bus, wire
`Cpu68000` into `System`, close audit finding 3 (the exceptions/async cluster), and freeze
`export_state` (audit policy 2). It precedes the macro-RTC perf pass (audit policy 7).

**Correction to the handoff record (found by the 2026-07-15 pre-pivot review):** the SST grind
is *not* complete. All 124 vendored files are loaded and 976,047 cases pass (re-verified green
today, both drivers), but **24,013 vendored cases are skipped** — and 18,336 of them are
*undocumented* skips: **ADDQ/SUBQ (16,529 cases, `0x5xxx`) and ADDI/SUBI (1,807) are simply
not implemented**, hiding as opcode contaminants inside the ADD/SUB files (ANDI 1,481 /
ORI 1,461 / EORI 2,733 are also unimplemented but at least documented as skipped; +2 corrupt
ASL.b entries, correctly excluded). ADDQ is among the most common instructions in real 68000
code — a real ROM panics decode within its first few hundred instructions. **Push 0 below
closes this before anything else.**

## Scope

**In:** everything the 68000 needs to run *real code on the real machine's bus* rather than
one SST case in a flat 16 MiB harness — decode totality (no reachable `todo!()`), the
asynchronous exception cluster (interrupts, trace, STOP, privilege, ILLEGAL/line-A/line-F),
the reset sequence, the Mega Drive memory map with a deterministic policy for every address,
scheduler wiring, and the frozen `export_state` differential currency.

**Out (unchanged from the charter):** the VDP (design already ratified in
`docs/2026-07-01-vdp-design.md` — its ports get a deterministic stub here), the Z80 core
(its RAM is mapped, nothing executes), sound chips, DMA/bus-arbitration *timing* (the seam
is defined, the cycle-stealing model lands with the VDP), and the macro-RTC perf pass
(triggered immediately after this pivot, per policy).

The prime directive carries over: **the micro-op single-definition property is preserved.**
Every new behavior — an interrupt entry, a trace exception, an ILLEGAL frame — is a micro-op
recipe executed by the same `exec_one`, reached by both drivers through shared orchestration.
Nothing in this pivot adds a second execution path.

---

## D1 — The orchestrator: `Cpu68000::step()`, processor states, and event priority

Today the SST harness *is* the orchestrator: it calls `decode` → `run_to_completion` (or the
step driver) on a pre-loaded prefetch queue. The pivot moves that loop into the CPU as the
one place that decides *what happens next*:

```rust
enum CpuState { Normal, Stopped, Halted }   // Exodus's serialized set, minus a separate
                                            // "Exception" (in-flight recipes cover it)

impl Cpu68000 {
    /// Decide and begin the next unit of work: finish the in-flight recipe if quiesced
    /// mid-instruction; else service (in priority order) a pending group-0 latch, trace,
    /// an unmasked interrupt; else decode prefetch[0] (which yields exception recipes for
    /// illegal/privileged opcodes). Shared by BOTH drivers — the decision logic exists once.
    fn begin_next(&mut self) -> BeginOutcome;

    /// Fast path: begin_next + run_to_completion. Returns CPU cycles consumed.
    pub fn step(&mut self, bus: &mut impl Bus68k) -> u32;

    /// Quiesce path: begin_next (if nothing in flight) + exec_one.
    pub fn step_micro_op(&mut self, bus: &mut impl Bus68k) -> Step;  // exists; gains begin_next
}
```

- **`CpuState::Stopped`** — entered by `STOP #imm`; `step()` consumes idle cycles until an
  unmasked interrupt (or reset) wakes it. **`Halted`** — the double-fault terminal state (a
  group-0 fault while stacking a group-0 frame); only reset leaves it. Both are plain fields
  on `Cpu68000` (bincode, `Clone`) — snapshot/restore of a stopped or halted CPU is free.
- **Priority** is the 68000 group order, implemented once in `begin_next`: reset >
  address/bus error (already handled in-flight by the Shape-B rewrite) > **trace** >
  **interrupt** > decode-time exceptions (illegal, privilege) > plain decode.
- The existing `run_instruction` (decode + RTC, no inflight) remains the SST harness's
  entry; `step()` is a thin superset. The harness does not change — the 976k-case record
  keeps gating every commit.

## D2 — Decode totality: no reachable `todo!()`

The pre-pivot review measured the hole precisely: **21,391 of 65,536 opcode words panic
`decode` today** (the `todo!()` fall-through at `decode.rs:1127` plus reachable `todo!()`s in
the EA builders for illegal-EA encodings, e.g. `ea.rs:440` and friends, `decode.rs:2711`) —
and, worse, **some illegal encodings decode silently as if legal** (e.g. `AND.b An,Dn`
mode 1 flows into `arith_ea_dn` because that arm trusts the *test harness's* `covered()`
filter, which a ROM doesn't have). Both classes are fixed together so **`decode` is total
and correct over all 65536 opcodes**:

- **ILLEGAL** (all unassigned encodings + the official `0x4AFC`) → the standard group-1
  frame, vector 4.
- **Line-A** (`0xAxxx`) → vector 10; **line-F** (`0xFxxx`) → vector 11. (Sonic-era games
  genuinely hit these — line-A is a common crash signature; getting the frame right is
  debugger product value, not pedantry.)
- **Privilege violation** (vector 8): the privileged arms (`MOVEtoSR`, `ANDI/ORI/EORItoSR`,
  `MOVE USP`, `RESET`, `RTE`, `STOP`) gain the user-mode gate that SST never exercised
  (every vendored case is supervisor). The check happens at decode time — registers are
  available — and yields a frame recipe instead of the instruction's recipe.
- **`STOP #imm`** (`0x4E72`) — currently not decoded at all — lands here as the last
  instruction, with its `CpuState::Stopped` semantics (and the T-bit interaction pinned
  during recon).
- Every decode arm gets its **illegal-EA gate** (the checks `covered()` was silently
  providing): illegal mode/reg combinations for otherwise-legal instructions route to the
  ILLEGAL recipe, exactly as hardware traps them.
- A **totality test** sweeps all 65536 opcodes through `decode` and asserts a recipe (never
  a panic), with spot checks that known-illegal encodings map to the right vectors.

Timing/bus-stream ground truth: SST has **no** files for these. Numbers are pinned during
each push's recon from the M68000UM cycle tables + Yacht.txt (permissive documentation, per
the clean-room policy) and cross-checked behaviorally against BlastEm **over the bus** —
never its source. Divergences go to the versioned xfail manifest (SST-tiebreak policy
extends: UM/Yacht is the tiebreak where SST is silent).

## D3 — Trace

The T bit is **latched at instruction start** (the 68000 samples it before execution, so
setting T mid-instruction traces the *next* instruction, and clearing it mid-instruction
still traces the current one). After the instruction's recipe completes, `begin_next` sees
the latch and begins the vector-9 group-1 frame (which clears T for the handler — the
`SR_TRACE` constant and `EnterException`'s T-clearing already exist). Trace outranks
interrupts; both outrank decode. The latch is one serialized bool on `Cpu68000`.

## D4 — Interrupts: the IPL latch, autovectors, and STOP wake

- The CPU gains a **latched IPL input**: `Cpu68000::set_ipl(level: u8)` (0–7), a plain
  serialized field. The *System* owns the encoder (VDP VInt = level 6, HInt = level 4,
  controller EXT = level 2 — wired as each source lands; nothing asserts yet in this pivot
  beyond tests).
- `begin_next` takes an interrupt when `ipl > SR.I` (level 7 is edge-triggered NMI —
  modeled correctly even though the Genesis never wires it; it is a few lines).
- **The Mega Drive autovectors everything** (VPA asserted for IACK): vector = 24 + level.
  The entry is one micro-op recipe: idle + IACK cycle + SR/PC pushes + vector fetch + the
  two-prefetch reload (the existing `push_standard_frame`/`vector_fetch_and_reload`
  machinery, plus one new micro-op for the IACK cycle so the bus stream shows it).
  Sampling is at instruction boundaries — correct at bus-access granularity, consistent
  with the ratified quiesce granularity.
- **STOP wake**: a stopped CPU wakes when `ipl > SR.I` (and on reset). The stopped idle is
  consumed in scheduler-slice-sized chunks so `run_until` terminates.

## D5 — Bus unification: `Bus68k` stays the CPU-facing trait

The audit said "unify `Bus68k` with `Bus`". The right unification is **adapter, not
rewrite**:

- **`Bus68k` remains exactly as it is** — it encodes real 68000 electrical semantics (16-bit
  data bus, byte = one half, long = two words, FC codes, the indivisible TAS RMW cycle) that
  976k cases pinned. Rewriting the CPU onto a generic byte/word/long trait would churn
  proven code for zero behavior.
- The **generic `crate::bus::Bus`/`BusEvent` layer absorbs the 68000's vocabulary** instead:
  `BusEvent` gains `fc: u8` (0 for non-CPU masters) and a `BusOp::Tas` kind; the duplicate
  `bus::Size` / `microop::Size` enums collapse to one. The event stream stays the single
  instrumentation surface (watchpoints, tracers, the profiler) for *all* masters.
- A new **`MegaDriveBus`** (split-borrow adapter, same shape as today's `SystemBus`)
  implements `Bus68k` over the `System`'s fields and emits `BusEvent`s per access. The
  SST harness keeps its private `FlatBus` — the CPU core cannot tell the difference, which
  is the point.
- The deferred-write seam is unchanged: writes landing in another chip's domain (VDP ports,
  Z80 RAM while the Z80 runs — later) queue and drain at `apply_writes()`.
- **The stall channel lands with the unification** (cheap now, risky later): `Bus68k`'s
  access methods gain a wait-cycle return — each returns the access's *extra* cycles beyond
  the base 4 (`FlatBus` always returns 0, so SST behavior is bit-identical), and `exec_one`
  adds it in its bus arms (cycle costs live only there, never in recipes, so no recipe
  changes). This is the seam VDP-port wait states, Z80 bus contention, and DMA-halt timing
  plug into later without touching the CPU again. Nothing *uses* it in this pivot.

## D6 — The Mega Drive memory map, open bus, and the TAS quirk

`MegaDriveBus` gives **every** 24-bit address a deterministic answer:

| Range | Behavior (this pivot) |
|---|---|
| `$000000–$3FFFFF` | ROM (read-only; short ROMs: open bus past the end — no mirroring assumption) |
| `$400000–$7FFFFF` | open bus |
| `$A00000–$A0FFFF` | Z80 address space: 8 KiB Z80 RAM as plain bytes (68k side only; nothing executes) |
| `$A10000–$A1001F` | I/O: version register returns a fixed NTSC/no-disk value; controller ports return a documented constant (real pads land Phase 2) |
| `$A11100/$A11200` | Z80 BUSREQ/RESET latches: writable, readable, bus always "granted" (so boot code proceeds); arbitration *timing* deferred |
| `$C00000–$C0000F` | VDP ports: deterministic stub — writes latch into a small placeholder block, status reads return a fixed empty-FIFO/VBlank-clear constant; replaced wholesale by the ratified VDP design |
| `$E00000–$FFFFFF` | 64 KiB work RAM, mirrored across the whole window |

- **Open bus** returns the last word that crossed the bus (`last_bus_word: u16`, one
  serialized field, updated on every access). Cheap, deterministic, and closer to hardware
  than a constant; refined only if a differential ever cares.
- **The TAS write-drop quirk**: on the Mega Drive the RMW write cycle of `TAS` is not
  honored by the bus controller — the read happens, the write is dropped (the famous
  Gargoyles/Ex-Mutants behavior). This is a *bus* property, not a CPU property:
  `MegaDriveBus::tas()` performs the read, skips the store, still logs the `Tas` event.
  `FlatBus` keeps the datasheet behavior (SST pins it). One focused test pins each.
- **Reset**: `System::reset` runs the real power-on sequence — the group-0 reset recipe
  (fetch SSP from `$0`, PC from `$4`, S set, T clear, I=7, then the two-prefetch fill),
  as a micro-op recipe like everything else. This establishes the prefetch invariant the
  decoder relies on, from the machine's own vector table.
- **ROM in snapshots**: `System` owns `rom: Vec<u8>` and it IS included in `Clone`/bincode
  for now — correctness and O(struct) snapshotting first; if Phase-2 rewind makes 4 MiB/frame
  hurt, the ROM region moves behind a checksum + reattach seam *then* (snapshot policy 5
  already voids cross-version compat, so this is a free future change).

## D7 — Scheduler wiring, `run_until`, and the determinism gate on the real CPU

- **Clock conversion**: the 68000 runs at mclk/7. `step()` returns CPU cycles; the System
  advances `mclk += cycles * 7`. All scheduling stays in mclk (the sole clock). The
  `exec_one`/`run_to_completion` docs currently say "master cycles" — **rename to CPU
  cycles** at the seam, or someone wires a silent 7× timing bug (review finding).
- **The run loop** gets its real primitive: `System::run_until(deadline_mclk)` — pop due
  scheduler events (which may set the IPL latch), step the CPU, repeat until the deadline;
  `run_frames(n)` = `run_until(now + n × MCLK_PER_FRAME)`. A CPU step may overshoot the
  deadline by up to one instruction — that is the ratified bus-access/sync-on-demand model
  (a debugger touch quiesces to a bus boundary; frame edges don't need sub-instruction
  precision). The overshoot carries into the next slice so long-run time stays exact.
  Worst-case overshoot is one micro-op inside one instruction — up to ~150 CPU cycles
  (~1,050 mclk) for the self-booking DIV arms and `Internal{124}` for RESET; the VDP design
  must budget for this (flagged there).
- **`StubCpu` and `m68000::prototype` are deleted** (both are documented placeholders; the
  prototype's job ended when the cycle-granularity brief was ratified).
- **The determinism gate upgrades its currency.** With the stub gone and no VDP yet, the
  Oracle-compatible VDP `state_hash` would be constant. The gate (two instances, 120
  frames, byte-identical sequence) now hashes **`export_state` v1** (D8) — CPU regs + RAM +
  placeholders — which is strictly stronger. The Oracle-compatible `state_hash` remains
  for the live-Oracle differential, unchanged.
- **A vendored test ROM** (hand-authored bytes, built in-test: vector table + a loop that
  exercises RAM, an interrupt handler, STOP, an illegal-instruction handler) gives the gate
  and the integration tests real code with no toolchain dependency.

## D8 — `export_state` v1: frozen at this pivot (policy 2 comes due)

The canonical cross-backend differential currency, per the audit policy:

- **Version field first**, then fixed region order with fixed sizes; every not-yet-emulated
  chip serializes as a fixed all-zero placeholder region so the layout never shifts as chips
  land: `m68k regs` (d0–d7, a0–a6, usp, ssp, pc, sr, prefetch — the SST vocabulary) →
  `work RAM 0x10000` → `Z80 block` → `VDP block` (vram/cram/vsram/regs at the `state_hash`
  sizes) → `FM block` → `PSG block`.
- **Instruction-boundary export only** in v1 (mid-instruction `MicroState` is snapshot
  territory, not differential territory — BlastEm can't be stopped mid-instruction anyway).
- **The master clock is *not* in the diff currency** (SST-model timing vs BlastEm timing
  will legitimately diverge; per the tiebreak policy those are xfail-manifest entries, not
  state divergences). Time is logged alongside, compared separately.
- Spec doc: `docs/export-state-v1.md` — byte order, offsets, sizes, the version-bump rule.

---

## Sequencing — four pushes, the proven cadence (recon → dated plan → gated implementation)

| Push | Contents | Gate |
|---|---|---|
| **0 — finish the vendored coverage** | ADDQ/SUBQ (+16,529 cases; the one wrinkle: `.w`/`.l` to An is sizes-to-long, no flags) and ADDI/SUBI/ANDI/ORI/EORI (+9,482; the CMPI immediate-capture idiom + the `ea_dst` RMW skeleton, both proven) — vendored data already exists, standard grind cadence. Also: make CI fail (not vacuously pass) when the vendor dir is missing | Suite threshold → **1,000,058** (all vendored cases minus the 2 corrupt); the per-file docs' "no intra-class deferral" claims become true |
| **A — exception/async cluster** (audit finding 3) | D1 orchestrator + states, D2 decode totality, D3 trace, D4 interrupts/STOP. Pure CPU work over `FlatBus`; no `System` changes. | 65536-opcode totality sweep; hand-authored vectors for each new frame (UM/Yacht-pinned streams + cycles, both drivers, snapshot at every boundary — the SST discipline without SST); full SST suite stays green |
| **B — bus + memory map** | D5 unification (BusEvent+fc/Tas, one `Size`), D6 `MegaDriveBus` + open bus + TAS quirk + reset sequence; delete `prototype.rs` | Map tests per region; reset-sequence bus-stream test; TAS quirk pair; SST green (harness untouched) |
| **C — System wiring** | D7: `run_until`, clock ×7, IPL plumbing from scheduler events, delete `StubCpu`, test ROM, determinism gate on the real CPU | Determinism gate (new currency) as the first CI job, still; proptests (`run_frames(N) == N×1`, snapshot/restore) on real execution |
| **D — export_state freeze** | D8 + spec doc; determinism-gate currency switch lands here (with C if convenient) | Golden-layout test (offsets/sizes pinned); gate green |

Push 0 first (it's the proven cadence and removes the biggest real-ROM panic class); then
A is independent of B; B before C; D rides with C. After D, **the macro-RTC perf-pass
trigger fires** (policy 7) — before any differential-fleet workload. The BlastEm behavioral
differential harness (lockstep over `export_state`) is the natural *next* brief after the
pivot, and push A's recon should already use manual BlastEm-over-the-bus experiments to pin
interrupt timing.

## Risks

- **No SST safety net for the async cluster** — the whole point of finding 3. Mitigation:
  recon-grade pushes with UM/Yacht-pinned hand vectors, behavioral BlastEm cross-checks, and
  the same both-drivers/snapshot-everywhere discipline the grind used.
- **Exception-priority subtleties** (trace vs interrupt vs stopped, T-latch edge cases,
  double-fault) are classic emulator bug territory. Mitigation: they live in ONE function
  (`begin_next`), property-tested; states are plain serialized data.
- **The VDP stub could quietly become load-bearing.** Its constants are documented as
  placeholder, it lives behind the region dispatch the real VDP replaces, and nothing in the
  gate depends on its values beyond determinism.
- **Scope creep toward bus-arbitration timing** (Z80 BUSREQ stalls, DMA steal). Explicitly
  deferred to the VDP push; the seam (deferred writes + the stall channel + the scheduler)
  is defined here so deferral costs nothing.

## Opportunistic cleanups (do while in the area, per review)

- `m68000/mod.rs` module doc still says "Phase-0 vertical slice" — stale; rewrite at Push A.
- The SST stream's `'n'` idle tokens are pinned only in aggregate (total length), not by
  position; acceptable, but note it in the harness header so nobody assumes placement is
  asserted.
- Recipes bake decode-time state (live CCR for Bcc/Scc/DBcc/TRAPV, the MOVEM mask) — they
  are **not cacheable per opcode** and decode must happen exactly at the instruction
  boundary. Document as an invariant where the macro-RTC pass will look.
