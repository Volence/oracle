# Aeon's third demand-side statement: the CPU profiler (2026-08-19)

Recorded for the same reason the gap list and the scanline-readback demand were: a
demand-side statement of what this bus is missing, **evidenced rather than asserted**.
Every claim below is transcribed from the consumer's own source with a `file:line`
anchor, and where the relayed summary and the code disagree the disagreement is flagged
under §7 rather than silently reconciled.

Aeon repo state at transcription: `aeon` @ `18af84f` (2026-08-19).
Relay source: the Aeon overseer, 2026-08-19 (two messages — the demand spec, and the
parity-corpus addition in §8).

---

## 0. Why this is a demand and not a feature idea

Aeon's cost-gate lane runs on the **old C++ oracle today, and only there**. Their own plan
says so in as many words:

> `docs/superpowers/plans/2026-08-18-scanline-p2-specialization-budget.md:9`
> *"**oracle** for all Phase-0 profiling (oracle-next has no profiler instrument yet …)"*

> `…:19-25`
> *"## Instrument: PHASE 0 RUNS ON ORACLE … Confirmed with the oracle-next session
> 2026-08-18: **oracle-next has no profiler instrument at all.** None of its 31 bus methods
> is profiler-shaped. So the HInt/VInt question is neither reproduced nor fixed there — it
> is absent, and there is nothing to migrate onto. **Phase 0 runs on oracle, full stop.**"*

That answer was ours, given here on 2026-08-18 (`docs/2026-08-18-aeon-scanline-readback-demand.md:132-145`).
It is the only reason the lane is still on the old instrument.

**The old instrument is actively costing them.** Their aggregate gate runner records a
measured operational failure, not a preference:

> `aeon/tools/effects_gates.py:56-66`
> *"the **oracle stop-race intermittently wedges ONE emulator-backed gate forever** … It
> burned three sessions on 2026-08-19 (two full-lane timeouts recovered only by
> hand-segmenting with `--only`, plus orphaned `oracle_gui` processes left behind)"*

> `aeon/tools/effects_gates.py:136-139`
> *"While it was being taken, `snapshot_poison` **WEDGED live — 642 s and still hanging**
> with its `oracle_gui` alive, against 10 s when it ran clean minutes later."*

The wedge forced a whole architecture on their runner (self-re-invocation per segment,
per-segment timeouts, one retry, PID reaping by argv token — `effects_gates.py:52-70`,
`:185-230`, `:294-317`). That architecture is a workaround for the instrument's host, and
the migration retires it.

**The migration gap is exactly three bus rows.** Every other call
`raster_cost_probe.py` makes already exists on this bus today:

| Probe call | anchor | on this bus? |
|---|---|---|
| `emulator/load_symbols` | `raster_cost_probe.py:601` | yes |
| `emulator/reset {wait, run}` | `:501` | yes |
| `emulator/run_frames {frames}` | `:502`, `:508`, `:522`, `:532` | yes |
| `emulator/write_memory {addr, bytes}` / `{addr, value, width}` | `:506-518` | yes (CR-21 / §11.13) |
| `emulator/read_memory {addr, len}` | `:542` | yes |
| **`emulator/set_profiler {enabled}`** | `:530`, `:537` | **absent** |
| **`emulator/get_profiler {}`** | `:534` | **absent** |
| **`emulator/get_profiler_frames {frames, top}`** | `:535` | **absent** |

---

## 1. The load-bearing consumer: `tools/raster_cost_probe.py`

What it is, in its own first line (`raster_cost_probe.py:2`): *"measure what one raster
fire actually costs, per op class."* It pokes a synthetic raster program into
`Raster_Buf_A` and reads back the per-routine profiler row for the HBlank trampoline.

### 1.1 The exact call sequence per fixture sample

Transcribed from `_one()`, `raster_cost_probe.py:492-544`:

```
emulator/reset            {"wait": True, "run": False}                :501
emulator/run_frames       {"frames": settle}          (settle=180)    :502
emulator/write_memory     Debug_Scene_Freeze = 1  (width 1)           :506
emulator/run_frames       {"frames": 2}                               :508
emulator/write_memory     Raster_Buf_A = <program image, hex bytes>   :510
emulator/write_memory     Raster_Patch_Tab = 0        (width 4)       :511
emulator/write_memory     Effects_Offscreen_Entry = 0 (width 4)       :513
emulator/write_memory     Raster_Active_Buf = buf     (width 4)       :515
emulator/write_memory     Raster_Program = buf        (width 4)       :517
emulator/run_frames       {"frames": 2}                               :522
emulator/set_profiler     {"enabled": True}                           :530
    asyncio.sleep(0.4)                                                :531   <-- see §7.1
emulator/run_frames       {"frames": sample}          (sample=31)     :532
    asyncio.sleep(0.4)                                                :533
emulator/get_profiler     {}                                          :534
emulator/get_profiler_frames {"frames": sample - 1, "top": 200}       :535
emulator/set_profiler     {"enabled": False}                          :537
emulator/read_memory      {"addr": buf, "len": len(words)*2}          :542
```

Session setup once per boot: `emulator/load_symbols {path: <.lst>}` (`:601`), inside
`headless_emulator(rom)` (`:612`), one boot per `--repeat` iteration (`:611-614`).

### 1.2 The response shape they consume

Two fields, and only two, are read out of the responses:

- `get_profiler` → **`frames_recorded`** (`:536` — `st.get("frames_recorded")`). Carried
  through into the result dict; a provenance/sanity field, never arithmetic.
- `get_profiler_frames` → **`routines[]`**, walked at `hint_row()`, `:547-563`:

```python
for r in prof.get("routines", []):
    a = int(r.get("addr", "$0").lstrip("$"), 16)      # :559
    if (a & 0xFFFFFF) == (hb_addr & 0xFFFFFF):        # :561
        return r
```
and the row's fields used downstream, `:633-634`:
```python
cyc = [int(x["cycles"]) for x in rows]
cal = [int(x["calls"]) for x in rows]
```

So the load-bearing shape is exactly: **`routines[]`, rows keyed by routine entry
address, each carrying `addr` (hex string), `cycles` (int), `calls` (int)**. Nothing
else in either response is read.

### 1.3 Property (a) — division inside the emulator

Their own statement of why this matters, `raster_cost_probe.py:19-22`:

> *"`cycles` and `calls` are both **divided by the frame count inside the emulator**, so a
> multi-frame sample is **exact to within 1 cycle rather than averaged-with-noise**."*

This is not a nicety. It is what lets a 31-frame sample act as a single exact measurement,
and it is what makes the marginal-cost arithmetic at `:653-659` — `(fixture − F0) / n` —
mean a per-fire cost rather than a per-fire cost plus a sampling error.

### 1.4 Property (b) — `calls` counts actual invocations

Their own statement, `raster_cost_probe.py:21-22`:

> *"`calls` is a **free correctness check**: it says how many fires actually happened, so a
> fixture that failed to install cannot be silently measured."*

Restated at `:66-69` as the cross-check on the probe's second, independent transcription
of the wire format:

> *"The cross-check that it is transcribed correctly is **empirical and strong** — `calls`
> reports the fires the hardware actually took, so a mis-encoded program shows up as the
> **wrong fire count before any cycle figure is read**."*

It is enforced three ways:
- varying `calls` across repeat boots is printed as a defect (`:640-641`);
- a dense fixture's fire count is **derived** (`lines + 5`, `dense_fire_count()`, `:362-364`)
  and checked against the measurement (`:645-649`);
- the same derived check runs inside the merge gate (`effects_gates.py:772`):
  `calls_ok = all(f["calls"][0] == f["dense"]["lines"] + 5 for f in (d1, d2))`.

The derivation was **corrected against the hardware rather than relaxed to it** — `:313-316`:
*"It was derived as lines + 4 first, and the hardware said 13 and 45 where 12 and 44 were
predicted … the count is corrected here rather than the check being relaxed to whatever was
measured."* An instrument whose `calls` were approximate would have hidden that.

### 1.5 The exactness bar is stronger than "within 1 cycle" — it is `==`

`raster_cost_probe.py` is not only a hand tool. Gate 5 of Aeon's pre-merge lane
(`effects_gates.py:35-39`, registry row `("cost_model", True, 900)` at `:152`) shells out
to it (`:737-741`) and asserts **integer equality** against expectations computed from the
constants the engine ships (`effects_gates.py:754-756`):

```python
ok = (got_f0 == f0 and got_f1 == expect_f1 and got_f3 == expect_f3
      and got_f4 == expect_f4 and got_f5 == expect_f5 and got_f8 == expect_f8)
```

and the dense row asserts an exact slope equality (`:773`): `ok_d = calls_ok and slope == dense_line`.

**Consequence for our design:** the profiler's per-frame `cycles` figure is consumed by a
merge gate with `==`. Any jitter of even one cycle turns their lane red. "Exact to within
1 cycle" is the floor; **bit-exact and reproducible is the actual requirement.**

### 1.6 Their noise bar

`--repeat N` boots the emulator N independent times (`:611-614`) and the table prints a
spread (`:636`):

```python
spread = f"{min(cyc)}..{max(cyc)}" if len(set(cyc)) > 1 else str(cyc[0])
```

The bar the old instrument already clears, from their plan
(`2026-08-18-scanline-p2-specialization-budget.md:56`):

> *"every one matched its cost model **to the cycle, 3 boots, spread 0**."*

**Spread 0 across 3 boots is therefore a floor we must meet, not a target.**

### 1.7 The parity fixture (already on record here)

Eight raster cost fixtures, re-measured on oracle 2026-08-18 against the current wire
format, marginal cost per fire `(fixture − F0)/n`, F0 = 572 — table at
`2026-08-18-scanline-p2-specialization-budget.md:67-77` (restated as a row pair at `:88-90`)
and duplicated in this repo at
`docs/2026-08-18-aeon-scanline-readback-demand.md:170-182`. Not restated here; read it
from one of those two places so it cannot go stale in three.

---

## 2. The interrupt-bucket conflation — why `interrupts.hint` is never consumed

The probe's module docstring is the primary source, `raster_cost_probe.py:4-15`:

> *"`get_profiler_frames` returns an `interrupts.hint` figure, and **it is NOT HBlank cost
> in this ROM**. Oracle classifies an interrupt by the handler address the vector points at:*
>
> ```c
> if (vec == 0x78 || (vec >= 0x70078 && vec <= 0x7FFFF)) vint += dur; else hint += dur;
>          -- oracle linux-port/gui/ControlSocket.cpp, OpGetProfilerFrames
> ```
>
> *Aeon's `VBlank_Handler` sits at `$2310` and its HBlank trampoline at `$FFB452`, so **BOTH
> fall into the `else`** and `interrupts.hint` is (HBlank + VBlank). That is the whole
> explanation for the 2026-08-18 session's "the hint counter includes VBlank work" caveat
> and for the ~380-cycle jump it saw when the off-screen ship turned on: the ship is VBlank
> work being counted as HInt. **The counter is not subtly contaminated, it is measuring both
> handlers.**"*

Their plan restates it as a standing, non-negotiable discipline
(`aeon/docs/superpowers/plans/2026-08-18-scanline-p2-specialization-budget.md:25`, and all
bare `:N` anchors in §2 and §4 are into that same file):

> *"**Per-routine rows keyed by entry address are mandatory; `interrupts.hint` is never a
> valid source.** … This is a **silent wrong number rather than a missing one**, which is
> why the discipline is non-negotiable rather than a preference."*

And the sharpened generalisation they asked us to keep pinned
(`docs/2026-08-18-aeon-scanline-readback-demand.md:199-202`):

> *"entry-PC bucketing **mis-buckets for ANY ROM** whose vector points where the heuristic
> didn't anticipate, producing a silent wrong number rather than a missing one, **'which is
> the worse kind.'**"*

**They will consume our split buckets.** Their plan's Task 3 (budget axis 4b) is a
per-frame HInt total that has *never been measured* precisely because of this
(`…:155`):

> *"design §5 axis 4 splits into (4a) per-fire spacing … and **(4b) a per-frame HInt TOTAL,
> which is genuinely new. The toml's absolute HInt rows have never been measured —
> `interrupts.hint` conflates VBlank, which is why.**"*

and the plan already anticipates our surface (`…:29`):

> *"**When a profiler surface does land on oracle-next**, its design is already pinned on
> their side: **HInt and VInt as separate buckets keyed by interrupt cause, never by
> handler-entry-PC matching**, with Aeon's finding cited as the measured counterexample. At
> that point Task 1 becomes a genuine cross-instrument parity check."*

**This repo's pinned rule** (registered 2026-08-18,
`docs/2026-08-18-aeon-scanline-readback-demand.md:142-145`): ★ HInt and VInt MUST be
separate buckets keyed by **cause** (which interrupt was taken), never by handler-entry-PC
pattern matching.

---

## 3. The two old-oracle warts they name, and what deleting them buys

### 3.1 Sign-extended address strings

`raster_cost_probe.py:549-555`, in `hint_row`'s docstring:

> *"Oracle prints the row's key as `$FFFFB452` (the raw 68000 PC, sign-extended by the
> short-form addressing the vector slot is reached through) while the listing spells the
> same location `$FFB452`. **Comparing the low 24 bits is what makes the two agree;
> comparing the printed strings does not** …"*

The reconcile code is `:559-561` — a `lstrip("$")`, an `int(…, 16)`, and two `& 0xFFFFFF`
masks. **A canonical 24-bit address form deletes it.** The 68000 has a 24-bit address bus;
a 32-bit sign-extended PC in a row key is an artefact of the reference implementation, not
of the machine.

### 3.2 No symbol name on rows

Same docstring, `:553-555`:

> *"the row also **has no symbol name attached**, so matching on `HBlank_Vector_Slot` would
> find nothing either."*

The consequence is visible in the probe's own architecture: it must parse the sigil listing
itself (`parse_lst()`, `:474-489`) and carry a `SYMS` tuple (`:468-471`) purely to turn
names into addresses, **even though it also calls `emulator/load_symbols` on the bus**
(`:601`). The emulator has the symbols and does not use them for profiler rows.

**A resolved `name` field on each row is a strict improvement, and it is load-bearing for
work already scheduled**, not speculative: their Task 2 wants per-routine rows for five
*named* routines — `Parallax_Update`, `Raster_VBlank`, `Palette_Compose`,
`Enqueue_Dirty_Buffers`, `BgAnim_Update`
(`2026-08-18-scanline-p2-specialization-budget.md:120`) — and Task 4 fits a walker cost
model from a per-parameter fixture set (`…:186-200`). Every one of those is a
name→address→row hop today.

---

## 4. What the five Phase-0 tasks actually ask the instrument for

Transcribed from `aeon/docs/superpowers/plans/2026-08-18-scanline-p2-specialization-budget.md`.
Phase 0 is the gate on everything downstream (`:17`): *"Phase 0 produces data and nothing
else. **No gate in Phases 1-2 may reference a row Phase 0 did not measure.**"*

| Task | anchor | what it needs from the instrument |
|---|---|---|
| **1 — re-confirm oracle's own figures** | `:52-111` | The eight F0–F8 fixture rows, `(fixture − F0)/n`, 3 boots. Explicitly *"originally written as an oracle-next parity check; it is not, because oracle-next has no profiler"* (`:54`). **Our arc converts this task back into what it was written as.** |
| **2 — engine baseline rows** | `:113-151` | Per-routine rows for five **named** routines at two pinned camera states; *"Five boots per state; report spread. Every figure ships with a wall-clock uptime beside it"* (`:127`). |
| **3 — per-frame HInt total (axis 4b)** | `:153-182` | The HBlank trampoline's per-frame total on **shipped content** at both camera states, cross-checked against the summed model (`:167`). Row is written as `hint_total_cycles_per_frame = 0 # REPLACE — measured, NOT interrupts.hint` (`:172`). **This is the task our cause-keyed HInt bucket serves directly** — *"(4b) a per-frame HInt TOTAL, which is genuinely new. The toml's absolute HInt rows have **never been measured** — `interrupts.hint` conflates VBlank, which is why"* (`:155`). |
| **4 — walker fitted cost model** | `:184-210` | Per-routine rows for parallax procs across one-variable-at-a-time fixtures; *"The residual is the deliverable, not a footnote"* (`:199`) with a 0-residual target. |
| **5 — max contiguous DMA stall (awareness row)** | `:212-240` | *"Measure the longest contiguous DMA stall in a frame at both camera states"* (`:219`) — **explicitly non-gating** this phase (`:214`, `:225`). **No profiler surface, old or new, provides this today**; see the design doc's better-approach pass. |

**One property of the floor that Tasks 2 and 4 silently inherit:** the old instrument's
`cycles` are **inclusive of callees** — the shadow stack charges a routine's whole span to
that routine and subtracts nothing for its children
(`oracle/linux-port/gui/ControlSocket.cpp:1989-1991`), so its `pct` column sums well past
100%. For the HBlank trampoline (Task 1/3) that is harmless — it is effectively a leaf. For
`Parallax_Update` and the walker fit (Tasks 2/4) it is **load-bearing and undeclared**: a
fitted per-layer slope taken from inclusive rows is a slope over the whole call tree. Our
design can offer self *and* inclusive; §4 of the design doc prices that, and the demand side
should be asked which one their fit wants before the corpus A/B is read.

Task 2 also carries a standing methodological warning worth honouring in any acceptance
protocol we write (`:123`): *"A baseline whose state is not reproducible is not a baseline.
The P2 baseline rows in `effects-p3` went camera-stale exactly this way."*

---

## 5. The standing caveat they carry about **us**

Not a demand — a constraint on how our numbers may be used, and it is theirs, accepted here
(`2026-08-18-scanline-p2-specialization-budget.md:30`):

> *"**Standing caveat carried from oracle-next:** absolute cycle claims keep oracle as the
> reference while oracle-next's **instruction-granularity slop** is open. Do not assert
> oracle-next cycle parity in any row this plan produces."*

Ours, same wording, at `docs/2026-08-17-aeon-switchover-gap-list.md:110-112`: *"A/B gates
cancel the slop, absolute measurements do not."*

This has a direct design consequence and it is stated here so the design cannot forget it:
**a profiler that reports our own cycle accounting inherits our cycle accounting.** The
migration acceptance is therefore an **A/B against the old instrument on the same ROM**,
with any delta owed a mechanism — not a claim of absolute parity. See §8.

**And the caveat cuts the other way too, which they do not yet know.** The old instrument's
cycle base is `M68000::_currentCycle`, advanced only by the static per-opcode cycle table;
bus wait states and VDP/DMA contention go into a *separate* real-time accumulator and
**never into the profiler's clock** (`oracle/Devices/M68000/M68000.cpp:1029-1031`, found in
the recon — see the design doc §B). So **every figure in the corpus of §8 is a
stall-free figure**, and Task 5's max-contiguous-DMA-stall row is measuring a quantity the
same instrument's cycle counter excludes by construction. Our scheduler-derived cycles
include stalls. **This is a first-order, expected A/B disagreement and it must be reported
to the demand side as a finding, not absorbed as a tolerance.**

---

## 6. `effects_gates.py` — the second consumer, and its shape

`effects_gates.py` does not call the profiler directly. It shells out to
`raster_cost_probe.py` (`:737-741`) as gate 5 of 22, with the widest wedge budget in the
registry (`:152`, 900 s) because *"it is the one segment that boots SIX emulator fixtures in
sequence, other sessions have reported it near 3 min"* (`:131-133`).

Its dependency on the profiler is therefore entirely transitive — but it is the reason the
exactness bar is `==` (§1.5) and the reason the old instrument's wedge is a *merge-lane*
problem and not just an ergonomics one (§0).

---

## 7. Relay-vs-code discrepancies

The relayed spec was checked claim by claim against the code. Four notes.

### 7.1 ⚠ The relay's call sequence omits two load-bearing sleeps

Relay: *"set_profiler{enabled:true} → drive N frames → get_profiler{} (sanity) →
get_profiler_frames{frames: sample-1, top: 200} → set_profiler{enabled:false}"*.

The code has `asyncio.sleep(0.4)` on **both** sides of the run (`:531`, `:533`), and
documents them as mandatory (`:523-529`):

> *"**THE SLEEPS ARE LOAD-BEARING, not politeness.** `run_frames` executes synchronously on
> the socket thread, but **the profiler is driven entirely by the GUI's MAIN loop**: that
> loop is what calls `m68k->SetProfilingEnabled(true)`, what services the reset request, and
> what drains the CPU's event ring into frame snapshots (`main_gui.cpp`, "Profiler: drain
> ring buffer"). **`set_profiler` only flips a flag.** Without a main-loop tick between the
> flag and the run, the CPU never starts recording and `get_profiler_frames` answers "no
> profiler frames recorded"; without one after, the tail of the run is still in the ring."*

This is a **third old-oracle wart**, unnamed in the relay: the profiler's arm/disarm and
its drain are **asynchronous to the command that requests them**, so correctness depends on
a wall-clock sleep tuned by hand. It is a race, and 0.4 s is a guess that currently works.

On this bus it is structurally absent — `crates/oracle-aether/src/server.rs`'s `engine_loop`
owns the `System` on one thread and drains commands in order, and a paused machine advances
zero cycles between a reply and the next command (verified and recorded at
`docs/2026-08-18-aeon-scanline-readback-demand.md:149-160`). **The design must state
explicitly that arm/disarm/read are synchronous with respect to the command**, so the probe
can delete both sleeps — that is ~0.8 s × 10 fixtures × N boots of pure latency removed
from a 900-second merge gate, and one race removed.

### 7.2 ⚠ `frames: sample - 1` is a workaround, not a semantic — mechanism now identified

Relay: *"get_profiler_frames{frames: sample-1, top: 200}"* — accurate as transcription
(`:535`, `sample` defaults to 31 → `frames: 30`). But **the probe never explains the
`- 1`**, and it prints its header as *"sample {args.sample - 1} frames"* (`:618`), i.e. it
believes it measured 30 frames after running 31.

**The old-oracle recon settles it** (see the design doc, `docs/2026-08-19-profiler-recon.md`
§B): the frame ring is indexed newest-first (`oracle/Devices/M68000/ProfileTypes.h:89-94`,
`get(0)` = newest) and `numFrames` walks back from newest
(`oracle/linux-port/gui/ControlSocket.cpp:1966-1968`). A snapshot's span is
`first-event → V-INT`, not V-INT → V-INT
(`oracle/linux-port/gui/main_gui.cpp:2006-2012`), so **the first frame after `set_profiler`
is arbitrarily short**. Asking for `count − 1` walks back from the newest and drops exactly
the oldest — the runt. **`- 1` is a hand-compensation for a partial first frame.**

Flagged here because it is demand-relevant twice over: **whatever our surface does about a
non-frame-aligned enable must be a stated semantic, not a number the consumer has to guess**
— and if our enable is frame-aligned by construction, the `- 1` deletes itself and their
sample becomes `sample` frames instead of `sample − 1`, which is a (small) change in what
their fixtures measure. That must be called out at migration, not discovered.

### 7.3 `top: 200` — a bound the consumer already supplies

Relay describes `top: 200` without comment; the probe passes it and then linearly scans
`routines[]` for exactly one address (`:547-563`). So `top` is, for this consumer, purely a
truncation bound on a list it does not otherwise use. **The consumer wants one row and is
being handed up to 200.** Named here because a lookup-by-address (or by name) parameter
would serve this consumer better than a `top`-truncated dump, and because §2.4's
bounded-list rules apply to whatever we do choose.

### 7.4 Everything else in the relay verified clean

- Call sequence (modulo 7.1): ✅ `:501-542`.
- `routines[]` keyed by routine entry address with `{addr, cycles, calls}`: ✅ `:547-563`, `:633-634`.
- Property (a) division inside the emulator: ✅ `:19-22`.
- Property (b) `calls` = actual invocations: ✅ `:21-22`, `:640-649`, `effects_gates.py:772`.
- Wart 1, sign-extended addr strings: ✅ `:549-561`.
- Wart 2, no symbol name on rows: ✅ `:553-555`.
- `interrupts.hint`/`.vint` conflated and consequently never consumed: ✅ `:4-15`,
  plan `:25`.
- They will consume split buckets for the per-frame HInt total (axis 4b): ✅ plan `:144-172`, `:29`.
- Noise bar, spread 0 across 3 boots: ✅ plan `:56`; probe's spread machinery `:636`, `:640-641`.

One observation in passing, not a demand: the probe's dense-fire-count message at `:648-649`
prints *"derived {want_calls} (lines + 4)"* while the derivation it calls is `lines + 5`
(`:362-364`) and the comment above it says `lines + 4` (`:645`). The **arithmetic is
correct** (`dense_fire_count` returns `lines + 5`, and `effects_gates.py:772` agrees); only
the two prose mentions are stale. Theirs to fix; noted so nobody transcribes `+ 4` as the rule.

---

## 8. The parity corpus — Phase 0's measurement session, in flight now

*Source: relayed by the Aeon overseer, 2026-08-19. **Merge SHA: PENDING** — they will flag
it when it lands on aeon master. Do not invent an anchor; fill this in on their signal.*

Their P2 Phase 0 measurement session is **executing right now on the old oracle**, and it
produces exactly the row set §4 describes:

- engine baselines at **two pinned camera states** (Task 2),
- the **per-frame HInt total** on shipped content (Task 3),
- a **fitted parallax-walker cost model with its residual** (Task 4),
- the **DMA-stall awareness row** (Task 5),
- **five boots per state, spread reported** (Task 2, `:120`).

When it merges, that row set is a **ready-made parity corpus for our profiler**: freshly
measured, provenance-stamped, with the camera states documented reproducibly in their
evidence files (`docs/benchmarks/scanline-p2/ENGINE-BASELINE.md`, `…/WALKER-MODEL.md`,
`…/INSTRUMENT-PARITY.md` — all created by Phase 0, none present at transcription time).

**Why this matters more than a fixture-only check.** The corpus is measured on the
instrument we are replacing, on the same ROM, at states we can reproduce. A/B-ing our first
build against it is therefore the **primary** acceptance shape for this arc, and it is where
the owner's better-approach directive stops being a design argument: their framing, which
matches ours, is that **if exact per-invocation attribution beats the old instrument's
floor, this corpus is where that becomes a measured claim.** The design doc's acceptance
protocol (§7 there) builds the comparison, including which rows are comparable 1:1, which
must legitimately disagree (the interrupt buckets — our split cannot match their summed
`interrupts.hint`, by construction), and the rule that a spread is not a tolerance.

**Placeholder to fill on their signal:**

```
aeon merge SHA:      PENDING
evidence files:      docs/benchmarks/scanline-p2/{ENGINE-BASELINE,WALKER-MODEL,INSTRUMENT-PARITY}.md
camera states:       PENDING (transcribe the two definitions verbatim when they land)
old-instrument spread: PENDING (five boots per state)
```

---

## 9. Summary of the demand

1. **Three bus rows** (`set_profiler`, `get_profiler`, `get_profiler_frames`, or better
   names the design earns) — everything else the consumer needs is already shipped.
2. **`routines[]` keyed by routine entry address**, each row `{addr, cycles, calls}` at
   minimum.
3. **Cycles and calls divided by frame count inside the emulator**, exact — consumed by a
   merge gate with `==`, not with a tolerance (§1.5).
4. **`calls` = actual invocations**, exact, because it is their fixture-correctness check
   and it has already caught a wrong derivation (§1.4).
5. **Spread 0 across 3 boots** (§1.6) — the old instrument already clears this.
6. **HInt and VInt as separate buckets keyed by interrupt cause** — the pinned rule, and
   the enabler for a budget row they have never been able to measure (§2).
7. **Canonical 24-bit `addr` strings** and a **symbol-resolved `name`** — both strict
   improvements over the floor, both deleting consumer code that exists today (§3).
8. **Synchronous arm/disarm/read**, so the two hand-tuned sleeps and their race go away (§7.1).
9. **A stated last-frame semantic**, so `frames: sample - 1` stops being folklore (§7.2).
10. **A/B against the Phase-0 corpus** as the primary acceptance, with the interrupt buckets
    expected to disagree and the disagreement explained rather than tolerated (§8).

---

## 10. Addendum — the three shape-check answers (relayed 2026-08-19)

*Added 2026-08-19 after CR-26's adjudication, which recorded these answers as **COULD NOT VERIFY** —
*"relayed, exist in no committed artifact"* (`docs/2026-08-19-ruling-cr26.md:184-186`, ruling S3). They are
committed here so the CR can cite an artifact instead of a transcript, and so the provenance is on the
record with its limits stated rather than implied.*

**Provenance, and what kind of source this is.** Three questions were sent to the Aeon overseer on
2026-08-19, at the controller ruling's direction — *"Demand-side shape-check runs in parallel (three
questions sent: walker-fit field, stall-gate handling, `perFrame[]` interest)"*
(`docs/2026-08-19-ruling-profiler-recon.md:88-89`). The answers came back the same day in a cross-session
message from the Aeon overseer, relayed into this session by the controller and **controller-attested**.

**This section is a transcription of that relay, not a quotation from a file in `aeon`.** Everything else in
this document is anchored `file:line` into the consumer's own source; these three were not at the time of
writing. **CLOSED 2026-08-19 (controller): the aeon-side anchor landed — aeon master `e0913e79`,
`docs/superpowers/2026-08-19-profiler-shape-answers.md`** — carrying the three answers as given, the
mid-session Task-5 correction, the migration-breakage notes (the `frames`-param removal flagged as the
first-run trap), and the CR-26 shape-check **PASS** with its verification basis (reviewed at oracle-next
`4cf7db5` against the probe's real consumption). The transcription below stands as written; the aeon file
is now the authoritative cross-check. (The §8 parity-corpus SHA remains separately PENDING.) What *is* independently verifiable is
the effect: every pin below is present in the amendment text and in the schema fragments, where an
adjudicator has checked it (`docs/2026-08-19-ruling-cr26.md`, HELD rows).

### 10.1 Which cycle field the walker fit wants — **inclusive**

> Every consumer they have reads rows **inclusively**. Their F-series measurement takes the HBlank
> trampoline's row as the whole fire *including callees*, and their Task-4 marginal method prices callees
> inside the caller by construction. `cyclesSelf` is welcome and changes nothing they do today.

Closes the question ruling Q3 left open (*"Which one the parallax-walker fit actually needs is **asked of
the demand side**"*, `docs/2026-08-19-ruling-profiler-recon.md:31-34`). It confirms the shape rather than
changing it: `cycles` stays inclusive and floor-compatible, `cyclesSelf` ships beside it. It also sharpens
§4's warning above — the inclusive-vs-self hazard on Tasks 2 and 4 is now a *known* property of their fit,
not an undeclared one.

### 10.2 What form the stall-gate subtraction takes — **`cycles - stallCycles`, with two pins**

> Their gates would reconcile stall-inclusive truth against their `.emp`-derived ideal constants as
> `cycles - stallCycles == constant`. Two things are asked for explicitly:
>
> **(a)** `stallCycles` keyed **identically** to `cycles` on the same row — same routine keying, same
> divided-inside exactness — because a stall figure with different aggregation semantics would poison the
> subtraction rather than enable it.
>
> **(b)** a **normative one-line definition of what counts as a stall**, pinned in the contract, so that a
> future stall source is a visible schema and amendment event rather than a silent semantic drift under
> green gates.

Both pins are in the amendment: (a) as the *"subset of `cycles` on the same row, keyed identically to it"*
wording in the §6 blockquote and in `stallCycles`'s own schema description, and (b) as the three enumerated
stall conditions in the same two places.

### 10.3 Whether they want per-frame rows — **record-and-later**

> Not consumed now. Worth recording for later lag and famine analysis.

Unchanged from ruling Q1: `perFrame` ships **opt-in and default-off**, bounded. The answer is also what
makes CR-26's migration delta 2 load-bearing — with no ring armed, a `frames` parameter is refused, so the
probe drops the parameter rather than the `- 1` alone.

### 10.4 The Task-5 correction they made mid-session

Relayed with the answers, and it is a change on *their* side rather than a request on ours: on receiving the
stall finding (§5 above, and the design doc's §A.3), their in-flight Phase-0 measurement **corrected its
DMA-stall task mid-session**. That row now records the old instrument's blindness as the finding — loud on
unmeasurable, citing the recon — instead of shipping a confident undercount, and every row of the parity
corpus is to carry an *"ideal-cycle by construction"* caveat naming what its cycles do and do not include.

**Why that matters to the acceptance protocol rather than merely being good news:** the corpus will state
its own basis. A delta on a stall-touching row then starts from a documented difference between two
instruments instead of an argued one, which is exactly the difference between a finding and a tolerance.
