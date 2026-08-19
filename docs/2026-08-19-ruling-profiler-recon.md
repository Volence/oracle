# Ruling on the profiler recon's open questions (2026-08-19, controller)

Applies to `docs/2026-08-19-profiler-recon.md` and `docs/2026-08-19-aeon-profiler-demand.md` on
this branch. **The recommended mechanism is ADOPTED**: exact call-graph shadow stack driven by one
new defaulted `on_step_retire` hook (the `bus.rs:184-188` extend-the-trait precedent), interrupt
buckets keyed from the fc=7 IACK the sink already receives (`bus.rs:1060-1064` — cause-keying with
zero core change), PC-sampling rejected on the recon's hard ground (Aeon's merge gate compares with
`==`, `effects_gates.py:754-756` — a sampled figure cannot be `==`-gated). Five load-bearing claims
spot-verified firsthand before this ruling: the dead cycle binding (`system.rs:1035`), the IACK
emit, the `==` gate, the §6 legacy rows (`protocol.md:1311-1313`), and the old oracle's conflation
site (`ControlSocket.cpp:1995/:2016` — the handler PC tested against vector-table constants).

Owner directive applied: the legacy shape is the floor. The v1 set from the better-approach table
is adopted as recommended — exact attribution, inclusive **and** self cycles, cause-keyed buckets,
symbol-resolved names, canonical 24-bit addresses, the stall-inclusive base (a correction, reported
loudly), and the GUI lens as a D15 decision. The flame event tap stays declined, visibly.

## Q1 — `perFrame[]`: yes, bounded, opt-in

Real per-frame rows make `get_profiler_frames` honest and kill the demand side's `frames: sample-1`
folklore (the runt-frame workaround the recon mechanised). Bounded ring, opt-in parameter,
D13-style cap refusal. CR-26 carries the shape.

## Q2 — `budgetPct`: derive from `TimingBasis`

Never a hardcoded NTSC constant. If the derivation is ambiguous for a mid-sample mode switch,
refuse the ambiguity explicitly in the reply rather than averaging over it.

## Q3 — inclusive `cycles` + additive `cyclesSelf`: both ship

`cycles` stays floor-compatible (inclusive, what the old rows meant in practice); `cyclesSelf` is
the additive improvement. Which one the parallax-walker fit actually needs is **asked of the demand
side** (question sent 2026-08-19); their answer informs their migration notes, not our shape —
both fields ship regardless.

## Q4 — per-scanline attribution: own CR, not this arc

Highest-value future item, pairs with the sub-line arc's machinery; folding it here would smuggle a
second instrument under one CR. Registered, not scheduled.

## Q5 — the `name` field: sidestepped by house precedent

Rows carry `name` (bare symbol) + `disp` (integer) as SEPARATE fields — the `symbol`/`symbolDisp`
pair the contract already retyped once (§11.6; the watch-hit rows use it). No `name+$1A` composite
is ever emitted, so `$defs/symbolName` conformance is structural. `F-SYMBOLNAME-DISP`'s foreground
check stays TAGged but no longer blocks anything.

## Q6 — nesting semantics: defined in the CR, exercised at acceptance

The CR pins the stack rule: cycles retired while a nested HInt runs attribute to the HInt bucket;
the suspended VInt accrues nothing (no double-count); bucket opens at IACK, closes at the matching
RTE, nesting tracked by depth. Whether any corpus ROM actually nests is a TAGged runtime question
for the acceptance run — the semantics do not wait on it.

## Q7 — the core-tests zero-file-diff record: broken deliberately

The record was a discipline signal for bus-surface arcs. A core instrument warrants core test
files. This ruling is the named decision the handoff will cite; the diff stays exact-path and
reviewed like everything else.

## Q8 — the lens bit: spend it, widen in the same slice

The profiler lens takes the last free `LensSet` bit and the same slice widens the set to `u16`,
so the next lens costs nobody a ruling.

## The stall correction — promoted to v1

Our clock includes bus/VDP/DMA stalls; the reference's excludes them (`M68000.cpp:1029-1031`), so
the old corpus is structurally stall-free and per-routine cycles will legitimately differ wherever
stalls exist. Ruling: **`stallCycles` per row ships in v1** (not slice 6) — it is cheap (thread the
bus wait returns), it is what lets Aeon's `==` gates reconcile stall-inclusive truth against their
`.emp`-derived ideal constants (`cycles - stallCycles == constant`), and it directly serves their
Task 5. `maxContiguousStallCycles` stays later. Their answer on gate handling is requested but the
field ships regardless — a truth the old instrument couldn't see, made auditable.

## Acceptance protocol — adopted as designed

Primary = A/B against the Phase-0 parity corpus (SHA pending from the demand side). `calls`
matches exactly; `cycles` matches exactly on stall-free paths; stall-heavy rows reconcile through
`stallCycles` or the arc stops; `hint`/`vint` must DISAGREE with the old summed counter by the
falsifiable equation (their `hint` ≈ our `hint + vint`); our spread is exactly 0 across three
boots — a spread is never a tolerance.

## Execution order

1. CR-26 draft (an **amendment** to the three existing §6 rows + first schema fragments for the
   `profiler` family) → un-framed adjudication → fixes.
2. Demand-side shape-check runs in parallel (three questions sent: walker-fit field, stall-gate
   handling, `perFrame[]` interest).
3. Implementation slices per the recon's plan, contract-first, gates and mutation discipline as
   the sub-line arc ran them.
