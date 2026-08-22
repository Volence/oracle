# The acceptance 21 — a priced, ordered survey

**Date:** 2026-08-22 · **Branch:** `acceptance-survey` · **Base:** `0fa34f1`

**What this is, plainly.** Our Rust Aether server serves 37 of the contract's 58 method fragments. The
other 21 describe methods only the legacy C++ server (`oracle-old/`) serves today — which is what
`mcp__oracle__*` reaches. That is not a defect: Aether exists to be the stable contract while the core is
swapped underneath, so a fragment describing a method the successor has not built is the transition
working as designed. What it means is that **those 21 are oracle's acceptance contract** — the definite
list of what the successor must serve before it can replace the legacy server.

This document re-derives that list from source, prices each of the 21 against what `oracle-core` can
already do, and proposes an order to build them in.

**Everything here is derived from source at `0fa34f1`.** No number is carried forward from
`docs/OVERSEER.md` item 7 or from `docs/2026-08-22-peer-schema-defect-answers.md`. Where a number in
those documents disagrees with what I measured, §8 says so.

---

## 1. The re-derivation

### 1.1 The fragment set

`crates/oracle-aether/tests/contract/bus-protocol.schema.json` hangs its method fragments off the
top-level `methods` object directly. Parsed:

| | count |
|---|---|
| keys under `methods` | 59 |
| `$`-prefixed keys (`$comment`) | 1 |
| **method fragments** | **58** |

**The vendored copy is byte-identical to the contract.** `sha256` of the vendored file and of
`git -C ../empyrean show origin/main:contract/schema/bus-protocol.schema.json` both come back
`8cc08be1b73b909341c6a3eef94b347a521966608c9e71cedd6decc5f6c7529d`, at empyrean `origin/main` =
`7df15a8`. So the fragments read below are the authority, not a stale local copy.

*(Note for the next reader: the spec is `contract/protocol.md` in empyrean, **not** `docs/protocol.md`.
See §8.)*

### 1.2 Derivation A — the naive string-literal grep

The prior derivation was "intersect the fragment key set against the `"emulator/*"` string literals in
`engine.rs`". Reproduced:

```
grep -o '"emulator/[a-z0-9_]*"' crates/oracle-aether/src/engine.rs | sort -u   →  39 literals
```

39 literals, of which two are events, not methods: `emulator/stopped` and `emulator/resumed`.
**37 method literals.** 58 − 37 = **21 unserved**.

⚠ **This derivation is accidentally correct, and the brief that described it was wrong about why.**
`engine.rs` carries **three** events, not two — `EVENTS` at `crates/oracle-aether/src/engine.rs:428`
is `["emulator/stopped", "emulator/resumed", "emulator/romReloaded"]`. The character class
`[a-z0-9_]` silently excludes `romReloaded` because of its capital `R`. Widen the class to
`[A-Za-z0-9_]` and the same grep returns **40**, and the naive subtraction gives 20 unserved — off by
one, in the direction that hides a method. The contamination the brief warned about is real and is
one item larger than the brief said.

### 1.3 Derivation B — the dispatch table, parsed structurally

Dispatch is table-driven and there is exactly one dispatch path
(`Engine::dispatch`, `engine.rs:984`): it looks the method up in `METHODS` and refuses anything absent
with `-32601`. `METHODS` (`engine.rs:200`) is therefore the served set *by construction* — the file's
own doc comment makes that claim, and the structure bears it out.

Parsing `name:` fields out of the `METHODS` table body:

```
METHODS entries: 37   (37 unique)
```

This derivation cannot be contaminated by event names at all: `EVENTS` is a separate `const` and its
strings never appear in a `MethodSpec`.

### 1.4 Reconciliation

| check | result |
|---|---|
| literals − served | `emulator/resumed`, `emulator/stopped` — exactly the two events, as expected |
| served − literals | **∅** (every table name is also a literal, trivially) |
| served − fragments | **∅** — every method we serve has a fragment; there is no unschematized served method |
| fragments − served | **21** |

**The two derivations agree exactly.** No disagreement to report on the membership itself; the only
correction is the third-event trap in §1.2's method.

### 1.5 A third, corroborating source (not independent — read on)

`crates/oracle-aether/tests/schema_conformance.rs:388-409` holds a **pinned literal set**
`SCHEMATIZED_NOT_ADVERTISED` of exactly these 21 names. It matches my derivation name for name.

I am reporting this as *corroboration*, not as a third independent derivation, and the distinction
matters: that constant is a hand-written list, and the test's job is to compare it against a parse of
the schema and the `METHODS` table. It cannot disagree with my derivation without the suite going red.
It is the gate that keeps the derivation honest, not evidence for it.

**Its practical consequence is a cost line on every parcel below:** the test fails when a method
*leaves* the set, so every commit that ships one of the 21 must delete its name from that array in the
same commit. That is by design.

### 1.6 The list

```
emulator/audio_spectrum        emulator/set_channel_enabled
emulator/breakpoint_add        emulator/set_layer_enabled
emulator/breakpoint_clear      emulator/step
emulator/breakpoint_list       emulator/step_out
emulator/get_channel_states    emulator/step_over
emulator/get_layer_states      emulator/vgm_start
emulator/log_clear             emulator/vgm_status
emulator/ping                  emulator/vgm_stop
emulator/run_to_scanline       emulator/wait_for_break
emulator/write_vram            emulator/z80_read
                               emulator/z80_write
```

Set-differenced against the list in the brief: **derived − brief = ∅, brief − derived = ∅.** The brief's
list was right.

---

## 2. Three facts that price every parcel

Before the per-method detail, three findings that apply across the whole set. Each was measured here and
each changes what "serve this fragment" costs.

### 2.1 No fragment declares any error condition — all 58 of them

The brief asked me to report each fragment's "declared error conditions". The honest answer is uniform:

```
fragments with an `errors` key: []
fragment top-level keys, across all 58: ['$comment', 'params', 'result']
```

Every error obligation in this document therefore comes from **prose** — `protocol.md` §5/§6 and the
fragments' own `$comment` text — never from a machine-checkable declaration. In particular the §6
run-control state rule (`-32005` with `data.reason: "machineRunning"`) binds `step`, `step_over`,
`step_out` and `run_to_scanline` through prose alone.

Consequence for pricing: the state gate is not free-by-schema, but it *is* free in code —
`Engine::require_paused` already exists (`engine.rs:1383`) and its own doc comment at `:1381` already
names `run_to_scanline` and `step*` as its intended callers. One line per method.

### 2.2 Our harness validates replies, never requests

`crates/oracle-aether/tests/common/schema.rs` validates: the envelope on every line, method **results**,
and **event** params. It does not validate request params against a fragment's `params` subschema.
The only request-side check anywhere is `Engine::dispatch`'s flat unknown-key closure (`engine.rs:999`)
plus `tests/params_closure.rs`, which asserts the `METHODS` row's key **set** equals the fragment's
key set — by parse, which is the right shape — but says nothing about types, bounds, `oneOf`
alternations or enums.

**So every `oneOf`, every `minimum`/`maximum`, and every `enum` on the params side of these 21
fragments is unguarded by the existing suite.** Each parcel must bring its own request-side tests;
none of them inherit coverage. This is the single largest hidden line item in the whole survey.

⚠ **And it has already bitten an in-flight method.** `emulator/run_to`'s fragment carries a
`oneOf: [{required:[addr]}, {required:[symbol]}]`, but `Engine::resolve_target` (`engine.rs:1278`)
checks `symbol` first and returns; if a client sends **both**, `addr` is silently ignored and the call
succeeds. The fragment refuses that request. This is a live, **unregistered** request-side divergence on
a method we ship today — `KNOWN_CONTRACT_DIVERGENCES` (`tests/common/schema.rs:276`) is currently empty.
I am recording it, not fixing it (this task is a survey). It matters here because
`breakpoint_add` and `breakpoint_clear` both want exactly `resolve_target`, and would inherit the
same gap.

### 2.3 The four known camelCase param conflicts are *all* inside these 21

Audit D-33 measured four genuine param-name conflicts between the legacy server and the contract:
`fftSize`/`fft_size`, `maxHz`/`max_hz`, `timeoutMs`/`timeout_ms`, `maxFrames`/`max_frames`.

All four land on methods in this set: `audio_spectrum` (two of them), `wait_for_break`, and
`run_to_scanline`. **Not one of the 37 methods we already serve carries a conflict.**

That is a genuinely good result and it should be stated as such: the migration burden is not spread
across the surface, it is concentrated entirely in work that has not been written yet, so it can be
absorbed by building the method right the first time rather than by a migration. It also means the
`wait_for_break` obligation to aeon (§6) is not an isolated courtesy — it is the *pattern* for three
other methods, and `run_to_scanline`'s `maxFrames` deserves the same heads-up treatment.

---

## 3. Per-method table

**Legend for "core":** **ready** = the primitive exists and is reachable from the `&mut System` the
engine holds. **partial** = exists but needs plumbing or extension. **absent** = the capability does not
exist in the tree.

**Legend for "CR?":** whether serving the fragment *as written* is blocked on a contract change.
**no** = pure conformance work, start today. **advisory** = conformant as written, but the written shape
is wrong for a consumer and a CR should be raised alongside. **yes** = cannot be served conformantly
without the contract moving first.

| method | core | server work | contract | CR? | consumers |
|---|---|---|---|---|---|
| `step` | **ready** | new counting sink + handler | D-02 `count` unbounded/no default | advisory | manual (2 aeon probes) |
| `step_over` | **ready** | new depth sink + handler | D-03 no result keys | advisory | none |
| `step_out` | **ready** | new depth sink + handler | D-03 no result keys | advisory | none |
| `run_to_scanline` | **partial** | new sink or core accessor | D-04 no `pc`; **262 vs 511** | advisory | manual (1 aeon probe) |
| `write_vram` | **partial** | handler + `Vdp::poke_vram` | D-16 ungated, unbounded | advisory | manual (1 aeon probe) |
| `z80_read` | **ready** | handler; flip `z80` cap | D-09 no `len` default | **no** | none |
| `z80_write` | **partial** | handler + `z80_ram_mut` | D-10 `value` has no width → `len` undefined | **yes** (`value` half) | none |
| `breakpoint_add` | **ready** (mechanism) | surface unbuilt | D-12, **D-13** | **yes** | **automated (aeon nightly)** |
| `breakpoint_clear` | **ready** (mechanism) | surface unbuilt | D-15, **D-13** | **yes** | **automated (aeon nightly)** |
| `breakpoint_list` | **ready** (mechanism) | surface unbuilt | D-14, **D-13** | **yes** | none |
| `wait_for_break` | **ready** (machine); **absent** (blocking transport) | architectural | D-05..D-08 | advisory | **automated (aeon nightly)** |
| `vgm_start` | **partial** | engine-owned logger + file I/O | D-18 | advisory | none (prose only) |
| `vgm_stop` | **partial** | as above | D-18 | advisory | none |
| `vgm_status` | **partial** | `active`/`samples`/`bytes` derivation | D-19 no `path` | advisory | none |
| `get_channel_states` | **absent** | mask layer in 2 chips + engine sink | D-17 no vocabulary | advisory | none |
| `set_channel_enabled` | **absent** | as above | D-17 no vocabulary | advisory | none |
| `audio_spectrum` | **partial** (FFT **absent**) | FFT + per-source buffers + ring | D-22 (3 gaps) | advisory | none |
| `get_layer_states` | **absent** | mask through fused renderer | D-17 sibling | advisory | none |
| `set_layer_enabled` | **absent** | as above | D-17 no vocabulary | advisory | none |
| `ping` | n/a | trivial handler | D-01 value undefined | **yes** (or pick + declare) | 1 manual, `hasMethod`-guarded |
| `log_clear` | **absent** (no log exists) | a log, first | D-23, coupled to D-29 | advisory | none |

**Reading the "CR?" column.** *no* = pure conformance work, start today. *advisory* = conformant as
written, but the written shape is wrong for a consumer and a change request should be raised alongside
the implementation, never instead of it. **yes** = cannot be served conformantly, or cannot be designed
unilaterally, until the contract moves.

Aggregate: **3 methods have an automated consumer, 4 more are manual-only, and 14 of 21 have no
consumer of any kind.**

---

## 4. Per-method detail

### 4.1 Stepping — `step`, `step_over`, `step_out`

**(a) What the fragments require.**

`emulator/step`: params `{count?: integer, minimum 0}`, `unevaluatedProperties: false`. Result requires
`pc` (`$defs/hex`); optional `symbol` (bare label, and *"a server MUST NOT fall back to the address
string"*), `symbolDisp` (integer ≥ 0). `caveat` **declared absent**. Named in §6's run-control state rule
via `step*`: requires a PAUSED machine, `-32005` `data.reason: "machineRunning"` otherwise. Emits
`emulator/stopped` with `reason: "step"`.

The fragment says of `count`, in its own words:

> `count` carries no lower bound above 0, no upper bound and no default, because §6's row states none —
> every other bounded count in this catalog spells its bound out […] and inventing one here would make
> the schema, which D14 says governs the wire, the author of a constraint the contract never agreed.
> Registered as a defect (audit D-02).

`emulator/step_over` and `emulator/step_out`: **no params, no result keys at all** — `result` is a bare
`$ref` to `replyFields`. Both fragments say this is the row, not an omission, and register the asymmetry
with `step` as audit D-03. Their `stopped` `reason` is also `"step"`, not a value of their own.

**(b) What the core can do — ready, and by a route the brief did not name.**

`crates/oracle-core/src/system.rs:828` `step_instruction` exists, as the brief said. **It is not the
primitive for this bus.** Its own doc comment: *"it does **not** advance the master clock (the caller
owns time)"*. Its only callers in the tree are `crates/oracle-core/tests/step_retire.rs:112` and
`crates/oracle-core/examples/differential_trace.rs:40` — nothing in any `src/`. Building `emulator/step`
on it would step the 68000 while the VDP, Z80, FM and the scheduler stood still, and the reply's own
machine stamp would not move.

The real primitive is the sink-generic run loop, `System::run_until_with_sink`
(`crates/oracle-core/src/system.rs:991`). Its body at `system.rs:1027-1048` does, per iteration:

```rust
sink.on_step_boundary(step_pc, self.scheduler.now() / MCLK_PER_FRAME);
if sink.stop_requested() { reason = StopReason::SinkRequested; break; }
let (outcome, stall_cycles) = self.step_cpu_stalled(sink);
sink.on_step_retire(StepRetire { pc, opcode, sp, ssp, supervisor, cycles, stall_cycles, executed });
```

So a sink that counts `on_step_boundary` calls and raises `stop_requested` after N gives `step {count:N}`
directly, with the machine left at an instruction boundary and the whole rest of the system advanced
correctly. `crates/oracle-core/src/bus.rs:305-318` ratifies the semantics explicitly.

For `step_over` / `step_out` the classification primitive is also already there and already proven:
`control_flow_of(opcode) -> ControlFlow{Call, Return, InterruptReturn, Jump, None}` at
`crates/oracle-core/src/m68000/decode.rs:223`, `pub`, and pinned against the real dispatch over **all
65 536 opcodes** by `control_flow_of_agrees_with_the_dispatch_over_every_opcode`
(`decode.rs:15458`). `StepRetire` carries `sp` (mode-selected A7), `ssp`, `supervisor` and `executed` —
i.e. everything needed to match a return to its call without closing the wrong frame on a numeric
coincidence.

And there is a **working reference implementation of exactly that shadow stack in the tree already**:
`impl BusEventSink for Profiler`, `crates/oracle-core/src/profiler.rs:892-946`, which opens a frame on
`ControlFlow::Call`, closes on `Return`/`InterruptReturn`, and guards on `r.executed` because *"on every
one of those paths the opcode names an instruction the CPU never executed, and classifying it would arm a
call that was never made and will never return."* `Profiler::open_frames()` (`profiler.rs:533`) is the
depth reader `step_out` wants.

Server side, every other piece exists: `Engine::require_paused` (`engine.rs:1383`, whose own doc at
`:1381` already names `step*`), `Engine::emit_stopped(reason, pc, extra)` (`engine.rs:1121`),
`Engine::symbol_at` (`engine.rs:1214`), and `Engine::advance_until` (`engine.rs:908`) as the `Fanout`
wiring template. `Engine::run_to` (`engine.rs:1467-1530`) is a near-complete shape to copy.

Classification: **ready.** No core change.

**(c) Cost and risk.** Server-only. Three handlers, one or two new sink types, three `METHODS` rows,
three deletions from `SCHEMATIZED_NOT_ADVERTISED`, and request-side tests (which, per §2.2, nothing
existing provides). Judgement estimate: **~300-400 lines across `engine.rs` plus a sink module, plus
tests.** No contract change is required to serve any of the three as written.

⚠ **But the written shape of `step` is wrong for a consumer, and the reason is measurable here.** Our
only advance primitive is frame-bounded (`Engine::advance_until` → `run_frames_with_sink(max_frames, …)`;
`EngineConfig::max_run_frames` defaults to 3600, `engine.rs:140`). `count` has no ceiling. So
`step {count: 10_000_000}` will exhaust the frame budget, stop early, and return `pc` — and the fragment
gives the reply **no key to say fewer than `count` instructions ran**: the result requires only `pc`, and
`caveat` is declared *absent*, so emitting one fails §8 item 20's closure. That is a silent weak answer
with no machine-readable discriminant, which is precisely the shape §2.4 exists to prevent.

**Change request to raise alongside (do not deviate unilaterally):** give `step` either a `stepped`
count in the result, or a ceiling on `count`, or permission to carry `caveat`. Any one closes it; the
current row closes none. This is D-02 with a consumer-facing consequence attached.

D-03 (`step_over`/`step_out` return nothing) is real but much milder: the answer arrives on
`emulator/stopped`, which we already emit and which carries `pc`. A client with the `events` capability
loses nothing. Worth reporting; not worth blocking on.

---

### 4.2 `run_to_scanline`

**(a) What the fragment requires.** Params: `line` **required**, integer 0-511; `maxFrames?` integer ≥ 1,
default 600. Result requires `line`, `reached`, `maxFrames`; `caveat` is **declared present** — one of
only two rows in the whole catalog that keep it explicitly (`run_to` is the other), because D12 gives it
SHOULD force: a `reached: false` reply SHOULD say in words that nothing about the machine's state follows
from where it stopped. Named in §6's run-control state rule: PAUSED required, `-32005 machineRunning`
otherwise. D-04 registers that the result carries no `pc` while `run_to`'s does.

**(b) What the core can do — partial.**

The current line is **derivable in one expression** and nothing more:
`(sys.scheduler().now() % MCLK_PER_FRAME) / MCLK_PER_LINE`. All three constants are `pub`
(`vdp.rs:17,19,21`) and `Scheduler::now()` is `pub` (`scheduler.rs:42`); the engine already does this
arithmetic at `engine.rs:1394`. There is **no named accessor** — `Vdp::current_line()` does not exist
(grep exit 1). `Vdp::v_counter` (`vdp.rs:435`) is `pub` but is the *remapped hardware* counter that jumps
`0xEA→0xE5` at line 235; it is lossy and non-monotonic and cannot be used as a target.

**The existing predicate path provably cannot serve this.** `StopWhen` (`bus.rs:550`) forwards
`on_step_boundary(pc, frame)` to a `FnMut(u32, u64) -> bool`, and the run loop supplies `frame` as
`self.scheduler.now() / MCLK_PER_FRAME` (`system.rs:1041`) — the integer division discards exactly the
intra-frame remainder a line number is. The closure captures no `&System` and cannot: the loop holds
`&mut self`. So `Engine::advance_until` is the wrong tool and a bespoke sink is required.

Of the ten `BusEventSink` hooks, only `on_scanline(line: u16, rgb)` (`bus.rs:254`) delivers a line — and
it has three limits, all measured: it fires only for `line < 224` (`system.rs:1197`), it lags one line
(the deferred emitter flushes the *previous* row, `system.rs:1186`), and it never fires for blanking.

Classification: **partial** — the capability is one expression away but there is no accessor and no hook
that carries it; a new sink plus a `pub` raw-line accessor is the clean shape.

**(c) Cost and risk.** Server-side sink + one small core accessor. Judgement estimate: **~150-200 lines.**

⚠ **A hard finding the brief did not contain: the fragment's range is unreachable above line 261.**
`LINES_PER_FRAME = 262` (`crates/oracle-core/src/vdp.rs:19`, NTSC V28, 224 active + 38 blanking), and
`system.rs:84` statically asserts `TimingBasis::NTSC.lines_per_frame == LINES_PER_FRAME`. The fragment
accepts `line` up to **511**. Lines 262-511 can never occur on this core, so `run_to_scanline {line: 300}`
can only ever answer `reached: false` after burning `maxFrames` frames. Neither the row nor the fragment
says which of "refuse it" and "run the budget and report false" is right.

**A decision is needed, and it is ours to make in the first instance** (`-32602` naming 0-261 as the
reachable range is the house pattern — refuse, never clip — and is strictly more informative than burning
600 frames to report nothing). But it should be reported upward, because the 0-511 span is §6's and was
chosen deliberately to be wider than `emulator/scanlines`' 0-223.

Also carries a param-name conflict: `maxFrames` vs legacy `max_frames` (§2.3).

---

### 4.3 `write_vram`

**(a) What the fragment requires.** Params: `addr` (hex) and `bytes` (hex string,
`^0x([0-9A-Fa-f]{2})+$`) both **required**; `unevaluatedProperties: false`. No `value`+`width` spelling —
byte payload only. Result requires `addr`, `len`. `caveat` **declared absent**. The fragment transcribes
three absences and registers them as D-16: the row is *not* named in §6's run-control state rule though
`write_memory` and `write_cram` both are; no address bound is stated; and there is no numeric payload
spelling.

**(b) What the core can do — partial, and this is where I would have gone wrong.**

`System::vram_mut()` is `pub` (`system.rs:875`), forwarding to `Vdp::vram_mut()` (`vdp.rs:394`). It
looks like `write_vram` is free. It is not.

`Vdp::vram_mut`'s own doc says it is *"used by tests to perturb state; the data-port write path lands in
a later slice"* — a test hatch that `System` re-exported. The real VRAM byte choke is
**`Vdp::write_vram_byte`, `crates/oracle-core/src/vdp.rs:782-802`, which is private**, and it does two
things a raw slice write does not:

1. `self.capture(VdpTarget::Vram, …)` — the watchpoint v2 VRAM choke (`vdp.rs:786`). A debugger poke
   **should** skip this, on `Vdp::poke_cram`'s standing rule (`vdp.rs:1546-1553`): a poke has no `pc` to
   name and no landing clock to supply. Consistent, and an argument for a dedicated poke fn rather than
   for using the raw slice.
2. **The SAT-cache write-through, `vdp.rs:794-801`.** Every VRAM byte landing in the cached half
   (bytes 0-3) of a sprite-attribute-table entry inside the reg-5 window mirrors into `self.sat_cache`.
   **`poke_cram` has no analogue of this, and a `write_vram` built on `vram_mut` would silently break
   it**: poking sprite Y/size/link bytes would update VRAM but leave `sat_cache` stale, and the sprite
   pipeline reads Y/size/link **from the cache only**. The result is a poked sprite table that
   `emulator/read_vram` reads back correctly while `emulator/sprites` and the renderer do not see it. The
   engine already surfaces a `cacheDivergence` flag for this condition (`engine.rs:2033`), which is the
   measure of how real it is.

Classification: **partial.** The fix is a `Vdp::poke_vram` sibling to `poke_cram` — split
`write_vram_byte`'s two halves, drop the capture, keep the SAT write-through.

Note also the region asymmetry, worth recording: VRAM is the **only** region with a raw `&mut [u8]`.
CRAM has a semantic `poke_cram` and no `cram_mut`; VSRAM has no mutator at all.

**(c) Cost and risk.** Small core addition (~15 lines + a test that the poke and the port path agree in
VRAM, mirroring `cram_poke_matches_the_port_path`) plus a handler modelled on `write_cram`
(`engine.rs:1792`). Judgement estimate: **~120 lines total.** No contract change needed to serve it as
written.

Two deliberate choices to record, neither a deviation:
- We would gate on `require_paused` although D-16 notes §6 does not. That is *stricter* than the
  fragment, cannot make a result non-conformant, and follows `write_cram`'s and `write_memory`'s house
  rule. Report it upward as support for D-16's recommendation.
- We would refuse an out-of-range `addr` with `-32602` rather than clip, matching `read`'s
  *"refused, never clipped"* (`engine.rs:1683`).

---

### 4.4 `z80_read`, `z80_write`

**(a) What the fragments require.**

`z80_read`: `addr` required (hex, §6 bounds 0-0x3FFF in prose — D-32 records that bounds on hex-string
fields are not schema-expressible); `len?` integer 0-8192, **no default** (D-09). Result requires `addr`,
`len`, `bytes`. `caveat` declared absent. A pure read — the run-control state rule does not reach it.

`z80_write`: `addr` required; **exactly one of `bytes` or `value`**, enforced by `oneOf`. `value` is
`integer, minimum 0` with **no maximum**, because — the fragment's words — *"NO `width` companion exists
on this row (contrast write_memory), so this fragment states no maximum: bounding it would be choosing a
width the contract has not chosen."* Result requires `addr` and `len`, and the fragment says `len` is
*"Determinate for a `bytes` payload; underdetermined for a `value` one until §6 pins a width (audit
D-10)."*

**(b) What the core can do.**

Read: **ready.** `System::z80_ram()` (`system.rs:838`) is `pub`, returns `&[u8]` over the 8 KiB of Z80
RAM. `Z80::step` at `crates/oracle-core/src/z80/mod.rs:412` is present and complete as the brief said
(the whole documented instruction set; only undocumented opcodes remain) — but it is not needed for
either of these two methods, which are memory access, not execution.

Write: **partial.** There is **no `z80_ram_mut`** — `grep -n 'z80_ram_mut' crates/oracle-core/src/system.rs`
returns nothing, and `System`'s only `pub` mutators are `vram_mut`, `vdp_mut`, `scheduler_mut`
(`system.rs:875,885,895`). One accessor to add.

`capabilities.z80` is already published as `false` (`engine.rs:1044`) and flips with this parcel.

**(c) Cost and risk.** `z80_read` is **pure conformance work, start today** — perhaps 80 lines,
modelled on `read`'s VRAM arm.

**`z80_write` is the one hard contract block in the whole set.** The result's `len` is **required**, and
for a `value` payload there is no width to compute it from. Serving the `value` spelling means inventing
a width — which is exactly what the fragment refused to do and registered as D-10. Two honest options:

1. **Ship the `bytes` half only**, and refuse `value` with `-32602` naming the open contract question.
   This is conformant (the fragment does not require a server to accept both spellings — the `oneOf`
   constrains the *request*, not the server's acceptance set) and it is loud.
2. **Hold the whole method** until D-10 is ruled.

I recommend (1) with the CR raised at the same time, because it unblocks the read+write pair for the
`bytes` spelling — which is the spelling every observed caller would use for a driver upload — while
leaving the ambiguity visibly refused rather than quietly guessed.

**D-10 has two implementers** (us and the legacy C++ server) and must not be adjudicated as though one
speaks for both.

---

### 4.5 `breakpoint_add`, `breakpoint_clear`, `breakpoint_list`

Design questions are in §7 and are **not** answered here, per the brief. What follows is only what the
fragments require and what our own surface already establishes as precedent.

**(a) What the fragments require.**

`breakpoint_add`: `addr` XOR `symbol`, mechanically enforced by `oneOf`. Result requires `addr` — *"the
RESOLVED address […] the answer, not merely an echo, when the request named a symbol."* `caveat`
declared absent. **D-12** registers two open behaviours: whether re-adding at an occupied address is an
error or an idempotent success, and what refusals the row can produce at all (no `-32012` / `-32013` are
named for the `symbol` spelling even though §5 defines both).

`breakpoint_clear`: three-way exactly-one `all` | `addr` | `symbol`, `oneOf`-enforced. Result requires
`removed` (integer ≥ 0). **D-15**: clearing an address that holds no breakpoint is unspecified;
`checkpoint_drop` and `watchpoint_clear` both pin the idempotent reading, *"but a precedent is not the
row."*

`breakpoint_list`: no params. Result requires `breakpoints[]` of
`{addr, enabled, hits}` with `additionalProperties: false`. The fragment is emphatic that this list
carries **none** of §2.4's bounded-list companions — no `total`, `returned`, `truncated`, `limit`,
`cursor` — and that this is *"a deviation from the bounded-list rule reported rather than repaired"*
(**D-14**). It also notes that `enabled` is reported but **no catalogued method sets it**.

None of the three is subject to §6's run-control state rule: §6 rules that arming and clearing an
observer does not mutate the timeline.

**(b) What the core can do — the mechanism is ready; only the surface is unbuilt.**

`crates/oracle-core/src/bus.rs:305-318`, on `stop_requested`:

> a sink that raises its flag from `on_step_boundary(pc, _)` gets **classic breakpoint semantics** (stop
> *before* `pc` runs); a sink that raises it from `on_event`/`on_vdp_write` […] stops at the *next*
> boundary, after the triggering instruction has fully committed.

That is the whole emulation-side requirement, already ratified and already exercised by
`Engine::run_to`, which is a one-address breakpoint with an automatic clear. Address resolution is
`Engine::resolve_target` (`engine.rs:1278`), which already implements the `addr`/`symbol` pair and
already emits `-32012` / `-32013`.

`breakpoint` occurs exactly **3 times across `crates/*/src`**, all prose: `bus.rs:312` (the semantics
note above), `engine.rs:1050` (`"breakpoints": false`), and `oracle-replay/src/runner.rs:542` (a comment
about an arming-order hazard on the sibling server). Command:
`grep -rioc 'breakpoint' crates/`, exit 0.

*(Method note against myself: my first pass scoped that grep to two crates and measured 2. The audit's
count of 3 was right and mine was under-scoped. A grep that answers "does this exist anywhere" must be
scoped to everywhere.)*

**(c) Cost and risk.** Implementation is modest — a `Breakpoints` instrument beside `Watchpoints`
(`engine.rs:550`), a sink arm, three handlers, and flipping `capabilities.breakpoints`. Judgement
estimate: **~350-450 lines** once the surface is decided.

**Design is blocked.** D-13 is, in the audit's own words, *"the largest single gap this pass found and it
is too big to fold into a schema transcription."* See §7.

⚠ **We would be strictly better than the legacy server here, and a consumer is already paying for the
difference.** `aeon/tools/raster_source_gate.py:32-39` documents that in `deterministic=True` mode the
legacy server answers `breakpoint_add` with *"det-mode stop granularity: PC may precede the breakpoint"*,
because its serial scheduler's rollback stops at commit granularity — and that a stop one instruction
early would make that gate **pass on code that never applied the offset at all**. The gate therefore
asks for a threaded launcher path just to get exact stop PCs. Our loop stops at the boundary *before*
`pc` runs, by construction. Serving breakpoints retires that workaround.

---

### 4.6 `wait_for_break`

**(a) What the fragment requires.** Params: `timeoutMs?` integer ≥ 0, **no default and no ceiling** —
the fragment declines to invent one because *"this row predates D12 and is deprecated"* (D-07). Result:
`pc?`, `symbol?`, `timeoutReached?`, `waitedMs?` — **every handler key is optional, so a conformant reply
can be the envelope alone.** `running` is deliberately *not* declared: it is the machine stamp's
(§2.2/D11), and §6 lists it only because the row predates D11 (**D-05**). `caveat` declared absent,
because `timeoutReached` is already the typed discriminant §2.4 rule 3 asks for. **D-06**: §5 spells the
field `timeout_reached`, §6 spells it `timeoutReached`; §3's camelCase convention makes §6's correct.
**D-08**: `symbol` has no `symbolDisp` companion, alone among the symbol-bearing rows, so a non-zero
displacement is unreportable.

The fragment states the deprecation and the retention obligation together: deprecated by
`emulator/stopped`, but *"clients without the `events` capability may still poll it, and a server MUST
keep answering it."*

**(b) What the core can do.** The machine side is trivial — this is `resume` plus "tell me when it
stops", and every piece exists. **The absent capability is not emulation, it is transport.**

Our server is synchronous by design: `Engine::dispatch` (`engine.rs:984`) returns
`Result<Value, RpcError>` on the engine thread, and every run method (`run_frames`, `run_to`) runs the
machine *inside* the handler and returns when it stops. A `wait_for_break {timeoutMs: 120000}` handler in
that shape blocks the engine for two minutes: no other client can be served, `emulator/pause` cannot
arrive, and the socket's own liveness is at the mercy of the wait. That is the real cost driver, it is
architectural rather than per-method, and it is what §6's estimate is most sensitive to.

Classification: **ready** for the machine half; **absent** for the blocking/async transport half.

**(c) Cost and risk.** **This is the estimate I am least confident in and the one that sets the date owed
to aeon.** See §6 for the sensitivity analysis.

---

### 4.7 VGM — `vgm_start`, `vgm_stop`, `vgm_status`

**(a) What the fragments require.**

`vgm_start`: `path?` string, minLength 1, naming a file **on the server's filesystem** (§6's paths note,
D8 — a trusted loopback local-developer API). Result requires `path`, *"REQUIRED even when the client
supplied it, because a caller who did not supply one has no other way to find the capture."*
**D-18**: what a start does while a capture is already running is unspecified.

`vgm_stop`: no params. Result requires `path`, `samples`, `durationSec` (a JSON **number** — fractional
by construction), `bytes`. D-18 again: what a stop answers when nothing is running is unspecified.

`vgm_status`: no params. Result requires `active`, `samples`, `durationSec`, `bytes` — **all four even
when `active` is false**, because §2.3's rule applies (absence and zero must not both mean "nothing has
been captured"). **D-19**: this row carries no `path` though both siblings do.

All three declare `caveat` absent.

**(b) What the core can do — partial, and the closest of the audio group.**

`VgmLogger` (`crates/oracle-core/src/vgm.rs:76`) implements `BusEventSink` (`vgm.rs:276-358`) and is
attached per-run via `run_frames_with_sink`. Decode, normalisation, sub-frame timing and canonical VGM
serialisation are all done and tested.

- **Start exists in substance.** `VgmLogger::reset` (`vgm.rs:163`) is documented in the core as
  *"the `vgm_start` reset (RT5)"* — it clears records, counters and latches.
- **`render_vgm` (`vgm.rs:180`) is `&self`** — non-consuming, non-clearing. It can be called mid-capture,
  which is what makes `vgm_status.bytes` reachable at all (`render_vgm().len()`, O(n) per poll; no
  incremental byte counter exists).
- **No `active` flag and no stop.** There is no `recording` field. The cleanest modelling is *attach /
  detach from the `Fanout`* — `impl BusEventSink for Option<S>` (`bus.rs:370`) gives "attached only
  sometimes" for free and needs no core change.
- **`samples` is a local, not state.** `render_vgm` computes `total_samples` as a local at `vgm.rs:182`
  and writes it into the header at `:232`; it is never stored and never exposed. It is derivable O(1)
  from `records()` (`vgm.rs:130`) or `mclks()` (`vgm.rs:137`) using the formula `render_vgm` itself uses
  at `vgm.rs:187`.
- **File I/O is absent from the core by charter and must stay that way.**
  `grep -rn 'std::fs\|File::create\|fs::write' crates/oracle-core/src/` → **exit 1**. `oracle-core` is
  declared "deterministic, no-I/O" in its own manifest. The write belongs in `oracle-aether`, which
  already does I/O and already holds `rom_path`/`symbols_path` (`engine.rs:503-504`).

**⚠ Important separation:** `vgm.rs` is **not** feature-gated — `crates/oracle-core/src/lib.rs` declares
`pub mod vgm;` plainly. So this parcel does **not** need the `synth` feature that §4.8 and §4.9 do. That
is what makes it the right first step of the audio group.

**(c) Cost and risk.** Engine-owned `VgmLogger` field beside `watchpoints` (`engine.rs:550`), threaded
into **both** run drivers (`engine.rs:874-879`, `engine.rs:915-921`) **and** the hosted player loop
(`host.rs`) — missing the hosted driver would silently capture nothing while the player runs. Plus three
handlers, path handling, and flipping `capabilities.vgm` (`engine.rs:1047`). Judgement estimate:
**~350-450 lines.** No contract change required to serve as written; D-18/D-19 are advisory.

---

### 4.8 Channel masks — `get_channel_states`, `set_channel_enabled`

**(a) What the fragments require.** `get_channel_states`: no params; result requires **all eleven**
booleans `fm1`-`fm6`, `dac`, `psg1`-`psg3`, `psgNoise` — all required because *"an omitted mask reads
exactly like a mask that is off."* `set_channel_enabled`: `channel` (**bare string, minLength 1, no
enum**) and `enabled` both required; result echoes both, with `enabled` being *"the state the channel is
in AFTER this call."* Both declare `caveat` absent. Neither is subject to the run-control state rule — a
mixer setting is not machine state.

The missing enum is deliberate and registered as **D-17**: §6 states no vocabulary, and emitting a
request enum a server validates under §2.5 would refuse by *name* every client that spells a member
differently. The eleven `get_channel_states` keys are the recommended value set.

**(b) What the core can do — absent.**

There is **no debugger-owned mask layer anywhere**. Command:
`grep -rniE 'debug_mute|channel_enabled|set_channel|chan_mask|mute_mask|force_mute|solo' --include='*.rs' crates/`
→ exit 0, **one hit**, and it is the method-name string in `schema_conformance.rs:397`. Everything else
that looks like a mute is guest-driven chip state and is excluded by definition: the SN76489 attenuation
register, `$60` AM enable, `$B4` L/R pan, `$2B` DAC enable, `$22` LFO enable, `$28` key-on/off, `$27`
timer control. The player has only a **global master mute** (`oracle-frontend/src/audio.rs:190`
`gain_for(step, muted)`), a host-side gain, not a per-channel mask.

The two mixing points a mask must hook are identified exactly:

- FM + DAC: `Ym2612Synth::tick_native`, `crates/oracle-core/src/synth/ym2612_synth.rs:1159-1173`.
- PSG + noise: `Sn76489::next_sample`, `crates/oracle-core/src/synth/sn76489.rs:243-248`.

**Discipline the existing code already establishes:** at `ym2612_synth.rs:1164` the DAC path does
`if i == 5 && *dac_enabled { continue; }` — it **still ticks the channel and then discards the output**,
and the comment at `:1160-1162` explains why: envelopes must keep evolving, or an unmute pops and the
chip state diverges from an unmuted run. A debugger mask must follow the same rule.

**⚠ The blocking prerequisite: `oracle-aether` cannot see the synth at all today.**
`crates/oracle-core/src/lib.rs:31` gates the module `#[cfg(feature = "synth")]`;
`crates/oracle-core/Cargo.toml:11` declares `synth = []`, **default off**; and
`crates/oracle-aether/Cargo.toml:13` depends on `oracle-core` with **no `features` key**. Only
`oracle-frontend` enables it. So the first audio-mask commit must also decide whether the Aether server
compiles the synth — which the core's own comment frames as deliberate isolation from the currency
gates.

**(c) Cost and risk.** Net-new core code in two chip structs, new `pub` accessors on `AudioSink` (which
today has exactly six: `set_console_model`, `console_model`, `sample_rate`, `samples`, `drain`,
`len_frames`, `audio_sink.rs:112-142`), an engine-owned `AudioSink`, the feature-enable decision, plus
two handlers and a name↔index mapping for the eleven keys. Judgement estimate: **~400-500 lines** plus
the feature decision. CR advisory only (D-17: recommend the eleven-member enum).

Per house practice on three-surface parity, this parcel should also consider a player-GUI channel-mute
panel; today the player has no per-channel control to reuse or extend.

---

### 4.9 `audio_spectrum`

**(a) What the fragment requires.** Params: `source?` enum `fm|psg` default `fm`; `fftSize?` integer
256-32768 default 4096, **coerced to a power of two** rather than refused; `maxHz?` number ≥ 0. Result
requires `source`, `sampleRate`, `fftSize` (enumerated over the eight powers of two — the size *actually
used*), `binHz`, `dominantHz`, `samplesAvailable`, `magnitudes[]`. `magnitudes` is **structurally
bounded** (its length follows from the echoed `fftSize` narrowed by `maxHz`), so per §2.4 clause (d) it
takes neither a truncation flag nor a cursor. `caveat` declared absent.

**D-22** registers three gaps: `maxHz` is not echoed although it determines the array's length;
`magnitudes`' **unit is unstated** (linear amplitude or dB), which two conformant servers can answer
differently with identically-shaped replies; and `samplesAvailable` below `fftSize` has no stated
consequence — which the fragment itself notes is *"exactly the weaker-answer condition §2.4 exists for"*,
while `caveat` is declared absent.

**(b) What the core can do — partial, with the central sub-primitive absent.**

- **FFT: absent, tree-wide.** Command:
  `grep -rniE 'fft|dft|fourier|spectrum|spectral|goertzel|rustfft|realfft|num-complex' crates/oracle-core/src/ crates/oracle-aether/src/ crates/oracle-frontend/src/ crates/oracle-replay/src/`
  → **exit 1**. Not in any `src/`, not in a test helper. Reading the manifests directly (no cargo):
  `oracle-core`'s only runtime dependency is `bincode`; `oracle-aether`'s are `oracle-core` and
  `serde_json`. **Both manifests carry explicit "must not grow" comments** about their dependency sets,
  so adding `rustfft` is a charter decision, not a routine add. A hand-rolled radix-2 Cooley-Tukey over
  `f64` is roughly 40 lines and needs no dependency; that is the realistic path.
- **FM and PSG are only ever mixed.** `AudioSink::render_frame`,
  `crates/oracle-core/src/synth/audio_sink.rs:225-238`, computes `psg` and `(fm_l, fm_r)` as locals and
  immediately sums them through the console filter into the single `self.out` buffer. Separating them is
  one push each inside the same loop — small, purely additive to a caller-owned struct. Note the console
  filter is applied to the *sum* only, so per-source buffers would be pre-filter; the fragment's `source`
  echo does not say which, so that is a defensible choice that should be documented.
- **Sample rate: ready.** `AudioSink::sample_rate()` (`audio_sink.rs:124`), default 44 100
  (`audio_sink.rs:24`).
- **The buffer is retained, and unbounded.** `samples()` (`audio_sink.rs:129`) borrows *"does not
  clear"*; `len_frames()` (`audio_sink.rs:140`) is `samplesAvailable` directly. But there is no ring and
  no cap: an engine-owned sink that never `drain`s grows at ~176 KB/s. A spectrum needs at most 32 768
  samples of history, so a bounded ring is required — a design gap, not a plumbing one.

Same synth-feature prerequisite as §4.8.

**(c) Cost and risk.** FFT + windowing, per-source buffers, a bounded ring, `maxHz` narrowing, the
power-of-two coercion, and the engine-owned sink. Judgement estimate: **~500-600 lines**, the largest
single method in the set. CR advisory (D-22) — but note the **unit** question is one a server cannot
avoid answering: whatever we emit, we should declare it and ask for it to be pinned, because two
conformant servers disagreeing on linear-vs-dB is invisible on the wire.

Carries two of the four param-name conflicts: `fftSize`, `maxHz`.

---

### 4.10 Layer masks — `get_layer_states`, `set_layer_enabled`

**(a) What the fragments require.** `get_layer_states`: no params; result requires all four booleans
`planeA`, `planeB`, `window`, `sprites`. `set_layer_enabled`: `layer` (**bare string, no enum** — D-17,
same reasoning as its channel sibling, with the four `get_layer_states` keys as the recommended set) and
`enabled`, both required and both echoed. `caveat` declared absent on both. Explicitly **not** subject to
the run-control state rule: *"This is a display MASK, not machine state."*

**(b) What the core can do — absent, and harder than it looks.**

Command:
`grep -rnE 'layer_mask|layers_enabled|layer_enable|set_layer|enabled_layers|show_layer|hide_layer|LayerMask|LayerStates|layer_visible|render_mask|debug_mask' --include='*.rs' .`
→ exit 0, **one hit**, the method-name string in `schema_conformance.rs:398`. No `src/` hit in any crate.
`pub enum Layer` (`crates/oracle-core/src/render.rs:91`) exists but is an **attribution output** — "who
won this dot" — never an input gate.

The renderer is a **single fused per-pixel priority resolve**, not separable passes:
`Vdp::resolve_line` (`render.rs:1272`) is the sole source that `render_line`, `render_scanline`,
`pixel_attribution` and `frame_report` all derive from; its per-dot loop (`render.rs:1300-1307`) calls
`plane_pixel(Plane::B, …)`, `a_slot_pixel(…)`, the sprite buffer, then `resolve_dot`. Three consequences
make "turn off plane B" ill-defined as written:

1. **Plane A and the window share one slot.** `Vdp::a_slot_pixel` (`render.rs:1149-1160`) returns the
   window pixel inside the window span, the R9 window-bug reused pixel just right of a left boundary,
   else plane A. Disabling `window` alone means bypassing one match arm — and the bug arm is *attributed*
   as `Layer::PlaneA` while sampling the *window's* nametable. A four-bit mask has no natural answer for
   that pixel.
2. **Shadow/highlight reads the priority bits of transparent pixels.** `Vdp::sh_state`
   (`render.rs:1207-1232`) defaults to `Shadow` iff both the A-slot and B priority bits are 0, reading
   them *even when those pixels are transparent* — the documented Bloodlines light-ray trick. "Masked =
   transparent" and "masked = not there" give visibly different output.
3. **Sprite operators shift underlying pixels without being drawn** (`resolve_dot`,
   `render.rs:1251-1261`). Masking `sprites` removes those shifts too — probably right, but a behaviour,
   not a no-op.

**Determinism impact, measured.** `state_hash` proper is safe: `StateHash::compute`
(`crates/oracle-core/src/state_hash.rs:44`) hashes exactly VRAM, CRAM, VSRAM and the registers — no
framebuffer, no render output. But `emulator/state_hash {includeFramebuffer: true}`
(`engine.rs:2063-2073`) FNV-hashes the RGB bytes, and **both** framebuffer sources go through the masked
path (`engine.rs:1358-1370`). So do `screenshot`, `scanlines` and `pixel_attribution`. Golden frames stay
frozen only if the mask defaults to all-four-on.

**Injection cost.** Roughly six lines inside `resolve_line`'s per-dot loop — *but* `resolve_line` is
`&self` on `Vdp`, so the mask must either become a `Vdp` field (which lands in the bincode snapshot and
the whole-machine `PartialEq` the determinism test asserts, since `Vdp` derives
`Clone, PartialEq, Eq, Encode, Decode`) or be threaded through six public render signatures and every
external caller. Neither is free.

**(c) Cost and risk.** Judgement estimate: **~300-400 lines**, but the number is not the risk — the three
ambiguities above are, and they need ruling before code. CR advisory (D-17). Zero consumers.

---

### 4.11 `ping`

**(a) What the fragment requires.** No params. Result requires `version`, `integer ≥ 0`. The fragment is
unusually explicit that the *value space is undefined*: §10 decision 2's `{"version": 2}` is the only
typed evidence in the document and simultaneously calls the value *"pre-bus"* and *"not part of bus
versioning"*. The field's own description says **"A client MUST NOT branch on this value; use
initialize."** Registered as **D-01**, and the fragment deliberately declines to close it by inventing a
constant. `caveat` declared absent.

**(b) Core.** Not applicable — no machine is involved. `ping` appears **nowhere** in
`crates/oracle-aether/src/` (verified: every apparent hit is a substring of "dropping", "stepping",
"mapping", "Sleeping").

**(c) Cost and risk.** Mechanically a handful of lines. The cost is entirely the decision: emitting any
integer is schema-conformant, but choosing one without a ruling is inventing the constant the fragment
refused to invent. Either get D-01 ruled, or pick a value and declare the choice loudly upward.

**Consumer note that should govern the priority.** `ping` is the **only one of the 21 with no legacy MCP
tool** — the MCP surface has 63 tools and every other one of the 21 is among them. Its single live caller
is `empyrean/clients/typescript/scripts/smoke.mjs:13`, and that line reads:

```js
if (client.hasMethod("emulator/ping")) console.log("ping:", …);
```

The only consumer already handles our not serving it, gracefully, by design.

---

### 4.12 `log_clear`

**(a) What the fragment requires.** No params, and **no result keys** — `result` is a bare `$ref` to
`replyFields`, on `emulator/release_all`'s precedent. `caveat` declared absent. **D-23** registers what
the row does *not* carry: no `removed`/`cleared` count, so a caller cannot tell a clear that discarded a
thousand entries from one that discarded nothing — the field both `checkpoint_drop` and
`breakpoint_clear` do have. D-23 also registers the open question of what a clear does to the
`token`/`since` continuation `emulator/log_tail` hands out, and the audit recommends settling it
**together with D-29** (`log_tail`, one of the eight BLOCKED rows with no fragment at all) rather than
patching this row alone.

**(b) Core — absent, and more so than "the method isn't built".** There is **no log surface anywhere**:
no ring, no buffer, no `log_tail`, nothing in either crate. A sweep for a log structure in
`crates/oracle-aether/src/` and `crates/oracle-core/src/` returns only unrelated prose about bus
transaction logging.

**(c) Cost and risk.** Serving `log_clear` against no log is *conformant* — its result is the envelope
and clearing nothing is indistinguishable from clearing something, which is exactly D-23's complaint —
and hollow. The real work is a log surface, which has no fragment to build against (`emulator/log_tail`
is not among the 58). **Recommendation: do not serve this method alone.** It should arrive with the log,
after D-29 is decided.

---

## 5. Consumer sweep

### 5.1 Method and its exclusions

Trees swept: `../aeon`, `../aurora`, `../seraph`, `../sigil`, `../empyrean` — **all five exist on disk**
(reported explicitly, so an empty result is a measurement rather than a missing tree).

Exclusion filter, stated so it can be checked:

```
grep -rn --binary-files=without-match <PATTERN> aeon aurora seraph sigil empyrean
  | grep -v "\.claude/worktrees/"
  | grep -vE "/(vendor|node_modules|target|\.git)/"
```

Both an identifier grep (bare token) and a quoted-string grep (`"emulator/<name>"`,
`emulator_<name>`) were run and reconciled.

### 5.2 The A/B reconciliation — both directions of mismatch are real

**B found, A missed** (the bare token was too noisy to grep for `step` and `ping`): `emulator/step` in
`aeon/docs/research/phase_harness/wedge_probe.py:38` and `wedge_probe_threaded.py:38`,
`aeon/tools/parallax_hscroll_probe.py:555`, `aeon/tools/raster_frame_epoch_probe.py:226`;
`emulator/ping` in `empyrean/clients/typescript/scripts/smoke.mjs:13` and `test/client.test.ts:21`.

**A found, B missed — and this is the larger direction.** An entire cluster of 14 scripts under
`sigil/crates/sigil-harness/golden/ab/**/*.py` calls these methods with **no `emulator/` prefix at
all** — literally `"breakpoint_add"`, `"wait_for_break"`, `"step"`, `"run_to_scanline"`. Verified
firsthand, e.g.:

```
sigil/crates/sigil-harness/golden/ab/g9/ab_g9_state.py:62:  await call(bus, "breakpoint_add", {"addr": f"0x{UPDATE_ENTRY:X}"})
sigil/crates/sigil-harness/golden/ab/g9/ab_g9_state.py:65:      await call(bus, "wait_for_break", {})
```

Their local helper is a pure passthrough and `empyrean/clients/python/aether.py:134` adds no prefix
either, so the literal wire method is the bare name. **A quoted-string grep on `emulator/…` is blind to
all ~49 of these hits.** They are archival (see 5.3), but the spelling is a second migration hazard worth
recording: our dispatch matches `METHODS` names exactly, so a bare name is `-32601`.

### 5.3 Classification — a file existing is not an invocation

**AUTOMATED (one chain, three methods).** This is the only automated bus consumer anywhere in the
workspace:

```
systemd user timer                       (aeon/CLAUDE.md:146)
 → aeon/tools/nightly_effects_gates.sh:60
 → aeon/tools/effects_gates.py:617       raster_source_gate.py
 → aeon/tools/effects_gates.py:635       snapshot_poison_gate.py
```

Both gates run by default (the nightly passes no `--gates` flag, and `effects_gates.py:493 wanted()`
returns true when `want is None`). There is **no `.github/workflows/` in aeon and no Makefile in any of
the five trees**; `aeon/build.sh` runs `pytest tools/`, which exercises only pure/subprocess-fake tests
and touches no bus.

**MANUAL LIVE** (a real call site with a documented `python3 tools/…` usage, but no automated invoker):
`aeon/tools/evict_witness.py`, `parallax_hscroll_probe.py` (also reached from `curve_probe.py:305`),
`raster_frame_epoch_probe.py`, `engine_baseline_probe.py`, and
`empyrean/clients/typescript/scripts/smoke.mjs`.

**ARCHIVAL EVIDENCE** (ran once, nothing invokes them now): all 14 `sigil/…/golden/ab/` scripts,
referenced only from `docs/superpowers/notes/*.md` and `golden/PROVENANCE.md` as provenance for past A/B
runs. No `.rs`, `.sh`, `.toml` or test references them.

**SPEC / OFFLINE** (not a bus call at all, and the reason raw grep counts look alive):
`empyrean/contract/schema/bus-protocol.schema.json` (31 hits) and `contract/schema/tests/vectors.json`
(117 hits), consumed by `validate_contract_schema.py`, which never opens a socket. **This is where most
of the 21 appear.**

**MOCK:** `empyrean/clients/typescript/test/client.test.ts:21` — a capability-list string against a
`FakeServer`. **DEAD:** `aeon/docs/research/phase_harness/wedge_probe{,_threaded}.py`, unreferenced
research artifacts parked under `docs/`.

### 5.4 Verdicts

| verdict | methods |
|---|---|
| **automated consumer** | `wait_for_break`, `breakpoint_add`, `breakpoint_clear` |
| **manual live consumer** | `step`, `run_to_scanline`, `write_vram`, `ping` |
| **no consumer at all** | `audio_spectrum`, `breakpoint_list`, `get_channel_states`, `get_layer_states`, `log_clear`, `set_channel_enabled`, `set_layer_enabled`, `step_out`, `step_over`, `vgm_start`, `vgm_status`, `vgm_stop`, `z80_read`, `z80_write` (14) |

Per tree: **aeon** is the only tree with automated consumers (8 code files). **sigil** has 14 archival
files, all bare-spelled. **empyrean** has the spec, one manual `ping` smoke script and one mock.
**aurora** has **no code hits at all** — two prose mentions
(`docs/ideas/2026-06-16-art-suite-vision.md:136,181`). **seraph** has **no code hits** — three prose
mentions naming `vgm_start/stop` and `z80_read` as *planned* consumers in S0/S2 plan documents.

Single most useful number here: **14 of 21 have no consumer of any kind.** Building this set is
capability work against a contract, not demand work against a queue — with the sharp exception of the
three methods in §6.

---

## 6. `wait_for_break` — the standing obligation to aeon

**The obligation is real and it is bigger than I was told.**

### 6.1 What was verified

```
aeon/tools/raster_source_gate.py:168:    r = await b.call("emulator/wait_for_break", {"timeout_ms": 120000})
aeon/tools/snapshot_poison_gate.py:64:    r = await b.call("emulator/wait_for_break", {"timeout_ms": 20000})
```

Both files exist, both really call it, and both send **`timeout_ms` — snake_case**. The contract
(`empyrean/contract/protocol.md:857`) and the fragment both spell it **`timeoutMs`**, and the fragment
sets `unevaluatedProperties: false`. Our server refuses an unknown top-level param with `-32602` at the
single dispatch choke **before the handler runs** (`engine.rs:999`); the legacy server ignores it and
silently defaults. So a gate that believes it waits 120 s would, against a naive cutover, either be
refused outright or wait a default it never chose.

**No caller anywhere in the workspace spells it `timeoutMs`.** The archival sigil scripts send
`wait_for_break {}` with no timeout at all and would be unaffected by the rename (though they would be
affected by the bare-name spelling — §5.2).

### 6.2 ⚠ The obligation covers three methods, not one

The brief framed this as a `wait_for_break` heads-up. It is not. **The same two gate scripts call
`breakpoint_add` and `breakpoint_clear` in the same flow:**

```
aeon/tools/raster_source_gate.py:131   emulator/breakpoint_clear {"all": True}
aeon/tools/raster_source_gate.py:161   emulator/breakpoint_add   {"addr": hex(probe_pc)}
aeon/tools/raster_source_gate.py:168   emulator/wait_for_break   {"timeout_ms": 120000}
aeon/tools/raster_source_gate.py:173   emulator/breakpoint_clear {"all": True}
aeon/tools/snapshot_poison_gate.py:62  emulator/breakpoint_add   {"addr": hex(addr)}
aeon/tools/snapshot_poison_gate.py:64  emulator/wait_for_break   {"timeout_ms": 20000}
aeon/tools/snapshot_poison_gate.py:68  emulator/breakpoint_clear {"all": True}
```

Those three methods **cannot migrate piecemeal**: the flow is arm → wait → clear. A server that serves
`wait_for_break` but not `breakpoint_add` is useless to these gates; a server that serves the breakpoints
but not the wait is equally useless. **The aeon-facing parcel is all three**, and that is what sets the
date.

Note the breakpoint calls carry no diverging param names (`{"all": True}`, `{"addr": hex(...)}`), so the
`-32602` hazard is confined to `timeout_ms` alone.

### 6.3 What the date estimate is sensitive to

The pricing that sets this date is dominated by **one thing that is not emulation**: `wait_for_break` is
a *blocking* call on a server whose entire dispatch model is synchronous (`Engine::dispatch`,
`engine.rs:984`, returns a value on the engine thread). A 120-second handler blocks the engine — no
second client, no `emulator/pause` arriving to end the wait. Solving that means either an async/deferred
reply path or an out-of-band completion, and **that is an architectural change to the server, not a
method.**

The estimate is therefore sensitive, in decreasing order, to:

1. **Whether a deferred-reply mechanism must be built.** If yes, this parcel is measured in weeks, not
   days, and it is the single biggest unknown in this whole survey. *(This is also why serving the
   `emulator/stopped` event — which we already do — is the better answer for any client that can take
   it, and why the fragment calls the method deprecated.)*
2. **D-13's ruling lead time**, because breakpoints ship in the same parcel and cannot be designed
   unilaterally (§7).
3. Not at all sensitive to the emulation work, which §4.5 and §4.6 show is ready.

**Recommendation on the date:** do not send aeon a date derived from the method count. Send it once
(1) is resolved, and send it as a date for the *three-method parcel*. Sending a `wait_for_break`-only
date would be precisely the kind of promise that looks kept and breaks a nightly at 04:17.

**Also owed, and cheap:** `run_to_scanline` carries the same class of conflict (`maxFrames` vs
`max_frames`) and has a live aeon consumer at `aeon/tools/engine_baseline_probe.py:586`. It should ride
the same heads-up rather than generate a second one later.

---

## 7. `breakpoint_*` — design questions, flagged not answered

Per the brief I am not designing this surface. What follows is what the fragments require, what our own
watchpoint surface already establishes as house precedent, and the questions that fall out of the
mismatch between the two.

**D-13 has two implementers — us and the legacy C++ server — and must never be adjudicated as though one
speaks for both.**

### 7.1 What our watchpoint surface already establishes

- **Opaque, server-assigned, never-reused handles.** `WatchId` (`watchpoints.rs:176`), whose own doc
  says *"Ids are never reused: `clear` and `remove` retire an id permanently, so a stale handle resolves
  to nothing rather than silently to a different watch."* Wire form via `watch_wire_id` /
  `resolve_watch_handle` (`engine.rs:3965`, `:3974`).
- **Idempotent clear.** `watchpoint_clear` (`engine.rs:3605`) is *"deliberately permissive"*: an unknown
  or retired handle answers `removed: 0` rather than refusing, because *"an error a client must learn to
  swallow teaches clients to swallow errors."*
- **Evidence is not deleted with the object.** Clearing a watch keeps its recorded hits, because *"a
  destructive clear would let one client erase another's evidence on a shared bus."*
- **Full §2.4 clause (a) bounded-list companions.** `watchpoint_list` (`engine.rs:3638`) emits `total`,
  `returned`, `limit`, `truncated` and a handle-valued `cursor`.
- **An advertised cap with a loud refusal.** `capabilities.watchpoints.maxWatches` (`engine.rs:1071`),
  and at the cap a `-32005 {reason:"watchCapReached", cap, count}` (`engine.rs:119`).

### 7.2 The mismatch, stated neutrally

The breakpoint fragments require the opposite of four of those five: breakpoints are addressed **by
address**, not by handle; `breakpoint_list` carries **none** of the bounded-list companions (D-14); there
is **no cap and no refusal** (D-13c); and `enabled` is reported by `breakpoint_list` while **no
catalogued method sets it** (D-13a).

The audit's correction to D-13's framing matters here and is recorded rather than argued: the history is
not "breakpoints never grew the discipline watchpoints got" — it is that **the watch surface was rebuilt
on the successor and breakpoints were never carried across**, and on the legacy server the *watchpoint*
surface is the worse of the two (add-only, no list/clear/hits). Two consequences the audit draws: a
duplicate address is **empirically possible** on the legacy server, which cuts against D-12's idempotent
reading; and the documented harm — an agent clearing seven breakpoints it judged "not mine", one at
1 691 410 hits, promising a restore it had no means to perform — is on our record, in
`docs/2026-07-23-timing-ground-truth-fable.md:162-165`.

### 7.3 The questions I am flagging for you

1. **Handle or address?** Adopting handles matches every other object on our bus and fixes the
   two-client hazard, but it is a **contract change**, not an implementation choice — the fragments
   require an `addr`-shaped `breakpoint_clear`. Do we implement the row as written and raise the CR, or
   hold?
2. **`enabled` is unwritable.** Do we emit it as a constant `true` (conformant, and carries zero bits),
   or does the CR add a `breakpoint_set_enabled`?
3. **`breakpoint_list` and §2.4.** D-14 offers two readings — complete-by-construction, or
   policy-bounded. Emitting companions against the current fragment **fails §8 item 20's closure**, so
   this must be decided before code, not during it.
4. **A cap.** Our watch surface has one and advertises it. `capabilities.breakpoints` is currently a
   bare `false` (`engine.rs:1050`) — flipping it to an object like `watchpoints`' is a wire change to the
   handshake.
5. **Duplicate add (D-12) and clear-of-nothing (D-15).** Both unspecified; our house precedent is
   unanimous for the idempotent reading, but the audit notes the legacy server's behaviour cuts the other
   way on duplicates.
6. ⚠ **A gap we would inherit for free unless it is fixed first.** `breakpoint_add`/`breakpoint_clear`
   want `Engine::resolve_target`, which does **not** enforce the `addr` XOR `symbol` alternation — see
   §2.2. Reusing it as-is reproduces an existing unregistered divergence in two more places.

### 7.4 The one thing that is unambiguously in our favour

Whatever surface is ruled, the *mechanism* is ready and is better than the incumbent's: our run loop
stops at the instruction boundary **before** `pc` executes, by construction (`bus.rs:305-318`), whereas
the legacy server's deterministic mode warns that *"PC may precede the breakpoint"* and forces
`aeon/tools/raster_source_gate.py` onto a threaded launcher to get exact stop PCs
(`raster_source_gate.py:32-39`). Serving breakpoints retires a live workaround in the only automated
consumer in the workspace. That is worth saying in the CR.

---

## 8. Where this survey contradicts what I was told

The brief that commissioned this survey asked loudly for its own numbers to be checked. Nine things came
back different. The membership of the 21 was **not** one of them — that was exactly right.

1. **`engine.rs` carries three events, not two.** The brief named `emulator/stopped` and
   `emulator/resumed` as the contamination to watch for. `EVENTS` (`engine.rs:428`) also holds
   **`emulator/romReloaded`**. The prescribed grep misses it only because the character class `[a-z0-9_]`
   excludes the capital `R`; widen it and the naive derivation is off by one, hiding a method. §1.2.

2. **The protocol spec is not at `docs/protocol.md`.** The brief's command
   `git -C ../empyrean show origin/main:docs/protocol.md` **fails** — that path does not exist at
   `origin/main` (`7df15a8`). It is `contract/protocol.md`. The audit path in the brief was correct.

3. **`crates/oracle-core/src/system.rs:828 step_instruction` is not the stepping primitive.** It exists
   as described, but its own doc says it *does not advance the master clock*, and it has **zero callers
   in any `src/`** — only a test and an example. Building `emulator/step` on it would freeze the VDP,
   Z80, FM and scheduler while the 68000 moved. The real primitive is `run_until_with_sink`'s
   `on_step_boundary` / `stop_requested` pair. §4.1.

4. **The watchpoint machinery is the nearest relative of the breakpoint *surface*, not of its
   *mechanism*.** The brief pointed at `watchpoint_add/list/clear/hits`. That is right for surface
   precedent (handles, idempotent clear, bounded lists) — but watchpoints match **bus accesses**, and a
   breakpoint is an **execution** stop. The mechanical relative is `Engine::run_to`, and the core already
   ratifies it in as many words at `bus.rs:312`. Worse, the two conflict: our watch surface's precedent
   is the opposite of what the breakpoint fragments require on four counts. §7.

5. **`System::vram_mut()` makes `write_vram` look free. It is not.** It is a `pub`-ised test hatch
   (its own doc says so), and writing through it bypasses the SAT-cache write-through in
   `Vdp::write_vram_byte` (`vdp.rs:794-801`). A poked sprite table would read back correctly through
   `read_vram` and be invisible to `emulator/sprites` and the renderer. §4.3.

6. **The synth tree the brief pointed at is not compiled into the Aether server at all.**
   `oracle-core/src/lib.rs:31` gates it `#[cfg(feature = "synth")]`, default off, and
   `oracle-aether/Cargo.toml:13` enables no features. Three of the audio methods carry a
   feature-enable decision before a line of their own code is written. Two path corrections too:
   `sn76489.rs` is at `synth/sn76489.rs`, not top level; and `vgm.rs` is **not** feature-gated, which is
   what separates the VGM parcel from the rest of the audio group. §4.7, §4.8.

7. **The aeon obligation covers three methods, not one.** The same two nightly gate scripts call
   `breakpoint_add` and `breakpoint_clear` around every `wait_for_break`. They cannot migrate
   separately. §6.2.

8. **The fragment's scanline range is unreachable.** `run_to_scanline` accepts `line` 0-511;
   `LINES_PER_FRAME = 262` (`vdp.rs:19`, statically asserted at `system.rs:84`). Lines 262-511 can never
   occur. §4.2.

9. **No fragment declares any error condition — all 58.** The brief asked for "declared error
   conditions" per method; the schema declares none anywhere. Every error obligation cited in this
   document comes from prose. §2.1.

**And one correction against myself, recorded because the method matters more than the number.** My
first sweep for `breakpoint` in the source scoped the grep to two crates and measured **2** occurrences,
which appeared to contradict the audit's **3**. The audit was right: the third is
`oracle-replay/src/runner.rs:542`. A grep whose *emptiness* or *count* is the finding must be scoped to
everywhere, or it measures the scope rather than the tree.

---

## 9. The proposed ordering

### 9.1 The two facts that drive the sequence

**Fact one: the aeon-facing work is one parcel of three, and it is the parcel that is design-blocked.**
`breakpoint_add`, `breakpoint_clear` and `wait_for_break` are called in a single arm→wait→clear flow by
the only automated consumer in the workspace (§6.2), and breakpoints cannot be designed unilaterally
(D-13, two implementers). So the **D-13 change request is on the critical path for the only demand-driven
work in the set**, and its lead time — an empyrean ruling — consumes no implementation capacity. It
should be raised **first and immediately**, in parallel with everything else.

**Fact two: cheapest-first would rework a surface twice.** Stepping and breakpoints are the same
mechanism — `on_step_boundary` + `stop_requested` + `emit_stopped` + `require_paused`. Building
breakpoints first would design the run-stop surface under a blocked contract question and then rebuild
it for `step*`. Building stepping first settles the house shape on ready primitives with no contract
dependency, and `run_to_scanline` and the breakpoint parcel both inherit it. That is why stepping leads
even though it has only a manual consumer.

### 9.2 The order

**Immediately, consuming no implementation capacity:**

- **CR-A — D-13, the breakpoint surface.** Longest lead time, two implementers, blocks the only
  automated consumer. Raise now. Carry §7.4 (our stop granularity is exact where the incumbent's is not)
  as supporting evidence, and §7.3's six questions as the agenda.
- **CR-B — D-10, `z80_write`'s missing width.** The only other hard block. Cheap to raise, and it
  unblocks half a parcel.
- **The aeon heads-up** — but see §6.3: send it as a date for the *three-method parcel*, once the
  blocking-transport question is answered, and fold `run_to_scanline`'s `maxFrames` into the same
  message.

**Wave 1 — run control. Server-only, ready primitives, sets the shape everything else inherits.**

1. **`step`, `step_over`, `step_out`.** Core ready; no contract dependency; establishes the stop surface.
   Raise the D-02 CR (§4.1) alongside — the silent-truncation hazard is real and the fragment has no key
   to report it.
2. **`run_to_scanline`.** Immediately after, reusing wave 1's sink shape. Carries the 262-vs-511 decision
   and the `maxFrames` conflict.

**Wave 2 — small writes. Independent of wave 1, closes read/write asymmetries.**

3. **`write_vram`** — with `Vdp::poke_vram`, not `vram_mut`.
4. **`z80_read`**, plus **`z80_write`'s `bytes` spelling only**, with `value` refused loudly pending
   CR-B. Flips `capabilities.z80`.

**Wave 3 — the aeon parcel. Gated on CR-A and on the blocking-transport answer.**

5. **`breakpoint_add`, `breakpoint_clear`, `breakpoint_list`, `wait_for_break` — together.** They ship
   together because their consumer calls them together. This is the parcel whose date is owed.
   `breakpoint_list` rides along because it shares the instrument, even though it has no consumer.

**Wave 4 — audio and video. Largest core work, zero consumers, so it is scheduled by cost not demand.**

6. **VGM (`vgm_start`, `vgm_stop`, `vgm_status`).** First of the audio group **because it needs no synth
   feature** (§4.7) and the core is closest to ready. Establishes the engine-owned-sink-threaded-into-
   both-drivers pattern that 7 and 8 reuse. Flips `capabilities.vgm`.
7. **Channel masks (`get_channel_states`, `set_channel_enabled`).** Pays the synth-feature decision once,
   and introduces the engine-owned `AudioSink` that 8 needs. Smaller than 8, so it goes first.
8. **`audio_spectrum`.** After 7, reusing 7's `AudioSink` ownership. Doing it first would introduce that
   sink and then rework it for the mask. Needs a hand-rolled FFT (the dependency sets must not grow), a
   bounded ring, and a declared magnitude unit.
9. **Layer masks (`get_layer_states`, `set_layer_enabled`).** Last of the substantive work: zero
   consumers, and the three renderer ambiguities in §4.10 need ruling before any code.

**Parked, deliberately:**

10. **`ping`** — its only consumer already guards on `hasMethod`, and it is the only one of the 21 with
    no MCP tool at all. Serve it whenever D-01 yields a value, or pick one and declare it. There is no
    cost to leaving it last.
11. **`log_clear`** — **do not serve alone.** The core has no log surface of any kind, and `log_tail` is
    not even schematized (D-29). Serving this method against no log is conformant and hollow. It should
    arrive with the log.

### 9.3 Which parcels are pure conformance and which need the contract to move

| | parcels |
|---|---|
| **Pure conformance — start today** | wave 1 (step\*), wave 2 (`write_vram`, `z80_read`, `z80_write` bytes-half), wave 4 items 6-9 |
| **Blocked on a CR before implementation** | breakpoints (CR-A, D-13); `z80_write`'s `value` spelling (CR-B, D-10); `ping`'s value (D-01, unless we pick and declare) |
| **Blocked on a decision that is ours to make** | `run_to_scanline`'s 262-vs-511; `write_vram`'s SAT-cache policy; the layer-mask ambiguities; `audio_spectrum`'s magnitude unit; whether Aether compiles the synth feature |
| **Blocked on an architectural answer, not a contract** | `wait_for_break`'s blocking transport |

Note the shape of that table: **only three of twenty-one methods are genuinely contract-blocked.** The
rest are ours to build. The contract is not the bottleneck here; the blocking-transport question and the
three absent core capabilities (FFT, channel mask, layer mask) are.

---

## 10. Open items

Each of these is left open deliberately, with its reason.

1. **Nothing here was confirmed at runtime.** Standing invariant: no emulator MCP tool was touched, so no
   claim in this document was checked against a running machine. Everything is source-derived.
   **TAGGED for foreground follow-up:** the `step` frame-budget truncation (§4.1) and the `write_vram`
   SAT-cache desync (§4.3) are both cheap to demonstrate live and both would strengthen their CRs.

2. **Nothing was compiled.** This task ran no cargo — another agent holds that lane. So: no build, no
   test run, no clippy. **Loudly unmeasured:** I have not verified that any classification here survives
   the type checker. In particular the claim that a counting sink slots into `Fanout` without touching
   `System` is a reading of the trait, not a compiled fact.

3. **The line-count estimates are engineering judgement, not measurement.** They are stated as ranges
   and labelled "judgement estimate" throughout. Do not treat them as data.

4. **The `wait_for_break` blocking-transport question is not answered here** and it is the single
   largest unknown in the survey (§6.3). It is a server-architecture question, it dominates the date owed
   to aeon, and it deserves its own investigation rather than a number in a table.

5. **I did not design the breakpoint surface** (§7), per the brief. Six questions are flagged.

6. **Two one-line-ish fixes found and deliberately not taken**, per the no-scope-creep rule:
   (a) `Engine::resolve_target` does not enforce the `addr`/`symbol` alternation the fragments' `oneOf`
   requires, which is a live unregistered request-side divergence on `run_to` today (§2.2);
   (b) `crates/oracle-aether/tests/schema_conformance.rs:6` and `:222-223` carry stale prose — "9 of the
   21 methods we advertise", "21 → 25" — that predates the 37/58 split and will mislead the next reader.
   Both are recommendations, not edits.

7. **The bare-method-name spelling in sigil's archival scripts** (§5.2) would be `-32601` on our server.
   Archival, so not urgent, but it is a second migration hazard beside the camelCase one and nobody has
   decided whether those scripts are ever meant to run again.

8. **The eight BLOCKED §6 rows are out of scope here** but touch this work: `log_tail` (D-29) gates
   `log_clear`; `call_stack` (D-28) overlaps `step_out`'s machinery; `z80_registers` (D-26) is the natural
   companion to `z80_read`/`z80_write`. Whoever picks up wave 2 or the parked `log_clear` should read
   those rows first.
