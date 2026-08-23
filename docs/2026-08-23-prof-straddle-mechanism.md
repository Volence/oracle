# Q-PROF-STRADDLE — one mechanism, and a correction against my own finding

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
