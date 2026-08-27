# CR-E — stop precision: a stop is exact, or the server says it is not

**Raised by:** the oracle lane (the ground-up Rust core + Aether server, `oracle/`).
**Against:** `empyrean` `contract/protocol.md` §2.1, §3 and §6, and
`contract/schema/bus-protocol.schema.json`, read at `origin/main` **`5625683`**.
**Extracted from:** `oracle/docs/2026-08-22-cr-a-breakpoints.md` §6 (proposal **A4**), on the instruction of
`oracle/docs/2026-08-27-ruling-cr-a.md` **M-3** (*"Raise it against §3/§6's stop surface as its own CR"*).
**Date:** 2026-08-27.
**Closes:** no audit defect. This is new normative surface.

---

## 0. How to read this document

This proposes a change to a **contract**, not to a server. It is written to be adjudicated by a reader with
no prior exposure to this repo. Every factual claim below is either quoted from a named source at a named
revision, or explicitly marked as a judgement, or explicitly marked as carried on someone else's word.

- §1 is the summary and §2 the evidence base, including **what was checked at second hand**.
- §3 is the problem, stated for a reader who has never seen this codebase.
- §4 is the proposal; §5 the exact textual and schema deltas; §6 the obligations no schema fragment can hold.
- §7 is the alternatives rejected, including **the one CR-A originally proposed**, which the ruling overrode.
- §8 is where this CR departs from the ruling or from CR-A, and why.
- §9 is the better-approach pass: where we would do better than what is proposed here, and what that costs.
- §10 is what this CR does **not** bind — there are two servers implementing this contract.
- §11 names where this CR is **weakest**, including one finding that materially weakens its own urgency.
- §12 is the questions handed to the adjudicator unanswered.

**Nothing was compiled, run, or measured for this document.** No `cargo` command was executed and no
emulator was contacted. Every behavioural claim about either server is a claim about **source text**, cited
by file and line. Cost figures are judgement estimates and are labelled as such.

---

## 1. Summary

When a debugger halts a machine on a condition that names an address, the halt can land **on** that
instruction — with its effects not yet applied — or **after** it, with its effects committed. Those are two
different machine states, and a client reading registers cannot tell which one it got.

The contract has no way to say which. A client therefore assumes the first, and a server that does the
second produces **a believable wrong answer rather than an error**. That is the failure class this contract
repeatedly names as the one it exists to prevent.

**The proposal, in one line:** a typed, ordered, three-member key `stopPrecision`, declared once per stop
condition at the handshake and carried on every `emulator/stopped` event, with the handshake declaration
**normatively binding** so that a client which never subscribes to events still has a complete answer.

| # | Change |
|---|---|
| E1 | An ordered enum `stopPrecision: "exact" \| "afterCommit" \| "approximate"`, `exact` strongest. |
| E2 | A top-level `stopPrecision` object in the `initialize` result, keyed by `reason`, one entry per stop reason the server can emit. Its **presence** is the discriminator for this amendment; its absence means the pre-amendment shape and MUST NOT be read as `"exact"`. |
| E3 | `stopPrecision` REQUIRED on **every** `emulator/stopped` event. |
| E4 | The **binding rule**: the value on an event MUST be at least as strong as the value declared at the handshake for that `reason`. A server may exceed its declaration; it may never fall short of it. |
| E5 | New normative §6 prose: a stop reports an exact PC or declares that it does not, and an imprecise mode is reachable only by explicit client opt-in. |

**What this replaces.** Today the one imprecision this workspace has actually characterised is written in
prose that clients are forbidden to parse — `protocol.md`:1163, quoted in §3.3 — and the one imprecision a
server was known to *announce* announced it in a `caveat` string, which `§2.4` rule 3 says is exactly what
a typed key is for (`protocol.md`:555, quoted in §4.6).

---

## 2. Evidence base

### 2.1 Sources read, and at what revision

| Source | Revision | How read |
|---|---|---|
| `empyrean/contract/protocol.md` | **`5625683`** (`origin/main`) | `git -C …/empyrean show origin/main:contract/protocol.md` |
| `empyrean/contract/schema/bus-protocol.schema.json` | **`5625683`** | same, parsed with `json.load` (not eyeballed) |
| `empyrean/docs/2026-08-22-protocol-schema-audit.md` | `5625683` | `git show`, grepped for CR identifiers |
| `oracle/docs/2026-08-22-cr-a-breakpoints.md` | worktree `043412b` | direct read, §6 and §11 in full |
| `oracle/docs/2026-08-27-ruling-cr-a.md` | worktree `043412b` | direct read, in full |
| `oracle/crates/oracle-core/src/bus.rs` | worktree `043412b` | direct read |
| `oracle/crates/oracle-aether/src/engine.rs` | worktree `043412b` | direct read |
| `aeon/tools/raster_source_gate.py`, `aeon/tools/snapshot_poison_gate.py` | `origin/master` **`1cee167`** | `git show`, committed blobs only |

**No empyrean or aeon file was read through a working-tree path**, and nothing in either repo was written.
Both are other lanes' live trees; every citation above is from a committed blob.

### 2.2 The currency statement — and the way the last one failed

⚑ **Read this before trusting any citation above.**

CR-A's §14.3 recorded that *"zero commits have touched `contract/protocol.md` since our anchor, so every
protocol citation in this document is current."* That sentence was true when written and **false five days
later** — the contract moved fourteen times underneath it, including one amendment (`§11.21`) that rewrote
the very rows CR-A quoted. The sentence kept asserting itself in the durable voice, in the exact section a
reader consults to decide whether to trust the citations.

So, stated the only way it can honestly be stated:

> **Every citation in this document was verified against `contract/protocol.md` and
> `contract/schema/bus-protocol.schema.json` at `origin/main` = `5625683`, on 2026-08-27.** That revision is
> named so it can be checked, not so it can be assumed. **A reader at any later date MUST re-run the check
> before relying on a line number or a quotation here:**
>
> ```
> git -C <empyrean> fetch -q origin
> git -C <empyrean> rev-parse origin/main            # if this is not 5625683, the citations are unverified
> git -C <empyrean> diff --stat 5625683 origin/main -- contract/
> ```
>
> If that diff is non-empty, treat every line number below as stale and every quotation as needing
> re-confirmation. **Nothing in this document is described as "current".**

`5625683` is `protocol: D12 names its one exemption, the deprecated wait_for_break keeps
timeoutMs/timeoutReached (hub ruling under delegation)`, dated 2026-08-27 — i.e. this CR is drafted against
a contract that moved **the same day**, which is precisely why the paragraph above is written the way it is.

**Verified firsthand at `5625683`:** `grep -c stopPrecision` returns **0** against `contract/protocol.md`
and **0** against `contract/schema/bus-protocol.schema.json`. Nothing in §11.21–§11.25 touches it. This
proposal is unlanded and has been ruled on by nobody.

### 2.3 What is carried at second hand, and is therefore not this lane's evidence

1. **The legacy C++ server's own wording.** The string *"det-mode stop granularity: PC may precede the
   breakpoint"* is quoted in CR-A §6.2 and again in `aeon/tools/raster_source_gate.py`. I verified the
   quotation exists **in aeon's source comment** at `origin/master` `1cee167` (lines 36–38). I did **not**
   read the legacy server, and I did not observe it emit that string. This is a quotation of a quotation and
   is the single most load-bearing external fact in this CR. An adjudicator who wants it confirmed should
   ask the legacy implementer, not this lane. ⟨RUNTIME⟩
2. **The 1,691,410-hit stale-breakpoint incident** cited in CR-A §2.4 and in `protocol.md`:1169. Taken as
   established; not re-derived here, and not load-bearing for this CR.

### 2.4 A correction to CR-A's evidence, found while preparing this one

**CR-A §2.3's consumer enumeration is stale, and the change cuts against this CR's urgency.** CR-A recorded
two aeon gate scripts driving `breakpoint_add` → `wait_for_break` → `breakpoint_clear`, with
`raster_source_gate.py` forcing a threaded launcher path *purely to obtain exact stop PCs*.

At aeon `origin/master` `1cee167` **both gates have been converted**, on 2026-08-26, to a single
`emulator/run_to` call against the Rust core:

> `snapshot_poison_gate.py`:38 — *"the breakpoint triple became one `run_to`"*
> `raster_source_gate.py`:143 — *"No breakpoints to clear (the Rust core serves none)"*
> `raster_source_gate.py`:38–40 — *"The Rust core has no such mode and no such caveat — `emulator/run_to`
> parks on the exact instruction — so the knob is gone and the requirement is simply met."*

Three consequences, all stated against this CR's interest:

- **There are now zero automated consumers of `breakpoint_add` or `wait_for_break`** in aeon's tools tree
  that read a stop PC. The workaround CR-A cited as live cost is gone.
- **The surviving consumers read their stop PC from a method *result*** (`run_to`'s `pc`, plus a separate
  `emulator/registers`), **not from `emulator/stopped`.** They are not events subscribers. This is what
  forces §4.4's binding rule: a design that puts the key only on the event is structurally blind to the
  only consumer this surface has.
- **The exactness the gate depends on is asserted in a Python comment**, about a Rust server, with no
  wire-level way for the gate to check it. That is the residual harm, and it is smaller and quieter than
  the one CR-A described. §11.1 says so plainly rather than letting it sit here.

---

## 3. The problem, for a reader who has never seen this codebase

### 3.1 The machine, in three sentences

A Sega Genesis executes 68000 instructions one at a time. A debugger attached over this bus can ask the
machine to stop when the program counter reaches a chosen address, or when a chosen memory location is
touched. When it stops, the client reads registers and memory and decides something.

### 3.2 Why one instruction matters

Consider the real fixture that motivated this clause (`aeon/tools/raster_source_gate.py`:20–33, read at
`1cee167`; the assembly and the reasoning are the gate author's, quoted):

```
                 adda.w (a1)+, a2          <-- the source pointer is complete HERE
.region_loop:    move.w (a2)+, VDP_DATA    <-- so we break at this label
```

The gate arms a stop at `.region_loop` and asserts that register `a2` holds a specific computed address.
The gate's own comment:

> *"A stop one instruction early lands BEFORE `adda.w`, where a2 still holds `Pal_Variant_Stage`
> unmodified — a plausible-looking value that would make this gate pass on code that never applied the
> offset at all."*

That is the entire problem in one sentence. The wrong stop does not crash, does not error, and does not
look wrong. It produces **a verdict**, the verdict is wrong, and the verdict is pasted into merge evidence.

**The failure mode that hurts is not an imprecise stop. It is an imprecise stop presenting as a precise
one.** (CR-A §6.1; adopted verbatim, and the ruling calls it *"the correct framing"*.)

### 3.3 The contract already documents an imprecision it forbids clients to read

This is not hypothetical and it is not confined to the legacy server. `contract/protocol.md`:1162–1165 at
`5625683`, describing what happens when a watchpoint ends a run:

> *"With `stopAfter: n` the run ends at the next instruction boundary once that watch has matched `n`
> accesses, **with the triggering instruction fully committed**, emitting `emulator/stopped` with
> `reason: "watchpoint"`…"*

So the contract states, in prose, that a watchpoint stop lands **after** the instruction that caused it. A
client reading registers after such a stop must act on that fact. And `§2.4` rule 3, at
`protocol.md`:553–556, is:

> *"Clients **MUST NOT parse** it… **Any consequence a client must act on needs its own typed key**."*

A consequence a client must act on is living in prose. That is what rule 3 forbids. The typed key does not
exist.

### 3.4 And the same property is asserted about the successor only in a source comment

`oracle/crates/oracle-core/src/bus.rs`:343–348, worktree `043412b`, verbatim:

> *"The machine always stops **at an instruction boundary, never mid-instruction**, with `pc` pointing at
> the instruction that has *not* yet executed. So a sink that raises its flag from `on_step_boundary(pc, _)`
> gets classic breakpoint semantics (stop *before* `pc` runs); a sink that raises it from
> `on_event`/`on_vdp_write` — i.e. in the middle of an instruction — stops at the *next* boundary, after the
> triggering instruction has fully committed."*

Both halves of the proposed vocabulary are in that one paragraph: a PC-armed stop is `exact`, an
access-armed stop is `afterCommit`. **One server, two precisions, on the same wire, today.** Nothing on the
wire distinguishes them.

*(CR-A cited this as `bus.rs:305-318`; the citation has drifted to 338–351. The claim survives; the line
numbers did not — which is §2.2's point restated.)*

---

## 4. The proposal

### 4.1 The enum, and its ordering

```
stopPrecision : "exact" | "afterCommit" | "approximate"
```

- **`"exact"`** — the machine is halted at an instruction boundary; the instruction at `pc` has **not**
  executed; and where the stop had a triggering address, `pc` **is** that address. Resuming executes the
  instruction at `pc`.
- **`"afterCommit"`** — the machine is halted at an instruction boundary and the instruction at `pc` has not
  executed, but the stop was caused by the instruction **immediately before** `pc`, which has **fully
  committed**. A client MUST read state as post-trigger, not pre-trigger. This is `protocol.md`:1163's
  watchpoint stop, given a name.
- **`"approximate"`** — `pc` is near the triggering address and the server promises **nothing** about which
  side of it, or by how much. A client MUST NOT read register or memory state as though the instruction at
  the triggering address had, or had not, executed.

`"approximate"` is deliberately a refusal to promise rather than a bounded error term. The one server known
to need it says *"may precede"* with no bound (§2.3), and inventing a bound the implementation does not hold
would recreate the defect one level down.

**The three values are totally ordered, strongest first**, and the ordering is normative because §4.4 needs
it: `exact` > `afterCommit` > `approximate`.

### 4.2 Level 1 — the handshake, as a per-reason map

A new **top-level** key of the `initialize` result:

```json
"stopPrecision": {"breakpoint":"exact", "watchpoint":"afterCommit", "step":"exact",
                  "runTo":"exact", "runToScanline":"exact", "runFrames":"exact",
                  "pause":"exact", "entry":"exact"}
```

**Keyed by `reason`, one entry per `reason` value this server can emit** — a server that serves no
breakpoints omits `breakpoint`, exactly as it omits the method. The key set is therefore derived from what
the server implements, on `methodSummaries`' key-set-equality discipline (`protocol.md`:470–471).

**Why top-level and not a capability.** Three reasons, in descending strength:

1. `capabilities.breakpoints` — where CR-A §6.3 put it — **is still a boolean** at `5625683`, and §11.21
   ruled deliberately that it stays one: *"a boolean a client already reads cannot become an object without
   breaking that client (§11.18's rule, applied to a capability)"* (`protocol.md`:1151–1152). CR-A's
   placement is unavailable. (Ruling **M-4**.)
2. `breakpoints` was always the wrong home for a key whose scope includes `runTo`, `step` and `watchpoint`.
3. Top-level is the house answer for *"every server has one, and `capabilities` is for what a server may
   lack"* — the argument `protocol.md`:372–375 gives for `implementation`/`serverBuild`, and
   `protocol.md`:434–436 gives for `timingBasis`. Every server that stops a machine has a stop precision.

**Why not `limits`.** `limits` is the other placement the ruling offered. Rejected on the document's own
words: `protocol.md`:450 — *"Three fields, all required, **all JSON numbers** (D9 category 2)"* — and every
one of the nine keys in that object at `5625683` is `{"type":"integer"}`. A string-valued map in `limits`
breaks the one property that object has. See §7(d).

**Presence is the discriminator; absence is meaningful and is NOT `"exact"`.** A server that omits the key
serves the pre-amendment shape. This is `limits.maxBreakpoints`'s exact device — *"OPTIONAL, and its absence
is meaningful: a server that omits it serves the PRE-AMENDMENT breakpoint shape"* (schema, `maxBreakpoints`
description at `5625683`) — and it is the only additive-safe way to introduce the key, because a shipping
server cannot be retroactively required to send one (D5, §11.18).

⚑ **The clause this hinges on, stated normatively:** *a client MUST NOT read the absence of `stopPrecision`
as a promise of exactness.* Absence means **"this server does not answer the question"**, which is a third
state, and conflating it with `"exact"` would rebuild the exact defect this CR exists to close — one level
up. §2.4 clause (a)'s reasoning about `truncated` (*"absence and `false` must not both mean 'you have
everything'"*, `protocol.md`:583–584) is the same argument in the same shape.

### 4.3 Level 2 — `emulator/stopped`, on every event

`stopPrecision` is **REQUIRED on every `emulator/stopped`**, unconditionally.

Not on four reasons; on all of them. The rule is the ruling's own Q2 argument taken to its end: *"every
`reason` that reports a `pc` carries a precision, so a subscriber never has to know which reasons opted
in."* `pc` is already unconditionally required on that event — the schema fragment's `required` array at
`5625683` is exactly `["reason","pc"]` — so **every** `stopped` event reports a PC and every one of them has
a precision. `pause`, `entry` and `runFrames` have no triggering address, so their answer is `"exact"` by
the definition in §4.1 and costs a constant string; what it buys is a subscriber that never has to carry a
table of which reasons opted in, and never has to distinguish "this reason has no precision" from "this
server did not send one".

The alternative — conditional on four reasons — is §7(e), rejected.

### 4.4 The binding rule, which is what makes two levels sufficient

> **A server MUST NOT emit a stop weaker than it declared.** For a given `reason`, the `stopPrecision` on
> an `emulator/stopped` event MUST be **at least as strong** as the value the server declared for that
> `reason` in its `initialize` result. A server whose precision for a reason can vary between stops MUST
> declare the **weakest** value it may emit. A server may emit a stronger value than it declared; it may
> never emit a weaker one.

**This is the clause CR-A did not have, and without it the two-level design is broken.** §2.4 established
that the only consumers of this surface read their stop PC from a **method result** (`run_to`'s `pc`), and
never subscribe to `emulator/stopped`. Under a bare two-level design such a client has no per-stop answer at
all. The binding rule gives it one without a single new key on any result row:

> **Resolution rule (normative).** A client that learns a stop PC from a **reply** rather than from
> `emulator/stopped` resolves that stop's precision by looking up, in the handshake `stopPrecision` map, the
> `reason` the invoked method produces (`run_to` → `runTo`, `step`/`step_over`/`step_out` → `step`,
> `run_to_scanline` → `runToScanline`, `wait_for_break` → whatever `reason` ended the wait, and when that is
> not determinable the client uses the **weakest** value in the server's map). The declaration is a floor, so
> this resolution is never optimistic.

Pessimism is safe here and optimism is not: the entire failure class is a client believing a stop is better
than it is. A client that under-trusts a stop refuses to produce a verdict; a client that over-trusts one
produces a wrong verdict. This rule can only ever produce the first.

### 4.5 Level 3 — struck

CR-A proposed a third level, on `breakpoint_add`'s reply. **It is struck**, per ruling **M-5**, which adopts
CR-A's own §11.7 self-assessment:

> *"The **arm-reply** level is the redundant one… If the adjudicator strikes one, strike that one; the
> two-level version keeps the whole property."*

It is struck here without argument or reservation. The three-level version also cost a REQUIRED key on a row
§11.21 had just rewritten, which would have been the third change to `breakpoint_add`'s result in six days.

### 4.6 The normative rule, stated generally

Requested as new §6 prose:

> **A stop either reports an exact PC or declares that it does not.** Whenever a server halts the machine
> and reports the PC at which it halted, that PC MUST be exact — the instruction at `pc` has not executed,
> and where the halt had a triggering address, `pc` is that address — unless the event carries
> `stopPrecision` naming a weaker granularity, or the server declared a weaker granularity for that
> `reason` at the handshake. A server that cannot offer exact stops in some mode MUST make that mode
> reachable only by **explicit client opt-in**, MUST declare the weaker granularity for every affected
> `reason` in its `initialize` result, and MUST carry it on every `emulator/stopped` the mode produces. A
> server MUST NOT offer imprecise stops as its default, and MUST NOT report an imprecise stop without the
> key. A client MUST NOT read the absence of `stopPrecision` from a server's `initialize` result as a
> declaration of exactness; absence means the server does not answer the question.

The carrier is a typed key and not `caveat`, on `§2.4` rule 3 at `protocol.md`:553–556 — *"Any consequence a
client must act on needs its own typed key"* — and clients **MUST NOT parse** `caveat`. A server that ships
this warning as prose ships something no gate can branch on, which is exactly what the legacy server did and
exactly why a consumer hard-coded a launcher workaround around it (§2.3, §2.4).

---

## 5. The exact deltas requested

Current text quoted first in every case, so the delta is visible rather than reconstructed. All quotations
are from `5625683`; see §2.2 before relying on a line number.

### 5.1 `contract/protocol.md` §2.1 — the handshake example, line 359

**Current** (the `limits` line of the `initialize` response example):
```
    "limits":{"maxRunFrames":3600,"maxReadLen":4096,"maxLineBytes":1048576},
```
**Proposed** — one line added after it:
```
    "limits":{"maxRunFrames":3600,"maxReadLen":4096,"maxLineBytes":1048576},
    "stopPrecision":{"watchpoint":"afterCommit","step":"exact","runTo":"exact","runToScanline":"exact","runFrames":"exact","pause":"exact","entry":"exact"},
```
*(The example server advertises `"breakpoints": false`, so it correctly has no `breakpoint` entry — see
§10.2.)*

### 5.2 `contract/protocol.md` §2.1 — new prose, after the `limits` block (which ends at line 456)

A new block in the style §2.1 uses for `limits` and `timingBasis`, carrying: the enum and its ordering
(§4.1); the key-set rule (§4.2); the presence-is-the-discriminator rule and the **absence-is-not-exact**
clause (§4.2); the binding rule and the resolution rule (§4.4).

### 5.3 `contract/protocol.md` §3 — the `emulator/stopped` row, line 714

**Current** (params column, tail):
```
…, `buttons`?, `port`?, `watch`?, `breakpoint`? *(§11.21)*
```
**Proposed:**
```
…, `buttons`?, `port`?, `watch`?, `breakpoint`? *(§11.21)*, **`stopPrecision`** *(§11.26)*
```
**The `reason` enum is unchanged.** No new stop condition is created by this CR.

*(`§11.26` is a placeholder for whatever amendment number this lands as; `§11.25` is the last one at
`5625683`.)*

### 5.4 `contract/protocol.md` §3 — new bullet, after the `watch` bullet (lines 750–753)

> - **`stopPrecision`** — the relationship between `pc` and the condition that halted the machine.
>   **REQUIRED on every `emulator/stopped`**, for every `reason`: the event always reports a `pc`, so it
>   always has a precision, and a subscriber never has to know which reasons opted in. Its value MUST be at
>   least as strong as the value this server declared for this `reason` in its `initialize` result (§2.1).

### 5.5 `contract/protocol.md` §6 — new normative blockquote

§4.6's paragraph, filed in §6 beside the run-control state rule, since its scope is every halting op in that
section and not the breakpoint rows alone.

### 5.6 Schema — `contract/schema/bus-protocol.schema.json`

| Fragment | Change |
|---|---|
| `$defs` | **new** `stopPrecision`: `{"enum":["exact","afterCommit","approximate"], "description":"…ordered, strongest first…"}`. A shared `$def` because the value appears in two places and a copied enum is a drift source. |
| `handshake.initialize.result.properties` | **new** `stopPrecision`: `{"type":"object","minProperties":1,"additionalProperties":false,"properties":{ <each of the eight `reason` values>: {"$ref":"#/$defs/stopPrecision"} }}`. **Not** added to that result's `required` array — its absence is the pre-amendment signal (§4.2). |
| `events["emulator/stopped"].params.properties` | **new** `stopPrecision`: `{"$ref":"#/$defs/stopPrecision", "description":"…"}` |
| `events["emulator/stopped"].params.required` | `["reason","pc"]` → `["reason","pc","stopPrecision"]` |

**On making it `required` in the event fragment.** This invalidates every `emulator/stopped` on the wire
today, including the contract's own canonical example at `protocol.md`:330. That is precedented, one
amendment ago and in the same fragment: §11.21's M2 clarification (ii) made `breakpoint` REQUIRED and ruled
that *"a pre-amendment (legacy) server emits no handle and is **not validated** by this schema for that
event — it is frozen, not conformed"* (`protocol.md`:1140–1144). The same disposition is requested here, and
§2's example gains a `stopPrecision` for the same reason it gained a handle. §11.3 records this as a
weakness rather than hiding it in a table.

The `methods` object is untouched: no method row gains a key, and no fragment is added. The object maps
method name → fragment **directly**, with no `properties` sub-object; it holds **63** entries at `5625683`
(62 fragments plus one `$comment`), unchanged by this CR.

---

## 6. ⚠ Obligations this CR creates that **no schema fragment can express**

The schema has a structural property that the acceptance survey established and I re-confirmed by parsing
the file at `5625683`: **no fragment declares any error condition.** Every error obligation on this bus lives
in prose and cannot be validated against a fragment. CR-A's §9.7 flagged this for the four obligations it
created; this CR flags it for four of its own, and one of them is the load-bearing clause.

| Obligation | Why no fragment can hold it |
|---|---|
| **The binding rule (§4.4)** — an event's value must be ≥ the handshake declaration for that `reason` | It relates **two different messages** on a connection. JSON Schema validates one document. |
| **The key-set rule (§4.2)** — the handshake map's keys must be exactly the reasons this server can emit | The set of reasons a server can emit is not present in the handshake; it is implied by `methods` and `capabilities`, and no fragment can cross-reference them. |
| **"Absence is not `exact`" (§4.2)** — a *client*-side rule | The schema constrains servers, not clients. Nothing validates a client's inference. |
| **The opt-in rule (§4.6)** — an imprecise mode must not be the default | "Default" is a property of a server's launch configuration, which never appears on this bus at all. |

**Stated loudly, because this is the same failure class one level up:** a conformance suite that validates
replies against fragments is **blind to all four**, and the first of them is the clause that makes the
two-level design sufficient. This CR therefore ships a rule that no automated gate can hold. §9.3 proposes
the only thing that would actually close it, and §12 Q6 hands the sequencing question to the adjudicator.

---

## 7. Alternatives rejected

**(a) A boolean `exact: true | false`.** *(CR-A §6.6(a)'s rejection, and the ruling vindicated it.)* A
boolean cannot grow a third granularity without becoming a lie. CR-A argued *"a granularity vocabulary is
exactly the kind of thing that grows"* as a prediction; it grew **on the first extension**, before landing,
when `protocol.md`:1163's watchpoint stop turned out to need a value that is neither exact nor
unbounded. Rejected on evidence, not on taste.

**(b) The two-member enum `"exact" | "approximate"` — what CR-A actually proposed.** Rejected per ruling
**M-6**. Reporting a watchpoint stop as `"approximate"` — *"the server promises **nothing** about which side
of it or by how much"* — would be **strictly worse than the prose it replaces**, because `protocol.md`:1163
already promises something precise: exactly one instruction boundary later, always, by construction. A typed
key that discards information the prose carried is not an improvement. Three members from the start.

**(c) Carry the warning in `caveat`.** Rejected on `§2.4` rule 3 (`protocol.md`:553–556) and on the observed
fact that the legacy server already does exactly this, in prose, and its consumer had to hard-code a
launcher workaround because a prose string is not something a gate can branch on (§2.3).

**(d) Put the handshake level in `limits`.** One of the two placements ruling **M-4** offered. Rejected:
`limits` is documented as *"all JSON numbers (D9 category 2)"* (`protocol.md`:450) and every key in it at
`5625683` is an integer. A string map there costs that object its one uniform property, and gains nothing a
top-level key does not already have.

**(e) Make the event key conditional on four reasons** (`breakpoint`, `watchpoint`, `step`, `runTo`), with an
`if`/`then`/`else` mirroring `watch`'s. Rejected: it forces every subscriber to carry a table of which
reasons opted in, and it makes "no precision for this reason" indistinguishable from "this server did not
send one" — the §4.2 defect, rebuilt inside the event. It also breaks the moment a future stop reason is
added, since an event field added later is permanently optional (CR-A §5.3's argument, which the ruling
endorsed).

**(f) A per-reason map at the handshake versus a single scalar floor.** The scalar is smaller and would be
adequate for a server whose stops are uniformly precise. Rejected because **our own server is not such a
server**: §3.4 shows one implementation emitting two different precisions, so a scalar would force it to
declare `afterCommit` for everything — understating its breakpoints, training clients to ignore the field,
which is the `read_memory` constant-caveat pathology `protocol.md`:565–572 warns about by name. The scalar
survives as the retreat shape (§11.5).

**(g) Refuse to serve stops at all in an imprecise mode.** *(CR-A §6.6(c), retained.)* It is not this
contract's place to forbid a server a mode; it is this contract's place to forbid a server a **silent** one.
The opt-in requirement in §4.6 is the enforceable half.

**(h) Do nothing, on the grounds that the imprecise server is being retired anyway.** This is the strongest
rejected alternative and it is argued against this CR in §11.1 rather than dismissed here.

---

## 8. Where this CR departs from the ruling, or from CR-A

Recorded explicitly, because the brief that commissioned this document says the ruling wins where they
conflict, and a reader is entitled to see every place I moved.

| # | Departure | From | Why |
|---|---|---|---|
| D1 | The handshake level is a **map keyed by `reason`**, not a scalar | Neither ruling nor CR-A specified a shape | M-4 said *"the drafters should pick and argue one"*. I pick, and argue in §4.2/§7(f). **The ruling did not notice that M-4 and M-6 interact**: once M-6's third member exists, a scalar is untruthful on the one server we can inspect. That interaction is my finding, not the ruling's. |
| D2 | `stopPrecision` is REQUIRED on **every** `stopped`, not on four reasons | CR-A §6.3 (four reasons) | The ruling's own Q2 rationale — *"every `reason` that reports a `pc` carries a precision"* — and `pc` is unconditionally required. This goes **further** than the ruling stated. §11.3 books the cost. |
| D3 | A **binding rule** and a **resolution rule** (§4.4) that neither document contains | new | §2.4's finding: the only consumers read the stop PC from a *reply*, never from the event. Without this, the ruling's two levels leave the actual consumer with nothing. This is the largest addition in this CR and the one most likely to be wrong (§11.4). |
| D4 | `limits` placement rejected outright | Ruling M-4 offered it as one of two | `protocol.md`:450. §7(d). |
| D5 | The arm-reply level is struck | CR-A §6.3 | Ruling **M-5**, adopted without reservation. §4.5. |
| D6 | Three enum members | CR-A §6.3 (two) | Ruling **M-6**, adopted. §7(b). |
| D7 | CR-A's §2.3 consumer facts are reported as **stale** | CR-A | Verified firsthand at aeon `1cee167`. §2.4. This cuts against the CR. |

**I do not believe the ruling is wrong on any point it decided.** M-3, M-5 and M-6 are each adopted in full,
and M-4 is executed rather than argued with. The one place I would push back is not a disagreement but a
gap: the ruling framed the design as *"two levels: handshake and `emulator/stopped`"* and did not consider
that the surviving consumers are reply-readers rather than subscribers, because CR-A's consumer enumeration
was stale in both documents. §4.4 fills that gap without adding a level. If the adjudicator holds that
§4.4's binding rule is itself a third level in disguise, then Q1 in §12 is the right place to say so, and
the honest answer is that it is a third level costing zero keys.

---

## 9. Better-approach pass — where we would do better than this

This lane's standing directive from the owner is that **a contract fragment or a legacy surface is the
compatibility floor, never the design ceiling.** Three things would be better than what §4 proposes. None is
proposed, and each is priced.

### 9.1 `triggerPc` — say *which* instruction committed, not just that one did

An `afterCommit` stop tells a client that the instruction before `pc` committed. It does not say **which
instruction that was**, and on a 68000 the client cannot compute it: instructions are variable-length and
you cannot decode backwards. So a client told `afterCommit` knows its state is post-trigger and still cannot
name the trigger without disassembling forward from a known-good anchor.

**Better:** an optional `triggerPc` (hex string) on `emulator/stopped`, REQUIRED when `stopPrecision` is
`"afterCommit"` — the address of the instruction that caused the halt. Our core has it: the watch fires from
`on_event`/`on_vdp_write` **during** that instruction, so the executing PC is in hand at the moment the flag
is raised (`bus.rs`:346–348).

**Cost to a consumer:** zero. It is additive, it is optional except where a server already emits
`afterCommit`, and a client that ignores it is exactly as well off as under §4. **Why it is not proposed:**
it widens a CR the ruling explicitly asked to be kept small and self-contained, and it should be ruled on
after the vocabulary exists rather than with it. Handed to the adjudicator as §12 Q2.

### 9.2 A bound, not a word

`afterCommit` is a vocabulary item standing in for the number **1**. A strictly more honest design is
`instructionsFromTrigger: 0 | 1 | null` — zero for exact, one for a watch stop, `null` for an unbounded
server — or a signed range `{min, max}` for a server that can characterise its slop.

**Why it is better:** it is machine-checkable arithmetic instead of a string a client must have a table for,
and it **cannot grow a vocabulary**. §7(a) records that this vocabulary grew before it landed; a design
whose extension is a number does not have that failure mode.

**Cost to a consumer:** real, and it is why this is not proposed. Every client does arithmetic where it
currently reads a word, `null` is `"approximate"` re-spelled with less clarity about *why*, and it presumes
imprecision is always instruction-countable — which is true for both servers we know of and is an assumption
about all future ones. **Judgement:** the vocabulary is the right call for this bus today and the number is
the right call for a bus with three imprecise servers. We do not have three.

### 9.3 Do not let this be a declaration nobody checks — the strongest of the three

§6 establishes that the binding rule cannot be validated by any fragment. A contract clause that says
*"declare your precision truthfully"* and is checked by nothing is **the same failure class as the bug it
fixes, moved one level up**: a server asserts something believable, no gate contradicts it, and a client
draws a confident wrong conclusion.

**Better:** land this CR **with** a §8 conformance item that *proves* the declaration instead of trusting
it. A concrete, cheap shape: arm a stop at an instruction with a single observable register effect, halt,
read the register, and assert the pre-state for a declared `exact` and the post-state for a declared
`afterCommit`. That is a differential test with an anti-vacuity property built in — it fails if the server
lies in either direction — and it is exactly the shape the aeon gate in §3.2 already uses for its own
purpose.

**Cost to a consumer:** zero; it costs the *implementers* one test each. **Why it is not proposed:** §8's
checklist is a section this CR does not otherwise touch, and whether a normative rule may arrive with its
own acceptance item is a sequencing call above this lane. Handed to the adjudicator as §12 Q6. **If the
adjudicator adopts exactly one thing from this section, it should be this one.**

---

## 10. What this CR does and does not bind

**It binds the contract.** It binds no server's schedule and asserts no conformance date. There are two
implementations of this contract, and they are asked for different things.

### 10.1 `oracle-cpp` — the legacy C++ server: **nothing is asked of it**

It is the one implementation known to have an imprecise stop mode (§2.3, at second hand). Under §11.19's
D-33 precedent and §11.21 design choice 4 — *"Legacy is frozen, not migrated"* — it is not asked to
implement this CR, and a `stopped` event it emits without `stopPrecision` is **not validated by this schema**
for that event, on the disposition §11.21's M2 clarification (ii) already set for `breakpoint`
(`protocol.md`:1140–1144). It is frozen, not conformed.

**One consequence must be said out loud rather than left implicit:** because a legacy server omits the
handshake key, a client correctly applying §4.2 reads its absence as *"this server does not answer the
question"* — **not** as `"approximate"` and **not** as `"exact"`. A gate that requires exactness must
therefore refuse to run against it. That is the intended outcome and it is the behaviour the aeon gate
hard-coded a launcher workaround to achieve (§2.3). It is also, functionally, the moment this CR becomes a
migration pressure on a server nobody plans to migrate — which is the correct direction, but it should be
adopted knowingly and not discovered later.

**No compatibility flag is proposed**, for D11's reason: a `stopPrecision: "unknown"` escape would let the
defect survive negotiation forever and force every client to carry both paths. Absence already carries that
meaning, exactly once, in one place.

### 10.2 `oracle-rs` — the Rust successor: two additive emissions, and one thing it may not yet claim

Verified firsthand at worktree `043412b`: `crates/oracle-aether/src/engine.rs`:1473 advertises
`"breakpoints": false` — **this server serves no breakpoints at all today.** So conformance costs it:

1. a top-level `stopPrecision` map in its `initialize` result, with **no `breakpoint` entry** until it
   serves breakpoints, and `"watchpoint": "afterCommit"` from the moment it serves a `stopAfter` watch;
2. `stopPrecision` on every `emulator/stopped` it emits.

⟨RUNTIME⟩ **And one prohibition.** §3.4's claim that this core stops exactly is read off a doc comment at
`bus.rs`:343–348 and has **not** been confirmed by running anything — no emulator was contacted for this
document. **This lane MUST NOT assert `"exact"` in a handshake until someone has confirmed it at runtime in
the foreground.** A source comment is not a measurement, and shipping a `"exact"` declaration on the strength
of one would be this CR's own failure mode committed by its author. This is CR-A §11.8's tag, carried
forward and sharpened into a prohibition.

---

## 11. Where this CR is weakest

Written so an adjudicator can rule against it on the merits without having to find the soft joints first.
The ruling judged CR-A's equivalent section *"both under- and over-stated in places"*, so these are ordered
by how much they should actually worry a reader, strongest objection first.

### 11.1 The motivating harm is now historical, and nothing would branch on this key today

§2.4 is the finding that most weakens this CR, and it was found while preparing it. Both aeon gates migrated
off the imprecise server on 2026-08-26; the launcher workaround is gone; **there is no client today that
would read `stopPrecision`.** A reasonable adjudicator could hold that this is a rule written for a hazard
that has already been solved by attrition, and that §7(h) — do nothing — is the right answer.

**The counter, and it is a judgement, not a derivation:** the hazard was solved by a *consumer* migrating,
not by the *contract* learning to express the property. The property is still undeclarable, the successor's
exactness is still asserted only in a Rust doc comment (§3.4), the successor still has **two** different
precisions on one wire, and `protocol.md`:1163's watchpoint imprecision is still living in prose that
clients are forbidden to parse. The next imprecise server — or the next mode of this one — arrives to the
same silence. But an adjudicator who weights demonstrated present cost over prevented future cost should
rule against this CR, and this section is the place to do it.

### 11.2 `afterCommit` has exactly one producer, and the contract already says so in words

The third enum member's entire evidentiary basis is `protocol.md`:1163 — one sentence, about one feature
(`stopAfter` watches). A client can already read that sentence. The marginal value of the typed key is that
it is machine-checkable and that a subscriber does not have to know the feature exists. `§2.4` rule 3 makes
that a **rule** rather than a preference, which is why this CR proposes it — but a reasonable adjudicator
could call the yield low and prefer the two-member enum with a prose carve-out for watchpoints. I think that
is wrong (§7(b)) and I am not certain it is.

### 11.3 REQUIRED on every `stopped` invalidates every event on the wire

Including the contract's own canonical example at `protocol.md`:330. §5.6 shows the precedent — §11.21 did
exactly this to the same fragment on 2026-08-26 — but that is the *point*: this would be the **second**
REQUIRED key added to a shared event in two weeks, by the same lane, each time with a precedent supplied by
the last one. A precedent chain of length two is how a shared event becomes something no independent
implementer can emit. An adjudicator who wants to stop that ratchet should stop it here, and the retreat is
§7(e)'s conditional form, which I argue against but which is better than losing the key.

### 11.4 The binding rule is invented here and has no precedent on this bus

§4.4 is the clause that makes two levels sufficient, it is the largest thing in this CR that neither the
ruling nor CR-A contains, and it is **cross-message** — no fragment can hold it (§6), and it constrains a
server's behaviour across the lifetime of a connection, which is a shape this contract has not previously
used. It also assumes precision is **totally ordered**. `exact` > `afterCommit` > `approximate` is
defensible, but a future imprecision that is neither better nor worse than `afterCommit` would have no place
in the lattice, and the rule would then be uninterpretable rather than merely wrong. **If the adjudicator
rejects §4.4, the two-level design does not serve the only consumers this surface has, and §12 Q1 flips
from "no" to "yes".**

### 11.5 The per-reason map may be over-built for the problem

Eight keys where one string might do. §7(f) argues it from our own server's two precisions, which is one
data point. The retreat is a scalar declaring the server's **weakest** stop precision, which keeps the
"should I run against this server at all" property and loses only the ability to say *"my breakpoints are
exact even though my watches are not"*. That loss is real but it is not the false-pass this CR exists to
prevent.

### 11.6 `afterCommit` is an invented spelling, and it names a mechanism in a vocabulary about accuracy

`exact` and `approximate` describe how close the answer is. `afterCommit` describes **why** it is where it
is. A vocabulary that mixes the two axes is one a future member will not fit cleanly. Alternatives
considered and not preferred: `postTrigger`, `nextBoundary`, `exactPlusOne`. The ruling explicitly left the
spelling to the drafters (*"spelling is the drafters'"*); this is the drafters' call and it is the weakest
of them.

### 11.7 Nothing here was verified at runtime, and one key fact is second-hand at one remove

No emulator was contacted; no `cargo` command was run. §3.4's claim about this lane's core is source text.
And the legacy server's `"det-mode stop granularity"` string — the concrete precedent the whole CR is built
on — is quoted here from **aeon's source comment about the legacy server**, not from the legacy server
(§2.3). If that comment is wrong, §3's motivating precedent is a legend. I judge it very unlikely to be
wrong (it names the exact opt-out flag and the exact behaviour) and I did not verify it. ⟨RUNTIME⟩

### 11.8 The key-set rule is unenforceable and might be better dropped

§4.2 requires the handshake map's keys to be exactly the reasons the server can emit. Nothing can check that
(§6), and a server that over-declares (an entry for a reason it never emits) harms nobody. The rule may be
ceremony. It is kept because an under-declared map is a silent gap — a client looking up `watchpoint` and
finding nothing has to decide what that means, and this CR would rather it never happen than specify a
fourth interpretation of absence.

---

## 12. Open questions handed to the adjudicator

Not answered here, deliberately.

1. **Should stop-reporting *replies* carry `stopPrecision` too?** `run_to`, `step`, `step_over`, `step_out`
   and `wait_for_break` all report a `pc` a client acts on, and §2.4 shows the only consumers read it there.
   §4.4 answers them with a binding handshake declaration and zero new keys; the alternative is five row
   edits and a key that is right at each individual stop. **This question is downstream of §11.4:** if the
   binding rule is rejected, the answer must become yes.
2. **Should `afterCommit` carry a companion `triggerPc` now, or later, or never?** §9.1 argues it is the
   most useful thing missing, that our core has the value in hand, and that it costs a consumer nothing.
   It is not proposed only because it widens a CR that was ordered kept narrow.
3. **Is presence-of-the-handshake-key an adequate amendment discriminator?** §11.21 gave clients a
   *`methods`-list* discriminator (`breakpoint_set_enabled`'s presence). This CR adds no method, so the only
   discriminator available is the key's own presence in `initialize` — a client must read a key's absence to
   learn a server's shape. That worked for `limits.maxBreakpoints`; whether it works for a key whose absence
   must **not** be read as a default is a different question.
4. **Does §4.6's opt-in clause have any live subject?** The only server known to default to an imprecise
   mode is frozen and will never implement this. The clause therefore binds nobody today. Ship it as a rule
   for future servers, or strike it as dead letter?
5. **Does the vocabulary already need a fourth member?** A server that steps by *cycles* rather than
   instructions, or one that halts on a DMA boundary, is neither `exact`, nor one instruction late, nor
   unbounded. §7(a) records that this vocabulary grew once before landing. If the adjudicator expects it to
   grow again, §9.2's numeric bound becomes the better design and this CR should be re-cut.
6. **May a normative rule land with its own §8 conformance item?** §6 establishes that this CR's central
   clause cannot be validated by any fragment, and §9.3 argues that an unchecked declaration is this CR's
   own failure class one level up. Whether the acceptance item rides along or is raised separately is a
   sequencing call above this lane's pay grade — but the two arriving apart means a window in which the rule
   exists and nothing checks it.
7. **Does this want a `protocolVersion` bump?** Every change here is additive under D5 except the event
   fragment's `required` array, which §5.6 disposes of on §11.21's precedent. If that precedent is judged to
   have been stretched far enough, this is the CR that should pay for the bump.

---

## 13. Provenance

Drafted by the oracle lane, 2026-08-27, against `empyrean` `origin/main` `5625683`, `aeon`
`origin/master` `1cee167`, and this repo at `043412b`. It is the extraction of CR-A's **A4** ordered by
`docs/2026-08-27-ruling-cr-a.md` **M-3**, incorporating **M-4** (re-sited handshake level), **M-5** (arm-reply
level struck) and **M-6** (three enum members). §8 lists every place it departs from either document.

The identifier **CR-E** was chosen as the next free letter: `cr-a`, `cr-b`, `cr-c`, `cr-d` and `cr-bank` are
taken in `oracle/docs/`, and `CR-BP` (§11.21), `CR-1` and `cr-socket-evidence` are taken in `empyrean/docs/`
at `5625683`; `CR-E` appears in neither repo. The ruling's suggested name `CR-STOP-PRECISION` is preserved
as this document's subtitle so a reader following the ruling finds it.

**No emulator was contacted, no `cargo` command was run, and nothing outside this repository was written.**
