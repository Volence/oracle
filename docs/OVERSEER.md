# OVERSEER.md: booting an oracle overseer session

> **Boot prompt (paste into a fresh session):**
> You are the oracle overseer. Read `docs/OVERSEER.md` in full, then the newest dated
> handoff/recon docs it names. You orchestrate subagents (dispatch → verify firsthand → merge);
> you do not implement directly. Work the queue top-down; keep this file current at merge windows.

Companion: the suite-wide protocol at `empyrean/docs/OVERSEER-PROTOCOL.md` (shared patterns; this
file is the oracle-specific half). Repo ground rules: the workspace `CLAUDE.md`. **Solo-first:**
everything below is workable with no peer sessions up: the queue, the follow-up register (now in
`docs/OVERSEER-REFERENCE.md`), and every demand are committed artifacts in this repo; peers accelerate, they are never prerequisites.

## The role

Dispatch Opus subagents for implementation and recon; adjudicate contracts un-framed (a fresh
Fable agent, no steer). **The seat is on HOLD, and that is a live owner ruling, not this seat's
call**: see owner ruling 2 below, and the substituted-reviewer rule under the 2026-08-27 hub ruling
for how adjudications run meanwhile. *(The 2026-08-22 provenance audit of the seat's original
ratification is closed and moot by that ruling's own words; moved verbatim to `OVERSEER-LOG.md`
2026-09-03. The standing rule it produced, never record an approval whose granting act you have
not seen, is live, and lives in the bars, now in `docs/OVERSEER-REFERENCE.md`.)* Verify every gate firsthand before accepting a slice;
make the design rulings (delegated by the owner: pick best, record why); merge and push. The owner's standing
directives: **a legacy surface or demand spec is the compatibility floor, never the design
ceiling** (run a visible better-approach pass on every request), and **instrument co-development
with aeon** is the ratified lane (their diagnoses name gaps; we build them; the engine gets fixed
with tools that then exist).

## The boot read is bounded (100,000 B, gated)

Closed history is in **`docs/OVERSEER-LOG.md`**, not read at boot. Governing rule:
`origin/main:docs/OVERSEER-PROTOCOL.md`. **A live ruling goes here, never only in the log.**

**Split by WHEN a rule is read, not by size** *(owner, 2026-09-04T15:38:47Z: one call for all six
lanes; the bound stays at 100,000 B and is never raised)*. This file is what you need to act **at
boot**. The house bars and the ops lessons live in **`docs/OVERSEER-REFERENCE.md`**. Open it at
the moment it applies: **before dispatching** a wave of agents, **before reviewing** returned work,
and **before landing**. It is not part of the boot read.

The rest of this section (orig lines 38-70: the cut history, the procedure and its proof-tool traps) moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before cutting this file or quoting its size.

## The queue (2026-08-19 end of day; reorder only with cause, record the cause)

*(Items 1-7 are closed and moved to the log. Item 8 keeps its live tail below; its closed sub-arcs
moved with them.)*

8. **▶ OPEN: THE ACCEPTANCE CONTRACT.** The definite list of what the successor must serve before it
   replaces the legacy C++ server. **Re-derive the membership, never transcribe it**: the machine-
   enforced source is `SCHEMATIZED_NOT_ADVERTISED` in `crates/oracle-aether/tests/schema_conformance.rs`,
   asserted as a whole sorted set, so it cannot drift silently. Board row `ACCEPT-16`. The arc's closed
   history (survey, CR-A, trio, CR-B) is in the log.

**Order of work, 2026-09-13 (the HUB's ruling, not his; overturnable by one word from him).** Under the owner's
goodnight delegation (empyrean `cdb8035`, verified an ancestor of their `origin/main`, a docs commit carrying his words:
*"if you need any decisions you don't think I need to answwer feel free to confer yourself"*), the hub ruled this lane's
pick by his 09-09 order of work (finish lens items first) and his 09-11 aim (*"oracle's cleaned up a bit"*):
**H22-68000-DECODE (design doc first) → M24-NEEDS-A-CURRENCY → M1-FILL-OVER-TIME.** LENS-WAVE-1's two look questions
stay his. If H22 needs a call that is his (look, or an irreversible bet), file the card and take M24.
**H22 design LANDED (merge `1404a07`, docs only) and RULED under delegation:** option E* adopted, staged; landing 1
(static-copy filler) takeable now; landings 2-3 HELD until decode share is re-measured on a workload that is not mostly a
VBlank spin loop (the doc's own falsifier); M32 refuted. **Next is M24; shape it with F-Z80-ACCESSES-UNWATCHED in the follow-up register (`docs/OVERSEER-REFERENCE.md`).**
**Hub amendment (2a), 2026-09-13 (verified at empyrean `3bb082d`, docs commit carrying it; the hub's, overturnable by him):
F-MACHINEREPLACED-EVENT-RACE goes AFTER M24 and AHEAD OF M1** (it reddens unrelated commits; may jump ahead of M24 if needed).
**M24 design LANDED (merge `9a09adc`, docs only, `docs/2026-09-13-z80-timing-currency-design.md`) and RULED by this seat
(delegated design call): the currency is in-tree Z80 timing probes (§1, §7), not an access-stream digest.** Order from here:
**§7 parcels 1 (F-Z80 caveat, XS) then 2 (probes C1-C4, S; closes M24-NEEDS-A-CURRENCY)**, one agent after the other, no
further ruling needed → **F-MACHINEREPLACED-EVENT-RACE** → **parcel 3 (EI delay, no ruling)** → M1.
**Parcels 1-2 LANDED 2026-09-13 (merge `453aa96`, one agent; M24-NEEDS-A-CURRENCY CLOSED). F-MACHINEREPLACED-EVENT-RACE
LANDED 2026-09-13 (merge `7ccd42d`, test harness only). F-PLAYER-SCREENTEXT-FIRST-READ
LANDED 2026-09-13 (merge `70a83cd`, test barrier + comments). NEXT: parcel 3** —
this seat's sequencing, (2a)'s own cause: a flake that reddens unrelated landings costs the signal every later one reads.
What parcels 3-4 must move is pinned in `crates/oracle-core/tests/z80_timing_probes.rs`'s header table (C1-C4, C2 now split
C2a/C2b); neither may move C3. Go for this pair: the hub, ruling (2) applied (empyrean `f1220de:docs/OVERSEER.md` 74-89). Parcel 3 is lens M24 proper,
so it follows the flake (every landing reads CI). Parcels 4 (/INT level: R6's medium-confidence corollary, TAG listen),
5 (Z80 access hook: a contract CR) and 6 (digest golden) each need a ruling when reached. New F-Z80 sub-finding, verified:
the `fc` watch filter is optional and the tap emits raw Z80 addresses, so a watch on ROM `$004000` records Z80 FM writes.
**OWNER, 2026-09-13T21:53:38Z (heard by the hub, banked verbatim at empyrean `a449627`, a docs commit on their `origin/main`):**
*"cut sounds good. I don't care about a or b they both look good, you can make the decision."* **d-39 CLOSED: the hub
picked `a-shared-card`** (theirs, overturnable by one word); delete B in one move (STYLE-NUMBER-BAKEOFF). **The cut is GO:**
this lane cuts its own OVERSEER.md to about 40 KB by the when-read rule (aeon's ~20 KB boot file the model), row OVERSEER-CUT.
**OVERSEER-CUT LANDED 2026-09-13 (merge `e4b4bbf`): 97,771 → 26,517 B, fourteen blocks to the reference. Hub GO covers the
next two in order (empyrean `492a2ac`): NEXT is delete B (STYLE-NUMBER-BAKEOFF), then parcel 3.**
**Delete B LANDED 2026-09-13 (merge `f0f0a96`, agent tip `421b005`; STYLE-NUMBER-BAKEOFF CLOSED, DATA-DISPLAY-AUDIT
unblocked). NEXT: M24 parcel 3 (EI delay), no further go needed (the hub, empyrean `579d485:docs/OVERSEER.md:173`).**
**M24 parcel 3 LANDED 2026-09-14 (merge `d421f51`, agent tip `06bd27a`; go: the hub under his delegation, read at empyrean
`f13feff:docs/OVERSEER.md:99`).** The EI delay: C1 `A`, C2a/C2b `HL` 0 → 1, nothing else moved; SST-z80 now grades the
corpus's `ei` field. Saves from before `d421f51` are refused by the layout fingerprint (precedent `28e4587`). **NEXT: M1-FILL-OVER-TIME**
(the hub's 09-13 order, H22 → M24 → M1). Parcel 4 (`/INT` level) needs his listen ruling when reached; no card filed yet.
**M1 DESIGN LANDED 2026-09-14 (merge `b34bac2`, docs only, agent tip `acb753b`; go: the hub under his delegation, order read at
empyrean `3d322af:docs/OVERSEER.md:105`; the go stops at the doc).** `docs/2026-09-14-m1-fill-over-time-design.md`: lazy catch-up on
`fifo_slot_clock`, one trailing `DmaRequest::FillRunning` variant, no save-layout move, old saves load (3/3 measured). **P1 FILL-RUN waits on
the hub's R1-R4** (R1 watch-hit attribution, wire-visible; this seat's hypothesis: the trigger pc as `FillRunning`'s payload keeps today's
meaning at no layout cost. R2 C-6 per-step stamps for fills. R3 old builds refuse new mid-fill saves. R4 three synthetic fixtures gain a
busy-poll). Parcel 4's card is filed: **d-51** (listen-first). **`docs/lane-status.json` stays TRACKED and is committed with the lane files at
landings** (the hub's 09-14 ask, this seat's call: untracking reddens `tools/land.sh` G2b, which validates the committed blob).
**HUB RULINGS R1-R4 on the M1 design, 2026-09-14T02:08:55Z (verified: empyrean `a3e2c42`, ancestor of their `origin/main`, the docs
commit carrying it, `docs/OVERSEER.md:215`; the hub's under his delegation, overturnable by one word from him). All four are this seat's
recommendations.** R1: a fill-write watch hit keeps today's `pc`, the TRIGGERING instruction (`protocol.md`: "the accessing instruction's pc
per hit, so no triggerPc key"), carried as `FillRunning`'s payload; per-step `mclk` stands. **If P1 finds that moves the save layout, STOP and
go back to the hub: option (c) is a contract change and runs as a CR in empyrean.** R2: per-step stamps for fills only; C-6 stands for
68k-to-VDP DMA. R3: accepted. R4: a busy-poll after each fill in the three synthetic fixtures. **NEXT: P1 FILL-RUN** (design §7 row 1),
no further go needed.
**OWNER GO ON THE TEST ROM, 2026-09-16, relayed by empyrean-b6 as *"oracle can keep fixing the issues with the test rom"*; P1 DISPATCHED on it
(branch `parcel/m1-fill-run`, worktree `../oracle-m1fill`, base `7e2bd6e`).** ⚑ **VERIFIED IN PART, and the part that failed is the part they quoted
to me.** At dispatch the words were at no remote (their tip `d8814c1`; the lane said so and dispatched anyway, the chain being unambiguous). They are
now partly checkable: empyrean `308df49` is an ancestor of their `origin/main` and `docs/OVERSEER-LOG.md`'s 2026-09-16T02:06:06Z entry records an owner
GO (*"Yeah go for it"*, *"Also feel free to have everything continue phase 2"*) and the relay to this lane — but as a PARAPHRASE, *"his separate words,
M1 fills = the last 3 of vdp_port_access 119/122"*. **`git grep "keep fixing the issues" origin/main` finds nothing.** So: the granting act is witnessed,
the oracle-specific sentence is still relay-only. Do not upgrade it to verbatim on this evidence. **The general shape, and it is new: a hub that pushes
its own summary of his words satisfies the push rule while leaving the quoted sentence exactly as unverifiable as before.** A relay flag comes off when
the WORDS are at a committed revision, never when a record of the relay is.
**P1 FILL-RUN LANDED 2026-09-15, merge `549f020` (agent tip `569439d`, branch `parcel/m1-fill-run`), PUSHED and origin confirmed moved.
M1-FILL-OVER-TIME CLOSED; `vdp_port_access` 119/122 → 122/122, `PORT_ACCESS_FAILING` empty.** Verified firsthand on the MERGED tree by this
seat, not taken from the agent: fmt clean, clippy 0, **debug 90 legs / 2915 / 0 / 6 and release 90 legs / 2918 / 0 / 3** (both profiles, per the
two-profile rule), the 122 rows aggregated from the ROM's own output (`ROWS=122 PASS=122 FAIL=0`), and red-first M-F reproduced here (mutation
quoted from disk, hit named `$3010` against `$32BC`, restored from committed `569439d`, green again). **R1's tripwire NOT tripped, re-measured
with this seat's own probe on both trees**: `layout_fingerprint` `dff350afa2eb3e1d`, snapshot 140,072 B, identical — so the hub needs no CR.
⚑ **The limit of that instrument, stated rather than glossed: the fingerprint hashes a POWER-ON snapshot, where no fill is running, so a trailing
variant cannot move it BY CONSTRUCTION.** It is the right instrument for the owner-facing claim (his saves load) and it is near-vacuous for
"the layout cannot have moved"; the load tests and R3's clean `UnexpectedVariant{allowed:0..=2,found:3}` refusal are what carry that half.
Two agent deviations ACCEPTED and banked in the design's landed note: the three fixtures need **display-off as well as** R4's busy-poll (a 64 KiB
fill is ~6.6 frames displayed against ~1.5 blanked), and the watchpoints doc's `[0,0,0,0]` "status arm entirely dead" row was false — `build_pad_poll`
drives 6,849 status reads in 2 frames. **THREE LIVE-LOOK TAGS ARE THIS SEAT'S, NOT AN AGENT'S** (no emulator from a background agent): §1.5 FIFO
EMPTY/FULL during a running fill, §1.6 a non-DMA command word and a data-port read mid-fill, §1.1 the per-line deficit. **H22 LANDING 1 LANDED 2026-09-16, merge `a663ccf`** (agent tip `737c7fa`; docs correction `b6410dd`; pushed, origin confirmed moved).
Verified firsthand on the merged tree: clippy clean in BOTH shapes, debug 90 legs / 2917 / 0 / 6, release 90 legs / 2920 / 0 / 3, and both new gates
reproduced red-first with the mutation on disk then restored. **It CORRECTED ITS OWN DESIGN'S CLAIM: −4.71 %/−4.88 % measured against the stated
−7.5 %**, with a byte-identical null arm validating the instrument at ±0.5 %; recorded at the design's canonical site (`b6410dd`), the original left
standing as the spike's record. **Landings 2-3 STAY HELD**, and this is evidence FOR the hold: the one figure this parcel could check was overstated
by a third. **TESTROM-SPRITE-MASKING (the clause below) LANDED 2026-09-16 as merge `25d9b4a`; the paragraph after this one carries the live NEXT.** It is kept whole, stale pointer included, because its lessons are about itself. **THEN-NEXT was: `TESTROM-SPRITE-MASKING`** — ⚑ *re-derive before proposing it; this clause names its SOURCES and deliberately carries no count and no
cause*: the scorecard row in `docs/2026-07-25-testrom-conformance.md`, ledger row P1 in `docs/2026-07-16-vdp-pixel-known-differences.md`, and
**`F-POSTHOC-STALE-CARRY` in `docs/OVERSEER-REFERENCE.md`, which any reading of the first two without the third will get wrong.**
⚑ **THIS POINTER LINE IS THE ARTIFACT CLASS THAT BIT THE HUB TONIGHT** (they proposed M1 hours after acknowledging it closed, off their own boot-read
copy of a figure): **a `NEXT:` clause naming a live measurement rots the moment the work lands, and it is the one sentence that costs someone hours.
Re-measure at the moment of the recommendation, from the artifact, never from this file.**
⚑ **AND THE CLAUSE THEN DID IT AGAIN, TO ITS OWN SUBJECT, WHICH IS WHY THE FIGURES ARE GONE RATHER THAN UPDATED.** It read *"2 failures … from one cause,
the whole-sprite pixel-budget cut"*. Re-derived at boot 2026-09-16 from the scorecard: **at most ONE of those two is ours.** Test 6 (MASK S1 ON DOT
OVERFLOW) reads `FAIL` through the scraper's post-hoc `Vdp::render_line` and **`PASS` through the live path** — `render_line` re-seeds the sprite
dot-overflow carry from the end-of-frame value on every line instead of advancing it, so that `FAIL` describes a machine state that never existed. The row
and P1 are knowingly left unamended; the reason is in `F-POSTHOC-STALE-CARRY` and it is load-bearing: re-pathing the scraper re-derives four glyph
constants **that were themselves pinned from post-hoc pixels**, i.e. the defect reproducing itself one layer down. **Two wrong figures out of this one
clause in six hours** (the hub's `119/122`, then this) — the strongest available argument for *name the source, never the figure*, made against the
sentence that states the rule. ⚑ **Its second lesson is a DIFFERENT mechanism from its first, and the fixes do not transfer** (the hub's split, banked
empyrean `docs/OVERSEER.md:85`): the `119/122` was **true when written and rotted**; this one was **wrong when written and never rotted**, surviving since
2026-08-15 because the artifact and a real failure are **identical in the output** — both a `FAIL` glyph. A rotted claim is fixed by naming sources; an
absence rendered as a positive finding is fixed only by **making the instrument fire on purpose**. Control arm here, one line, never run because the
harness is non-gating and the glyph "worked": **render a frame both ways and require them to agree.**

**SPRITE-MID-CUT LANDED 2026-09-16, merge `25d9b4a`** (agent tip `4a692ca`, lane files `9ddd5b7`; pushed, origin confirmed moved).
The per-line sprite pixel budget now cuts MID-SPRITE; `vdp_sprite_masking` test 3 flips, test 6 does not move, confirming
`F-POSTHOC-STALE-CARRY` owns test 6 and P1 owned exactly one of the two. Verified firsthand on the merged tree by the landing seat:
debug 90 legs / 2925 / 0 / 6, release 90 / 2928 / 0 / 3 (baseline 2917, +8 = 6 unit + 2 golden), red-first reproduced with the
mutation quoted off disk. ⚑ **Its transferable finding is a THIRD class beside the two above, and it is the strongest of the three:
`scene_no_mid_sprite_cut` was the ledger's NAMED LOCK for this defect and could never have detected it** — 256 is an exact multiple
of 32, so its ninth sprite straddled nothing. Proven, not argued: under the mutation reinstating the defect, the new fixture goes
red and that one stays GREEN. **A fixture can stand for years looking like coverage while testing nothing, and its NAME is what
stops anyone checking.** The control arm that catches it is one line and almost nobody runs it, because the new test going red
*feels* like the proof: **run the mutation against the EXISTING lock and watch what it does.** Copying that rule needs both halves —
the mutation AND a named prior fixture to run it against — or it collapses into an ordinary red-first check.

⚑ **AND THE `NEXT:` CLAUSE ROTTED AGAIN WITHIN THREE HOURS, WHICH IS WHY THIS PARAGRAPH EXISTS AND WHY THE ONE ABOVE IS LEFT
STALE RATHER THAN EDITED.** This is not either of the two classes it names. The sentence was **true when written**, and it rotted
**because the work it named SUCCEEDED** — success itself was the rotting agent. Note what does NOT reach it: *name your sources,
never the figure* is no help, because that clause **does** name its sources and names no figure. **The fix that fits is the
boot-file sibling of rule 8 (a `next` row is CONSUMED by being started): the same edit that starts or lands a row refreshes this
clause.** Booked as a habit, not a bar. Measured cost of the gap: the boot file pointed at a landed row for six hours while the
board and the memory frontier were both correct, so the exposure was outward — a peer or a fresh session taking a finished row as
the front. *(Class named by this seat 2026-09-16; adopted by the hub as a third sub-class, banked empyrean `70077e2`.)*

**LANDED 2026-09-16, merge `2428966`, pushed and origin confirmed moved (agent tip `1d736cc`): `F-PLANES-RASTER-EVERY-FRAME`, CLOSED** (worktree `../oracle-planes`, branch
`parcel/planes-raster`, base `9ddd5b7`; go from the hub under his delegation, anchors verified firsthand at empyrean `70077e2`,
an ancestor of their `origin/main`: `docs/OVERSEER.md:149` *"Do not boot into a stop and wait for a pick"* and `:156` *a lane
rebooted mid-project does NOT stop at its boot stop waiting for a pick*). ⚑ **THE BOARD'S OWN TITLE FOR THIS ROW WAS FALSE AND WAS
RE-DERIVED AT DISPATCH** — a fourth instance in one night, this one in a queue row rather than a pointer. It read *"redraws the
whole picture every frame even when nothing changed"*; the fingerprint gate has existed since `230ff33` (2026-09-05) and the panel
was BORN with it, so unchanged inputs are already skipped. The row's ORIGINAL 2026-09-06 booking (recovered from the board's own
git history, `3de47e4`) says the real thing: the saving *"never applies"* on rows whose picture changes every frame **by design**.
**A title restated from a booking drifts into its own negation, and the restatement is what everyone reads.** The parcel asks two
separable questions: **(A)** does `gather` (`crates/oracle-player/src/planes.rs:202`) mix scroll and spans into the fingerprint for
any non-Window plane **regardless of `want_scroll`**, so an unscrolled outline-off view re-rasters to a byte-identical image every
frame a game scrolls; **(B)** `dot`/`nibble` re-derive the cell and the tile base **once per pixel**, 64 times per 8x8 tile, which
is where the booking's half a million lookups live. Byte-identical output is the hard constraint, and the measurement ships with a
null control arm per the lesson above.

**PLANES-RASTER's result, and the dispatch hypothesis was CONFIRMED AND TOO NARROW.** Finding A held and reached further than
`scroll`: `gather` also mixed the plane base, regs `$0B`/`$0D` and the armed H-interrupt (`$00`/`$0A` — a *sentence* beside the
picture, never a pixel in it) into the texture's key, and `refresh` XOR'd the outline toggle in unconditionally, so toggling the
outline on the **scrolled** view — which ignores the flag — also bought a full re-raster of an unchangeable picture. `Inputs` now
carries `content` and `viewport` apart and `Inputs::fingerprint(outline)` folds the viewport in only where the raster reads it.
Finding B landed as a per-cell raster plus a per-raster colour LUT. Verified firsthand on the merged tree: fmt clean, clippy 0 in
both shapes, debug 90 legs / 2934 / 0 / 7, release 90 / 2937 / 0 / 4, **+9 passed and +1 ignored in both profiles reconciled BY
NAME** against the ten `#[test]` attributes the diff adds; two red-firsts reproduced here with the mutation quoted off disk and
every exit read unpiped; timing re-run by this seat (null control **1.00x**, then 1.73x / 1.39x / 2.67x).
⚑ **THE AGENT CORRECTED THE BOOKING'S PREMISE, AND THAT IS THE FINDING WORTH MORE THAN THE SPEEDUP.** The row was booked on *"up to
half a million pixel lookups per frame"* — and the per-cell raster, which removes exactly those lookups, was worth **1.10x**. LLVM
already hoists most of the per-pixel address derivation out of that loop. What it cannot hoist is the per-dot transparency branch
and the CRAM tuple load, and settling those once per raster is what earned 1.73x. **A cost model read off the SOURCE can be wrong
by the whole optimiser**, and the only thing that separated the two stories was a measurement with a null arm. Second instance in
two days of a parcel correcting its own design's figure (H22 landing 1 was the first, −4.71 % against a stated −7.5 %); **both were
caught by the same instrument and neither by a review.**
⚑ **A THIRD SUB-CLASS ARRIVED INSIDE THE PROOF ITSELF, AND IT IS BOOKED RATHER THAN FIXED.** The differential runs both arms through
the same `covered_edges`, so a change to the outline moves both arms together and the comparison stays green while the picture
moves — **the parity-pair bar, arriving as a limit on a proof this seat accepted.** The agent named it unprompted instead of
quietly relying on it, which is the behaviour the bars exist to produce. Registered as `F-OUTLINE-UNPROVABLE-BY-DIFFERENTIAL`; it
matters now because the outline is roughly **half the cost of the panel's default view**, so the next parcel in this area is aimed
at code this parcel's instrument cannot see.

**OWNER-UX-GROUP-B LANDED 2026-09-16, merge `535f719`** (agent tip `6af2d30`, branch `parcel/owner-ux-chrome`, base `c13e97b`;
`tools/land.sh` GREEN and pushed, origin confirmed moved by this seat: release 90 legs / 2937 / 0 / 4, byte-for-byte the
baseline, which is the expected result for a diff whose only change to `crates/` is comments). **The row was NOT three open
items and the parcel's output is the re-derivation, not the fix.** Verified firsthand here, not taken from the agent:
`ad7bc78` and `4f31f0d` (2026-09-09) and `541c872` (09-04) are all ancestors of the base, so capture §1.7 (dragging) and the
CHROME half of §1.4 and the navigation half of §1.1 were already done. What is genuinely left is the LAYOUT half of §1.4,
and it is a look call: cards **d-52** (the per-tab `x`) and **d-53** (the right column's three stacked leaves) are filed.
⚑ **THE ROW'S ID NAMES THE WRONG GROUP** — the board says `GROUP-B`; in the capture these three are the hub's **Group A**
(§3), and Group B is a disjoint list. Cite capture sections, never either name.
⚑ **AND THE BOARD CLAUSE WAS STALE BY ONE COMMIT, WHICH IS A FIFTH INSTANCE AND A NEW SUB-SHAPE.** It rested on lane-log
`13:29:21Z` — the ANALYSIS — and the WORK landed at `13:56:29Z` the same morning. Every earlier instance this week was a
restatement drifting from a source that stood still; **this one is a restatement that was TRUE OF ITS SOURCE and false of
the tree, because the source was a note about what was about to be done.** A lane log records intent as readily as outcome
and nothing in the entry distinguishes them. **Re-derive a queue row's premise from the TREE (`merge-base --is-ancestor`),
never from the note that booked it** — the note cannot know what happened after it.
⚑ **The parcel's own fix is a COMMENT, and it is the class worth keeping: a number in our own record wrong by half.**
`main.rs` said `egui_dock` draws *"the per-tab `x` on the ACTIVE tab"*. Re-verified at this seat against the crate:
`widgets/dock_area/show/leaf.rs:431` computes `show_close_button` INSIDE the per-tab loop at `:402`, with no reference to
`is_active`, and `nav.rs:69` had it right the whole time — **the crate asserted both things and the wrong one sat at the
decision site.** `ui::initial_dock` is 4 leaves / 11 tabs, so the remaining chrome is FIFTEEN controls and not eight;
counted firsthand in the committed screenshot. **A cost estimate for the next parcel in this area was being read off a
sentence that was wrong by half, and only a picture settled it** — the same instrument that corrected the last two parcels'
own figures. Second correction in the same parcel: `nav.rs` called `F-NAV-COLLAPSED-LEAF` *"real and still open"*; the
LIMITATION stands, the BOOKING closed as `d-31` `leave-it` 2026-09-09T23:18:15Z (verified in `decisions.jsonl`), and the id
is in no queue — so a reader who went looking would doubt the paragraph rather than the row.
⚑ **BOTH CARDS ANSWERED 2026-09-16, BY THE OWNER, AND THE ROW IS CLOSED WITH NO CODE CHANGE.** **d-52
`keep-it`** (the per-tab `x` stays) and **d-53 `leave-it`** (`ui::initial_dock` stays 4 leaves / 11 tabs at
0.68/0.45/0.5) — **both AGAINST this seat's recommendations** (`remove-the-x`, `you-show-me`). Entries
`d-52-answered` / `d-53-answered` in `docs/decisions.jsonl`. **So OWNER-UX-GROUP-B closes as DECIDED, never
as FIXED, and the distinction is load-bearing here**: his original complaint (*"if multiple are open it's
really hard as well"*, and the clutter) **is still true of the window** — he accepted each option's stated
cost rather than having it removed. A later reader finding the strips crowded is meeting a ruling, not a
regression, and should re-ask him rather than re-open the parcel.
⚑ **PROVENANCE, flagged because this repo's own rule makes it matter: BOTH ARRIVED AS OPTION SELECTIONS
THROUGH THE CONSOLE, WITH NO WORDS ATTACHED.** `said` is `null` in both entries rather than the option's
name dressed as a quote. A selection is a witnessed granting act and is strong; it is **not** verbatim
text, and the two must not be allowed to blur — the failure mode this file keeps recording is a paraphrase
hardening into a quotation one reader at a time.

⚑ **What the captures CANNOT say, stated rather than glossed:** all three shots are X11 on a private Xvfb and his desktop is
Wayland, `xdotool` is not installed so no gesture was synthesised, and §1.7's Wayland-safety is an argument from `egui_dock`'s
source (pointer drag, in-viewport `egui::Window`) rather than a measurement on his compositor. Tab drag does NOT inherit the
`DroppedFile` blind spot; that is reasoned, not observed.

**NEXT: `DATA-DISPLAY-AUDIT`, and its premise is now RE-DERIVED rather than booked** — the derivation is
banked at the audit's own canonical site (`docs/2026-09-05-debug-window-audit.md`, addendum 2026-09-16, written
at `16eef9e`), so a brief is composed from that addendum and never from §2. Sources, no figures: that addendum,
the per-panel build order it corrects, the style rules, and the 2026-09-03 tab ruling in `docs/OVERSEER-REFERENCE.md`.
Unblocked since `f0f0a96`. **Two items in it are HIS eyes, not ours.**
⚑ **The re-derivation earned its cost immediately, and in BOTH directions, which is new.** Every earlier
instance this week was a booking that overstated what was left. This one **overstated and understated at once**:
item 1's em dash was already gone (closed as a side effect of the P10 dash sweep, `7e16748`, which had no idea
it was touching this row), while its two secondary sites had **drifted ninety lines** from the cited `:504`/`:518`
to `:596`/`:610`. A brief written from §2 would have sent an agent to fix something already fixed and to read
two wrong addresses. **A booking does not rot in one direction, so "is it still needed" is only half the check;
the other half is "is it still WHERE it says".**
⚑ **And a rule-by-rule claim can be PARTLY closed by a sweep keyed to one of its rules.** §0.3 charged one
expression under P1, P3 and P10. P10 is closed and the other two are untouched — by a parcel that was not
looking at this row at all. **A multi-rule finding needs re-checking per rule, because nothing anywhere records
that a sweep closed a third of it.**
⚑ **One provenance check that changes how the item reads, and it is why it was worth running:** the doc comment
above `memory.rs`'s `Line` declares the JSON passthrough deliberate (*"Nothing here paraphrases the server"*).
`git log -S` puts it at `9c4908f`, **two days BEFORE the audit read that code and named the line its
highest-value fix.** So it is prior art the audit already overruled, not a later ruling — doing item 1 overturns
nothing. **A rationale sitting at the decision site outranks nothing by being there**, and a reader meeting it
cold would reasonably have stopped.

**DATA-DISPLAY-AUDIT ITEMS 1 + 3 AND THE PREREQUISITE LANDED 2026-09-16, merge `beed8d9`, pushed and origin
confirmed moved (agent tip `3c3c94a`, branch `parcel/data-display-1`, base `61288ff`; landing docs `75a76e8`).**
Verified firsthand on the merged tree by this seat, not taken from the agent: fmt clean, clippy 0 in BOTH shapes,
**debug 90 legs / 2946 / 0 / 7 and release 90 / 2949 / 0 / 4**, +12 in each reconciled BY NAME against the twelve
`#[test]` attributes the diff adds. Red-first reproduced here with the mutation quoted off disk and restored from the
committed baseline (`git checkout beed8d9 --`, tree clean before and after). ⚑ **CI READ TO COMPLETION AND GREEN** at `82eddc8` (and `75a76e8`), re-read from `gh run list` rather than taken
from the waiter's exit: all three jobs `success` — Determinism gate, Replay playthroughs (release), and
Build/test/clippy/fmt, the last running 14:19:49Z → 14:59:18Z. **Duration 41 min, inside this file's own measured
35-45 min band**, checked because a fast completion would have meant a job that did not run.
⚑ **THE FINDING, AND IT IS THE SECOND INSTANCE IN TWO DAYS OF THE SAME CLASS, WHICH MAKES IT A PATTERN ABOUT NAMED
LOCKS RATHER THAN A COINCIDENCE.** Under a restored catch-all dump, four new gates go red while
**`ui::json_tests::no_served_value_can_put_raw_json_on_the_screen` prints `ok`** — a lock named for exactly this
defect class, standing while seven production sites committed it. `scene_no_mid_sprite_cut` was the same shape a day
earlier. **Both were found by the same one-line control: run the mutation against the EXISTING lock, not only against
the new test.** The lock is not vacuous — it walks `render`'s variants and that is all it ever claimed — but **its
NAME describes the defect class and its BODY covers one function**, and the name is what stops anyone checking. Twice
now the name has been the thing standing between a real gap and the person who could have seen it.
⚑ **AND THE PARCEL PUBLISHED THREE WRONG COUNTS IN THE ADDENDUM ANNOUNCING THAT A BOOKING'S COUNT WAS WRONG.**
Re-derived at the merge and corrected at the audit's own site (`75a76e8`): `describe_reply` has **seven** production
call sites in **three** files, not *"eight in six"* — **six is the addendum's own table ROW count read as a file
count**, and the eighth is `palette.rs`, which the next paragraph says was deliberately left, so the sentence
contradicts its own page. The P10 positive control of *"243 em dashes"* reproduces at **no** revision (267 at base,
271 merged). **Not one of the three reaches the code**, which is the point: **a count in prose has nothing checking
it.** Fourth consecutive parcel here to correct its own brief, and the first to need correcting in the direction it
was correcting. The load-bearing halves all held and were re-verified: zero em dashes in the Profiler body at base,
and §4's four cited P10 lines land in `pacing`, so that charge was mis-addressed as well as closed.
⚑ **Corrections to THIS SEAT's own brief, both from the agent and both right:** `text_w` never took an
`objects::Col` (I flagged that one at dispatch), and **`header_cell` did and is named by neither §2 nor the morning
addendum** — so the prerequisite set was right in count and wrong in membership, in both directions, from the same
cause as §2's item 1. §4's eight Profiler citations are stale by ~1,080 lines (`fn profiler` was `:2761`; `:1674` is
inside `fn pacing`).
**`palette.rs`'s echo RULED to stay raw — `L-15` in the ledger**, delegated design call, no reviewer (seat on HOLD).
The command palette is a raw RPC console whose user typed a method name and came for the wire shape; P1/P3 do not
reach a surface whose subject is the wire. **Banked in the ledger rather than as a comment at the site**, because
this lane found twelve hours earlier that a rationale sitting at a decision site outranks nothing by being there.
`objects.rs`'s `Row::cell` catch-all booked as latent, for parcel 10.
**NEXT: the rest of `DATA-DISPLAY-AUDIT`** — parcels 4 (Watchpoints) and 5 (Breakpoints) reuse the landed
`table`/`Cell`/`TableRow` furniture directly. ⚑ Re-derive each from the TREE at the moment it is proposed, never from
§4, whose citations this landing has just measured as ~1,080 lines stale.

**SEQUENCING, this seat's, 2026-09-16, after the hub's nudge and recorded as a REASON rather than a wait.** The
hub observed — correctly — that this lane sat at a boundary with `awaiting` saying *"waiting on your pick"* while
its own `next` row needs no pick, which is the **inflating** direction of sigil's finding (*a row that manufactures
an owner-wait is the kind nobody audits, because it looks like care*). `awaiting` was fixed on the spot. The
licence not to wait is real and was verified firsthand rather than taken from the nudge: empyrean `70077e2` is an
ancestor of their `origin/main` and carries both *"Do not boot into a stop and wait for a pick"* and *a lane
rebooted mid-project does NOT stop at its boot stop*. **The dispatch is nevertheless HELD, on the hub's own
asymmetry: a background agent does not survive the `/clear` this lane is queued for on his card**, and the
protocol's rotation rule says to bank enough state to re-dispatch from the repo alone. That is exactly what the
addendum above is. **So the hold is on the AGENT, not on the row**, and it costs nothing: a fresh session boots,
reads the addendum, and dispatches without re-deriving anything.

The follow-up register and every registration under it (orig lines 112-440, through F-LEGACY-SILENT-DEFAULT) moved whole to docs/OVERSEER-REFERENCE.md (heading "Follow-up register"); read it when picking work past the order of work above, or before touching an area a booking names.


## ⚑ OWNER RULING: PUSH AUTHORIZATION. ✅ **CONFIRMED DIRECTLY BY THE OWNER, 2026-08-24, IN THIS SESSION**

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before pushing.

## ⚑ OWNER RULING, 2026-09-02T18:20:42Z: CUT THE CEREMONY. **IT OUTRANKS EVERY BAR IN THIS FILE.**

Verified at empyrean **`90554f2`**. ⚠ **Relayed here 09-02 and never banked: zero occurrences in this
file or the log until 09-03, so the 09-03 session spent hours on apparatus it forbids. The absence is the
failure.**

Owner, asked *"did we do something beurocratic to slow things down?"*: *"Yes please cut anyything that's
arbitrarily slowing us down without like an actual good reason please … as long as it's correct and stuff
and hitting our goal, that should be what we mainly care about."*

⚑ **THE SCOPE CLAUSE BELOW IS A LANE/HUB INFERENCE, NOT HIS WORDS, AND ITS CONDITION WAS MET 2026-09-06.**
His quote carries **no expiry and no project scope**; the "few weeks on this project" clause is his MOTIVE.
Empyrean labels the expiry as *the hub's application*; this file carried it in the register reserved for his
verbatim text — the seraph-hold defect, second file, opposite direction.
⚑ **RULED BY THE HUB 2026-09-10 under standing delegation (empyrean `origin/main`), overturnable by him on
read-back. (a)** The boot-doc-growth prohibition **has lapsed**, by the hub's terms not his. **(b)** It does
**not lift into permission**: the undated preference stands, so a new bar is now **governed by his test** —
*does it arbitrarily slow us down without an actual good reason* — rather than forbidden. **(c) Each hold that
rested on it is RE-DECIDED ON ITS OWN MERITS**, neither auto-revived nor auto-kept. Applied in the HERMETIC GATE ruling (moved to `docs/OVERSEER-REFERENCE.md`):
`SCHEMA-DRIFT-NIGHTLY` survives (merits argued, only the blocker outlived them); `F-CITATION-LINT` does not
(its own text says the hold was *the moratorium, NOT the merits*, so there is nothing to inherit).
⚑ **CORRECTION ACCEPTED, against this seat:** I proposed treating it as in force *because in-force is what
every lane observes*. **Circular** — universal observance of a lapsed rule is the SYMPTOM, not evidence
against it. **When a rule's only remaining support is that everyone still obeys it, that is the finding.**
⚑ **AND I THEN COMMITTED THE MORNING'S OWN LESSON:** fixed this at its canonical site and left its
paraphrases standing one screen away (two revival conditions, both firing on the lapse). **A claim repaired
at its canonical site leaves its paraphrases standing** — I banked that at 08:1xZ and broke it at 09:0xZ.

*Scoped, per that inference, to EFFECTS-W1:*
* **No new process bars, no rulings about rules, no boot-doc growth.** New bars go to
  `docs/OVERSEER-PENDING-BARS.md` PARKED, not into force. The protocol pass waits.
* **A correction is ONE LINE in the lane log. No story.**
* **DoD items and the bug tier only**: no cross-lane audits, no instrument or ledger work, no re-measuring
  a peer's numbers, unless it blocks a DoD item or ships wrong output.
* **Status files, decision cards and lane logs are written once in the accepted shape and not polished.**
* **The boot-read gate stays, but nobody hand-trims for it**: over the bound, move history out in ONE cut
  and carry on.
* **"Correct" is unchanged:** a landing still builds, passes the lane's own tests, and shows on screen or
  in a witness. What is cut is certifying things that are not the feature, and record-keeping about the
  record-keeping.

## ⚑ OWNER RULING, 2026-09-03T05:21:01Z: **REPORT TO THE HUB WHENEVER YOU FINISH OR STOP**

⚑ **RELAYED BY empyrean-01, NOT WITNESSED BY THIS LANE**: same flag, same reason, as the relayed
rulings below. **Verified firsthand rather than taken on the relay's word**, which is what makes it
usable: empyrean **`f04afe3`** is an ancestor of their `origin/main`, `--stat` shows it is a docs commit
carrying a docs ruling (so its SHA class matches what it anchors), and the owner's words are present in
the blob at that revision. His words: *"tell the agents any time theyy finish work or stop to report to
you please, loosk like aeon's stopped right now"*.

**Standing, every lane: a landing, a boundary, a block, an owner question, or a dispatched agent
returning (anything that leaves nothing running) gets ONE message to the hub saying what landed (SHA
emitted from git output, never typed) or why you stopped, and what you need.** Going quiet with nothing
running is the state he named.

⚠ **This does NOT license the aggregate waste bar 18 exists for.** The trigger is *finishing or
stopping*, not *changing something*: a pin, a correction, or an interesting finding still needs a named
reader before it is sent. The two rules compose: report your own state unconditionally; relay a *fact* to
a peer only when you can name their dependency on it.

## ⚑ OWNER RULING, 2026-09-09: **SAY WHEN YOU NEED A CONTEXT CLEAR.** The sibling of the rule above

⚑ **RELAYED BY empyrean-cd, NOT WITNESSED BY THIS LANE**, and flagged per this repo's own rule; the owner's
words as transcribed: *"Remind the agents to let us know when they need a clear."* Banked here rather than
only in the log because it changes what a session does at a boundary. **Verify the granting act at
empyrean's `origin/main` when convenient and replace this flag with the confirmation; do not delete it.**

**The rule, all lanes: do not silently run down to a compaction or drift through one.** When a clear would
help, say so in the same breath as your status — **in the message to the hub AND in `lane-status.json`'s
`awaiting`** — together with **what a fresh session needs to resume.**

⚑ **And the resumption anchor is a FILE AT A COMMITTED SHA, never a summary.** That is the whole of why
this is a rule and not a courtesy: a summary is written by the session that is about to stop, is the one
artifact its successor cannot check, and is exactly the shape bar 20 names — a claim that lives only in a
message, with no reader who can meet the contradiction. A SHA the successor can `git show` is checkable;
*"I was in the middle of the lens highs"* is not.

**It binds this lane harder than most, because this lane runs several seats at once**: an overseer's own
context is the one thing not banked in the repo, and every dispatched agent's brief was composed from it.

⚑ **The honest failure mode to guard against is the opposite of the obvious one.** The risk is not asking
for a clear too late out of stubbornness; it is that **a session near its limit is the least able to judge
that it is** — the same reason `updatedAt` must come from the clock rather than from your own sense of the
time. So report a *measurement* rather than a feeling, and let the owner decide.

⚠ **AMENDED WITHIN THE HOUR, TWICE, AND THE SECOND CORRECTION IS AGAINST THIS SEAT.** The first form said
to report *"what fraction of the window is gone"*. **There is no such measurement available here, and I
reported one anyway: "about 9 percent".** Two separate defects, and the second is the one that generalises:

1. **(aurora's catch) The counter is not monotonic.** They watched `total_tokens` RESET UPWARD mid-session,
   ~13.65M back to 15.0M, and refused to derive a percentage from it. Correct: a percentage off a
   non-monotonic counter has a confident shape and no meaning, **which is the exact artifact this rule
   exists to prevent, so producing one to satisfy the rule defeats the rule.**
2. **(this seat's, worse, and it holds even if the counter were perfect) IT MEASURES THE WRONG QUANTITY.**
   `total_tokens` is a session **budget remaining**. *"How full is my context window"* is a different
   question, and budget spent is not window occupancy — long tool output is persisted to files rather than
   held, and the window is summarised on its own schedule. **I took a budget figure, renamed it occupancy,
   and divided.** Name-is-not-behaviour applied to a counter.

**So the rule is REWRITTEN rather than exempted, because the thing that actually decides the question is
measurable and neither number was it.** *"Would a clear cost anything?"* is not a question about occupancy
at all — it is **"is anything load-bearing living only in my head?"**, which is exactly what `atBoundary`
already encodes and which is fully checkable: uncommitted work, an agent holding a branch, a ruling not yet
in `OVERSEER.md`, a decision taken and not written down.

**Report, in this order:** (a) **what is unbanked** — measured, itemised, and the half that actually
decides it; (b) a budget figure **only if you have one, named as budget and never as occupancy**, with
aurora's caveat that it has been seen to move upward; (c) **if the number is not available, say so, and
never substitute the feeling it would have replaced.** Loud on unmeasurable, in the one place where the
temptation is to produce a plausible number because two peers just did.

⚑ **AND TWO LANES' COUNTERS ARE NOT ONE SCALE — MEASURED HERE 2026-09-16, and it is the clause most likely
to be needed when a peer quotes a figure AT you.** The hub asked this lane *"are you under the ~200k line?"*
after sigil named **~202,000** from its context budget counter. This seat's instrument reports **~14.9M of a
15M budget** — a different quantity entirely, and **the ~200k line is defined on sigil's instrument only**.
Answering it on this lane's number would have made this lane look **70x emptier** than sigil's while the two
figures answer different questions. ⚑ **The same session also watched its own counter move UPWARD (~13.89M,
later 15.0M)**, reproducing aurora's non-monotonic catch firsthand. **So: never convert a peer's threshold
onto your own counter, and never estimate the figure you do not have** — a demand to fill a field gets it
filled, which is the queue table's law arriving on a number.
⚑ **THE ORDER IS THE WHOLE RULE, and the hub corrected itself on it (empyrean `2ecc9f4`, verified an ancestor
of their `origin/main`): (a) WHAT IS UNBANKED DECIDES; (b) the figure is SECONDARY.** The ~200k line is a
**proxy** for the rule's own stated test — *once the banked state fully covers the live state* — and asking
"are you under the line" lets the proxy displace the test. **When (a) is *nothing*, (b) is never needed at
all**, and a lossless boundary whose next item waits on no one is TAKEN, not parked on a click that may not
come for hours.

⚑ **AND NOTHING WILL ROTATE YOU ON A TIMER, SO SAY IT EARLIER THAN FEELS NECESSARY** *(from the hub,
2026-09-16T13:1xZ; banked as the INSTRUMENT, deliberately not as the reading)*. Overnight auto-clear was
reported disarmed at the runtime that afternoon, where it had been armed an hour earlier. **Read it from
the socket (`/ws`), never from `dominion.config.json`** — the file is the intent and the socket is the
state. The reading itself is exactly the class this repo keeps getting bitten by (*a verdict is true at an
instant*), so **re-check it at the boundary rather than trusting this sentence**; what is durable is that
when auto-rotation is off, **only his click rotates this lane**, so a size boundary goes into `awaiting`
earlier than it otherwise would, and the machine will not cover a late one.
⚑ **A SECOND INSTRUMENT FROM THE SAME EXCHANGE, and it is a false-negative that reads as a clean result:
`ListAgents` UPTIME CANNOT SEE A REBOOT.** A Clear+Reboot keeps the session row, so every lane still
reported *"started 13h ago"* after five of them had been cleared. The hub was about to read that as
*nobody rebooted*. What saw it was `rotation_advice`'s `upMs`/working-now field. **Same family as the
no-fetch finding: an instrument answering confidently about a question it does not measure.**

## ⚑ FOUR OWNER RULINGS, 2026-08-22: **RELAYED, NOT WITNESSED BY THIS LANE**

Reached us via empyrean-73, quoting the owner's own words in their session. **Flagged as a relay
per this repo's own rule** (*never record an approval whose granting act you have not seen*), which
was written earlier the same day, after that shape failed twice across two lanes. Quoted words with
a named source are far stronger than a status field and are still not a witnessed act. **Direct
owner confirmation requested in-session; replace this flag with the confirmation, do not delete it.**

1. **Wiki-emulator spike: APPROVED, and my flagged divergence resolved, in the direction that
   corrects empyrean, not us.** The approval was **real all along**; the spec's self-declared
   "Approved design" was factually correct, and empyrean's correction to me was wrong on the fact.
   ⚠ **Keep both halves:** an unverifiable claim turning out true does **not** retroactively make
   recording it without a citation correct: *we got away with one.* Owner: *"Yes I did but I was
   trying to save fable use so I never had an agent start. I can now if it wants with opus but just
   be careful and if we get stuck don't push."* **Authorised on Opus, with two conditions in his own
   words**. A spike, not a commitment: **report the wall rather than engineering around it.**
   Escalate to empyrean rather than burning a week proving feasibility that was meant to be cheap.
   **Not reprioritised above the acceptance parcels.**
2. **Fable seat: HOLD, with a new obligation that is better than either option I offered.**
   ⚠ **CLOSED AS AN OWNER ITEM 2026-08-22. STOP LISTING IT AS PARKED.** Asked whether to fund the
   seat long-term he answered *"Idk what you want for this"*, and the asking lane recorded that as
   **their badly-formed question, not his indecision**: the right way round. **No decision is needed
   today: hold stands, the ledger IS the mechanism, and the question returns naturally when the limit
   lifts.** Note this also retires the provenance worry above by superseding it: there is now a live
   cited ruling on the seat, so the unwitnessed 2026-08-21 ratification is **correct and moot**.
   Owner:
   *"keep careful record of what's done without fable so when our limit is no longer up the first
   thing it can do is make sure we made the correct decisions without it."* **Fable's FIRST job when
   the limit lifts is auditing exactly those decisions**, so the gap becomes a queue rather than a
   hole. ▶ **Ledger created: `docs/2026-08-22-unadjudicated-decision-ledger.md`** (L-01…L-06). Each
   entry must be adjudicable **cold**: verdict, alternatives, evidence at the time, and *what would
   have to be true for it to be wrong.* An entry recording only the verdict is useless to the audit
   it exists for. **Every future unadjudicated call gets an entry at the moment it is made**, not
   reconstructed later. Note: this supersedes the unwitnessed 2026-08-21 ratification audited above:
   there is now a live cited ruling on the seat, so that correction stands as *correct and
   superseded*.
3. **▶ THE MOST CONSEQUENTIAL, and it is aimed at this lane.** Owner: *"Oracle - let's make sure
   anything not going for the new oracle does and tell it to make sure to tell the oracle agent to
   build out any tools these other suite items/agents might need, that's how we're getting robust."*
   Two halves: **(a)** anything still pointed at the legacy C++ server should be moving to the new
   core: **the acceptance contract is the vehicle and is effectively blessed as the priority**;
   **(b) this lane is the SUITE'S TOOL-BUILDER.** empyrean is telling every lane to send named
   instrument asks here rather than working around gaps. **Inbound capability asks are first-class
   queue items, not interruptions**: his stated reason is *"that's how we're getting robust"*. This
   extends the existing aeon co-development lane from one peer to all of them.
4. **READMEs: make every suite repo's README accurate.** *"Doesn't have to be super in depth."*
   Ours must say plainly that **the MCP surface still reaches the legacy C++ server**: the fact
   most likely to mislead a reader, and the one this lane independently flagged in the status
   roll-up before the directive arrived.

> ▶ **BOOTING INTO THE CUTOVER? READ `docs/2026-08-22-cutover-handoff.md` FIRST.** It is written for
> the session that boots *after* the owner flips the config and relaunches every lane: what the
> rebuilt binary at `12cc17e` guarantees, the 17 remaining, why a `-32601` is a success signal, and
> what to do first when lanes report gaps. This section is the record; that file is the instructions.

## ⚑ HUB RULING UNDER DELEGATION, 2026-08-27: d-16 SUBSTITUTE: the reviewer seat, and the rule it creates

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before dispatching an adjudication or recording one made while the seat is on hold.

## ⚑ THE CUTOVER: ruled 2026-08-22 (RELAYED, see the flag above), mechanism determined firsthand

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before writing any brief, and before treating an unserved-method gap as a failure.

## ⚑ THE SOCKET CHAIN, AND F-CHAIN-QUOTED

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before starting a server, citing a socket path to a peer, or reviewing how an unserved method fails.

## ▶ LAYER-MASK: LANDED. One safety property survives it, and HALF OF IT IS A CONVENTION

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before a parcel touches a masked render or `render_scanline`'s signature.

## ⚑ HUB RULING, 2026-09-02: HERMETIC GATE IS THE RATIFIED SHAPE; DRIFT IS A NIGHTLY, AND IT GETS **NO SECOND OWNER CARD**

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before picking up SCHEMA-DRIFT-NIGHTLY, F-CITATION-LINT or ATTR-RGB-LATCH, before filing an owner card, and when a hub relay hands you a go.

## ⚑ `run_to` vs `resume`: TWO LIVE RULES (2026-09-02; derivation in `OVERSEER-LOG.md`)

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before touching `run_to`, `resume`, or a client that waits on a halt event.

## ⚑ HUB RULING, 2026-09-07: HOW THE LENS COUNT IS REPORTED — **"PACKET MINUS FIXED", NEVER THE LEDGER'S OPEN COUNT**

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before reporting a lens count.

## ⚑ OWNER RULING, 2026-09-07: THE LENS RITUAL GAINS A UX SEAT PAIR, AND **THIS LANE IS THE PILOT**

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before chartering or running a UX lens.

## ⚑ OWNER RULING, 2026-09-03: WHAT GETS A TAB IN THE DEBUG WINDOW (ORACLE-DEBUG-UI)

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before a panel or debug-window parcel.

## ⚑ OWNER RULING, 2026-09-02T20:05:08Z, d-25 DOCK SHAPE: **option 3 `swap-toolkit`, NOT our recommendation**

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it before a panel or window parcel, before retiring `oracle-frontend`, and before renaming the player binary or its socket path.

## ⚠ Bootstrap: read the protocol at a COMMITTED revision (stays here on purpose)

*This stanza did not move to `docs/OVERSEER-REFERENCE.md` with the bars around it, and must not:
it is upstream of the boot read itself, and a rule filed in a file you open later cannot protect a
read that has already happened. The protocol's own preamble sanctions each repo carrying exactly
this one stanza.*

> ⚠ **READ THE PROTOCOL AT A COMMITTED REVISION, NOT THROUGH THE PATH** (seraph's rule, empyrean
> `origin/main`; the most upstream rule in that document): `../empyrean/docs/OVERSEER-PROTOCOL.md`
> is **one peer's live working tree**, so booting by path delivers the suite's shared contract by
> reading somebody's uncommitted directory. Use
> `git -C ../empyrean fetch -q origin && git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md`.
> Correct citation discipline applied to a bad source produces a **more** convincing artifact, not a
> less convincing one. **This session's own boot is the measured case**: it read the file by path
> while empyrean held sixteen unpushed commits, and got the right bytes only because their worktree
> happened to be clean at that minute; 59 lines landed in that path minutes later, and the file
> reached **422 lines by day's end against the 245-line snapshot the session booted on**. Right
> answer, by timing luck, with nothing in the output saying so.
>
> The commentary on protocol bars 8-15 moved to the log: they are read at boot from the protocol
> itself, at a committed revision, which is the only copy that cannot drift. The stanza above stays
> because the protocol's own bootstrap exception sanctions it.

## Coordination (when peers are up; all optional to progress)

Moved whole to docs/OVERSEER-REFERENCE.md (same heading); read it when a peer files a demand, and when seraph's S1 lands (it fires the VGM item).

## Where the detail lives

The dated `docs/2026-0[89]-*.md` files are the arc records (handoff/recon/CR/ruling per arc; newest
first is the reading order). The 08-* arcs end-to-end: scanline acceptance + convention
(`…-subline-*`), CR-25/26/27 with rulings, the profiler demand/recon/deltas, the Aurora client
demand, the streaming asks. `docs/2026-08-19-subline-shipped.md` is the model handoff shape.

**Three files, split by WHEN each is read, and a section is classified by its CONTENT, not its heading:**

* **`docs/OVERSEER.md`** (this file) is the boot read, bounded at 100,000 B. It holds scope, the queue,
  any resume brief, and the standing rulings that change what a session does FIRST.
* **`docs/OVERSEER-REFERENCE.md`** holds the bars and the ops lessons: not read at boot, opened
  before dispatching, before reviewing returned work, and before landing. Since the 2026-09-13 cut it also
  holds the follow-up register and the live rulings that moved out of this file; each keeps its heading here
  with one pointer line naming the moment to read it.
* **`docs/OVERSEER-LOG.md`** holds closed history, append-only, newest last: not read at boot, read by
  `tail`/`grep` when a particular night or a moved entry is in question. **A live ruling goes in
  `OVERSEER.md`, never only in the log.**
