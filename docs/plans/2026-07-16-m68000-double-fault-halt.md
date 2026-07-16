# Follow-up slice — Halted / double-bus-fault wiring

**Status: PLAN, 2026-07-16.** A reviewer-docket item surfaced during Push C, addressed after the Push D
freeze. Touches the frozen `m68000/*` core, so full discipline: reference-pinned behavior, both drivers,
snapshot-safe, SST re-run.

## The gap

`CpuState::Halted` exists and the reset-wakes-Halted arm is wired + tested
(`reset_wakes_a_halted_cpu`), but **nothing in production ever sets `Halted`**. A group-0 fault whose own
frame-stacking faults again — e.g. a power-on reset over a garbage ROM yielding an odd SSP + odd PC — spins:
`install_address_error` rebuilds the same 14-byte frame (rewinding `step` to 0) every time the frame's odd
write re-faults, so `is_done()` never becomes true and `MicroState.cycles` climbs until it **overflows u32
and panics** (debug) / loops (release). This is crash-on-adversarial-input for the eventual debugger.

## Reference (behavior pinned, not xfail'd)

**M68000UM §5.4.4 Double Bus Fault** (verbatim): *"When a bus error exception occurs, the processor begins
exception processing by stacking information on the supervisor stack. If another bus error occurs during
exception processing (i.e., before execution of another instruction begins) the processor halts and asserts
HALT. This is called a double bus fault. Only an external reset operation can restart a processor halted due
to a double bus fault."* And: *"A double bus fault occurs during a reset operation when a bus error occurs
while the processor is reading the vector table (before the first instruction is executed)."*

On the 68000 an **address error** is a group-0 fault handled like a bus error, so a second address/bus error
while stacking a group-0 frame is a double fault. **Yacht HALTED STATE**: `?(0/0)`, `(n-)*` — *"entered when
a major failure happens and can only be exited by resetting the CPU."* Only reset exits (already wired).

## Design (mirror the STOP-flag pattern)

- **Mark the group-0 frame.** `MicroState` gains `in_group0_frame: bool`, set `true` when
  `install_address_error` builds the 14-byte frame.
- **Detect the double fault.** On *re-entry* to `install_address_error` while `in_group0_frame` is already
  set, do **not** rebuild the frame: set `double_fault = true`, force `step = len` (so `is_done()` and the
  driver loop terminate — bounding `cycles`, killing the overflow), and return 0.
- **Terminal transition.** A `double_faulted()` accessor (twin of `requests_stop()`); both drivers apply it
  at completion exactly where they apply `requests_stop → Stopped`:
  - Driver 2 `step_micro_op`: on `is_done()`, `if recipe.double_faulted() { state = Halted }`.
  - The `step` orchestrator: after every `run_to_completion` arm (reset / trace / interrupt / decode — the
    reset arm matters because the garbage-ROM prefetch faults *inside* the reset recipe), `if
    recipe.double_faulted() { state = Halted }`. Factored into a small `run_terminal` helper so all arms
    share it.
- **Halted executes nothing.** `begin_next` gains a guard *after* the reset check (reset is serviced first,
  the only exit): `if state == Halted { return STOPPED_IDLE_SLICE }` — a Halted CPU consumes a nominal idle
  granule so `run_until` still advances, but decodes/executes nothing. (The idle is a progress device, not
  pinned timing — Yacht's HALTED `?(0/0)` `(n-)*` is explicitly unbounded.)
- **Snapshot-safe.** Both new fields are plain `bool` in the fixed-size bincode `MicroState`/`Cpu68000`, so
  a halted (or mid-double-fault) snapshot round-trips.

## TDD

- **RED (unit):** supervisor regs with an **odd SSP** + an instruction that address-errors (odd write) →
  the first fault stacks at the odd SSP → second fault. Assert `state == Halted`, no panic, bounded cycles.
  Currently overflow-panics.
- **RED (System, the named scenario):** `System::new` + `load_rom(all-0xFF garbage)` + `reset()` → the odd
  reset vector faults during the vector-table/first-fetch → double fault. Assert `Halted`, no panic.
- Both-drivers agreement (run-to-completion vs step-one-micro-op reach the same `Halted`).
- The existing `reset_wakes_a_halted_cpu` already covers the wake arm; add: a Halted CPU **without** reset
  executes nothing (PC unchanged, returns the idle slice); snapshot/restore of a Halted CPU round-trips.

## Gate

Full triplet + fmt + clippy-D + SST re-run (`ran >= 1_000_058` — SST never hits a double fault, always valid
supervisor stacks, so it stays green; this is the both-drivers regression check on the frozen core).

## Out of scope

Bus-error (as opposed to address-error) group-0 faults — the MegaDriveBus never signals a bus error yet
(unmapped space is open-bus, not `/BERR`); the same double-fault machinery will cover them when `/BERR`
lands. Halt-pin electrical modeling (external `/HALT` assertion) — no consumer.
