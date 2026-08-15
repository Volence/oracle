# Ruling — CR-9 (`press` and the stop reason) and CR-11/CR-12 (the watchpoint surface), 2026-08-15

An un-framed adjudication pass over three documents: `docs/2026-08-15-watchpoint-bus-surface.md` (the
CR-11/CR-12 design, with four JSON fragments), CR-9's entry in `docs/2026-08-14-aether-change-requests.md`,
and the contract itself. Ruled together because they turn out to be **one question asked from two ends** —
what `emulator/stopped`'s `reason` is *for* — and because the second document's answer to it is the first
document's answer, arrived at independently.

The precedent is `docs/2026-08-15-fable-ruling-cr13-cr14.md` and
`docs/2026-08-15-fable-ruling-attribution.md`, where the same mechanism adopted change requests and
**changed them on the way in**.

## Verdicts

**CR-9 — adopt neither drafted option.** The CR offered a ninth `reason` enum value (`press`) or one
sentence confirming `runFrames` already covers a press-driven advance. The ruling takes **option 2's
sentence, refuses option 1, and adds the half both options left out**: two additive params, `buttons` and
`port`, on `emulator/stopped`.

**CR-11 / CR-12 — adopt both, as a package, with eight conditions.** The seam (what a watch *is* versus
what a watch *tells you*) is right and "both or neither" is sound. Both rulings the design settled in its
own pass are **upheld**: value-changes-not-write-counts, and the interactive-debugging line. Poll-only for
hits is upheld. `via` in the census enum is adopted — the one core change — on evidence.

---

## What the ruling verified rather than accepted

**★ The design was conformant when it was written and was not by the time it was ruled on, and the gap is
two hours.** Measured from both repositories' history rather than argued:

| | commit | timestamp |
|---|---|---|
| the design | `oracle-next` `330606d` | 2026-08-15 **14:37:56** −0400 |
| §2.4 (shared result conventions) | `empyrean` `f309cc8` | 2026-08-15 **16:49:21** −0400 |

**2 h 11 m 25 s.** (The ruling as delivered says "2h12m"; the measured figure is a minute and a half under
it, and is recorded here at full precision because §11.3 struck two false chronology claims from this same
arc.) Nothing about the design changed. The rules it had to satisfy did — which is why condition 1 is a
*rebase* and not a correction, and why the drafts are left un-rewritten below the amendment marks.

**★ `emulator/stopped`'s `watch` param was prose-only, and this was checked rather than assumed.** The
design has exactly four fenced JSON blocks — `docs/2026-08-15-watchpoint-bus-surface.md:624`, `:693`,
`:769`, `:892`. `emulator/stopped` appears in JSON at `:626` (a `$comment`) and `:650` (a `description`),
both inside `watchpoint_add`'s fragment, and **in none of the four does an `events` entry exist**. That is
**CR-16's exact defect** — a registration that stopped at the prose — proposed on the day CR-16 was adopted
for it, and under §8 item 20's closure the server's own conformant stop event would have been rejected by
the artifact meant to describe it.

**The `old == 0` structural claim is true, and the design's two anchors are each off by one.**
`crates/oracle-core/src/watchpoints.rs:851` builds every bus hit with `old: 0,` unconditionally, in
`on_event`; `:892` uses the captured `w.old` in `on_vdp_write`. The design cites `:850` and `:891`. The
claim survives, the anchors do not — recorded because the two prior rulings in this arc both spot-checked
anchors and found exactly one bad each, and this pass keeps the practice.

Likewise verified: `CensusKey::Fc` **cannot** answer the CPU-vs-DMA question on a VDP watch. Core's own
doc comment (`watchpoints.rs:203`) says *"On a VDP-internal watch this is always 0"*, and `on_vdp_write`
hardwires it at `:896` — the literal `fc: 0`, with the comment *"a VDP-internal write has no bus function
code; CPU-vs-DMA is in `via`"* beside it. So §4.6's `via` census is not a convenience over an existing key;
it is the only instrument that reaches.

**The poll-only volume figures are real, and they are this repo's own measurements.**
`docs/2026-07-25-testrom-conformance.md:787` — `cram_flicker`, *"420,386 over 120 frames … all CPU writes,
zero DMA"*. `:809` — `direct_color_dma`, *"99.997% DMA (4,923,072 of 4,923,206 writes over 120 frames)"*.
Both are group-by computations done by hand, which is simultaneously the argument **for** the census enum
and the argument **against** a per-hit push channel: 4.9 million events in two seconds of wall time is not
an event stream, and routing it through the queue would move `droppedEvents` for reasons having nothing to
do with a client's ability to keep up — degrading the exact signal D17 defines.

**And one thing the executed run found that no reading would have.** The drafted `caveats[]` array is
**accepted** by the published fragment even after the `caveat` collapse, and refused only under §8 item 20's
test-time `unevaluatedProperties: false`. That is not a defect; it is D5 working as designed (the published
artifact is deliberately open, closure binds servers in the harness). It is recorded because it is the
second time in this arc that the difference between "the schema accepts it" and "the harness rejects it"
turned out to be load-bearing.

---

## CR-9 in detail

### Why the enum value is refused, and it is not economy

**The enum's organizing principle is already the stop condition, not the method.** `step` covers *three*
methods — `step`, `step_over`, `step_out` — because those three share one condition. `runTo` and
`runToScanline` are separate values because their *conditions* differ, not because two methods exist.
`press` is a method whose condition is an exhausted frame count, which is `runFrames`. Minting a value for
it would have been the first time this enum named a **caller**.

So §3's `runFrames` is redefined by the condition:

> *"a bounded frame advance ran to completion — `emulator/run_frames`, `emulator/press`, or any future
> method whose stop condition is an exhausted frame count. `reason` names the condition that ended the run,
> never the method that drove it."*

**And option 1 is incomplete on its own terms.** The CR's whole case for the enum value is that a
subscriber who was not the caller cannot otherwise learn an input was injected. True — and a bare
`reason: "press"` does not say *what* was pressed or *on which pad*, which is exactly what a subscriber
reconstructing the experiment needs. A side effect of a run is **param material**. Once `buttons` and
`port` are on the event, the enum value carries **zero** additional bits.

### The house rule this sets

The watchpoint design, written by a different pass on a different capability, proposes the identical
pattern from the other end: `reason: "watchpoint"` (an existing enum member) plus an additive `watch` param
naming the cause. Two independent arrivals at the same shape is enough to write it down, and §11.7 does:

> **`emulator/stopped`'s `reason` is a small closed vocabulary of stop *conditions*, and anything that
> identifies which instance of a condition fired belongs in a param.** A new `reason` value is justified
> only by a genuinely new condition — never by a new method, and never by a new cause.

### The cost, recorded rather than hidden

`buttons`/`port` **cannot** be bound to their trigger mechanically. The event carries no discriminator,
*precisely because* `reason` no longer names the driving method — a press-driven advance and a
`run_frames` one both read `runFrames`. That is a real cost of this ruling. The half that *is* enforceable
is enforced: `dependentRequired: {buttons:[port], port:[buttons]}`, because a subscriber told which buttons
went down but not which pad would attribute the input to the wrong controller in a two-pad session.

`watch` is the contrasting case and shows the rule is not a general retreat: `reason: "watchpoint"` *is* a
discriminator, so that param's presence rule is enforced in both directions by an `if`/`then`/`else`.

---

## CR-11 / CR-12 — the eight conditions

**1. ★ Rebase both CRs onto §2.4.** Three non-conformances, and they are the same mistake in three
costumes — *an honesty mechanism invented locally where the bus had just specified one*:

- `watchpoint_hits` and `watchpoint_list` are **policy-bounded** lists (the ring capacity is a number the
  server chose), so §2.4 clause (a) requires `total` **and** `returned` beside `truncated`. Both drafts
  carried `truncated` alone. They take `checkpoint_list`'s **flat** spelling, not `$defs/boundedList`: in
  both cases the list *is* the whole result, and a nested container inside a result whose entire content is
  that container buys a level of indirection and nothing else.
- The drafted **`caveats[]`** — a REQUIRED array present even when empty — contradicts §2.4's `caveat`:
  singular, string, **optional**, handler-emitted, never parsed. Replaced.
- The optional `limit` echo of §2.4 clause (a) was missing on both list rows. Added.

**2. ★ The prose-only `watch` param.** Verified above. It is in the schema now, together with CR-9's
`buttons`/`port`.

**3. Specify the watch cap.** `capabilities.watchpoints.maxWatches` was advertised with **no behaviour at
the cap**, and core's spec list is an unbounded `Vec`. **D13 rule 3 verbatim**: refuse with
`-32005 {reason:"watchCapReached", cap, count}`, never silently grow, never silently evict. The reason is
sharper here than for checkpoints: a silently-dropped watch produces a `seen`-positive, `matched`-zero
reading that is indistinguishable from a genuine negative finding — which is the one failure this whole
instrument exists to make impossible.

**4. `censusKey` must not be silently ignored.** The draft said *"required when mode is census, ignored
otherwise"*, against §5's refuse-and-name ethos. It is `-32602`, enforced mechanically by an `if`/`then` —
the same device the fragments already use for `old`/`fc` — in both directions.

**5. Housekeeping with teeth.** `watchpoint_list.limit` gets the house 4096 cap its sibling already had (a
`limit` bounded on one list and unbounded on its twin is two policies wearing one name). The package-rule
hedge is struck. Every `protocol.md` anchor is recomputed — see below.

**6. Adopt CR-11 with `via` in the census enum.** Verified above.

**7. Adopt CR-12 poll-only, as drafted** — `hits()` never `take_hits()` (a draining read on a shared bus is
one client stealing another's evidence), `dropped`/`seen`/`matched` in the body, per-hit coordinates inside
`hits[]`.

**8. Handlers land after rows and fragments.** Contract first. That is the next pass, not this one.

### The `caveats[]` → `caveat` collapse, and where the note went

Nothing was lost, and the reason is §2.4 rule 3 doing exactly its job. Core emits four caveats
(`watchpoints.rs:772-810`) and each was placed:

| core's caveat | where it went |
|---|---|
| `seen == 0` | already a typed key — **`seen`**, REQUIRED on every hits read |
| census hit its key cap | already typed — **`keysCapped`** + **`censusOverflow`** + `keyCap`/`distinctKeys` |
| a VDP hit's `mclk` is step-granular | **§6 prose, as a permanent property** |
| an `fc` census over the PSG port cannot attribute a master | unreachable from the wire — `fc` is not an exposed `censusKey` — and covered by the optional `caveat` if a server ever can emit it |

The design had, independently, already given every machine-actionable half a typed key. What was left was
one **permanent property**, and §2.4's own advisory settles where that goes: *a caveat that is always
present is documentation wearing signal's clothes, and clients learn to ignore it — including on the one
reply where it would have mattered.* It is read once by an implementer in §6 now, instead of ignored
forever by a client. `caveat` remains **declared** in every new fragment, because §2.4 clause 4 requires a
fragment to declare it for any method that can emit one or §8 item 20 rejects it.

### The recomputed anchors

The design's `protocol.md` anchors were **exact at its base** (`empyrean` `18a551e`, 1,070 lines) and are
now stale: §11.5 landed after it, and these two amendments after that, leaving the file at **2,028 lines**.
Perishability, not error. Every anchor was relocated by exact-text match against the **post-amendment**
contract, which is what a reader opens:

| design's anchor | subject | now |
|---|---|---|
| `:74-79` | D6, push is the highest-leverage upgrade | **`:84-89`** |
| `:113-115` | D9 category 4's "a number invites the computation" | **`:123-125`** |
| `:154` | D12, a result that only echoes its input | **`:166`** |
| `:254` | D17, loss counted never silent | **`:266`** |
| `:277-278` | `droppedEvents` is a *connection* fact | **`:289-290`** |
| `:370-372` | §2.2, the stamp overwrites same-named keys | **`:424-426`** |
| `:381-382` | `timingBasis` is a property of the machine | **`:435-436`** |
| `:398-400` | a moved counter means re-read your model | **`:452-454`** |
| `:488-491` | §5, never resolve a wrong-state case implicitly | **`:750-753`** |
| `:531-539` | §6's run-control state rule | **`:793-801`** |
| `:573` | the `breakpoint_clear` row | **`:848`** |
| `:645-647` | checkpoint ids are server-assigned | **`:1102-1104`** |
| `:655-657` | "it names a snapshot, not a position" | **`:1112-1114`** |
| `:684-694` | deletion is idempotent | **`:1142-1152`** |
| `:813-821` | §8 item 16's cost | **`:1322-1330`** |

**Five did not merely move.** Three changed *section*, which is the part a mechanical +offset would have
got wrong — §11.5 lifted §6.1's cursor rules **out of §6.1 and into §2.4** whole — and two were rewritten
by these very amendments:

| design's anchor | subject | now |
|---|---|---|
| `:696-698` | "a client must never be handed a partial list" | the **rule** is §2.4 clause (a), **`:504-511`**; `checkpoint_list`'s own paragraph is **`:1154`** |
| `:703-705` | the cursor invariant | **§2.4 clause (c), `:526-528`** |
| `:713-716` | the monotonic-ids non-normative hint | **§2.4 clause (c), `:536-539`** |
| `:411` | §3's `stopped` row | **`:570`** — rewritten by §11.7 (it now names `press` and carries `buttons`/`port`/`watch`) |
| `:568-574` | §6 *breakpoints & watchpoints* | **`:843-852`** — rewritten by §11.8; one watch row became four |

The offsets are not uniform — +10 in §1, +54 in §2, +159 in §3, +262 in §5, +275 in §6, +457 in §6.1, +509
in §8 — which is the general reason a line anchor is a citation and not an address, and why the two
rewritten rows have no offset at all. *(These are against `empyrean` `af434a2`; they will move again with
the next amendment, which is the point.)*

### Registered, not built — each with its reversal condition

| | why not now | what reverses it |
|---|---|---|
| **`sinceSeq` / read-from-now on `watchpoint_hits`** | There is no bus-side "read from now" at all today. A client wanting only post-arm hits pages to the tail, bounded by the advertised `ringCap` — which is exactly why `ringCap` is advertised. | A client measured paying that cost on a hot range. |
| **A bus-side last-value table** | It is what exact change counts on the **68000 bus** would need, and it is real new state on the instrumented path. `old` is structurally `0` there and no consumer has asked. | A consumer that needs exact change counts off the VDP spaces. |
| **The per-frame sampler** | A different instrument. A sampler reads one address once per frame whether or not anything wrote it; a watch is silent about an address nobody touched and floods on one rewritten 60×/frame with the same value. Shipping the recorder and *calling* it the value trace would be the ranking error the handoff itself measured. | Its own evidence, as its own capability. |
| **`CensusKey::Pc`** | A genuine finding out of scope: core's justification for its 256-key default cites *"390–516 distinct PCs"* — the cap's headline example is a census no `CensusKey` variant can perform. No bus consumer. | A bus consumer. Adding it now would be ranking by how good the argument sounds. |

---

## Where the ruling disagreed with the designs — and with itself, twice

**CR-9.** Both drafted options. Option 1 would have made this enum name a caller for the first time and
would still not have carried what its own argument says a subscriber needs; option 2 alone closes the
conformance question and leaves the stream unable to say an input happened.

**CR-11/CR-12.** The `caveats[]` shape; the missing `total`/`returned`/`limit`; the prose-only `watch`; the
unspecified cap; the silently-ignored `censusKey`; the uncapped `list.limit`; the package hedge. **Not**
the seam, the two settled rulings, poll-only, the handle-not-address argument, the `hits()`-not-`take_hits`
rule, the nested per-hit coordinate, the `old`/`fc` presence rules, or the refusal list in §5 — all of
which survived checking.

**And two places the ruling as delivered was itself wrong, both minor and both recorded rather than
quietly fixed:**

1. **The package hedge is in the design, not the register.** The ruling's condition 5 says *"strike the
   register's 'if exactly one is adopted, CR-12 is the one that…' hedge."* The register
   (`docs/2026-08-14-aether-change-requests.md:430-465`) says only *"Adopt both or neither"* and carries no
   hedge. The sentence lives at `docs/2026-08-15-watchpoint-bus-surface.md:935`, in §8.1, two sentences
   after that same document states the package rule. It was struck where it actually is.
2. **"~240 lines stale" is right for §6 and wrong as a single number.** The offset ranges from +10 to +361
   depending on section, and three anchors moved *sections* rather than lines. The table above replaces the
   estimate.

## What lands where

**`empyrean`, two commits** — CR-9's ruling changes §3 for *every* stop-shaped method and earns its own
line in the permanent record rather than riding a watchpoint amendment:

- `8adf219` — **§11.7**: the §3 redefinition, `buttons`/`port` on `stopped`, §6's `press` row, §8 item 13
  widened. Prose and schema in one pass (D14).
- `af434a2` — **§11.8**: §6's four watch rows and their normative prose, `capabilities.watchpoints`, the
  four rebased fragments, `$defs/watchStamp`, the `watch` param on `stopped`, the cap refusal, the
  `censusKey` `if`/`then`, §5's `-32005` reason list, §2.1's capability example, and a new §8 item 21.

*One addition beyond the ruling's eight conditions, flagged as such:* **§8 item 21**. The ruling did not
ask for a checklist entry, and a new conformance item is exactly the kind of thing that should not be
slipped in — but the cap refusal and the `censusKey` refusal are **MUSTs that bind servers**, and §8 is the
list of those. Leaving them off would repeat the pattern the contract itself flagged for item 19: *"a rule
whose whole subject is capabilities that escape the conformance checklist is the last one that should be
missing from it."* It adds no obligation the §6 prose does not already carry.

**`oracle-next`:** this document, the amendment marks on the design, and the register closures. **No server
code, and no handler** — condition 8.

## Executed, not merely written

Every fragment was spliced into the **real** `contract/schema/bus-protocol.schema.json` and driven through
**both** validators — `jsonschema` 4.26 (Python) and `jsonschema` 0.49 (Rust, the crate
`crates/oracle-aether/tests/common/schema.rs` compiles fragments with, and compiled the same way that
harness compiles them). The final run is against the **committed** contract, not a scratch copy:

```
[py] the amended contract schema: valid draft 2020-12 metaschema   OK
[py] D3 request pattern accepts all 4 new method names             OK
[py] methods in the amended schema: 26
[py] all 11 landed subschemas compile (open and closed)            OK
[py] cases: 72/72 met (24 accept / 48 refuse)                      OK
[rs] spliced document: valid draft 2020-12 metaschema              OK
[rs] all 11 spliced subschemas compile (open and closed)           OK
[rs] cases: 72/72 met (24 accept / 48 refuse)                      OK
```

The refusals include a **bus hit carrying `old`**, a **VDP hit missing it**, a bus hit missing `fc`, a VDP
hit carrying `fc`, a **numeric `watch` handle at every one of the five places it appears** (§8 item 16's
mistake), a **`censusKey` without `mode:"census"`** and `mode:"census"` with no key, a list result missing
`total` or `returned`, `reason:"press"` (CR-9 option 1 as drafted), `buttons` without `port`, a
`watchpoint` stop that does not name its watch, a `watch` handle on a non-watchpoint stop, `limit: 4097` on
**both** list methods, a `+$disp`-suffixed symbol, and an eleventh undeclared key at the top level and
inside a hit.
