# Push A / A3.2 + docket: the BlastEm-over-the-bus differential harness

**Status: BUILT + RUN 2026-07-16.** Stands up the one BlastEm-over-the-bus differential
harness (the last Push-A item, scheduled immediately before Push C because it is the same
infrastructure C's nightly differential needs), resolves the A3.2 STOP×trace fork, and
clears the whole differential docket in one pass. Harness lives in
`tools/blastem-differential/`; the STOP×trace pin is wired into `step()`.

Clean-room: BlastEm (GPL-3) is driven strictly as a black-box oracle over a standard
protocol; its source is never opened (nor jgenesis / Genesis Plus GX). Behavior enters
only through the protocol and the observable side effects of a clean-room harness ROM.

## The mechanism (the one design decision that was never pinned)

"BlastEm-over-the-bus" was the ratified *method*, not an implementation. Recon settled it:

`blastem ROM.bin -D` runs BlastEm as a **GDB Remote Serial Protocol (RSP) stub over stdio**
— it halts the 68000 at the ROM entry point and hands control to a GDB-remote client. We
drive it with a small clean-room Python RSP client (`tools/blastem-differential/rsp.py`):
read/write the 68000's registers and memory, single-step, breakpoint, continue. That is
"over the bus" in its most literal form — we observe the bus-visible CPU state directly.

- Launched under `xvfb-run` (isolated disposable headless X per run; repeated windowed
  sessions on a shared `:0` are unreliable), `-g` (SDL software renderer),
  `SDL_AUDIODRIVER=dummy`.
- The interactive `-d` debugger is **not** viable headless (it spawns an external
  `x-terminal-emulator`); the RSP stub is the scriptable, deterministic path.
- Stub quirks (BlastEm 0.6.2, discovered black-box): `g` = 18×32-bit `d0-7,a0-7,sr,pc`;
  `G` (write-all) crashes → use `P` (write-one); `P` for **pc** returns `E01` → PC is driven
  indirectly by a **RAM-dispatch ROM** (CPU halts at `Init`; the driver writes a RAM control
  block + breakpoints + continues; the ROM dispatches); a timed-out command may be answered
  late, so every command drains stale input first (request/reply stay in lockstep).

This mechanism is reusable verbatim for the Push C nightly differential.

## A3.2 — STOP × trace, resolved

STOP uniquely modifies SR (T) as its effect, *before* its own stop/trace decision, so
§6.3.8's uniform "T at instruction start" rule is ambiguous for it. The candidate rules:
- **start-T** (M68000UM §6.3.8 uniform): the T *before* STOP loads the immediate.
- **loaded-T** (M68000 PRM STOP description + owner's prior pin): the T of STOP's immediate.

### The differential (full 2×2 + controls)

Two T bits: start-T (SR before STOP, set by the ROM's `move d0,sr`) × loaded-T (STOP's
immediate). Per cell we observe: does a trace frame appear, the stacked PC, the final state.

```
control  start-T=0  NOP  -> FELL-THROUGH   (no trace)                  [detector negative]
control  start-T=1  NOP  -> TRACE          (stackedPC=$23A=next, SR=$A700) [detector positive]

         start-T  loaded-T   BlastEm 0.6.2
            0        0        STOPPED
            0        1        STOPPED
            1        0        STOPPED
            1        1        STOPPED
```

**BlastEm never traces STOP — for any T.** The NOP controls prove the harness detects a
trace correctly (it stacks the right next-instruction PC and the T-set SR), so this is a
**BlastEm instrument blind spot, recorded as a limitation — NOT a pin.** BlastEm 0.6.2 does
not model trace-on-STOP at all: it fails even the (start-T=1, loaded-T=1) cell where *every*
candidate rule agrees a trace must occur. The two diagonal cells — (0,1) and (1,0) — are
exactly where the uniform-UM and loaded-T rules diverge, so a faithful instrument would have
discriminated there; BlastEm discriminates nowhere.

### The pin: loaded-T (trace preempts stop) — owner decision 2026-07-16

STOP with T set in the **loaded** SR services a trace exception **instead of** stopping —
*trace preempts stop*. start-T is irrelevant to STOP's trace decision. Verbatim references:

- **M68000 PRM, STOP** (the STOP-specific rule that governs): *"The immediate operand is
  copied into the entire status register (i.e., both status byte and CCR are modified), and
  the program counter advanced to point to the next instruction to be executed. … The
  execution of instructions resumes when a trace, an interrupt, or a reset exception occurs.
  **A trace exception will occur if the trace bit is set when the STOP instruction is
  encountered.**"* The sentence keys the trace off the SR *after* the immediate was copied in
  (mirroring the very next clause, which keys the privilege check off "the bit of the
  immediate data corresponding to the S-bit"). → the **loaded** T governs.
- **M68000UM §6.3.8** (the uniform rule STOP is the exception to): *"If the T bit is set (on)
  at the beginning of the execution of an instruction, a trace exception is generated after
  the instruction is completed. … The saved value of the program counter is the address of
  the next instruction."*

Truth table pinned (loaded-T):

| start-T | loaded-T | outcome  | note |
|:---:|:---:|:---|:---|
| 0 | 0 | STOPPED | |
| 0 | 1 | **TRACE** | discriminator — traces-not-stops (hard-asserted) |
| 1 | 0 | **STOPPED** | discriminator — stops-not-traces (hard-asserted) |
| 1 | 1 | TRACE | uniform-UM and loaded-T agree; BlastEm still STOPPED |

### Wiring (`step()`), and one frame smell to note

The current `step()` sets both `Stopped` and `trace_pending` for a start-T=1 STOP, which
hangs (the `Stopped` arm returns before the trace is serviced). The pin resolves it: after a
`requests_stop()` recipe, branch on the **loaded** T (= `regs.sr & SR_TRACE` *after* STOP's
`LoadSr` ran):
- loaded-T set → pend the trace, do **not** enter `Stopped` (trace preempts stop);
- loaded-T clear → enter `Stopped`, do **not** pend a trace.
Non-STOP instructions keep the existing start-T (`trace_armed`) latch.

**Frame details — condition-4 report (one special-case is needed):**
- *pushed SR = the loaded SR (with T)* → falls out cleanly: `EnterException` saves the live
  `regs.sr`, which STOP's `LoadSr` already set to the immediate.
- *stacked PC = post-STOP (next instruction)* → does **NOT** fall out cleanly. `stop_recipe`
  (`LoadSr` + `Stop`) does no prefetch, so at completion `regs.pc` still points at the STOP
  opcode. The trace path must advance `pc += 4` past the 2-word STOP before servicing the
  trace — **identical to the A4.2 STOP-wake's `pc += 4`**. This is a small, precedented
  special-case (it is exactly where oracle-next chooses to apply the PRM's "program counter
  advanced to point to the next instruction"), but it *is* a special-case, reported here per
  the owner's condition 4.

## Differential docket — every item resolved

The ratified tiebreak resolves what `docs/reference/` leaves silent/contradictory. Batched
into this one harness session:

1. **STOP × trace 2×2** — **PINNED** (loaded-T, above). Differential ran; BlastEm blind
   (limitation recorded in `known_differences.py`); pin from PRM + owner decision. Wired.
2. **STOP cycle-2 idle structure** (Yacht L908-911: "no real clues how the second microcycle
   is spent") — **CONFIRMED-DEFERRED (pure timing).** The RSP stub exposes architectural
   state, not cycle-level bus timing; STOP's *behavior* is pinned, only the cycle-2 idle
   placement is open. Not observable through this instrument → stays timing-deferred/
   xfail-manifest.
3. **A4 residue:**
   - **IACK interleave placement** (exact `ni`/`n-` position within the `s S s` write stream)
     — **CONFIRMED-DEFERRED (pure bus-order timing).** RSP exposes final memory/register
     state, not intra-instruction bus ordering; only a cycle-exact bus trace would show it.
   - **Level-7 NMI edge** (`ipl == mask == 7`) vs the pure level comparison — **CONFIRMED-
     DEFERRED / not naturally reachable on MD.** The Mega Drive has no level-7 interrupt
     source; the strict `ipl > mask` comparison (§6.3.2) is already pinned and correct for
     every MD-reachable level. The L7-specific NMI edge is a bare-68000 concern the
     MegaDriveBus never exercises. Revisit only if a future test ROM injects L7.
   - **STOP-wait per-poll idle cadence** (`STOPPED_IDLE_SLICE`) — **CONFIRMED-DEFERRED (pure
     timing).** Explicitly an un-pinned `run_until` progress device, not hardware timing.
   - **spurious / uninitialized vectors (24 / 15)** — **CONFIRMED-DEFERRED / not naturally
     reachable.** MD autovectors (VPA) so vector 15 (uninitialized) is not hit; vector 24
     (spurious) needs a bus error during IACK, which the MegaDriveBus does not generate. No
     current ROM exercises them.
4. **Push-B review item — 68k word-access-to-Z80-RAM quirk** — **CONFIRMED-DEFERRED
   (documented placeholder retained).** The Z80 bus is 8-bit, so a 68000 *word* write to
   `$A00000-$A0FFFF` does not store both bytes as a normal word (permissive docs:
   Plutiedev / SpritesMind). No oracle-next consumer differentiates on it yet; the
   MegaDriveBus keeps its documented placeholder. Behaviorally testable via this harness if a
   future differential cares.
5. **Push-B review item — `MegaDriveBus::tas` open-bus latch value** — **CONFIRMED-DEFERRED
   (internal modeling nit).** The open-bus latch value after a TAS write-drop is "what was
   last driven," which is not cleanly hardware-observable and has no current consumer.
   Documented placeholder retained.

Only pure-timing / unreachable / no-consumer residue stays deferred (allowed). The one
behaviorally-observable-and-consumed fork — STOP×trace — is pinned with hand-authored tests.

## Anti-cheating / invariants

- **SST is ground truth and untouched.** `step()`/`trace_pending`/STOP+trace are off the
  vendored path (every SST case is supervisor, T=0 throughout, and never traces). The gate
  stays at exactly `ran >= 1_000_058`, re-run green before each commit.
- The pin contradicts BlastEm's *measured* output on purpose — recorded as an instrument
  limitation in `known_differences.py`, not silently. Pinned from the PRM (a permissive doc),
  the ratified fallback when the differential instrument is blind.
- Hand tests assert all four cells; the two diagonal discriminators are hard-asserted.

## Commits

1. `docs(plan)` — this document.
2. `test(m68000)` / harness — `tools/blastem-differential/` (RSP client, harness ROM +
   pre-built `harness.bin`/`.lst`, experiment driver, known-differences ledger, README).
3. `feat(m68000)` — the A3.2 wiring (loaded-T trace-preempts-stop + `pc += 4`) and the
   hand-authored both-driver STOP×trace tests.
