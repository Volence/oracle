# CR-S — `emulator/lookup_equate`'s prefix reply and `$defs/boundedList`: a consistency question, **not the conformance defect it was filed as**

**From:** oracle lane, 2026-09-09.
**Occasion:** lens finding **H33**, filed 2026-09-06 in `docs/superpowers/notes/2026-09-06-oracle-lens-sweep.md`.
**Status: ADJUDICATED 2026-09-09, ADOPT WITH CHANGES — option A with three MUSTs**, at empyrean `90fc5c4`, `contract/protocol.md` §11.43. Verified here: ancestor of their `origin/main`, a contract commit carrying `protocol.md` +74, and **only prose moved — no schema, no vectors — so no re-vendor is owed**. B and C were rejected on this document's own pricing.

⚠ **Registered upstream as CR-T, not CR-S.** This lane picked `CR-S` without checking and that label belongs to §11.42, the pacing amendment of 09-06. Same collision, and the same remedy §11.42 itself records. The filename is left as written rather than renamed, since it is cited at `4b3b495`.

**What §11.43 added beyond option A as filed:** `boundedList` binds a reply only through that reply's own §6 row; the "two spellings" prose becomes **three shapes**, with the query echo stated in its own right and carrying this document's `spawn.rs` reason; and a new bounded reply SHOULD take `boundedList` or the flat spelling, MAY take the query echo, and then **MUST carry its reason in its own §6 row**. A bespoke shape with no stated reason is a defect; with one it is a decision — readable from the contract rather than from the server. **H33 closes as NOT A DEFECT.**

⚠ **Read this first: the finding's premise does not survive being checked against the contract, and the
correction is the substance of this CR.** H33 says `lookup_equate`'s bounded list *"bypasses the blessed
helper its sibling uses on the identical bound … where `$defs/boundedList` makes them required"*, and
that *"the fragment was transcribed to the hand-rolled shape, so `schema_conformance` is structurally
blind to it"*. Both halves were transcribed into this lane's dispatch and into the hub's message to it.
**Neither holds.** What is real is a narrower and less alarming thing: one adjudicated reply shape does
not use a shared definition that exists beside it, and nothing says whether it should.

## What was measured, and where

All from source at oracle `main` `9963736`, and from `empyrean origin/main` read at a committed
revision, never through the sibling working tree.

| fact | where | value |
|---|---|---|
| what we serve on the prefix branch | `crates/oracle-aether/src/engine.rs:7076-7092` | `query`, `matches[]`, `truncated` |
| what the shared helper emits | `crates/oracle-aether/src/rpc.rs:302` | `items`, `total`, `returned`, `limit`, `truncated` |
| what the contract's shared definition requires | vendored `bus-protocol.schema.json`, `$defs/boundedList` | required `items`, `total`, `returned`, `truncated`; also allows `cursor`, `limit` |
| what the vendored fragment declares for this method | same file, `methods["emulator/lookup_equate"].result` | `name`, `value`, `query`, `matches`, `truncated`, `caveat` — and **no `$ref` to `boundedList`** |
| vendored copy's provenance | `crates/oracle-aether/tests/contract/PROVENANCE.md:63-68` | `pin.revision = 3f83c6c9b9cd619b9087cc54ab982e21305865cc`, `pin.blob = 72ad05de47b124a2be6bc4e667927b4cf1059342` |

## Why the finding's two claims fail

**1. `$defs/boundedList` does not bind this method, and the contract's own prose specifies the shape we
serve.** `protocol.md` §3's method table, line 1697 at `empyrean origin/main`, gives this method's result
as *"`name`,`value` (exact hit …) or `query`,`matches`[],`truncated`? (prefix form; empty is an answer);
`caveat`?"* — the bespoke shape, in the contract, adjudicated as **CR-M / §11.36** (`protocol.md:5121`).
So `boundedList` is a **definition available for reuse, not a rule every bounded reply obeys**, and this
reply is conformant to the contract as written rather than deviating from it.

**2. The fixture is therefore not blind in the way claimed.** The hub asked this lane to state
explicitly that *"your conformance check cannot see the gap because the description you vendored was
copied from the same wrong place"* — a fixture derived from the contract being unable to test the
contract. **That is a real class and this is not an instance of it.** The vendored fragment agrees with
the contract's own prose, not merely with our code, so there is nothing for it to catch. Saying otherwise
would put a second wrong claim into the amendment while correcting the first.

**What IS true, and is all that is left of H33:** two replies in one server express *"a capped list plus
whether it was capped"* in two different vocabularies — `items`/`total`/`returned`/`limit` through the
helper, and `query`/`matches`/`truncated` here — and no document says which a new method should use.

## The consumer, and the argument against changing the shape

`crates/oracle-frontend/src/spawn.rs:1701-1707` states the omission as **deliberate and load-bearing**:

> *"`truncated` is read off the reply's own flag, and there is no `total` to reach for. … So this reply
> deliberately has no `total` key: an implementation that reconstructed truncation as
> `total > matches.len()` would read a missing key as zero, conclude the list was whole, and draw a short
> list that looks complete."*

Adopting the shared shape renames `matches` → `items`, which is **breaking**, and it retires a documented
safety property. That is a much larger act than the finding's *"add three fields"*.

## Options

**A. Documentation only, and it is this lane's recommendation.** Amend §11.36 (or `$defs/boundedList`'s
own description) to say that `boundedList` is a **shared definition, not a blanket rule**, and name which
replies bind it. Cost: one paragraph. Nothing on the wire moves, no consumer migrates, no re-vendor is
forced beyond the ordinary one. It closes the finding by making the next reader unable to file it again,
which is the actual defect: the ambiguity, not the shape.

**B. Migrate `lookup_equate` to `$defs/boundedList`.** Renames `matches` → `items`, adds the counts.
Breaking for the one in-repo consumer above, and it discards that consumer's stated reason. Requires the
amendment, a real re-vendor at a new revision (both schema **and** vectors pins move — `schema_conformance`
step 0 asserts the two revisions are equal), and a consumer change landing with it.

**C. Additive middle.** Keep `query`/`matches`/`truncated`, add `total` and `returned` as **required**.
Non-breaking for anything reading `matches`, and it answers the spawn.rs objection on its own terms: that
objection is about a **missing** key being read as zero, which an always-present key cannot cause. Costs
an amendment plus a re-vendor, and leaves two vocabularies for one concept, which is the thing option A
merely documents.

**Recommended: A.** The shape is adjudicated, has a recorded reason, and breaks nothing. Take C only if
the counts are wanted for their own sake; take B only if one vocabulary is worth a breaking rename.

## What oracle owes once adjudicated

Nothing under A beyond re-vendoring at the next revision in the ordinary way. Under B or C: the serve and
the re-vendored fragment land **in one commit** (a fragment describing a server that does not exist is a
gate arguing against a conformant implementation), both pins move together, and under B the `spawn.rs`
consumer and its test migrate in the same parcel.

## Provenance of this document

Every claim above was read from source or from a committed revision at authoring time and is cited by
file and line. The two claims this CR **retracts** were carried into this lane by relay and by an agent
brief, and were repeated confidently in both; they were checked here only because the amendment would
have hardened them into contract text.
