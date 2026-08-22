# What each profiler's per-routine cycle figure actually contains (2026-08-22)

**Scope.** A source-reading audit, no emulator, no build, no cargo. Every claim below carries a
`file:line`. Two codebases are read: **ours** (`oracle`, Rust, this tree at `a27e4d2`) and **theirs**
(`oracle-old`, the legacy C++ Exodus port at `/home/volence/sonic_hacks/oracle-old`, HEAD `d629771`,
read-only, not mine, nothing modified). Where I could not settle a question from source, it is marked
**INFERENCE** or **BLOCKED** and listed again in §7.

**The question.** *Exactly what does each instrument's per-routine cycle figure include, and are they
the same quantity?*

**The one-line answer.** **No — they are not the same quantity, and the difference is not the one
caveat 0 names.** Caveat 0 is *verified correct about their clock* and is *irrelevant to the disputed
rows*, because our own figure is provably a pure ideal-cycle count whenever `stallCycles == 0` (§1.3),
which is true of all five. The two instruments differ in three places instead: **the bracket** (theirs
charges the `JSR` to the callee and drops the `RTS`; ours does the exact opposite — §3.1), **preemption**
(theirs folds an interrupt that fires inside a routine into that routine; ours never does — §3.2), and —
the load-bearing one — **how a return is matched to an entry** (ours requires an exact stack-pointer +
privilege-mode match; theirs pops whatever is on top of a LIFO that is thrown away and rebuilt empty at
every frame — §3.3).

---

## 1. Ours — what is in `cyclesTotal`

### 1.1 The bracket

The profiler is a pure accumulator over the retire stream. The order inside one retirement decides the
bracket, and it is explicit (`crates/oracle-core/src/profiler.rs:892-947`):

1. `pending_call.take()` → push the callee frame, keyed by **this** step's `pc` (`:894-900`).
2. `pending_iack.take()` → push an interrupt bucket, and arm a call so the handler gets its own row
   (`:914-924`).
3. `charge(r.pc, …, r.cycles, r.stall_cycles)` → bill this step to the **innermost open frame**
   (`:926-932`). The comment at `:925` states the intent: *"The entry cost belongs to the frame it
   opened, so charge after pushing."*
4. Only now classify the opcode (`:938-946`): `Call` arms `pending_call`; `Return` runs `close_routine`;
   `InterruptReturn` runs `close_interrupt`.

Reading the consequences off that order:

| event | which frame is billed | why |
|---|---|---|
| `JSR` / `BSR` | **the caller** | at the `JSR`'s own retirement `pending_call` is still `None`, so no push has happened; `charge` (`:926`) bills the innermost frame, which is the caller. `pending_call` is only armed afterwards, at `:942`. |
| the callee's first instruction | the callee | step 1 pushes before step 3 charges. |
| `RTS` / `RTR` | **the callee** | `charge` (step 3) runs before `close_routine` (step 4, `:943`), so the return's own cycles land in the frame it is about to close. |
| the **entry prefetch** | **the caller** | `jsr_recipe` contains the two reload `Prefetch` micro-ops itself (`crates/oracle-core/src/m68000/decode.rs:3512-3513` and the comment at `:3506-3511`: *"Bus order: [r@target, w@SP−4, w@SP−2, r@target+2]"*). They are inside the `JSR`'s cycle total, which is charged to the caller. |
| the **return prefetch** | **the callee** | symmetrically, `rts_recipe` ends with two `Prefetch` ops at the return target (`decode.rs:3603-3604`), inside the `RTS`'s total, charged to the callee. |

So: **our routine bracket is `[callee's first instruction … its RTS, inclusive]`.** On a 68000 that is
`RTS` = 16 cycles inside, `JSR (xxx).L` = 20 cycles outside.

The closing rule is an **exact** match, and this is the point of comparison with §2: a frame entered at
`entry_sp` closes only on a return leaving `sp == entry_sp + 4` (`RTS`) or `+ 6` (`RTR`), **and** with the
same privilege mode (`profiler.rs:747-766`, predicate at `:753-757`). A `move.l #target,-(sp)` / `rts`
dispatch leaves the stack where it started, matches nothing, and correctly closes nothing (`:42-46`,
`:732-735`). The search runs innermost-first through the whole stack, and anything above a match is
unwound as **abandoned** — cycles kept, `calls` not, counted in `Report::abandoned_frames` (`:762-765`,
`:406-412`).

`JMP` is deliberately *not* a frame event on our side either (`decode.rs:213-216`, `profiler.rs:945`) —
symmetric with theirs, so it is not a difference.

### 1.2 What the cycle counter counts

`Counts::cycles` is fed exactly one number per step: `StepRetire::cycles`. Tracing it back:

- `profiler.rs:930` — `u64::from(r.cycles)`.
- `crates/oracle-core/src/system.rs:1049-1066` — `let (outcome, stall_cycles) = self.step_cpu_stalled(sink);`
  … `cycles: outcome.cycles`. The same value the scheduler is advanced by (`system.rs:1093`).
- `system.rs:1311` — `let outcome = cpu.step_reporting(&mut bus);`
- `crates/oracle-core/src/m68000/microop.rs:3285` — `let cycles = recipe.run_to_completion(&mut self.regs, bus);`
- `microop.rs:1563-1569` — sums `exec_one` per micro-op.
- **Every bus-touching micro-op returns `nominal + wait`**: `Read` `4 + wait` (`microop.rs:1611`),
  `Write` `4 + wait` (`:1640`), `Prefetch` `4 + wait` (`:2672`), `Tas` `10 + wait` (`:2789`),
  `IntAck` `4 + wait` (`:2816`), `MovemStore` (`:2977-2978`), `MovemLoad` (`:3028`), `MovepWrite` (`:3046`),
  `MovepRead` (`:3062`). Pinned by `bus_wait_cycles_are_added_to_the_access_cost_in_exec_one`
  (`microop.rs:3419-3441`).

So **our per-instruction figure is `ideal + bus wait`, not a nominal count.** `StepRetire::cycles`'
own doc says it in one line (`crates/oracle-core/src/bus.rs:124-126`): *"Stall-inclusive: our clock bills
bus/VDP/DMA waits to the instruction that incurred them."* **Verified, not carried.**

### 1.3 `stallCycles` — what it covers, what it does not, and the identity that matters

`stall_cycles` is accumulated **inside the bus**, not by the CPU (`bus.rs:807-820`), at exactly two
points: `vdp_read_word` (`:1106-1110`, `self.stall_cycles += wait`) and `vdp_write_word`
(`:1135-1139`). The field's doc claims completeness by construction; **I checked that claim by reading
every arm of the `Bus68k` impl rather than trusting it** (review bar 8 — enumerate by what *touches*
the value):

| method | arms and their returned wait |
|---|---|
| `read16` (`bus.rs:1217-1260`) | fc=7 IACK → `(v, 0)` `:1229`; VDP port → `vdp_read_word` `:1232-1235`; Z80 window → `:1245` `0`; mapped/open bus → `:1259` `0` |
| `write16` (`:1262-1287`) | VDP port → `vdp_write_word` `:1265-1268`; everything else → `:1286` `0` |
| `read8` (`:1289-1321`) | VDP port → `vdp_read_word` `:1294-1302`; everything else → `:1320` `0` |
| `write8` (`:1323-1338`) | VDP port → `vdp_write_word` `:1329-1332`; everything else → `:1337` `0` |
| `tas` (`:1340-1355`) | `:1354` `0`, unconditionally |

**Complete.** Every non-zero wait the whole bus can return flows through one of the two accumulating
functions, and each of those adds *the same value it returns*. Therefore:

> **`cycles == ideal + stall_cycles`, exactly, per step — and `stallCycles == 0` on a routine row means
> that row's `cyclesTotal` IS an ideal cycle count.**

This is the audit's most consequential finding, because it **removes caveat 0 from the disputed rows
entirely**. Their figure is ideal-only; ours is ideal-plus-stall; on a row where our stall is zero the
two definitions coincide *on this axis*, and all five disputed rows report `stallCycles: 0` (corpus doc
§6.1/§6.2). Whatever separates them, it is not the stall term.

**What `stall_cycles` covers** (enumerated at `bus.rs:131-135`, and each verified at its site):

1. a 68000 **write** to the VDP data port held off by a full 4-entry FIFO — `Vdp::data_write_at`
   (`crates/oracle-core/src/vdp.rs:1292-1312`), non-zero only in the `fifo_len == 4` branch (`:1302-1309`);
2. a 68000 **read** of the data port waiting for the write FIFO to drain (`vdp_read_word_inner`
   `bus.rs:1114-1130`; status and HV reads return `0` explicitly at `:1122-1127`);
3. the whole **68k→VDP DMA bus-hold window**, billed to the instruction that armed it
   (`run_mem_dma`, `bus.rs:1184-1213`, returns `cost.div_ceil(MCLK_PER_CPU_CYCLE)` at `:1212`).

**What it does NOT cover, and what that means:** a VRAM **fill** and a VRAM **copy** return `0`
(`bus.rs:1167-1176`) — not by a filter, because the 68000 keeps running through both. So
`stallCycles: 0` does **not** mean "no VDP activity"; it means "no port-access wait and no DMA halt".

### 1.4 Every path that can add to a routine's attribution

Enumerated by mutation site, not by field name:

- `charge` → `top.self_cycles += cycles` / `top.self_stall += stall` (`profiler.rs:629-630`) — the only
  writer of a frame's own time. Runs **before** the `!r.executed` early return (`:938`), so exception
  entries and idle slices are billed too.
- `pop_frame` → `parent.child_cycles += frame.inclusive()` / `child_stall += inclusive_stall()`
  (`:718-726`) — a completed **routine** child's full lifetime rolls into its parent's inclusive figure.
  **No such roll-up for an `Interrupt` frame** (`:727`).
- `checkpoint` → `row.self_cycles += d_self; row.cycles += d_incl; row.stall_cycles += d_incl_stall`
  (`:663-669`) — the only writer of a row. Runs at every frame boundary (`:973-975`) and at every pop
  (`:687`), so nothing is double-counted (`unreported`/`mark_reported`, `:318-331`).
- `on_frame_boundary` commit → `committed.entry(addr).or_default().add(c)` (`:997-999`) via `Counts::add`
  (`:171-178`).
- `report` → `div(c.cycles)` (`:821`) for the divided `cycles`; the **undivided** `cyclesTotal` is read
  straight back out of `sample_routines()` (`crates/oracle-aether/src/engine.rs:2285-2288`, `:2320-2324`,
  emitted at `:2354`/`:2470`/`:2522`). **The wire is a pass-through — no transformation, no second
  measurement.**

### 1.5 Cost paths that land in NEITHER `cyclesTotal` nor `stallCycles`

1. **Preemption.** An interrupt taken inside a routine goes to its own bucket and its handler's own row,
   never to the preempted routine (`profiler.rs:637-643`, and the absent roll-up at `:727`).
   **Deliberate**, and it is one of the three real differences (§3.2).
2. **A pre-sample interrupt** — suppressed, its cycles reported as `unattributed_cycles` (`:656-662`,
   `:400`).
3. **Everything before the opening frame boundary** — `zero_accrual()` (`:964-969`, `:332-341`).
4. **The trailing partial frame** — `pending_*` is cleared, never committed (`:1011-1016`).
5. **A call past `MAX_DEPTH`** — charged to the innermost *tracked* frame, so the callee gets no row and
   its parent is overstated; counted in `depth_exceeded` (`:574-581`).
6. **Z80 execution and 68k↔Z80 bus arbitration.** The Z80 catch-up is a separate clock (`system.rs:1097`)
   and a 68k access into `$A00000-$A0FFFF` returns wait `0` (`bus.rs:1240-1247`, `:1278-1283`). Real
   hardware stalls the 68k there. **A genuine model gap — but it is absent from `cycles` too, so it is
   not a hidden term inside `cyclesTotal`.**
7. **No DRAM-refresh model, no ROM/RAM wait states** anywhere in the bus (every non-VDP arm returns `0`,
   §1.3 table). The VDP's own refresh slots are internal to its slot model (`vdp.rs:626`) and never
   reach the 68000.

And, the other direction — **in `cyclesTotal` but NOT an ideal 68000 instruction cycle**:

8. **`STOPPED_IDLE_SLICE = 4`** (`microop.rs:3078`), returned per poll for a `Stopped` CPU (`:3266`) and a
   `Halted` one (`:3250`). Its own doc says it is *"a progress device, not timing"* — Yacht has no
   STOP-wait entry. These retire with `executed: false`, and `charge` bills them to the innermost open
   frame anyway (`profiler.rs:926` precedes `:938`). **So a routine that spins in `STOP` accrues 4
   nominal cycles per poll that are neither a real 68000 timing nor a stall.** Flagged for the ROM-side
   agent: if `VSync_Wait` uses `STOP`, this is a live term in its row (its deltas are +2.5% idle /
   −4.9% maxdiag). None of the five disputed rows is plausibly a `STOP` spinner, so it does not touch
   them.
9. Exception-entry recipes (interrupt = 44 cycles: `decode.rs:3938-3958`; address error 50; trace 34).
   Real and pinned — listed only for completeness of the enumeration.

---

## 2. Theirs — what is in their figure

Read-only, and it is not my codebase; where their intent is not written down I say so.

### 2.1 Caveat 0, checked firsthand: **VERIFIED, with one correction to the line numbers**

Aeon's transcription: *"Oracle's clock adds only `cyclesExecuted` to `_currentCycle`
(`M68000.cpp:1029-1031`) while bus, VDP and DMA stall accumulate in `additionalTime` and land in
`_currentTime`. Nothing includes a stall."*

At `Devices/M68000/M68000.cpp:1029-1031`:

```cpp
double totalExecutionTime = CalculateExecutionTime(cyclesExecuted) + additionalTime;
_currentCycle += cyclesExecuted;
_currentTime  += totalExecutionTime;
```

**Correct as stated.** The split is real and it is systematic: the instruction's own prefetch read adds
to `additionalTime` (`M68000.cpp:1004`, `additionalTime += ReadMemory(...)`), as does every operand
access (`opcodeExecuteTime.additionalTime`, `:1008-1009`), while `cyclesExecuted` is the tabulated
figure alone. The same three-line pattern repeats at `:741-743`, `:750-752`, `:781-783`, `:825-827`,
`:835-837`, `:897-899`, `:930-932` — I checked all eight sites; `_currentCycle` is written nowhere else
(`grep` over `M68000.cpp`/`M68000.h`: `:332` init, the eight `+=`, and two *readers* at `:941` and
`M68000.h:95`). **So their profile timestamps are ideal cycles, always.**

*Small correction for the record:* the exact lines are **1029-1031**, and aeon cites 1029-1031 — that
matches; our own corpus doc §4 also cites `M68000.cpp:1029-1031`. No correction needed. What I would add
is that the claim is stronger than they wrote it: it is not "the clock they happen to use", it is *the
only cycle counter in the device*, and the profiler stamps from it directly (`M68000.h:93-96`).

### 2.2 Their bracket

Their profiler is inside the CPU device: `RecordProfileEvent` stamps `_currentCycle` at the moment of
the call (`M68000.h:93-96`). There are exactly five emission sites in the whole tree:

| event | site | when, relative to the instruction's own cycles |
|---|---|---|
| `SubroutineEnter` | `Devices/M68000/JSR.h:110` | inside `M68000Execute`, i.e. **before** `_currentCycle += cyclesExecuted` at `M68000.cpp:1030` |
| `SubroutineEnter` | `Devices/M68000/BSR.h:69` | same |
| `SubroutineExit` | `Devices/M68000/RTS.h:48` | same |
| `InterruptExit` | `Devices/M68000/RTE.h:57` | same |
| `InterruptEnter` (+ `FrameBoundary` for L6) | `M68000.cpp:927-928` | **after** `cyclesExecuted = ProcessException(...)` is computed (`:923`) but **before** `_currentCycle += cyclesExecuted` (`:930`) |

Therefore:

> **Their routine bracket is `[the JSR/BSR itself … the last instruction before the RTS]`.** The `JSR` is
> charged to the **callee**; the `RTS` is charged to the **caller**. Exactly the mirror of ours.

And for an interrupt: **their bucket includes the 44-cycle exception entry** (stamped before the `+=`)
and **excludes the `RTE`** — where ours puts the entry in the *bucket* and the `RTE` in the *handler's
routine row*.

Two omissions with consequences, both confirmed by the absence of any `RecordProfileEvent` call in
those files:

- **`JMP` is not hooked** (no emission site in `JMP.h`) — same as ours, so not a difference by itself,
  but it matters for §3.3.
- **`RTR` is not hooked.** `RTR.h` costs 20 cycles (`RTR.h:36`) and emits nothing. Ours classifies `RTR`
  as a `Return` (`decode.rs:227`, 6-byte pop, `profiler.rs:130-131`). **A routine returned via `RTR`
  closes on our instrument and never closes on theirs** — an unmatched `Enter` on their side (see §3.3).

### 2.3 Their aggregation — where the per-routine number is actually computed

`linux-port/gui/ControlSocket.cpp:1966-2022`, inside `OpGetProfilerFrames`:

```cpp
for (int fi = 0; fi < numFrames; ++fi) {                 // :1966
    const auto& snap = hist.get(fi);
    struct StackEntry { uint32_t address; uint64_t startCycle; bool isInterrupt; };
    std::vector<StackEntry> stack;                        // :1972  <-- EMPTY, per frame
    for (const auto& ev : snap.events) {
        case SubroutineEnter: stack.push_back({ev.address, ev.cycle, false}); break;   // :1978
        case InterruptEnter:  stack.push_back({ev.address, ev.cycle, true});  break;   // :1981
        case SubroutineExit:
        case InterruptExit:                                                             // :1984-85
            if (!stack.empty()) {
                auto& top = stack.back();                  // <-- NO stack-pointer match. NO mode match.
                uint64_t dur = ev.cycle - top.startCycle;  // :1989
                routineMap[top.address].cycles += dur;     // :1990
                routineMap[top.address].calls++;           // :1991
                stack.pop_back();
            }
            break;
    }
    while (!stack.empty()) {                               // :2007
        auto& top = stack.back();
        uint64_t dur = snap.endCycle - top.startCycle;     // :2010
        routineMap[top.address].cycles += dur;             // :2011
        routineMap[top.address].calls++;                   // :2012
        stack.pop_back();
    }
}
```

and the publication step (`:2041-2043`):

```cpp
uint64_t avgCycles = st.cycles / numFrames;   // floor
int      avgCalls  = st.calls  / numFrames;   // floor
if (avgCalls < 1) avgCalls = 1;               // fabricated up to 1
```

Five properties fall straight out, and four of them are structural, not incidental:

- **P1 — inclusive of callees, and inclusive of preemption.** `dur` is a raw timestamp difference. An
  interrupt that fires inside a routine is inside that routine's `dur`.
- **P2 — the reconstruction stack is thrown away and rebuilt empty at every frame** (`:1972` is inside
  the `fi` loop). This is the corpus doc's W4.
- **P3 — a return is paired with the top of the LIFO, with no verification of any kind.** No
  stack-pointer match, no privilege check, no address check. `SubroutineExit` and `InterruptExit` share
  one arm, so an `RTE` can close a `JSR` frame and an `RTS` can close an interrupt bucket.
- **P4 — an `Exit` seen with an empty stack is silently dropped** (`if (!stack.empty())`).
- **P5 — a frame's still-open entries are flushed at the V-INT with `snap.endCycle - startCycle`**
  (`:2007-2012`), **and each gets a `calls++`.**

### 2.4 Their frame seam

`linux-port/gui/main_gui.cpp:2002-2012`:

```cpp
if (ev.type == ProfileEventType::FrameBoundary) {
    snap.startCycle = profilerPendingEvents.front().cycle;   // :2008  <-- FIRST EVENT, not the previous seam
    snap.endCycle   = ev.cycle;                              // :2009  <-- the V-INT
    ...
}
```

The `FrameBoundary` event is emitted only for `Exceptions::InterruptAutoVectorL6` (`M68000.cpp:926-927`).
So their frame is **first-profile-event → V-INT**, exactly as the corpus doc §6.3 records — which
understates `snap.totalCycles()` by the gap from the V-INT to the first `Enter`. **That affects their
`total_cycles` denominator and the `pct` column only; it does not touch a routine row's `cycles`.**

One further hazard worth naming since I read it: `ProfileRingBuffer::push` (`Devices/M68000/ProfileTypes.h`)
overwrites without any full check and never advances `_readIdx`, so a drain that falls more than 65 536
events behind returns torn/out-of-order events. At 60 Hz drain this is very unlikely to fire; it is
listed as a hazard, not as a proposed mechanism.

---

## 3. The two definitions side by side, and the predicted delta

| | **ours** | **theirs** |
|---|---|---|
| bracket opens at | the callee's **first instruction** (`profiler.rs:894-900`) | the **`JSR`/`BSR` itself** (`JSR.h:110`, `BSR.h:69`, stamped before `M68000.cpp:1030`) |
| bracket closes at | the **`RTS`, inclusive** (`profiler.rs:926` before `:943`) | the instruction **before** the `RTS` (`RTS.h:48`, same reason) |
| entry prefetch | caller (inside `JSR`'s total, `decode.rs:3512-3513`) | callee (inside `JSR`'s total, which is inside their bracket) |
| cycles per instruction | **ideal + bus wait** (`microop.rs:1611` etc.) | **ideal only** (`M68000.cpp:1029-1031`) |
| stall reported | yes, separately and as a subset (`bus.rs:127-141`) | never — it is in `_currentTime`, which the profiler does not read |
| callee time | included (`profiler.rs:718-726`) | included (timestamp difference) |
| **preemption** | **excluded** (`profiler.rs:637-643`, `:727`) | **included** |
| return→entry pairing | **exact `entry_sp` + privilege match, innermost-first over the whole stack** (`profiler.rs:747-766`) | **`stack.back()`, unverified** (`ControlSocket.cpp:1986-1996`) |
| stack at a frame boundary | **kept** — *"a call that straddles a boundary is one call, not two"* (`profiler.rs:418-420`) | **discarded and rebuilt empty** (`ControlSocket.cpp:1972`) |
| unmatched return | closes nothing (`profiler.rs:758-760`) | closes the top entry, or is dropped if the stack is empty (`:1987`) |
| routine still open at the seam | keeps accruing, `calls: 0` (`profiler.rs:98-103`) | **closed at the V-INT with a truncated `dur` and a `calls++`** (`:2007-2012`) |
| `RTR` | a return (`decode.rs:227`) | **not hooked at all** |
| published `calls` | real count, undivided partner beside it | `max(1, floor(total/frames))` (`:2041-2043`) |

### 3.1 The bracket delta — derivable, signed, small

Per invocation, **theirs − ours = cost(JSR/BSR) − cost(RTS)**.

Their table (`Devices/M68000/JSR.h:11-13`): `(An)` 16 · `(d16,An)` 18 · `(d8,An,Xn)` 22 · `xxx.W` 18 ·
`xxx.L` 20 · `(d16,PC)` 18 · `(d8,PC,Xn)` 22. `BSR` = 18 (`BSR.h:52`). `RTS` = 16 (`RTS.h:35`).

| entry form | predicted theirs − ours |
|---|---:|
| `jsr (An)` | **0** |
| `bsr` / `jsr (d16,An)` / `jsr xxx.W` / `jsr (d16,PC)` | **+2** |
| `jsr xxx.L` | **+4** |
| `jsr (d8,An,Xn)` / `jsr (d8,PC,Xn)` | **+6** |

**Sign: theirs is HIGH or equal, never low.** Magnitude: 0–6 cycles per invocation, full stop.
(INFERENCE, small: I did not read our own cycle totals for `JSR`/`RTS` off a table — our recipes are
pinned against SST vectors and the corpus doc §5.1 shows cycle-exact agreement over 1878 cycles/frame on
a stall-free row, which is strong corroboration that our figures equal the standard 68000 ones. If ours
ever differed, the delta above would shift by that difference.)

**A falsifiable prediction that costs nothing to check on the ROM side:** the corpus's PHASE-0 reference
row `$FFB452` agrees **exactly**, delta 0, at 4 entries per frame (corpus doc §5.1). Under this model
that is only possible if the row is entered by **`jsr (An)`** — the one form with a zero delta — *and*
`$FFB452` is a `JSR` target rather than the HInt vector itself. (If it were the vector, their bucket
would carry the 44-cycle entry and drop the 20-cycle `RTE` while our row carries the `RTE` and not the
entry: predicted +24/fire = +96/frame, and the measured delta is 0.) **This is a hard, cheap test of the
whole bracket model and I hand it to the ROM-side agent.**

### 3.2 The preemption delta — derivable in sign, not in magnitude

Theirs folds an interrupt that fires inside a routine into that routine's `dur` (§2.3 P1); ours never
does (`profiler.rs:637-643`). **Sign: theirs HIGH.** Magnitude: the cost of whatever interrupts landed
inside, which is data, not source. Expected value scales with the routine's length, so it is
*proportional*, not a fixed offset.

### 3.3 The pairing delta — the mechanism the arc was missing

This is the one that is structural, both-signed, state-dependent, and length-scaling in the right way.

Our closer requires an exact `entry_sp` + mode match (`profiler.rs:747-766`). **Theirs pops
`stack.back()` unverified, from a stack rebuilt empty every frame** (`ControlSocket.cpp:1972`,
`:1986-1996`). So their event stream can *desynchronise*, and once it does, every later pairing in that
frame is shifted. There are exactly four desynchronisers, all visible in their source:

- **D1 — an excess `Exit`:** an `RTS` with no matching `Enter`. Produced by (a) a routine **entered by
  `JMP`** (not hooked) that returns by `RTS`; (b) the `move.l #target,-(sp)` / `rts` **dispatch idiom**,
  which their instrument cannot distinguish from a return because it does not look at the stack pointer;
  (c) an `RTE` from a `TRAP`, which emits `InterruptExit` (`RTE.h:57`) with no `InterruptEnter` (their
  only `InterruptEnter` site is the interrupt path, `M68000.cpp:928`).
- **D2 — an excess `Enter`:** a routine entered by `JSR` that never emits an `Exit`. Produced by (a) a
  **tail `JMP` out** to a path whose `RTS` is consumed elsewhere; (b) **`RTR`, which is not hooked at
  all** (§2.2); (c) any call that straddles the frame seam.
- **D3 — the seam itself:** every call open at the V-INT is an excess `Enter` in frame *N* (flushed at
  `:2007-2012`) and its real `Exit` is an excess `Exit` in frame *N+1*.
- **D4 — interrupt/subroutine confusion:** the two `Exit` kinds share one arm (`:1984-1985`), so an
  imbalance inside a handler leaks into mainline pairing and vice versa.

**Now the signs, derived rather than assumed.** Take a frame's event list and inject one stray event:

*One excess `Exit`* into `E1 E2 X2 X1`:
`E1 E2 [X✗] X2 X1` → `X✗` pops E2 early (**E2 reads LOW**); `X2` pops E1 early (**E1 reads LOW**); `X1`
finds an empty stack and is dropped (P4). **An excess `Exit` makes victims read LOW, uniformly.**

*One excess `Enter`* into `E1 X1 E3 X3`:
`E1 [E✗] X1 E3 X3` → `X1` pops `E✗` (its cycles go to the *wrong address*); `E3`/`X3` pair correctly;
and **`E1` is never closed, so the end-of-frame flush charges it `snap.endCycle − t(E1)`** — everything
from its entry to the V-INT. **An excess `Enter` makes the victim one level out read HIGH, and the
overshoot can be enormous** (bounded by the distance from its entry to the frame's V-INT).

**This answers the controller's question directly, and the answer is "yes, but not by the route the W4
note describes."** The W4 note says a straddling call's real `RTS` in the next frame *"pops an unrelated
entry"*. Read against `:1987` that route is **mostly harmless**: real programs unwind strictly LIFO, so
every call made after the seam and nested inside a straddling frame has already been popped by the time
the straddler's `Exit` arrives — the stack is empty and P4 drops it. **The sign-correct route is the
other half of the same event: the excess `Enter` left behind in frame *N*, closed by the flush at
`:2007-2012`, which charges a victim everything up to the V-INT.** That is a mechanism that *adds*
cycles to a victim, it is bounded by position-in-frame rather than by routine length, and it fires only
in the frames where a desync actually occurs.

**The corpus doc's rejection of W2 was tested on the wrong partition, and I say so precisely.** §6.3
rejected "unhooked `JMP`" by checking *which routines contain a `4EF9`*. But a desync is **not local**:
one stray event mis-pairs **every subsequent `Exit` in that frame**, whichever routine it belongs to. So
`Camera_Update` containing a `JMP` and agreeing to 2.4% does not refute the mechanism, and
`Palette_Compose` containing none and disagreeing by 17% does not refute it either. The correct partition
is *"does a desync occur between this routine's `Enter` and its `Exit`, or between its `Enter` and the
frame seam"* — which is a property of the frame's event ordering, not of the routine's own opcodes.
**That reopens W2.**

---

## 4. The empirical facts, marked

Using the **measured per-video-frame** figures supplied by the corpus author (their `cyc/logic-tick`
column is a reconstruction, not a measurement, and is excluded), against a 30-frame window, with our
`callsTotal` at 29 over 31 frames at idle.

### Fact 1 — `Parallax_Update` at idle: theirs 19511/frame, ours 20196/invocation. **EXPLAINED (as "no mechanism fires here"), and it survives.**

29 invocations × 20196 = 585,684 over the sample; theirs `avgCycles = 19511` ⟹ `st.cycles ∈ [585330, 585359]`
⟹ **theirs is low by ~325–354 over the whole sample, i.e. ~−12 cycles per invocation, −0.06%.**

- §3.1 bracket: predicts **0 to +6** per invocation. Consistent in magnitude, wrong in sign by ~12
  cycles — which is inside the fuzz introduced by the two instruments sampling *different 30/31-frame
  windows* (their window need not contain the same 29 invocations as ours). **Not contradicted, not
  confirmed.**
- §3.2 preemption: at idle there are 4 HInt fires/frame costing ~470 each (corpus §5.1: 1878/frame), and
  a 20,196-cycle invocation occupies ~16% of a frame, so the *expected* number of fires landing inside is
  ~0.63 → an expected **+296/invocation (+1.5%)**. **The measured −0.06% contradicts this.** Either
  `Parallax_Update` runs in a window where HInts do not fire (a raster-program phase question — ROM-side),
  or preemption is not a live term on this row. **Marked CONTRADICTED for this row; §3.2 must not be
  applied as a general proportional inflation until that is settled.**
- §3.3 pairing: predicts **no error unless a desync occurs in this routine's neighbourhood**. The
  measured near-zero is exactly the "no desync here" case. **Consistent.**

**Nothing I propose materially inflates this row.** §3.1 caps at +6/invocation (+0.03%). §3.3 is zero
absent a desync. Only §3.2 would, and Fact 1 is the evidence against §3.2.

### Fact 2 — `BgAnim_Update` at idle: ours a hard constant 154.0/invocation, theirs **181/frame**. **PARTIALLY EXPLAINED — mechanism named (§3.3, excess-`Enter` branch), magnitude not derivable from source.**

Ours over 30 frames: 154 × 29 / 30 = **148.9/frame**. Theirs 181. **Excess = +32.1/frame = +963 cycles
over the 30-frame sample.**

- §3.1 bracket: **at most +6/invocation → at most +5.8/frame.** Covers 18% of the gap at the very most.
  **Insufficient alone.**
- §3.2 preemption: an HInt landing inside a 154-cycle routine costs ~470. Two such hits in 30 frames give
  +31.3/frame — arithmetically close, but this is **fitting**, and it is the same mechanism Fact 1
  contradicts. **Not adopted.**
- §3.3 pairing, excess-`Enter` branch: two shapes both reach +963, and source cannot choose between them:
  - **one flush event.** If in a single frame `BgAnim_Update`'s `Enter` is left unmatched, the flush at
    `:2010` charges it `snap.endCycle − t(Enter)`. To add 963 net it needs to sit ~1117 cycles before the
    V-INT in that one frame. **Requires exactly one desync in 30 frames** — and at idle the corpus's own
    lag pattern is *one lag frame in 31*, so exactly one boundary in the sample behaves differently from
    the other thirty. **The arithmetic works and the count matches.**
  - **a systematic one-position shift.** If `BgAnim_Update`'s `Exit` is consistently consumed by an
    earlier stray `Exit` and its own row is instead closed by the *next* event, the overshoot is the
    caller's residual work — ~32 cycles/invocation would do it. Systematic, small, and equally consistent.
- **Why it disagrees at idle and agrees at max-diagonal:** the corpus author's own arithmetic settles the
  max-diagonal side without reference to attribution (154/2.067 = 74.5 vs their measured 74), so **the
  max-diagonal agreement is not evidence about cycle attribution and I do not model it.** What remains is
  that a desync is a property of *where the frame seam falls in the program's call structure*, which is
  precisely what changes between a 1.03-frame logic tick and a 2.07-frame one. §3.3 is state-dependent by
  construction; §3.1 is not (it is a fixed per-invocation offset) and §3.2 is only weakly so.

**Verdict: mechanism named and sign-correct; the choice between "one flush event" and "a systematic
one-position shift" needs the paired trace.** That is a real narrowing — the arc had no named mechanism
at all — but it is not a closure.

### Fact 3 — `Palette_Compose`: ours **exactly 180.0** at both states; theirs **145/frame** (idle) and **67/frame** (maxdiag). **EXPLAINED IN SIGN AND IN THE CONSTANT-VS-NOT ASYMMETRY; magnitude not derivable.**

- **Why ours is a constant and theirs is not** is now a source fact, not a puzzle. Our bracket is closed
  by an exact `entry_sp` + privilege match (`profiler.rs:753-757`), so an invocation of a
  data-independent routine yields the same number every time, at every camera state, by construction.
  Theirs is closed by LIFO position in a per-frame event list (`ControlSocket.cpp:1972`, `:1988`), so its
  value depends on **where the frame seam and any desync fall relative to this routine** — which is state.
  **A definition that reads a routine's own instructions produces a constant; a definition that reads its
  neighbours in an event stream cannot.**
- **Sign.** Theirs reads LOW. §3.1 predicts HIGH (0..+6) and §3.2 predicts HIGH — **both are the wrong
  sign for this row and neither can produce it.** §3.3's **excess-`Exit`** branch is the only one of the
  three that produces LOW, and it does so by closing the entry early. **So Fact 2 and Fact 3 require
  *opposite branches of the same mechanism*, which is exactly what §3.3 supplies and what a fixed offset
  or a one-signed inflation cannot.**
- Magnitude: not derivable from source.

### The one row I explicitly do NOT claim

`VInt_Level` (corpus §6.3/§11.1) sits on their frame seam by construction (§2.4) and additionally hits
§3.1's *interrupt* form (their bucket carries the 44-cycle entry, ours puts it in the bucket and the
`RTE` in the handler's row). Three mechanisms at once. **Unexplained, and I am not stretching a model
over it.**

---

## 5. The VDP-port partition — falsifiable, and it converts to a ROM-side test

From §1.2 and §1.3, our per-instruction cycle figure diverges from a pure ideal 68000 count **if and only
if** one of these holds:

1. the instruction **writes** the VDP data port while its 4-entry FIFO is full (`vdp.rs:1302-1309`);
2. the instruction **reads** the VDP data port while the write FIFO has not drained (`bus.rs:1114-1130`);
3. the instruction **arms a 68k→VDP DMA** (`bus.rs:1184-1213`);
4. the step is a `Stopped`/`Halted` idle poll (`microop.rs:3250`, `:3266`) — 4 nominal cycles, not a
   68000 timing.

**Cases 1–3 are exactly and completely what `stall_cycles` accumulates (§1.3 table). Case 4 is not.**
Therefore, over the fourteen profiled routines:

> **PARTITION.** A routine's `cyclesTotal` differs from the sum of its instructions' ideal 68000 cycle
> counts **iff** `stallCycles != 0` **or** the routine spins in `STOP`. Control-port writes, status
> reads, HV reads, VRAM fills and VRAM copies contribute **nothing** — a fill/copy is 68k-transparent
> (`bus.rs:1167-1176`) and a control-port write never stalls (`bus.rs:1149-1152` returns only
> `run_pending_dma`, which is 0 unless the command was a mem-DMA).

Consequences the other agent can act on without running anything:

- **All five disputed rows report `stallCycles: 0` ⟹ our figure is a pure ideal count on all five ⟹
  caveat 0 cannot be any part of their explanation.** The instrument difference on those rows is 100%
  bracket + preemption + pairing.
- The partition predicts a **clean split of the fourteen** by `stallCycles`, and the corpus data already
  shows it: only `VInt_Level` and `VInt_Lag` carry non-zero stall, and they are the only two rows the doc
  could not reconcile arithmetically.

**What falsifies it.** Any one of:

- a routine with `stallCycles == 0`, not containing `STOP`, whose `cyclesTotal` disagrees with a
  hand-summed ideal cycle count over its executed instruction stream (a listing-level sum — no emulator
  needed);
- a wait returned by any `Bus68k` arm not listed in the §1.3 table (falsifiable by re-reading
  `bus.rs:1217-1355`; I read all five methods and every arm);
- a micro-op that reaches the bus without folding `wait` into its returned cost (falsifiable against the
  nine sites listed in §1.2).

Beware the trap the brief names: this partition **predicts a clean constant** for our side, and a clean
constant is normally a confound. Here it is not fitted — it is derived from a completeness argument over
five method bodies, and the argument names its own falsifiers above.

---

## 6. So: are they measuring the same quantity?

**No.** They agree on "cycles retired between two points in a call", and they disagree on all three of:
*which two points* (§3.1), *whether a preemption counts* (§3.2), and *how the second point is identified*
(§3.3). Only the third is capable of producing errors of the size and the two signs the disputed rows
show, and only the third is state-dependent. The first is bounded at 0–6 cycles per invocation and the
second is contradicted by the control row.

The sharpest way to say it: **ours measures a routine; theirs measures the interval between two events in
a stream, and identifies the closing event by position rather than by identity.** On a program whose
event stream is perfectly balanced within every frame the two coincide to within §3.1's 0–6 cycles. On a
real program with tail calls, dispatch idioms, `RTR`, and calls that straddle the V-INT, they cannot.

---

## 7. Everything marked as inference rather than finding

1. **Our `JSR`/`RTS` cycle totals equal the standard 68000 table** (§3.1). I read the *recipes*
   (`decode.rs:3421-3530`, `:3568-3605`) and confirmed their composition, not a numeric total. Corroborated
   by the corpus doc §5.1's cycle-exact 1878/frame agreement on a stall-free row. If ours differ, §3.1's
   table shifts by that difference.
2. **`$FFB452` is a `jsr (An)` target rather than the HInt vector itself** (§3.1). *Derived as a
   prediction from the model plus the observed delta of 0* — it is a test to run, not a fact I verified.
   ROM-side.
3. **The magnitude of §3.3 on any given row.** Both candidate shapes for Fact 2 (one flush event vs a
   systematic one-position shift) reach +963; source cannot choose. Needs the paired trace.
4. **The count "one desync in 30 frames"** matching the corpus's "one lag frame in 31" (§4 Fact 2). The
   arithmetic is mine; the correspondence is suggestive and is *not* established.
5. **§3.2's expected magnitude on `Parallax_Update`** (~+296/invocation) assumes HInt fires are uniformly
   distributed over the frame. They are raster-program-scheduled, so they are not. The contradiction in
   Fact 1 is therefore softer than it reads — but it is enough to bar §3.2 from being applied as a general
   proportional inflation.
6. **The ring-buffer overwrite hazard** (§2.4) is a code reading, not an observed event. Listed as a
   hazard, never used as a mechanism.

## 8. BLOCKED

Nothing. Every question in the brief was answerable from the two source trees. Two things are **out of
scope by construction** rather than blocked: the ROM's actual call shapes (`jsr` addressing modes,
`JMP`/`RTR`/dispatch-idiom sites per frame, whether `VSync_Wait` uses `STOP`) belong to the ROM-side
agent, and the paired event-level trace needs the reference running, which this task forbids.

## 9. Handoff — the three cheapest next tests, in order

1. **The `$FFB452` entry form** (§3.1, inference 2). A listing lookup. If it is `jsr (An)`, the bracket
   model is confirmed on the sharpest row in the corpus; if it is the HInt vector, the model predicts
   +96/frame and is refuted outright.
2. **The desync census** (§3.3). Count, per frame at each camera state: routines entered by `JMP`,
   `move.l/rts` dispatch sites, `RTR` returns, `TRAP`s, and calls open at the V-INT. **Zero desyncs
   anywhere in the frame kills §3.3.** Note this is a *different* partition from the one §6.3 already
   tested and rejected.
3. **The ideal-sum spot check** (§5). Hand-sum `Palette_Compose`'s ideal cycles from the listing and
   compare against our 180.0. If it matches, our side is confirmed exact and the entire residual lives on
   theirs.
