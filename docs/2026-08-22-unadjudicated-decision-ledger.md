# The unadjudicated-decision ledger

**What this is.** Every design decision this repo has taken **without un-framed adjudication**,
recorded so it can be re-examined cold. Created 2026-08-22 on an owner ruling relayed via the
empyrean lane (see the provenance note below): *hold* the Fable adjudicator seat, and

> "keep careful record of what's done without fable so when our limit is no longer up the first
> thing it can do is make sure we made the correct decisions without it."

So this file is **Fable's first work item when the limit lifts**, not a confession. The ruling
converts the adjudication gap from a hole into a queue.

**⚠ PROVENANCE — READ BEFORE ACTING ON THIS FILE'S PREMISE.** The ruling above reached this lane as
a **relay** (empyrean-73, 2026-08-22, quoting the owner in their session). It is quoted words with a
named source, which is far stronger than a status field, and **it is not a granting act this lane
witnessed.** Flagged per this repo's own rule — *never record an approval whose granting act you
have not seen* — which was written hours earlier, today, after exactly this shape failed twice in
one day across two lanes. Owner confirmation requested directly in-session; this note gets replaced
by the confirmation, not quietly deleted.

**The self-referential hazard, stated because it is real:** this ledger's own existence rests on a
relayed approval. If the relay is wrong, the ledger is still correct to keep — a list of
unadjudicated decisions costs nothing and is useful regardless of who asked for it — so nothing
downstream depends on resolving the provenance. That is why it was written before confirmation
rather than after.

## How to read an entry

Each entry must be adjudicable **cold, months later, by someone who was not here.** That means:
what was decided, who decided it, what the alternatives were, what evidence existed at the time, and
**what would have to be true for the decision to be wrong.** An entry that only records the verdict
is useless to the audit this file exists for.

Status values: `UNADJUDICATED` (no ruling of any kind), `PEER-RULED` (a peer lane ruled; not the
un-framed seat), `SELF-RULED` (the overseer ruled on returned work).

---

## L-01 — CR-A, the breakpoint surface · `UNADJUDICATED`

**Artifact:** `docs/2026-08-22-cr-a-breakpoints.md` (1114 lines), merged `1265995`.
**Why unadjudicated:** the un-framed Fable adjudicator was dispatched and died immediately on the
Fable 5 account limit. No ruling of any kind exists.
**Standing bar it violates:** *a ruling authorizes the change; adjudication is what authorizes the
TEXT.* Nothing in CR-A may be implemented until this entry clears.

**Decisions inside it, each separately adjudicable:**
1. **Handles are the addressing primitive** (not addresses). Adopted from aeon's argument:
   address-keyed clear silently kills another subscriber's breakpoint when two lanes arm the same
   PC. *Wrong if:* the concurrency premise is false — if in practice only one subscriber ever arms
   a given PC, handles are cost with no benefit.
2. **`clear {all: true}` survives as a distinct teardown primitive.** aeon's, and the half this lane
   would not have reached alone: a gate that crashed mid-flow cannot enumerate what it armed.
   *Wrong if:* crash-path teardown can be served some other way. Recorded with its reason precisely
   because a future editor will try to simplify it away.
3. **The `stopped` event names the fired handle — PROMOTED to REQUIRED** over aeon's nice-to-have,
   on the grounds that the pre-release window for REQUIRED additions shuts at first ship. *Wrong
   if:* the promotion costs a consumer more than the ambiguity it removes.
4. **Stop precision: either the stop PC is exact, or the server says it isn't.** *Wrong if:* no
   imprecise mode ever ships, making the field dead weight.
5. **`wait_for_break` resolves against an EVENT and must not block the connection.** *Wrong if:*
   the transport can guarantee liveness some cheaper way.
6. **Four drafter departures from my rulings**, each argued at the point of use and each ratified by
   me rather than by the seat: plural `breakpoints` array on `stopped`; `breakpoint_set_enabled`
   rejected (⚠ **this one is a higher bar than an open design choice — it declines an item named in
   the very audit Recommendation that authorises the CR**); D-12 ruled against the audit's
   recommendation; a third reading of D-14 neither of its two offered.

**Strongest finding in it, and the one most worth a second opinion:** D12 mandates a `maxFrames`
bound on any `wait_for_break`-shaped op and is **structurally incapable** of the job — a frame bound
is a bound in emulated time, and a wedge is the state where emulated time stops advancing.

---

## L-02 — CR-B, the Z80 pair · ~~`UNADJUDICATED`~~ **ADJUDICATED 2026-08-30** (original kept below)

**Ruled by the hub** under the owner's standing delegation, at empyrean `ec008ec` — `protocol.md` §11.28,
plus the pair's normative blockquote in §6 and the `z80_read.len` schema description. Verified here:
reachable from their `origin/main`, `--stat` shows `contract/protocol.md` +49 and the schema, so the SHA
carries what it anchors. **Reviewer named per the substitution rule: the hub, which took no part in
drafting this CR and is independent of this lane.**

**Outcome, decision by decision against the five above — and one of ours was ruled AGAINST:**

1. *All three defects in one CR, B4 severable* — **stood.** B3 (§11.24) and B4 (§11.22) had already been
   adopted; B2b **declined for the CR's own reasons**, revisited with D-16.
2. *D-10 as optional `width` ∈ {1,2}, default 1* — ⚑ **REJECTED.** §11.28: one byte per `value`, multi-byte
   spelled `bytes` **low-address-first**, and *"there is no `width` and there will not be one"*. **The
   ruling is better than the proposal and the reason is ours:** our own §2.4 evidence — that the legacy
   server declined a width deliberately and said why — supports *no width at all* more than it supports a
   defaulted one. We carried that evidence to the edge of the right conclusion and stopped one step short,
   proposing a compatible shape where the honest reading was that the parameter should not exist.
3. *Byte order little-endian, argued from the rule not the sibling* — **the reasoning stood and the
   question dissolved.** With one byte per `value` and `bytes` laid down low-address-first, endianness
   never reaches the wire. The argument was right; what it was arguing about was avoidable.
4. *Bound kept at `0–$3FFF`* — **stood**, with the mirror sentence now normative: `$2000`–`$3FFF` is the
   machine, a server MUST NOT correct it, and only the fold past `$3FFF` is wrong.
5. *Six items listed as SETTLED for an adjudicator to object to* — **no objection recorded.**

**And one change to the CR's own text: the overrun code is `-32004`, not `-32602`.** §11.22 had written
`-32602`; §11.28 aligned it with `read`/`memory_hash`/`write_memory`, which carry `-32004` for the
identical refusal. `-32602` stays for **shape** refusals — a `value` out of range, two payload spellings.
The two are different failures and the ruling keeps them distinguishable.

**The live defect finding was upgraded from source-derived to DEMONSTRATED**, which this entry flagged as
the half an adjudicator should know: the legacy start-only bound is now reproduced as a recorded mutation
(move the bounds check after the write → `$0000` holds `0x55667788`, bytes 5–8 of the payload, exactly as
the CR predicted from reading `oracle-old d629771`).

⚑ **THE HUB'S LESSON, AND IT IS WHY THIS ENTRY SAT UNADJUDICATED FOR A WEEK: most of CR-B was already
ruled on 2026-08-26, by an audit pass that never named the CR.** So the contract had moved and the ledger
could not learn it — a decision recorded as open while its answer sat in a section nobody would think to
re-read. **Name the CR in the amendment that takes it**, or the record of what is outstanding rots in the
safe-looking direction: it over-reports work as pending, which costs a lane a week rather than costing
anyone a wrong answer.

*(Original entry follows, unaltered.)*

## L-02 — CR-B, the Z80 pair · `UNADJUDICATED`

**Artifact:** `docs/2026-08-22-cr-b-z80.md` (1028 lines), merged `37a06f9`.
**Why unadjudicated:** same seat, same block. Drafted after the block was known, deliberately — a
drafted CR costs nothing while the seat is held and is strictly ahead when it lifts.

**Decisions inside it:**
1. **All three defects (D-09/D-10/D-11) in one CR, B4 severable.** *Wrong if:* the audit's own
   pairing of D-11 with D-16 is the better split — handed over as Q4 rather than settled.
2. **D-10 shaped as optional `width` ∈ {1,2}, default 1, little-endian** — (a)'s default with (b)'s
   ceiling, against the audit's (b) verbatim. Evidence the audit did not have: **the legacy server
   declined to have a width on purpose and left a comment saying why**, so (b) verbatim refuses
   every bare-`value` invocation on record. *Wrong if:* the domain should be {1,2,4} (Q2).
3. **Byte order little-endian**, argued from the rule rather than the sibling's consequence
   (`write_memory` says "big-endian, *as the 68000 stores*" — the clause after the comma is the
   rule). *Wrong if:* consistency across rows outranks correctness per machine. It does not.
4. **Bound kept at `0–$3FFF`, not narrowed to `$1FFF`** — narrowing was already proposed and ruled
   against in this repo (`docs/2026-08-16-ruling-cr20.md`).
5. **Six items listed as SETTLED** inside the CR so an adjudicator can object to the settling
   itself. Those six are part of this entry's audit surface, not exempt from it.

**Carries a live defect finding, not just contract text:** legacy `z80_write` bounds only the start
address and `WriteRamByte` returns `true` unconditionally, so a write at `$3FFF` clobbers `$0000`
and reports success. **Source-derived, NOT yet demonstrated at runtime** (see the foreground
follow-ups in `docs/OVERSEER.md` item 8) — an adjudicator should know which half is which.

---

## L-03 — The step trio's serve rulings · `SELF-RULED`

**Artifact:** merged `56cc545`, rulings recorded at `490dd31`. The code **is shipped**, which makes
this entry materially different from L-01/L-02: those are text nobody has built to, this is
behaviour in the tree.

1. **`deadlineReached` emitted on a `step` stop — RATIFIED.** §3 scopes it to run-shaped reasons in
   *prose*; the schema permits it unconditionally. Ratified because silence when the bound was hit
   is a believable wrong answer. *Wrong if:* the prose scope is normative, in which case we emit a
   field on a reason §3 did not intend. **Registered as a CR item** rather than left as our reading.
2. **`capabilities` left unchanged — RATIFIED** on §8's invention ban (no step-related capability
   key exists in the schema).
3. **`lookup_symbol` caveat overwrite confirmed and deliberately NOT fixed** — conformant, real
   loss. *Wrong if:* "conformant" is the wrong bar for a known information loss.
4. **F-STEP-FRAME-BOUND:** the 600-frame bound is **server policy and undiscoverable** — no contract
   key exists for it. Rides the D-02 CR.

---

## L-04 — D-02 / D-03 CR text · `UNADJUDICATED` (not yet drafted as a CR)

Banked at `490dd31`, improving on the audit in both cases. **D-02:** `count? (≥0, def 1, ≤
maxStepCount)`, refused above the ceiling rather than clamped; **floor stays 0 against the audit's
`≥1`** (zero is definitional for a count and is a useful *where am I without moving the machine*
probe); plus `reached` on the result, because today the one case a caller most needs the truth —
*did my 10,000 steps happen?* — is the case the result cannot express. **D-03:** give
`step_over`/`step_out` `step`'s `pc`/`symbol?`/`symbolDisp?`; the asymmetry makes the method
**unanswerable** for a conformant client that did not negotiate `events`.

---

## L-05 — `run_to_scanline`: lines 262–511 · `SELF-RULED`, in flight

The fragment bounds `line` at 0–511; this core runs 262 lines per frame, so 262–511 are
contractually legal and physically unreachable. **Ruled: accept them, run the `maxFrames` bound,
return `reached: false` with a caveat saying the line cannot occur in this video mode** — rather
than refusing a value the fragment declares legal (§8's invention ban). *Wrong if:* burning a
600-frame budget on a statically-impossible target is worse for callers than an early refusal.
The implementing agent was explicitly invited to refute this; its verdict belongs in this entry when
the parcel lands.

---

## L-06 — Dispatching the step trio ahead of the pricing survey · `PEER-RULED`

Ruled by the **empyrean lane, not the un-framed seat**: instance ratified, generalisation rejected.
Recorded here because a peer ruling is not adjudication, and this file is the list of things that
did not get the seat.

---

## L-07 — CR-A adjudicated by a SUBSTITUTE reviewer · `SUBSTITUTE-ADJUDICATED`, in flight

**Not an unadjudicated decision — an adjudicated one whose reviewer was not the seat.** Recorded
here because this file is the list of things Fable audits first when the owner lifts the limit, and
that is exactly what the ruling below directs.

**The ruling that put it here.** d-16 (`docs/decisions.jsonl`) asked the owner whether to unpark the
premium independent-reviewer seat, substitute the ordinary model on the record, or keep holding —
three items were stacked behind it. Ruled **SUBSTITUTE** on 2026-08-27 by the **empyrean hub, under the
owner's own overnight delegation** (*"if anything needs decision that they can't make you make it for
them"*, transcribed by the hub into empyrean `OVERSEER.md` addition (f) at 05:39Z and banked at
`091ac59`). ⚑ **This is the hub's ruling, not the owner's, and it is flagged as a relay: this lane did
not witness the granting act.** The owner reviews it on return. The hub's terms, carried verbatim in
substance: run the adjudications on the ordinary model; **name the reviewer on the record in every
ruling**; keep the ledger entry per decision open.

**What was adjudicated under it.** CR-A (`docs/2026-08-22-cr-a-breakpoints.md`, 1114 lines incl. the
§14 overseer addendum) — the breakpoint surface: handles, teardown, attribution, stop precision.
Dispatched un-framed to a fresh reviewer that took no part in the drafting, on branch `ruling-cr-a`,
deliverable `docs/2026-08-27-ruling-cr-a.md`. **Independence is preserved; reviewer tier is not.**

**What the audit should re-run.** The whole-CR verdict, the five per-proposal verdicts (A1–A5), the
seven §12 open questions the draft handed over deliberately unanswered, and above all §14.1's
resolution of the **procedural objection** — whether the audit's *"the answer belongs to the legacy
server"* clause reserves ruling authority, which if wrong voids CR-A entirely. That claim is the CR's
load-bearing precondition and it was resolved by the raising lane's own addendum.

*Wrong if:* the substitute reviewer's tier is what a contract adjudication actually buys — i.e. if the
ruling that comes back is one a premium reviewer would have reached differently on a **material**
item rather than a stylistic one. The M/S split the ruling is required to produce is precisely the
instrument for measuring that, so the audit has a cheap first cut: re-run the M items only.
