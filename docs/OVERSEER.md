# OVERSEER.md — booting an oracle overseer session

> **Boot prompt (paste into a fresh session):**
> You are the oracle overseer. Read `docs/OVERSEER.md` in full, then the newest dated
> handoff/recon docs it names. You orchestrate subagents (dispatch → verify firsthand → merge);
> you do not implement directly. Work the queue top-down; keep this file current at merge windows.

Companion: the suite-wide protocol at `empyrean/docs/OVERSEER-PROTOCOL.md` (shared patterns; this
file is the oracle-specific half). Repo ground rules: the workspace `CLAUDE.md`. **Solo-first:**
everything below is workable with no peer sessions up — the queue, the follow-up register, and every
demand are committed artifacts in this repo; peers accelerate, they are never prerequisites.

## The role

Dispatch Opus subagents for implementation and recon; adjudicate contracts un-framed (a fresh
Fable agent, no steer — the cost was questioned and RATIFIED by the owner 2026-08-21: the ruling
is where one judgment becomes permanent contract text, so the smartest model sits there and
nowhere in the bulk work); verify every gate firsthand before accepting a slice; make the design
rulings (delegated by the owner — pick best, record why); merge and push. The owner's standing
directives: **a legacy surface or demand spec is the compatibility floor, never the design
ceiling** (run a visible better-approach pass on every request), and **instrument co-development
with aeon** is the ratified lane (their diagnoses name gaps; we build them; the engine gets fixed
with tools that then exist).

## The queue (2026-08-19 end of day — reorder only with cause, record the cause)

1. ~~Profiler slice 4 + merge window~~ **DONE 2026-08-19 late** — merged oracle `f7a8d54` /
   empyrean `5232574` (CR-26 + 3 deltas; the branch is now `main`; the repo folder is now
   `oracle/`, the legacy C++ one is `oracle-old/`). Rename fallout note: the compat symlink
   (`oracle-next` → `oracle`) protected consumers of THIS repo's paths, but consumers of the OLD
   repo's path (`oracle/linux-port/...` — aeon's 13 probe/gate tools, our MCP config) got a path
   that resolves into the WRONG repo, not a dead one; aeon hotfixed theirs to `oracle-old/`
   (aeon `17bcd111`), ours was updated at rename time. Lesson: renaming A→B while B's old name
   goes to C breaks C's consumers silently — cover BOTH sides of a swap. Completed taxonomy
   (round 2, aeon's oracle_gui font-path segfault): a swap breaks THREE consumer classes —
   live paths (compat symlink), old-name references (grep-and-fix), and compile-time-frozen
   paths (invisible until the binary runs; fix = reconfigure/rebuild in the new home, done
   2026-08-20 for oracle-old, verified via strings over the binary).
2. ~~CRAM handlers~~ **DONE 2026-08-20** — merged oracle `e8421f5` (CRAM pair served, params
   closure at the single dispatch choke, advertised 35→37 with the schematized-vs-advertised gap
   ZERO for the first time) / empyrean `e0467f7`+`d340205` (§11.17 + reload_rom postscript), both
   pushed. Controller-verified 48/1738/0/4, zero currency movement.
3. ~~Profiler slice 5~~ **DONE 2026-08-20** — merged in `018612a` (lens half `c0dab78`:
   `LensId::Profile` closes D15's fourth surface, `LensSet` u8→u16 per Q8; MCP tool rows shed
   three dead legacy claims and gain disp/perFrame). 48/1754/0/4.
4. ~~C1 witness + corpus A/B~~ **DONE 2026-08-20** — witness demonstrated (M1 red at predicted
   counts); A/B PASSED (`docs/2026-08-20-profiler-corpus-ab.md`: reference row to-the-cycle,
   spread exactly 0, their dense anomaly explained as their instrument's straddle loss, K.3's
   21.55-point dropped-work finding, Task 5 measurable). Migration flip is aeon's call (K.4).
   Open residue: the five-short-rows 11–40% disagreement (unexplained, settling experiment in
   the doc §11.5); the walker fixture leg (not re-driven).
5. ~~CR-28~~ **DONE 2026-08-21, full arc in one day** — shape check → aeon's answers anchored both
   sides (their `6edb08ef`+`ff01881f`, ours `docs/2026-08-21-cr28-shape-answer.md`) → draft →
   un-framed Fable adjudication (ADOPT WITH CHANGES, 7 M / 4 S; `docs/2026-08-21-ruling-cr28.md`)
   → applied → served. Merged: oracle (docs + `cr28-serve`; aggregate 48/1770/0/4, +16, zero
   currency movement, 15 recorded mutations) / empyrean `70c7bb4` (§11.18 + the amended §11.16
   two-shapes bound + the §2.4 flat-spelling rule; 37 fragments unmoved, closure 0 open, both
   re-derived at the merge) / oracle-old `d629771` (MCP tool rows). `entryKind` is
   `"hint"|"vint"|"root"|"depthCap"` — the consumer's literal spelling adjudicated over, accepted
   by them as exceeding their floor. TRACKED_REVISION retired to None (`d95bf59`).

6. ~~§11.5 short-routine residual~~ **DONE 2026-08-22, merged `d778dec`+`c6a1ac6`** —
   `docs/2026-08-22-shortrow-residual-measurement.md` (`fff9cc2`+`c942864`) and
   `docs/2026-08-22-cycle-attribution-audit.md` (`b4a78c9`). **Whole arc is docs-only** (zero
   non-docs files across `a27e4d2..c6a1ac6`, verified by `git diff --name-only`), so the aggregate
   and currency are unmoved *by construction* — no cargo run, recorded as reasoning rather than
   skipped silently (aurora held a cargo lane at the time).
   **VERDICT: three parts, two closed, one characterised — and NONE of it is on our side.**
   - **Stage B decided it with a third party neither emulator authored.** Hand-derived from ROM
     bytes (capstone, an independent decoder) + Yacht v1.1, on a bracket pinned from OUR source
     *before* counting: `Palette_Compose` = **exactly 180**, `BgAnim_Update` = **exactly 154**. Ours
     reads 180.0/154.0; theirs ~150/~187. **Matches ours on both, theirs on neither.** Free third
     confirmation from `run_to` clock timestamps: the two entries sit 188 cycles apart = 154 +
     `rts`(16) + `bsr.w`(18), touching no profiler figure. **This — not the walker — is now the
     evidence that the two instruments share nominal timings** (see §9.5 below).
   - **Part A CLOSED (H1 SUPPORTED, ~half the spread).** The denominator artifact. Purest
     demonstration: `Raster_VBlank` and `Enqueue_Dirty_Buffers` are **+101%** and **+95%** wrong at
     maxdiag *by pure construction* — frame-driven routines scaled by 2.067 anyway — and the corpus
     A/B's §6.2 **silently omitted both**.
   - **Part B CLOSED — a conserved transfer across ONE call boundary.** `BgAnim_Update` (+991) is
     matched by `Parallax_Update` (−1039); they are called back-to-back with **one instruction
     between them** (`jsr $941C.l` at `$0A194A`). Conservation independently **derives N = 29.998**
     against a `Logic_Tick` ground truth of 30, **with no free parameter** — that is what keeps it
     from being a coincidence fitted after the fact. **Two agents converged from opposite
     directions with no contact**: one reading their C++ (a shadow stack popping by position, never
     comparing addresses), one measuring and never reading their source.
   - **Part C OPEN, far better characterised.** Four rows low by **6×–336×** the W4 straddle
     ceiling (`Palette_Compose` maxdiag 336×; `Tile_Cache_Fill` idle −39.9% vs a 3.1% ceiling).
     Sign-consistent with a positional-pop defect; **magnitude unpinned.**
   - **A hard bound worth keeping**: their idle `BgAnim_Update` is **arithmetically impossible**,
     not merely different — 30 ticks × 154 caps it at 4620 and they publish ≥5611, implying
     36.3–36.5 invocations of a routine that ran 30 times. Holds under either rounding convention.
   **Hypothesis verdicts:** H1 SUPPORTED (~half). **H2 REJECTED by an INVERTED partition** — a
   complete VDP-port census (2050 hits, 0 dropped) puts the heaviest port-toucher
   (`HBlank_Vector_Slot`, 384 writes) at the **most exact agreement in the set, 0.000%**, while all
   five §11.5 rows **write to no port at all**: exactly backwards from H2's prediction. **H3
   REJECTED and our bracket independently VALIDATED** (bounded ±36 cyc/inv; no fixed convention can
   put BgAnim at +34 and Palette at −30 at idle and both at ~0 at maxdiag). **My own relayed
   flush-route hypothesis REFUTED on magnitude** — BgAnim enters 19.3% into the frame with 103,286
   cycles still to run, so a flush would deliver ~103,000 where 34 was measured: **wrong by
   ~3,000×**. Good outcome; the brief asked for falsification and got it.
   **W2 is UNTESTED, not dead** — the §6.3 rejection asked which routines *contain* a `4EF9`, but a
   desync is not local (it mis-pairs every subsequent Exit in the frame), so it was the wrong
   partition. Fuel confirmed present: live call chain across their seam every frame, static `RTR` 7,
   `TRAP` 78, `JMP` 347.
   **Two overseer catches, both material** (recorded because firsthand verification earned its cost
   twice in one arc): (a) **the rounding band was the wrong convention** — the agent carried their
   published integers as truncated; their probe **rounds to nearest**, proven **26/26 vs 15/26 with
   11 discriminating rows, 0 for TRUNC** (re-derived independently by me *and* by them over all 26
   published row-states). This **deleted the headline "the pair conserves, the interval contains
   zero"**: the pair is **short by 17–79 cycles**. It also **unified two loose ends into one** — that
   shortfall IS the separately-flagged "2–3 cycles above the measured transfer" gap, ×30
   invocations. One open quantity measured two ways, ~1–3 cyc/inv, **unexplained and not absorbed**;
   lead recorded without claiming it (both sides straddle `rts`+`jsr (xxx).W` = 34 without either
   containing it). (b) The **`calls`-fingerprint** idea I sent is **unavailable in the published
   form** — their column only reaches `2` at ≥62 raw calls, so `1` bounds it at ≤61 and carries no
   information. Also self-corrected by the agent: "nine of fourteen" was stale **and** mis-counted
   (true figure **13 of 28 cells**), and `EntityWindow_Scan` idle flipped marginally **outside** ±1
   invocation (≈30 invocations + ~62 cycles; 60× smaller than any Part C row, changes no conclusion,
   but no longer clean window phase).
   **Artifacts relocated out of the dying worktree** to `/home/volence/sonic_hacks/corpus-rom-d22dda85/`
   (plain dir, outside any git tree): `s4.debug.bin` (`d22dda85`/713295, sha256 `ad289eae947b2dd4`),
   `s4.debug.lst` (5162 lines / 2578 symbols), and `PROVENANCE.md` carrying the byte-identity
   binding proof, the rebuild recipe, **both snags aeon will hit**, and every address the doc names.
   All four re-verified firsthand here. **Live ask out to aeon:** raw `calls` for `Parallax_Update`
   and `BgAnim_Update` at idle — it would confirm Part B **from their own instrument**.
   **§9 of the doc lists six proposed edits to the corpus A/B**, incl. §9.5: the *"the walker agrees
   to 0.17%"* line **cannot** carry the weight the A/B put on it (it is Part B's other half, hidden
   by a 20,000-cycle divisor); what can: Stage B's hand derivations, and the display-driven
   `HBlank_Vector_Slot` row at 1878-vs-1878, which has no lag denominator to hide anything in.
   **Stage C (paired trace) NOT taken and not needed for parts A/B**; §8.1 specifies it concretely
   enough to dispatch if Part C is ever pursued.

   *(Item 6's dispatch record follows, kept for the method — how the arc was framed before any
   result was in. Read it as the setup, not as open work.)* The one
   unexplained item left by the profiler corpus A/B: five ungated short rows (`Tile_Cache_Fill`
   idle, `EntityWindow_Scan` maxdiag, `Section_UpdateColumns`, `Palette_Compose`, `BgAnim_Update`)
   disagree 11–40% with aeon's instrument at W4 straddle exposure under 4%; W2 and state-phase
   already tested and rejected. Two agents dispatched in parallel off `a27e4d2`:
   `profiler-shortrow-residual` (measurement + an instrument-independent hand-derivation of the
   two constant rows) and `profiler-attribution-audit` (source-side: what each instrument's cycle
   figure actually *includes*, both codebases).
   **The registered settling experiment was deliberately NOT taken as written.** §11.5 says "a
   paired event-level trace, which needs the reference running"; two cheaper and stronger
   experiments come first — (a) partition count-vs-cost against the corpus's own four-column
   tables, since their `cyc/logic-tick` is **derived** (`cyc/video-frame × frames-per-tick`), not
   measured, and their `calls` is per-video-frame integer, so ours and theirs may not be the same
   quantity at all; (b) hand-derive true 68000 cycles for `Palette_Compose` (ours exactly 180.0)
   and `BgAnim_Update` (ours exactly 154.0) from the listing + `docs/reference/` tables — a third
   party neither emulator authored. Paired trace is held as stage C, dispatchable only if these
   leave it open. Named hypotheses under falsification: H1 denominator/lag-scaling artifact, H2
   cycle-attribution/port-cost asymmetry (their caveat 0 is ideal-cycles-only; our `stallCycles:0`
   may mean "no DMA halt", not "no stall"), H3 bracket convention (JSR 20 / RTS 16 on a 154-cycle
   routine is up to 23%).
   **Sharpest lead, called out by name in both briefs:** `BgAnim_Update` agrees at maxdiag (theirs
   153 / ours 154) and disagrees 21% at idle (theirs 187 / ours 154) for a routine that is a hard
   constant on ours at both states — and theirs reads HIGH there while the other four read LOW, so
   no lose-only mechanism covers the class. Control is `Parallax_Update` at 0.17% over 20k cycles.
   **Setup cost collapsed at dispatch time:** the corpus ROM needs no toolchain rebuild — sigil's
   committed golden at `7b46f075` (`crates/sigil-harness/golden/s4.debug.bin`, blob `633f5f88`) is
   byte-identical to the corpus pin `d22dda85`/713295, verified firsthand here. Only the `.lst`
   still needs the §1.3 pinned-toolchain recipe, and it self-checks: a rebuild that reproduces
   those bytes proves the listing beside it is the corpus listing. ⚠ The **live**
   `aeon/s4.debug.{bin,lst}` are a different ROM (`f8a1c567`/715010) — the server's binding check
   refuses the mismatched pair, and that refusal is a safety net, never to be defeated.

   **H1 CONFIRMED same morning by the corpus's own author** (aeon overseer, arithmetic against
   their own table, five routines × both states): their `cyc/logic-tick` is derived
   (`cyc/video-frame × frames-per-tick`, rounded) — *"treat it as a reconstruction, not a
   measurement"* — and their `calls` column is per-video-frame integer, so for a **tick-driven**
   routine at non-integer frames-per-tick it cannot represent the true rate (idle true rate 0.968,
   column reads 1; maxdiag 0.484, column still reads 1). Their per-frame figure averages over
   frames the routine never ran in; ours is per-invocation. **Commensurability is therefore a
   per-row property** — tick-driven vs frame-driven, determined per routine, never assumed.
   **But H1 does NOT cover the class, and their own data is what kills it:** BgAnim is tick-driven,
   so 154/2.067 = 74.5 vs their measured 74 — the scaling round-trips, and H1 explains the maxdiag
   *agreement*, not a disagreement; while 154/1.033 = 149.1 vs their **measured** 181 is 21% high
   **before any derivation touches it**. The artifact is downstream of the idle divergence.
   **Class split deliberately into four low rows + BgAnim-at-idle** (the only theirs-HIGH row,
   needing its own mechanism) — two honestly-bounded mechanisms beat one stretched over five. Mind
   the sign: theirs-HIGH is the opposite of what the naive attribution story predicts, so the
   sign-correct candidate is their W4 shadow-stack re-init attributing a foreign entry's cycles to
   a victim (adds to a victim rather than losing) — under test, not assumed. **Stage B
   (hand-derivation) is now the arc's centre**: aeon has stated in writing they will **retract**
   the affected numbers rather than defend them if it rules against them — the number sits in a doc
   other parcels have quoted — and the same prominence is promised here if it rules against our
   180.0/154.0. An adjudicator neither side authored is only worth using if it may rule against you.

   **Stage C: reachable but not free.** aeon hit `ModuleNotFoundError: No module named 'launcher'`
   rebuilding at `bc048e2a` — the **identical** failure §1.3 already root-caused, refined by them
   to `bc048e2a:tools/raster_cost_probe.py:55` hardcoding the PRE-rename
   `oracle/linux-port/harness`; **current aeon master is already fine** (points at
   `oracle-old/...`), so this is vintage-tree archaeology, not a live defect in their tooling.
   Resolution `--no-lint` (the lint stage emits no bytes), with `[map.undeclared-island] at 0x99F0`
   waiting behind it — **both halves of the blocker are the one pinned revision `7b46f075`**. They
   stood down from a parallel sigil build; our agent's listing + binding proof gets handed to them.

   ⚠ **Peer-claim correction — a bar-paying instance, logged because it nearly cost a parcel.**
   aeon reported no committed `d22dda85` blob existed, having checked sigil's golden at **master**
   (`0dbaa80f`/715010): a correct measurement of the opposite fact. **A golden path is a MOVING
   POINTER** — `master:<path>` answers "what is the golden *now*", never "does this artifact exist
   in history", and the tip is the one revision guaranteed not to hold a vintage artifact. Find it
   at the revision that PINNED it, named in the refreeze commit that paired them (`7b46f075`), and
   confirm with `git merge-base --is-ancestor`. They verified by hashing the extracted bytes and
   corrected their tree at `77caeefd` rather than dropping it quietly. Relayed to empyrean as a
   proposed **amendment to `9b604f0`** (not a separate bar — without it the rule reads as satisfied
   by exactly the check that failed). Composes with `43fbfc9` from the other side: that one says a
   SHA has a class, this one says a **path has a time**; both failures look like a competent lookup
   returning a clean answer.

**~~⏸ CR-28 implementation paused~~ RESOLVED same day** — the same agent was resumed after the
7pm reset with context and worktree intact (resume path A worked as written) and finished clean.
Kept for the pattern: *(original note follows)* the implementer died on the API limit after committing 2 of ~5 stages on branch
`cr28-serve`: `07eb724` (schema re-vendor from empyrean `callers-amendment` `7c4b9fc`) +
`b096370` (core accumulator: second map keyed (callee, caller)). Uncommitted `engine.rs` WIP
(the Aether surface, mid-edit — last words "profiler_row and the edge emitter") lives in its
worktree `.claude/worktrees/agent-ab2d0e3a4885815cb` — **do not prune that worktree**. Resume
path A (preferred, this session only): SendMessage the same agent after the reset — it keeps
its context and its worktree. Resume path B (any session): commit the worktree's engine.rs
diff as WIP on `cr28-serve`, then fresh-dispatch from the branch + the CR (`cr28-callers`
`22d57ca`) + ruling (`ruling-cr28` `52ddf03`) + amendment (empyrean `7c4b9fc`). Contract side
is FINAL and fully committed — only server code remains. Then: merge window (code + amendment
+ CR + ruling, both repos, one window), ship notice to aeon.

~~**Incoming (registered 2026-08-21, no action until triggered):** aeon's CR-28 **consumption
verdict**~~ **RECEIVED 2026-08-21 late, POSITIVE — CR-28 arc fully closed.** Anchor verified
firsthand: aeon master `25ef878c`, `docs/benchmarks/streaming/STAGING-LIFETIME.md` §6. Their
probe rebuilt oracle-aether at our `f476785`, armed `set_profiler{callers:true}` +
`get_profiler_frames{topCallers:8}`; the caller lens reproduced their independent slot-ledger
**exactly** — S4LZ_DecompressDict 1/1 caller with cyclesSelfTotal 115,604 == the three bursts
to the cycle; DecompressBlock 15+24 == the 39-claim ledger; at `right` it names the speculative
claim site (Tile_Cache_Fill 15/15) with no geometry inference. Both normative `==` sums held on
live data. PHASE-0 corpus control passed unchanged under pre- and post-CR-28 binaries. **Zero
wire surprises, zero asks.** Their residual (per-claim block/slot/eviction detail still needs
their RAM snapshots — the instruments compose, don't overlap) is an observation, not a request.
depthCap stays their booked caveat, follow-up explicitly NOT requested.
F-TICK-BOUNDARY-DIVERGENCE ping stands on the joint ledger (already in the register below).
Also: the wiki-emulator PoC — empyrean's
spec `adfb0f1` (`docs/superpowers/specs/2026-08-19-wiki-emulator-poc-design.md`, updated `04c35cb`)
proposes a thin `oracle-wasm` crate over an UNTOUCHED `oracle-core` plus a pad-reactive fixture ROM.
Awaiting the owner's review on empyrean's side; the implementation dispatch arrives from the
empyrean overseer when approved. Also: the session-rotation protocol rule (empyrean `ae9e4ef`)
rides their next push.

**Follow-up register** (each named where registered; deferrals here are unaudited estimates —
measured 3-for-3 cheaper than documented): F-SCANLINE-INDEX / F-SCANLINE-SH (priced down by the
sub-line arc), F-CRAMDOT, F-SUBLINE-{HGRID, ACCESSMCLK, DMASPREAD, CAPTURE-SCRATCH}, F-VCOUNT-PHASE,
the a2 B-2 gate gap (needs an H40/mode-switch fixture), F-HOSTED-RESET-SRM (**hosted reset bypasses
the player's .srm flush — warn clients off hosted reset until closed**), F-EQUATES-NAMESPACE,
F-CRAM-RAMP, F-PROF-TOTALS (superseded by delta 3), F-PALETTE-DRAG-PACE (evidence filed, rated
minor by its own filer), ~~stock-S1 symbols~~ (**CLOSED 2026-08-20** — the `|`-reader, the 48-bit
addresses, the forward-only equ ruling and the no-appendix binding all shipped; F-LST-AS-COLUMNS and
F-LST-NONDEB2-BINDING retire with it), **F-TICK-BOUNDARY-DIVERGENCE** (2026-08-20, from aeon's spike hunt, TICK-VARIANCE.md): over one
31-frame max-diagonal window on byte-identical ROM bytes, oracle-old runs 26 logic ticks where we
run 29 — exact agreement at the corpus-era state, one-tick difference at idle, divergence only
where a tick sits near the frame boundary: the two emulators disagree how much work fits in a
frame. Settling experiment (theirs): a single-tick trace at the first divergent boundary (frames
~7-8; states in TICK-VARIANCE §1.2) on both instruments. Corroborating fossil: the 2026-07-23
RT-3 finding — oracle-old OVER-drops ~8 startup ticks via `ClampHandshakeTimeDeterministic`'s
over-conservative bus-arb clamp, and ours was the tick-accurate side then too. Unresolved, not
urgent, CR-28-era sweep candidate. plus the Tier-1 carry-forwards in
`docs/2026-08-18-tier1-bus-methods.md`.

**Registered by the CR-27 serve review (2026-08-20), all contract-side or cosmetic, none blocking:**

| id | what | revival condition |
|---|---|---|
| **F-PLAYINPUT-ITEMS-OPEN** | `emulator/play_input`'s `rows[]` **items** are not closed — §8 item 20's closure is applied at the result's top level, so a surplus key *inside* a row passes. The one array-of-objects on the bus whose item shape is unguarded; `read_cram`'s `palette[]` items and `watchpoint_hits`' `hits[]` items both close theirs. | Contract-side: an `additionalProperties: false` on the item subschema, next time `play_input`'s fragment is opened. No server change. |
| **F-ONEOF-COMBINATIONS** | The `oneOf`/`dependentRequired` alternations (`write_cram`'s triple-vs-`raw`, `write_memory`'s `bytes`-vs-`value`+`width`) are enforced per-fragment but nothing sweeps **every combination** of a fragment's declared keys against its own rules. Today each is hand-tested; the sweep would be mechanical and would cover the ones nobody thought to write. | A third alternation lands, or a hand-written combination test is found wrong. |
| **F-REVIEW-N12**, **F-REVIEW-N14** | Recorded by id from the CR-27 serve review; their content is in the review itself, not restated here — the implementing agent was given the ids without the text and is not inventing a description for them. | Whoever holds the review restates them; then they get a real row. |

*(N9 — the `Resolution::name`/`Display` duplication — was **taken**, not registered: `Display` now calls
`name()`. It was two lines.)*

## The bars (house methods — each earned by a measured failure; do not thin)

> The shared protocol gained review bars 8–10 and two SHA-citation rules on 2026-08-22 (empyrean
> `dc629a5`, `c2c81e2`, `00334b6`, `43fbfc9`, `9b604f0`+`e650b96`+`aadf63f`). **Not transcribed here — read them there**; the
> protocol is changed in empyrean and never forked into a repo copy. Two bear directly on this
> lane: **bar 9** (never change the subject to suit the instrument — an instrumented run reported
> as the uninstrumented number is the named failure) governs every profiler measurement we take,
> and **bar 8** (enumerate by what TOUCHES the data, not what defines it) governs state/snapshot
> field sweeps. Both were written into the 2026-08-22 §11.5 dispatches. `9b604f0` — *prefer the
> committed artifact to the recipe that recreates it, and verify by hashing the extracted bytes* —
> came OUT of this lane the same morning (the corpus-ROM find below is its precedent): a recovery
> recipe carried in prose is a standing claim that it still reproduces the artifact, and nobody
> re-tests that claim until it fails silently. Its `e650b96` amendment (a path has a TIME — find a
> vintage artifact at the revision that pinned it) came out of this lane too, and `aadf63f`
> (aurora's correction) scopes it: that clause governs **recovery** — reaching a known artifact —
> while a **currency** check ("has the contract moved?") must run at TIP or it is vacuous, because
> a pinned blob equals itself by construction. Both operations look identical at the call site:
> **name the question before choosing the revision.**
>
> **Our currency check, run at tip 2026-08-22** (the operation `aadf63f` names): the vendored
> `crates/oracle-aether/tests/contract/bus-protocol.schema.json` is `f038672daf6eb2b8`, and
> empyrean `origin/main` `aadf63f`:`contract/schema/bus-protocol.schema.json` is the same
> `f038672daf6eb2b8` — byte-identical, with **zero commits touching that path** since our CR-28
> vendor at `70c7bb4`. The day's seven empyrean commits are all `OVERSEER-PROTOCOL.md`, not the bus
> contract, which is why. TRACKED_REVISION stays retired at None.

- **Contract-first, always**: CR → un-framed adjudication → apply fixes → the code and its
  amendment merge in one window so `protocol.md` never describes a server that does not exist.
  Post-adjudication changes ride **deltas** (same adjudicator, same standard). Adjudication is not
  optional even for your own rulings — a ruling authorizes the change; adjudication is what
  authorizes the *text*.
- **Verify firsthand before accepting**: run fmt + clippy ×2 + the full aggregate yourself
  (`cargo test --workspace 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6; i+=$8; n+=1} END
  {print "LEGS="n" PASSED="p" FAILED="f" IGNORED="i}'`). Agent reports have matched every time —
  verify anyway; the one time they don't is the point.
- **Serialized cargo**: NEVER two cargo runs anywhere in this repo at once — including
  isolated-worktree runs while any agent runs cargo (measured: legs truncate with a spurious
  failure, three data points). Queue acceptance gates; verify BEFORE resuming an implementer.
  Short release builds for an owner-facing unblock are the recorded exception (nice -19, logged).
- **Mutation discipline**: every evidence-bearing test carries a recorded mutation (edit → touch →
  observe "Compiling" → named FAIL → revert → green; cargo's fingerprint is MTIME-based). A
  mutation that catches nothing is strengthened BEFORE recording, never recorded hollow. When an
  expectation and the code disagree, investigate to ground truth — three times today the code was
  right and the expectation wrong.
- **Currency scrutiny**: goldens never regenerate silently; every mover carries a named, measured
  mechanism in its `cause:` comment; any unexplained mover is a STOP-and-report, not a re-pin.
  Zero-file-diff on `crates/oracle-core/tests/` is the default expectation for bus work; breaking
  it is a named decision.
- **Demands are committed artifacts**: transcribed from the consumer's own source with anchors
  (never from a relay — relays get flagged as such until an anchor lands), corrections recorded
  supersession-style (original visible, correction over it), gap triage into
  satisfied / composable-today / genuinely-new.
- **Dedicated adversarial review** for load-bearing slices (the slice that carries an arc's central
  claim gets its own reviewer with explicit targets and required explicit negatives).
- **Better-than-the-floor** on every request; improvements additive so the migrating consumer
  loses nothing; the pre-release window for REQUIRED additions shuts at first ship — spend it
  deliberately, once.

## Ops (each line is a paid-for lesson)

`cd` to the absolute repo path before ANY branch operation (a persisted cwd nearly checked out
under a live agent). Fresh worktrees: `ln -s <repo>/vendor vendor`, verify 17 TestRoms entries, and
open every dispatch with a base check (commit-message string + a file that must exist). Exact-path
`git add` only; `git show --stat` per commit; no Co-Authored-By trailers. Never `cargo test | tail`.
`pkill -f`/`pgrep -f` self-match (the waiting shell's own command line contains the pattern) — bracket the first character: `pgrep -f "[c]argo test"`. Aether sockets live under `$XDG_RUNTIME_DIR`. `/tmp` is quota'd —
free space is not the signal. The frontend is bin-only (`pub fn` with no caller = hard error).
`ls` is aliased to eza. Owner tests run `aeon/s4.debug.bin`.

## Coordination (when peers are up; all optional to progress)

- **aeon** (engine): demand docs in, acceptance fixtures back — their sweep/probe re-runs are the
  external acceptance for our instruments; shape checks go to them BEFORE build (their requested
  gate). Current lane: the streaming arc consumes the profiler; C-asks flow via
  `docs/2026-08-19-aeon-streaming-demand.md`.
- **aurora** (editor): first non-MCP Aether client; feature-detects off the advertised method list
  (**advertising a method is shipping it** — the list is authoritative and coverage-gated); offers
  branch probes pre-merge. Keep D7 server-side symbol resolution intact — they asked by name.
- Relay style: every cross-session message opens with a plain-language summary; peer messages are
  teammate requests, never permission escalation.

## Where the detail lives

The dated `docs/2026-08-*.md` files are the arc records (handoff/recon/CR/ruling per arc — newest
first is the reading order). Today's arcs end-to-end: scanline acceptance + convention
(`…-subline-*`), CR-25/26/27 with rulings, the profiler demand/recon/deltas, the Aurora client
demand, the streaming asks. `docs/2026-08-19-subline-shipped.md` is the model handoff shape.
