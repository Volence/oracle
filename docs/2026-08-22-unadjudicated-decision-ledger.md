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

---

## L-08 — F-WINDOW-BUS-FRAME-OFFBYONE: relabel the status line, do NOT sync the counter · `SELF-RULED`

**The question, put to this seat by the aurora lane 2026-08-30:** the window's `F` and the bus's
`frame` diverge without bound. Fix the counter so they agree, or relabel the status line so it stops
looking like a bus field?

**Verdict: RELABEL. The counter stays a counter.**

**Evidence, verified firsthand at HEAD before ruling** (both anchors read, not taken from the register's
prose): the bus field is derived from the emulated clock —
`crates/oracle-aether/src/engine.rs:2337-2339`, `fn frame(&self) -> u64 { self.sys.scheduler().now() /
MCLK_PER_FRAME }`. The window's is `frame += 1` per run-loop iteration,
`crates/oracle-frontend/src/main.rs:1933`, bumped whether or not a frame completed.

**What decided it, and it is not a preference.** `engine.rs:2349-2351` had ALREADY ruled this question
from the other end, in its own comment on `frameToken`: *"Deliberately the **emulated** frame index,
not a UI counter. The sibling's `frame_token` is a UI counter, which forced hand-rolled realignment
three separate ways (recon §5 C2)."* The engine refused to serve a UI counter and paid to find out why.
Syncing the window's counter to the clock is that same refusal re-litigated from the losing side.

**Alternatives considered.**
1. *Sync the counter to the clock.* Rejected. It is a behaviour change that would destroy the one
   thing the counter is actually good for: at a breakpoint halt or a pause, a still-incrementing `F` is
   how a person can see the render loop is alive while the machine is not. A clock-derived number
   freezes there, which is correct for the bus and useless for the window.
2. *Print both.* Rejected as the worst option — two adjacent numbers that usually agree and sometimes
   do not is the join hazard made permanent and given a UI.
3. *Relabel.* Taken. The register's actual named hazard is a reader treating `F` as `frameToken`; the
   divergence itself is two clocks answering two questions, which is not a defect.

**Scope, stated so the next session does not over-read it.** This rules the LABEL. It does not rule
what the label should say, which is a wording call for the parcel that takes it, and it does not
reopen `screen_text`'s disclaimer — the fragment already carries the do-not-join warning in its own
description, so the wire side is closed and only the human-facing side is open.

*Wrong if:* a consumer turns up that genuinely needs the window to report emulated frame position on
its status line — i.e. if the number a person reads there is wanted as a machine coordinate rather
than as a liveness signal. The cheap falsifier: ask the two lanes that read our window (aeon's
eyeball requests, aurora's editor) which of the two questions they are asking when they look at `F`.
Nobody has been asked. If either answers "machine coordinate", option 1 comes back.

**Reviewer:** none — self-ruled by the oracle overseer, per the substitute-seat terms requiring the
reviewer be named on the record. This is a labelling call inside one lane's own frontend and was not
sent to the seat.

---

## L-09 — F-ACCEPT-TABLE-CROSSCHECK-BLIND: `--fail-on-gap` must verify ROW PRESENCE, from a second derivation · `SELF-RULED`

**The question.** The emitter's axis-A/axis-B reconciliation appends to `claimed_lines` **before the row
is written**, so it witnesses that every *access* was claimed and never that every *row* survived.
Measured firsthand 2026-08-30: with the four unguarded `addr` rows dropped cleanly, `--fail-on-gap`
prints `cross-check : AGREES`, `parse complete : yes` and **exits 0** while `UNGUARDED reads` falls
43 → 39. Fix it, or document the limit and point consumers at the suite?

**Verdict: FIX IT — `--fail-on-gap` asserts row presence against a SECOND, INDEPENDENT source
derivation. Documenting the limit is not sufficient.**

**Why not the cheaper option.** "Say what it does not witness and point at the runner" is honest and
would not work. `--fail-on-gap` is the **one-command form**, and a consumer building a gate reaches for
the one-command form — that is what it is for. A caveat in the output does not survive being wired into
a build once; nobody re-reads a gate's preamble. This is the repo's own *loud-on-unmeasurable* bar: a
check that cannot detect the failure it exists for must **fail**, not explain.

**⚑ The constraint that makes this non-trivial, and it is the whole ruling.** The expectation **must
not** be derived from `build_table`'s output, or the flag grades its own homework and inherits exactly
the blindness being fixed. A self-consistent tool agreeing with itself is the vacuous gate this suite
has spent the night finding. **The second derivation already exists and is proven:** the hardening
parcel's `direct_reads_from_source()` (in `tools/test_legacy_accept_table.py`) reads `ControlSocket.cpp`
by a path sharing no code with `build_table`, and its deliberately cruder guard rule makes its unguarded
set a **conservative subset** — it can never invent a row the table is entitled to lack. **Promote that
into the tool as a library function and have `--fail-on-gap` assert against it.**

**Alternatives considered.**
1. *Document the limit, gate on the runner.* Rejected above — correct and inert.
2. *Have `--fail-on-gap` re-run the test suite.* Rejected: couples a data tool to a test framework, and a
   consumer wanting the table should not need `unittest` to get it.
3. *Make the cross-check count rows instead of accesses.* Rejected as insufficient — a count still
   passes if one row is dropped and another spuriously added, and it stays inside `build_table`'s own
   frame, which is the defect.

**Scope.** Rules that the flag must verify row presence and by what means. Does **not** rule the exit
code's granularity, the message format, or whether the conservative subset should later be tightened.

*Wrong if:* the promoted derivation turns out to share a frame with `build_table` after all — e.g. if
both ultimately depend on one brittle assumption about the file's structure (brace matching, say), in
which case a source change could blind both at once and the "independent" second path is theatre. The
falsifier is cheap and should be run when it is built: **change the file's shape in a way that breaks
one derivation and confirm the other still reports.** If both fail together, this ruling has bought
less than it claims.

### FALSIFIER RUN 2026-08-30 — it FIRES. Measured, not predicted.

The prediction above is now a measurement. Three perturbations of a scratch copy of `ControlSocket.cpp`
(the real file untouched), each legal C++ a compiler accepts:

| perturbation | `build_table` (D1) | `direct_reads_from_source` (D2) | caught? |
|---|---|---|---|
| `const char c = '{';` in `OpReset` — brace **desync** | rows 75 → 113 (bodies bleed) | pairs 56 → 92 (bleeds too) | **YES** — `agrees=False`, `complete=False`, exit 1 |
| `const char c = '}';` early in `OpZ80Read` — brace **truncation** | drops `emulator/z80_read.addr` | stops expecting `emulator/z80_read.addr` | **NO** — `agrees=True`, `complete=True`, row-presence reports 0 missing, exit 0 |
| `static std::string OpZ80Read` → `static ReplyString OpZ80Read` | keeps the row (any-return-type regex) | loses the row (literal-signature regex) | n/a — they disagree, in the **safe** direction |

Row 2 is the falsifier landing. `match_braces` does not skip character literals, and it is shared, so a
single legal source edit truncates the same function body for **both** readings: the row leaves the
table, the second derivation stops asking for it, and every check in the tool reports clean. The table
then reads "`z80_read` takes no address", i.e. "this command is safe" — the exact wrong answer the
ruling names as dangerous — with nothing firing anywhere.

**So this ruling bought less than it claimed, and precisely this much less.** What it did buy is real
and is not theatre: row 3 shows the two readings are genuinely independent in **method discovery, key
extraction and the guard rule**, which is the dimension a table-side row drop lives in. Red-first
proof, same commit: with the four unguarded `addr` rows dropped cleanly from the emitter and the source
untouched, the pre-change gate exits **0** with empty stderr, and the post-change gate exits **1**
naming all four (`row missing from table: emulator/z80_read.addr: row DROPPED (source reads it at
line(s) [702]; the table has no such key)`), while `cross-check : AGREES` and `parse complete : yes`
still print — which is why the old signals could never have caught it.

The claim is therefore **narrowed, not withdrawn**: `--fail-on-gap` verifies row presence against a
reading that is independent *in the dimension the check depends on*, and is **not** independent of the
shared lexer. That narrowing is written into `direct_reads_from_source`'s docstring, replacing the
"shares NO code path with `build_table()`" over-claim, which was false as written.

**OPEN — needs a ruling, deliberately NOT decided here.** The row-2 residual is a silent blind spot in
a shipped gate, which is the same *loud-on-unmeasurable* bar this ruling invoked to reject the
document-it option. A cheap candidate mitigation exists and was costed but not built, because it is a
design call the scope above does not cover: **have the second derivation refuse to run — loudly, as
`unmeasurable`, which already fails the gate — when the comment-blanked source contains a brace inside
a character literal.** That is an exact detector for the violated assumption (`match_braces` handles
string literals and not character literals), it is a few lines, and it has zero false positives on the
file today (`grep -c "'{'\|'}'"` = 0; braces appear only inside string literals, which are handled).
Alternatives not evaluated: teach `match_braces` character literals (fixes the cause but moves a shared
primitive under both readings at once), or give D2 its own lexer (real independence, real cost).

**Reviewer:** none — self-ruled by the oracle overseer, named on the record per the substitute-seat
terms. Bounded to one lane's own tool; not sent to the seat.

**Landed:** `parcel/gap-row-presence` — `c655945` (tool + tests, 61/61 green via
`tools/run_accept_table_tests.sh`).

---

## L-10 — the L-09 residual: the second derivation must STOP SHARING THE LEXER, not alarm on it · `SELF-RULED`

**The question, returned BLOCKED by the implementing agent and correctly so.** L-09's falsifier fired.
Measured, and reproduced firsthand at the merge: a brace inside a **character literal** —
`const char c = '}';`, legal C++ — truncates `OpZ80Read`'s body for **both** derivations, because
`match_braces` skips `"` string literals and has **no case for `'`**. The row leaves the table, the
second derivation stops asking for it, `cross-check : AGREES`, `parse complete : yes`, **exit 0**, and
the table then says `z80_read` takes no address. `z80_read` rows fall 11 → 5 in silence. Their proposal:
have the second derivation refuse to run, loudly, when it sees a brace in a char literal.

**Verdict: REJECTED in favour of removing the shared dependency. The second derivation must bound
function bodies by the NEXT COLUMN-0 SIGNATURE and never call `match_braces` at all.**

**Why not the alarm.** It is a patch with a bell on it. It detects the one instance we happened to find,
leaves the shared dependency in place for the next lexer defect, and — worse — **refuses legal source**:
the day someone writes `'{'` in that file for an honest reason, the gate stops working rather than
working correctly. A detector for a known bug is not independence; it is a named exception to a claim
that is still false.

**Why the boundary rewrite.** Measured before ruling: **all 54** `static std::string Op*(` signatures in
`ControlSocket.cpp` sit at **column 0** (`^`-anchored, `re.M`), and the existing code already relies on
that anchoring for `CanonicalOp`. So each body can be bounded from its own signature to the next
column-0 `static ` line, using **no brace matching whatever**. That is a *different structural
assumption*, which is the entire property L-09 claimed and did not have. Under the falsifier's
perturbation the two derivations then **disagree** — D1 truncates and drops the row, D2 still expects
it — and the gate **fires**, which is the outcome the ruling was written to produce.

**Second, separable: `match_braces` should skip `'…'` as it already skips `"…"`.** That is a plain
correctness bug in the *primary* derivation, not an independence question, and it is worth fixing on its
own account. ⚠ **It must not be conflated with the fix above, and it does not substitute for it** —
patching the lexer removes today's trigger while leaving both derivations yoked to one primitive. The
order matters: with D2 independent, a future lexer defect **fails loudly**; with only the lexer patched,
the next one is silent again.

**Scope.** Rules how the second derivation bounds bodies, and that the lexer bug is fixed alongside.
Does **not** rule the exit code's granularity, the message format, or subset tightening — all still
untouched from L-09.

*Wrong if:* the column-0 anchoring is not actually load-bearing in the source — e.g. a handler is
defined inside a namespace block or indented — in which case the boundary walk silently mis-bounds a
body and buys a *different* blind spot rather than none. **The check is cheap and must be run as an
assertion in the code, not once by hand:** if the signature census disagrees with the handler count the
derivation already validates (`< 20 handlers` is an existing hard error), it must fail as
`unmeasurable`, which already fails the gate.

**Credit where the class was found:** the implementing agent ran three perturbations rather than the one
the falsifier named, and the one that mattered was not the one I predicted. My own dispatch note guessed
the independence would "probably hold in the dimension row-presence depends on" — it does, for a
table-side row drop, and does not for a source-side truncation. **The prediction was half right and the
failing half was the dangerous one.**

### BUILT AND MEASURED 2026-08-30 — `parcel/lexer-independence`, `8ba020a` + `cef4d06`

The ruling above was written from one hand-run census and a prediction. Both are now measurements.

**The census, re-derived rather than trusted.** 53 `^static std::string Op*(` signatures plus
`^static std::string CanonicalOp(` = **54**, every one at column 0 — an indentation-tolerant grep finds
the same 53, i.e. there is no Op handler defined off column 0 anywhere in the file — and `Handlers()`
dispatches exactly those 53. The ruling's "all 54" holds.

**`_col0_body` replaced the brace matching, and the rewrite is inert on the file it reads.** On the clean
source the new derivation's `pairs`, `unguarded`, `guarded` and `handlers` are **identical** to the
brace-matched version. Nothing that was right changed; only what happens under perturbation did.

**The falsifier's own row is now caught, with change (1) alone applied:**

| perturbation (scratch copies; the real file was verified untouched) | before | after (1) | after (1)+(2) |
|---|---|---|---|
| `const char c = '{';` in `OpReset` | exit 1 (cross-check disagrees) | exit 1, same | **exit 0 — neutralised at the lexer; the table is now correct** |
| **`const char c = '}';` in `OpZ80Read`** | **exit 0, silent, rows 75 → 73** | **exit 1, naming `emulator/z80_read.addr` AND `.len` by name, while `cross-check : AGREES` and `parse complete : yes` still print** | exit 0 — neutralised; table correct |
| `static std::string OpZ80Read` → `static ReplyString` | exit 0, they disagree safely | exit 0, unchanged (D2 loses 2 rows, the table keeps them) | exit 0, unchanged |
| `#if 0` / `}` / `#endif` in `OpZ80Read` — NEW | — | — | **exit 1, naming both rows** |

The second row is the ruling landing. The third confirms the census assertion does **not** fire on a
retype: that stays a safe-direction disagreement rather than becoming a false `unmeasurable`.

**The `unmeasurable` falsifier ships as code, and was proven red-first.** Three raises in
`_assert_definitions_are_at_column_zero`: an Op definition off column 0, a dispatched handler with no
column-0 definition, and a census count that disagrees with `Handlers()`. Red-first each returned
`AssertionError: AssertionError not raised` before the code existed. A fourth test reads the compiled
function's reachable names (bytecode, not text — the docstring *names* `match_braces` to explain why it
must not call it) and a fifth holds `_col0_body` itself, so the brace matching cannot simply move one
call deeper.

**⚑ The lexer fix voided the acceptance test's control, exactly as the ruling warned it must not be
conflated.** With `'}'` correctly lexed, that perturbation no longer breaks the builder, so it no longer
demonstrates anything about independence — and the test said so loudly instead of passing. The
acceptance perturbation is now `#if 0 } #endif`: a brace the preprocessor removes and a brace *counter*
cannot, which rests on no live bug and cannot be neutralised by fixing one. The character-literal case is
kept as its own regression test asserting **both** defences. **Order confirmed empirically:** had only
(2) shipped, every one of these perturbations would read clean and the next lexer defect would be silent
again.

**`match_braces` moved no currency.** At all 468 `{` positions, on both the raw text and the
`blank_comments()` output, old and new return the same index — 0 differences — and the emitted table is
identical object-for-object.

**What is still shared, and what was probed rather than argued.** `blank_comments` is the one `lat`
helper left on the second derivation's path, kept deliberately: a hand-copied second lexer written by the
same author on the same day is two copies of one opinion, not independence. It is a narrower exposure
than `match_braces` was — it is not a body-extent helper, so a defect in it can no longer move a body's
boundary or attribute it to the wrong function, and it does handle character literals (read, not
assumed). It does **not** handle C++11 raw strings. Four legal `R"(...)"` injections were run: `R"(")"`
desynced the builder into bleeding neighbouring bodies and **the cross-check fired**; one crashed
`parse_handlers` with a loud `ValueError`; two were harmless. **None produced the dangerous shape — both
readings losing the same row with every check clean.** So the raw-string gap is real and the blind spot
is **unproven**. Registered here as `F-ACCEPT-TABLE-RAWSTRING`, not claimed, and deliberately not ruled:
whether to teach `blank_comments` raw strings is a cheap fix on its own merits, and whether the second
derivation should stop sharing comment blanking at all is the same design question one level down.

**Suite:** 61 → 73 tests, all green via `tools/run_accept_table_tests.sh` — 0.699s for the tests, ~0.7s
for the whole runner (baseline 61/61 in 0.592s at uptime 4 days 21:27; final 73/73 at uptime 4 days
21:39). No existing test was weakened or skipped; two stale docstrings that still carried the "shares no
code with the table builder" over-claim were corrected.

---

## L-11 — Parcel 2a's two design calls: the panel/parcel split, and `Host::call` applying neither deferred run-state change · `SELF-RULED`

**Reviewer: none.** Ruled by the oracle overseer on the ordinary model, the Fable seat being on HOLD per
owner ruling 2 of 2026-08-22. Named here at the top per the 2026-08-27 hub rule that a substituted or
self-ruled adjudication announces itself in the artifact a cold reader picks up. **Entered after the
dispatch rather than at it** — the split was decided when the brief was written and the ledger entry was
not; that is the standing rule missed by a few hours and is recorded rather than backdated.

### (a) Parcel 2 was split 2a / 2b / 2c, and the transport bar moved out of 2a

**Verdict.** `docs/2026-09-03-debug-panels-design.md` §5.5 recommends parcel 2 = P1 + P2 + P3 **plus the
transport bar**. I shipped 2a as `Host::call` + P1 + the parity test only, and deferred the transport bar
to sit with the Memory panel's gated writes.

**Why.** The transport bar's `step` / `run_to` / writes are `require_paused`, so the bar requires the
player's pause flag to be mirrored onto the bus (`Host::set_paused`). That is a run-loop coupling, and the
design's own §4.3 item 4 says it "must be *designed*, not discovered". Discovering it inside a parcel
whose other half is a register grid is how it gets discovered.

**Alternatives considered.** (i) Ship §5.5 whole — rejected: the pause-mirroring lands as a side quest
inside an unrelated parcel. (ii) Ship `Host::call` alone — rejected on this repo's own bar that *a merged
serve is not a served method*: an entry point with no caller is unexercised. The parity test routing
through `call` is what gave it a real consumer inside 2a.

**What would have to be true for this to be wrong.** That pause-mirroring turns out to be trivial and
independent of the run loop, making the split pure overhead — two merges and two verifications where one
would have done. **The audit should re-run:** whether 2b's pause-mirroring actually touched the loop. If
it did not, this call cost a merge cycle for nothing.

### (b) `Host::call` applies neither `pending_free_run` nor `pending_break`

**Verdict.** RATIFIED as the agent built and argued it, against the alternative of mirroring `pump`'s
apply. Pinned by `call_leaves_the_deferred_run_state_changes_for_the_drain`.

**The reasoning, which is the agent's and which I checked rather than accepted.** Both applies emit
`emulator/stopped` / `resumed`, so a panel repainting at 60 Hz through `call` would mint run-control
events as a side effect of drawing itself. And a second apply site adds an interleaving `pump`'s ordering
comment does not cover: ordered `run ▸ record_break ▸ call ▸ set_paused ▸ pump`, the halt applies in
`call` and `set_paused` — comparing against the now-halted engine — then queues `free_run = true` for the
drain, which is a machine that stops on a breakpoint and silently resumes. That is precisely the
believable wrong answer `pump`'s load-bearing ordering exists to prevent, reintroduced by duplicating the
site.

**The cost, named and not hidden.** Between a latch and the next drain, a `call` to a run-state-reporting
method can answer with the pre-halt state. Bounded by one iteration, self-correcting at the next `pump`,
and `Host::is_paused` (which already consults `pending_free_run`) is the truthful host-side reading
meanwhile — so the panel is told to read *that*.

**What would have to be true for this to be wrong.** That some caller legitimately needs `call` to see
post-latch run state within the same iteration — a per-gesture command that must observe a halt the
current frame latched. No such caller exists in 2a. **The audit should re-run:** the transport bar in 2b,
which is the first plausible one, and whether its Step button reads a stale `running` for a frame.

**Verified firsthand at merge**, not taken from the report: merged tree 66 legs / 2097 passed / 0 failed /
6 ignored, exit 0, HEAD stable `31d3408` across the run, all seven new tests present **by name** in that
run's own log; `fmt` 0; `clippy` 0/0. An independent mutation of my own — splitting the `A7 = SP` row into
two single-key rows, applied and read back from disk — failed `the_shared_a7_sp_row_carries_both_keys_and_says_so`
(`left: 0, right: 1`) **while the main parity test stayed green**, which establishes that the two tests
have independent teeth rather than one being a restatement of the other. The agent's own M3 tripped both
at once and could not have shown that.
