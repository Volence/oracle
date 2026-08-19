# The CPU profiler — recon + design (2026-08-19)

**Status: recon and design only. No code was written; nothing under `crates/` is touched by this
arc's two commits.** Branch `profiler-recon`, cut from `m68000-microop-framework` at `39c9717`.

Companion: `docs/2026-08-19-aeon-profiler-demand.md` — the demand side transcribed from source.
Read that first; this document assumes it.

---

## 0. Recommendation, in one page

**Mechanism: an exact, call-graph shadow stack driven by one new defaulted retire hook.**

Everything a routine-level profiler needs is already on this core's seams *except one number*: the
just-retired instruction's cycle cost. It exists as a live binding in the run loop
(`crates/oracle-core/src/system.rs:1035`, `let cycles = self.step_cpu(sink);`) and is never handed
to the sink. One additive, defaulted `BusEventSink` method closes that gap, and the precedent for
adding exactly that is set three times over in `bus.rs` itself (`on_event_at`, `on_frame_boundary`,
`stop_requested`).

With that hook the profiler is a **caller-owned sink** in the `Watchpoints` mould — `System` never
stores it, it never writes to the machine, and it is in neither frozen currency by construction.

| Question | Answer | Why |
|---|---|---|
| Attribution | Shadow stack over JSR/BSR/RTS/RTE + IACK, charging each instruction's **exact** cycles | Exact beats sampled; our per-instruction cycle counts are SST-validated ground truth |
| Sampling | **Rejected** | The consumer's merge gate compares with `==` (`aeon/tools/effects_gates.py:754-756`). A sampled number cannot be `==`-gated. Exact is also *cheaper* here than on the reference. |
| Interrupt cause | The **fc = 7 IACK bus event**, level 4 = HInt / level 6 = VInt | Already on the sink stream today (`crates/oracle-core/src/bus.rs:1063`) — **zero core change**, and it is a *cause*, not a handler PC |
| Frame division | Frame-aligned by construction; report exact **totals** plus `frames`, derive per-frame | Deletes the reference's `frames: sample-1` folklore and its two fabricated values |
| Cost when off | The `&mut ()` path gains one inlined-away call, matching `on_step_boundary` | The house's own standard, stated at `system.rs:971` |
| Contract | **CR-26 amends three §6 rows that already exist** and adds their first schema fragments | `protocol.md:1311-1313` already catalogues the legacy shape; the schema calls `profiler` a deferred family |

**Three things this design does that the reference cannot**, all additive, none costing the floor:

1. **Self *and* inclusive cycles.** The reference has only inclusive, so its `pct` sums far past
   100%. Ours falls out of the same stack for free.
2. **Split HInt / VInt buckets keyed by cause.** The pinned rule, and a budget row Aeon has never
   been able to measure.
3. **Symbol-resolved `name` and a canonical `addr`.** Both delete consumer code that exists today.

**The one finding that must reach the demand side before any number is compared:** the reference's
cycle clock **excludes bus and DMA stall time** (`oracle/Devices/M68000/M68000.cpp:1029-1031`).
Ours includes it. Their whole Phase-0 corpus is a stall-free corpus. This is a first-order,
mechanism-backed A/B disagreement, and §7.4 designs the comparison around it rather than tolerancing
it away.

---

## A. What the reference actually does

Recon of `/home/volence/sonic_hacks/oracle` (C++ Exodus port + a Python MCP shim), done statically.
Every claim is anchored. This section exists so the design can consciously not reproduce what it
finds; the numbered wart list is at §A.6.

### A.1 The call chain

| Layer | Anchor |
|---|---|
| MCP tool table (pure data, no logic) | `oracle/linux-port/mcp/oracle_mcp.py:704`, `:719`, `:726` |
| MCP → bus (`emulator/<op>`, verbatim passthrough) | `oracle_mcp.py:889-903`, `:923-931`, `:966-971` |
| Bus dispatch (**on the connection thread, unlocked**) | `oracle/linux-port/gui/ControlSocket.cpp:2790-2824`, table `:2671-2673` |
| The three ops — **all aggregation math lives here, not in the core** | `ControlSocket.cpp:1919-1932`, `:1934-1942`, `:1944-2091` |
| GUI main-loop producer/consumer (arm, reset, ring drain) | `oracle/linux-port/gui/main_gui.cpp:1966-1990`, `:1992-2035` |
| Data types | `oracle/Devices/M68000/ProfileTypes.h:1-111` |
| CPU emit sites | `JSR.h:109`, `BSR.h:68`, `RTS.h:47`, `RTE.h:56`, `M68000.cpp:924-929` |

### A.2 Attribution — instrumented tracing, and what it misses

It is **not** PC sampling. Every JSR/BSR/RTS/RTE and every interrupt entry pushes a 16-byte event
into a 64 K ring, and `OpGetProfilerFrames` replays that ring through a shadow stack **at query
time**, rebuilding a `std::map<uint32_t, RoutineStats>` from scratch on every call
(`ControlSocket.cpp:1961`, `:1990-1991`). There is no persistent routine table.

Three structural holes:

- **`JMP` is not hooked at all.** There is no emit site in `JMP.h`; the design doc concedes it
  (`oracle/docs/superpowers/specs/2026-04-23-m68k-profiler-design.md:321`). Jump-table dispatch and
  tail calls are invisible, and their cycles are charged to whatever is on top of the stack.
- **`move.l addr,-(sp) / rts` dispatch** — idiomatic in Sonic-lineage engines — emits a bare
  `SubroutineExit` that pops and **prematurely closes the caller** (`ControlSocket.cpp:1984-2001`).
- **The shadow stack is re-initialised empty at every frame boundary** (`:1972`, inside the
  per-frame loop). A call straddling V-INT is closed with a truncated duration and an incremented
  `calls` at `:2007-2022`, and its *real* RTS in the next frame pops an unrelated entry.

**Cycles are inclusive of callees only** — the exit path charges the whole span to the top frame and
subtracts nothing from the parent (`ControlSocket.cpp:1984-2001`). There is no self/exclusive figure
anywhere, so `pct` sums well past 100% and the top-N list is dominated by outermost routines.
**Interrupts nest on the same stack** (`:1981-1982`), so a V-INT's cost is folded into whatever
subroutine it preempted.

**`calls` counts stack *pops*, not entries** (`:1991`), plus one extra per frame-boundary flush
(`:2007-2022`).

### A.3 The cycle base — and the finding that outranks the rest

`RecordProfileEvent` stamps `_currentCycle` (`oracle/Devices/M68000/M68000.h:93-96`), which is
advanced **only by the static per-opcode cycle table**. At `M68000.cpp:1029-1031`:

```cpp
double totalExecutionTime = CalculateExecutionTime(cyclesExecuted) + additionalTime;
_currentCycle += cyclesExecuted;
_currentTime  += totalExecutionTime;
```

`additionalTime` — bus wait states, VDP/DMA contention, memory-access stalls — goes into
`_currentTime` (a real-time double) and **never into `_currentCycle`**.

**Consequence:** every figure the reference has ever produced, including Aeon's F0–F8 corpus and
everything Phase 0 is measuring right now, is a **stall-free** figure. And Aeon's Phase-0 Task 5
asks the same instrument for a *max contiguous DMA stall* — a quantity its cycle counter excludes
by construction. This is §7.4's central A/B term.

The MCP tool text calls the instrument "cycle-accurate" (`oracle_mcp.py:708`); the design doc repeats
it at `:5`, `:29`. Neither notes the omission.

### A.4 The interrupt conflation — confirmed, and worse than reported

The classification appears **twice, verbatim duplicated** — `ControlSocket.cpp:1992-1999` (normal
exit) and `:2013-2020` (frame-end flush):

```cpp
if (top.isInterrupt)
{
    uint32_t vec = top.address;
    if (vec == 0x78 || (vec >= 0x70078 && vec <= 0x7FFFF))
        vintCycles += dur;
    else
        hintCycles += dur;
}
```

The variable is named `vec` and `0x78` is the **vector-table address** of the level-6 autovector
(`oracle/Devices/M68000/IM68000.inl:57`, `InterruptAutoVectorL6 = 0x1E`; `0x1E × 4 = 0x78`). But
`top.address` is the **handler entry PC**, captured by `GetPC()` at `M68000.cpp:928` *after*
`ProcessException` has already loaded the vector. The two halves never agreed: the design doc
specifies `interruptVector.GetData()` at `:122`, the core shipped the PC, and both landed in the
same commit `49eab16`.

**Net effect, generalised beyond Aeon's ROM:** unless a handler begins exactly at `$000078` or
inside the undocumented window `$070078–$07FFFF`, the `else` fires. So `interrupts.hint` is
**HInt + VInt summed** and `interrupts.vint` is **structurally always 0** on any realistic ROM —
and `vint.pct` prints `0.0` (`:2077`), a zero that means *unmeasurable*. Aeon's sharpened statement
(`docs/2026-08-18-aeon-scanline-readback-demand.md:199-202`) is exactly right: this mis-buckets for
any ROM whose vector points where the heuristic did not anticipate, and it is *silently wrong*
rather than missing.

The `0x70078…0x7FFFF` window has no explanation in code, comment, or doc. **BLOCKED on intent** —
recorded as unexplained rather than guessed at.

### A.5 "Divided inside", and the `frames: sample - 1` mystery, resolved

Four plain integer divisions, all truncating, no rounding, in `OpGetProfilerFrames`:

```cpp
uint64_t avgTotal  = totalCycles / numFrames;   // :2031
if (avgTotal == 0) avgTotal = 1;                // :2032   <-- fabricated denominator
uint64_t avgCycles = st.cycles / numFrames;     // :2041
int      avgCalls  = st.calls  / numFrames;     // :2042
if (avgCalls < 1) avgCalls = 1;                 // :2043   <-- fabricated call
uint64_t avgVint   = vintCycles / numFrames;    // :2070
uint64_t avgHint   = hintCycles / numFrames;    // :2071
```

`numFrames` is the **caller's `frames` parameter**, clamped to `[1, hist.count()]`
(`:1950-1953`) — not the number of frames a given routine actually appeared in. A routine that ran
once in a 60-frame window reports `cycles = total/60` and `calls = 1` (floored up from 0).

**Why the consumer's numbers come out exact anyway.** Truncation is one-sided and up to
`numFrames − 1` cycles low *per row*. Aeon never sees it because their fixture is deterministic and
static: every profiled frame is identical, so `total = N × per_frame` divides exactly. The
"exact to within 1 cycle" property is therefore a property of **their determinism**, not of the
instrument — and any frame-to-frame variation silently truncates.

**`frames: sample - 1` explained.** The frame ring is newest-first (`ProfileTypes.h:89-94`,
`get(0)` = newest) and `numFrames` walks back from newest (`:1966-1968`). A snapshot's span is
**first-event → V-INT**, not V-INT → V-INT (`main_gui.cpp:2006-2012`,
`snap.startCycle = profilerPendingEvents.front().cycle`), so the first frame after `set_profiler`
is arbitrarily short. Asking for `count − 1` drops exactly the oldest — the runt. **The `-1` is a
hand-compensation for a partial first frame**, and nothing in the tool or the docs says so.

`set_profiler(false)` does **not** clear history (`main_gui.cpp:2027-2035`); `get_profiler_frames`
never clears anything; enabling **always** hard-resets (`main_gui.cpp:1980-1989`), so a stray arm
destroys an in-flight sample.

### A.6 Wart list — what this design will not reproduce

Named so each can be checked off. Anchors are into `/home/volence/sonic_hacks/oracle`.

| # | Wart | Anchor | Our answer |
|---|---|---|---|
| W1 | Inclusive-only cycles; `pct` sums ≫ 100% | `ControlSocket.cpp:1989-1991` | self **and** inclusive (§D.1) |
| W2 | `JMP` / jump-table / tail calls never hooked | no emit site; design `:321` | §D.4 + `F-PROF-POSITIONAL` |
| W3 | `move.l/rts` dispatch prematurely closes the caller | `ControlSocket.cpp:1984-2001` | SP-anchored pop (§D.2) |
| W4 | Shadow stack reset per frame; straddling calls double-counted | `:1972`, `:2007-2022` | stack is sample-lifetime (§D.2) |
| W5 | Interrupts nest on the subroutine stack | `:1981-1982` | separate typed frames on one stack (§E.3) |
| W6 | **Cycle base excludes bus/VDP/DMA stalls** | `M68000.cpp:1029-1031` | our clock is the scheduler's (§D.3) |
| W7 | A "frame" is first-event → V-INT, not V-INT → V-INT | `main_gui.cpp:2008` | frame-aligned by construction (§F) |
| W8 | `hint`/`vint` keyed on handler PC vs a vector address; `vint` always 0 | `ControlSocket.cpp:1995`, dup `:2016` | cause-keyed at IACK (§E) |
| W9 | Undocumented magic window `0x70078…0x7FFFF` | `:1995` | n/a — no heuristic exists to document |
| W10 | `"$%06X"` on an unmasked 32-bit PC → `"$FFFFB452"` | `:2050` | `hex::addr(pc & 0xFFFFFF)` (§G.3) |
| W11 | `FindNearest(addr, 0)` — exact-match-only, fed a sign-extended addr | `:2047`, `Symbols.cpp:163-173` | `SymbolTable::resolve` (§G.3) |
| W12 | `name` silently falls back to the address string | `:2054` | `name` absent when unresolved |
| W13 | Truncating division, no reported error bound | `:2031`, `:2041-2042`, `:2070-2071` | exact totals + `frames` (§F.2) |
| W14 | Divisor is the requested window, not each routine's appearances | `:1950-1953`, `:2041` | totals are totals (§F.2) |
| W15 | `if (avgTotal == 0) avgTotal = 1;` — fabricated denominator | `:2032` | no fabrication; absent or refused |
| W16 | `if (avgCalls < 1) avgCalls = 1;` — fabricated call | `:2043` | `calls` is a count, never floored |
| W17 | `budget_cycles` hardcoded NTSC `128000`, twice; ~16 % wrong on PAL | `:2033`, `main_gui.cpp:7128-7129` vs design `:181` | derive from `TimingBasis`, or drop the key (§G.5) |
| W18 | Ring `push` has **no full check**; overflow corrupts out of order | `ProfileTypes.h:31-42` | no ring; accumulate in place (§D.2) |
| W19 | Hardcoded unconfigurable unreported limits (65536 events, 120 frames) | `ProfileTypes.h:29`, `:79` | §2.4-conformant bounded list (§G.4) |
| W20 | Drain tied to the GUI tick while `run_frames` runs on the socket thread | `main_gui.cpp:1993-2019` vs `ControlSocket.cpp:2336-2356` | single-threaded engine loop (§G.1) |
| W21 | Enabling always hard-resets; no resume | `main_gui.cpp:1980-1989` | stated reset semantics (§F.3) |
| W22 | `set_profiler(false)` discards the un-drained ring | `M68000.h:91` | nothing to drain |
| W23 | One flag means MCP-enable + window-visible + CPU-instrumented, persisted across runs | `GuiState.h:50`, `main_gui.cpp:7122` | bus arm and GUI lens are independent (§G.6) |
| W24 | GUI Pause silently stops recording; `get_profiler` still says `enabled: true` | `main_gui.cpp:2022-2026` | one owner, one state |
| W25 | History read from the connection thread while the main thread `std::move`s it | `ControlSocket.cpp:1974` vs `ProfileTypes.h:83` | structurally absent (§G.1) |
| W26 | The stack reconstruction is implemented **twice**, already divergent | `ControlSocket.cpp:1974-2022`, `main_gui.cpp:7252-7283` | one model, two renderers (§G.6) |
| W27 | `routines`/`interrupts` are hand-concatenated raw JSON with unescaped names, `snprintf`'d into `char[256]` | `:2036-2068`, `:2073-2078`, `:2088-2089` | `serde_json` |
| W28 | Empty history is an RPC **error**, not an empty result | `:1948` | empty result + `frames: 0` |
| W29 | `get_profiler_frames` returns **no per-frame rows** despite its name | `:2080-2090` | see §G.2 — the name is a lie we must decide about |
| W30 | `frame_count` echoes the request; `frames_recorded` counts pushes, not retrievable frames | `:2082`, `:1940`, `main_gui.cpp:2007` | one honest count |
| W31 | The entire `get_profiler_frames` surface shipped **undesigned** — no spec, no plan task | commit `49eab16` | CR-26 first (§G) |

**W29 deserves a sentence of its own.** The reference's `get_profiler_frames` returns exactly one
averaged object; there is no way to see frame-to-frame variance at all. Aeon's noise discipline
(`--repeat` across whole boots) exists partly because the instrument cannot show them variance
*within* a sample.

---

## B. Our core's seams — what exists, and the one thing that does not

### B.1 The sink trait

`crates/oracle-core/src/bus.rs:58`. Nine members; one required (`on_event`, `:59`), eight defaulted.
The two that matter here:

```rust
fn on_step_boundary(&mut self, _pc: u32, _frame: u64) {}      // bus.rs:75
fn on_frame_boundary(&mut self, _frame: u64) {}               // bus.rs:189
```

`BusEvent` itself (`bus.rs:48-55`) carries `{op, fc, addr, size, value}` — **no PC, no mclk, no
cycles**. The standing rule for adding anything is stated in the file, `bus.rs:184-188`:

> *"**Why on the trait and not on `BusEvent`:** the standing precedent of `on_event_at` (SY-4a) and
> `on_step_boundary` — extend the trait, never the event struct (it derives `Eq` and is recorded
> into `Vec<BusEvent>` by tests asserting exact event sequences)."*

That is the licence this design uses, and it has been exercised three times (`on_event_at`,
`on_frame_boundary`, `stop_requested`).

### B.2 The run loop, and the missing number

`System::run_until_with_sink`, `crates/oracle-core/src/system.rs:982-1083`:

```rust
sink.on_step_boundary(self.cpu.regs.pc, self.scheduler.now() / MCLK_PER_FRAME);  // :1027
if sink.stop_requested() { reason = StopReason::SinkRequested; break; }          // :1031
let cycles = self.step_cpu(sink);                                               // :1035
…
self.scheduler.advance(cycles as u64 * MCLK_PER_CPU_CYCLE);                      // :1062
self.cpu.set_ipl(self.vdp.ipl());                                               // :1070
```

**`cycles` at `:1035` is the exact CPU-cycle cost of the instruction (or exception entry) that just
retired, and it never reaches the sink.** That is the whole gap.

`Cpu68000::step` is the retire point (`crates/oracle-core/src/m68000/microop.rs:3212-3293`); the
count comes from `recipe.run_to_completion` at `:3264` and returns at `:3292`.

**Why that number is trustworthy.** Our per-opcode cycle counts are validated against
SingleStepTests — final state, cycle count **and** the per-cycle bus-transaction stream
(`docs/decisions/2026-06-24-cycle-granularity.md`, "Both paths are byte-identical to real hardware
traces … final regs/SR/RAM/prefetch, the cycle count, **and** the per-cycle bus-transaction
stream"). The corpus lives at `vendor/ProcessorTests/68000/v1/` and includes `JSR.json`,
`BSR.json`, `RTS.json`, `RTE.json`. **A sum of SST-validated per-instruction costs is the strongest
cycle number this project can produce.**

This is also the precise scope of the standing "instruction-granularity slop" caveat
(`docs/2026-08-17-aeon-switchover-gap-list.md:110-112`): the open question is *where inside an
instruction* an access lands, never *what the instruction costs*. **A routine-level profiler never
asks the open question.** What it does inherit is our system-level accounting of DMA/bus stalls,
which is a different axis and the honest subject of §7.4.

### B.3 Cost when disabled — the canonical pattern, named

Sinks are **monomorphized and passed per-run**, never registered (`bus.rs:10-12`). `System` never
owns one — `system.rs:909-910`: *"The sink is the caller's — `System` never stores it, so it is in
neither frozen currency and cannot move a state hash."*

`run_frames` passes `&mut ()` (`system.rs:904`); `impl BusEventSink for ()` (`bus.rs:453-455`)
inherits every empty default. The standard is stated at `system.rs:971`: *"the default
`on_step_boundary` is a no-op, so `&mut ()` is byte-for-byte the old hot path"*, and at `:980-981`
for `stop_requested`: *"the added code is `if false { break }` … unchanged **by construction**, not
merely by optimisation."*

Four existing instruments use the identical shape — **`Watchpoints`** (`watchpoints.rs:885-980`),
**`ScanlineCapture`** (`scanline_capture.rs:14-15`), **`VgmLogger`** (`vgm.rs:6`, `:276`), and
today's sub-line work (which added *no* trait method at all, reusing `wants_scanlines`).

Two-level arming is also house practice — `crates/oracle-aether/src/engine.rs:644`:

```rust
let armed = (self.watchpoints.watch_count() > 0).then_some(&mut self.watchpoints);
let mut sink = Fanout::new(&mut self.screen, Observe(armed));
```

with the reason at `:672-674`: an unarmed instrument still counting events *"costs the unarmed path
something for nothing"*.

**The profiler copies `Watchpoints` verbatim**: owned by `Engine`, attached through
`Option` + `Fanout` + `Observe` only when armed.

A new trait method must be added to the four forwarding impls or a composite silently drops it:
`&mut S` (`bus.rs:220-248`), `Option<S>` (`:253-293`), `Observe<S>` (`:312-341`),
`Fanout<A,B>` (`:374-409`).

### B.4 Control flow — where "this is a call" is already known

`crates/oracle-core/src/m68000/decode.rs`, in `decode_dispatch` (`:191`), already classifies by
exact opcode test:

| Instruction | Guard | Recipe |
|---|---|---|
| `BSR` | `decode.rs:899-901` (`opcode & 0xFF00 == 0x6100`) | `bsr_recipe` `:3156` |
| `JMP` | `:909-911` | `jmp_recipe` `:3236` |
| `JSR` | `:917-919` | `jsr_recipe` `:3352` |
| `RTS` | `:925-927` (`opcode == 0x4E75`) | `rts_recipe` `:3499` |
| `RTR` | `~:995` | `rtr_recipe` `:3635` |
| `RTE` | `:1006-1008` (`opcode == 0x4E73`) | `rte_recipe` `:3723` |

The **target** address is not a plain decode-time value — it is assembled into private scratch slots
(`JSR_TARGET_SLOT` `:3316`, `BSR_TARGET_SLOT` `:3132`, `JMP_TARGET_SLOT` `:23`). That does not
matter: **after a JSR/BSR the very next `on_step_boundary` PC *is* the callee entry address**, and
after an RTS it is the return site. The profiler learns targets by watching, not by decoding.

**There is no existing call-stack tracker anywhere in this tree.** A recursive grep for
`step_out|step_over|call_stack|callStack` across `crates/` returns nothing. `emulator/step_over`
and `emulator/step_out` are catalogued in the contract (`empyrean/contract/protocol.md:785-786`) and
exposed by the legacy MCP, but oracle-next advertises neither. **A profiler would be this tree's
first shadow-stack consumer — nothing to reuse, nothing to break.**

### B.5 The capability slot already exists, and it is off

`crates/oracle-aether/src/engine.rs:834`, inside `initialize`'s capabilities object:

```rust
"profiler": false,
```

described at `:830-831` as *"Method groups from the catalog that this thin slice does NOT implement.
Clients branch on these, never on the version integer (D5)."* **This is the exact key the arc flips
to `true`.** The schema already types it (`tests/contract/bus-protocol.schema.json:200`).

---

## C. Interrupt cause — the anchor, and it costs nothing

### C.1 The cause exists at the source, twice over

Two separate latches and two separate raise functions in `crates/oracle-core/src/vdp.rs`:
`vint_pending` (`:176`) / `hint_pending` (`:179`); `raise_vint` (`:1436`) / `raise_hint` (`:1442`);
public introspection `vint_pending()` (`:1396`) / `hint_pending()` (`:1401`).

Raised from the scheduler arms in `System::deliver_event`:

```rust
EventKind::HInt => { if self.vdp.hint_anchor_tick(line as u16) { self.vdp.raise_hint(); } }  // system.rs:1203-1208
EventKind::VInt => { self.vdp.raise_vint(); self.z80.set_int_line(true); }                   // system.rs:1211-1214
```

IPL is **combinational**, recomputed by `System` — there is no stored IPL (`vdp.rs:1385-1393`):

```rust
pub fn ipl(&self) -> u8 {
    if self.vint_pending && self.regs[1] & 0x20 != 0 { 6 }
    else if self.hint_pending && self.regs[0] & 0x10 != 0 { 4 }
    else { 0 }
}
```

**Level 6 ⇔ VBlank, level 4 ⇔ HBlank, bijectively, by construction.**

### C.2 The IACK is already on the sink's event stream

The interrupt is taken at `crates/oracle-core/src/m68000/microop.rs:3255-3259`:

```rust
if self.ipl > self.regs.int_mask() {
    let level = self.ipl;
    let recipe = crate::m68000::decode::interrupt_exception_recipe(level);
    return self.run_terminal(recipe, bus);
}
```

The recipe (`decode.rs:3869-3889`) pushes `MicroOp::IntAck { level }` (`exception.rs:348`), executed
at `microop.rs:2810-2817`, which performs an fc = 7 read at `0xFFFF_FFF1 | (level << 1)`. The bus
handles it at `crates/oracle-core/src/bus.rs:1052-1065`:

```rust
if fc == 7 {
    let level = ((a >> 1) & 0x07) as u8;
    self.vdp.acknowledge(level);
    let v = *self.last_bus_word;
    self.emit(BusOp::Read, fc, a, Size::Word, v as u32);   // <-- the sink sees this
    return (v, 0);
}
```

**So a sink receives `BusEvent { op: Read, fc: 7, addr, … }` where `addr` is `0xFFFFF9` for level 4
(HInt) and `0xFFFFFD` for level 6 (VInt).** Independently corroborated in
`crates/oracle-core/src/watchpoints.rs:439` (*"interrupt-acknowledge cycles at `$FFFFF9`/`$FFFFFD`
in CPU space"*) and exercised over the ROM corpus by `crates/oracle-core/tests/watchpoints.rs:769-785`.

**Cause-keyed interrupt bucketing therefore requires zero core change.** It is the level on the
bus, not a guess about a PC. This is the pinned rule
(`docs/2026-08-18-aeon-scanline-readback-demand.md:142-145`) satisfied structurally rather than by
discipline.

### C.3 Bucket edges — exact definitions

| Edge | Rule | Anchor |
|---|---|---|
| **Opens** | The step during which an fc = 7 event of level *L* was observed is the interrupt-entry step. At that step's retire, push an `Interrupt(L)` frame and charge the step's cycles (the pinned 44, `decode.rs:3863`) to it. | `bus.rs:1063`, `system.rs:1035` |
| **Closes** | At the retire of the `RTE` (opcode `0x4E73`) whose pre-execution SP equals the frame's recorded SP. The RTE's own cycles are charged to the bucket, then popped. | `microop.rs:2791-2797`, `decode.rs:1006-1008` |
| **Nesting** | Frames stack. Depth is whatever the CPU permits: `SetIntMask { level }` (`microop.rs:2804-2809`) raises the running priority, so VInt-inside-HInt is natural (6 > 4) and HInt-inside-VInt requires the handler to lower SR. | `microop.rs:3255` |

**Why SP-matching and not "the next RTE".** An RTE returning from a `TRAP` taken *inside* a handler
would otherwise close the interrupt bucket early — the same class of bug as reference wart W3. The
SP recorded when the entry frame finished disambiguates exactly.

**Deliberately not modelled as "the handler's PC range".** That is the reference's mistake. Our
bucket knows nothing about where the handler lives, which is precisely why it cannot be wrong about
a RAM trampoline or a ROM handler.

**⚑ Open, needs a runtime observation (not a design question):** whether any corpus ROM actually
nests HInt inside VInt. Static reading cannot answer it, and no emulator call may be made from a
background agent. Registered for the controller's foreground follow-up; the design is correct either
way, but a nesting fixture would be worth having in the test set if the answer is yes.

---

## D. Attribution mechanism — options priced

### D.1 The recommended design

**One new defaulted trait method**, at `crates/oracle-core/src/bus.rs`, alongside `on_step_boundary`:

```rust
/// The instruction (or exception entry) that just retired: the PC it started at, the opcode word
/// it was decoded from, the SP after it committed, and its exact CPU-cycle cost.
fn on_step_retire(&mut self, _r: StepRetire) {}
```

with `StepRetire { pc: u32, opcode: u16, sp: u32, cycles: u32 }` — a **new struct for a new hook**,
never a field on `BusEvent` (the `bus.rs:184-188` rule). One call site, at `system.rs:1035-1036`,
immediately after `let cycles = self.step_cpu(sink);` and before the `scheduler.advance` at `:1062`.
Every value it carries is already a live binding or a plain register read at that point:
`pc` is what `:1027` already stamped, `opcode` is `self.cpu.regs.prefetch[0]` read at the boundary,
`sp` is `self.cpu.regs.a[7]`.

**The profiler's state**, all of it caller-owned:

```
stack:  Vec<Frame>              Frame { key, entry_sp, self_start, child_cycles }
rows:   BTreeMap<RowKey, Row>   Row  { self_cycles, incl_cycles, calls }
key:    RowKey = Routine(addr) | Interrupt(level)
```

Per retire:
1. Charge `cycles` to the top frame's **self**.
2. If the step was an interrupt entry (an fc = 7 event of level *L* arrived during it) → push
   `Interrupt(L)`.
3. Else if `opcode` is a **call** (`JSR`, `BSR`) → arm `pending_call`; the *next* retire's `pc` is
   the callee entry, and that is when the frame is pushed with `entry_sp` recorded.
4. Else if `opcode` is a **return** (`RTS`, `RTR`, `RTE`) and the top frame's `entry_sp` matches →
   fold the frame's totals into its row (`self`, `self + children` as inclusive, `calls += 1`) and
   pop, adding its inclusive total to the new top frame's `child_cycles`.

Per frame boundary (`on_frame_boundary`, `bus.rs:189` / `system.rs:1187`): close the sample's frame
counter. **The stack is *not* reset** — that is wart W4, and the whole point of keeping it is that a
call straddling V-INT stays one call.

**What this buys, against the floor:**

| Property | Reference | Here |
|---|---|---|
| `cycles` inclusive | ✅ | ✅ |
| `cycles` self / exclusive | ✗ | ✅ (free — it is the accumulator) |
| `calls` exact under recursion | ✗ (one row, nested spans summed) | ✅ (depth-aware) |
| calls straddling a frame | double-counted, mis-closed | one call |
| `move.l/rts` dispatch | closes the caller | SP-matched, no false pop |
| stall cycles included | ✗ | ✅ |
| interrupt cost separated from the code it preempted | ✗ | ✅ (typed frame) |

### D.2 Where the opcode predicate lives — the one duplication risk

The profiler needs six opcode tests that `decode_dispatch` already performs. Copying them into the
profiler creates a **second source of truth for what a JSR is** — precisely the defect Aeon's own
probe warns about in its module note (`aeon/tools/raster_cost_probe.py:62-69`: a second
transcription is acceptable *only* because something checks it empirically).

**Design rule:** the predicate lives in `decode.rs`, next to the dispatch it mirrors, as a pure
function

```rust
pub(crate) fn control_flow_of(opcode: u16) -> ControlFlow   // Call | Return | Jump | None
```

and is **pinned by a test that walks all 65 536 opcodes** and asserts agreement with which recipe
`decode::decode` actually selects. A mirror that can drift silently is the thing this project keeps
finding; a mirror checked over its entire domain cannot.

### D.3 Option (ii) — flat entry-address attribution (the reference's shape)

Cheaper: no stack, no SP, no returns. Charge every instruction's cycles to the *most recent* address
control jumped to. What it misses, precisely: **self-vs-inclusive is not expressible at all**
(there is no notion of a parent), recursion is indistinguishable from iteration, and a routine that
falls through into its neighbour is silently merged. It also still needs the retire hook, so it
saves only the stack — perhaps 60 lines. **Rejected**: the saving is small and the losses are the
two things the demand side's Tasks 2 and 4 will most want.

### D.4 Option (iii) — PC sampling

**Rejected, with a reason stronger than the usual one.** The consumer's merge gate compares
profiler output with `==` against constants derived from engine source
(`aeon/tools/effects_gates.py:754-756`, `:773`). A sampled figure cannot be `==`-gated at all, so
sampling would not merely be less accurate — it would make the migration impossible. And the
determinism bar (`spread 0 across 3 boots`) is a statement that this consumer wants an *instrument*,
not an *estimator*. Our scheduler makes exact affordable; sampling would be paying accuracy for a
speed we do not need.

### D.5 Option (iv) — symbol-range (positional) attribution, as an additive second lens

Attribute each instruction's cycles to `SymbolTable::resolve(pc)` — nearest-preceding within an
`AddrSpace` (`crates/oracle-core/src/symbols.rs:551-562`). Properties:

- **Immune to W2 and W3 structurally**, because attribution is positional and never derived from
  call/return pairing. A JMP-dispatched routine gets its own cycles.
- `calls` = transitions into a symbol with displacement 0 — which counts JMP-dispatched entries too.
- Requires symbols, and is only as good as the listing's label granularity: a loop label inside a
  routine would split it. `Symbol` already carries `is_synthetic` (`symbols.rs:159-162`) and
  `ambiguous` (`:163-166`) to help, and `addr` is **already the 24-bit bus address** (`:153`).

**Not the primary mechanism** — it cannot produce inclusive cycles and it depends on symbols the
floor does not require. **Registered as `F-PROF-POSITIONAL`**, and it has a use beyond coverage:
running both and **diffing them localises exactly the routines W2 would have mis-charged**. That is
a real diagnostic, not a redundancy.

### D.6 Cost accounting

**Disabled** (`&mut ()`): one extra call at `system.rs:1036` whose body is empty and inlines away,
plus the construction of a 16-byte `StepRetire` the optimiser sees is unused. Identical in kind to
`on_step_boundary`, and the same claim applies verbatim — unchanged **by construction**, not by
optimisation. The gate that proves it is §H.

**Armed:** one `BTreeMap` probe per call/return, an integer add per instruction, and a `Vec` push/pop
per call. No allocation in the steady state after the first few hundred rows. There is **no ring
buffer and no drain** — the reference's entire W18/W20/W25 cluster exists because it defers
reconstruction to query time; we accumulate in place, on the one thread that owns the machine.

---

## E. Interrupt buckets — the reported shape

Two buckets, keyed by cause, never by handler PC:

```
interrupts: { hint: { cycles, calls }, vint: { cycles, calls } }
```

- `cycles` is the **inclusive** cost of the bucket: entry (44), the handler, everything it calls, and
  the RTE. A handler that spends its time in a subroutine still shows its full cost here — which is
  what a budget axis wants.
- `calls` is the number of times the interrupt was **taken** (IACKed), not the number of times it
  was raised. A raised-but-masked HInt does not appear. That distinction must be normative text.
- The routine rows are **unaffected** by the split: a handler's own entry address still gets its own
  `routines[]` row, so Aeon's existing per-routine discipline keeps working unchanged during and
  after the migration. The buckets are additive.

**What Aeon gets that they have never had:** `interrupts.hint.cycles` becomes the per-frame HInt
total their Task 3 (budget axis 4b) is defined by
(`aeon/docs/superpowers/plans/2026-08-18-scanline-p2-specialization-budget.md:153-182`), directly,
without summing model terms.

---

## F. Frames and the division

### F.1 What a frame is here

`on_frame_boundary` fires **exactly once per emulated frame, at the start of vblank** — the line-224
`Scanline` event (`system.rs:1176-1190`), which is *after* line 223's row and *before* any line of
the next frame. `bus.rs:149-159` records why line 0 is the wrong choice (a top-of-frame hook orphans
the final frame of every run).

Sharp edges the design must respect, all documented in `bus.rs`:

- **`frame` is not monotonic across a reset** (`bus.rs:178-182`): it is `mclk / MCLK_PER_FRAME`, and
  `System::reset` zeroes mclk. *"Count boundaries yourself … `frame` is a position on the emulated
  clock, not a tally."* → **the profiler counts boundaries; it never subtracts frame indices.**
- **"Once per frame" is a lifetime invariant, not a per-run one** (`bus.rs:169-176`): a run ending
  inside the ~one-line window between line 223's render and the line-224 event delivers **zero**
  boundaries, and `run_frames(0)` delivers none.
- The index comes from the *event's own deadline*, not `scheduler.now()`, because one `step_cpu` can
  advance the clock by more than a frame when a DMA is billed as CPU wait cycles (`bus.rs:161-167`).
  That same fact is why our cycles include stall time and the reference's do not.

Note this is **V-INT-adjacent-to-V-INT**, i.e. what the reference *intended* and did not implement
(W7). `vint_offset()` is H $02 into line 224 (`vdp.rs:1486-1488`), so the boundary and the VInt
raise are within a hair of each other.

### F.2 The division — exact totals, derived per-frame, nothing fabricated

**Rows carry exact integer totals over the sample**, plus the sample's frame count:

```
routines[]: { addr, name?, cycles, cyclesSelf, calls }        // totals over the sample
frames: <count of boundaries observed while armed>
```

and the per-frame figures the consumer asked for are **derived and reported**, with the divisibility
stated rather than hidden:

```
cyclesPerFrame, callsPerFrame, perFrameExact: bool
```

Rationale, point by point against the floor:

- **The demand is met** (`docs/2026-08-19-aeon-profiler-demand.md` §1.3): division happens inside the
  emulator, and for a deterministic static fixture it is exact — because each frame is identical, so
  `total = N × per_frame` divides exactly. That is the *real* mechanism behind "exact to within 1
  cycle", and stating it is better than inheriting it (§A.5).
- **`perFrameExact` is a typed key, not a caveat string.** §2.4's rule 3
  (`empyrean/contract/protocol.md:471-500`) is explicit: *"Any consequence a client must act on
  needs its own typed key."* A consumer whose gate uses `==` must be able to see that a division
  truncated.
- **No `if (avgCalls < 1) avgCalls = 1`** (W16) and **no `if (avgTotal == 0) avgTotal = 1`** (W15).
  A row with zero calls does not exist; a sample with zero frames returns `frames: 0` and empty
  rows, not an error (W28).
- Carrying totals as well as per-frame figures also gives Aeon something the reference never could:
  they can compute their marginal `(fixture − F0)/n` from exact integers rather than from two
  independently-truncated averages.

### F.3 Arm / disarm semantics — the `-1` deleted

**Normative, and this is the part that retires the folklore:**

1. `set_profiler{enabled:true}` **refuses on a running machine** or arms at the next frame boundary
   — the same class of decision as `write_memory`'s `-32005 machineRunning`
   (`crates/oracle-aether/src/engine.rs:1287`, `require_paused`). Aeon's probe already arms while
   paused between `run_frames` calls, so this costs them nothing and makes a partial first frame
   **inexpressible** rather than merely avoidable.
2. Arming **resets the accumulators**. Stated, not discovered (W21). There is no resume in v1;
   if one is wanted it is a separate parameter, not a surprise.
3. Disarming **retains** the accumulated sample so it can be read after the fact, and reading never
   clears. Both stated.
4. Arm, disarm and read are **synchronous with respect to the command** — a direct consequence of
   the engine loop owning the `System` on one thread and draining commands in order
   (`crates/oracle-aether/src/server.rs`, verified and recorded at
   `docs/2026-08-18-aeon-scanline-readback-demand.md:149-160`). **This is what deletes the probe's
   two hand-tuned `asyncio.sleep(0.4)` calls and the race behind them**
   (`aeon/tools/raster_cost_probe.py:523-533`, demand doc §7.1).

**Consequence for the migration, which must be flagged to Aeon rather than discovered by them:**
with a frame-aligned arm, `frames: sample - 1` becomes `frames: sample`, and their sample grows from
30 profiled frames to 31. Per-frame figures are unchanged for a static fixture; totals are not.

---

## G. Contract surface

### G.1 The rows already exist — this is an amendment, not an invention

**`empyrean/contract/protocol.md:1311-1313`, under `### status / misc`:**

```
| `emulator/set_profiler`        | `enabled`         | `enabled` |
| `emulator/get_profiler`        | —                 | `enabled`,`framesRecorded`? |
| `emulator/get_profiler_frames` | `frames`?,`top`?  | `frameCount`,`totalCycles`,`budgetPct`,`routines[]`,`interrupts{}` |
```

Three facts follow:

1. **The catalogued shape is the legacy one, warts included.** `budgetPct` is W17 written into the
   contract; `routines[]` and `interrupts{}` have no field-level specification at all; the group is
   *status / misc*, which is not where a measurement instrument belongs.
2. **There is no schema fragment.** The schema's own description names `profiler` among *"the
   deferred families … completed as each is implemented, per §8 item 20: a server's suite closes
   every result against its fragment, so a method without one cannot ship a result nobody has
   checked"* (`crates/oracle-aether/tests/contract/bus-protocol.schema.json:5`).
3. **`UNCOVERED_METHODS` is pinned empty** (`crates/oracle-aether/tests/schema_conformance.rs:142`).
   Advertising a profiler method without a fragment turns that test red — which is the mechanism
   forcing contract-first here, not merely the convention.

**So CR-26 is an amendment**: it re-specifies the three rows, gives them their first fragments, and
moves them out of *status / misc* into their own group. Next section number is **§11.16**; the last
is §11.15 (`protocol.md:2627`).

### G.2 Names and shapes

**Keep the three legacy names.** They are already catalogued, the legacy MCP already exposes them,
and Aeon's tool already calls them — renaming would buy tidiness and cost the migration its
drop-in property. The one name that lies is `get_profiler_frames`, which returns no per-frame rows
(W29); the honest fix is **to make it true** rather than to rename it (see the `perFrame[]` candidate
in §I).

Proposed result shapes, D9-typed (`protocol.md:110-127`: addresses are hex strings, counts are JSON
numbers):

```jsonc
// emulator/set_profiler  { enabled: bool }
{ "enabled": true }

// emulator/get_profiler  { }
{ "enabled": true, "frames": 31, "routineCount": 214 }

// emulator/get_profiler_frames  { frames?: int, top?: int }
{
  "frames": 31,                       // boundaries observed, counted (never mclk arithmetic)
  "totalCycles": 3701240,             // exact sum over the sample
  "cyclesPerFrame": 119394,
  "perFrameExact": false,
  "interrupts": {
    "hint": { "cycles": 318460, "calls": 6510 },
    "vint": { "cycles": 254100, "calls": 31 }
  },
  "routines": [
    { "addr": "0x00FFB452", "name": "HBlank_Vector_Slot",
      "cycles": 318460, "cyclesSelf": 318460, "calls": 6510 }
  ],
  "total": 214, "returned": 200, "limit": 200, "truncated": true
}
```

### G.3 `addr` and `name`

**`addr` = `hex::addr(pc & 0xFFFFFF)`** → `"0x00FFB452"`. `crates/oracle-aether/src/hex.rs:14-17`
is the house formatter (`0x` + 8 uppercase digits, D9); masking to the 24-bit bus address is what
kills W10, and it agrees with `Symbol::addr`, which is *already* the 24-bit form
(`crates/oracle-core/src/symbols.rs:153`).

**`name` = `Engine::symbol_at(addr)`** (`crates/oracle-aether/src/engine.rs:977-981`), i.e.
`SymbolTable::resolve` — nearest-preceding, never crossing an `AddrSpace`
(`symbols.rs:551-562`). Because a routine row's key *is* an entry address, the displacement will be
0 in the normal case, so the rendered name is the bare label. **Omit the key entirely when nothing
resolves** — never fall back to the address string (W12).

> **⚑ Controller ruling wanted, and it is not a profiler question.** The recon flagged an apparent
> contract divergence: `symbol_at` returns `Resolution::to_string()`, which appends `+$1A` for a
> non-zero displacement (`symbols.rs:207-211`), while the schema's `$defs/symbolName` pattern
> **forbids** that suffix and `emulator/status.symbolAtPc` `$ref`s it. This was found by static
> reading only and is **not** asserted as a bug — it wants a foreground `cargo test -p oracle-aether`
> and a live `emulator/status` at a non-label PC. It lands here because a profiler naming routine
> entries uses the same helper, so whichever way it is ruled, this arc should consume the ruling
> rather than pick a side. Registered as **`F-SYMBOLNAME-DISP`**.

### G.4 §2.4 conformance — the bounded list

`routines[]` is bounded by **policy** (the server picks a cap; `top` is a client hint). Therefore,
per `protocol.md:502-560`:

- **(a)** `total`, `returned` and `truncated` are **required**, `truncated` even when `false`;
  `limit` is optional. Helper: `rpc::bounded_array` (`crates/oracle-aether/src/rpc.rs:303-311`).
- **(b)/(d)** the method accepts no cursor, so it **MUST NOT** emit one — and `rpc::bounded_array`
  deliberately emits none (`rpc.rs:293-302`).
- **Spelling:** the list is a *field* of the result, not the whole result, so it takes the nested
  `$defs/boundedList` container form (`protocol.md:550-560`).
- **`top` out of range is refused, never clamped** — `hex::parse_count` (`hex.rs:86-89`), the same
  discipline `emulator/scanlines` states as *"refused never clipped — a decision, not drift"*
  (`docs/2026-08-18-cr24-scanlines.md:220`).
- **No constant caveat** (`protocol.md:492-500`). If `perFrameExact` is false, that is a typed key,
  not a caveat string.

### G.5 `budgetPct` — the catalogued wart

`budgetPct` in the existing row is the reference's hardcoded NTSC `128000` (W17), ~16 % wrong on PAL,
against a design doc that promised region auto-detection. Two options for CR-26:

- **(a) Drop it.** It is a presentation figure derivable by the client from `totalCycles` and the
  frame's cycle budget, and `emulator/status`/`TimingBasis` already publish the timing basis
  (`crates/oracle-core/src/system.rs:62-84`, accessor `:548`).
- **(b) Keep it, derived.** `MCLK_PER_FRAME / MCLK_PER_CPU_CYCLE` from `TimingBasis`, correct for
  whatever region the machine is in.

**Recommendation: (b).** It costs three lines, it removes a wart from the contract rather than a key
from it, and dropping a catalogued key is a subtractive change where an additive one is available.
Flagged for the ruling either way.

### G.6 Four-surface accounting (D15)

The rule, `protocol.md:238-244`: *"A server SHOULD expose through the bus every capability its own
GUI consumes: new debugger surface is built registry-first, the method (and its schema) before the
panel that renders it. A capability that exists only inside a panel is the `list_ops` drift of §0
re-created in pixels."* Ordering, from the Tier-1 plan
(`docs/superpowers/plans/2026-08-18-aeon-tier1-bus-methods.md:11-13`): contract → schema re-vendor →
handler → MCP → GUI.

| # | Surface | Files | Decision |
|---|---|---|---|
| 1a | §6 rows + normative blockquote + §11.16 | `empyrean/contract/protocol.md` (rows at `:1311-1313`; ledger `:2547`) | **Required — first.** |
| 1b | Schema fragment, then re-vendor | `empyrean/contract/schema/bus-protocol.schema.json` → `crates/oracle-aether/tests/contract/bus-protocol.schema.json` (+ `PROVENANCE.md:137-150` procedure) | **Required.** `UNCOVERED_METHODS` stays empty. |
| 2 | Aether handlers + `METHODS` rows + `"profiler": true` | `crates/oracle-aether/src/engine.rs` (`METHODS` `:155`, capabilities `:834`, arming pattern `:644`) | **Required.** |
| 2b | Wire conformance tests | `crates/oracle-aether/tests/profiler.rs` (new), pattern from `tests/scanlines.rs` | **Required** — ships with the handler. |
| 3 | MCP tool rows | `oracle/linux-port/mcp/oracle_mcp.py` — the three tuples already exist at `:704`, `:719`, `:726` | **Verify, don't add.** They point at the legacy server; confirm the shapes still match after CR-26 and amend the descriptions (they currently claim V-INT/H-INT timing that the legacy server cannot deliver). |
| 4 | Player GUI lens | `crates/oracle-frontend/src/lens/` + `commands.rs` | **BUILD IT — a decision, not an omission.** See below. |

**On surface 4, explicitly (D15).** CR-24 recorded "no GUI surface" as a *decision*; here the
opposite is the right one. A "hot routines" panel is the single most useful thing a player-embedded
profiler can show, the tree already has the exact scaffolding, and the marginal cost is small:

- Add `LensId::Profile` to the enum (`crates/oracle-frontend/src/lens/mod.rs:32-40`) and `ALL`
  (`:42-50`); `key()`/`title()`/`label()` arms follow.
- The command palette row is **generated** from `LensId::ALL` (`commands.rs:213-217`), and
  `every_lens_registers_a_visible_command` (`commands.rs:406-420`) makes forgetting it a test
  failure — so surface 4 partially enforces itself.
- **One real constraint, and it is the reason to name this now rather than later.** `LensSet` is a
  `u8` with `const _: () = assert!(LensId::ALL.len() <= 8)` (`lens/mod.rs:94-98`), and the comment
  reads: *"Six are here and the audio meters are gated rather than cancelled, so the seventh is
  already spoken for."* A profiler lens is **the eighth and last free bit**. Adding it is fine;
  adding it *without saying so* leaves the next lens author to discover the widening. Say it in the
  CR.
- **W26 is avoided by construction**: the lens is a *renderer over the same profiler model* the bus
  serialises, following the `models`/`draw` split (`lens/mod.rs:1-14`). The reference reimplemented
  its stack reconstruction for its flame chart and the two have already diverged; there must be
  exactly one accumulator here.
- **W23/W24 are avoided**: the lens toggle is display only. It never arms or disarms the instrument,
  and the bus's `enabled` is the single source of truth for whether recording is happening.

---

## H. Currency inventory

**Expected movement: zero.** Not by inspection — by construction, and here is the enumeration of
everything that *could* move, with why it does not.

| Candidate | Moves? | Why |
|---|---|---|
| `state_hash` (VRAM/CRAM/VSRAM/REGS) | No | The profiler writes nothing to the VDP. `crates/oracle-core/src/state_hash.rs` hashes VDP regions only. |
| `export_state` / `export_state_hash` | No | The profiler is caller-owned; `System` never stores it (`system.rs:909-910`), so it is not in the exported layout. |
| `memory_hash` | No | No memory is written. |
| Golden frames / scanline goldens | No | No render path is touched; `wants_scanlines()` stays `false`. |
| Determinism gate (120 frames × `export_state_hash`, two instances) | No | Nothing added is nondeterministic: no wall clock, no `HashMap` in accumulated state (`BTreeMap`, per `scheduler.rs:4-7`'s own rule), no RNG. |
| SingleStepTests | No | No CPU behaviour changes. The hook is called *after* `step_cpu` returns. |
| Instruction stream / bus traffic / clock | No | The hook reads `pc`, `prefetch[0]`, `a[7]`, `cycles` — all already computed. It writes nothing back. |
| VDP write-capture arming | No | `wants_vdp_writes()` stays `false`; the currency-sensitive capture path is untouched (`system.rs:994-1006`). |
| `initialize` capabilities | **Yes, deliberately** | `"profiler": false → true` (`engine.rs:834`). Additive per D5, and the key is already schema-typed (`bus-protocol.schema.json:200`). |
| `UNCOVERED_METHODS` | **Must stay `[]`** | `schema_conformance.rs:142`. The fragment lands with the handler or the suite goes red — which is the enforcement, not a hope. |
| `crates/oracle-core/tests/` zero-file-diff record | **Broken, with a stated reason** | The record is noted at `docs/superpowers/plans/2026-08-18-aeon-tier1-bus-methods.md:1175-1183`. A new core instrument needs its own core test file; that is the reason, and it goes in the slice's commit message. |

**The gate that proves neutrality** is already a named shape in this tree —
`crates/oracle-core/tests/scanline_capture.rs:268-300`, `frame_boundary_is_state_neutral`. Four
assertions, in order:

1. `export_state_hash()` equal between an untapped and a tapped run;
2. **the whole `System` struct equal** (`assert_eq!(plain, tapped)`) — *"the WHOLE machine is
   identical, not just the hash"*;
3. **a liveness control** — the sink demonstrably observed something, because otherwise deleting the
   hook call outright leaves the test green;
4. a structural assertion on what was observed.

**The profiler's neutrality test copies all four**, and clause 3 is not optional: without it, a
profiler that silently records nothing passes.

---

## I. The better-approach pass (the owner's directive)

The floor is the legacy surface. Everything here is **additive over it** — a migrating consumer
loses nothing by taking any row of this table.

| # | Candidate | Value to the demand side | Cost | Additive? | Verdict |
|---|---|---|---|---|---|
| 1 | **Exact per-invocation attribution** (no sampling error) | Their gate uses `==`; exactness is the entry ticket, not a bonus | ~free given the retire hook — it *is* the mechanism | yes | **In v1** |
| 2 | **Self / exclusive cycles beside inclusive** | Tasks 2 and 4 fit models from per-routine rows; an inclusive-only slope is a slope over the whole call tree, and they do not currently know that (demand doc §4) | one accumulator | yes — `cycles` keeps its floor meaning, `cyclesSelf` is new | **In v1** |
| 3 | **Cause-keyed HInt / VInt buckets** | Task 3 (budget axis 4b) becomes a direct read; the pinned rule | zero core change (§C.2) | yes — buckets exist today, they are just wrong | **In v1** |
| 4 | **Symbol-resolved `name`** | Deletes `parse_lst` + the `SYMS` tuple from their probe; makes Task 2's five *named* routines a lookup instead of a hop | one `Engine::symbol_at` call per row | yes | **In v1** |
| 5 | **Canonical 24-bit `addr`** | Deletes their `lstrip`/`int`/mask reconcile (`raster_cost_probe.py:549-561`) | one mask | yes | **In v1** |
| 6 | **Stall cycles included in the base** | The reference's numbers are stall-free (W6); theirs is the only instrument they have | free — it is our clock | **no — this is the one non-additive change**, and it is a *correction* | **In v1, reported loudly** (§7.4) |
| 7 | **`stallCycles` as its own per-row field** | Separates "this routine is slow" from "this routine waited for the VDP"; makes #6 auditable rather than merely present | needs the bus's `wait` returns threaded to the retire hook (`read16` already returns `(value, wait)`, `bus.rs:1052`) — a real plumbing slice | yes | **Slice 6, named** |
| 8 | **`maxContiguousStallCycles`** — Task 5's row, directly | Task 5 is *"Measure the longest contiguous DMA stall in a frame"* (`plan:219`) and **no profiler, old or new, provides it**; the VDP already computes each DMA's cost (`bus.rs:1047`, `dma_complete(record, …, cost)`) | small once #7 exists | yes | **Slice 6, named** |
| 9 | **True per-frame rows** (`perFrame[]`) — make the method's name honest | Kills W29/W30; lets them see variance *inside* a sample instead of inferring it from whole-boot repeats; would have made their camera-stale baseline failure visible | a bounded ring of per-frame totals; §2.4(a) applies again | yes | **Candidate — ruling wanted** |
| 10 | **Per-scanline cost attribution** — which lines a routine burns in | Pairs with the sub-line arc; for a raster engine this is arguably the *most* interesting axis, and Aeon's whole P2 is line-budget work | the retire hook would need mclk (or the profiler latches it from `on_event_at`); line = `mclk % MCLK_PER_FRAME / MCLK_PER_LINE` | yes | **Candidate — high value, own CR** |
| 11 | **Event tap for a flame view** | Nothing in the current demand asks for it | a re-introduction of the reference's ring, with its overflow class | yes | **Declined for now** — named so the decline is visible |
| 12 | **Player GUI lens** | Not Aeon's ask; it is D15 parity and the reason the instrument gets used by a human at all | small (§G.6), but spends the last `LensSet` bit | yes | **In v1 as a decision** |

**On #6, the one non-additive item.** Including stall cycles is not a feature we may switch off to
preserve parity — it is what our clock *is*, and it is more correct. The right handling is not to
suppress it but to **make it visible and quantified** (#7), so an old-vs-new delta on a
DMA-heavy routine has a named mechanism attached instead of being reported as noise.

---

## J. Slice plan

Dispatch-sized, contract-first, every intermediate green. Tests-first shapes with their mutation
requirements are named per slice: **a gate must be proven red-first, wired into a runner, derive its
expectation from source rather than from a measurement, and be loud on unmeasurable.**

### Slice 0 — CR-26 (docs only, in `empyrean`)

Draft `§11.16` + the three amended §6 rows + the schema fragments. Contents: the shapes of §G.2, the
`perFrameExact` typed key, the §2.4 bounded-list conformance for `routines[]`, the `budgetPct`
ruling (§G.5), the arm/disarm normative text (§F.3), and the interrupt-bucket definition keyed by
cause with `calls` = *taken*, not *raised* (§E). **No handler work until the ruling lands.**

Green criterion: `empyrean` docs only; oracle-next untouched.

### Slice 1 — the retire hook, empty (the neutrality slice)

`StepRetire` + `on_step_retire` defaulted on `BusEventSink`, the four forwarding impls
(`bus.rs:220`, `:253`, `:312`, `:374`), one call site at `system.rs:1036`. **No profiler yet.**

Tests, first: the four-assertion neutrality shape (§H) with a `RetireLog` sink — including clause 3,
the liveness control. **Mutation requirement:** deleting the `sink.on_step_retire(...)` line must
turn the test red; if it does not, the control is vacuous and the slice is not done.

Green criterion: full workspace suite at the current baseline, `export_state_hash` unmoved,
determinism gate green.

### Slice 2 — `control_flow_of` and its whole-domain pin

The pure function in `decode.rs` (§D.2) plus the 65 536-opcode agreement test against what
`decode::decode` actually selects. **Expectation derived from the dispatch, never from a table
written by hand.**

### Slice 3 — the accumulator

`crates/oracle-core/src/profiler.rs`: the shadow stack, the rows, the interrupt buckets fed from the
fc = 7 event (§C.2), the frame counter fed from `on_frame_boundary`.

Tests, first, on `testrom`-built fixtures (`crates/oracle-core/src/testrom.rs`) so expectations are
derived from the fixture's own construction:

- a routine called *k* times reports `calls == k` — and *k* is a constant the fixture builder used;
- self + children == inclusive, for a two-level fixture;
- a recursive fixture reports one row with the right `calls` and no cycle total exceeding the run;
- a `move.l addr,-(sp)/rts` dispatch fixture does **not** close the caller (the W3 regression);
- a call straddling a frame boundary is **one** call (the W4 regression);
- an HInt-only fixture puts zero cycles in `vint`, and a VInt-only fixture zero in `hint` — **the W8
  regression, and the single most important test in the arc**;
- two identical runs produce byte-identical profiler output (pattern:
  `crates/oracle-core/tests/watchpoints.rs:234-252`).

> ⚠ Note the testrom builder's unguarded branch-displacement truncation, **F-TESTROM-DISP-GUARD**
> (`docs/2026-08-19-scanline-readback.md:241-246`): a fixture whose loop body passes ±127 bytes
> assembles a *different valid branch* rather than failing. A profiler fixture with a real call tree
> is exactly the shape that trips it. Either land that guard first or keep the fixtures short and say
> which.

### Slice 4 — the bus surface

`METHODS` rows, handlers, `"profiler": true`, the `Option`+`Fanout`+`Observe` arming
(`engine.rs:644` pattern), `crates/oracle-aether/tests/profiler.rs` as wire round trips validated
against the vendored schema. `UNCOVERED_METHODS` stays `[]`.

Include the wire-level determinism gate (`tests/scanlines.rs::a1_determinism_three_boots_byte_identical`
is the pattern) — **three boots, byte-identical**, which is Aeon's own noise bar expressed as our
suite's gate.

### Slice 5 — MCP verification + the player lens

Surface 3 (verify/amend the three existing tuples) and surface 4 (`LensId::Profile`, §G.6, with the
last-free-bit note in the commit message).

### Slice 6 — stall accounting

`stallCycles` per row (#7) and `maxContiguousStallCycles` (#8) — Task 5's row. Separate because it
threads the bus's `wait` returns, which is real plumbing and deserves its own currency argument.

### Slice 7 — the corpus A/B (the acceptance artifact)

Not code. See §7.4 below; it produces a committed evidence document.

---

## K. Acceptance protocol

### K.1 The primary shape: A/B against Aeon's Phase-0 corpus

Their Phase-0 measurement session is running now on the reference and will land: engine baselines at
two pinned camera states, the per-frame HInt total on shipped content, the fitted walker model with
its residual, and the DMA-stall awareness row — **five boots per state, spread reported**
(`plan:127`). Merge SHA **PENDING**; demand doc §8 holds the placeholder.

This is stronger than any fixture-only check because it is *the same ROM, at states documented to be
reproducible, measured on the instrument we are replacing.*

**Comparability, row by row:**

| Row class | 1:1 comparable? | Expected relationship |
|---|---|---|
| Per-routine `cycles` at the pinned camera states | **Yes**, against our **inclusive** figure — that is what theirs is (W1) | Equal **only where the routine does no stalling work**; a DMA-touching routine must read **higher** on ours (#6/W6) |
| Per-routine `cyclesSelf` | No counterpart | New information; report, do not compare |
| Per-routine `calls` | **Yes**, and this is the sharpest check | **Must match exactly** — no stall-clock term, no truncation term. A `calls` disagreement is a real mechanism difference (W2/W3/W4 are the candidate causes and each is identifiable) |
| The eight F0–F8 fixtures | **Yes** | The clean A/B: a synthetic, DMA-free raster fixture is where our clock and theirs *should* agree, so a delta here has no stall excuse |
| `interrupts.hint` | **No — must legitimately disagree** | Theirs is HInt + VInt summed (W8). **The prediction is arithmetic:** their `interrupts.hint` ≈ our `hint.cycles + vint.cycles`. That is not a tolerance — it is a *falsifiable equation*, and checking it is the best available proof that our split is the correct one |
| `interrupts.vint` | **No** | Theirs is structurally 0. Ours will not be. A non-zero `vint` on our side against their 0 is the expected result, not a discrepancy |
| Task 5's DMA stall | **No counterpart on their side** at all | Their instrument's clock excludes the thing the row is about (§A.3). Ours measures it. Report as new. |

### K.2 On spreads, and why a spread is not a tolerance

Their five-boots-per-state spread is **the old instrument's noise floor**, not an error bar we are
permitted to hide inside. Our attribution is exact and our machine is deterministic, so:

- **our spread must be exactly 0**, across boots — anything else is a bug in us, and it is a *gate*,
  not an observation (Slice 4's three-boot wire test);
- **any old-vs-new delta needs a named mechanism**, not a tolerance. The mechanisms available are
  enumerated and each is distinguishable: stall inclusion (#6, moves DMA-touching routines only, one
  direction), inclusive-vs-self confusion (would show as a *large* delta on parents only),
  W2/W3/W4 attribution holes (show as `calls` disagreements first), and truncation (bounded by
  `frames − 1`, one-sided low, and only when `perFrameExact` is false).

If a delta has no mechanism, the A/B has found something and the arc stops — it does not widen a bound.

### K.3 The measured claim the owner's directive asks for

Their framing matches ours: **if exact per-invocation attribution beats the old instrument's floor,
this corpus is where that stops being a design argument and becomes a measured one.** That
measurement is an explicit acceptance artifact of Slice 7:

**`docs/2026-08-2X-profiler-corpus-ab.md`** — for each comparable row: their figure (with their
five-boot spread), ours (with our spread, expected 0), the delta, and its mechanism. Plus:

- the `interrupts.hint ≈ hint + vint` equation, evaluated;
- the `calls` exactness table, which is pass/fail with no tolerance column at all;
- the F0–F8 fixtures re-run through our surface, compared to the reference's 8 rows
  (`plan:67-77`) — the DMA-free control;
- the migration deltas Aeon must know about *before* they read anything: the `-1` deletion (§F.3),
  the stall inclusion (#6), and the inclusive-vs-self question on Tasks 2/4 (demand doc §4).

### K.4 Migration acceptance, on their side

`raster_cost_probe.py` migrates by changing one constant (its `HARNESS` path,
`aeon/tools/raster_cost_probe.py:55`) and deleting three things: the two sleeps (`:531`, `:533`),
the `- 1` (`:535`), and the address-reconcile in `hint_row` (`:559-561`, replaced by matching
`name == "HBlank_Vector_Slot"`). Acceptance:

- **Fixture exactness:** `effects_gates.py`'s gate 5 passes with `==` against constants derived from
  `raster_dsl.emp` — the same assertion, on our instrument (`effects_gates.py:754-756`, `:773`).
- **`calls` exactness:** the derived dense-fixture count `lines + 5`
  (`raster_cost_probe.py:362-364`, `effects_gates.py:772`) matches, unchanged.
- **Noise:** `--repeat 3` reports spread 0, matching their existing bar (`plan:56`).
- **Their wedge goes away:** the segmented-runner architecture
  (`effects_gates.py:52-70`, `:185-230`, `:294-317`) exists for a hang in the reference's host. It
  is not this arc's deliverable to delete, but it is the arc's *purpose*, and it should be stated as
  the success condition the owner actually cares about.

---

## L. Open questions needing a controller ruling, ranked

1. **`get_profiler_frames` name-vs-shape (W29): do we add real `perFrame[]` rows?** (#9). It makes
   the catalogued name honest and gives the demand side within-sample variance they have never had —
   at the cost of a second bounded list in one result. **Recommendation: yes, in CR-26, bounded and
   off by default (`perFrame` requested explicitly).** Ranked first because it changes the CR's
   shape, not just its content.
2. **`budgetPct`: derive it (§G.5b) or drop it (§G.5a)?** Recommendation: derive. Ranked second
   because CR-26 cannot be written without an answer.
3. **Inclusive vs self as the *primary* `cycles` key.** The floor's `cycles` is inclusive, so
   keeping that meaning is the drop-in choice — but Tasks 2 and 4 arguably want self. Recommendation:
   `cycles` stays inclusive (floor-compatible), `cyclesSelf` is additive, and **the demand side is
   asked which their fit wants before the corpus A/B is read** (demand doc §4).
4. **Per-scanline attribution (#10): this arc, a follow-on CR, or declined?** High value for a
   line-budget consumer and it pairs with the sub-line instrumentation that shipped today.
   Recommendation: **own CR after v1** — folding it in would double this arc.
5. **`F-SYMBOLNAME-DISP`** (§G.3): the `symbol_at` `+$hex` suffix versus `$defs/symbolName`'s
   pattern. Needs a foreground test run to confirm before it is called a bug. This arc should
   *consume* the ruling, not make one.
6. **Does any corpus ROM nest HInt inside VInt?** (§C.3). Runtime question; no emulator calls from a
   background agent. Affects only whether a nesting fixture joins the test set.
7. **Breaking the `crates/oracle-core/tests/` zero-file-diff record** (§H). A new core instrument
   needs a core test file. Recommendation: accept, with the reason in the commit message. Ranked low
   because the answer seems clear, but the record was deliberate and should not lapse silently.
8. **The eighth `LensSet` bit** (§G.6). Spending the last free bit on the profiler lens is correct,
   but it should be a decision, and the next lens author should inherit a widened `LensSet` or a
   written note. Recommendation: spend it, and widen `LensSet` to `u16` in the same slice so the
   constraint stops being a trap.

---

## M. Verification note

**Nothing in this arc was measured on a running machine.** No emulator MCP tool was called (standing
invariant), no build or test run was performed, and no file under `crates/` was modified. Every
claim about either codebase is a static read with a `file:line` anchor; every claim about the
consumer is transcribed in the companion demand doc. The two items that want a foreground runtime
check are flagged `⚑` (§C.3, §G.3) and neither is load-bearing for the design's shape.

The suite baseline is therefore unchanged and unmeasured by this arc: **40 legs / 1606 passed /
0 failed / 4 ignored**, as recorded before it began. Slice 1 is the first change that can move it,
and its acceptance criterion is that it does not.
