# Push C — System wiring (integration pivot D7)

**Status: PLAN, 2026-07-16.** Design of record: `docs/decisions/2026-07-15-integration-pivot-design.md`
§D7 (read §D8 — export_state — which C builds the *currency* for but does NOT freeze). Follows Push 0
(vendored coverage, threshold 1,000,058), Push A (exception/async cluster), Push B (bus unification + MD
map + power-on reset recipe), and the BlastEm-over-the-bus differential harness slice.

## Goal

Wire the real `Cpu68000` into `System` and make the machine *run real code on the Mega Drive bus*:
the mclk/7 clock conversion, `System::run_until(deadline_mclk)` with exact overshoot carry, IPL + reset
plumbing from scheduler events into the already-built `begin_next` arms, deletion of `StubCpu`, a
hand-authored vendored test ROM, the determinism gate re-homed onto a working `export_state`, and the
nightly BlastEm differential job.

## What already exists (Push A/B — do NOT rebuild)

- `Cpu68000::step(&mut impl Bus68k) -> u32` **returns CPU cycles** and already contains the entire
  `begin_next` priority dispatch: reset (group 0) > STOP-wake > trace > interrupt > decode. All arms are
  built and unit-tested standalone (`microop.rs`).
- `Cpu68000::set_ipl(level)`, `assert_reset()`, the `reset_pending`/`ipl`/`trace_pending` latches,
  `CpuState{Normal,Stopped,Halted}`, `STOPPED_IDLE_SLICE` (the stopped-CPU `run_until` progress granule).
- `reset_exception_recipe()` — the power-on `/RESET`: forces SR=0x2700, reads SSP@`$0/$2` and PC@`$4/$6`
  (all FC=6), primes the prefetch queue. Every instruction recipe ends in `Prefetch` ops, so the queue is
  **self-sustaining** after reset — continuous execution needs no per-instruction fetch bookkeeping.
- `MegaDriveBus` (`bus.rs`) implements `Bus68k` over `System`'s `rom/ram/z80_ram/last_bus_word`, writes
  work RAM / Z80 RAM directly (no deferred seam), full MD memory map. `System::mega_bus(sink)` split-borrow.
- `System` owns `rom` (preserved across reset), `ram`, `z80_ram`, `last_bus_word`, the VDP memories, and
  the `Scheduler` (sole mclk + RNG). `System::load_rom` / `rom()` exist.

## The traps (flagged in the brief — pinned here)

1. **The ×7 conversion lives in exactly ONE place** — `run_until`'s step loop (`mclk += cycles * 7`).
   If a `* 7` appears anywhere else, it is a bug. `MCLK_PER_CPU_CYCLE = 7` is a named constant.
2. **Deleting `StubCpu` and switching the gate currency are SEPARATE slices** so a determinism-hash change
   is attributable to exactly one cause (currency switch vs. real-CPU execution).
3. **Every SST invariant is untouchable**: the SST harness keeps driving `FlatBus` directly; threshold
   stays `ran >= 1_000_058`; no assert weakened; `FlatBus` returns wait 0 so every SST stream is
   bit-identical. Push C touches only the `System` side + the determinism gate.

## The `master cycles` rename (review finding — silent 7× bug trap)

`step()` correctly documents "CPU cycles", but `exec_one`/`run_to_completion`/`Step::Done` and several
bus/EA doc comments + test-assertion strings say **"master cycles"** for the same 68000-clock quantity.
Left as-is, a later reader wires `mclk += cycles` (missing ×7) or `cycles * 7` in the wrong layer. Slice 1
renames every "master cycle(s)" that denotes a CPU-clock count to "CPU cycle(s)", as its own early slice so
all later slices build on unambiguous vocabulary. Sites (from `grep -rn "master cycle"`):
`bus68k.rs:43`, `ea.rs:501`, `microop.rs:{1026,1054,1507,1518,3059}`, test messages
`microop.rs:{3238,3718,4169}` ("a word/byte bus access is 4 master cycles" → "4 CPU cycles").
Doc/string-only, zero behavior change — TDD exemption (comments); verified by a fully-green suite.

## `export_state` currency (D8 region order — build, do NOT freeze)

`System::export_state() -> Vec<u8>` — the gate currency, laid out in **D8's region order** with fixed
sizes so the layout never shifts as chips land. Push D freezes v1 + writes `docs/export-state-v1.md`;
here the placeholder sizes are named consts, provisional-pending-D. Layout:

| Region | Bytes | Source (this push) |
|---|---|---|
| `version: u16` LE | 2 | constant `EXPORT_STATE_VERSION = 1` |
| m68k regs | 4·8 (d0–7) + 4·7 (a0–6) + 4 (usp) + 4 (ssp) + 4 (pc) + 2 (sr) + 2·2 (prefetch) = **74** | real `cpu.regs` (LE); zero placeholder while `StubCpu` (slice 3) |
| work RAM | `RAM_SIZE` = 0x10000 | real `ram` (the one region that meaningfully evolves) |
| Z80 block | `Z80_RAM_SIZE` = 0x2000 | all-zero placeholder |
| VDP block | VRAM 0x10000 + CRAM 0x80 + VSRAM 0x50 + REGS 24 | all-zero placeholder |
| FM block | `FM_PLACEHOLDER = 0x200` | all-zero placeholder |
| PSG block | `PSG_PLACEHOLDER = 0x10` | all-zero placeholder |

- **Zero placeholders** (not the seeded VDP memory) per the brief: "CPU regs + work RAM + fixed all-zero
  placeholder regions". The Oracle-compatible `state_hash` (frozen FNV-1a VDP layout,
  `state-hash-byte-layout`) is **untouched** — it stays for the live-Oracle differential.
- `export_state_hash() -> u64` = an **independent** FNV-1a over the `export_state` bytes (own function,
  same algorithm, so `state_hash.rs` is not touched). The gate compares these per-frame.
- Instruction-boundary only (v1) — `run_frames` leaves the CPU quiesced at an instruction boundary
  (`inflight == None`), so `export_state` never captures a mid-instruction `MicroState`.

## The vendored test ROM (`testrom.rs`)

Hand-authored 68000 machine-code bytes built in-test — **no toolchain dependency**. `#[doc(hidden)] pub
fn build() -> Vec<u8>` so the determinism gate (integration binary), the `System` unit tests, and the
integration tests all share it. Every opcode is verified by the already-SST-proven decoder: the ROM's
*behavior* under the real CPU is asserted, which is the ground truth for opcode correctness.

Structure (little-endian words; ROM base `$000000`):

```
$000000  dc.l $00FFFFFE      ; [0] initial SSP — top of work RAM (even)
$000004  dc.l $00000200      ; [1] initial PC  — code start
$000010  dc.l ILLEGAL_H      ; vector 4  (illegal instruction)  = $00000280
$000078  dc.l INT_H          ; vector 30 (autovector level 6, VInt) = $000002A0
; (all other vectors default to whatever ROM byte lands there — never taken in the gate)

$000200  main:                               ; entered from reset
         move.w #$2000, SR    ; 46FC 2000    ; supervisor, T=0, INT mask 0 (enable interrupts)
$000204  reload:
         lea    $00FF0000, A0 ; 41F9 00FF 0000; A0 = work-RAM base
         move.w #$7FFF, D1    ; 323C 7FFF    ; 0x8000 words to stir
$00020C  inner:
         move.w (A0), D0      ; 3010
         addq.w #1, D0        ; 5240
         move.w D0, (A0)+     ; 30C0         ; += 1, advance
         dbra   D1, inner     ; 51C9 FFF6    ; branch back to inner
         bra.w  reload        ; 6000 FFF2    ; forever (each pass +1 to every RAM word)

$000280  ILLEGAL_H:
         move.w #$DEAD, $FF0000 ; 31FC DEAD 0000 ; sentinel, proves illegal entry
         stop   #$2700        ; 4E72 2700    ; park (never returns; exercises STOP)

$0002A0  INT_H:
         move.w #$1234, $FF0002 ; 33FC 1234 FF0002... (MOVE.W #imm,(xxx).L) 33FC 1234 00FF 0002
         rte                  ; 4E73         ; sentinel then return; proves interrupt entry
```

- The **main loop** stirs all 0x8000 work-RAM words by +1 per pass, reloading A0 each pass so it stays in
  RAM — work RAM (seed-seeded at power-on) evolves deterministically every frame. This is what gives the
  determinism gate teeth *and* per-frame evolution.
- **STOP** (in the illegal handler) and the **illegal handler** + **interrupt handler** are present and
  decodable (design requirement). The gate runs at reset-then-`move #$2000,SR` = INT mask 0; with **no
  events scheduled** nothing is taken, so the gate path is the pure RAM stirrer (deterministic).
- The **interrupt handler** is exercised by slice 5's test (schedule a VInt at level 6 → mask 0 < 6 → taken
  → writes `$1234` at `$FF0002` → RTE). The illegal handler is exercised by an integration test feeding an
  illegal opcode.

Exact byte assembly is emitted by `build()` (a `Vec<u8>` with `push_word`/`push_long` helpers, one word per
source line above, commented identically). A unit test asserts: `[0]==0x00FFFFFE`, `[1]==0x00000200`,
vector-4 and vector-30 slots, and the first few opcode words — plus a *behavioral* test that runs it under
the real CPU and asserts a RAM word incremented.

## Slices (each: TDD, one conventional commit, full triplet + SST re-run, fmt/clippy clean)

### Slice 1 — `docs(m68000): rename "master cycles" → "CPU cycles" at the clock seam`
Rename at the 8 doc sites + 3 test-message sites listed above. No behavior. Verify: full `cargo test
--workspace` green (SST included), fmt/clippy clean.

### Slice 2 — `feat(oracle-core): hand-authored vendored test ROM fixture`
- New `src/testrom.rs`: `#[doc(hidden)] pub fn build() -> Vec<u8>` per the layout above + `push_word`/
  `push_long`. `pub mod testrom;` in `lib.rs`.
- Tests (in `testrom.rs`): vector table words, opcode-word spot checks, ROM length ≥ `$2B0`.
- No `System`/gate change yet. Green.

### Slice 3 — `feat(oracle-core): export_state gate currency (D8 region order), still on StubCpu`
- `system.rs`: `EXPORT_STATE_VERSION`, region-size consts, `export_state()` (regs region = **zero
  placeholder** — StubCpu has no 68k regs), `export_state_hash()` (independent FNV-1a).
- `determinism_gate.rs`: capture `sys.export_state_hash()` per frame instead of `state_hash().combined`.
- Tests: `export_state` length == sum of region sizes; version bytes; a work-RAM byte appears at the right
  offset; `export_state_hash` deterministic for a seed and differs across seeds. Gate green (sequence is
  constant-per-run under StubCpu — StubCpu writes VRAM, not work RAM — but `gate_detects_divergence` holds
  because power-on work RAM is seed-seeded). **Hash value changes here = attributable to the currency
  switch.**

### Slice 4 — `feat(oracle-core): run the real 68000 — run_until, ×7 clock, delete StubCpu`
- `system.rs`:
  - Replace `cpu: StubCpu` → `cpu: Cpu68000` (constructed with a zeroed `Registers`). Add
    `frame_boundary_mclk: u64` (serialized; the overshoot-carry anchor). Drop `STUB_STEPS_PER_FRAME`.
  - `MCLK_PER_CPU_CYCLE = 7`.
  - `step_cpu<S: BusEventSink>(&mut self, sink: &mut S) -> u32` — destructure `System` into `cpu` + the
    memory fields, build `MegaDriveBus`, `cpu.step(&mut bus)` (no `apply_writes` — MegaDriveBus is direct).
  - `run_until(deadline_mclk)`: `while self.scheduler.now() < deadline_mclk { let c = self.step_cpu(&mut
    ()); self.scheduler.advance(c as u64 * MCLK_PER_CPU_CYCLE); }` — the **only** ×7 site. A step may
    overshoot by ≤ one instruction; that overshoot is carried by `frame_boundary_mclk`.
  - `run_frames(n)`: `let target = self.frame_boundary_mclk + n * MCLK_PER_FRAME; self.run_until(target);
    self.frame_boundary_mclk = target;` — deadlines are **absolute** frame boundaries, so overshoot from
    frame k is absorbed in frame k+1 and long-run time stays exact.
  - `reset()`: preserve `rom`, `*self = Self::new(seed)` (fresh RAM/CPU, `frame_boundary_mclk = 0`), restore
    `rom`, then drive the power-on reset: `self.cpu.assert_reset(); self.step_cpu(&mut ());` (cycles
    discarded — reset is the mclk-0 anchor, clock not advanced). Prefetch is now primed from the ROM vector
    table.
  - `export_state()` regs region now serializes real `self.cpu.regs`.
- Delete `src/stub_cpu.rs` + `pub mod stub_cpu;` + `step_chip`'s StubCpu path. (`prototype.rs` already gone,
  B1.)
- Update `system.rs` tests: replace `run_frames_evolves_state`/`step_chip_*` with real-CPU equivalents that
  `load_rom(testrom::build())` + `reset()`; `run_frames_advances_master_clock` asserts the **boundary
  invariant** (`frame_boundary_mclk == n*MCLK_PER_FRAME`, and `now()` in `[target, target + slack)`), not
  exact equality (overshoot). Add an overshoot-carry test: 100× `run_frames(1)` leaves
  `frame_boundary_mclk == 100*MCLK_PER_FRAME` and `now()` within one-instruction slack of it.
- `determinism_gate.rs`: `load_rom(testrom::build())` before `reset()`; now the export_state sequence
  **evolves** per frame and stays byte-identical across instances. **Sequence change here = attributable to
  the CPU swap.**
- SST harness untouched (still `FlatBus`). Full triplet + SST green.

### Slice 5 — `feat(oracle-core): IPL + reset event plumbing into run_until`
- `scheduler.rs`: `pop_due(now_mclk) -> Option<(u64, EventKind)>` — pop the earliest event **iff** its
  deadline `<= now_mclk` (peek-then-remove; leaves not-yet-due events). Test: pops only due events, in order.
- `system.rs` `run_until`: at the top of each iteration, drain due events —
  `while let Some((_, kind)) = self.scheduler.pop_due(now) { self.deliver_event(kind); }` — where
  `deliver_event` maps `EventKind::VInt => cpu.set_ipl(6)`, `HInt => cpu.set_ipl(4)`, `Scanline`/`FrameEnd`
  => no IPL (housekeeping). (Nothing *schedules* VInt/HInt in this pivot — the VDP does later; the mapping +
  drain is the plumbing.)
- Tests: schedule a `VInt` inside a frame, run with the test ROM (main loop at INT mask 0) → the interrupt
  is taken (sentinel `$1234` at `$FF0002` appears; `cpu` PC/handler observable). A masked case (mask stays
  7, no `move #$2000,SR`) → not taken. Reset-wake: `assert_reset` from `Stopped` returns to `Normal`
  (already unit-tested at the CPU level; here assert the System-level path). Full triplet + SST green.

### Slice 6 — `ci: nightly BlastEm-over-the-bus differential job`
- `tools/blastem-differential/nightly_differential.py` — reuse `rsp.py` (RSP transport) + `harness.asm`
  RAM-dispatch pattern to run a short instruction sequence in BlastEm, and drive the same sequence in
  oracle-next (via a tiny Rust `--differential` harness or a JSON dump), comparing **instruction-boundary**
  state (regs + touched RAM) only. Import `known_differences.py` so the STOP×trace cells never false-alarm.
  **Timing differences are xfail-manifest entries, never state divergences** (per D8 — the master clock is
  not diff currency).
- `.github/workflows/nightly-differential.yml` — `schedule: cron` nightly; builds oracle-next, sets up
  `xvfb-run` + the BlastEm binary, runs the driver. `continue-on-error`/non-gating (BlastEm availability is
  environment-dependent; this is an instrument, not a merge gate). Run once locally and record the first
  result in the plan + status memory.

## Anti-cheating / invariants (reviewer-enforced)

- Clean-room: BlastEm touched only as a black-box RSP oracle (the harness binary-over-protocol); its source
  is never opened. No jgenesis / GPGX.
- SST: `FlatBus` semantics frozen, threshold `ran >= 1_000_058`, no assert weakened, harness drives FlatBus
  directly (not rerouted through `MegaDriveBus`).
- `state_hash.rs` (Oracle FNV-1a VDP layout) is **not touched** — `export_state_hash` is a separate
  function.
- `* 7` appears at exactly one site (`run_until`). `export_state` is instruction-boundary only.
- Per-commit: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, determinism gate +
  proptests, `cargo test --workspace` (SST sweep ~600–800 s — use a ≥600 000 ms timeout / background).
- Conventional commits, no co-author trailer. Do not `git push` (reviewer verifies + pushes). Never modify
  `../oracle/`.

## Boundary with Push D (do NOT do here)

- No `export_state` **v1 freeze** and no `docs/export-state-v1.md` spec — the placeholder sizes are
  provisional; Push D confirms/freezes the layout.
- No VDP work beyond the existing port stub; no Z80 execution; no perf pass; no bus-arbitration timing.

## Opportunistic cleanups (per the design's review notes)

- `m68000/mod.rs` doc is already refreshed (Push A) — leave it.
- Note in the harness header that SST `'n'` idle tokens are pinned in aggregate only (already documented).
