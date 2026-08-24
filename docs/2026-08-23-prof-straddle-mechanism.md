# Q-PROF-STRADDLE — one mechanism, and a correction against my own finding

> ⚠ **SETTLED 2026-08-24 — it is a DEFECT, and it is FIXED.** The test §5 asks for was written, went
> red with the numbers §7 records, and is green on the fix. **§2 of this document is itself wrong** and
> §7.2 retracts it; **§3 and §6 rest on a premise that does not hold** and §7.3 retracts that. Read §7
> before acting on anything above it. Branch `prof-straddle`; tests and fix in
> `crates/oracle-core/tests/profiler.rs` and `crates/oracle-core/src/profiler.rs`.

**2026-08-23.** Written after aeon returned an *empirical* straddle observation against the
reasoned one this repo booked on 2026-08-22. Method: source read at
`crates/oracle-core/src/profiler.rs` and `crates/oracle-aether/src/engine.rs`, cited by line.
**No cargo was run and no machine was booted** — every claim below is a source claim, and the one
thing that would settle it empirically is still the unwritten test in §5.

## 1. The two derivations are the same mechanism

**Mine** (`docs/2026-08-22-aeon-instrument-asks.md` §3.3a): `perFrame[].vintCycles` displaces a
boundary-straddling handler's cost into the frame it returns in. Reasoned from Rust source.

**aeon's** (their `docs/DEFERRED_WORK.md`): differencing inclusive `cyclesTotal` is not a per-frame
quantity — `GameState_OJZScroll_Update` reads 3,836 in one frame and **149,104** in the next, more
than a whole ~128,000-cycle frame. Measured on a sustained-streaming workload.

They are the same line of code. The chain, and it is kind-agnostic at every step:

1. `Frame::unreported()` (`:320`) returns the delta of `inclusive()` — `self_cycles + child_cycles`.
2. `checkpoint()` (`:653`) writes `row.self_cycles += d_self` and `row.cycles += d_incl` (`:668-669`)
   through **one match arm pair that treats `Routine` and `Interrupt` identically** — the only
   kind-dependence is which map the row is drawn from.
3. A parent's `child_cycles` acquires a callee's time **only when the callee pops**:
   `parent.child_cycles += frame.inclusive()` (`:724`), the callee's whole lifetime at once.
4. So any boundary checkpoint taken while a callee is still in flight beneath a frame gives that
   frame's **inclusive** figure no credit for the in-flight time, and the whole of it lands in the
   frame where the callee returns.

This is documented, and documented correctly, at `:105-108`: *"Its distribution across frames lags
where a callee straddles a boundary… `self_cycles` has no such lag."* aeon observed the documented
property on a routine row; I reasoned it onto an interrupt bucket. **Same defect, two surfaces.**

Under the protocol's bar 19 this is genuine corroboration rather than echo: the enumeration
parameters could not have shared a frame — one is a source read, the other a per-frame series off a
real workload. It also means the reproduction already exists and neither lane has to write one to
*observe* the mechanism.

## 2. Correction against myself — the magnitude was overstated

> ⚠ **RETRACTED 2026-08-24 — this section is wrong.** It reasons about the generic parent/child rule
> and misses that the accountant interposes a *routine* frame for the handler beneath every bucket, so
> a bucket's `self_cycles` is the exception entry alone and the handler's own retirement is already
> child time. The original §3.3(a) claim this section "corrected" was right. See §7.2.

§3.3(a) said frame *N* reports *"`vintCycles` about equal to the exception-entry cost alone"*. **That
is wrong**, and the error is not cosmetic.

`checkpoint` runs for **every live frame** at a boundary, interrupt frames included (`:971`). So an
in-flight handler flushes its own `self_cycles` delta on time, every boundary. What frame *N* misses
is **only the time held inside a callee that is itself open across the boundary** — not the handler's
own execution, and not callees that opened and closed inside frame *N*.

Consequences worth stating plainly:

- A handler with **no** open-across-the-boundary callee — a flat loop, or one whose `jsr`s all return
  before the boundary — shows **no displacement at all**. The finding does not fire.
- The displacement is bounded by the in-flight callee's lifetime, which is exactly the quantity
  aeon measured at 149,104.
- The severity ranking in the asks-doc register still stands (latent, displacement not loss,
  aggregate exact), but the *trigger* is narrower than written: it needs a straddling **callee**
  under the handler, not merely a straddling handler.

## 3. The answer to aeon's open question — `cyclesSelf` saves the bucket too, but not on the ring

> ⚠ **RETRACTED 2026-08-24 — the premise is false.** A bucket's `self_cycles` is not "the bucket's
> cost with callees subtracted"; it is the **exception entry alone**, for the reason §7.2 gives. It is
> lag-free and useless in the same breath, so "use `cyclesSelf`" was never advice a bucket consumer
> could act on — on the ring **or** on the aggregate. See §7.3.

They asked, correctly flagging it as unsettled: their remedy is to build per-frame work from
`cyclesSelf`; does that also save the interrupt bucket?

**On the aggregate surface, yes.** `row.self_cycles += d_self` (`:668`) is written for
`FrameKind::Interrupt` through the same arm as `Routine`, and `self_cycles` carries no in-flight
lag by construction — a frame's own retired cycles are charged to it at `:630` as they retire. So
`interrupts[].cyclesSelfTotal` is lag-free, and the reconciliation identity is stated on self for
exactly this reason (`:89`).

**On the per-frame ring, no — and this is the actionable gap.** `FrameRow` carries five fields and
the two bucket figures are **inclusive only** (`:359-361`, cut from `Counts.cycles` at `:978-991`).
The wire row is the same five keys (`engine.rs:2996-3001`). There is no `vintSelfCycles`, no
`hintSelfCycles`, and no per-routine breakdown on the ring at all.

So a consumer told *"use cyclesSelf"* can act on that advice for routines via `get_profiler`, and
**cannot act on it at all for buckets via `get_profiler_frames`** — the field it would need does not
exist. Any fix that ships as advice rather than as a field leaves the ring consumer with nothing.

## 4. Framing correction, accepted from aeon

This repo told the hub *"aeon has held their profiler migration on it"*. That over-reads, and aeon
re-measured to say so: **three named cost probes are held** (`raster_cost_probe.py`,
`engine_baseline_probe.py`, `streaming_choke_probe.py`, all still on the legacy harness); the
migration itself is **open and partly landed** (`tick_variance_probe.py` runs oracle-aether with
`--no-pace` today); and **nothing on their queue in flight waits on this.** The lane-status queue
item has been reworded — Q-PROF-STRADDLE is not an unblock-a-peer item and should not be ranked as
one.

## 5. What the test must do, and what would discharge the hold

Unchanged in shape from §3.3, sharpened by §2. A synthetic stream where:

1. an `iack` opens a level-6 bucket;
2. **the handler calls a routine** — this is the part §2 shows is load-bearing, and the part the
   originally-drafted test would have missed;
3. a frame boundary lands while that callee is still open;
4. the callee returns, then the `RTE` arrives, in the following frame.

Assert on the ring: frame *N*'s `vintCycles` omits the in-flight callee time, frame *N+1*'s carries
all of it, and `row.vint_cycles < row.cycles` — which the existing core assertion
(`tests/profiler.rs:894-897`) makes only against a bare-`rte` fixture that cannot straddle.

No machine needed. aeon's banked condition discharges **either way** — a fix with this test, or a
reasoned conclusion that it is not a defect. Their stated scar tissue is holds nobody revisits, so
the closing note matters as much as the fix.

## 6. aeon's named ask — per-frame bucket SELF cycles on the ring

> ⚠ **SUPERSEDED 2026-08-24 — do not build this as specified.** `vintSelfCycles`/`hintSelfCycles`
> would ship a per-frame column carrying the exception-entry cost and nothing else, which is not the
> quantity the ask was filed to get. The straddle fix delivers what aeon actually needs *on the field
> they already have*. §7.3 states what, if anything, is left of `PROF-RING-SELF`.

**Filed 2026-08-23** after they verified §1–§3 firsthand at this repo's `origin/main`, anchored by
symbol rather than line. Sorted by them under the protocol's gap rule, and the sort is correct:

- **Genuinely-new:** `vintSelfCycles` / `hintSelfCycles` on the `perFrame[]` row.
- **Composable today, explicitly NOT part of the ask:** aggregate bucket self via
  `interrupts[].cyclesSelfTotal`. Whole-sample questions are already answerable.

**Their condition, and it is the right one.** Advice to "use `cyclesSelf`" with no field to use
means a porter reaches `perFrame[]`, finds only the inclusive bucket figures, and either uses them
silently or invents a workaround — the permissive-stale failure mode. On their workloads a tick is
190,931 cycles against a ~128,000-cycle frame, so **straddling is their normal case** and the wrong
number would look plausible every time. Their booking: do not port a per-frame bucket consumer until
the field exists **or the ask is refused**. A refusal discharges it as cleanly as a fix — they would
build the consumer differently rather than argue. No date wanted, and **explicitly no reordering**;
the breakpoint parcel stays `next` because that one does gate their unattended gates and this gates
nothing of theirs today.

### 6.1 Price — the accounting is already there; the cost is contract surface

They asked whether it falls out of the straddle fix for free. **On the core, effectively yes.**

`pending_buckets` is a `BTreeMap<u8, Counts>` (`:428`) and `Counts` already carries `self_cycles`
alongside `cycles` (`:162`). The ring's `bucket` closure (`:978-982`) reads `.cycles` from exactly
that struct; a sibling closure reading `.self_cycles` needs **no new accounting, no new call site and
no change to when anything is charged**. It is also correct at that point by construction: the row is
cut *after* `checkpoint()` has flushed every live frame into `pending_*` and *before* the drains
empty them (`:969-976`), and `self_cycles` carries no in-flight lag — so the figure would be the
**exact** per-frame answer rather than a mitigation for the inclusive one.

The real cost is the contract, not the code: two `FrameRow` fields, two wire keys
(`engine.rs:2996-3001`), a schema fragment, the conformance pin, and a protocol section. That is a
CR, and it should be priced as one rather than smuggled in beside a bugfix.

**One caveat to state before anyone consumes it**, so it is not discovered as a surprise: a bucket
already open when the sample opened is `suppressed`, and its self cycles go to
`unattributed_cycles` rather than to the bucket row (`:279-286`, `:660-663`). `vintSelfCycles` would
inherit that rule exactly as `vintCycles` does today — it is the documented retroactive-entry rule,
not a new hole, but it means the first frames of a sample armed mid-handler under-report on both
fields alike.

**The caveat has a detector on the consumer side, and it is the `== 0` half.** aeon checked their
tree rather than filing the caveat: `tools/tick_variance_probe.py` gates the reconciliation identity
at every prefix rung *and* asserts `unattributedCycles == 0` at the states it samples. That second
assertion is the whole protection, and the distinction matters — a suppressed bucket **conserves**
cycles, routing its self time to `unattributedCycles` (`:660-663`), so **the identity still closes**
with the term large. This is §3.3(c) of the asks doc restated from the other side: the identity is a
loss detector, not a correctness proof. A porter who carries the closure check and drops the `== 0`
assertion has kept the shape of the gate and thrown away the part that fires.

They have banked "every remaining port carries the identity check" as a MUST for exactly this
reason, preferring it to an arming rule because a rule has to be remembered while the identity fails
loudly. Same family as the suite's `updatedAt`-from-the-shell ruling: mechanism over vigilance.

**Status: registered, not started.** Queue item `PROF-RING-SELF`. Unpriced beyond the above until
the owner picks it, and a refusal remains a live and complete answer.

## 7. Settled — 2026-08-24. It is a defect, it is fixed, and two of the sections above are wrong

**Method, stated because §1–§6 could not claim it:** the test §5 asks for was written first and run
against unmodified `main`. Every figure below was produced by `cargo test`, not reasoned. No emulator
and no machine — these are synthetic streams into the accumulator, exactly as §5 said they could be.
Branch `prof-straddle`; `68461a7` is the red tests alone, `4111c88` the fix.

### 7.1 The fork, and the evidence that decided it

The dispatch named two defensible readings: **(A)** the per-frame ring is defective, or **(B)** it is
correct-as-documented and the remedy is the separately-registered `PROF-RING-SELF`. **(A).** Three
pieces of evidence, in the order they landed:

1. **The row breaks the ring's own stated invariant, on a fixture that cannot be argued with.**
   `tests/profiler.rs` has asserted `row.vint_cycles < row.cycles` since the ring shipped — *"the
   interrupt is part of the frame, not the whole of it"* — against a bare-`rte` ROM that cannot
   straddle. Put a bucket across a boundary and the row reports **40 against a 30-cycle frame**, and
   with a callee in flight **80 against a 50-cycle frame**. A figure that exceeds the total cycles the
   machine retired in the frame it names is not a lagging per-frame quantity; it is not a per-frame
   quantity at all. Reading (B) has to defend shipping that, and it cannot.
2. **The header comment (B) rests on does not cover the ring.** It is exact and it is about
   *inclusive routine figures*: the parent's checkpoint cannot see time in flight beneath it. That is
   true and it is fine — `Report.routines[]` is a whole-sample total, where the lag cancels.
   `FrameRow` is the one surface where distribution across frames *is* the product, and its own field
   comment made the flat per-frame claim (*"What this frame's level-6 (VBlank) interrupt cost"*) with
   no lag caveat anywhere near it. Documented-elsewhere-about-something-else is not documented.
3. **(B)'s remedy does not work.** §7.3. That is what turned a judgement call into a settled one:
   choosing (B) would have deferred the fix to a CR that could not have delivered it.

### 7.2 §2 is retracted — a straddling bucket displaces whether or not a callee is in flight

§2 argued the trigger needs an in-flight **callee** beneath the handler, because `checkpoint()` runs
for live interrupt frames too and flushes their `self_cycles` on time. The flush is real. The
conclusion does not follow, and the reason is one branch §2 never opened:

`Profiler::on_step_retire` consumes the acknowledge and pushes the bucket — and then **arms a call**
from the same signal, so the next retire's PC opens a *routine* frame for the handler's entry address
**beneath the bucket**. That is deliberate and documented there (a handler is code, and code gets a
row; the interrupt split is additive to per-routine rows rather than a replacement). Its consequence
for this question is that **an interrupt bucket's `self_cycles` is the exception entry alone** — the
one step charged before the handler's frame is pushed. Every cycle the handler itself retires is
already child time, so the bucket's inclusive figure gets *none* of it until the `RTE`.

This is not a new reading of the code; it is already pinned, in this repo, by
`a_nested_hint_inside_a_vint_charges_the_inner_bucket_alone`, which asserts
`(hint.self_cycles, hint.cycles) == (STEP_CYCLES, 3 * STEP_CYCLES)` and says so in its comment. §2 was
written without consulting it.

Measured, on the no-callee fixture — a handler that `jsr`s nothing at all, 4 steps in frame *N* (3
under the bucket), 3 in frame *N+1* (2 under it):

| | frame *N* | frame *N+1* |
|---|---|---|
| the stream (steps × `STEP_CYCLES`) | **30** | **20** |
| §2's prediction (entry + handler's own self) | 30 | 20 |
| what `main` actually reported | **10** | **40** |

§2 predicted no displacement here at all. There is full displacement here. The original §3.3(a) —
*"`vintCycles` about equal to the exception-entry cost alone"* — was **right**, and the `⚠ CORRECTED`
banner now standing on it in `docs/2026-08-22-aeon-instrument-asks.md` is the error. The trigger is a
straddling **bucket**, full stop; the callee variant (§5's shape, `(10, 80)` reported against `(50,
40)` true) is a larger instance of the same thing, not a different one.

**Why the correction went wrong, since the pattern is the reusable part:** §2 reasoned about the
generic parent/child rule and stopped there. It never asked what a bucket's children actually *are* —
and the answer is "all of it, always", from a push the acknowledge arms rather than from anything in
the program. Two adjacent facts, correct on their own, that only compose when read together.

### 7.3 §3 and §6 are retracted — `cyclesSelf` cannot answer a bucket question, at any granularity

§3 answered aeon's open question with *"on the aggregate surface, yes"*: `interrupts[].cyclesSelfTotal`
is lag-free, so a consumer told to build per-frame work from `cyclesSelf` is served for buckets too,
just not on the ring. §6 then registered `PROF-RING-SELF` to close the ring half by adding
`vintSelfCycles`/`hintSelfCycles`.

By §7.2 the premise is false in a way that reverses both. A bucket's `self_cycles` **is** lag-free —
and it is the exception-entry cost, a small near-constant that has nothing to do with what the VBlank
cost. It is not "the bucket's cost with callees subtracted", because for a bucket there is no residue
after subtracting callees: the handler *is* the callee. So:

- **§3's aggregate answer is void.** `interrupts[].cyclesSelfTotal` is not a fallback for
  `cyclesTotal`; it is a different quantity. A consumer who substituted one for the other would read a
  near-constant a *long* way below the figure they wanted — the ratio is the whole handler over one
  exception entry, which on the corpus in `docs/2026-08-20-profiler-corpus-ab.md:727-810` means a
  `vintCycles` of 6,212–21,472 collapsing to the entry alone.
- **§6's ask, as specified, must not be built.** `vintSelfCycles` on `perFrame[]` would be a new wire
  column reporting the entry cost per frame. It would be exact, lag-free, cheap, and would not answer
  the question it was filed to answer — the worst kind of field to ship, because it looks like the
  remedy. §6.1's price ("on the core, effectively yes — a sibling closure reading `.self_cycles`") was
  right about the cost and wrong about the value.
- **What aeon needs, they now have, on the field they already read.** `perFrame[].vintCycles` is the
  right column; it was simply being cut wrong. §7.4.

**Left of `PROF-RING-SELF`:** not nothing, but not this. If a per-frame *self* series is still wanted
it is a question about **routine** rows (`perFrame[]` carries no per-routine breakdown at all today),
which is a different and much larger ask. The bucket half is closed by the fix and should be struck
rather than re-scoped. **Owner call** — this repo does not close another lane's queue item — but the
recommendation is unambiguous: refuse §6 as written and tell aeon why, since §6 itself banked a
refusal as discharging the hold "as cleanly as a fix".

### 7.4 The fix

Charge the ring's cause split on the **`self`** side, as the cycles retire, instead of reading the
bucket's inclusive figure. Both live in `crates/oracle-core/src/profiler.rs`:

- **`Frame::enclosing_bucket`** — the stack *index* of the innermost interrupt frame a frame sits
  inside, read once at the push from the frame beneath, in `push_frame`, next to and exactly like
  `Frame::caller`. `O(1)`, off the hot path, and valid for the frame's whole life because the stack is
  strictly LIFO. An **index** rather than a level so the point of use can honour the suppression rule;
  **innermost** so a nested HInt inside a VInt takes the HInt, which is the split `pop_frame` already
  makes by declining to fold an interrupt into its parent.
- **`Profiler::pending_bucket_retired`** — cycles retired beneath each bucket during the frame in
  progress, accumulated in `checkpoint` from the **same delta** the rows are written from. The ring's
  `bucket` closure in `on_frame_boundary` reads this instead of `pending_buckets[level].cycles`.

Summed over a sample it equals the bucket's inclusive total **exactly** — a bucket's inclusive *is*
the self time of everything beneath it — so this is one quantity distributed two ways, never two
tallies that can drift. Each test asserts that closure directly (`Σ ring vint == sample_interrupts
[VINT].cycles`) rather than trusting the argument.

**What deliberately did not move.** The aggregate: `routines[]`, `interrupts[]`, the reconciliation
identity, `unattributedCycles`. The wire: no new field, no schema change, no protocol change, no
conformance pin touched. The unarmed path: the accumulation is gated on `per_frame_depth > 0`, so a
sample without the ring does exactly the work and produces exactly the figures it did before. And a
bucket that opens and closes inside one frame — every fixture and every corpus row we have —
reports what it always did; only a bucket that actually straddles moves.

**The suppression rule is inherited, and needed its own guard.** A bucket already open when the sample
opened is suppressed and may not be credited retroactively. The frames *beneath* it are not
suppressed — a handler that ran inside the sample really ran — so the guard has to test the **bucket**
rather than the frame being charged. Removing that one check turns
`the_ring_credits_no_bucket_for_an_interrupt_the_sample_never_saw_entered` red.

### 7.5 The tests, and their red-first evidence

Four, all in `crates/oracle-core/tests/profiler.rs`, all synthetic streams, every expected figure
counted off the stream itself (*n* steps × `STEP_CYCLES`) and never read back from a run:

| test | red on | reported | true |
|---|---|---|---|
| `a_straddling_vblank_handler_is_charged_to_the_frames_it_ran_in` | `main` | `(10, 40)` | `(30, 20)` |
| `a_callee_straddling_a_boundary_beneath_a_handler_is_charged_to_the_frames_it_ran_in` (§5's shape) | `main` | `(10, 80)` | `(50, 40)` |
| `a_nested_hint_straddling_a_boundary_is_charged_to_the_inner_bucket_alone` | `main` | hint `(10, 30)`, vint `(10, 30)` | `(20, 20)` each |
| `the_ring_credits_no_bucket_for_an_interrupt_the_sample_never_saw_entered` | the fix with the suppression guard dropped | vint `(10, 10)` | `(0, 0)` |

The first three were red on the parent commit `68461a7`, which is the red-test commit and is meant to
fail. The fourth guards a rule the old code got right for a different reason, so it was made to fail by
mutating the fix, and the mutation restored.

Each also asserts, beyond the two figures: `vint_cycles <= cycles` per row (the invariant the defect
broke), `hint_cycles + vint_cycles <= cycles` on the nested one (the two causes partition, they do not
nest), and that the per-frame figures sum to the undivided bucket — displacement was never loss, and
the fix must not make it one.

A fifth assertion went into the **ROM-driven** ring test that was already there,
`the_per_frame_ring_records_one_row_per_counted_frame`: the ring's VBlank column sums to
`interrupts[VINT].cycles` on a booted machine. The four above pin the distribution and are synthetic
by necessity; this pins the sum through the real retire path, where entries, `rte`s and boundaries
arrive on the machine's schedule. Two accumulators of one quantity is the shape that drifts, so the
cross-check is not optional. Red when the interrupt frame's own delta is dropped from the accumulator:
80 reported against the bucket's 256, with no other assertion in that test moving.

**The gap this closes, stated as the asks-doc stated it:** *no test in either suite put an interrupt
bucket across a mid-sample frame boundary.* Every synthetic bucket opened and closed between two
boundaries and every ROM fixture's VBlank handler is a bare `rte`. That is why an assertion that looks
like it would have caught this (`row.vint_cycles < row.cycles`) never fired.

### 7.6 What is still open

- **Not verified against a running machine.** The fix is proven on synthetic streams, which is what §5
  called for and is the only way to place a boundary on a chosen instruction. Its effect on a real
  workload is unmeasured here — deliberately, per the no-emulator rule this work ran under.
  **TAGGED for foreground follow-up:** re-run aeon's `perFrame[]` capture (the corpus in
  `docs/2026-08-20-profiler-corpus-ab.md:727-810` is the natural control) and confirm the bimodal
  `vintCycles` histogram is unchanged. It should be: those tables carry no near-zero row and no
  doubled row, which is what a straddle would look like, so nothing at those states straddles and the
  fix should move nothing. A *changed* histogram would mean some of them do straddle — itself the
  answer to "is it live in practice", which §3.3(a) could only answer "no evidence at the states
  already measured". Either outcome is worth having; both are cheap.
- **The aggregate keeps a smaller version of the same lag, and it was left alone.** A bucket (or any
  frame) still open at the sample's **closing** boundary has not folded its in-flight children into
  its inclusive figure, so `interrupts[].cyclesTotal` under-reports by one straddle's worth at the end
  of a sample. It is a fixed-size edge effect on a whole-sample total rather than a per-frame error,
  and correcting it would move aggregate figures the conformance surface pins. Named, not fixed.
- **`hintCycles` was never the same risk and still is not.** An HBlank handler outlasting a frame
  boundary would have to survive ~262 of its own firings; the fix covers it for symmetry and because
  the nested case needed the innermost rule to be right, not because it was live.
- **The framing this item carried in our lane status was wrong and is not repeated here.** §4 records
  aeon's refutation: their probes are held on the legacy harness, their migration is open and partly
  landed, and nothing on their queue in flight waited on this. The item was worth doing because the
  mechanism was corroborated from two independent directions, not because anybody was stopped.
