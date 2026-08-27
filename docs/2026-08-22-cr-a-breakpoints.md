# CR-A — the breakpoint surface: handles, teardown, attribution, and stop precision

**Raised by:** the oracle lane (the ground-up Rust core + Aether server, `oracle/`).
**Against:** `empyrean` `contract/protocol.md` §3, §6, and `contract/schema/bus-protocol.schema.json`,
read at `origin/main` `9d6ab1f`.
**Closes:** audit defects **D-12**, **D-13** (a, b, c), **D-14**, **D-15**, and — as a consequence rather
than as its purpose — **D-07**.
**Date:** 2026-08-22.

---

## 0. How to read this document

This CR proposes changes to a **contract**, not to a server. It is written to be adjudicated by a reader
with no prior exposure to this repo, so every claim below is either quoted from a cited source or marked
as a judgement. Section 1 is the summary; §2 is the evidence base and what was *not* checked; §3–§8 are
the substantive proposals, each with the alternatives that were rejected; §9 states the exact textual
deltas requested; §10 states what this CR does **not** bind; §11 is a self-assessment naming the places
where this CR is weakest, and §12 is the list of questions handed to the adjudicator unanswered.

**Two implementers.** The methods this CR touches are described by contract rows that were written from
the **legacy C++ server** (`oracle-old/`), which is the implementation that serves them today. The Rust
successor serves none of them. §10 states precisely what this CR asks of each. Nothing here is drafted as
though one implementer speaks for both, and §11.1 raises the procedural objection that could void this CR
entirely.

**A note on cost.** No code was compiled, run, or measured for this document. It is a design proposal.
Every cost figure is a judgement estimate and is labelled as one.

---

## 1. Summary

The contract's breakpoint surface is three rows in §6:

```
| `emulator/breakpoint_add` | `addr`\|`symbol` | `addr` |
| `emulator/breakpoint_list` | — | `breakpoints[]{addr,enabled,hits}` |
| `emulator/breakpoint_clear` | `all`\|`addr`\|`symbol` | `removed` |
```
*(`contract/protocol.md` lines 1015–1017.)*

Directly beneath them sit the watchpoint rows, which were rebuilt in 2026-08-15 (§11.8, CR-11/CR-12) and
now carry opaque handles, an idempotent handle-keyed clear, bounded lists, and an advertised cap refused
loudly. The breakpoint rows were not carried across. The audit calls the resulting gap **"the largest
single gap this pass found and it is too big to fold into a schema transcription"** (D-13).

This CR proposes seven changes:

| # | Change | Closes |
|---|---|---|
| A1 | `breakpoint_add` returns an opaque **handle**; `breakpoint_clear` addresses breakpoints by that handle, and `addr`/`symbol` are **removed** from `breakpoint_clear` | D-13(b) |
| A2 | `breakpoint_clear {all: true}` **survives** as a distinct teardown primitive, with its reason written into the contract so a future editor cannot "simplify" it away | — |
| A3 | `emulator/stopped` gains **`breakpoints`** (an array of handles), REQUIRED when `reason` is `"breakpoint"` | D-13(b) |
| A4 | A **stop-precision** declaration at three levels — handshake, arm-reply, and stop-event — making an imprecise stop impossible to mistake for a precise one | new (generalises past breakpoints) |
| A5 | `wait_for_break` is pinned as **non-blocking on the connection**, gains a bounded default and ceiling, and gains a cancel: `emulator/wait_cancel` | D-07 |
| A6 | An advertised **cap** with a loud refusal; `capabilities.breakpoints` becomes an object | D-13(c) |
| A7 | `enabled` is **struck** from `breakpoint_list`; the list takes §2.4's flat companions and no cursor; `hits` gets a reset rule; duplicate-add and clear-of-nothing are ruled | D-13(a), D-14, D-12, D-15 |

Two of these reverse a recommendation the audit itself made (A7 on D-12's idempotent reading, and the
partial rejection of D-13's `breakpoint_set_enabled`). Both reversals are argued at the point of use and
listed again in §11.

**What it costs the only automated consumer of this surface: nothing.** See §2.3 — this was the most
surprising finding in preparing this CR and it materially lowers the risk of A1.

---

## 2. Evidence base

### 2.1 Sources read, and at what revision

| Source | Read as | Revision |
|---|---|---|
| `empyrean/contract/protocol.md` | committed blob | `origin/main` `9d6ab1f` |
| `empyrean/docs/2026-08-22-protocol-schema-audit.md` | committed blob | `origin/main` `9d6ab1f` |
| `bus-protocol.schema.json` | vendored copy in this repo, `crates/oracle-aether/tests/contract/` | worktree at `6ad68ac` |
| `oracle/docs/2026-08-22-acceptance-21-survey.md` | worktree file | `6ad68ac` |
| `oracle/crates/oracle-aether/src/engine.rs` | worktree file | `6ad68ac` |

The vendored schema was parsed rather than eyeballed: `schema["methods"]` maps method name → fragment
**directly** (no `properties` sub-object) and holds **58** fragments plus one `$`-prefixed key.

### 2.2 What was not checked, and is therefore taken as given

- **No emulator was run and no cargo command was executed** for this document. Every behavioural claim
  about the Rust core is a claim about source text, cited by file and line.
- **The aeon tree was not read here.** The gate-script facts in §2.3 were verified firsthand by the
  requesting overseer against aeon's `origin/master` and are taken as established. They are the load-
  bearing external facts in this CR and an adjudicator who wants them independently confirmed should
  ask aeon, not this lane.
- **The legacy C++ server was not read here.** Its behaviour is cited only where the audit or the survey
  quotes it.

### 2.3 The only automated consumer, and what this CR costs it

Two aeon gate scripts drive this surface from a nightly systemd chain. Both run the same flow:

```
raster_source_gate.py:161   breakpoint_add   {addr: hex(probe_pc)}
raster_source_gate.py:168   wait_for_break   {timeout_ms: 120000}
raster_source_gate.py:173   breakpoint_clear {all: True}
snapshot_poison_gate.py:62  breakpoint_add   {addr: hex(addr)}
snapshot_poison_gate.py:64  wait_for_break   {timeout_ms: 20000}
snapshot_poison_gate.py:68  breakpoint_clear {all: True}
```

Two properties of these callers decide most of this CR:

1. **They never clear by address.** Every clear is `{all: true}`.
2. **They do not depend on breakpoint identity at all.** They assert *stop-PC* identity separately
   (`raster_source_gate.py:176`, `snapshot_poison_gate.py:70`), and a mismatch is a SETUP FAILURE with
   exit code 2 — never a verdict. Verified present in both.

**Therefore the handle migration (A1) requires zero changes to the only automated consumer of this
surface.** `breakpoint_add`'s params are unchanged; its reply gains a key these callers ignore;
`breakpoint_clear {all: true}` survives verbatim. The requesting overseer described the migration as
"a trivial rewrite"; it is not a rewrite at all.

*(The separate `timeout_ms` → `timeoutMs` param-spelling conflict on `wait_for_break` is **D-33**, is
already ruled upstream, and is deliberately out of scope here. This CR does not re-open it and does not
depend on it.)*

### 2.4 The measured harm this surface has already caused

Recorded in the audit's "verified-by-them" section and, per the survey (§7.2), in this repo at
`docs/2026-07-23-timing-ground-truth-fable.md:162-165`: an agent cleared **seven breakpoints it judged
"not mine"** — one of them at **1,691,410 hits**, and one address duplicated twice — while writing
"restore if another workflow needs them", which it **had no means to do**.

Every element of that incident is a defect this CR closes: it could not tell whose breakpoints they were
(A1's handle, and A7's `label`); it could not restore them (A1's handle plus A7's list); and the
duplicate address it observed is the empirical fact that decides A7's duplicate-add ruling.

---

## 3. A1 — handles are the addressing primitive

### 3.1 The proposal

`emulator/breakpoint_add` returns a server-assigned opaque handle. `emulator/breakpoint_clear` takes
that handle. `addr` and `symbol` are **removed** from `breakpoint_clear`'s params.

### 3.2 The argument

**Address-keyed clear is ambiguous the moment two subscribers arm the same PC, and clearing by address
silently disarms another subscriber's breakpoint.** This is not a hypothetical: §2.4 records it happening,
and the audit records that a duplicate address is **empirically possible** on the legacy server, so the
two-breakpoints-one-address state is reachable today.

This is the identical hazard the contract has already ruled on twice, both times the same way:

> Ids are assigned by the server, never proposed by the client, so two clients sharing one bus cannot
> collide or overwrite each other's coordinates. *(§6.1, on checkpoint ids)*

> **A watch is an opaque handle** (D9 category 4): a server-assigned string, **never reused**, so a stale
> handle resolves to nothing rather than silently to a different watch. It cannot be an address — one
> address may carry several watches […] *(§6, the §11.8 amendment)*

The breakpoint rows are the last object on this bus addressed by its own content rather than by an issued
identity. The workspace's direction of travel is more concurrent lanes against fewer emulators, which
makes the two-subscriber case the normal case rather than the exotic one.

### 3.3 The wire shape, and why

**Type: an opaque string**, `$ref: "#/$defs/handle"` — the existing definition, unchanged. Argued from
the contract's own reasoning rather than from taste:

> A type a client must never compute on should not be a number, because a number invites the computation
> this paragraph would otherwise have to forbid in prose. *(§6.1, on typing the checkpoint id as a string)*

and the schema already types handles as strings in five places, and §8 item 16 records the reference
server having shipped a *numeric* handle once already and having had to change it. A fourth handle type
on this bus that is not a string would be the fourth chance to make the same mistake.

**Field name: `breakpoint`.** This deliberately does **not** follow the watchpoint surface's
abbreviation. `watchpoint_add` returns `watch`, because "watchpoint" is a compound whose head noun,
"watch", is a good field name standing alone. "Breakpoint"'s head noun is "break", which is a poor field
name (it reads as a verb and as a control-flow keyword) and a worse one to hand to a client author. The
symmetry that matters — *the handle is a top-level, singular, opaque, never-reused string named for the
object it identifies* — is preserved; only the abbreviation is not, and the abbreviation is the part with
no semantics in it.

**Never reused.** Same rule and same reason as `watch`: a stale handle must resolve to nothing rather
than silently to a different breakpoint.

### 3.4 Alternatives rejected

**(a) Keep `addr`/`symbol` on `breakpoint_clear` as a convenience alongside the handle.** Rejected: it
preserves the exact call that causes the harm, and a surface that offers both a safe and an unsafe
spelling of the same operation will be used through the unsafe one, because the unsafe one is shorter and
needs no state. The legitimate use case — an interactive debugger whose user clicks a gutter to toggle a
line — is served *better* by list-then-clear: that client sees that there are **two** breakpoints at the
address, which is precisely the fact the address-keyed call destroys.

**(b) Make the handle an integer index.** Rejected on §6.1's reasoning, quoted above.

**(c) Make the handle the address, formatted as a hex string.** Rejected: it is the address-keyed model
wearing a handle's clothes, and it cannot express the duplicate that §2.4 records as already existing.

---

## 4. A2 — `clear {all: true}` survives as a distinct teardown primitive

### 4.1 The proposal

`emulator/breakpoint_clear` takes **exactly one of** `breakpoint` (a handle) **or** `all` (a boolean) —
the `oneOf` shape `watchpoint_clear` already uses. `all` is not deprecated, not discouraged, and not
reachable only through the handle path.

### 4.2 The argument, which is normative text and not commentary

**A client that crashed mid-flow cannot enumerate what it armed.** Teardown must therefore not require
handle tracking. Handles are the **addressing** primitive; `all` is the **teardown** primitive; they solve
different problems and **collapsing them into one mechanism breaks crash-path cleanup**.

This is stated here as a design constraint with its reason attached because the natural instinct on
reading A1 is that `all` is now redundant — "you have handles, iterate them" — and that instinct is
wrong in exactly the case that matters. The client with a complete handle list is the client that did not
crash. The stale breakpoint at 1,691,410 hits in §2.4 is what the other case looks like.

Note also that `all` is what both automated consumers actually call (§2.3), so removing it would break
the only automated consumer while the handle migration breaks nothing.

**Requested as normative prose in §6**, not merely as a schema shape, precisely so that the reason
survives the next editor.

### 4.3 What `all` clears

Every breakpoint the server holds, **including breakpoints armed by other connections**. This is stated
explicitly because it is the one place where A2 and A1 pull against each other: `all` is exactly the
cross-subscriber destruction that A1 exists to prevent.

It is accepted deliberately, and the distinction is that `all` is **unambiguous about what it does**. A
client calling `clear {all: true}` cannot be surprised; a client calling `clear {addr}` and destroying
someone else's breakpoint *is* surprised, and that difference — not the blast radius — is what makes one
acceptable and the other not. A surface may offer a destructive operation; it may not offer an operation
that is destructive by accident.

*(An adjudicator could reasonably ask for a scoped `clear {all: true, mine: true}` variant. §12.3 hands
that question over rather than answering it.)*

---

## 5. A3 — the `stopped` event names the breakpoints that fired

### 5.1 What §3 already pins

`emulator/stopped`'s `reason` enum already contains **`breakpoint`**:

> `reason` (`breakpoint`\|`watchpoint`\|`step`\|`runTo`\|`runToScanline`\|`runFrames`\|`pause`\|`entry`)
> *(§6 row at line 637; the schema's `events["emulator/stopped"]` carries the same enum.)*

**So no new `reason` value is needed, and none is proposed.** This is worth stating explicitly because
the question was open: the stepping trio establishes that one `reason` may legitimately span several
methods — §3 pins `reason: "step"` for `step`, `step_over` and `step_out` alike — so "does a breakpoint
stop need its own reason?" was a real question with a precedent pointing at "no". Here it is answered by
the enum rather than by the precedent: `breakpoint` is already there.

This also mirrors §11.8's finding about `watchpoint`, which was *"an enum member §3 has always defined and
no catalogued method could produce"* until CR-11 made it producible. `breakpoint` is in the same state
today on the Rust successor, and this CR retires it there for the same reason.

### 5.2 The proposal, and the deviation in it

Add to `emulator/stopped`'s params:

```
breakpoints  — array of $defs/handle, minItems 1
```

**REQUIRED when `reason` is `"breakpoint"`, and MUST NOT be present otherwise** — mechanically enforced
by an `if`/`then`/`else` on `reason`, exactly as the schema already enforces `watch`.

**This is a plural array where the commissioning ruling said "the handle that fired" (singular), and the
deviation is deliberate.** Under A7 two subscribers may arm the same address and both breakpoints are live.
When the machine reaches that address, **both fire**, and both increment their `hits`. A singular field
would force the server to pick one and report it as *the* cause, which is a small silent-wrong-answer of
exactly the class this bus exists to prevent. `minItems: 1` keeps the common case unambiguous.

The alternative — forbid duplicate adds so that the singular field is always correct — is rejected in
§7.3: it reintroduces address-keyed semantics through the back door, refusing the *second* subscriber in
precisely the scenario A1 exists to serve.

**An observation, not a proposal:** `watch` on the same event is singular and has the same latent
multiplicity (two watches over overlapping ranges can cross their `stopAfter` thresholds on the same
access). This CR does **not** propose changing `watch` — it is out of scope, has a live consumer, and
would deserve its own CR. It is recorded so that the asymmetry between `watch` and `breakpoints` is on
the record as known rather than discovered later as an inconsistency.

### 5.3 Why REQUIRED rather than optional, and what that costs

REQUIRED, for two reasons.

1. **The pre-release window for REQUIRED additions shuts at first ship.** A field that is optional at
   ship becomes permanently optional, because every client must then carry the absent-field path forever.
   Adding a field to an *event* later is materially harder than adding one to a *result*: a result is
   consumed by the caller, who knows what it asked for; an event is consumed by subscribers who did not
   make the call, so an optional field on an event is a field no subscriber can rely on.
2. **Without it, "wrong breakpoint fired" and "right breakpoint, wrong PC" are one indistinguishable
   failure message.** A subscriber sees a stop at a PC it did not expect and cannot tell whether its
   instrument misfired or someone else's did.

**The cost, stated plainly: this makes the legacy C++ server non-conformant on an event it already
emits.** The contract's own canonical example is a `stopped` event with `reason: "breakpoint"` and no
handle (`protocol.md:321`). A server that keeps emitting that becomes non-conformant the moment this
lands. See §10 for why that is the accepted outcome under the contract's existing precedent — and §11.2
for why this is one of the two weakest joints in this CR.

---

## 6. A4 — stop precision: exact, or the server says it is not

**This is the clause with the widest blast radius, it generalises past breakpoints, and it is the one
this CR would most regret losing.**

### 6.1 The property

**Either the stop PC is exact, or the server says it is not.** The failure mode that hurts is not an
imprecise stop; it is an **imprecise stop presenting as a precise one**.

### 6.2 The precedent, which is concrete and costing a consumer today

The legacy server, run with `deterministic=True`, answers `breakpoint_add` with the note *"det-mode stop
granularity: PC may precede the breakpoint"* — its serial scheduler rolls back to commit granularity, so
the halt can land one instruction early. The requesting overseer verified that note verbatim at
`aeon/tools/raster_source_gate.py:32-40`.

For that gate, a stop one instruction early lands **before** an `adda.w`, leaving a register holding a
plausible-looking unmodified value **that would make the gate PASS on code that never applied the
offset**. That is a false pass, not a crash — the gate produces a verdict, the verdict is wrong, and the
verdict is pasted into merge evidence. The gate's authors worked around it by forcing a threaded launcher
path purely to obtain exact stop PCs.

This is the failure this clause exists to make impossible: **a client discovering imprecision by getting a
believable wrong answer.**

### 6.3 The proposal

A three-level disclosure, using one new typed key spelled the same way at each level.

```
stopPrecision : "exact" | "approximate"
```

- **`"exact"`** — the machine is halted at `pc`; the instruction at `pc` has **not** executed; `pc` is the
  armed address. Resuming executes the instruction at `pc`.
- **`"approximate"`** — the reported `pc` is near the armed address, and the server promises **nothing**
  about which side of it or by how much. A client MUST NOT read register or memory state as though the
  instruction at the armed address had, or had not, executed.

The `"approximate"` wording is deliberately a refusal to promise rather than a bounded error term. The
legacy note says "may precede", with no bound, and inventing a bound the implementation does not hold
would recreate the defect one level down.

**Three levels, each answering a different question at a different time:**

| Level | Key | The question it answers |
|---|---|---|
| Handshake | `capabilities.breakpoints.stopPrecision` | *Should I run against this server at all?* — answerable before anything is armed. This is what lets a gate refuse to produce a verdict instead of producing a wrong one, which is exactly what `raster_source_gate` currently hardcodes a launcher workaround to achieve. |
| `breakpoint_add` result | `stopPrecision` | *Is this particular breakpoint exact?* — answerable before the 120-second wait is spent. |
| `emulator/stopped` params | `stopPrecision` | *Is **this** stop exact?* — answerable by a subscriber that made no call and saw no handshake. |

**REQUIRED at all three levels** where the level applies:

- On `emulator/stopped`: REQUIRED when `reason` is `"breakpoint"` or `"runTo"` — the two stop conditions
  that name a PC the client chose — and MUST NOT appear otherwise. (`pause`, `entry`, `runFrames` and
  `runToScanline` have no PC target, so precision is not a property they have. `step` and `watchpoint`
  are discussed in §12.2 and are **not** included by this CR.)
- On `breakpoint_add`'s result and in `capabilities.breakpoints`: REQUIRED unconditionally.

**Why REQUIRED and not "absent means exact".** Because §2.4 clause (a)'s reasoning for `truncated`
applies word for word: absence and `"exact"` must not both mean exact, or a server that simply omitted
the key is indistinguishable from one that promised. The whole clause exists to stop imprecision from
looking like precision, and a defaulting rule is a mechanism for exactly that.

### 6.4 The normative rule, stated generally

Requested as new §6 prose:

> **A stop either reports an exact PC or declares that it does not.** Whenever a server halts the
> machine on a condition naming a PC and reports that PC, the PC MUST be exact — the instruction at
> `pc` has not executed — unless the reply or event carries `stopPrecision` naming a weaker
> granularity. A server that cannot offer exact stops in some mode MUST make that mode reachable only
> by **explicit client opt-in**, and MUST carry the granularity on every stop the mode produces. A
> server MUST NOT offer imprecise stops as its default and MUST NOT report an imprecise stop without
> the key.

### 6.5 What our own server can offer, and what it cannot

The Rust core's run loop stops at the instruction boundary **before** `pc` executes, by construction —
`crates/oracle-core/src/bus.rs:305-318` documents that a sink raising its flag from
`on_step_boundary(pc, _)` gets *"classic breakpoint semantics (stop before `pc` runs)"*. There is no mode
in which it does otherwise. So it can advertise `stopPrecision: "exact"` unconditionally, and it has no
opt-in mode to define. **This claim is a claim about source text; it has not been confirmed at runtime
and is tagged as needing runtime confirmation before this lane asserts it in a handshake.**

### 6.6 Alternatives rejected

**(a) A boolean `exact: true|false`.** Rejected: it cannot grow a third granularity without becoming a
lie, and a granularity vocabulary is exactly the kind of thing that grows.
**(b) Carry the warning in `caveat`.** Rejected on §2.4 rule 3 — *any consequence a client must act on
needs a typed field* — and on the observed fact that the legacy server already does this, in prose, and
the consumer had to hardcode a launcher workaround because a prose string is not something a gate can
branch on.
**(c) Refuse to serve breakpoints at all in an imprecise mode.** Rejected: it is not this contract's place
to forbid a server a mode; it is this contract's place to forbid a server a *silent* one. The opt-in
requirement in §6.4 is the enforceable half.

---

## 7. A5–A7 — the remaining rulings

### 7.1 A6 — the cap

`capabilities.breakpoints` becomes an **object**, matching `checkpoints` and `watchpoints`:

```json
"breakpoints": {
  "supported": true,
  "maxBreakpoints": <int ≥ 1>,
  "stopPrecision": "exact" | "approximate"
}
```

At the cap, `breakpoint_add` MUST fail with `-32005` carrying
`{"reason": "breakpointCapReached", "cap": n, "count": n}`. It MUST NOT silently grow past the advertised
number and MUST NOT silently evict an existing breakpoint.

This is **D13 rule 3 verbatim**, applied to a third object, with the reason §11.8 already stated: *"a
handle a client is still holding must never quietly start meaning nothing."* Nothing new is being argued
here — it is the house rule reaching the one object on this bus it has not reached.

A server that does not serve the family emits `{"supported": false}`. The bare boolean spelling is
retired. **This is a wire change to the handshake** and it is a change to *this lane's* server too, which
currently emits `"breakpoints": false` (`crates/oracle-aether/src/engine.rs:1050`).

⚠ **A note addressed to the legacy implementer, not a contract clause.** The audit records that the legacy
server infers its wire error codes from **message substrings** (`ControlSocket.cpp:211-222`), so *"any cap
ruled in must have its message engineered to hit the right substring."* This CR cannot and does not
specify anyone's message text; it flags the hazard so the cap's introduction there does not silently land
on the wrong code.

### 7.2 A7(a) — `enabled` is struck

D-13(a): `breakpoint_list` reports `enabled` per row and **no catalogued method sets it**. It is a
read-only report of a state nothing on this bus can change.

**Struck, not made writable.** As long as nothing writes it, `enabled` is a constant `true`, and a
constant is a pure function of membership in the list. §11.10 struck per-entry flags on exactly that
ground, and §11.13 struck four proposed `play_input` result keys as *"pure functions of `rows` and
`frames`"*. The nearest live comparison also confirms it: `watchpoint_list` reports `stopAfter` *"present
only when this watch will halt the run […] so it is listed rather than left to be discovered"* — a
breakpoint is **always** that thing, so membership in the list already carries the entire signal
`enabled` would carry.

**This rejects half of D-13(a)'s own recommendation**, which was to add `breakpoint_set_enabled`. The
argument against adding it: it is a fourth method with **zero consumers anywhere in the workspace**
(the survey's consumer sweep found no consumer for `breakpoint_list` at all), and a CR that ships a method
nobody asked for on the strength of a field nobody writes has moved a defect rather than removed it.

**The condition that would reopen it, named so it is not lost:** a client that needs to *disable without
losing identity*. Under never-reused handles, clear-then-re-add issues a **new** handle, so a client
holding the old one loses the object. That is a real cost, and the day a debugger UI (aurora) or any
other consumer needs a temporary disable, `breakpoint_set_enabled` should be added — additively, with
`enabled` returning to `breakpoint_list` at the same time. This is registered as a deferred follow-up,
not proposed here. §11.4 lists this as one of the soft joints.

### 7.3 A7(b) — duplicate add (D-12), ruled *against* the audit's recommendation

**Ruling: a second `breakpoint_add` at an occupied address creates a second, distinct breakpoint with a
distinct handle. It is never an error and never idempotent. Both fire; both count `hits`.**

D-12 recommends the opposite — *"pin the idempotent reading (a re-add succeeds and returns the same
`addr`)"*. That recommendation was reasoned inside an **address-keyed** model, where two identical
breakpoints would be genuinely indistinguishable and collapsing them costs nothing. Under handles they are
distinguishable, and collapsing them costs the thing A1 exists to buy: two subscribers arming the same PC
would share one object, so either subscriber's clear would disarm the other's — the original hazard,
reproduced.

Corroborating: the audit's own "verified-by-them" section records that on the legacy server *"a duplicate
address is empirically possible, which cuts against D-12's idempotent reading."* The empirical behaviour
and the handle model agree; only the pre-handle recommendation dissents.

Rejected alternative — **refuse a duplicate with `-32005`**: this is address-keyed enforcement wearing a
different hat. It refuses the *second* subscriber in exactly the two-subscriber scenario A1 exists to
serve, which is strictly worse than the harm A1 fixes.

### 7.4 A7(c) — clear of nothing (D-15)

**Ruling: `breakpoint_clear` of an unknown or retired handle SUCCEEDS with `removed: 0`.**

House precedent is unanimous and the argument is quoted from §6.1 and repeated at §11.8 for
`watchpoint_clear`: *"an error a client must learn to swallow teaches clients to swallow errors."* D-15's
only reservation was that *"a precedent is not the row"* — this CR makes it the row.

`removed` keeps `checkpoint_drop`'s meaning: how many actually went. It is what lets a client distinguish
a clear that found something from one that found nothing without a second round-trip, which is also what
makes `clear {all: true}` a usable teardown: it reports what the crashed predecessor had left armed.

### 7.5 A7(d) — `breakpoint_list`, and D-14 answered with a third reading

D-14 offers two readings of why `breakpoint_list` carries none of §2.4's bounded-list companions:
(a) the list is complete by construction and unbounded, so clause (a) does not bite; or (b) it is
policy-bounded like every sibling, so clause (a) is a MUST the row violates.

**Neither is right once A6 lands, and the third reading is better than both:** the collection is
**policy-bounded** (A6 caps it), so clause (a) applies and its companions are a MUST — but the cap makes
the whole collection smaller than any sensible page, so **the list can always be returned in one reply.**

Proposed shape, §2.4's flat spelling:

```
params : —
result : breakpoints[]{breakpoint, addr, symbol?, symbolDisp?, label?, hits}, total, returned, truncated
```

- `total`, `returned`, `truncated` — clause (a), REQUIRED, `truncated` **even when false**.
- **No `cursor` and no `limit`.** §2.4 clause (b): *"a method that accepts no continuation MUST NOT emit
  one."* With a normative "MUST return every live breakpoint in one reply", `truncated` is always `false`
  and a cursor would be a token that can never be handed back.

**This diverges from `watchpoint_list`, which does carry `cursor` and `limit`, and the divergence is
argued rather than accidental.** Watches are capped too, so `watchpoint_list`'s cursor is arguably surplus
by the same argument — but `watchpoint_list` was shaped to mirror its sibling `watchpoint_hits`, whose
ring genuinely can exceed a page. Breakpoints have no hit log and therefore no sibling to mirror. This CR
does **not** propose changing `watchpoint_list`; §12.4 hands the question over.

`symbol?`/`symbolDisp?` are added on §4's naming rule, which binds every symbol-bearing row on this bus,
and because a list of raw addresses is the least useful form of a list a human reads. `label?` is
discussed next.

### 7.6 A7(e) — `label`, adopted from the watchpoint surface

`breakpoint_add` takes an optional `label` string, *"carried back verbatim and never interpreted"* —
`watchpoint_add`'s field with `watchpoint_add`'s meaning, and `checkpoint`'s before it.

**Why it belongs here specifically:** the harm in §2.4 was an **attribution** failure. An agent looked at
seven breakpoints, could not tell whose they were, judged them "not mine", and destroyed them. A handle
fixes the *mechanism* of that harm; it does nothing for the *judgement* that preceded it, because a
handle is opaque by design and tells a human nothing. `label` is the cheapest primitive that does.

### 7.7 A7(f) — `hits`, kept, with the reset rule D-14 noticed was missing

`hits` stays. It is the incumbent field, and it is the number that diagnosed the 1,691,410-hit stale
breakpoint — the single most useful field on this row.

**Not renamed to `matched`** despite `watchpoint_list` spelling the analogous count that way. On the
watch surface, `matched` exists to be distinguished from the *hits stored in the ring*, which is a real
distinction there (`watchpoint_hits.total` vs `matched` vs `dropped`, three different numbers §11.8 is at
pains to keep separate). Breakpoints have no ring and no second number, so the ambiguity `matched` was
coined to resolve does not exist, and renaming would cost the incumbent spelling to buy a distinction with
nothing on the other side of it.

**The reset rule, which §6 currently omits** (the fragment notes *"§6 does not say what resets it"*):
`hits` counts stops caused by **this handle** and is **never reset**. Handles are never reused, so a fresh
count is a fresh handle, and there is nothing a reset could mean that clearing and re-arming does not
already mean.

### 7.8 A7(g) — refusals on `breakpoint_add`'s `symbol` spelling

D-12's second half: the row names no refusals, so a server has no stated code for "no symbols loaded" or
"symbol not found" on the `symbol` spelling, *"even though §5 defines both."*

**Ruling: `-32012` (no symbols loaded) and `-32013` (symbol not found), named in the row.** §5 already
keeps these two distinct *"so clients can tell"*, and `run_to` — the row `breakpoint_add`'s `addr`-XOR-
`symbol` shape was transcribed from — resolves the same pair through the same machinery. This is
bookkeeping, not a design decision, and it is included only because leaving it out would leave D-12 half
closed.

---

## 8. A5 — `wait_for_break`

### 8.1 The property

**`wait_for_break` resolves against an event; it does not block the connection.**

If the call blocks the connection, a wedged emulator takes the socket with it and **the client-side
timeout becomes unenforceable — destroying the property the call exists for.** aeon's 120 000 ms is a
*wedge detector*, not a performance budget, and a wedge detector that cannot give up is not a detector.

### 8.2 The proposal, in three parts

**(1) A transport obligation, requested as normative text.**

> A server MUST NOT serialise replies on a connection. A `wait_for_break` outstanding on a connection
> MUST NOT prevent that connection from being served — `emulator/pause`, `emulator/status` and the
> `emulator/breakpoint_*` family in particular MUST remain callable while a wait is outstanding.
> Replies MAY be delivered in an order other than the order the requests arrived; the JSON-RPC `id` is
> the correlation and a client MUST NOT assume FIFO.

This is the **minimal** wire change consistent with the property: JSON-RPC 2.0 already permits
out-of-order replies, so a correct client needs no change at all. aeon's `await b.call(...)` blocks the
caller's coroutine, not the socket, and keeps working verbatim.

It is stated as a contract obligation rather than left to servers because it is currently *not* how at
least one implementation works: this lane's server is synchronous by construction — `Engine::dispatch`
(`engine.rs:984`) returns a value on the engine thread, and every run method runs the machine *inside* the
handler. Under that model a 120-second wait blocks the engine, no second client can be served, and
`emulator/pause` cannot arrive to end it. The survey (§4.6) classifies this as **architectural, not
per-method**, and the single biggest unknown in pricing the whole acceptance parcel.

**⚠ Honest disclosure: this is the most expensive clause in this CR, and it is expensive for the lane
raising it.** It is proposed anyway because the alternative is a method whose defining property does not
hold. §11.5 records the objection that this clause arguably belongs in §2 (transport) rather than in a
method CR.

**(2) A bound: `timeoutMs` gets a default and an advertised ceiling.** D-07 records that `timeoutMs` has
*"no default and no ceiling"*, and that *"an unbounded default would be the hang D12 exists to prevent"* —
the fragment declined to invent one because the row is deprecated. This CR invents one, because a
deprecated method that a nightly gate depends on is not a method anyone is about to stop serving.

- **Default: 10 000 ms.** Derived by analogy: D12's house bound for wait-shaped ops is
  `maxFrames` default **600**, which is ten seconds at 60 Hz. This is that number in the units this method
  actually uses. It is an invented number and §11.6 says so.
- **Ceiling: `limits.maxWaitMs`**, advertised in the handshake alongside `limits.maxWriteLen` and
  `limits.maxInputRows`. Above it, `-32602` — **refused, never clamped**, which is the house rule every
  bounded param in §6 already follows. A server serving aeon's gates must advertise ≥ 120 000.

**⚠ A defect in D12 found while drafting this, reported rather than worked around.** D12 says:

> Any method that runs the machine until a condition — `emulator/run_to`, `emulator/run_to_scanline`,
> and any future **`wait_for_break`-shaped op** — MUST accept a `maxFrames` bound (default 600)…

**A frame bound cannot bound a wedge, because a wedge is precisely the state in which frames stop
advancing.** D12 names `wait_for_break` explicitly and prescribes for it the one bound that provably
cannot bind it. The wall-clock `timeoutMs` is the correct bound for this op and the *only* one that works,
and `waitedMs` is already *"the one wall-clock quantity in this catalog, and legitimately so"* for the
same reason. This CR requests a one-sentence carve-out in D12 naming why. It does **not** propose changing
`run_to` or `run_to_scanline`, which are frame-bounded correctly.

`timeoutReached` is D12's `reached` with inverted polarity. The polarity is not changed: the spelling is
incumbent, and D-06 has already ruled its casing (`timeoutReached`, not `timeout_reached`). Noted so the
asymmetry is on the record rather than found later.

**(3) A cancel: `emulator/wait_cancel`.**

```
params : requestId  ($ref #/$defs/id — integer|string)
result : cancelled  (boolean)
```

- **`requestId` is the JSON-RPC `id` of the outstanding `wait_for_break` request.** It reuses the existing
  `$defs/id` unchanged, and it is the **only** identifier the client holds for a call that has not
  replied — which is why the cancel cannot be keyed on a handle: under (1) the wait has no immediate
  reply in which to return one.
- **Scope: the same connection only.** A `requestId` is per-connection, so `wait_cancel` MUST NOT cancel
  a wait outstanding on another connection. Without this rule one client can end another's wait, which is
  the address-keyed-clear hazard of §3.2 rebuilt on the wait surface.
- **Idempotent:** an id that is not outstanding answers `cancelled: false`, on §6.1's rule. `cancelled`
  is a boolean rather than a count on `emulator/pause.wasRunning`'s precedent — it *"reports the
  transition, not the destination"*, and at most one wait can carry a given id.
- **The cancelled wait still gets a reply**, as JSON-RPC requires. It resolves with a new typed key
  `cancelled: true` and **no `pc`**.

**A consequence worth stating: `wait_for_break` now has three outcomes and three typed discriminants.**
`pc` present (a real stop) / `timeoutReached: true` / `cancelled: true`, mutually exclusive. Before this
CR only two of the three were expressible, and §2.4 rule 3 requires a typed key for *any consequence a
client must act on*.

### 8.3 Is a server-side timeout still needed once cancel exists? Yes.

They cover disjoint failures, and this was asked as an open question rather than assumed:

- **Cancel requires a live client.** It is the mechanism for a client that is running and changes its mind.
- **The timeout is the mechanism for a client that died.** An orphaned wait from a crashed client can
  never be cancelled, and without a server-side bound the server holds that wait forever.

The dead-client case is the same case A2 exists for on the breakpoint side, which is a small piece of
evidence that the pairing is the right shape: **every long-lived object on this bus needs both a
client-driven release and a server-driven one**, because the client that most needs releasing is the one
that cannot ask.

### 8.4 Alternatives rejected

**(a) `$/cancelRequest`, LSP's spelling.** Rejected mechanically: the envelope's method pattern is
`^([a-z][a-z0-9]*/)?[a-z][a-z0-9_]*$`, which forbids it.
**(b) Use `emulator/pause` as the canceller.** It works — the machine stops, the wait resolves with
`reason: "pause"` — and needs no new method. Rejected: it **changes the machine's state in order to end a
wait**, which is the implicit mode change §5 forbids resolving on the client's behalf, and it offers no
way to stop waiting while letting the machine run. A GUI's "cancel" button should not be a "pause"
button.
**(c) Make `wait_for_break` return a handle immediately and poll for completion.** Rejected: it converts
every consumer into a poll loop, which is what `emulator/stopped` was introduced to retire (§8 item 9,
D6), and it breaks both existing consumers.
**(d) Delete `wait_for_break` and tell clients to subscribe to `emulator/stopped`.** Rejected: the
fragment states the retention obligation — *"clients without the `events` capability may still poll it,
and a server MUST keep answering it"* — and both live consumers use the method.

---

## 9. The exact deltas requested

Quoted current text first in every case, so the delta is visible rather than reconstructed.

### 9.1 `contract/protocol.md` §6, lines 1015–1017 — replace

**Current:**
```
| `emulator/breakpoint_add` | `addr`\|`symbol` | `addr` |
| `emulator/breakpoint_list` | — | `breakpoints[]{addr,enabled,hits}` |
| `emulator/breakpoint_clear` | `all`\|`addr`\|`symbol` | `removed` |
```

**Proposed:**
```
| `emulator/breakpoint_add` | `addr`\|`symbol`, `label`? | **`breakpoint`** (str), `addr`, `stopPrecision`, `label`? |
| `emulator/breakpoint_list` | — | `breakpoints[]{breakpoint,addr,symbol?,symbolDisp?,label?,hits}`, `total`, `returned`, `truncated` — §2.4's flat spelling |
| `emulator/breakpoint_clear` | `breakpoint` (str)\|`all` | `removed` |
```

### 9.2 `contract/protocol.md` §6, line 857 — amend

**Current:**
```
| `emulator/wait_for_break` *(deprecated by `stopped`)* | `timeoutMs`? | `running`, `pc`?, `symbol`?, `timeoutReached`?, `waitedMs`? |
```

**Proposed:**
```
| `emulator/wait_for_break` *(deprecated by `stopped`)* | `timeoutMs`? (≥0, def 10000, ≤`limits.maxWaitMs`) | `pc`?, `symbol`?, `symbolDisp`?, `timeoutReached`?, `cancelled`?, `waitedMs`? |
| `emulator/wait_cancel` | `requestId` (int\|str) | `cancelled` |
```

*(`running` is dropped from the row per **D-05** — it is the envelope stamp's, not the handler's, and §6
lists it only because the row predates D11. `symbolDisp?` is added per **D-08**, which records that this
is the only symbol-bearing row in the catalog without it, so a non-zero displacement is currently
unreportable. Both are pre-existing audit findings this CR closes in passing; neither is argued here.)*

### 9.3 `contract/protocol.md` §3 / §6 line 637 — the `stopped` row

**Current params list:**
```
`reason` (…), `pc` (hex str), `symbol`?, `symbolDisp`?, `frames`?, `deadlineReached`?, `buttons`?, `port`?, `watch`?
```
**Proposed:** append `, `breakpoints`?, `stopPrecision`?` — with the conditional-requirement rules in §5.2
and §6.3 stated in prose beneath the table, as `watch`'s already are.

**The `reason` enum is unchanged.** `breakpoint` is already a member.

### 9.4 New normative prose in §6

Four blockquoted paragraphs, in the style §6 already uses for the watchpoint surface:

1. **The handle rule** (§3.2–3.3) — server-assigned, never reused, opaque string.
2. **The teardown rule** (§4.2) — `all` survives, *with its crash-path reason written down*, and the
   explicit statement that collapsing it into the handle mechanism breaks crash-path cleanup.
3. **The stop-precision rule** (§6.4) — quoted verbatim in that section; **the clause this CR would most
   regret losing.**
4. **The lifecycle rules** (§7.3–7.7) — duplicate add creates a distinct breakpoint; clear of an unknown
   handle is `removed: 0`; `hits` is never reset; `-32012`/`-32013` on the `symbol` spelling.

### 9.5 D12 — one-sentence carve-out

Per §8.2(2): D12 names `wait_for_break`-shaped ops and prescribes `maxFrames`, which cannot bound a wedge.
Requested: a sentence naming the exception and its reason. No change to `run_to` or `run_to_scanline`.

### 9.6 Schema — `contract/schema/bus-protocol.schema.json`

| Fragment | Change |
|---|---|
| `methods["emulator/breakpoint_add"]` | rewritten: `label?` param; result gains `breakpoint` (`$ref: #/$defs/handle`), `stopPrecision` (enum), `label?` |
| `methods["emulator/breakpoint_clear"]` | `oneOf` narrows from three-way `all\|addr\|symbol` to two-way `breakpoint\|all` — `watchpoint_clear`'s exact shape |
| `methods["emulator/breakpoint_list"]` | `enabled` struck; `breakpoint`, `symbol?`, `symbolDisp?`, `label?` added; `total`/`returned`/`truncated` added |
| `methods["emulator/wait_for_break"]` | `timeoutMs` gains `default: 10000`; result gains `cancelled?`, `symbolDisp?` |
| `methods["emulator/wait_cancel"]` | **new fragment** (58 → 59) |
| `events["emulator/stopped"].params` | `breakpoints` (array of handle, `minItems: 1`) and `stopPrecision` (enum) added, each with an `if`/`then`/`else` on `reason` mirroring `watch`'s |
| `handshake…capabilities.breakpoints` | `{"type": "boolean", "description": "Whether the breakpoint family (§6) is served."}` → object with `supported`, `maxBreakpoints`, `stopPrecision` |
| `handshake…limits` | `maxWaitMs` registered |

### 9.7 ⚠ Four error obligations this CR creates that **no fragment can express**

The survey established a structural fact about the schema: **no fragment among the 58 declares any error
condition — all carry only `$comment`, `params` and `result`.** Every error obligation on this bus lives
in prose and cannot be validated against a fragment.

This CR adds four:

| Obligation | Where it must live |
|---|---|
| `-32005 {reason:"breakpointCapReached", cap, count}` at the cap | §6 prose only |
| `-32602` on `timeoutMs` above `limits.maxWaitMs` — refused, never clamped | §6 prose only |
| `-32012`/`-32013` on `breakpoint_add`'s `symbol` spelling | §6 prose only |
| `breakpoint_clear` of an unknown handle is **not** an error (`removed: 0`); `wait_cancel` of a stale id is **not** an error (`cancelled: false`) | §6 prose only |

**Consequence, stated loudly:** a conformance suite that validates replies against fragments is **blind to
all four**. This CR therefore ships four obligations that no automated gate can hold.

**Raised separately, and out of scope here:** fragments should gain an `errors` sub-object so that error
shapes become as mechanically checkable as result shapes. That is a change to every fragment and to the
gate, it is larger than this CR, and it should not ride along — but this CR is a concrete demonstration
that the gap has a growing cost, and the adjudicator should know that adopting CR-A widens it.

---

## 10. What this CR does and does not bind

**It binds the contract.** It does not bind any server's schedule, and it does not assert that either
implementer will conform by any date.

**The legacy C++ server becomes non-conformant on this surface if this lands.** Specifically: an
address-keyed `breakpoint_clear` becomes a call the contract does not define, and a `stopped` event with
`reason: "breakpoint"` and no `breakpoints` array becomes non-conformant — including the contract's own
canonical example at `protocol.md:321`.

**That is the accepted outcome under precedent this contract has already set twice**, and the precedent is
quoted rather than paraphrased because it is the whole of the argument:

> *Consequence, accepted with eyes open:* the moment this is normative the legacy C++ Oracle is
> **non-conformant** […] That is the correct outcome, not a problem to design around: the roadmap already
> has oracle-next inheriting the bus role, and **a server on its way out does not get to hold the contract
> at the weaker shape**. No compatibility flag is offered — a `stamped: false` capability would let the
> defect survive negotiation forever, and every client would have to carry both code paths.
> *(protocol.md, D11)*

and, from the audit's own revised D-33 sequencing ruling:

> **Revised ruling: never retire the alias on the legacy server. Retire it by REPLACING the server.** The
> legacy alias dies with the binary that hosts it […]

**No compatibility flag is proposed here either, and for D11's reason.** A `capabilities.breakpoints.
handles: false` escape would let the two-subscriber hazard survive negotiation indefinitely and force
every client to carry both paths forever. `capabilities.breakpoints.supported: false` remains available to
a server that does not serve the family at all; there is deliberately no way to advertise *the old shape*.

**What is asked of the legacy implementer:** nothing by this CR. If and when that server implements the
amended rows, the message-substring hazard in §7.1 applies to its cap refusal.

---

## 11. Where this CR is weakest

Written so an adjudicator can rule against it on the merits without having to find the soft joints first.

### 11.1 The procedural objection, which could void this CR entirely

The audit says, of its own findings:

> **"Which implementation has this been built against?" has two different answers**, and for D-10,
> **D-13** and D-17 the answer belongs to the legacy server, not to the lane that owns the successor.
> **Do not adjudicate those three as if one implementer speaks for both.**

**D-13 is the defect this CR principally closes, and this CR is raised by the successor's lane.** If that
clause reserves *ruling authority* on D-13 to the legacy implementer, then CR-A is out of order and should
be returned unadjudicated, regardless of its merits.

The reading this CR proceeds on is the narrower one: the clause governs **facts** — the history, the
measured harm, the empirical duplicate-address behaviour — all of which are taken here from the legacy
implementer's own account without re-derivation (§2.4, §7.3). The *forward shape of the contract* is not a
per-implementer question, and D11's precedent (§10) settles it the other way explicitly.

**But that reading is this lane's, it is self-serving, and the adjudicator should decide it first**,
because everything downstream is moot if it goes the other way.

### 11.2 A3's REQUIRED handle has no capability gate

Making `breakpoints` REQUIRED on `stopped` when `reason` is `"breakpoint"` invalidates an event the legacy
server emits today and invalidates the contract's own example at `protocol.md:321`. §10 argues this is
correct under D11. But D11 was a decision taken deliberately at the top of the document with a whole
paragraph of justification, and this CR is doing the same thing in a subordinate clause. A reasonable
adjudicator could hold that a REQUIRED addition to a *shared event* is a bigger act than a CR should
perform, and demand either a capability gate or a separate decision entry.

The fallback if so: make `breakpoints` OPTIONAL on the event and REQUIRED only when
`capabilities.breakpoints.supported` is the object form. This CR does **not** recommend that — §5.3's
argument is that an optional event field is a field no subscriber can rely on — but it is the shape to
retreat to, and it is better than losing the field.

### 11.3 The array in A3 is a deviation from the commissioning ruling

The ruling said "the handle that fired", singular. §5.2 makes it plural. The argument is sound only if
§7.3's duplicate-add ruling stands; if the adjudicator prefers refusing duplicates, the singular field is
correct and the array is over-engineering. **These two decisions must be ruled together, not separately.**

### 11.4 Striking `enabled` removes a field from an existing row

§7.2 argues it carries zero bits today. But it is a field that exists, a debugger UI is the obvious
consumer, and none exists *yet* only because the successor serves no breakpoints at all. An adjudicator
who weights "do not remove published surface" heavily should keep `enabled` and add
`breakpoint_set_enabled` — which is D-13(a)'s own recommendation, and this CR is rejecting half of it on
a minimality argument that is a judgement call, not a derivation.

### 11.5 A5(1) is a transport rule inside a method CR

"A server MUST NOT serialise replies on a connection" governs §2 (the envelope), not §6 (a method). It
is here because `wait_for_break` is the method that exposes it. It is also, by the survey's own pricing,
*"an architectural change to the server, not a method"* and the single biggest unknown in the acceptance
parcel. An adjudicator could reasonably split it into its own CR against §2 — and if so, **CR-A's
breakpoint half should still land**, because the breakpoint trio does not depend on it.

### 11.6 Three invented numbers

`timeoutMs` default **10 000** is derived by analogy from a frame bound and is otherwise arbitrary. The
suggested `limits.maxWaitMs` floor of 120 000 is read off one consumer's current call. `maxBreakpoints`
is left unspecified deliberately (it is a server policy number, like `maxWatches`), but that means this CR
mandates a cap without saying what a reasonable one is.

### 11.7 The three-level `stopPrecision` may be one level too many

The handshake level and the stop-event level each answer a question the other cannot. The **arm-reply**
level is the redundant one: it matters only if a server's precision can change between handshake and arm,
which on the one implementation known to have an imprecise mode is a launch-time flag. If the adjudicator
strikes one, strike that one; the two-level version keeps the whole property.

### 11.8 Unverified at runtime

§6.5's claim that this lane's core stops exactly, by construction, is read off `bus.rs:305-318` and has
**not** been confirmed by running anything. It is tagged for foreground runtime confirmation and must not
be asserted in a handshake until it is.

---

## 12. Open questions handed to the adjudicator

Not answered here, deliberately.

1. **§11.1's procedural question** — does the audit's "the answer belongs to the legacy server" clause
   reserve ruling authority on D-13? This one is prior to all the others.
2. **Should `stopPrecision` extend to `reason: "step"` and `reason: "watchpoint"`?** This CR scopes it to
   `breakpoint` and `runTo` — the reasons with a client-chosen PC target. But a `step` on a server that
   cannot stop exactly has the same failure mode, and §11.8 already pins a watch stop as landing *after*
   the triggering instruction commits, which is a *documented* imprecision that arguably wants the same
   typed key rather than the prose it currently has. Widening it is cheap now and expensive later.
3. **Should there be a scoped teardown — `clear {all: true, mine: true}`?** §4.3 accepts that `all` is
   cross-subscriber destructive. A per-connection scoping would keep the crash-path property while
   removing the blast radius. It is not proposed because "mine" needs a connection-identity concept the
   contract does not currently have, and inventing one is larger than this CR.
4. **Should `watchpoint_list` lose its `cursor`/`limit`?** §7.5's argument that a capped collection needs
   no continuation applies to watches as well. Not proposed — it has a live consumer and deserves its own
   CR — but the two lists will look inconsistent until someone rules.
5. **Should `watch` on `stopped` become plural, matching `breakpoints`?** §5.2 records the same latent
   multiplicity. Not proposed, same reason.
6. **`maxBreakpoints`: is there a house number?** `maxWatches` and `checkpoints.cap` are both server
   config in the one implementation that has them. If the contract wants a floor a client can rely on,
   this is where to say so.
7. **Should the `errors` sub-object (§9.7) be raised now?** This CR adds four unvalidatable obligations to
   a schema that already cannot express any. Adopting CR-A widens a known hole; whether that argues for
   fixing the hole first is a sequencing call above this lane's pay grade.

---

## 13. Provenance

Drafted by the oracle lane, 2026-08-22, against `empyrean` `origin/main` `9d6ab1f` and this repo at
`6ad68ac`. The design inputs are the aeon overseer's stated requirements on the two gate scripts
(verified firsthand by the requesting overseer against aeon's `origin/master`, and taken as established
here — §2.2), and the rulings recorded in `docs/2026-08-22-acceptance-21-survey.md` §7. Where this CR
departs from the brief that commissioned it — the plural `breakpoints` array (§5.2), the rejection of
`breakpoint_set_enabled` (§7.2), the third reading of D-14 (§7.5), and the D12 carve-out (§8.2) — the
departure is argued at the point of use and listed again in §11.

No emulator was contacted and no build was run in preparing this document.

---

## 14. Overseer addendum — added at merge, 2026-08-22

Three items the drafting agent did not have. Added by the oracle overseer *before* adjudication so the
adjudicator rules on the strongest version of the argument, not a weaker one. Everything below was
verified firsthand at the revisions named.

### 14.1 The prior question in §12 is RESOLVED — this CR is in order

The draft flags a question prior to its own merit: the audit says *"for D-10, D-13 and D-17 the answer
belongs to the legacy server, not to the lane that owns the successor"*, which if it reserved **ruling
authority** would put CR-A out of order regardless of quality. The agent proceeded on the narrower
reading and — correctly and to its credit — called that reading **self-serving** and asked for it to be
decided first.

**It resolves against the worry, on the audit's own text.** Read at empyrean `origin/main`,
`docs/2026-08-22-protocol-schema-audit.md`:

- **D-13's own Recommendation, lines 193–195:** *"raise a change request that brings the breakpoint
  surface up to the watchpoint surface's shape — handle, `breakpoint_set_enabled`,
  `capabilities.breakpoints.maxBreakpoints` with `-32005 {reason:"breakpointCapReached"}`."* The audit
  does not merely permit this CR; **it commissions it by name.**
- **Line 481 speaks of *"the eventual amendment"*** as a presupposition, which only parses if a
  forward-shape change was always contemplated.

So the clause at lines 31–32 governs **whose behaviour the observed facts describe** — those fragments
describe the legacy C++ server — and its operative instruction is *"do not adjudicate those three as if
one implementer speaks for both."* That is a **binding on scope, not a reservation of authority**:
nothing ruled here binds the legacy server, which remains what `mcp__oracle__*` reaches. The agent's
narrower reading was right, and it is worth recording that it reached the right answer while
distrusting its own motive for reaching it.

**One consequence the drafter should not absorb silently:** §7.2 rejects `breakpoint_set_enabled`, which
is **named in the very Recommendation that authorises this CR**. That rejection is argued well (a
constant-true field is a pure function of list membership; §11.10/§11.13 struck per-entry flags for that
reason) and its reopening condition is named. But the adjudicator should see it as *declining a specific
item the commissioning text asked for*, which is a higher bar than an open design choice, and should
weigh it as such.

### 14.2 The D12 finding is correct, and there is a precedent for the carve-out the draft did not cite

Verified verbatim at empyrean `origin/main`, `contract/protocol.md:158-160` — D12 does explicitly reach
this method: *"Any method that runs the machine until a condition — `emulator/run_to`,
`emulator/run_to_scanline`, and **any future `wait_for_break`-shaped op** — MUST accept a `maxFrames`
bound (default 600) and MUST return `reached`."*

The draft's objection stands and understates itself. **A `maxFrames` bound is a bound in EMULATED time;
a wedge is the state in which emulated time stops advancing.** So the bound cannot trip in precisely the
failure it would need to catch — it is not a weak bound on a wedge, it is structurally incapable of
being one. Only a wall-clock bound detects a wedge, which is exactly why the sole automated consumer's
`timeout_ms` is a wedge detector rather than a performance budget. D12's own stated rationale (*"a hang
in the debug transport destroys evidence"*) is therefore an argument the frame bound **cannot deliver**
for this method, and D12's second mandate fits no better: `reached` is specified *"beside its echo of the
target"*, and a wait for any breakpoint has no target to echo.

**The precedent the draft missed, and it materially strengthens the ask** — D12 has already been scoped
out of a method in the contract's own text. `contract/protocol.md:1432-1433`, on `play_input`:
*"**D12 does not apply** — the stop condition is an exhausted count, not a predicate, so there is no
`reached`."* So the request is not for a novel exception; it is for **the second instance of an
established pattern**, and the reasoning is the same shape: D12 assumes a predicate over an advancing
machine, and a method that does not fit that assumption is scoped out rather than contorted.
This is bar 12 arriving again — *the rule was in the contract we already owned.*

Accordingly the ask should be put to empyrean as **more than a one-sentence carve-out**: either D12
distinguishes emulated-time from wall-clock bounds for wait-shaped ops, or `wait_for_break` is scoped out
on the `play_input` precedent. The adjudicator may prefer either; the draft's framing offered only one.

### 14.3 Currency, and one correction to the record

The draft anchors empyrean at `9d6ab1f`. Checked **at tip**, which is the correct direction for a
currency question: `9d6ab1f` is a real commit, an ancestor of `origin/main`, and **zero commits have
touched `contract/protocol.md` since it** — so every protocol citation in this document is current, even
though empyrean's tip has since moved (that churn is elsewhere in their tree). The anchor is good.

> ⚑ **THE PARAGRAPH ABOVE IS NOW FALSE, and it is the most instructive thing in this document.** Verified
> at the 2026-08-27 adjudication: **14 commits have touched `contract/protocol.md`** since `9d6ab1f`
> (`git log --oneline 9d6ab1f..origin/main -- contract/protocol.md | wc -l`). It was almost certainly
> **true when written on 2026-08-22** — and that is the defect. **A currency check is a measurement with a
> shelf life, and this one was recorded in the durable voice** (*"every protocol citation in this document
> is current"*), so it went on asserting itself for five days after it stopped being true, in the one
> section a later reader would consult *to decide whether to trust the citations*. Every empyrean citation
> in this CR was **exact at `9d6ab1f`** — the adjudicator re-checked them all — and **all of them are now
> stale.** Two things did not drift: the procedural clause §14.1 turns on, and D12. **Durable rule: a
> currency claim must carry the revision it was true at and an instruction to re-run, never a bare
> "current".** See `docs/2026-08-27-ruling-cr-a.md` for the full drift report and for what the drift did to
> each proposal — in short, **A1 and A2 landed upstream in the shape this CR proposed, three things this CR
> argued AGAINST landed anyway, and only A4 (`stopPrecision`) is still live.**

**Correction to §2.2, in the requesting overseer's own favour and therefore worth stating plainly:** the
brief told the drafter that migrating the consumer to handles costs them *"a trivial rewrite"*. The
drafter found the stronger and correct result — **it costs them nothing at all**: both gate scripts clear
only with `{all: true}` and never pass an address to clear, `breakpoint_add`'s params are unchanged, and
its new reply key is simply ignored by a client that does not read it. The brief's weaker claim came from
me and was not derived; the drafter's stronger one was. Recorded because a brief's stated facts get
transcription-grade trust, and this is the third time today a commissioning fact of mine has been
corrected by the agent it was given to.

### 14.4 The consumer enumeration was wrong in BOTH lanes — and the true set strengthens two clauses

Added after a round-trip with the consumer lane. Both sides had been reasoning about **two** gate
scripts. The consumer then checked their own tree and reported **three** call sites rather than the two
they had given us. That was still short. Enumerated firsthand across **all** of `tools/` at aeon
`origin/master` — every file, not the files under discussion:

| file | `breakpoint_add` | `breakpoint_clear` |
|---|---|---|
| `evict_witness.py` | `:85` | `:100` |
| `parallax_hscroll_probe.py` | `:557` | `:548`, `:567`, `:571`, `:573` |
| `raster_frame_epoch_probe.py` | `:220`, `:221` | `:219`, `:258` |
| `raster_source_gate.py` | `:161` | `:131`, `:173` |
| `snapshot_poison_gate.py` | `:62` | `:68` |

**Adds (6): `:85`, `:557`, `:220`, `:221`, `:161`, `:62`. Clears (10): `:100`, `:548`, `:567`, `:571`,
`:573`, `:219`, `:258`, `:131`, `:173`, `:68`.** Sixteen call sites across five files, against the
"2 files, 3 sites" both lanes were working from — a 5.3× widening of the evidence base. *(Totals are
written with their elements adjacent deliberately; see the method note below for why this paragraph in
particular earned that.)* Three consequences, and the first is the one that matters for adoption:

1. **The zero-cost finding holds, and now across the whole consumer surface rather than two files.**
   **Every one of the 10 clears listed above passes `{all: true}`; every one of the 6 adds passes
   `{addr}` and nothing else. There is not one address-keyed clear anywhere in `tools/`.** §2.2's
   conclusion was reached from a 3-site sample and survives the full census unchanged — a much
   stronger basis than it had.

2. **`raster_frame_epoch_probe.py:220-221` arms TWO breakpoints at once**, then clears all at `:258`.
   This is the **first identified multi-breakpoint consumer**, and neither lane knew it existed while
   debating §5.2. It is the concrete case for the plural `breakpoints` array: with two armed, *"which
   one fired"* is a live question the consumer can answer today **only from the PC**, which is exactly
   the conflation §5.2 argues against. §5.2 was adopted on a hypothetical; it now has an instance.
   It also raises the stakes on the REQUIRED fired-handle field (ruling 3): at two armed breakpoints,
   *"wrong breakpoint"* versus *"right breakpoint, wrong PC"* stops being diagnostic nicety.
   **The consumer lane's own addition, and it is the sharpest point in this section: the two
   breakpoints sit on DIFFERENT HANDLERS.** So a misattribution there does not yield a nonsense answer
   that a reader would question — it yields **a plausible one, in the other handler's frame.** That is
   the same failure class as the det-mode false-pass in §6.4, arriving from the attribution side
   rather than the precision side, and it is the strongest single argument in this CR for why the
   fired handle is REQUIRED rather than optional.

3. **`parallax_hscroll_probe.py` calls clear-all five times against a single add**, at `:548` before
   arming and at `:567`/`:571`/`:573` on what read as exit and error paths. That is **direct empirical
   support for keeping `clear {all: true}` as a distinct teardown primitive** (§4.2 / ruling 2), which
   until now rested on the consumer's stated reasoning about crashed gates rather than on observed
   code. Teardown-shaped clear-all outnumbers arming here 5:1.

**Method note, and it is this document's own instance of the failure it cites elsewhere.** Both lanes
enumerated over *"the two gate scripts"* — the files the conversation had been about — while phrasing
the result as a fact about `tools/`. Neither was careless; the scope was inherited from the discussion
and never restated as a choice, so **it was never a parameter either side could think to vary.** The
correcting command was one loop over `git ls-tree`. This is precisely the shared-frame pattern proposed
upstream to empyrean the same afternoon, caught here by a consumer choosing to re-derive a claim made
about their own code instead of accepting a flattering correction.

**The consumer lane's refinement, which is the part that generalizes: verifying a claim ADOPTS ITS FRAME
unless you deliberately widen it.** They re-derived our claim and reproduced our scope anyway, because
they checked *whether the statement about their code was true* rather than *whether it was complete*.
Those are different questions and only the first is what verification naturally asks — so **a
verification pass is not a frame change, and the shared-frame bar is not discharged by having someone
check your work.**

**A third error, arithmetic, recorded because it happened inside the correction whose own subject was
census rigor.** This section first read "17 call sites… 11 clears" while the table beside it enumerated
10. The table was right. The cause was not drift: **the 17 was a count of `grep` output rows**, one of
which — `raster_source_gate.py:33` — is a **prose comment** mentioning `breakpoint_add`, not a call
site. That is the same error the requesting overseer made in this arc's predecessor, where a fragment
count included a `$comment` key: *enumerating the rows a tool printed rather than the things those rows
represent.* Twice in two days, same seat, same shape — which makes it a mechanism problem, not a
care problem. **The guard is applied above: a count whose elements are enumerated beside it cannot
drift**, so this section's totals are written with their line numbers listed. A later edit wanting a
bare total here should delete the total instead.

**Recorded because the adjudicator should know this CR's consumer evidence was wrong THREE times before
it was right — twice on scope, once on arithmetic — and should weigh §2.2 as a census rather than as
testimony.**
