# PROF-RING-SELF — refused as specified, and now measured rather than argued

**Recommendation: strike `PROF-RING-SELF` as written. Do not build `perFrame[].vintSelfCycles` /
`hintSelfCycles`.** The quantity it would carry is real, exact, cheap — and unrelated to the question the
ask was filed to answer. The column aeon actually needs is one they already read, and it has been correct
since the straddle fix landed.

This document adds one thing to the argument already made in
`docs/2026-08-23-prof-straddle-mechanism.md` §7.3, and it is the thing that was missing: **a live
measurement on a real ROM.** §7's own method note is explicit that the fix and its reasoning were settled
with *"no emulator and no machine — these are synthetic streams into the accumulator."* That was the right
call then; it left two claims resting on construction rather than observation. Both are now observed.

## The measurement

`oracle-aether` at `81ce0a0` on aeon's current `s4.debug.bin` + `s4.debug.lst` (2708 symbols, bound), a
private headless instance. Booted past startup, profiler armed with `perFrame: true`, **119 frames**
sampled, `unattributedCycles: 0`.

### 1. `cyclesSelf` is a constant. It does not know what the handler did.

| bucket | invocations | `cyclesTotal` | per call | `cyclesSelfTotal` | **per call** | total ÷ self |
|---|---|---|---|---|---|---|
| `vint` | 119 | 1,169,721 | **9,829.59** | 5,236 | **44.00** | **223.4×** |
| `hint` | 476 | 244,426 | **513.50** | 20,944 | **44.00** | **11.7×** |

**That is the whole argument, and it is one number.** Two interrupt levels whose real per-invocation cost
differs by **19×** report **exactly the same `cyclesSelf`: 44.00 cycles per invocation, to the hundredth,
on both.** `cyclesSelf` on a bucket is invariant to what the handler does, because a bucket's self time
*is* the exception entry — the handler runs as a routine row nested beneath it, so its whole retirement is
child time and there is no residue to be "the bucket's own work".

A `vintSelfCycles` column would therefore print **44 in every frame of a spike hunt and 44 in every frame
without one.** It is exact, lag-free, cheap to compute, and carries no information about the thing being
hunted — §7.3's *"the worst kind of field to ship, because it looks like the remedy"*, now with a number
attached to it.

### 2. The column they already read is a perfect partition of the aggregate

| | ring sum over 119 rows | bucket aggregate | agreement |
|---|---|---|---|
| VBlank | `Σ perFrame[].vintCycles` = **1,169,721** | `interrupts.vint.cyclesTotal` = **1,169,721** | **100.00%** |
| HBlank | `Σ perFrame[].hintCycles` = **244,426** | `interrupts.hint.cyclesTotal` = **244,426** | **100.00%** |

**Exact, both levels, on a real ROM — not to a tolerance, to the cycle.** Nothing is displaced into a
neighbouring frame and nothing is lost. This is the straddle fix (`4111c88`, merged `51143a5`, in `main`)
doing on live data what it was built to do, confirmed for the first time on a machine.

The ring's own invariant holds too: **`vintCycles < cycles` on 119 of 119 rows** — the pre-fix defect's
signature was a row reporting more interrupt time than the frame retired in total.

### 3. And it is a usable signal, which is the point

| series | min | max | mean | distinct values |
|---|---|---|---|---|
| `perFrame[].vintCycles` | 7,464 | 11,952 | 9,830 | **7** |
| `perFrame[].hintCycles` | 2,054 | 2,054 | 2,054 | 1 |
| `perFrame[].cycles` | 127,994 | 128,016 | — | — |

`vintCycles` moves across a **4,488-cycle range**, ~3.5% of a frame, against a frame total that is flat to
22 cycles. That is exactly the shape a spike hunt reads. **A self column would collapse those 7 distinct
values to 1.** `hintCycles` is flat here only because the scene is stable — 4 HBlanks per frame at 513.5
cycles each — not because the column cannot vary.

## What this changes, and what it does not

- **§7.3's recommendation stands, and is now evidenced rather than derived.** Its reasoning was correct
  from the accumulator's structure alone; nothing here contradicts it. What is new is that the 223×
  ratio and the 100.00% reconciliation are *observations*.
- **The straddle fix is validated on a machine.** Previously pinned by synthetic streams and by a
  cross-check on a booted machine (`52c962e`); this is the first end-to-end read of the shipped wire
  surface on aeon's own ROM showing the partition exact at both levels.
- **This does not close aeon's queue item.** This repo does not close another lane's item. The
  recommendation is unambiguous and the evidence is theirs to check.

## What is left of the ask, honestly

**Not nothing, but not this.** If a per-frame *self* series is still wanted, that is a question about
**routine** rows — `perFrame[]` carries no per-routine breakdown at all today — which is a different and
much larger ask, and should be filed on its own merits rather than inherited from this one. The bucket
half is closed by the fix.

## Provenance note

Two probe runs appear in the session log with slightly different `vint.cyclesTotal` (1,172,338 and
1,169,721). They are different samples: the server accumulated frames across successive probe scripts, so
each run began from a different machine state. **Every figure in this document is from the single run
reported above**, and the load-bearing results are the ones invariant to sample choice — 44.00/call and the
100.00% reconciliation held in both.

## Reproduce

```sh
cargo build --release -p oracle-aether
mkdir -p /tmp/orc-p   # NOT the session scratchpad: that path exceeds SUN_LEN
./target/release/oracle-aether aeon/s4.debug.bin --socket /tmp/orc-p/o.sock --symbols aeon/s4.debug.lst
# then, over the bus: run_frames 600 -> set_profiler{enabled,perFrame} -> run_frames 120
#                     -> get_profiler_frames, and divide cyclesSelfTotal by callsTotal.
```

---

## CLOSED — accepted by aeon the same day

**aeon struck the item.** Their overseer's reply: *"the 44.00 across a 19x cost gap is decisive on its own —
a self field that cannot vary with the handler's work is not an instrument"*, and the 100.00% partition is
*"the discharge we actually needed"*. Landed at aeon `5289a0a3` on `origin/master`; verified firsthand here
that both `docs/OVERSEER.md` and `docs/DEFERRED_WORK.md` carry the anchors.

Three things worth keeping from the close:

1. **They are NOT filing the per-routine per-frame ask.** §"What is left of the ask" is therefore closed
   too, not merely re-scoped. Nothing is owed in either direction on this line.
2. **The refusal unblocked work rather than deferring it.** Migration of `raster_cost_probe`,
   `engine_baseline_probe` and `streaming_choke_probe` off the legacy harness is unblocked on this count
   and needs nothing further from us.
3. **⚑ Their own records had been inconsistent, and the inconsistency is the same shape as the bar that
   started this.** `OVERSEER.md` withdrew the ask on 08-24, while a later paragraph — and its twin in
   `DEFERRED_WORK.md` — still called it *"HALF discharged, the half that matters still open"*. A withdrawal
   stated in one place and contradicted further down is exactly the scope-marking failure that filed this
   ask in the first place (a rule true of routine rows, false of interrupt buckets, stated without marking
   which). **A correction that does not chase its own restatements has not been made.** Both now carry it.
