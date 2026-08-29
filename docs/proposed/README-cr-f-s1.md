# CR-F / §11.26 — S1 fragments and vectors, proposed by the oracle lane

⚑ **SUPERSEDED 2026-08-29 — APPLIED at empyrean `b5b8184` (verified an ancestor of their `origin/main`).
The applied text is authoritative; these files are the submission, kept for the record and NOT kept in
sync.** Vendoring anchors, verified against the remote-tracked tree rather than taken from the message:
`contract/schema/bus-protocol.schema.json` blob `3b638be34cefdd4ecc3d83739b940576511a61fc`,
`contract/schema/tests/vectors.json` blob `083bbfd6eb5bff8620a935f21549dc4793022798`. Gate green there:
63 fragments, 83 pass, 118 red, 44 closure.

## ⚠ TWO DEFECTS IN THIS SUBMISSION, BOTH THIS LANE'S, BOTH CAUGHT BY THE APPLYING LANE

**1. Nine of my eleven vectors could not have passed the gate.** Every result case wrote
`"layout": {}`. `$defs.decoderLayout` **requires** `engine`, `detectedBy`, `slotBytes`, `slotCount` and
`baseAddr` — so each of those cases would have failed validation immediately, on a field I never
checked. The applying lane filled the literals in.
**The uncomfortable part is the shape of the error, not its size.** The README below argues at length
that an unrecorded residue "reads as guarded to everyone who sees a green schema run" — while
submitting vectors that **could not have produced a green run at all.** I verified programmatically
that every case cited a clause; I never once ran them against the schema I was writing them for, which
was readable at a committed revision the whole time and which I read other parts of. **A completeness
claim I could have checked in one command and did not** — this repo's own bar 17, committed by the
lane quoting it.

**2. I changed the contract's substance between filing and authoring, and did not flag it as a delta.**
CR-F §2.1 as filed said `owner.raw` is served *"always, so a caller can audit us"*. The fragment I then
wrote makes it **absent when `kind == "unavailable"`** — which is the better rule, and is what was
adopted, but it is a change to a filed artifact that I introduced silently in a second one. The
applying lane caught it and asked that the serve note it. **The right form was a named delta at
submission time**; an improvement introduced without a flag is indistinguishable from an inconsistency.

**Both land on the serve as obligations rather than notes:** the served implementation follows the
APPLIED schema (`raw` absent when `unavailable`), and no vector this lane writes again goes out without
being run against the schema it targets.

---

**Date:** 2026-08-29 · **Status:** proposed, **not applied, not pushed to empyrean.**

## Why these live here and not on a branch of `contract/schema`

The hub offered either option. This lane's push grant from the owner carries the condition **"never
push another lane's repo"**, and creating a branch in empyrean's tree is writing in it. So this lane
authors the content — which is the half needing the measurements — and **the contract lane applies it.**
No permission of anyone's is being routed around; the split is the grant's own.

Files: `2026-08-29-cr-f-s1-fragments.json` (the `object_at` result shape and the `clicked` event params),
`2026-08-29-cr-f-s1-vectors.json` (11 cases: 6 pass, 5 red-first).

## What is guarded, and it is enforced rather than described

Every conditional §11.26 states as an *iff* is an `if`/`then`/`else` in the fragment, not a `$comment`:

* `owner.slot` present **iff** `kind == "object"` (M2);
* `owner.raw` **absent iff** `kind == "unavailable"` (M2/M5) — with no table there was no word, so a
  `raw` there is a fabricated reading of memory nobody consulted;
* `world` present **iff** `worldSource == "camera"` (M3);
* `winner.spriteIndex` present **iff** the winner is a sprite (M1, since it mirrors
  `pixel_attribution`'s winner exactly).

Each red-first case perturbs its neighbouring pass case in **exactly one** way, and says which clause
forbids it. The three the ruling names by name are all present: **ring click**, **unresolvable owner
table**, **unresolvable camera**.

## ⚠ ONE OF THE THREE IS ONLY HALF-EXPRESSIBLE AS A VECTOR, AND SAYING SO IS THE POINT

**M2's central rule — *"a server MUST answer `unavailable`, never `none`, when the symbol does not
resolve"* — is a claim about SERVER BEHAVIOUR, and a schema cannot see it.** A schema validates a
document. Both of these are perfectly valid documents:

```jsonc
{ "owner": { "kind": "unavailable" } }                 // correct on a release ROM
{ "owner": { "kind": "none", "raw": "0x0000" } }        // WRONG on a release ROM — and schema-valid
```

So a server that merges the two — the exact defect M2 exists to prevent, and the one that makes a
picker report an empty screen as a true answer — produces replies **no vector in this set can fail.**
The vectors above guard the *shape* of `unavailable` (no `slot`, no `raw`); they cannot guard that it is
*chosen*.

**This is flagged rather than papered over because S1 is a gate.** If these land and the residue goes
unrecorded, the merge prohibition reads as guarded by everyone who sees a green schema run, which is
this workspace's recurring failure — a check that reports nothing read as a check that found nothing.

**Where the other half belongs, and it is ours:** a conformance row that loads a **release** ROM (no
`Sprite_Owner` on its listing — verified: 0 occurrences in `s4.lst`, `FFFFE1EE` in `s4.debug.lst`),
clicks a sprite, and asserts `owner.kind == "unavailable"`. That is a server test in this repo, not a
contract vector, and **this lane owns it.** It is registered here so it lands with the serve rather than
after it.

The camera half has the same shape and the same answer: no vector can prove a server resolved
`Camera_X` rather than a cached address. §11.26 M3's re-resolve rule is behaviour, and its test is a
`romReloaded` conformance row — also ours.

## Note on provenance, because the vectors file has a standing rule about it

`vectors.json` requires that every case be **derived from a spec row**, never taken from a server's
replies — the contract leads, the emulator implements. Every `why` here cites the §11.26 clause under
test. The measured literals (dot `160,112`; owner word `0x8ED6`; camera `96,144`) appear only as
*plausible values in a shape*, never as the thing asserted; substituting different integers would change
no case's verdict. Stated explicitly because this lane holds a measurement and the temptation to let it
define the contract is exactly what that rule guards against.
