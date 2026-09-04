# `pixel_attribution.rgb` — aeon's finding is real, the fix is FORBIDDEN, and the defect is discoverability

**Date:** 2026-08-30 · **By:** oracle overseer, foreground · **Outcome:** no server change; an anti-fix pin,
a doc note, and a CR to draft

> **Updated 2026-09-03 — §6 below closes this out.** The CR §3 asks for was raised as **CR-G**, adjudicated
> ADOPT WITH CHANGES, and is now `protocol.md` **§11.27**. The server half shipped on
> `parcel/attr-rgb-caveat`. §§0–5 are left exactly as written on 2026-08-30: they are the record of a
> near-miss, and the one useful thing about a near-miss is what it looked like from inside. §5's "Owed"
> list is discharged item by item in §6 — **including the one item there that turned out to be wrong.**

## 0. The short version

aeon reported that `emulator/pixel_attribution` returns the right `cramIndex` and the **wrong `rgb`** on a
ROM that repaints CRAM mid-frame. **It reproduces exactly.** I built the obvious fix, verified it against
the reproduction, and then found the contract forbids it **in terms**. The fix is reverted. What survives
is a pin that stops the next person doing what I did, a doc note aeon asked for, and a CR for the half that
is genuinely broken.

**The behaviour is correct-by-contract. The defect is that nothing tells a caller so.**

## 1. Reproduced, by a route neither of us shared

aeon: a 71,680-pixel sweep on `s4.debug.bin` (their raster band demo, frame 186), cross-referenced against
`emulator/screenshot`'s PNG. 702 in-band pixels reported the base colour while the framebuffer held the
three authored band colours.

Here, before touching anything: `vendor/TestRoms/color_1536.bin`, 90 frames, paused, comparing
`pixel_attribution` against **`emulator/scanlines`** rather than a PNG.

```
checked 55 rows at x=100 -> MISMATCHES: 55
  y=  4 attribution=(0, 72, 0)  raster=(0, 54, 0)  cramIndex=0
  …
```

Same `cramIndex`, different colour, every row. **Different ROM, different instrument, different lane** — so
this is corroboration rather than echo (bar 19: the enumeration parameters genuinely differ).

I also confirmed the mechanism from our source independently of both measurements — `render.rs:1897`,
`rgb: self.cram_rgb_state(winner.cram_index, winner.state)`, where `cram_index` comes from the resolved
line and the colour comes from **live** CRAM.

## 2. The fix I built, and why it is gone

`Engine::latched_raster_pixel` — take the pixel from `last_frame` (the raster the machine actually drew,
the same source `screenshot` and `scanlines` serve) when no layer mask is set. Verified against the
reproduction: **220 dots across 55 rows × 4 columns, 0 mismatches**, up from 55-of-55 wrong.

It is reverted, because `contract/protocol.md` §11.3 says, of this method:

> *"A server answers by resolving the scanline from live VDP state — VRAM, CRAM, the registers, the sprite
> table — and **MUST NOT read a framebuffer**."*

and, about this exact disagreement:

> *"a whole-frame-state read disagrees with the picture **whether the machine is running or not** … **This
> is not a defect in either method and a server MUST NOT try to paper over it**; a client that needs the two
> to agree needs a per-scanline capability — `emulator/scanlines`."*

and names it as already booked: *"closing that divergence is a registered follow-up
(**F-SCANLINE-INDEX**), not a defect in either method."*

**F-SCANLINE-INDEX is in this repo's own follow-up register**, and this seat read it at boot the same day.

⚠ **How close this came to shipping, recorded because that is the useful part.** The fix compiled, the
reproduction went green, the full suite passed, and I had already told aeon my "current lean" was to do
exactly this. Nothing in the code, the tests, or the reproduction would have stopped it. **The only thing
that stopped it was reading the method's own contract text before merging** — and I only did that because I
went looking for whether `caveat` was available on the fragment, i.e. for an unrelated reason. That is
bar 8's cheap frame-changer arriving by luck rather than by discipline, which per bar 21 makes it a
coincidence with a good track record, not a practice.

**And the near-miss is bar 9 exactly, from the inside:** the instrument (attribution) could not reach the
subject (what was drawn), and I reconfigured the subject until it could. It produced better-looking data —
220 agreeing dots — and *nothing about it announced itself*. It is the textbook shape and I was in it.

## 3. What is actually wrong, and it is aeon's own framing

The contract's answer to "I need the colour that was drawn" is **`emulator/scanlines`** — which is,
independently, exactly the workaround aeon arrived at (`cramIndex` from attribution, colour from the
raster). So the mechanism works. What failed is that **nothing in the reply says any of this.**

aeon's cost was not the wrong value. In their words: *"my run had `cramIndex` right and `rgb` wrong in the
same object, and there was no way to tell from the object alone. A caller with one instrument cannot detect
disagreement between two fields that never disagree out loud."*

**That is the defect, it is real, and it is contract surface rather than server behaviour**: this fragment
declares no `caveat` (19 of 64 do), so a server cannot say it. §8's invention ban means we do not add one
unilaterally.

**CR to draft — `pixel_attribution` gains a `caveat`**, emitted when the divergence is *possible* (a
completed frame exists to disagree with), naming `emulator/scanlines` as the reconciliation path. Two
properties worth writing as properties:

1. **It must not be a heuristic that claims to detect mid-frame CRAM writes.** A flag that fires only when
   we think a raster program ran will be wrong in both directions; the honest statement is about which
   moment this row answers for, which is always true.
2. **It must name the path, not just the hazard.** aeon had the workaround already; a caller who does not
   needs to be sent somewhere, or the caveat costs an hour and saves none.

## 4. What landed

* **The anti-fix pin** — `rgb_resolves_against_live_state_and_the_row_must_not_read_a_framebuffer`
  (`tests/pixel_attribution.rs`). Asserts `rgb` **follows live CRAM** after a repaint, and says in its own
  body why a green-by-latched-raster is the defect. **Recorded mutation:** apply the framebuffer read →
  the row fails naming §11.3. It exists because this seat tried the fix and got a green suite.
* **The truncation note aeon asked for** (`render.rs::intensity`). `step * 255 / 14` truncates, so `$0224`
  is `(72, 36, 36)` and not the `(73, 36, 36)` a rounding formula gives. Comparing our output against a
  `round()`-based reference scores **correct** pixels as mismatches by one unit — small enough to read as
  noise, large enough to fail an equality check. No behaviour change; aeon explicitly did not ask for one.

## 5. Owed

* Tell aeon: their report is right, their workaround is the contract's own prescribed path, and the fix
  they ranked first is forbidden — so the thing they ranked *second* (name the moment) is what gets built.
* Draft the caveat CR.
* **Do not re-attempt the framebuffer fix.** If a future session believes attribution should agree with the
  picture, the contract change is the work, not the server change — and F-SCANLINE-INDEX is where it lives.

## 6. What landed, 2026-09-03 — CR-G is §11.27 and the server now discloses

**The ruling changed the CR's emission rule, and the change was the right one.** §3 above proposed emitting
the caveat *"when the divergence is possible (a completed frame exists to disagree with)"*. The hub refused
that half and said why: **both engines in this suite rebuild CRAM every vblank**, so "a completed frame
exists" is true on every reply after the first frame. That is not a conditional caveat, it is
`emulator/read_memory`'s constant debug string wearing a new label — the exact pathology §2.4's advisory
names, in a document this lane had read. The ratified rule is a **measurement**:

> a server emits the caveat when the CRAM entry at `cramIndex` has been written since line `y` of the last
> completed frame was drawn, or when no frame has completed; it is absent otherwise.

**So the item in §5's "Owed" list that said "draft the caveat CR" is discharged, and the shape §3 drafted
was corrected on its way through.** Worth recording plainly: this lane got the hazard right and the
*trigger* wrong, in the direction that would have produced a field nobody reads.

### The measurement we could make, and the one we did not have to settle for

§11.27 permits a fallback — *"a server that cannot yet stamp per-entry writes MAY emit on any CRAM write
since the line drew (coarser, still conditional) and MUST NOT emit unconditionally."* The first question was
therefore about this engine and not about taste: **can Oracle stamp per-entry CRAM writes?**

**It can, and cheaply.** There are exactly **two** places in the whole tree that store a CRAM byte —
`Vdp::write_target`'s `Target::Cram` arm (every guest path funnels through it: data port timed and untimed,
DMA 68k→CRAM, DMA fill, fill-trigger) and `Vdp::poke_cram` (the `emulator/write_cram` debug poke). Both
already know the entry index. So per-entry costs one `[Option<u64>; 64]` and one store more than the coarse
rule would, and the coarse rule would disclose on entries nobody touched. **We implement the precise form.**

The stamp is currency-neutral by construction: `state_hash` and `export_state` read only VRAM/CRAM/VSRAM/regs,
so neither can see it. It is written unconditionally on the store path (never gated on a capture being armed),
so an instrumented machine stays byte-identical to a plain one. The save-state container's layout fingerprint
is derived from a power-on snapshot, so it moves on its own and stale states are refused cleanly.

`poke_cram` takes its instant as a **parameter** rather than reading `Vdp::now_mclk`. `now_mclk` is when the
VDP last did *guest-driven* work; a debug poke is not guest-driven, and on a machine paused after a quiet
stretch that value can be arbitrarily stale — it would date the poke *before* the line it must be reported as
landing after. The caller knows the machine's real now and passes it. Same reasoning as the pre-existing rule
that a poke must not fabricate a capture clock.

### The sentinel that was wrong, caught by asking the vacuity question

The first draft stored the stamp as a bare `u64` with `0` for "never written", on the argument that 0 could
only satisfy the rule's comparison when the line also drew at mclk 0. **It can: line 0 of frame 0**, which any
caller can ask for as soon as one frame has completed. Every untouched palette entry disclosed at that
coordinate — the unconditional shape §11.27 forbids, arrived at by arithmetic rather than by intent, and
green on every other row in the file. `Option<u64>` now distinguishes the two, and
`a_never_written_entry_is_silent_for_every_line_of_every_frame` pins both sides (`None` silent at that cell,
`Some(0)` disclosing, or the arm is untested).

This is the second time on this arc that the wrong answer looked exactly like the right one. The first was
the framebuffer fix, which passed every gate the server owned. This one passed thirteen of fourteen rows.

### What is pinned, and where

**Four wire vectors** (`docs/proposed/2026-08-30-cr-g-vectors.json`, merged upstream into
`contract/schema/tests/vectors.json` as cases 241–244). Re-run against the vendored fragment on 2026-09-03,
closed with `unevaluatedProperties: false` per §8 item 20: **4/4 agree**, with controls confirming the
validator refuses a non-string `caveat` and an undeclared key rather than accepting everything.

⚑ **Two of the four vectors §11.27's text names are not in that file, and the substitution is correct.** The
clause names *"a caveat that names no method (red)"* and *"a pre-first-frame reply carrying it (valid)"*.
Neither is expressible as a **document**: `caveat` is `type: string`, so a string naming nothing is valid
JSON against the fragment, and a pre-first-frame reply is byte-identical in shape to case 241. The merged
file substitutes two red vectors a schema genuinely can judge (a structured `caveat`; a reply that discloses
*instead of* answering, i.e. no `rgb`) and moves the two live properties to conformance rows — which is
§11.27's own instruction for the "required when applicable" half. **Those two properties are now asserted on
the wire**, so nothing was dropped; it moved to the instrument that can see it.

**Five conformance rows** (`crates/oracle-aether/tests/pixel_attribution.rs`), built as **pairs over one
fixture at one dot** so that each differs from its partner in the write stamp and in nothing else:

| row | what it measures |
|---|---|
| `the_caveat_is_absent_when_nothing_wrote_the_entry_since_its_line_drew` | the absence half, and the anti-vacuity control for the rest |
| `a_cram_write_after_the_line_drew_makes_the_reply_say_so_and_name_the_path` | disclosure, `emulator/scanlines` named, `rgb` still present |
| `the_caveat_is_per_entry_so_repainting_an_unrelated_colour_stays_silent` | **which rule is in force** — red under the coarse fallback |
| `a_reply_before_the_first_completed_frame_discloses_and_still_answers` | §11.27's second trigger, and §11.3's power-on sentence |
| `rgb_resolves_against_live_state_and_the_row_must_not_read_a_framebuffer` | the anti-fix pin, now also asserting the divergence is audible |

Plus five unit rows on the rule as arithmetic (then in `engine.rs`, now in `oracle_core::render` — see
§7), where the `>=` boundary and the vblank-write case can be stated exactly rather than posed
approximately.

### The vacuity that had to be designed out

A caveat test that never arranges a qualifying write proves only that the key is **absent** — which is also
what a server that never emits produces, i.e. the state this repo was in before today. Every row above has a
partner that differs in the write stamp, and the per-entry row is the one that distinguishes the implemented
rule from the coarse fallback §11.27 also permits: under the fallback it is red at its first assertion.

### What did NOT change, and must not

`rgb` is still the **live** colour. §11.3's pin stands, the anti-fix pin stands, and **F-SCANLINE-INDEX is
untouched** — closing the divergence is still that follow-up. §11.27 makes the divergence *audible*, which is
what aeon actually asked for: *"a caller with one instrument cannot detect disagreement between two fields
that never disagree out loud."* They now disagree out loud, on the reply, naming where to go.

### Still owed

* Tell aeon the caveat has shipped and what triggers it — §5's first bullet, now with something concrete to
  point at.
* **Runtime confirmation on a real ROM is NOT done here.** These are wire and unit rows against posed
  fixtures; nobody has driven `color_1536.bin` — the ROM that reproduced the original finding, 55 of 55 rows
  — through the caveat. That is a foreground want, not a background one.
* ~~**The player's click panel does not carry this clause yet.**~~ **Landed 2026-09-04 as
  `F-ATTR-CAVEAT-PANEL` — see §7 below.**

---

## 7. F-ATTR-CAVEAT-PANEL, 2026-09-04 — the window carries it too, from the wire's own function

§6 left the third surface open: the bus disclosed and the window did not, so the same question had two
answers and only one of them was honest. The owner's standing ruling on the debug window
(`docs/OVERSEER.md`, "WHAT GETS A TAB IN THE DEBUG WINDOW") is that **a panel must show the same answer a
tool gets**.

### Parity is not the panel calling the bus

D15 (`contract/protocol.md:238`) is explicit that an in-process GUI *"reads the method registry directly,
in-process; it does not open a socket to itself"*, and our `Host::pump` would make it worse — a click would
enqueue a command and wait a frame to answer what it can answer synchronously. Parity is **one
implementation under two consumers, plus an assertion that says so**, which is exactly the move
`sprite_tile_at` made one field over (contract §8 item 19).

### Where the rule lives now, and why it is `oracle-core`

`cram_divergence_caveat` moved from a private free function in `oracle-aether::engine` to a `pub fn` in
`oracle_core::render`, beside `sprite_tile_at` and beside `PixelAttribution`, the struct it annotates.

`oracle-core` rather than `oracle-aether` is a **correctness** call and not packaging: `oracle-aether` is an
**optional** dependency of `oracle-frontend` (its `aether` feature), because the player is deliberately
buildable with no control surface at all. A caveat sourced from there would silently vanish from the window
in a `--no-default-features` build — a hole that opens on a feature flag, which is worse than one that opens
always, because nothing red ever names it. Measured, not assumed: `cargo tree -p oracle-frontend
--no-default-features -e normal` contains no `oracle-aether`, and that build compiles with the clause intact.

It stays a **free function** rather than a `Vdp` method for the reason `Vdp::cram_written_mclk`'s own doc
gives — the VDP stays a machine and does not acquire an opinion about what a caller should be warned of.
That reasoning survives the move untouched; what changed is only that the opinion now has two readers, so it
can no longer live inside either of them. **That comment was itself the stale ruling this parcel had to
resolve rather than route around**: it said the comparison lived in `oracle-aether` "with the reply that
discloses it", which was true with one consumer and false with two (and it named a function,
`cram_divergence`, that never existed under that spelling). A stale rule inside a comment outlives every doc
that recorded it, because nobody re-reads a comment to check whether the rule it cites still holds.

### One wording change, and it is the price of sharing the sentence

The string is shown to a person **verbatim on both surfaces** (§2.4 rule 2), so it may not name one
consumer's result key. `` `rgb` is the LIVE colour by contract `` was a dangling reference in a window that
reports no `rgb` — the "purple boxes" failure with correct arithmetic under it. Both arms now say *"the
colour reported here"*, with *"(`rgb` on the wire; protocol.md §11.3)"* keeping the bus client's signpost.
§11.27's two pinned properties are untouched: it still names `emulator/scanlines`, and it is still prose.

### The panel

`pick::resolve` takes `now_mclk` and its callers pass `sys.scheduler().now()` — the same instant the handler
stamps its own verdict with, and deliberately **not** `Vdp::now_mclk`, which is the instant the VDP last did
*guest-driven* work and on a paused machine is arbitrarily stale: it would date a write **before** the line
it must be reported as landing after, i.e. silence exactly where the disclosure is owed. That is the argument
`Vdp::poke_cram`'s doc already makes about its own `at_mclk`, one function over.

The clause joins the **existing** per-answer mechanism rather than inventing a second one: `describe` now
assembles a *list* of clauses (mask first, then colour) onto the human-facing headline, because a dot can
earn both and dropping either would be a silent choice made on the reader's behalf. The verdict is computed
**once, before the `match`**, so no winner arm can be the one that forgets it — every winner resolves a
`cram_index`, so every winner's colour can go stale.

### Rows added, and the red-first evidence

Three rows in `crates/oracle-frontend/src/pick.rs`'s `bus_parity` module (runner: `cargo test -p
oracle-frontend --bins`), plus the five unit rows moved into `oracle_core::render`'s tests so the rule's
witness lives in the crate **both** consumers link rather than in one of the two.

| mutation, applied on disk | what went red |
|---|---|
| **A** — `cram_divergence_caveat` stubbed to `return None` (the *shared* derivation) | `the_panel_and_the_bus_carry_the_same_colour_caveat` at its **COULD NOT MEASURE** anchor; also `the_panel_stays_silent…`, `the_panel_answers_at_the_clock…`, 2 rows in `oracle-core`, and 4 wire rows in `oracle-aether` |
| **B** — panel drops the clause (`describe` ignores `colour`) | *"the bus discloses and the window does not — the two have DRIFTED"* and *"the clause is not wired in at all"* |
| **C** — panel emits the clause unconditionally | *"the window must be silent too — found `palette entry was written` in: …"* |
| **D** — panel reads `vdp.now_mclk()` instead of the caller's clock | *"the panel must be answering with the clock its caller passed"* (it took the pre-first-frame arm) and the DRIFTED assertion |

**Mutation A is the one that matters.** A parity test between a panel and a handler that share a derivation
is *structurally blind* to a defect in the derivation they share: break it and both sides move together,
agreeing perfectly and both wrong. So the first assertion in the parity row is not about agreement at all —
it is that the **wire** discloses, i.e. that the fixture actually poses divergence. Under the stub, that
assertion is what fails; without it, the stub would be green and the row would witness two silences shaking
hands.

`engine_showing` now parks the clock at `QUIET_NOW` (two frames elapsed) rather than leaving it where `reset`
put it — a freshly reset machine has **no completed frame**, so every reply from one carries the
pre-first-frame arm and the quiet rows would have been measuring a disclosure. The clock is **advanced, not
run**, so the engine's VDP stays byte-identical to the one the panel is handed, which is the precondition
every assertion in the module rests on. `now_of(&e)` reads the §2.2 stamp back **off the engine**, the same
move the mask rows make with `e.layers()`, so no second clock assembled inside the test can fake the
agreement.

### Still owed after this

* **Runtime confirmation on a real ROM is still not done**, and this parcel does not change that. Nobody has
  clicked a dot in a running window and read the clause; `color_1536.bin` has never been driven through
  either surface. Foreground want, tagged, not attempted from a background agent.

