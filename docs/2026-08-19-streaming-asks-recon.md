# The three streaming asks — recon + design (2026-08-19)

Design for §9(c) of aeon's streaming-choke packet, transcribed in
`docs/2026-08-19-aeon-streaming-demand.md`. Anchors there; this document is the answer.

---

## 0. The verdicts, in one page

| Ask | Verdict | Sizing | Where it lands |
|---|---|---|---|
| **C1** — attribution correct under preemption | **ALREADY FIXED BY CONSTRUCTION.** Traced end to end in §2; our accumulator lacks the mechanism that produced the 20.6%, and it lacks it for four independent reasons. No contract change, no code change. What is missing is the **witness**, specced in §2.5 | test-only | its own commit, before slice 7 |
| **C2** — `callsTotal` as an exact integer | **REAL GAP ON OUR SIDE TOO** — our v1 reproduces the same unusable `2`. The number already exists undivided in the accumulator and is exposed in Rust; the wire drops it. Cheap, additive, mechanical | **delta** (refinement) | CR-26 **delta3**, before slice 4's merge window |
| **C3** — per-routine caller breakdown | **DO IT, as option (ii): an opt-in `callers[]` sub-array per row.** Their "nearly free" hint is mechanically correct and §4.1 says exactly why — but the wire and contract halves are not free | **CR-28** (behaviour change) | its own round, after the arc merges |

Ordering and the one hard deadline: §5. Nothing here blocks the corpus A/B.

**Ground truth read for this pass.** oracle-next `profiler-s1` at **`b46bbbc`** — and the drift the brief
warned about happened mid-pass: the branch advanced `4b1dd81` → `b46bbbc` ("the opt-in per-frame ring, and
D-S1's dead write made impossible", 743 → 818 lines) while this was being written. **The trace was
re-verified against `b46bbbc` and stands**: the diff is purely additive (the `FrameRow` ring plus the
removal of `charge()`'s dead refused-root branch), and it touches none of `push_frame`, `checkpoint`,
`pop_frame`, `close_routine`, `close_interrupt`, the boundary checkpoint loop, or the stack's
sample-lifetime property. Every `profiler.rs:NNN` below is `b46bbbc` and may drift again; the symbol names
will not.
empyrean `profiler-amendment` at **`6d5cb4b`**, still the tip, still unmerged. The reference at
`/home/volence/sonic_hacks/oracle`, read firsthand rather than carried from our recon. aeon at
`3469c920`.

---

## 1. What C1 is actually asking, restated

Their ask has two halves and they are not the same size (`CHOKE-DIAGNOSIS.md:596-599`):

1. *"define and document what the profiler does when an interrupt preempts a profiled routine"* — a
   documentation ask.
2. *"make `cycles` / `cyclesSelf` exact under preemption — credit the preempted routine's pre- and
   post-interrupt segments to it, and the handler to the handler"* — a correctness ask.

Half (2) is a **verbatim description of our design.** The interesting question is therefore not "can we
build this" but "is the hypothesis that we already have it *true*, mechanically, or is it a comfortable
assumption". §2 is that check, done by tracing both instruments rather than by reading either's prose.

---

## 2. C1 — the trace

### 2.1 The old instrument's loss mechanism, in its own source

Read firsthand at `/home/volence/sonic_hacks/oracle/linux-port/gui/ControlSocket.cpp`,
`OpGetProfilerFrames` beginning `:1944`. Three facts about it decide everything:

- **The shadow stack is rebuilt from an event ring at query time, per frame, and it is declared *inside*
  the per-frame loop** — `for (int fi = 0; fi < numFrames; ++fi)` at `:1966`, `std::vector<StackEntry>
  stack;` at `:1972`. Every frame therefore starts with an empty stack that has never seen an earlier
  frame's `SubroutineEnter`.
- **Cycles are charged only at an exit event**, never continuously: `dur = ev.cycle - top.startCycle`
  (`:1989`) into `routineMap[top.address].cycles` (`:1990`) — and only if the guard `if (!stack.empty())`
  at `:1986` passes.
- **`totalCycles` is accumulated unconditionally**, outside all of that: `totalCycles +=
  snap.totalCycles()` at `:1969`.

Those three compose into the defect. Naming the parts, because they are separable and only the first is
the loss:

**L1 — the orphaned resumption. This is the 20.6%.** A routine entered in frame *N* and returning in
frame *N+1* has its `SubroutineExit` arrive against a stack that never saw its entry. If that stack is
empty at the moment the exit arrives, the `:1986` guard **charges nothing at all** — the routine's entire
post-boundary segment reaches no row. It nevertheless sits inside `snap.totalCycles()`, counted at
`:1969`. The gap between `total_cycles` and the summed rows *is* those cycles. This is why the loss
appears only when a tick spans a VBlank, and why it scales with how much of the tick lands after the
boundary: at 2.067 frames/tick roughly half of every tick is post-boundary work, and 20.6% of the frame
is what fell out.

**L2 — the displaced pop. This is the *signature* they noticed, and it is a different bug.** If the stack
is *not* empty when that stray exit arrives — because the resumed routine called a child first — then the
exit pops **the child**, and charges the child a span that began at the child's entry but ends at the
*parent's* return. Every subsequent exit in that frame is then off by one frame of stack depth.
Attribution walks downward: spans that belong to a parent land on its children. That is precisely the
impossibility aeon reported — `GameState_OJZScroll_Update` (the boundary-spanning parent) smaller than
`Tile_Cache_Fill` + `Parallax_Update` (children that enter and exit inside one frame) — and throwaway B's
**negative** own cost is the same mechanism one level deeper.

**L3 — the fabricated call.** The end-of-frame flush at `:2007-2022` charges each still-open frame
`snap.endCycle - top.startCycle` **and does `calls++`**, so one invocation open across *k* boundaries
books *k+1* calls. Then `:2042` divides by the requested window and `:2043` floors *up* to 1. L3 is not
part of the cycle loss; it is C2's half of the same code, and it is why their `calls` cannot be trusted
even before division.

**Why the healthy states close to ±1–2%.** At 1.000 frames/tick nothing spans the boundary, so L1 and L2
never fire; the residue is aeon's own explanation at `CHOKE-DIAGNOSIS.md:422-423` — the HInt trampoline
is double-counted because it fires inside whichever routine was executing. Our design does not have that
one either (the bucket is a typed frame, not a nested subroutine — the W5 answer), but it is not what C1
is about.

> This is W4 in our recon's wart list (`docs/2026-08-19-profiler-recon.md:195`, mechanism at `:82-84`),
> written before the packet existed and now measured by it. The recon called it "straddling calls
> double-counted"; the measurement shows the cycle effect is **loss**, not double-count — L3 is the
> double-count and it lands on `calls`, not on cycles. Worth the correction: our own doc undersold which
> quantity the defect destroys.

### 2.2 Our accumulator's preemption story, traced

`crates/oracle-core/src/profiler.rs` at `profiler-s1` `b46bbbc`. Routine **R** is executing; a VInt
preempts it; R resumes; R completes. Step by step, through the real code paths:

1. **R is the innermost frame.** Every retired step calls `charge()` (`:433`), which adds that step's
   cycles to `self.stack.last_mut()` — R — and to `pending_cycles`. Attribution is **continuous per
   retired step**, not deferred to an exit event. This single difference is the whole of L1's absence:
   there is no "charge at exit" path that can be missed, because there is no charge at exit.

2. **The interrupt is taken.** `on_event` latches the fc = 7 acknowledge; the same step's
   `on_step_retire` (`:680`) pushes a `FrameKind::Interrupt { level, frame_ssp }` frame on top of R and
   arms a call so the handler's entry address gets its own routine frame nested inside the bucket. R is
   now *beneath* two frames.

3. **Charge follows the innermost frame, so it goes to the bucket and the handler — not to R.** R accrues
   nothing while preempted, which is correct: R is not executing. This is the "handler to the handler"
   half of their ask, and it is structural rather than a rule applied.

4. **`RTE` closes the bucket.** `close_interrupt()` (`:576`) matches on the **supervisor** stack
   pointer, unwinds the handler routine frame (which folds its inclusive into the bucket's child time)
   and then the bucket itself. `pop_frame()` (`:501`) then does **not** fold the bucket into R:
   `if matches!(frame.kind, FrameKind::Routine { .. })` guards the parent credit, and `:532` says so out
   loud — *"Deliberately no parent.child_cycles for an Interrupt frame."* R's inclusive figure is
   therefore independent of interrupt load, which is the property a consumer gating with `==` needs and
   cannot otherwise verify.

5. **R resumes and accrues again.** R is innermost once more; `charge()` resumes adding to it. R's
   `entry_sp` was never touched, so its eventual `RTS` still matches exactly.

6. **Every frame boundary R spans is a checkpoint, not a teardown.** `on_frame_boundary` (`:756`) walks
   `for idx in 0..self.stack.len()` (`:777`) and calls `checkpoint(idx)` (`:482`) on **every live frame**,
   moving each frame's accrual-since-last-checkpoint into its row and marking it reported. The stack
   itself is untouched — the field doc at `:306-308` is explicit: *"Deliberately **not** reset at a frame
   boundary: a call that straddles a boundary is one call, not two."*

7. **R completes.** `close_routine` → `pop_frame` → `checkpoint` writes only the **unreported remainder**
   (`Frame::unreported()` / `mark_reported()`), then `calls += 1` — once, for one invocation.

**Result: R's row carries all of R's own cycles, whole, regardless of how many VBlanks interrupted it and
how many frame boundaries its single invocation spanned.** Nothing is lost, nothing is double-counted,
and `calls` is 1.

### 2.3 Why our design cannot have the defect — four independent reasons

Any one of these alone would prevent L1. We have four, which is worth stating because it means the fix
is not resting on a single line someone could refactor away:

1. **Continuous charge.** Cycles are attributed at every retired step (`charge()`, `:433`), not at exit
   events. The old instrument's loss is a *missed charge*; we have no charge to miss.
2. **A sample-lifetime stack.** The stack is never cleared at a boundary (`:306-308`). The old
   instrument's stack is per-frame by construction (`:1972`), which is what orphans the resumption.
3. **Boundary checkpoints.** A frame's accrual reaches its row while it is still running (`:777` →
   `:482`), so a row is honest even for an invocation that never completes. Nothing waits on a completion.
4. **The reconciliation identity.** Σ `routines.self_cycles` + Σ `interrupts.self_cycles` +
   `unattributed_cycles` == `sample_cycles`, exactly, with `unattributed_cycles` as the single named
   escape hatch. A loss of L1's kind is **not expressible**: the cycles are in `pending_cycles` because
   `charge()` put them there, and the only way out of a frame's accrual is into a row.

> Reason 4 got **stronger** during this pass, which is worth recording. `b46bbbc` applied ruling D-S1 by
> deleting `charge()`'s refused-root fallback — a dead second write to `pending_unattributed` — and
> replacing it with an `expect` stating the impossibility (*"a root push is never refused: an empty stack
> is below `MAX_DEPTH`"*). The escape hatch now has provably **one** source rather than one live source
> and one dead one, so "a loss of L1's kind is not expressible" is a claim about the code's shape and no
> longer only about its current behaviour.

And L2 cannot occur for a fifth reason: `close_routine()` matches on `entry_sp` **exactly**, plus
privilege mode, searching innermost-first through the whole stack. A return that matches nothing closes
nothing. There is no path by which a return pops a stranger and silently re-parents a span.

### 2.4 Three things we do *not* claim

Being precise here matters more than being reassuring, because these are what a careful reader of the
A/B will ask.

- **Inclusive `cycles` lags in its per-frame *distribution*, though never in total.** The module doc says
  so (`profiler.rs:85-88`): a parent's boundary checkpoint cannot see time still in flight beneath it, and
  catches up when the callee pops. Over the committed sample the total is exact; within a `perFrame[]`
  series a callee's inclusive contribution can land in the frame it *popped* in. `cyclesSelf` has no such
  lag, which is why the identity is stated on self. **This does not touch C1** — it is a property of
  callees, not of preemption, and it is a distribution artefact, not a loss.
- **Our inclusive `cycles` deliberately excludes preemption.** A routine's row does not grow when a VBlank
  interrupts it. That is what their ask asks for, and it means our `cycles` will differ from old oracle's
  in *both* directions on a boundary-spanning parent: lower, because we do not fold the interrupt in;
  higher, because we do not drop the post-boundary segment. Two mechanisms, opposite signs, so an A/B row
  that happens to agree is not evidence of anything. **The A/B must compare `cyclesSelf` and the identity,
  not the inclusive figure.**
- **The `preemptedCycles` / `resumedSegments` fallback they offer is moot, and should be declined
  explicitly.** They scoped it *"if an exact split is not free"* (`:599-601`). It is free. Adding a
  diagnostic that measures a loss we do not have would be a field whose honest value is always zero — and
  §2.3 of the contract's own rule (absence and zero must not both mean nothing happened) is about fields
  that *can* be non-zero. Declining it is the right answer and the demand-side reply should say why, not
  just say no.

### 2.5 The witness fixture — spec for the implementing agent

C1 needs no code change. It needs a test that would have caught the old instrument, so that "fixed by
construction" is a gate rather than a claim in a document. Tests-first, per the house rule: **proven red
first, wired into a runner, expectation derived from source rather than from a measurement, loud on
unmeasurable.**

**Home.** `crates/oracle-core/tests/profiler.rs`, beside the W3/W4/W8 regressions from slice 3 — this
joins that family. Fixtures built with `crates/oracle-core/src/testrom.rs` so every expectation comes from
a constant the builder used.

**The fixture.** One ROM, run twice.

```
main:   jsr R                      ; exactly one invocation
        bra  main                  ; (or spin; R's row is what is asserted)

R:      jsr  Ca                    ; child, completes inside a frame
        <delay loop, sized to span >= 2 frame boundaries>
        jsr  Cb                    ; child, completes inside a frame
        rts

H:      <bounded work>             ; the VBlank handler
        rte
```

Run **A**: VInt enabled. Run **B**: VInt masked. Identical ROM, identical run length, identical arming.

**Sizing constraint, load-bearing.** The delay must be long enough that (a) R's single invocation spans at
least two frame boundaries, so the checkpoint fold runs at least twice mid-invocation, and (b) at least
one VInt fires *inside R's body*, between the two child calls. Derive both from the testrom builder's own
cycle constants, not from a trial run.

> ⚠ **F-TESTROM-DISP-GUARD applies directly here** (`docs/2026-08-19-scanline-readback.md:241-246`). The
> builder truncates branch displacements without a guard, so a loop body passing ±127 bytes assembles a
> *different valid branch* rather than failing. A long delay loop is exactly that shape. Either land the
> guard first or keep the loop body short and iterate a counter — and **say which in the commit message**.

**Assertions.** All on the *undivided* sample via `Profiler::sample_routines()` / `sample_interrupts()`,
which sidesteps the division entirely and keeps this a test of attribution rather than of arithmetic.

1. **Parent ≥ Σ children** — `R.cycles >= Ca.cycles + Cb.cycles`. The direct negation of aeon's
   impossibility signature. Weak on its own; included because it is the assertion whose failure a reader
   of the packet will recognise instantly.
2. **The exact inclusive relation** — `R.cycles == R.self_cycles + Ca.cycles + Cb.cycles`. This is the
   real gate. It is an **equality**, not an inequality, precisely because of the non-folding ruling:
   preemption is in neither term, so there is no interrupt cost to make room for.
3. **The identity closes** — Σ `self_cycles` over rows and buckets + `unattributed_cycles` ==
   `sample_cycles`, exactly, in run A. (Slice 3 asserts this across eight fixture shapes already; asserting
   it *here* is what ties the identity to the preemption case specifically.)
4. **One invocation, not one per boundary** — `R.calls == 1` in both runs. This is the L3 regression.
5. **★ The money assertion — R's own cycles do not depend on interrupt load.**
   `A.routines[R].self_cycles == B.routines[R].self_cycles`, exactly, and
   `A.interrupts[6].calls >= 1` while `B.interrupts[6].calls == 0`.
   The second pair is the **liveness control** without which the first is vacuous, and the house has been
   bitten by a vacuous control before. The first pair is C1 in one line: it is exactly the property old
   oracle violated by 20.6%, expressed as an equality between two runs that differ only in whether the
   preemption happened.

**Mutations. The test is not done until each of these turns it red.**

| # | Mutation | Reproduces | Must break |
|---|---|---|---|
| M1 | Insert `self.stack.clear();` in the `else` branch of `on_frame_boundary` | `ControlSocket.cpp:1972` — the per-frame stack, i.e. frame-window attribution | 2, 4, 5. If 5 does not break, the delay is too short to span a boundary and the whole fixture is vacuous — **check this one first** |
| M2 | Delete the `if matches!(frame.kind, FrameKind::Routine { .. })` guard in `pop_frame` so an interrupt folds into its parent | the W5 conflation | 2 only. 5 stays green, since `self_cycles` is untouched — the discrimination between M1 and M2 is itself worth asserting |
| M3 | In `checkpoint`, write the frame's **full** accrual instead of `unreported()` | double-counting across boundaries | 2 and 3 (the identity over-closes) |

M1 is the one that matters: it is a faithful reproduction of the defect aeon measured, applied to our
accumulator, and the test must detect it.

---

## 3. C2 — the cheap delta

### 3.1 It is a real gap on our side too, and that needs saying first

The tempting reading is "old oracle divided badly; ours divides honestly; done". It is wrong. Our
`report()` does `calls: div(c.calls)` (`profiler.rs:628` for rows, `:643` for buckets) — plain integer
division by `frameCount`. For aeon's 4.53-invocations-per-tick routine over a 31-frame sample at 2.067
frames/tick, that is roughly 68 undivided calls ÷ 31 = **`2`**. The same unusable number, arrived at
honestly.

We are better than the reference in one respect only, and it is not the one that helps them: we never
floor **up**, so `TileCache_DecompressBlock` reports `0` where theirs fabricates `1`. Honest, still
unusable. **C2 is a genuine gap in our v1, not a defect we already avoided.**

What makes it cheap is that the exact number already exists. `Counts` is documented at `profiler.rs:134-136`
as *"One accumulator's worth of counters. Every field is a raw, undivided sample total; the division into
per-frame figures happens once, in `Profiler::report`."* And `sample_routines()` (`:346`) already exposes
it in Rust, for exactly this reason — its own doc says a caller *"checking how many rather than how many
per frame needs this"*. **The accumulator has it; the wire drops it.** C2 is publication, not computation.

### 3.2 The pre-release argument — re-checked, and it still holds

D-M3 scoped the argument to something factual and operational
(`docs/2026-08-19-ruling-cr26.md:283-294`; contract text at `contract/protocol.md:3020-3032`, headline at
`:3023`): *"no server speaking this bus implements these three methods, and no client has ever received a
reply any fragment governed"*, expiring *"the day after the first server ships one"* (`:3030-3031`).

Checked against the tree as it stands, not against the ruling's snapshot:

| Condition | State at `profiler-s1` `b46bbbc` | Verdict |
|---|---|---|
| A handler for any of the three methods on any branch | **None.** `crates/oracle-aether/src` contains no `get_profiler` / `set_profiler` / `get_profiler_frames` handler | HOLDS |
| The `initialize` capability | Still `"profiler": false` — `crates/oracle-aether/src/engine.rs:834` | HOLDS |
| What the arc has touched since the ruling | `4b1dd81` — four **test** files only (`tests/common/schema.rs`, `tests/contract/PROVENANCE.md`, the vendored schema, `tests/schema_conformance.rs`); `b46bbbc` — `oracle-core` only (the per-frame ring). Nothing that serves a reply | HOLDS |
| Upstream fragment revision | `profiler-amendment` tip is still `6d5cb4b`; nothing newer to reconcile against | HOLDS |

**So the window is open, and slice 4 is what closes it.** The moment slice 4 flips `engine.rs:834` to
`"profiler": true` and a handler serves a reply, the predicate fails by its own operational terms and
`callsTotal` becomes a breaking change. That is the deadline in §5, and it is a hard one.

**One drafting obligation that comes with riding the argument.** The paragraph is headed *"Why **three**
REQUIRED additions are safe here"* and D-M1 already made it four without renumbering the heading. Delta3
must recount rather than inherit a stale number: `callsTotal` on the routine row is a fifth addition and
on the interrupt bucket a sixth, on delta2's own counting convention (where `cyclesSelf`-on-bucket was
"a fourth"). The drafter may prefer "a fifth, applied to both row shapes" — either is defensible; leaving
it at three is not, and this is exactly the class of staleness the house catches at adjudication.

### 3.3 The designed shape

**`callsTotal`: an undivided integer count of completed invocations over the whole sample, REQUIRED,
carried on the routine row and on both interrupt buckets, alongside the existing divided `calls`.**

Five properties, each with its reason:

- **Undivided.** That is the entire point; a divided count is what broke.
- **REQUIRED, not optional.** Under §2.3's rule (absence and zero must not both mean "nothing happened")
  and under the pre-release argument, which is precisely what makes required-now possible. An optional
  field would force every consumer to branch, forever, on whether the server bothered.
- **Alongside `calls`, not replacing it.** `calls` is the migration-parity field; the demand doc's whole
  migration story (`protocol.md:2962`) rests on a consumer's existing arithmetic not changing.
- **On buckets too.** Symmetry with `cyclesSelf`, which delta2 put on the bucket for the same reason: a
  figure a client sums or gates must be keyed identically wherever it appears. An exact count of VBlanks
  *taken* over a sample is independently useful, and it is the natural cross-check on `frameCount`.
- **NOT on `perFrame[]` rows.** Those carry no per-routine breakdown and are already undivided — there is
  nothing to un-divide. Saying so in the delta pre-empts the obvious reviewer question, and §3.5 case 10
  turns it into a negative control.

**Two prose-only invariants** (JSON Schema cannot express either; they join the existing prose list in the
`get_profiler_frames` `$comment`, beside "stallCycles <= cycles on every row"):

- `calls == callsTotal / frameCount` under integer division, on every row and both buckets, **when
  `frameCount > 0`**. The scoping matters: with `frameCount` 0 the relation is undefined, and our
  `report()` returns no rows at all in that case, so nothing is being papered over.
- Equivalently, and more useful to a client: `calls * frameCount <= callsTotal < (calls + 1) * frameCount`.
  This makes `callsTotal` a **bound on the truncation** rather than merely a second number.

**No new `initialize.limits` key. No new fragment.** The count stays at **36**, before and after — the
same precision note delta2 had to make.

### 3.4 Draft delta text for the CR-26 delta3 round

Drafted for the delta drafter, who owns the empyrean commit. **Nothing in this section was committed to
`empyrean`.**

**(a) §6 catalog row** (`protocol.md:1312`) — two field lists gain one name each:

> `routines` (§2.4 container of `{addr,name?,disp?,cycles,cyclesSelf,stallCycles,calls,callsTotal}`),
> `interrupts{hint,vint}{cycles,cyclesSelf,stallCycles,calls,callsTotal}`

**(b) A new normative bullet in §6's profiler blockquote**, to sit immediately after the "Division happens
inside the server" bullet:

> - **One count is published undivided.** Every other figure in this reply is a per-frame figure, and for
>   cycles that is the useful form. For a **count** it frequently is not: a routine invoked 4.53 times per
>   frame reports `calls: 2`, and one invoked once across a 31-frame sample reports `calls: 0`. Both are
>   correct — a count is never floored up — and neither can be equality-gated against a fire count derived
>   from engine source, which is the consuming pattern this bus exists to serve. **`callsTotal` is the
>   exact number of completed invocations over the whole sample, undivided**, keyed identically to `calls`
>   and REQUIRED on every routine row and both interrupt buckets. It is exact **regardless of
>   `perFrameExact`**, which is the point of it. It is not a second measurement: when `frameCount > 0`,
>   `calls == callsTotal / frameCount` under integer division, so `callsTotal` also **bounds** the
>   truncation in `calls` — `calls × frameCount ≤ callsTotal < (calls + 1) × frameCount`. It is not
>   carried on `perFrame[]` rows, which hold no per-routine breakdown and are already undivided.

**(c) The fragment delta** — one property added to `routines.items.properties` and the identical property
added to `$defs.interruptBucket.properties`, with `"callsTotal"` appended to both `required` arrays:

```json
"callsTotal": {
  "type": "integer",
  "minimum": 0,
  "description": "Added 2026-08-19 by §11.16's third delta. The UNDIVIDED count of completed invocations over the whole sample — `calls` BEFORE the division, not a second measurement of it: when frameCount > 0, calls == callsTotal / frameCount under integer division, so this figure also bounds the truncation in `calls` (calls * frameCount <= callsTotal < (calls + 1) * frameCount). REQUIRED, because a per-frame count is the one figure here that division routinely DESTROYS rather than merely truncates: a routine invoked 4.53 times per frame reports calls 2, and one invoked once across the sample reports calls 0 — both correct, since a count is never floored up, and neither equality-gatable against a fire count derived from engine source. Exact regardless of perFrameExact. Counts COMPLETED invocations on the same rule `calls` uses, so a routine still running at the close of the sample carries its cycles with callsTotal 0, and abandonedFrames is where that understatement is reported."
}
```

**(d) The `$comment` prose list** on `get_profiler_frames` gains one clause, beside the existing
`stallCycles <= cycles` and `sampleCycles == frameCount * totalCycles` clauses:

> when `frameCount` > 0, `calls == callsTotal / frameCount` under integer division on every routine row
> and both interrupt buckets

**(e) The pre-release paragraph** (`protocol.md:3020`) is recounted per §3.2, and its scoping sentence is
re-verified rather than restated — the delta should carry the check, not the assumption: no handler on any
branch, `"profiler": false` at `oracle-next` `crates/oracle-aether/src/engine.rs:834`, upstream at
`6d5cb4b`. **With the SHA of the branch it was checked on**, because that check has a shelf life measured
in commits.

**(f) The §11.16 precision note** repeats delta2's: the fragment count **does not move** — 36 before, 36
after, no other fragment touched, no new `initialize.limits` key.

### 3.5 Harness cases for the delta3 round

The delta2 round ran **34 for 34**. These are the classes delta3 earns; the numbering continues from
there. Cases 1–6 are refusals, 7–9 acceptances, 10 the negative control.

| # | Reply shape | Expected | What it proves |
|---|---|---|---|
| 1 | routine row missing `callsTotal` | **REFUSED** | the wanted pre-release behaviour — an old-shape reply stops conforming, exactly as delta2's three did |
| 2 | `hint` bucket missing `callsTotal` | **REFUSED** | the bucket half was actually added, not just described |
| 3 | `vint` bucket missing `callsTotal` | **REFUSED** | both buckets share the `$defs` shape — this catches a one-sided edit |
| 4 | `callsTotal: -1` on a row | refused | `minimum: 0` |
| 5 | `callsTotal: 2.5` on a row | refused | `type: integer` — a divided average leaking into the field |
| 6 | `totalCalls` on a row, and again in a bucket | refused ×2 | `additionalProperties: false` on both shapes; the delta2 `selfCycles` precedent |
| 7 | `frameCount: 31`, `calls: 2`, `callsTotal: 140` | accepted | **the case the ask exists for** — 4.51/frame reporting as `2`, with the exact figure recoverable |
| 8 | `frameCount: 31`, `calls: 0`, `callsTotal: 1` | accepted | `TileCache_DecompressBlock`'s shape: invoked once in the sample, `calls` honestly 0, `callsTotal` exact |
| 9 | `calls: 0`, `callsTotal: 0`, non-zero `cycles`, beside a normal row | accepted | the still-running main-loop row — cycles without a completed invocation stays legal |
| 10 | a `perFrame[]` item carrying `callsTotal` | **REFUSED** | the negative control: the field went into exactly two shapes, not three, and that shape's `additionalProperties: false` enforces it |

Case 10 is the one worth insisting on. Every other case checks that the field arrived; only 10 checks that
it did not arrive somewhere it does not belong, and a field that quietly spreads is how a shape stops
meaning one thing.

### 3.6 The wider question, registered rather than folded in

**F-PROFILER-EXACT-TOTALS.** §11.16 states plainly that `perFrameExact` *"will read **`false`** in nearly
every real sample, which is the text working as written rather than a defect"* (`protocol.md:3035-3036`).
Follow that through: when it is false, every divided figure is floored by up to `frameCount − 1`, and
`sampleCycles == frameCount × totalCycles` no longer holds — so **a client cannot recover any row's exact
cycle total by multiplication.** After delta3, the reply carries an exact undivided total for the machine
(`sampleCycles`) and an exact undivided count per row (`callsTotal`), and **no exact per-row cycle figure
at all**.

Aeon's consuming model is `==` against source-derived constants (`docs/2026-08-19-aeon-profiler-demand.md`
§1.5). Their `==` checks on **counts** fail loudly — 4.53 becomes 2, which is why C2 exists. Their `==`
checks on **cycles** would fail **quietly**: ≤ 30 cycles missing from ~51,357 looks exactly like
agreement. That asymmetry is the reason C2 got noticed and this did not.

The symmetric fix is `cyclesTotal` / `cyclesSelfTotal` / `stallCyclesTotal` on rows and buckets, and the
pre-release argument covers it identically — same window, same reasoning, no extra fragment.

**Recommendation: register it, do not fold it into delta3.** Three reasons. It is not what was asked. It
changes the division story rather than refining it, so it deserves its own adjudication instead of riding
one. And the corpus A/B is the place to establish whether it bites in practice — if a real sample's rows
close under `==` anyway, the question answers itself. But it is named **here** rather than after the fact,
because the pre-release window shuts at the same moment C2's does, and the controller should get the
choice while it is still cheap rather than discover it once `"profiler": true` has shipped.

---

## 4. C3 — the caller breakdown

### 4.1 "Your shadow-stack design may get this nearly for free" — verified, with the precise statement

Mechanically correct, and the reason is more interesting than the conclusion.

**The caller is available at the push, for free.** `on_step_retire` (`profiler.rs:680`) step 1 pushes the
callee's frame when the *previous* retirement armed a call. At that instant `self.stack.last()` is the
caller's frame, and its `kind` — `Routine { addr }` or `Interrupt { level }` — is the caller key. One
read, `O(1)`, no search, no bookkeeping. Store it as a field on `Frame`.

**And here is the part that makes it cheap rather than merely possible.** Our attribution is
**per-invocation folded**, not per-step keyed. Cycles land on the innermost frame via `charge()` (`:433`)
and only reach a *row* at `checkpoint()` (`:482`) — which runs once per live frame per boundary, plus once
per pop. So per-caller accounting costs:

| Path | Frequency | Added cost |
|---|---|---|
| `charge()` — the hot path | **every retired step** | **nothing.** Untouched |
| `push_frame()` | once per `JSR`/`BSR`/interrupt entry | one `self.stack.last()` read, one word stored on `Frame` |
| `checkpoint()` / `pop_frame()` | once per live frame per boundary, plus once per pop | one additional `BTreeMap` entry-and-add — roughly doubling the map work that path already does |
| memory | — | one `BTreeMap<(u32, CallerKey), Counts>`, bounded by distinct **edges** of the observed call graph (typically 1–3× the node count), plus 4–8 bytes per live frame |

**The mechanical verification, stated as a claim someone could refute:** the per-caller cost is
proportional to **calls**, not to **instructions**, and it is so *because* cycles are folded at
invocation boundaries rather than keyed per step. A sampling profiler, or any design that keyed cycles by
PC on every step, would need the caller at every step — which is the expensive version of this feature.
Ours does not. That is the whole content of their "nearly free", and it is true.

**Should it still be armed?** Yes — `set_profiler{callers: true}`, on the `perFrame` precedent. Not
because the accumulator cost is scary (it is not), but because the *reply* grows by rows × N sub-objects
and the contract's own pattern is that a second lens is opt-in. Unarmed, the second map is never
allocated and the `checkpoint` path takes its current single map op.

**Three edges the design must name, none of them expensive:**

- **The caller can be an interrupt bucket, not an address.** A handler's routine frame is pushed with the
  bucket beneath it (`on_step_retire` step 2 arms the call from the acknowledge). Either key it as a
  distinct caller kind, or omit `callerAddr` on that entry and let its absence mean "entered from an
  interrupt". The latter fits §2.3's presence discipline better and needs no new enum on the wire.
- **The caller can be the inferred root frame.** `charge()`'s empty-stack root push keys a frame by
  *whatever PC retired first*, which is mid-routine, not an entry (`:426-431` documents this honestly). A
  `callerAddr` pointing there is real but is not an entry point, and the wire description must say so
  rather than let a client assume every `callerAddr` resolves to a bare label at `disp: 0`.
- **The caller can be absent.** Depth-cap refusal, and the outermost frame. Absence is the honest answer;
  a fabricated `0x0` is not.

### 4.2 The three wire shapes, priced

**(i) Rows keyed by `(addr, caller)`.** — **REJECT.**

| | |
|---|---|
| Accumulator | cheapest of the three (one map, re-keyed) |
| Wire | no new field |
| **Contract** | **breaking, and in the one way D5 does not cover.** §6 pins *"rows are keyed by entry address, never by symbol"*, and the migration story rests on a consumer's addr-keyed lookup continuing to work (`protocol.md:2962`). A consumer doing `rows[addr]` would suddenly get N rows per address, silently, with every one of them smaller than the figure it used to read |

Cheapest to build, most expensive to own. Their own ask is *"per routine row"*, not "instead of".

**(ii) An optional `callers[]` sub-array per row.** — **RECOMMEND.**

| | |
|---|---|
| Accumulator | §4.1's table; the second map, armed |
| Wire | one new optional field on the existing row shape; one new `set_profiler` param; one new `initialize.limits` key (`maxProfilerCallers`) |
| Contract | **additive under D5**: a client that ignores the field is unaffected, and a server that was never armed for it emits nothing. Opt-in follows the `perFrame` precedent exactly, down to the refusal semantics (`get_profiler_frames{...}` refusing `-32005` `callersNotArmed` mirrors `perFrameNotArmed`) |

Shape, matching their ask plus the C2 lesson (`{addr, calls, cycles}` was their minimum; an exact count is
worth having here too, and `cyclesSelf` is the figure that actually attributes):

```
callers[]: { callerAddr?, callerName?, callerDisp?, cycles, cyclesSelf, calls, callsTotal }
```

`callerAddr` optional per §4.1's three edges. Ordered by `cycles` descending, so a truncated list is the
expensive end — the same rule `routines` already carries.

> **One open question for the CR to settle, flagged rather than assumed.** `routines` is already §2.4's
> *nested* container spelling (a list that is a field of a larger result). Making each row's `callers` a
> full §2.4 container puts a container inside an item of a container — a third level, with
> `total`/`returned`/`truncated` repeated per row. That is a lot of scaffolding for a top-N list.
> **Recommendation: a plain array plus a per-row `callersTruncated` boolean**, bounded by the advertised
> `maxProfilerCallers`, with the reasoning stated in the CR so the adjudicator rules on it deliberately.
> The §2.4 adjudicator may well rule the other way; what must not happen is the question going unasked.

**(iii) A separate method — `emulator/get_profiler_callers{addr, top?}`.** — **fallback.**

| | |
|---|---|
| Accumulator | identical to (ii) — same map, same arming |
| Wire | a **fourth fragment** (36 → 37), a new §6 method row, a second round trip |
| Contract | leaves the row shape completely untouched (zero migration risk), and confines reply growth to the one routine asked about — which *is* aeon's actual pattern: they care about `TileCache_FindStagedBlock`, not about all 200 rows |

Genuinely attractive, and it loses on one thing: **cross-read skew.** Both methods are pure reads that
clear nothing, and the sample keeps accumulating at every frame boundary, so two calls a frame apart
describe different samples. A client correlating a `callers[]` breakdown against a row from a *different*
read is comparing figures with different divisors — silently. That is mitigable (carry `frameCount` in the
caller reply and pin the same "MUST agree when no frames were run between the calls" rule `get_profiler`
already uses for `framesRecorded`), but it is a hazard (ii) does not have at all, because (ii) is one
atomic snapshot.

### 4.3 Recommendation and sizing

**Take (ii).** It is what they asked for (*"`callers: [...]` per routine row"*), it is one coherent
snapshot with no cross-read skew, and the reply-size objection is answered by the mechanism that already
answers it for rows: the field is opt-in, and `top` already bounds how many rows come back, so the client
controls `top × maxProfilerCallers` end to end. Keep (iii) on the record as the fallback if either the
reply-size or the nested-container objection carries at adjudication — the accumulator work is identical
either way, so the choice can be made late without wasting anything.

**Sizing: CR-28, not a delta.** The house line is behaviour-change versus refinement, and C3 lands on the
wrong side of it three times over:

- a new `set_profiler` param changes **what the instrument does**, not merely what it prints;
- a new `initialize.limits` key changes what a server **advertises about itself**;
- a new nested shape with its own ordering and truncation rules is new **surface**, not a new column.

Contrast C2, which publishes a number the accumulator already computes, into shapes that already exist,
under no new param, with no new server behaviour — a refinement, and delta-sized. Stating the contrast is
worth more than stating either verdict alone, because it is the line itself that will get cited next time.

**The D5-additive story for the CR:** every part of C3 is additive in the direction D5 covers. A client
that never sends `callers: true` sees a byte-identical reply. A client that ignores the field is
unaffected. The one non-additive direction — a *required* addition, which is what the pre-release argument
exists to license — **is not used at all here**: `callers[]` is optional-by-arming, so C3 does **not**
depend on the pre-release window being open and can therefore land safely after slice 4 ships. That is not
a footnote; it is the reason C3 can be sequenced last without risk, and C2 cannot.

---

## 5. Ordering

**1. C2 → delta3, now, before slice 4's merge window. This is the only hard deadline in the round.**

The binding constraint is the house pattern slice 4's own commit states — *"the schema lands before the
handler, so the wire tests in the next commit validate against the real fragments rather than against what
this server happens to emit"* (`4b1dd81`). Slice 4's wire tests are the next thing to be written.
Sequence:

> delta3 on empyrean `profiler-amendment` → oracle-next re-vendors (schema bytes + `PROVENANCE.md` +
> `TRACKED_REVISION`) → slice 4 implements `callsTotal` in the handler and its wire tests in the same pass.

That costs **one** extra re-vendor commit, which the arc was taking anyway. Landing it after slice 4's
tests are written costs a second full round trip and a re-green of tests that had just gone green.

⚠ **And the window has a hard edge, not a soft one.** The pre-release predicate expires operationally:
*"the day after the first server ships one"* (`protocol.md:3030-3031`). Slice 4 flipping
`crates/oracle-aether/src/engine.rs:834` to `"profiler": true` **is** that day. After it, `callsTotal` is
a breaking change and the whole argument in §3.2 has to be replaced with a migration. C2 must be in
before the flip.

**2. C1 → the witness fixture, its own commit, before slice 7.**

No contract surface, no wire, no currency, no competition for the delta window — it can land any time.
Land it before the corpus A/B, because the A/B's most interesting rows are precisely the lagging
max-diagonal ones aeon could only publish as *indicative*, and the witness is what licenses us to publish
ours as exact. Riding slice 5 is acceptable (slice 5 is MCP verification; the fixture is core-level), but
the cleaner home is its own commit adjacent to slice 3's test file, which already holds the W3/W4/W8
regressions it joins.

**3. C3 → its own round, CR-28, after the arc merges.**

Raising a CR-sized adjudication inside the delta window would stall C2 behind it, for no gain: per §4.3,
C3 uses no required addition and therefore does not need the pre-release window at all. Its accumulator
work is small and additive, so nothing is lost by going last.

**4. The corpus A/B stays unblocked throughout.**

It waits on aeon's Phase-0 merge SHA (`docs/2026-08-19-profiler-recon.md:986`, still PENDING), not on any
of C1–C3. Two things to carry into it from this round: the ROM-CRC pin
(`docs/2026-08-19-aeon-streaming-demand.md` §5 — their fix ladder moves the ROM under the A/B), and §2.4's
warning that the **inclusive** figure must not be the compared quantity, because our two mechanism
differences from the reference have opposite signs and can cancel into a false agreement.

---

## 6. Verification note

**Docs only, zero `crates/` changes. No `cargo` of any kind was run — another agent holds the serialized
build lock — and no emulator MCP tooling was used at any point.** Nothing in §2.5's fixture, §3.4's delta
text or §4's shapes has been executed; they are specifications for the implementing agent and the delta
drafter respectively.

**Read at these revisions, stated because two of them are moving.** oracle-next `profiler-s1` at
**`b46bbbc`** (advanced from `4b1dd81` mid-pass; see §0) — another agent is actively committing there, so
every `profiler.rs:NNN` above may drift again while the symbol names will not. empyrean `profiler-amendment` at **`6d5cb4b`**, confirmed still the tip
and still not an ancestor of the contract's default branch. aeon master at **`3469c920`**. The reference at
`/home/volence/sonic_hacks/oracle`, working tree.

**Verified firsthand rather than carried.** `OpGetProfilerFrames` was read in full at
`ControlSocket.cpp:1944-2050` and every line number in §2.1 was confirmed against that read, not quoted
from our own recon — which is how §2.1's correction to W4 was found (the recon called the straddling-call
defect a double-count; the cycle effect is **loss**, and the double-count lands on `calls`). Our
accumulator's six-step preemption path in §2.2 was traced through `charge`, `push_frame`, `checkpoint`,
`pop_frame`, `close_routine`, `close_interrupt`, `on_step_retire` and `on_frame_boundary` in the
`b46bbbc` blob, after re-verifying it against the drifted tip rather than trusting the earlier read. The
pre-release predicate in §3.2 was re-checked as a live condition against that same blob — no handler, capability still `false` — rather than inherited from the delta2 ruling's snapshot.

**Not committed to `empyrean`.** §3.4 is a draft handed to the delta drafter, who owns that commit.

Branch `streaming-asks-recon`, cut from `m68000-microop-framework` at `a535384`.
