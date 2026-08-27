# Ruling — CR-A (the breakpoint surface: handles, teardown, attribution, and stop precision)

## Reviewer attribution — required, stated first

**This ruling was produced by the model `claude-opus-5` ("Opus 5").**

It was made **under a recorded budget substitution**: the usual independent-adjudicator seat is a premium
model that is currently parked (this repo's `docs/decisions.jsonl` `d-15`/`d-16` record the seat as parked
and CR-A as one of three items stacked behind it). This ruling is therefore a **substitute ruling** and is
labelled as one so a later audit can find it and re-run it under the premium seat. Nothing below should be
treated as having the authority of the parked seat; it should be treated as the best independent reading
available tonight, with its sources named at revisions so it can be checked line by line.

**Adjudicated:** `oracle/docs/2026-08-22-cr-a-breakpoints.md`, 1115 lines, read in full.
**Ruling date:** 2026-08-27.

### Revisions I actually read

| Source | Revision | How read |
|---|---|---|
| `empyrean/contract/protocol.md` | **`a69327f`** (`origin/main` at ruling time) | `git show a69327f:contract/protocol.md` |
| `empyrean/contract/protocol.md` | `9d6ab1f` (CR-A's base) | `git show 9d6ab1f:…` |
| `empyrean/contract/schema/bus-protocol.schema.json` | `a69327f` and `9d6ab1f` | `git show`, parsed with `json.load` |
| `empyrean/docs/2026-08-22-protocol-schema-audit.md` | `a69327f` and `9d6ab1f` | `git show` |
| `oracle/crates/oracle-core/src/bus.rs`, `crates/oracle-aether/src/engine.rs` | worktree at `6568ca0` | direct read |
| `oracle/docs/decisions.jsonl`, `docs/lane-status.json`, `docs/2026-07-23-timing-ground-truth-fable.md`, `docs/2026-08-22-acceptance-21-survey.md` | worktree at `6568ca0` | direct read |

I never read `empyrean` through a working-tree path. Every empyrean citation below is from a committed blob.

### Two corrections to my own brief, before anything else

**(1) The brief told me `origin/main` is `091ac59`. It is not.**

```
$ git -C /home/volence/sonic_hacks/empyrean rev-parse --short origin/main
a69327f
```

`091ac59` is real and is an ancestor of `origin/main`, four commits back
(`git merge-base --is-ancestor 091ac59 origin/main` → true). The four commits between them touch only
`docs/OVERSEER.md`, `docs/lane-log.jsonl` and `docs/lane-status.json` — **nothing under `contract/`** — so
the brief's error is immaterial to the substance. I read at `a69327f` and say so. Verified firsthand.

**(2) The brief says "the CR's §3–§8 carry proposals A1 through A5". It carries A1 through A7.**

§1's own table lists seven (A1–A7). The mapping is §3=A1, §4=A2, §5=A3, §6=A4, §7=**A6 and A7**, §8=A5.
I rule on all seven. This is partly the CR's fault and is booked as a defect below: **§7 is titled
"A5–A7 — the remaining rulings" and contains no A5** — A5 is in §8. A reader following §0's promise that
"§3–§8 are the substantive proposals" cannot map proposals to sections without discovering this by hand,
which is what happened to my brief.

Everything else the brief told me — that §14 is a merge-time overseer addendum, that §10 states binding
scope, that §11.1 raises a possibly-voiding procedural objection, that the schema's `methods` object maps
method name → fragment with no `properties` sub-object — I verified and all of it is correct. (`methods`
is a 63-key dict, 62 fragments plus one `$comment`; `"properties" in methods` is `False`.)

---

## 1. Verdict on the CR as a whole

# ADOPT WITH CHANGES — and the changes are structural, not editorial.

**The single fact that governs this ruling: the surface CR-A proposes to amend was amended on 2026-08-26,
four days after CR-A was drafted and one day before this ruling, by `§11.21 (CR-BP)`.** Every one of the
five audit defects CR-A's header claims to close is marked closed or ruled at `a69327f`:

| Defect | Status at `a69327f` | Source |
|---|---|---|
| D-07 | *"RULED 2026-08-26 (batch B1, APPLIED as §11.24)"* | `audit`:147 |
| D-12 | *"CLOSED 2026-08-26 by §11.21"* | `audit`:196 |
| D-13 | *"RULED AND APPLIED 2026-08-26 as §11.21 (CR-BP)"* | `audit`:206 |
| D-14 | *"CLOSED 2026-08-26 by §11.21"* | `audit`:223 |
| D-15 | *"CLOSED 2026-08-26 by §11.21"* | `audit`:236 |

CR-A's `Closes:` line is therefore **false at tip**, and its §9 — "The exact deltas requested", which quotes
current text "so the delta is visible rather than reconstructed" — quotes three §6 rows that no longer
exist in that form. A CR whose entire delta set does not apply cannot be adopted as written.

**But this is not a REJECT, for three reasons that matter more than the bookkeeping:**

1. **CR-A was substantially *right*, and was substantially *vindicated*.** Of its seven proposals, five
   sub-rulings and two whole proposals landed in §11.21 in essentially the shape CR-A argued for —
   including the field name `breakpoint`, the never-reused opaque string, the removal of `addr`/`symbol`
   from clear, the survival of `all`, the `-32005 {reason:"breakpointCapReached", cap, count}` refusal, the
   `label`, the never-reset `hits`, the `-32012`/`-32013` refusals, and — against the audit's own written
   recommendation — the ruling that a duplicate add creates a **second breakpoint**. §11.21 credits *"the
   oracle lane's peer answer"* by name. This lane's reasoning is in the landed contract.
2. **A4 (`stopPrecision`) is entirely unlanded and entirely live.** `grep -c stopPrecision` over
   `protocol.md` at `a69327f` returns **0**. The clause CR-A says it *"would most regret losing"* has not
   been ruled on by anybody. It is the most valuable surviving content in this document and it is currently
   hostage to six proposals that are moot.
3. **A5's D12 finding is not merely still live — the landed amendment made it worse**, and nobody noticed.
   See A5 below. That is a defect CR-A identified before it existed.

**What ADOPT WITH CHANGES means operationally here.** CR-A must be **re-cut against `a69327f` as a set of
amendments to the landed §11.21/§11.24, not as a replacement for the pre-amendment rows.** Concretely: A1,
A2, A6's cap, and five of A7's seven sub-rulings are withdrawn as **already satisfied**; A7(a) and A7(d) and
A6's capability-object placement are withdrawn as **ruled against**; A3 is reduced to a narrow residual;
A4 and A5 are split out and re-raised on their own. If the drafters prefer the cleaner bookkeeping of
**withdrawing CR-A and re-issuing the survivors as CR-A′ and CR-STOP-PRECISION, that is an equally correct
execution of this ruling** — my verdict is a disposition of the proposals, not a preference about document
identity.

---

## 2. Per-proposal verdicts

Change markers: **M = material** (the proposal is wrong or incomplete without it); **S = stylistic**
(improves it; adoption does not hinge on it).

### A1 — handles are the addressing primitive (§3)

# ADOPT — and it is already landed, in the shape proposed.

`protocol.md`:1087–1090 at `a69327f`:

```
| `emulator/breakpoint_add` | `addr`\|`symbol`, `enabled`? (def `true`), `label`? | **`breakpoint`** (str), … |
| `emulator/breakpoint_clear` | `breakpoint` (str)\|`all` | `removed` *(§11.21)* |
```

and the normative prose at `protocol.md`:1102: *"**A breakpoint is an opaque handle** (D9 category 4): a
server-assigned string, **never reused**, so a stale handle resolves to nothing rather than to someone
else's breakpoint."* `$defs/handle` is `{"type":"string","minLength":1,…}` — CR-A's §3.3 type argument
carried. `addr` and `symbol` are gone from `breakpoint_clear`, as §3.1 asked.

The argument in §3.2–§3.4 is sound and I would have adopted it on the merits. **No changes required.**
Withdraw as satisfied.

*Note for the record:* CR-A's choice of the field name `breakpoint` over the watchpoint surface's
abbreviation, argued in §3.3 on the grounds that *"'Breakpoint''s head noun is 'break', which is a poor
field name"*, is the spelling that landed. That is a small, real, independent contribution.

### A2 — `clear {all: true}` survives as a distinct teardown primitive (§4)

# ADOPT — landed in substance; one S-level residue.

`protocol.md`:1123–1126: *"`breakpoint_clear {all:true}` removes every breakpoint on the server,
**including other clients'** — that is the one deliberately shared verb, kept because a session recovering
a wedged machine needs it, and it is why `all` is a separate spelling rather than a wildcard handle."*

Both halves of A2 landed: `all` survives as a `oneOf` alternative, and its cross-subscriber blast radius is
stated explicitly rather than hidden (§4.3's ask).

**S-1.** The landed reason is *"a session recovering a wedged machine"*. CR-A §4.2's reason is sharper and
different: *"A client that crashed mid-flow cannot enumerate what it armed… collapsing them into one
mechanism breaks crash-path cleanup."* The wedged-machine reason survives a future editor who thinks "you
have handles, iterate them"; the crash-path reason **refutes** that editor. If the lane wants A2's residue,
it is one sentence appended to the landed prose. Not material — the property is landed and reasoned.

### A3 — the `stopped` event names the breakpoints that fired (§5)

# ADOPT WITH CHANGES — the field and its REQUIRED-ness landed; the plural is REJECTED; a narrow residual survives that neither document identifies.

**What landed** (`protocol.md`:708 and 1133–1137): `emulator/stopped` gains `breakpoint`? in the row, and
the M2 clarification rules it *"'additive' to the **document** and **REQUIRED on the handle shape**: a
handle-shape server MUST emit it whenever `reason` is `breakpoint` and MUST NOT otherwise, under the same
if/then the schema applies to `watch`."* Verified in the schema: `events["emulator/stopped"].params.properties`
now carries `breakpoint`. CR-A won the field, won REQUIRED, and won the if/then mechanism.

**Why the plural is rejected.** CR-A §5.2's argument is that a singular field *"would force the server to
pick one and report it as *the* cause, which is a small silent-wrong-answer."* §11.21 defeats that argument
without touching its premise, by publishing the pick (`protocol.md`:1107–1110):

> When any enabled breakpoint at the PC fires, the machine halts once, `emulator/stopped` carries
> `reason: "breakpoint"` and the additive **`breakpoint`** param naming **one** handle — the
> **earliest-added enabled breakpoint at that address** — and **every** enabled breakpoint at that address
> increments its `hits`.

A published deterministic selection rule is not a silent wrong answer; it is a documented one. CR-A §11.3
frames the choice as binary — plural, or refuse duplicates — and **does not consider the third option that
actually won.** With the rule published, a client can reconstruct the full firing set from
`breakpoint_list` (which handles share the address) plus `hits`.

**M-1 (material).** §14.4's item 2 is presented as *"the strongest single argument in this CR"* and *"the
concrete case for the plural `breakpoints` array"*. **It is not a case for plurality at all.**
`raster_frame_epoch_probe.py:220–221` arms two breakpoints — and §14.4's own emphasis is that *"the two
breakpoints sit on DIFFERENT HANDLERS."* Two breakpoints at two different addresses fire one at a time; a
singular handle names the one that fired, unambiguously and completely. §11.21's tie-break only engages
when several breakpoints share **one address**, which this consumer does not do. §14.4 conflates *"a
fired-handle field is needed"* (true, and it is the strong argument, which §14.4 also makes) with *"the
field must be plural"* (does not follow). **This claim must be struck or corrected before any re-raise;
leaving it standing would put a demonstrably invalid argument into the record as the CR's strongest.**

**M-2 (material) — the residual defect, which neither CR-A nor §11.21 identifies.** Under the landed rule,
`hits` increments on **every** enabled breakpoint at the address, but `stopped` names only the
earliest-added one. So a second subscriber holding the later handle at a shared address **sees its `hits`
move and never once sees its own handle on an event** — and the first subscriber receives an event
attributing the stop solely to its handle when another subscriber's breakpoint also fired. That is
misattribution in exactly the two-subscriber case A1 exists to serve, arriving through the event rather
than through `clear`. CR-A's instinct in §5.2 is right; its diagnosis of *why* is wrong (the pick is not
arbitrary, it is incomplete). **Re-raise as a narrow amendment with two acceptable landings**, cheapest
first:

- **(a)** one normative sentence: a client MUST NOT infer sole causation from the named handle, and MUST
  consult `breakpoint_list` when it holds more than one handle at that address; **or**
- **(b)** CR-A's plural array, correctly motivated this time — on same-address multiplicity, which is
  reachable by construction under the landed duplicate-add rule, and **not** on §14.4's example.

I do not choose between (a) and (b); either closes it, and the cost difference belongs to the implementers.

**S-2.** §5.2's closing observation about `watch`'s latent multiplicity is good and should survive the
re-cut — see Q5, where it is now a live under-specification in the landed contract rather than a symmetry
worry.

### A4 — stop precision: exact, or the server says it is not (§6)

# ADOPT WITH CHANGES — the most valuable surviving content in this CR, and it must be split out of it.

`stopPrecision` appears **zero** times in `protocol.md` at `a69327f`. Nothing in §11.21–§11.25 touches it.
This proposal is untouched by the drift and unruled by anybody.

**On the merits I find for it.** §6.1's property (*"the failure mode that hurts is not an imprecise stop; it
is an **imprecise stop presenting as a precise one**"*) is the correct framing, and §6.2's evidence — a
det-mode stop landing one instruction early, before an `adda.w`, *"that would make the gate PASS on code
that never applied the offset"* — is a false-pass, which is the exact class the contract repeatedly names as
what it exists to prevent. §6.3's REQUIRED argument is not a taste call: it is `§2.4` clause (a) applied,
and I verified that clause applies word for word at `protocol.md`:577–578 — *"`truncated` is required **even
when it is `false`**: absence and `false` must not both mean 'you have everything'."* §6.6(b)'s rejection of
`caveat` as the carrier is likewise not a taste call: `§2.4` rule 3 at `protocol.md`:549 is verbatim
*"**Any consequence a client must act on needs its own typed key**"*, and clients **MUST NOT parse**
`caveat`. A server that today ships this warning as prose is shipping something no gate can branch on,
which is precisely what the consumer worked around with a launcher hack.

**M-3 (material) — split it out.** A4 is not a breakpoint clause. Its own §6.4 rule text governs *"whenever
a server halts the machine on a condition naming a PC and reports that PC"*, and its scope already reaches
`runTo`. Filing a bus-wide stop-semantics rule inside a CR about four breakpoint rows is the same category
error §11.5 correctly self-diagnoses for A5(1) — and it is now the reason A4 has sat unruled while its
six housemates went moot underneath it. **Raise it against §3/§6's stop surface as its own CR.**

**M-4 (material) — the handshake level cannot land where A4 puts it.** §6.3 and §7.1 put it at
`capabilities.breakpoints.stopPrecision`. `capabilities.breakpoints` is **still a boolean** at `a69327f`,
and §11.21 design choice 3 ruled deliberately that it stays one (see A6 below). Independently of that
ruling, `breakpoints` was always the wrong home for a key whose own scope includes `runTo`. **Re-site the
handshake level** — a top-level `stopPrecision` in the `initialize` result, or the additive `limits`-style
treatment §11.21 chose for its own cap, are both available; the drafters should pick and argue one.

**M-5 (material) — strike the arm-reply level.** I adopt CR-A's own §11.7 recommendation, which is correct:
*"The **arm-reply** level is the redundant one… If the adjudicator strikes one, strike that one; the
two-level version keeps the whole property."* Two levels: handshake (*should I run against this server at
all?*) and `emulator/stopped` (*is **this** stop exact?*). The three-level version also costs a REQUIRED
key on `breakpoint_add`'s result, which is surface on a row that was just rewritten.

**M-6 (material) — the enum needs a third member, and Q2 is why.** See §4, Q2. `"exact" | "approximate"`
cannot express the one imprecision this workspace has actually characterised (a watch stop lands *after*
the triggering instruction commits — precise, just not at the armed instruction). CR-A §6.6(a) rejected a
boolean because *"a granularity vocabulary is exactly the kind of thing that grows"*. It grows immediately.
Ship three members from the start.

**S-3.** §6.5's claim about this lane's own core is **well-grounded** and I verified it firsthand, though
its line numbers have drifted. At `crates/oracle-core/src/bus.rs` (worktree `6568ca0`), the `stop_requested`
doc block reads verbatim: *"a sink that raises its flag from `on_step_boundary(pc, _)` gets classic
breakpoint semantics (stop *before* `pc` runs)"*, preceded by *"The machine always stops **at an instruction
boundary, never mid-instruction**, with `pc` pointing at the instruction that has *not* yet executed."* The
CR cites `bus.rs:305-318`; it is now around 338–346. The claim survives; the citation does not. ⟨RUNTIME⟩
per §11.8 — a source claim is not a runtime confirmation and this must not be asserted in a handshake until
someone runs it.

### A5 — `wait_for_break` (§8): three parts, three different verdicts

#### A5(2) — the bound

# MOOT — landed at §11.24, on a better method than CR-A used.

`protocol.md`:929 at `a69327f`: `` `timeoutMs`? (≥0, def 30000, ≤300000; refused above) ``, and the audit's
D-07 ruling: *"The default is the measured legacy behaviour (oracle-old `90f40b8`
`getInt("timeout_ms", 30000)`)… the ceiling is five minutes, refused not clamped."* Also landed from
§9.2: `running` struck (D-05) and `symbolDisp?` added (D-08), both as CR-A noted in passing.

CR-A's two invented numbers are superseded and were beaten by a measurement. Its 10 000 ms default,
*"derived by analogy"* from a frame bound, is replaced by a value read out of the incumbent server. Its
`limits.maxWaitMs` is replaced by a hardcoded 300 000 — which, note, comfortably clears CR-A §8.2(2)'s
*"a server serving aeon's gates must advertise ≥ 120 000"*. Nothing is owed here. See the §11.6 judgement
for what this says about §2.2's scope choice.

#### The D12 carve-out (§8.2(2) / §9.5)

# ADOPT — and it is MORE urgent than when drafted, because §11.24 landed on top of the defect without seeing it.

**This is the sharpest live finding in the whole CR and I am upgrading it.** At `a69327f`,
`protocol.md`:161–163 is **unchanged**:

> **D12 — Every wait-shaped op is bounded, and reports whether it fired.** Any method that runs the
> machine until a condition — `emulator/run_to`, `emulator/run_to_scanline`, and any future
> `wait_for_break`-shaped op — MUST accept a `maxFrames` bound (default **600**) and MUST return
> `reached` (boolean) beside its echo of the target.

And `protocol.md`:929, landed 2026-08-26, gives `wait_for_break` a `timeoutMs` bound, **no `maxFrames`**,
and **no `reached`**. §11.24 closed D-07 without touching D12 and did not record noticing it. **The
contract now contradicts itself in two live clauses**, where before CR-A was drafted it merely
under-specified one. CR-A found this before it existed.

CR-A's reasoning is correct and §14.2 sharpens it correctly: *"A `maxFrames` bound is a bound in EMULATED
time; a wedge is the state in which emulated time stops advancing."* The bound cannot trip in the one
failure it would have to catch.

**M-7 (material) — take §14.2's framing over the draft's.** §14.2 is right that the ask should be more than
"a one-sentence carve-out", and right that the precedent already exists. I verified it: `protocol.md`:1577
(moved from 1432) still reads *"**D12 does not apply** — the stop condition is an exhausted count, not a
predicate, so there is no `reached`."* That is the template. Either D12 distinguishes emulated-time from
wall-clock bounds for wait-shaped ops, or `wait_for_break` is scoped out on the `play_input` precedent.
Either closes it; the second is smaller and I would take it.

**M-8 (material) — re-file this against §11.24, not against the pre-amendment row.** As drafted, §9.5 asks
for a carve-out to a rule that was then merely unsatisfied. It is now *violated by landed text*. That
changes the ask from a clarification to a **contract-consistency defect report**, and it should be filed as
one, with `protocol.md`:161 and `protocol.md`:929 quoted side by side. This is the piece of CR-A I would
route upstream tonight regardless of what happens to the rest.

#### A5(1) — the transport non-serialisation rule

# ADOPT WITH CHANGES — split it out, and de-prioritise it.

**M-9 (material) — split.** I act on §11.5, which is the best-calibrated entry in CR-A's whole
self-assessment: *"'A server MUST NOT serialise replies on a connection' governs §2 (the envelope), not §6
(a method)… An adjudicator could reasonably split it into its own CR against §2 — and if so, **CR-A's
breakpoint half should still land**."* Split it. §11.5 pre-authorised this and it is correct.

The property itself I find **sound**: §8.1's argument that a blocking call makes the client-side timeout
unenforceable, *"destroying the property the call exists for"*, is right, and the survey's §4.6 grounds the
cost honestly (*"our server is synchronous by design: `Engine::dispatch`… every run method runs the machine
*inside* the handler"* — verified present in the survey at `docs/2026-08-22-acceptance-21-survey.md`:557–562;
the `engine.rs:984` line number has drifted, `fn dispatch` is now at 1378).

**S-4 — but the urgency claim must be rewritten, because this lane's own ledger has refuted it.** See §6
finding F-4: `docs/decisions.jsonl` `d-5` (2026-08-24) establishes that every consumer of this surface
spawns its own private legacy emulator on its own socket. There is **no multi-client contention on any live
socket today**. The rule is still right; the "this is the biggest unknown in the acceptance parcel" framing
is a *pricing* fact about building the successor's transport, not evidence of a live harm. Say which.

#### A5(3) — `emulator/wait_cancel`

# REJECT — with a named reopening condition.

Four grounds, in descending weight:

1. **It has no meaning under either server that exists.** §8.2(3) states its own dependency: *"the cancel
   cannot be keyed on a handle: under (1) the wait has no immediate reply in which to return one."* A5(3)
   is therefore conditioned on A5(1), which I have just split out as unscheduled. A method whose
   specification presupposes a transport model neither implementation has is not ready to be contract text.
2. **It grows a deprecated method's surface.** `protocol.md`:760 retains `wait_for_break` *"for one
   transition window"*, and §8.4(d) itself quotes the retention obligation. Adding a sibling **method** — a
   63rd fragment — to a row on its way out is backwards. §11.21 and §11.24 both took the opposite posture
   toward retained/frozen surface, deliberately.
3. **Zero demand.** §14.4's census — five files, sixteen call sites, enumerated firsthand across all of
   `tools/` — found not one caller that needs to stop waiting early. Every wait is bounded by a timeout the
   caller chose. CR-A applied exactly this test to reject `breakpoint_set_enabled` in §7.2 (*"a fourth
   method with **zero consumers anywhere in the workspace**"*) and should apply it to itself here.
4. **The gap it was sized against has since closed.** §8.3's dead-client case is *"the mechanism for a
   client that died… without a server-side bound the server holds that wait forever."* §11.24 landed that
   bound: `≤300000`, refused above. The server-driven release now exists; only the client-driven one is
   missing, and no client has asked.

§8.3's general principle — *"every long-lived object on this bus needs both a client-driven release and a
server-driven one"* — is a good principle and I do not dispute it. It just does not, on this method,
outweigh four grounds against.

**Reopening condition, named so it is not lost:** land A5(1) (non-serialised replies) on a server that
actually serves `wait_for_break`, then re-raise A5(3) with a consumer that needs it. Until then it is a
solution to a problem nobody can currently have.

### A6 — the cap, and `capabilities.breakpoints` as an object (§7.1)

# SPLIT: the cap is ADOPTED (landed). The capability-object placement is REJECTED — and it was wrong at CR-A's own base revision.

**The cap landed, verbatim as asked.** Schema at `a69327f`, `limits.maxBreakpoints`: *"the server REFUSES
at this count with `-32005 {reason:'breakpointCapReached', cap, count}` and MUST NOT silently grow past the
advertised number."* CR-A §7.1's *"This is **D13 rule 3 verbatim**, applied to a third object"* was right
and won.

**The placement is rejected.** §11.21 design choice 3 (`protocol.md`:4066–4071):

> **Cap via `limits`, not via widening `capabilities.breakpoints`.** The watch surface put its cap on an
> object-valued capability; the breakpoint capability is already a **boolean** that shipping clients read,
> and §11.18 says an emitted shape cannot be widened under a client that already parses it.

**M-10 (material) — and this is a defect at CR-A's own base, not drift.** §11.18 landed **2026-08-21**, one
day *before* CR-A's base revision `9d6ab1f` (2026-08-22 16:43). The rule was available to the drafter and is
never cited or addressed. A6 asserts the change *"matches `checkpoints` and `watchpoints`"* — true of the
shape, silent on the fact that those two were *born* objects while `breakpoints` shipped as a boolean.

**And there is a concrete silent-wrong-answer A6 would introduce that neither CR-A nor §11.21 spells out.**
A6 specifies that a non-serving server emits `{"supported": false}`. In every language a client here is
written in, **a non-empty object is truthy** — so a shipping client testing `if caps["breakpoints"]` flips
from *correctly reporting "not served"* to *wrongly reporting "served"* the moment a server adopts A6. That
is not a parse failure a client would notice; it is a believable wrong answer, which is the exact class
§6.2 of this same CR is written to prevent. **A6's placement is refuted by A4's own argument.**

**S-5.** §7.1's ⚠ note about the legacy server inferring wire error codes from message substrings is now
moot in effect — §11.21 design choice 4 asks the legacy server to change nothing (*"Legacy is frozen, not
migrated"*), and §11.21's closing line is *"The legacy server is asked to change nothing."* The hazard was
correctly flagged; it no longer has an occasion.

### A7 — the remaining rulings (§7.2–§7.8): five adopted, two rejected

| Sub | Proposal | Verdict | Basis at `a69327f` |
|---|---|---|---|
| **A7(a)** | strike `enabled` | **REJECT** | Landed contract keeps `enabled`, adds `breakpoint_set_enabled`, **and** adds `enabled?` to `breakpoint_add` (`protocol.md`:1087–1089) |
| **A7(b)** | duplicate add → a second distinct breakpoint | **ADOPT — landed, and CR-A was right against the audit** | `protocol.md`:1102–1107 |
| **A7(c)** | clear of unknown handle → `removed: 0` | **ADOPT — landed** | `protocol.md`:1120–1123 |
| **A7(d)** | `breakpoint_list` takes no cursor/limit | **REJECT** | Landed with `cursor`?/`limit`? (`protocol.md`:1089) |
| **A7(e)** | `label` | **ADOPT — landed** | `protocol.md`:1087, 1089 |
| **A7(f)** | `hits` kept, never reset, not renamed | **ADOPT — landed** | `protocol.md`:1112–1113 |
| **A7(g)** | `-32012`/`-32013` on the `symbol` spelling | **ADOPT — landed** | `protocol.md`:1114–1116 |

**A7(b) deserves explicit credit.** CR-A ruled *against* D-12's written recommendation (*"pin the idempotent
reading"*), on the grounds that the recommendation *"was reasoned inside an **address-keyed** model."* The
landed contract agrees and uses CR-A's own move: *"this closes audit D-12: the question 'is a re-add
idempotent' dissolves once the identity is the handle and not the address"* (`protocol.md`:1105–1106), and
§11.21 design choice 1 says it *"dissolves D-12 instead of answering it."* This is CR-A's clearest
independent win and it changed the contract.

**A7(a) — why the rejection is not close.** CR-A lost this three ways at once: `enabled` was kept,
`breakpoint_set_enabled` was added (the half of D-13(a) CR-A declined), and `breakpoint_add` gained an
`enabled?` param CR-A did not contemplate. §7.2's minimality argument — *"as long as nothing writes it,
`enabled` is a constant `true`"* — is answered by the obvious rebuttal CR-A does not consider: **make
something write it**, which is what the audit recommended and what landed. And §11.21 supplies the consumer
CR-A said did not exist: *"Making `breakpoint_add` take `enabled?` too is a convenience for arming a batch
disarmed."* **M-11 (material): withdraw A7(a); the deferred-follow-up framing in §7.2 is now backwards, and
the "reopening condition" it names has already occurred.**

**A7(c) — adopted, with a caution worth recording.** §7.4 argues from a *unanimous house precedent*
(*"an error a client must learn to swallow teaches clients to swallow errors"*). §11.21 landed that rule
for `clear` — and **deliberately broke it for `set_enabled`**: *"`set_enabled` refuses with
`-32005 {"reason":"unknownBreakpoint"}` (a client that thinks it is toggling something must learn it is
toggling nothing)"* (`protocol.md`:1120–1122). CR-A's rule is right for the operation it was written about
and would have been **wrong if generalised**, which §7.4's "house precedent is unanimous" framing invites.
The distinction — a *release* of something you may not hold is idempotent; a *mutation* of something you
believe you hold is not — is better than either document states. **S-6: carry that distinction forward.**

**A7(d) — why the rejection stands, and why the argument should be withdrawn rather than re-filed.** §7.5
offers a "third reading" of D-14: policy-bounded (so clause (a) applies) but always single-reply (so no
cursor). The landed contract took the sibling-consistent shape instead. Beyond consistency, **the premise is
unsound**: `limits.maxBreakpoints` is a server-policy number with no contract floor or ceiling (see Q6), so
the contract cannot know the cap is smaller than a page, and CR-A's proposed normative *"MUST return every
live breakpoint in one reply"* would bind a server whose operator set the cap to 100 000. §2.4 clause (b)
forbids **emitting** a cursor you do not **accept**; it does not require dropping one you do accept.
**M-12 (material): withdraw §7.5's third reading. Do not re-file it.** (It also contains an internal
inconsistency — see finding F-1.)

---

## 3. Judgement on §11 — the CR's self-assessment, subsection by subsection

Authors are unreliable narrators of their own weak points in both directions. Overall: **§11 is unusually
honest and above the median for this kind of document**, and its two most useful entries (§11.5, §11.7) I
have simply executed. Its characteristic failure is **understating by mis-locating** — naming a real soft
joint but diagnosing the wrong thing about it, which is a subtler failure than either over- or
under-claiming.

| § | Weakness claimed | My judgement |
|---|---|---|
| 11.1 | The procedural objection could void the CR | **Correctly identified, mildly OVERSTATED** |
| 11.2 | A3's REQUIRED handle has no capability gate | **Correctly identified; resolved by an answer §11.2 does not name** |
| 11.3 | The plural deviates from the commissioning ruling | **Correctly identified but UNDERSTATED — the framing is falsely binary** |
| 11.4 | Striking `enabled` removes published surface | **UNDERSTATED — and the outcome confirms it** |
| 11.5 | A5(1) is a transport rule in a method CR | **Correctly identified; best-calibrated entry; executed** |
| 11.6 | Three invented numbers | **Confession accurate, diagnosis UNDERSTATED** |
| 11.7 | Three levels may be one too many | **Correctly identified; I adopt its own recommendation** |
| 11.8 | Unverified at runtime | **Correctly identified; incomplete — one hazard missing** |

**§11.1 — mildly overstated.** Raising an objection against your own interest is the right instinct and
earns credit. But the alarm level ("could void this CR entirely") exceeded the evidence the drafter already
held: the audit's D-13 **Recommendation** at `audit`:217–220 reads *"raise a change request that brings the
breakpoint surface up to the watchpoint surface's shape."* A clause that commissions a CR by name is poor
evidence that raising it is out of order. §14.1 supplies exactly this and is right to.

**§11.2 — correctly identified; the resolution came from outside both options it names.** §11.2 posed a
binary: keep REQUIRED and accept invalidating the legacy server's event plus `protocol.md`:321's own
example, or retreat to OPTIONAL-gated-by-capability. **§11.21 found a third answer neither option
contains**: `breakpoint` is *"'additive' to the **document** and **REQUIRED on the handle shape**"* — gated
by *shape*, discovered via `methods`-list presence of `breakpoint_set_enabled`, with the legacy server
declared *"frozen, not conformed"* and **not validated** by the schema for that event. And the cited
casualty is repaired: `protocol.md`:324 now reads
`{"reason":"breakpoint","breakpoint":"bp-3","pc":"0x00012A4C",…}` (audit D-34, closed). The soft joint was
real; the CR's own named fallback was worse than what landed.

**§11.3 — understated, and it is the most consequential §11 entry.** It correctly says A3 and A7(b) *"must
be ruled together"* — right, and I have. But it frames the space as *plural, or refuse duplicates*, and the
option that actually defeated the plural — **allow duplicates and publish a deterministic tie-break** — is
absent from the CR entirely. Because §11.3 did not enumerate it, §5.2's whole argument is built against an
"arbitrary pick" that no competent implementation was going to ship. The residual defect I identify in M-2
is what §11.3 *should* have found: not that a singular field picks wrongly, but that it reports
**incompletely** while `hits` moves on handles the event never names.

**§11.4 — understated in three ways.** (i) It calls the rejection *"a minimality argument that is a
judgement call, not a derivation"* — fair — but omits that it is declining **an item the commissioning
Recommendation names by name**; §14.1 supplies that correction and is right that it *"is a higher bar than
an open design choice."* (ii) It repeats §7.2's *"zero consumers anywhere in the workspace"*, which is an
argument from a survey of current callers of a family **the successor does not serve at all** — an absence
of callers for an unserved method cannot establish absence of demand, and CR-A's own §5.1 makes precisely
this point about `reason: "watchpoint"` being *"an enum member §3 has always defined and no catalogued
method could produce"* until CR-11 made it producible. (iii) It never contemplates the use §11.21 found
(`breakpoint_add {enabled: false}` to arm a batch disarmed). The outcome — rejected on the contract floor —
confirms the weakness was larger than admitted.

**§11.5 — correctly identified and correctly calibrated.** The best entry in §11: it names the category
error, names the right remedy, and pre-authorises the split while protecting the rest (*"CR-A's breakpoint
half should still land"*). Executed as M-9.

**§11.6 — the confession is accurate; the diagnosis is understated, and the better lesson is one line
away.** §11.6 owns that 10 000 ms is *"derived by analogy from a frame bound and is otherwise arbitrary"*.
True. What it misses: **the number did not need inventing.** §11.24 sourced it from a measurement —
oracle-old `90f40b8` `getInt("timeout_ms", 30000)`. That measurement was available to the drafter, and
§2.2's *"The legacy C++ server was not read here"* is the deliberate scope choice that forced the
invention. So the weakness is not "I invented three numbers"; it is **"I declined to read the source that
would have supplied one, then invented a substitute and labelled the invention rather than revisiting the
scope."** That is the transferable lesson and §11.6 does not draw it. (§2.2's other two exclusions were
sound: no emulator, and aeon taken on the requesting overseer's firsthand verification.)

**§11.7 — correctly identified.** I adopt its recommendation verbatim (M-5). This is what a useful
self-assessment looks like: it does the adjudicator's work and gets it right.

**§11.8 — correctly identified, appropriately scoped, and incomplete by one item.** The claim is honest and
the ⟨RUNTIME⟩ discipline is right. I verified the underlying source text firsthand and it holds (see S-3),
which strengthens the CR without discharging the tag. **What §11.8 should also list:** the same doc block
in `bus.rs` warns that on a stopping iteration `on_step_boundary` *"is called for an instruction that does
not run, and it is called again for that same PC when the caller resumes… a counting sink must account for
it."* That is a live **double-count hazard for `hits`** — the field §7.7 rules on and §11.21 landed
(*"`hits` counts firings while enabled"*). A naive breakpoint sink on this core counts one stop twice. It
is an implementation hazard rather than a contract defect, but §11.8's list of *things read off source and
not confirmed* is exactly where it belongs. ⟨RUNTIME⟩.

**What §11 misses entirely.** Nothing in §11 flags the internal inconsistency between §7.2 and §7.5 (F-1
below) — the one place where two of CR-A's own sections apply opposite rules to the same phenomenon one
page apart. That is the most instructive omission in the self-assessment: §11 catches the joints the
drafter *argued about*, and misses the one they never noticed they had taken two positions on.

---

## 4. The seven open questions of §12

### Q1 — Does the audit's "the answer belongs to the legacy server" clause reserve ruling authority on D-13?

**NO. CR-A was in order. §14.1's resolution is CORRECT, and it is now moot besides.**

The clause, verbatim at `a69327f` `audit`:31–32 (unchanged from `9d6ab1f`):

> **"Which implementation has this been built against?" has two different answers**, and for D-10, D-13
> and D-17 the answer belongs to the legacy server, not to the lane that owns the successor. Do not
> adjudicate those three as if one implementer speaks for both.

Three grounds, the third of which §14.1 could not have had:

1. **The operative instruction is a scope binding, not a reservation of authority.** *"Do not adjudicate
   those three as if one implementer speaks for both"* constrains *what a ruling may assume about whose
   behaviour is described*. It does not name a party who alone may rule. §14.1 reads it this way and is
   right.
2. **The same audit commissions this CR by name.** `audit`:217–220: *"**Recommendation:** raise a change
   request that brings the breakpoint surface up to the watchpoint surface's shape — handle,
   `breakpoint_set_enabled`, `capabilities.breakpoints.maxBreakpoints` with
   `-32005 {reason:"breakpointCapReached"}`."* And `audit`:536 speaks of *"the eventual amendment"* as a
   presupposition. A document cannot both commission a change request and forbid its being raised.
3. **Settled by conduct, decisively, since §14.1 was written.** empyrean — the contract owner — **itself
   ruled D-13** as §11.21 on 2026-08-26, and its own text credits *"the oracle lane's peer answer there,
   **which corrected the history**"* (`protocol.md`:4047). The contract owner did not merely permit
   the successor lane's input on D-13; it adopted it and said so. The procedural worry is closed by what
   actually happened.

**One correction to §14.1, S-level:** its citations *"lines 193–195"* and *"line 481"* were correct at
`9d6ab1f` and are stale at `a69327f` (now 217–220 and 536; the audit gained 76 lines). Its substance is
unaffected.

**And one thing §14.1 gets right that deserves preserving:** *"nothing ruled here binds the legacy
server."* That is now the landed position too — §11.21 design choice 4, *"Legacy is frozen, not migrated"*,
and its closing *"The legacy server is asked to change nothing."*

**On §14.1's caution about A7(a):** §14.1 warns the adjudicator that rejecting `breakpoint_set_enabled` is
*"declining a specific item the commissioning text asked for, which is a higher bar than an open design
choice."* That warning was correct and I have applied it — see A7(a), rejected.

### Q2 — Should `stopPrecision` extend to `reason: "step"` and `reason: "watchpoint"`?

**YES to both, and `watchpoint` needs a THIRD enum member. This is also the reason A4 must not be a
breakpoint CR (M-3).**

**On `watchpoint`,** and this is not a hypothetical: the imprecision is documented **in this lane's own
core**, today. `crates/oracle-core/src/bus.rs`, the `stop_requested` doc block, worktree `6568ca0`:

> a sink that raises it from `on_event`/`on_vdp_write` — i.e. in the middle of an instruction — stops at
> the *next* boundary, **after the triggering instruction has fully committed.**

CR-A §12.2 already knew the contract says this (*"§11.8 already pins a watch stop as landing *after* the
triggering instruction commits, which is a *documented* imprecision that arguably wants the same typed key
rather than the prose it currently has"*). It is right, and `§2.4` rule 3 settles it rather than merely
supporting it: *"Any consequence a client must act on needs its own typed key."* A client reading register
state after a watch stop **must** act on whether the triggering instruction committed. It lives in prose
today. That is what rule 3 forbids.

**But the honest value is not `"approximate"`.** The watch stop is *precisely characterised* — one
instruction boundary later, always, by construction. Reporting it as *"the server promises **nothing**
about which side of it or by how much"* would be a **worse** answer than the prose it replaces. So:
`"exact" | "afterCommit" | "approximate"` (spelling is the drafters'). This vindicates §6.6(a)'s rejection
of a boolean — *"a granularity vocabulary is exactly the kind of thing that grows"* — and it grows on the
very first extension.

**On `step`:** include it. On the successor the answer is `"exact"` by construction and it costs a key. It
buys the thing that makes §6.4's general rule enforceable instead of a three-reason special case: **every
`reason` that reports a `pc` carries a precision**, so a subscriber never has to know which reasons opted
in. §5.3's own argument applies with full force — an event field added later is permanently optional.

CR-A's *"widening it is cheap now and expensive later"* is correct. Widen it now. **M-6.**

### Q3 — Should there be a scoped teardown, `clear {all: true, mine: true}`?

**NO. Settled here — do not raise it.** Three grounds, the third dispositive and named in neither document:

1. §11.21 has **just landed** `all` as *"the one deliberately shared verb"*, with the sharing stated as the
   point rather than as a regret. A `mine` scope re-opens a clause ruled one day ago.
2. It requires a connection-identity concept the contract does not have, as §12.3 concedes — and the harm
   it would buy down is small: the measured incident in §2.4 was an **agent judgement** failure (clearing
   what it could not attribute), which `label` — landed — addresses at the cause.
3. **`{all, mine}` cannot do the job `all` exists for.** `all`'s whole justification (§4.2) is the client
   that **crashed** and cannot enumerate what it armed. The session that cleans up after it is a *different
   connection*. Under `mine`-scoping, the dead client's breakpoints are not "mine" to its successor — so
   the scoped variant is **precisely blind to the crash-path case that motivates the primitive.** The two
   asks are not merely in tension; the second negates the first.

### Q4 — Should `watchpoint_list` lose its `cursor`/`limit`?

**NO. Settled here.** Two grounds:

1. **The inconsistency §12.4 worried about does not exist.** It asked because CR-A proposed a cursorless
   `breakpoint_list` and feared *"the two lists will look inconsistent until someone rules."* §11.21 landed
   `breakpoint_list` **with** `cursor`?/`limit`? (`protocol.md`:1089), mirroring `watchpoint_list` and
   `checkpoint_list`. All three now agree.
2. **The underlying premise is unsound anyway.** *"A capped collection needs no continuation"* assumes the
   cap is smaller than a page. `limits.maxBreakpoints`, `watchpoints.maxWatches` and `checkpoints.cap` are
   all **server-policy numbers with no contract floor or ceiling** (see Q6), so the contract cannot know
   that. §2.4 clause (b) forbids **emitting** a cursor you do not **accept**; it says nothing against
   accepting one.

Withdraw §7.5's argument rather than re-filing it against watches (**M-12**).

### Q5 — Should `watch` on `stopped` become plural, matching `breakpoints`?

**I decline to settle the substance, and I am declining for a reason that changes the question.**

Why I decline: `watch`'s multiplicity semantics are §11.8's surface, that surface **has a live consumer**
(CR-A §5.2's own reason for not proposing it), and I have neither read that consumer nor the watch
implementation. Ruling it from the breakpoint side would be exactly the "one implementer speaks for both"
error Q1 is about. **Who should settle it: empyrean as contract owner, on a defect report from the oracle
lane, with the watch consumer consulted.**

**But the question has inverted since it was written, and that is a finding worth relaying.** CR-A framed
it as a *symmetry* worry — `breakpoints` plural vs `watch` singular. What landed is `breakpoint`
**singular with a published tie-break rule** (*"the earliest-added enabled breakpoint at that address"*)
and **no equivalent rule for `watch`**. So the asymmetry is now the reverse of the one predicted:
`breakpoint`'s behaviour under multiplicity is **specified**; `watch`'s is **unspecified**. Two watches
whose ranges overlap and whose `stopAfter` thresholds cross on one access produce one `stopped` event
naming one `watch`, and the contract does not say which. **That is a live under-specification in landed
text**, not a cosmetic inconsistency, and it should be raised as such — a one-sentence amendment stating
the tie-break, or the plural. **M-13.**

### Q6 — `maxBreakpoints`: is there a house number?

**NO house number, and none should be invented. Settled here — no change needed.**

Grounds: neither sibling cap carries a floor (`checkpoints.cap`, `watchpoints.maxWatches` are both server
config), and a floor would have to bind a server §11.21 has just declared frozen. More importantly, the
landed schema gets something a floor could not: **absence is made meaningful.** From
`limits.maxBreakpoints`' description at `a69327f`:

> OPTIONAL, and its absence is meaningful: a server that omits it serves the PRE-AMENDMENT breakpoint shape
> (§11.21 design choice 3).

A client that needs N breakpoints reads the number and refuses if it is short — the same contract the two
siblings already offer, and better discovery than a floor would give.

**§11.6 is right that "this CR mandates a cap without saying what a reasonable one is", and that is fine.**
A cap's *discoverability* is the contract's business; its *value* is the operator's. That distinction is
already the house position on two objects and CR-A should simply say so instead of apologising for it.

*(One thing I found while answering this, reported below as F-6: the landed text now gives **two**
discovery mechanisms for one fact and does not rank them.)*

### Q7 — Should the `errors` sub-object be raised now?

**YES — raise it, as its own CR, and the case is measurably stronger than CR-A knew. But it is empyrean's
to raise, not this lane's.**

**§9.7's structural claim survives the drift intact — I re-derived it by parsing at `a69327f`:**

```
62 fragments:  58 with keys exactly ($comment, params, result)
                4 with keys exactly (params, result)
fragments containing an "errors" key: 0
```

CR-A measured this at 58 fragments; at 62 it is still true of every single one. Every error obligation on
this bus lives in prose and **cannot be validated against a fragment**.

**And the hole grew on its own, twice, in the four days since CR-A was drafted:**

- §11.21 registered **two new `-32005` reasons** — `breakpointCapReached` and `unknownBreakpoint` — in
  prose only.
- §11.24 made four things refusable (`step.count: 0`, `step.count: 1000001`, `timeoutMs: 300001`, an empty
  `step_over`/`step_out` result). The *param* refusals are schema-checkable; the **error shape** each
  server must return is not.

So CR-A's speculative framing — *"adopting CR-A widens a known hole"* — is superseded by a measurement:
**the hole widens roughly every amendment, whether or not CR-A lands.** That converts a sequencing worry
into a rate, and a rate is an argument. §9.7's *"a conformance suite that validates replies against
fragments is **blind to all four**"* is exactly right and now understates the count.

**Who:** it edits every fragment and the gate, both of which empyrean owns. CR-A's instinct that *"it
should not ride along"* is correct and I affirm it. **Sequencing: it does not block anything above.** Do
not hold A4 or the D12 defect behind it.

---

## 5. Drift report

**CR-A was drafted against `9d6ab1f` (2026-08-22 16:43). `origin/main` is now `a69327f` (2026-08-27
01:43).** In between, **14 commits touched `contract/protocol.md`** (+803 lines) and the schema grew by
+6062 lines. The audit gained 76 lines.

### 5.1 The load-bearing drift: §14.3 is now false

§14.3 states, as a verified-firsthand currency check:

> Checked **at tip**, which is the correct direction for a currency question: `9d6ab1f` is a real commit,
> an ancestor of `origin/main`, and **zero commits have touched `contract/protocol.md` since it** — so
> every protocol citation in this document is current… The anchor is good.

**Two of those three clauses hold; the third does not, and it is the one the conclusion rests on.**

```
$ git log --oneline 9d6ab1f..a69327f -- contract/protocol.md | wc -l
14
```

**I judge §14.3 to have been true when written and false now.** The earliest of the 14 is `252fe46` at
2026-08-22 21:32 — about five hours *after* `9d6ab1f` and plausibly after §14.3 was checked. This is not an
error of care; it is a **perishable claim recorded as a durable one**, which is a pattern this repo's own
history names (`empyrean` `9f14d8c`: *"Q-21 candidate: perishable claims in artifacts nobody re-reads"*).
The lesson for the re-cut: **a currency check must carry the revision it was true at**, not the word
"current".

### 5.2 The substantive drift: §11.21 (CR-BP) and §11.24 landed the CR's own subject matter

| Landed | Date | Effect on CR-A |
|---|---|---|
| `f6004b8` §11.21 DRAFT (CR-BP): handle, set_enabled, cap | 08-25 21:56 | Occupies A1, A2, A6, A7 |
| `37a015c` §11.21 schema M2 (58→59 fragments) | 08-25 | Occupies §9.6 |
| `8f92fe0` §11.21 closes; §2 example carries a handle (D-34) | 08-25 22:08 | **Removes §10/§11.2's cited casualty** |
| `fc7d7a5` §11.24 batch B1 (D-05, D-06, **D-07**, D-08 …) | 08-25 22:42 | Occupies A5(2), §9.2 |
| `21dd6da` *"audit rows D-12..15 marked closed by §11.21"* | 08-25 22:12 | **Falsifies CR-A's `Closes:` line** |
| `a0c50a1` §11.25 (CR-D) | 08-26 | 59→62 fragments; falsifies §9.6's "58 → 59" |

**Clause-by-clause, every quotation CR-A anchors a delta to has moved:**

| CR-A cite | At `9d6ab1f` | At `a69327f` |
|---|---|---|
| §9.1 `protocol.md`:1015–1017 breakpoint rows | ✅ **exact** | ❌ replaced; rows now at 1087–1090, four of them |
| §9.2 `protocol.md`:857 `wait_for_break` | ✅ **exact** | ❌ now line 929, `running` struck, bounds added |
| §9.3 `protocol.md`:637 `stopped` row | ✅ **exact** | ❌ now line 708, carries `breakpoint`? |
| §10 `protocol.md`:321 canonical example | ✅ **exact** (no handle) | ❌ now line 324, **carries `"breakpoint":"bp-3"`** |
| §9.6 "58 → 59 fragments" | ✅ 58 at base | ❌ **62** now; `breakpoint_set_enabled` already present |
| §14.1 `audit` lines 193–195, 481 | ✅ **exact** | ❌ now 217–220, 536 |
| §2.4 harm, `timing-ground-truth-fable.md`:162–165 | ✅ **verbatim** | ✅ **still exact** — 7 breakpoints, `0x5CAC8` (×2), `0x3C46` (1,691,410 hits), *"Restore if another workflow needs them."* |
| §9.7 "no fragment declares any error condition" | ✅ | ✅ **still true at 62** |
| §6.5 `bus.rs:305-318` | (not checked at base) | ⚠️ **claim exact, line numbers drifted** (~338–346) |
| §7.1 `engine.rs:1050` `"breakpoints": false` | (not checked at base) | ⚠️ **claim exact, now line 1473** |
| §8.2 `engine.rs:984` `Engine::dispatch` | (not checked at base) | ⚠️ **claim exact, `fn dispatch` now 1378** |

**I want this on the record: every empyrean citation I checked was exact at `9d6ab1f`.** CR-A's quotation
discipline was excellent. Nothing below is a criticism of its rigour at the time; it is a statement that the
ground moved.

### 5.3 Two things that did *not* drift, and both matter

- **The procedural clause** (`audit`:31–32) is byte-identical at both revisions. Q1 is answerable on the
  same text CR-A read.
- **D12** (`protocol.md`:161–163) is byte-identical at both revisions. A5's finding is untouched — and, as
  ruled above, is now *violated* by `protocol.md`:929 rather than merely unaddressed.

### 5.4 Drift inside this repo

`crates/oracle-aether/tests/contract/bus-protocol.schema.json` — CR-A's own vendored source, cited in §2.1
as "worktree at `6ad68ac`" with 58 fragments — **has already been re-vendored** (mtime 2026-08-27 01:44):
62 fragments, `emulator/breakpoint_set_enabled` present, `breakpoint_add` carrying the post-§11.21 shape.
**This lane's own tree already carries the amended surface that the CR proposing to amend the
pre-amendment surface is still asking for.**

---

## 6. What the CR gets wrong that it does not know it gets wrong

The category none of §11's eight subsections covers. **Four material, four stylistic.**

### F-1 (M) — §7.2 and §7.5 apply opposite rules to the same phenomenon, one page apart

§7.2 strikes `enabled` **because it is a constant**:

> As long as nothing writes it, `enabled` is a constant `true`, and a constant is a pure function of
> membership in the list. §11.10 struck per-entry flags on exactly that ground…

§7.5, three pages later, **requires a field that its own normative rule makes constant**:

> `total`, `returned`, `truncated` — clause (a), REQUIRED, `truncated` **even when false** … With a
> normative "MUST return every live breakpoint in one reply", `truncated` is always `false`…

Under CR-A's own proposal `truncated` is a constant `false` and a pure function of the row's normative
text. By §7.2's rule it should be struck. CR-A keeps it — **correctly**, because `§2.4` clause (a) at
`protocol.md`:577–578 requires it for a stated reason: *"absence and `false` must not both mean 'you have
everything'."* So the contract explicitly mandates a constant field, on the grounds that **a field's
information content is not the only thing that decides whether it belongs** — silence and a value must not
be confusable. That reasoning applies to `enabled` too, and §7.2 never meets it.

**This is the CR's deepest error and it is invisible to §11.** It is not that A7(a) lost; it is that CR-A
held a general principle ("strike constants") that its own §7.5 abandons and that its own strongest section
(§6.3, on why `stopPrecision` must be REQUIRED rather than defaulted) argues **against**. §6.3 says it
outright: *"a defaulting rule is a mechanism for exactly that."* Three sections, two positions, no
reconciliation.

### F-2 (M) — A6's boolean→object widening is a silent wrong answer, and was already forbidden at CR-A's base

Covered under A6/M-10. In short: `§11.18` landed 2026-08-21, **before** CR-A's base revision, and CR-A
neither cites nor addresses it; and `{"supported": false}` is **truthy**, so every shipping client testing
`if caps["breakpoints"]` silently flips from a correct "not served" to a wrong "served". This is a defect at
CR-A's own base, not drift — it was findable on the day it was written.

### F-3 (M) — §14.4 item 2 does not support the conclusion it is presented as supporting

Covered under A3/M-1. Two breakpoints on **different addresses** are disambiguated completely by a
**singular** handle. §14.4 calls this *"the concrete case for the plural `breakpoints` array"* and *"the
strongest single argument in this CR"*; it is neither. It is a good argument for the field being **REQUIRED**
— which §14.4 also makes, correctly, and which won.

**The irony is structural and worth naming:** §14.4's own closing lesson is that *"verifying a claim ADOPTS
ITS FRAME unless you deliberately widen it"*, delivered after a census that was wrong three times. The
census, once corrected, was then read through the frame the debate had already established (*"plural or
singular?"*) rather than asked what it actually showed. **The corrected evidence was fitted to the standing
conclusion.** That is the same failure one level up, inside the section written to warn about it.

### F-4 (M) — the entire consumer analysis measures consumers of a server that will never implement any of this, and this lane's own ledger has already said so

CR-A §1: *"**What it costs the only automated consumer of this surface: nothing.** See §2.3 — this was the
most surprising finding in preparing this CR and **it materially lowers the risk of A1**."* §14.4 widens it
to 16 call sites across five files and calls the zero-cost result *"a much stronger basis than it had."*

**`docs/decisions.jsonl` `d-5`, dated 2026-08-24T02:41:01Z — two days after CR-A, three days before this
ruling — refutes the premise, in this repo, in this lane's own hand:**

> **REFUTATION: aeon's gates spawn their own emulator on their own socket.**
> `oracle-old/linux-port/harness/launcher.py:11` GUI = `linux-port/build/oracle_gui` (**legacy C++, which
> SERVES breakpoints**), :40 `mkdtemp(prefix='oracle-harness-')`, :47 isolated HOME+XDG_RUNTIME_DIR, :58
> `sock = tmp/'oracle.sock'`. … Swept aeon/tools: **9 gate files spawn their own, 0 dial a shared socket**,
> zero `BusClient()` without an explicit `socket_path`.

And `d-5` names the error shape precisely: *"exposure inferred from which methods the gates CALL, never
from which server ANSWERS."* CR-A's §2.3 and §14.4 are both censuses of *which methods the gates call*.

**Combine that with §11.21 design choice 4 — *"Legacy is frozen, not migrated"*, closing with *"The legacy
server is asked to change nothing"* — and the consequence is sharp:**

- The "zero cost to the only automated consumer" finding is **true and vacuous**. The consumer is on the
  far side of a frozen boundary and will never be served the amended shape at all. It is not evidence that
  the migration is cheap; it is evidence that **there is no migration**, because there is no consumer to
  migrate.
- §14.4's item 2 (already refuted on its own terms in F-3) is an argument for a REQUIRED field **from a
  consumer that will never receive that field**.
- §14.4's item 3 (`parallax_hscroll_probe.py` calling clear-all 5:1 against arming) is the one that
  **survives**: it is evidence about *how debugging clients behave*, which generalises past which server
  answers. Keep it; drop the framing that it is evidence about *this contract's consumers*.

This lane already knows: `docs/lane-status.json` now titles CR-A *"Breakpoints + wait_for_break. **NO
consumer after all**: aeon's gates spawn their own legacy emulator, so nothing breaks without it."* **That
title has not been propagated into the CR**, where §1's headline still reads the other way. **Fix §1, §2.3
and §14.4 before any re-raise.** A CR whose summary line is refuted by its own repo's decision ledger
cannot be handed to a contract owner.

*(This does not weaken A1–A7 on the merits — they were argued from hazard and precedent, not from consumer
demand. It removes a risk-reduction claim the CR leans on twice and promotes to its §1 headline.)*

### F-5 (S) — §7.4's "house precedent is unanimous" would be wrong if generalised

Covered under A7(c). §11.21 landed idempotent success for `clear` **and** a loud `-32005
{"reason":"unknownBreakpoint"}` for `set_enabled`. The distinction — **a release of something you may not
hold is idempotent; a mutation of something you believe you hold is not** — is better than either document
states, and §7.4's framing invites the over-generalisation. Worth carrying forward as a rule.

### F-6 (S) — a finding against the *landed* contract, surfaced while answering Q6

§11.21 supplies **two** discovery mechanisms for one fact and does not rank them:

- `protocol.md`:1118 — *"a server that omits [`limits.maxBreakpoints`] serves the pre-amendment shape"*;
- `protocol.md`:1143–1144 — *"**`emulator/breakpoint_set_enabled` present means the handle shape**, absent
  means the address shape."*

A server that lists `breakpoint_set_enabled` in `methods` but omits `limits.maxBreakpoints` satisfies one
test and fails the other, and the contract does not say which governs. Not CR-A's defect — it postdates the
CR — but it is in scope for anything re-raised against this surface, and it is one sentence to fix.
**Route to empyrean.**

### F-7 (S) — §7's heading names a proposal it does not contain

§7 is titled *"A5–A7 — the remaining rulings"* and contains **A6 and A7 only**; A5 is §8. §0 promises the
document is navigable by *"a reader with no prior exposure to this repo"*, and this specific defect
propagated into my own dispatch brief, which asserted the CR carries "A1 through A5". A structural error
that has demonstrably misled a downstream reader is worth more than its size.

### F-8 (S) — bare section numbers collide between the CR and the contract it cites

CR-A has its own §2.4, §3, §5, §6, §11 — and cites the *contract's* §2.4, §3, §5, §6, §11 by bare number
throughout. §6.6 *"Rejected on §2.4 rule 3"* means the contract; §2.4 in this CR is "The measured harm this
surface has already caused". §7.1's *"the reason §11.8 already stated"* means the contract, while CR §11.8
is "Unverified at runtime". Each is resolvable with effort; collectively, in a document whose §0 stakes its
adjudicability on a cold reader, they are a real tax. **Prefix contract references** (`contract §2.4`) in
any re-cut. Cheap, and it is the single highest-leverage readability fix in the document.

---

## 7. Items I did not rule, and why

| Item | Why | Who should |
|---|---|---|
| **Q5's substance** — should `watch` on `stopped` be plural | `watch` is §11.8's surface, it has a live consumer, and I read neither the consumer nor the watch implementation. Ruling it from the breakpoint side is the "one implementer speaks for both" error Q1 forbids. | empyrean as contract owner, on a defect report from the oracle lane, with the watch consumer consulted. I have reframed the question (M-13) rather than answering it. |
| **Any runtime behaviour** | Invariant 1: no emulator, from a background agent. §6.5's stop-exactness claim, and the `on_step_boundary` `hits` double-count hazard I add to §11.8, are both **⟨RUNTIME⟩** — tagged for the controller's foreground follow-up. | Controller, foreground. |
| **The legacy C++ server's actual behaviour** | Not read here, same as CR-A §2.2. Every legacy claim below is carried on the audit's or §11.24's account, both of which cite pinned revisions (oracle-old `90f40b8`). | — |
| **aeon's 16 call sites** | Not re-derived. Carried on §14.4's firsthand enumeration *and* on this repo's `d-5`, which is a firsthand read of `launcher.py` by this lane. Note these two agree on the sites and disagree on what they imply — see F-4. | — |
| **Implementation cost of anything** | No cargo run, per invariant 2. Every cost statement here is the CR's or the survey's, attributed. | — |

---

## 8. Summary of required changes

**Material (13):**

| # | Against | Change |
|---|---|---|
| M-1 | §14.4 item 2 | Strike or correct: different-address breakpoints do not motivate a plural array |
| M-2 | A3 | Re-raise as the narrow same-address residual (hits move on handles the event never names); pick (a) prose or (b) plural |
| M-3 | A4 | Split `stopPrecision` out of the breakpoint CR; raise against §3/§6's stop surface |
| M-4 | A4/§7.1 | Re-site the handshake level off `capabilities.breakpoints` (still a boolean; and always the wrong home for a `runTo`-scoped key) |
| M-5 | A4 | Strike the arm-reply level — §11.7's own recommendation, adopted |
| M-6 | A4 | Three enum members, not two: `watchpoint`'s imprecision is characterised, not unbounded |
| M-7 | §9.5 | Take §14.2's framing: scope `wait_for_break` out of D12 on the `play_input` precedent (verified live at `protocol.md`:1577) |
| M-8 | §9.5 | Re-file as a **contract-consistency defect report** against landed §11.24 — `protocol.md`:161 vs :929 — not as a clarification |
| M-9 | A5(1) | Split the transport rule into its own CR against §2 (executing §11.5) |
| M-10 | A6 | Withdraw the `capabilities.breakpoints`→object placement; the cap landed in `limits` |
| M-11 | A7(a) | Withdraw; `enabled` kept, `breakpoint_set_enabled` landed, `enabled?` param added |
| M-12 | A7(d)/§7.5 | Withdraw the third reading of D-14; do not re-file it against watches |
| M-13 | Q5 | Raise `watch`'s unspecified multiplicity tie-break as a defect against landed §11.8 |

**Stylistic (8):** S-1 (A2's crash-path reason as prose residue), S-2 (keep §5.2's `watch` observation),
S-3 (re-anchor `bus.rs` line numbers; ⟨RUNTIME⟩ stands), S-4 (rewrite A5(1)'s urgency framing per `d-5`),
S-5 (drop §7.1's moot legacy-substring note), S-6 (carry forward the release-vs-mutation distinction),
F-7 (fix §7's heading), F-8 (prefix contract section references).

**Plus, prerequisite to any re-raise and not optional:** correct the `Closes:` line (all five defects are
closed), correct §14.3's currency claim, and correct §1/§2.3/§14.4's consumer framing per F-4.

---

## 9. Closing note on the quality of what I am ruling against

I have marked a lot of this superseded and two proposals rejected outright, so the balance is worth stating
plainly. **CR-A is a good document that lost a race.** Its quotation discipline was exact at its base
revision — I checked every empyrean citation and every one held. It labelled its estimates as estimates. It
raised, unprompted, a procedural objection that would have voided it, and called its own favourable reading
*"self-serving"*. It ruled **against the audit's written recommendation** on duplicate-add and was right,
and that ruling is in the landed contract. Its §11 identified five of the eight joints I would have found
independently, and two of its entries (§11.5, §11.7) I did nothing with except execute.

Its failures are concentrated in one place and share one shape: **claims about the world outside the
contract**. §14.3's currency check, §2.3/§14.4's consumer census, §7.2's "zero consumers", §11.6's invented
numbers — each is a case of reasoning from a scope that was inherited rather than chosen, exactly the
pattern §14.4's own method note names and then commits again. The textual work is strong; the empirical
work is where it broke, three times, in ways its own repo had already corrected twice.

**The single most valuable thing in it — A4's stop-precision rule — is unruled by anyone, unaffected by the
drift, and is currently blocked behind six proposals that are moot.** If one thing comes out of this
ruling, it should be that A4 gets raised on its own, this week.

---

*Ruled by `claude-opus-5` on 2026-08-27, substituting for the parked premium adjudicator seat
(`docs/decisions.jsonl` d-15/d-16). Sources read at `empyrean` `a69327f` and `9d6ab1f`, `oracle` worktree
`6568ca0`. No emulator was contacted; no cargo command was run; nothing in `empyrean` was written.*
