# CR-M — serve the listing's equates, in a namespace of their own

**Raised by:** oracle lane, 2026-09-05.
**Target:** `contract/protocol.md` §11.36 (next free).
**Reviewer:** to be named by the adjudicator in the ruling itself, per the 2026-08-27 substituted-reviewer rule.

## What is unreachable

Every listing this suite loads carries an **Equate Table** — build-time constants, published as
`EQU <NAME> = $<hex>`. `oracle-core`'s symbol reader **recognises the section and deliberately does not
ingest it**, consuming header, rows and trailer so they do not count as parse damage. Nothing we serve can
answer *"what is the value of `RING_BUFFER_ENTRY_SIZE`?"*

Measured on `aeon/s4.debug.lst` (and reproduced on `s4.lst`):

| | |
|---|---|
| equates | **742** — matching the listing's own `742 equates` trailer, so the count is self-verifying |
| labels in the Symbol Table | **3,014** — matching the row count our engine reports for the same file |
| equate↔label **name collisions** | **0** |
| equate values falling inside the cart window `$000000-$3FFFFF` | **631** |

## The honest state of the demand

**This row was promoted on having two named consumers. It has one.** Saying so, because a queue row's
justification ages and nothing re-reads it:

* **LIVE — the Objects panel's ring ceiling.** `crates/oracle-player/src/objects.rs:299-311` measures the
  ring buffer's span in bytes and cannot turn it into a count of rings, because the divisor is
  `RING_BUFFER_ENTRY_SIZE`, which the listing publishes **only** as an equate. The panel says so in
  as many words rather than guessing, and the reason is pinned by a test that is a deliberate tripwire:
  `the_rings_ceiling_is_unknown_because_equate_values_are_not_ingested` — *"the day equates become
  readable this goes red and asks for the division to be finished."* Verified present in the listing:
  `EQU RING_BUFFER_ENTRY_SIZE = $00000006`.
* **MOOT — the out-of-act placement check.** This was the second consumer when the row was promoted. It
  was closed on 2026-09-04 by CR-L (§11.35) using `Level_Width`/`Level_Height` as **RAM words**, not
  equates. It is no longer an argument for this CR and should not be counted as one.

**Scope limit, stated so this CR is not read as more than it is.** Serving equates makes reachable exactly
what the listing *publishes*. Values the build supplies on the command line do not appear in the listing at
all — that is sigil's `DEFINES-REACH-THE-LISTING`, still **open** at their board (23:14Z), and it is a
different fix. A reader should not conclude from this CR that "build-time constants are now reachable"; the
true sentence is "the constants the listing carries are now reachable."

## The design question, and why the measurement answers it

`symbols.rs` records the open question in its own header: *"whether equates become addressable at all, and
how a same-named equate/label collision is resolved."*

⚑ **631 of 742 equate values fall inside the cart address window.** So an implementation that folds equates
into the existing symbol table poisons `addr→name` for 631 values: an equate whose *value* is `$1800`
becomes indistinguishable from a label *at* `$1800`. Every reverse-lookup path — disassembly annotation,
`lookup_symbol`'s addr branch, the panel's own resolution — would have to filter equates out, and **one
missed filter is a confidently wrong name on a disassembly line**, which is the silent-wrong-answer class
this bus exists to refuse.

The collision question has an empirically empty answer *today* (**0 collisions**), and that is a reason to
**state the policy anyway rather than to skip it**: a rule with an unasserted precondition is this
workspace's recurring defect, and the next listing — or sigil's own dialect — may collide.

## Options

**A — a separate door: `emulator/lookup_equate {name | prefix}` → `{name, value}` (recommended).**
Equates never enter the address table and never reach `addr→name` by construction, so the poisoning above is
impossible rather than filtered. Error semantics stay clean and §4-consistent: no table loaded is `-32012`,
a name absent from the Equate Table is `-32013`, and neither can be confused with a label miss. Mirrors
`lookup_symbol`'s bounded prefix search, which is the ergonomic path a client already knows.
Cost: one new method on a bus that is trying not to grow.

**B — extend `lookup_symbol` with a `kind` discriminator.** One door, no new method. Cost: the result
becomes polymorphic — an address for a label, a value for an equate — and **a client that ignores `kind`
receives a value where it expected an address**, with nothing on the wire to stop it. That is a silent
wrong answer in the one direction this bus refuses to serve.

**C — ingest equates into the symbol table with a `kind` field.** Cheapest to implement, worst failure
mode: correctness now depends on every reverse-lookup path remembering to filter, forever, including paths
not yet written. The 631 measurement is the argument against it.

**Recommendation: A.** Not on tidiness — on the 631. A and C differ in whether the dangerous case is
*impossible* or merely *currently handled*, and this lane's standing bar is that a rule whose precondition
nothing asserts is the defect waiting to happen.

**Policy to adopt with it, whichever option wins:** an equate and a label sharing a name are **two
different things and both are answerable**; the equate door answers the equate, the symbol door answers the
label, and neither silently shadows the other. Zero collide today, which is exactly when the rule is cheap
to state.

## What would have to be true for this to be wrong

* If a consumer needs `addr→name` to *include* equates — e.g. annotating an immediate operand with a
  constant's name. That is a real want, and it is **not** what option A serves. If the adjudicator judges it
  the primary use, the design changes shape and A is the wrong recommendation. I have no consumer asking
  for it today; the one I have asks `name→value`.
* If the ring ceiling is judged too thin to justify a method. It is one panel line. I think the capability
  is worth more than its first consumer — 742 constants currently unreachable, with the engine team's own
  build values among them — but that is a judgement and the adjudicator may weigh it differently. If it is
  refused, the honest outcome is that the tripwire test stays red-in-waiting and the panel keeps saying
  "ceiling unknown", which is not a defect.
* If sigil's `DEFINES-REACH-THE-LISTING` would change the *shape* of what arrives (a second class of
  constant with different provenance), this may want sequencing behind it rather than beside it. Their row
  is open and unscheduled; I do not think it blocks, because it adds rows to a section we would already be
  reading, but it is their call to say otherwise.
