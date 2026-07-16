# BlastEm-over-the-bus behavioral differential harness

The reproducible instrument for resolving 68000 behavioral forks that the in-tree
references (`docs/reference/Yacht.txt`, `M68000UM.pdf`) leave silent or conflicting —
by observing **real BlastEm behavior over the bus**, never its source. This is the
same infrastructure the Push C nightly differential will reuse.

**Clean-room:** BlastEm (GPL-3) is driven strictly as a black-box oracle via a
standard protocol. Its source is never opened. Same for jgenesis / Genesis Plus GX.
If a differential result is confusing, the answer is *more probes*, never their code.

## Mechanism (settled 2026-07-16)

`blastem ROM.bin -D` runs BlastEm as a **GDB Remote Serial Protocol (RSP) stub over
stdio** — it halts the 68000 at the ROM entry point and hands control to a GDB-remote
client. We drive it with a small clean-room RSP client (`rsp.py`): read/write the
68000's registers and memory, single-step, set breakpoints, continue. This *is*
"BlastEm over the bus" in its most literal form.

- BlastEm is launched under `xvfb-run` (isolated, disposable headless X per run —
  repeated windowed sessions on a shared `:0` are unreliable) with `-g` (SDL software
  renderer) and `SDL_AUDIODRIVER=dummy`.
- Empirical facts about this stub (BlastEm 0.6.2, black-box):
  - `g` returns 18 x 32-bit words: `d0-d7, a0-a7, sr, pc`.
  - `G` (write-all-registers) **crashes** the stub → use `P` (write one register).
  - `P` for **pc** (reg 17) returns `E01` (unsupported) → PC is driven indirectly via
    a **RAM-dispatch ROM** (`harness.asm`): the CPU halts at `Init`, we write a RAM
    control block + set breakpoints + continue, and the ROM dispatches.
  - A timed-out command may still be answered *late*; every command drains stale
    input first to keep request/reply in lockstep.

## Files

| file | role |
|---|---|
| `rsp.py` | clean-room GDB-RSP client (the transport) |
| `harness.asm` | clean-room RAM-dispatch harness ROM (the instrument) |
| `build_rom.sh` | assemble `harness.bin` via the native `asl`/`p2bin` (aeon tools) |
| `run_stop_trace.py` | the STOP×trace 2×2 experiment (+ NOP controls) |
| `vdp_pending.asm` / `build_vdp_pending.sh` / `run_vdp_pending.py` | the VDP control-port pending-toggle experiment (recon R1) |
| `nightly_differential.py` | the Push C nightly job — instruction-boundary state differential |
| `known_differences.py` | ledger the nightly differential imports to avoid false alarms |

`harness.bin` / `harness.lst` are committed pre-built; regenerate with `build_rom.sh`.

## Nightly differential (Push C)

`nightly_differential.py` runs a fixed **register-only** 68000 sequence (a register-init
prologue + an ALU/shift/logic body) in both emulators and compares the **architectural
state at every instruction boundary** (d0-7, a0-7, pc, sr):

- oracle-next side: the `differential_trace` example (`cargo run --example
  differential_trace -- <rom> <n>`) emits one JSON state per boundary.
- BlastEm side: breakpoints + `continue` over the RSP stub (the reliable path — the stub's
  single-step `s` is unstable, and it crashes on register writes, so the ROM prologue aligns
  the register file instead of `P`).

Only architectural state is compared. **Timing is never a state divergence** — SST-model
cycles vs BlastEm cycles legitimately differ and are xfail-manifest entries (D8). Scenarios in
`known_differences.py` (e.g. STOP×trace) are treated as expected, not regressions. It is an
*instrument, not a merge gate*: it prints `SKIPPED` and exits 0 where BlastEm/xvfb are absent
(GitHub-hosted CI), and runs nightly via `.github/workflows/nightly-differential.yml`.

First recorded result (2026-07-16, local, BlastEm 0.6.2): **PASS — 13 instruction boundaries
agree on all architectural state.**

```bash
cd tools/blastem-differential
python3 nightly_differential.py           # builds the ROM, runs oracle-next + BlastEm, diffs
```

## Reproduce

```bash
cd tools/blastem-differential
./build_rom.sh                 # -> harness.bin (+ harness.lst label table)
python3 run_stop_trace.py      # runs BlastEm; prints the 2x2 + controls
```
Requires: the BlastEm binary (`emulators/blastem64-0.6.2/blastem`, override via
`$BLASTEM`), `xvfb-run`, and the aeon `asl`/`p2bin` for `build_rom.sh` (override via
`$TOOLS`). Individual runs self-terminate via a watchdog.

## Recorded result — STOP × trace (2026-07-16)

Two T bits: `start-T` (SR just before STOP, set by the ROM's `move d0,sr`) × `loaded-T`
(the T bit of STOP's immediate). Per cell we observe whether a **trace frame** appears,
its **stacked PC**, and the final **CpuState**.

```
control  start-T=0  NOP  -> FELL-THROUGH   (no trace)              [detector negative]
control  start-T=1  NOP  -> TRACE          (stacked PC=next, SR=A700) [detector positive]

         start-T  loaded-T   BlastEm
            0        0        STOPPED
            0        1        STOPPED
            1        0        STOPPED
            1        1        STOPPED
```

**BlastEm never traces STOP — for any T.** The NOP controls prove the harness detects a
trace correctly, so this is a **BlastEm instrument blind spot**, not hardware truth:
BlastEm 0.6.2 does not model the trace-on-STOP quirk (it fails even the (1,1) cell where
every candidate rule agrees a trace should occur).

**oracle-next pins the loaded-T rule** (owner decision 2026-07-16), per the M68000 PRM
STOP description — *"The immediate operand is copied into the entire status register …
A trace exception will occur if the trace bit is set when the STOP instruction is
encountered"* — i.e. STOP with T set in the **loaded** SR traces instead of stopping
(*trace preempts stop*). See `docs/plans/2026-07-16-m68000-blastem-differential.md`.
The divergence is recorded in `known_differences.py` so the nightly differential does
not false-alarm on these cells.

## Recorded result — VDP control-port pending toggle (2026-07-16)

VDP recon R1's open cell: which accesses clear the control port's first/second-write
toggle? Permitted docs pin the data-port-write clear and are silent on status/HV reads.
Per cell, `vdp_pending.asm` arms the toggle, applies one probe, then writes an ambiguous
word whose interpretation routes a `$BBBB` sentinel to VRAM `$0200` (toggle survived)
or `$0300` (toggle cleared); results are read back through the data port into work RAM.

```
sel=0 control (no probe) -> $0200=BBBB  pending PERSISTS   (validates the discriminator)
sel=1 status read        -> $0300=BBBB  status read CLEARS the toggle
sel=2 HV counter read    -> $0200=BBBB  HV read does NOT clear it
sel=3 data-port write    -> $0300=BBBB  data write CLEARS it (doc-pinned, confirmed)
```

**Instrument-sourced pin** (BlastEm 0.6.2, same standing as the STOP×trace pin): a
status read clears the toggle; an HV read does not. Recorded in
`docs/2026-07-16-vdp-recon.md` (R1) and the amended `docs/2026-07-01-vdp-design.md`.
