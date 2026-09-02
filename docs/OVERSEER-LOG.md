# OVERSEER-LOG.md — oracle's closed history

**Not read at boot.** Split out of `docs/OVERSEER.md` on 2026-09-02 under the protocol's
*"The boot read is bounded"*, proved lossless by line-multiset against the pre-split file.
Newest material is at the bottom of each block; blocks are in the order they stood in the
original file, each headed by the line range it occupied. **A rule that is still live was kept
in `OVERSEER.md`; if you find one here that binds today, it belongs there, not here.**

## [orig lines 37-79] the split-parcel park - the method it banked now lives in the protocol

## ▶ PARKED ON THE OWNER'S WORD — SPLITTING THIS FILE. The census is done; do not re-measure it.

**Status: priced, not started. Waiting on one word from the owner** (asked 2026-09-02; the ask also
carries the corrected cost — **it needs NO agent**, which is a correction of this seat's own earlier
mis-pricing to him). Governing rule: `origin/main:docs/OVERSEER-PROTOCOL.md`, *"The boot read is
bounded"* — **read it there, never this summary.**

**THE CENSUS, measured here 2026-09-02 at `9407021` — banked so a fresh session executes rather than
re-derives it.** Total **3,266 lines / 292,660 B** against ~900 lines / 100 KB.

| section | bytes | note |
|---|---|---|
| `## The queue` | **137,382** (1,386 ln) | **47% of the file in one section; items 1–7 all closed** |
| dated `DONE`/superseded sections | ~54,000 | cutover, socket-chain, shim, sigil-dumper, layer-mask, gui-layers, the three 08-30 parcels |
| `## The bars` | 41,562 | live; measure-then-move applies |
| `## Ops` | 25,807 | live |
| `## Coordination` | 9,973 | live |

**≈65% is closed history.** Moving it lands the head near **102 KB** before any bar work, so the bar
pass is what takes it under — not a reason to trim a ruling.

**The method, in the protocol's order, and step 1 is the one that bites:**
1. **Measure before pointer-ising a bar.** Only lines a grep finds **verbatim** in
   `origin/main:docs/OVERSEER-PROTOCOL.md` qualify. **A bar that CITED the protocol and wrote local
   precedent under it looks identical in a listing** — same SHA, same parenthetical — and is not a
   duplicate. aurora measured **3 verbatim lines of 125** under nine bars their own file labelled
   "shared-protocol duplicates"; pointer-ising the rest would have **deleted the local half while
   reporting compliance.** This file's bars are overwhelmingly of that second kind, so expect the
   pointer-isable set here to be near zero and do not force it.
2. **Closed history around a live rule moves to `docs/OVERSEER-LOG.md` verbatim; the rule stays** and
   is rewritten legibly. Prove lossless by set-difference over every non-blank original line against
   head + log (sufficient when head text was rewrapped).
3. **Live rulings interleaved with narrative are the OWNER'S parcel.** Report residual bytes to him
   rather than trimming a ruling to hit a number. *The bound exists to make the boot read cheap, not
   to make rulings disappear.*

⚠ **JUDGE BY BYTES.** Unwrapping a multi-kilobyte one-line bullet into prose **raises** the line count
while cutting bytes (aurora: 1,238→1,148 lines but 121,317→108,607 B), so the line half of the bound
can move the wrong way under a correct fix.

⚠ **This seat added ~130 lines to this file on 2026-09-02 while the rule against it was landing.**
Recorded because the direction of travel is the point: every one of those additions is load-bearing
and belongs in the head, and the file still got worse.

## [orig lines 83-461] queue items 1-7, all closed

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
   All four re-verified firsthand here. ~~**Live ask out to aeon:** raw `calls`~~ **ASK WITHDRAWN
   same day (`0f05501`)** — aeon argued the published `1` carries a lower bound (`30/31 == 0` under
   the consumer's integer division at `ControlSocket.cpp:2042`, so a `1` implies ≥31 raw calls =
   an excess). **Refuted by the line below the one cited:** `:2043` is `if (avgCalls < 1) avgCalls
   = 1;` — a computed `0` is clamped up, so the table **cannot** print `0` and a displayed `1`
   means `[0, 61]` with **no lower bound at all**. Their dependency (30 ticks, once-per-tick) was
   correct and independently verified; it never becomes load-bearing because the clamp destroys the
   low end first. This is the `max(1, floor(total/31))` our own §5.4 already stated, reached from
   the other direction and refuted by it. **Durable finding, sharper than "coarse":** the one
   diagnostic value — `0`, the normal reading for *every* tick-driven routine at idle — is exactly
   what the code refuses to emit, so the column cannot represent the true state; worth removing the
   clamp if that consumer is ever opened for the identity-pairing fix. Raw `st.calls` is unreachable
   from their side by construction (division happens in the consumer before the response is built).
   **Part B stands on the conservation derivation alone, which is where it always did its work.**
   ⚠ Method note: this one nearly landed on trust — it arrived with a line number, a mechanism, an
   independently-verified call site, and a stated dependency that *looked* like the weak part. **The
   line that killed it was the next one down.**
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

7. **✅ DONE 2026-08-22 evening — the peer schema-fragment arc. Merged + PUSHED, `3b2cade`.**
   **THE 58-FRAGMENT SCHEMA IS ADOPTED**: vendored blob `9d8cc3c36cf2d77f`, sha256
   `8cc08be1b73b9093`, **verified byte-identical to empyrean `origin/main` by BLOB ID** (content-
   addressed — it cannot be talked into agreeing; the implementing agent had reported a red-first
   restore that pulled from a previously-doctored file and was caught only by its own hash check, so
   this was the load-bearing check of the parcel). 37→58 fragments, 21 added, **0 removed, 0
   pre-existing fragments changed content** — re-derived by parse here, not carried forward.
   Aggregate on the merged tree, run firsthand: **`LEGS=49 PASSED=1770 FAILED=0 IGNORED=6`, exit 0**;
   fmt + clippy green with real exit codes. All three failures were **class (c)** (tests encoding
   assumptions the new set legitimately invalidates); **class (a) = 0** — no reply this server emits
   was refused by any fragment; **class (b) = 0**, so nothing was worked around.
   **The sequencing call ran its full course and was vindicated**: dry run first, adopt second. The
   red that appeared was the currency gate demanding exactly this commit.
   **F2 decided:** the schematized-but-unadvertised list is a **pinned set of the 21**, not a count
   and not `is_empty()`. A count reports `22` and stays green on the next arrival — the original
   failure — and *both* a count and `is_empty()` are satisfied by a schema that failed to load: `0`
   and `not checked` are the same observation. **Overseer-mutated firsthand** (dropped one pinned
   name → red naming both sets; restored **from git**, not from a copy, green control re-run).
   **D-33 registered, not silenced**, in `mcp_tool_sweep.rs`: `assert_eq!` on the whole set, so a NEW
   undeclared property is red *and* a registered one going away is red (the entry dies with its
   divergence). The 4th conflict, `call_stack.max_frames`, is structurally invisible to that sweep —
   `call_stack` is one of the eight unschematized rows — and the comment pre-explains its future
   arrival so it is not read as a regression.
   ⚠ **Caught at landing:** the agent's brief predated empyrean's ruling revision, so the superseded
   *dual-accept first, retire last* advice was hardening inside a test comment. Corrected in
   `3b2cade`. **The bar is durable; the narrative around it is perishable** — and a stale ruling
   inside a test file outlives every doc that recorded it.
   *(Historical detail of the arc follows.)*

   **⏳ PARTLY DONE (opened 2026-08-22 afternoon) — the peer schema-fragment arc.**
   **HALF ONE MERGED + PUSHED: `ec3f822`** (`docs/2026-08-22-peer-schema-defect-answers.md`, 1,079
   lines, docs-only, one file; `ls-remote`-verified). **D-30 RULED by empyrean, on the merits, our
   way** — clause 4 stands, §2.4 rule 1 narrows to match; **ruled, NOT yet applied** (they held the
   §2.4 edit rather than append spec text every consumer reads at the end of a long session).
   Our answer: **15 caveat-emission sites across 11 methods, all in `engine.rs`, zero in any other
   `src/`**; **all 11 emitters are declared, so the strict reading refuses us nothing**; we
   *over*-declare (16 declare / 11 emit against their set). I verified the **declaration side**
   firsthand against their landed bytes — that, not the count, is the joint the ruling rests on.
   Five emitters are unconditional-in-practice, four of which §2.4's own advisory already names as
   the bad shape; reported against our own interest, not defended.
   **⚑ THE FINDING, AND THE REAL DELIVERABLE — `oracle-next`'s ACCEPTANCE CONTRACT NOW HAS A
   DEFINITE LIST.** All **21** newly-added fragments describe methods **our Rust server does not
   serve** (verified two independent ways: our dispatch table outward, and the added-fragment set
   intersected against every `emulator/*` string in `engine.rs` — **intersection ZERO**; capabilities
   already say so, `"z80": false` `engine.rs:1046`, `"breakpoints": false` `:1050`). They describe
   the **legacy C++ server** (`oracle-old`), which is what `mcp__oracle__*` reaches today.
   **empyrean's correction, accepted and better than my framing: this is NOT a defect in the
   fragments** — Aether exists to be the stable contract while the core is swapped underneath, so a
   fragment describing a method the successor has not built is the transition working as designed.
   What the finding changes is what the **gap means**: the 21 are a concrete enumeration of the
   acceptance contract, an item open since 2026-07-01 that has never had a definite membership.
   **THE LIST** (take as a work item; owner picks priority): `audio_spectrum`,
   `breakpoint_{add,clear,list}`, `get_{channel,layer}_states`, `log_clear`, `ping`,
   `run_to_scanline`, `set_{channel,layer}_enabled`, `step`, `step_out`, `step_over`,
   `vgm_{start,status,stop}`, `wait_for_break`, `write_vram`, `z80_{read,write}`.
   **Binding consequence:** D-10/D-13/D-17 have **two implementers** and must never be adjudicated
   as if one speaks for both.
   **⚑ STANDING OBLIGATION TO AEON — carry this across session boundaries.** The two servers spell
   some wire keys differently: legacy C++ uses `snake_case` in places, the contract and our Rust
   server use `camelCase` (25 known conflicts — **4 param**: `fftSize`, `maxHz`, `timeoutMs`,
   `maxFrames`; **21 result** across 11 methods, incl. `frameToken`, `symbolCount`, `symbolAtPc`,
   `romLoading`, `otherMatches`). empyrean ruled camelCase, **direction only**. Our server is
   **already fully camelCase** — zero snake_case wire keys in `crates/oracle-aether/src/` (swept,
   with the grep validated against `ControlSocket.cpp` first so the empty result is a measurement,
   not a broken pattern) — so this costs the acceptance work nothing.
   **THE OBLIGATION: message aeon with a DATE before the successor serves `emulator/wait_for_break`**,
   so they pre-migrate `tools/raster_source_gate.py` and `tools/snapshot_poison_gate.py` (both send
   `timeout_ms`) instead of debugging a red nightly at 04:17. Promised 2026-08-22; banked here
   because a promise living only in a chat log dies at the next `/clear`.
   **Sequencing, revised (now empyrean's bar 15):** *never retire the alias on the legacy server —
   retire it by REPLACING the server.* Our server refuses unknown params with `-32602` naming the
   key and listing the accepted set, at the single dispatch choke **before the handler runs**
   (`engine.rs:4290` / `:999` / doc `:186-187`). The legacy server **ignores** them —
   `ControlSocket.cpp:127` `getInt(k, d=0)`, `:149` `getU32` — so retiring the alias there hands
   `30000` to every caller still sending `timeout_ms`, and a gate that believes it waits 120s waits
   30 and still pastes a verdict into merge evidence. **When two implementations of one contract
   disagree about unknown keys, sequence the cutover onto the STRICT one** — a permissive
   implementation can only ever report success. Corollary: **dual-accept does not exist on the SEND
   side** (you cannot accept both spellings of a key you are *sending*), so it covers zero param
   sites; fail-loudly-on-absence is for *results*.
   **Two peer-claim corrections logged against this session, both mine:** (1) I cited four aeon
   files as diverging-key clients; **three were refuted** — `boot_override_gate.py:193` is a helper
   *named* `frame_token` whose body at `:197` reads `["frameToken"]` (already camelCase, already a
   subscript that fails loudly), and `raster_frame_epoch_probe`'s `frame_token` mentions are a
   comment and two docstrings while the executing code keys off `Frame_Counter` via `read_memory`.
   Bar 11 exactly: a citation points into code that keeps executing past the line you were shown.
   (2) Worse — I **endorsed empyrean's `.aeon-nightly` inference and ranked it above my own findings
   without checking any of it**. Refuted by aeon: `.aeon-nightly` is a clean *build* tree; systemd
   runs the MAIN tree's script; that script invokes one tool with zero diverging keys. **Carrying is
   not running.** The scariest item in the set — an inverted stall detector in an unattended loop —
   **does not exist**; `repro_wedge3` is hand-run. A second session endorsing an unverified
   inference adds no evidence and makes it look corroborated. Now a clause on empyrean's bar 14:
   **trace the invocation chain; never infer exposure from a file existing in a tree.**
   **Carried findings worth acting on** (all anchored in the merged doc): D-13's framing is
   backwards — the watch surface was rebuilt on the new server and **breakpoints were never carried
   across** (legacy watchpoints are the worse surface: add-only, no list/clear/hits); the stale-
   breakpoint harm is **ours on the record** (`docs/2026-07-23-timing-ground-truth-fable.md:162-165`
   — an agent cleared 7 breakpoints "not mine", one at 1,691,410 hits, promising a restore it could
   not perform); **§6 lines 1137/1138 (`read_vdp_registers`, `read_vsram`) describe methods NO
   implementation has ever served** (design task, not transcription backlog; `read_vsram` may want
   retiring since `emulator/read {space:"vsram"}` covers it); **`call_stack` params are
   `max_bytes`/`max_frames` in code vs `maxBytes`/`maxFrames` in §6:1373** — §2.5's closure turns
   that into a hard `-32602`; **legacy error codes are inferred from message SUBSTRINGS**
   (`ControlSocket.cpp:211-222`) so rewording a message changes the wire code; **`lookup_symbol` can
   silently discard a caveat** (`engine.rs:2940` overwrites `:2933`, no `else` — conformant, real
   loss, no fix applied); legacy Z80 rows bound only the start address (multi-byte wraps mod 8 KiB
   and clobbers `$0000` reporting success).
   **Bar 12 payoff:** D-17's proposed remedy is **already a ruled precedent in `protocol.md` itself**
   (`:925`/`:933`, *"two vocabularies wearing one name"*), enforced here by one shared parser
   (`engine.rs:3998` from `:1664` and `:3511`). The rule was in the contract we already owned.
   **Two self-corrections banked** (both mine, both against my own report): our vendored count is
   **37→58, not 38→59** — my enumerator counted `methods.properties` keys and one is `$comment`,
   which cancels in the diff, so the added set matched and the error looked inert (**I enumerated the
   container's keys rather than the things the container holds**); and `git diff --name-only A..B`
   compares **tips**, so on a branch whose base has moved it reports the base's newer commits back as
   branch *deletions*, listing files the branch never opened — it reads as "what does this branch
   change" and answers something else (harmless at merge time, which uses three-dot semantics;
   dangerous in review, where it gets quoted as evidence a branch touched what it did not). Both are
   now in empyrean's anchor cluster.
   **STILL OPEN: the dry run** (`schema-dryrun`, holds the cargo lane). Its class (c) is to be
   reported as **the acceptance delta**, never as an unmeasured coverage column, and it needs a
   **witness that the run happened** rather than an unchanged hash (bar 4's converse).

   *(Original dispatch record follows — the setup, before any result was in.)* empyrean landed
   per-method JSON-Schema fragments for the bus protocol (`origin/main` `3bbfeb18`; the merge
   carrying code is **`fe5a238`**, `--stat`-verified as a real code merge: 5 files / +1160 —
   fragments, a gate `contract/schema/tests/validate_contract_schema.py`, 52 pass + 64 proven-red
   vectors, and the audit `docs/2026-08-22-protocol-schema-audit.md`). Coverage **37 → 58 of §6's
   66 methods**; the schema blob is sha256 `82dde99ef8c62d41…` (unchanged across `415eb18` →
   `3bbfeb18`, re-verified). **Eight methods deliberately carry NO fragment** — `z80_registers`,
   `read_vdp_registers`, `read_vsram`, `object_slot`, `object_list`, `player_state`, `call_stack`,
   `log_tail` — because under §8 item 20's closure a *half* fragment actively refuses a conformant
   server while §2.5 reads an absent one correctly as "not yet transcribed". **Absence is not
   latitude**: do not shape those replies freely, do not fill them unilaterally.
   **Two agents dispatched off `3ca8521`:** `peer-schema-answers` (source-side, no cargo — the four
   defects empyrean asked the *implementer* to settle before they rule: **D-30** the §2.4
   caveat self-contradiction, **D-13** the breakpoint surface's missing handle discipline, **D-10**
   `z80_write` width/byte-order/`len`, **D-17** the two setter enums with no vocabulary in the spec;
   plus the observed reply shapes of the eight absent-fragment methods, labelled transcription
   material, NOT proposed fragments) and `schema-dryrun` (holds the cargo lane — their 58 fragments
   run against the replies our server actually emits).
   **THE SEQUENCING CALL (do not reorder without cause): validate first, adopt second.** The
   vendored `crates/oracle-aether/tests/contract/bus-protocol.schema.json` **stays at
   `f038672daf6eb2b8…` (37 fragments) until the dry run reports.** Reason: once an unvalidated
   contract is a gate in our suite, a *fragment* defect and a *server* defect are indistinguishable
   from inside the gate — the red presents as ours and the whole gradient pushes toward changing the
   server to satisfy a wrong fragment. That is **bar 9 with the causation hidden** (nobody decides to
   bend the subject; the red build just makes bending it cheapest). empyrean adopted this as a
   corollary to bar 9 with our declining-to-vendor as the precedent. Order: dry run → split class (a)
   ours / (b) theirs / (c) unreached → they rule on (b) → then vendor, so the green means something.
   **Dry-run classes are never merged, and class (c) is reported loudly as unmeasured** — a silently
   skipped method read as a pass is the single most dangerous output of that task.
   **D-30 pushback, accepted by them as a standing CR-flow rule:** "which sentence has Oracle been
   built against" is the right question and the wrong thing to *decide on* — it is evidence about our
   history, not an argument on the merits. Otherwise "the spec is the source of truth" quietly
   inverts into ratifying whatever shipped, i.e. that principle failing while appearing to be
   followed. We supply the count; they rule. **If it rules against us we take the conformance work.**
   **Anchor hygiene, five faces in one afternoon** (empyrean `6459f92`, reachable from `3bbfeb18`):
   their missing push; **ours — measuring an UNPUSHED preview** (our dry run began against a
   working-tree copy; the blob turned out identical so it anchored retroactively, but that was **luck
   confirmed by a check, not a property of the method** — re-anchor with `git merge-base
   --is-ancestor` *and* hashing the pushed blob against the bytes actually measured); seraph's
   receiver side (a rewritten history dangles a cited anchor and **only the citing side can warn
   them**); aurora's, which needs nobody to forget anything — **on this shared machine, reading the
   sibling *directory* measures a peer's live working tree**, so `cd ../empyrean && git rev-parse
   HEAD` answers cleanly and confidently about an uncommitted mid-edit tree (⚠ **this session did
   exactly that** when hashing their schema file — it happened to equal their local `main`; prefer
   `git show <rev>:<path>` over reading a sibling's working file); and aeon's fifth, outside anchors
   entirely — **on a byte-neutral parcel a matching CRC cannot witness that the build RAN**, since a
   stale ROM and a correctly rebuilt one are byte-identical exactly when no bytes moved. Unifying
   line: **every one is a competent lookup returning a clean answer about the wrong object, and none
   is visible from inside the session that made it** — bar 8's shared-frame conclusion arriving from
   a different direction. New peer-protocol rule from their side: **push before you cite, and verify
   `origin/<branch>` actually moved** — a *positive act* against the remote, never the absence of an
   error.
   **Currency note:** our previous "vendored == empyrean tip" check (booked in the bars section
   below, run at tip 2026-08-22 morning) **passed for the wrong reason and is now superseded** — the
   contract genuinely had not moved *at the remote*, which is the only place a consumer can look,
   while sixteen commits sat unpushed locally. Not a defect in the check; a limit of it.


## [orig lines 462-862] item 8's closed sub-arcs: the acceptance survey, CR-A drafted and parked, the step trio served, CR-B drafted

8. **▶ OPEN — THE ACCEPTANCE CONTRACT: the 21 methods. Opened 2026-08-22 evening.** Item 7's real
   deliverable becomes the queue's front. The 21 fragments that describe methods our Rust server
   does not serve **are** the definite list of what the successor must serve before it replaces the
   legacy C++ server — an item open since 2026-07-01 that has never had a membership until now.
   The list is in item 7; **re-derive it, never transcribe it.**
   **Baseline re-derived firsthand at boot** (`0fa34f1`): the schema carries **58** fragments —
   note `schema["methods"]` maps method → fragment **directly**, with no `properties` sub-object,
   which is the shape my earlier `$comment` miscount came from. ~~`engine.rs` carries **39**
   `"emulator/*"` string literals = **37 methods + 2 events** (`stopped`, `resumed`).~~
   **⚠ CORRECTED by the survey, verified firsthand — 40 literals = 37 methods + THREE events.**
   The third is **`emulator/romReloaded`**, and my boot grep's character class `[a-z_]` **hid it
   behind the capital R**. The method count 37 was right, but *for a reason that could as easily
   have gone the other way*: had the hidden literal been a method rather than an event, I would
   have silently lost one from the acceptance list and the two "independent" derivations would
   have agreed on a wrong number, because **both were fed by the same character class**. Durable
   form: **a regex character class is an unstated assumption about the data, and it fails silently
   and identically in every derivation that shares it.** Use `[A-Za-z0-9_]` on wire names —
   `camelCase` is the contract's own spelling rule, so a lowercase-only class contradicts a rule
   this repo already enforces everywhere else. Core spot-checks: `system.rs:828 step_instruction`
   and `z80/mod.rs:412 Z80::step` exist; `breakpoint` appears in `oracle-core/src/` **only** in
   `bus.rs`, so the breakpoint surface is genuinely unbuilt and the served watchpoint quartet
   (add/list/clear/hits) is its nearest house precedent.
   **Two agents dispatched off `0fa34f1`:**
   - `acceptance-survey` (**no cargo**) — price all 21: fragment requirements, core readiness
     (ready/partial/absent), cost + whether serving as-written needs a CR, and a real consumer
     sweep across sibling trees (both identifier and quoted-key greps, worktrees excluded).
     Deliverable `docs/2026-08-22-acceptance-21-survey.md` + a proposed parcel ordering.
   - `serve-step-trio` (**holds the cargo lane**) — serve `step`/`step_over`/`step_out`.
   **Why the trio was dispatched WITHOUT waiting for the survey** (recorded because it is the kind
   of call the survey exists to prevent): their fragments are already **final and landed upstream**,
   so it is pure conformance with no contract fork to adjudicate, and the survey cannot make them
   the wrong first parcel — only reorder what follows. The survey's job is pricing the other 18.
   **Two contract defects ride this parcel and are to be served as-written, then flagged:**
   **D-02** — `step`'s `count` has no default, no minimum above 0 and **no upper bound**, so an
   unbounded step is contractually legal; **D-03** — `step` returns `pc` while `step_over`/
   `step_out` declare **no result keys at all**, an asymmetry §6 owns and the fragment authors
   deliberately preserved. Both become CR text to empyrean, never a unilateral server deviation.
   ⚠ The named failure mode for this parcel, stated in the brief: **a `step_over` that silently
   behaves like `step`** is worse than an unimplemented one. BLOCKED on the pair is a good outcome.

   **SURVEY LANDED `610aa0d`** (`docs/2026-08-22-acceptance-21-survey.md`, 1257 lines, docs-only —
   so aggregate and currency are unmoved *by construction*, recorded as reasoning rather than a
   skipped run; the trio agent held the cargo lane throughout). **The 21-item list is CONFIRMED
   correct** — derived − briefed = ∅ in both directions, by two derivations (the literal grep and a
   structural parse of the `METHODS` dispatch table, both 37), with
   `SCHEMATIZED_NOT_ADVERTISED` (`schema_conformance.rs:388`, item 7's D-33 pin) matching
   name-for-name as corroboration. The agent correctly declined to call that a third derivation: it
   is a hand-written list, so agreement means the pin is current, not that the parse is right.
   **Nine of the brief's facts came back wrong. I verified five firsthand; all five held:**
   (1) the `romReloaded` event above; (2) **`docs/protocol.md` does not exist at empyrean
   `origin/main` — it is `contract/protocol.md`**, so the read command I put in *both* briefs
   fails; (3) **`system.rs:828 step_instruction` is NOT the stepping primitive** — zero callers in
   any `src/`, only `tests/step_retire.rs` and `examples/differential_trace.rs`, and I named it as
   the starting point in both briefs (the survey adds that it does not advance the master clock —
   *not* independently confirmed here, flagged as a claim); (4) the three-method aeon scope above;
   (5) **no fragment declares any error condition — all 58 carry only `$comment`/`params`/`result`**,
   so every error obligation (`-32005` machineRunning, `-32602` unknown-param) lives in **prose**
   and cannot be validated against a fragment. Conformance tests for error shapes must derive from
   `contract/protocol.md`, and a suite that only validates replies against fragments is **blind to
   the entire error surface** — worth a named gate.
   Unverified here, carried as the agent's claims: `vram_mut` is a pub-ised test hatch that bypasses
   the SAT-cache write-through (so `write_vram` is not the freebie it looks like); the synth tree is
   `#[cfg(feature = "synth")]`, default-off, and **not compiled into the Aether server at all**,
   while `vgm.rs` is ungated — which is what separates the VGM parcel from the audio ones;
   `run_to_scanline`'s contractual 0–511 is unreachable above 261 (`LINES_PER_FRAME = 262`,
   confirmed present); sigil's 14 hits use the **bare method name with no `emulator/` prefix**, so a
   prefixed grep cannot see them. Two claims are tagged for foreground runtime follow-up (the `step`
   frame-budget truncation and the `write_vram` SAT desync) — **not** to be settled by a subagent.
   **Consumer sweep: 14 of 21 have no consumer of any kind**; three are reached by aeon's nightly
   systemd chain; four are manual-only; aurora and seraph have **zero code hits** (prose only).
   **Proposed ordering, adopted:** trio → `run_to_scanline` → `write_vram` (via a new `poke_vram`,
   **not** `vram_mut`) → `z80_read`/`z80_write` → **breakpoints + `wait_for_break` together**
   (gated on CR-A) → VGM → channel masks → `audio_spectrum` → layer masks. `ping` and `log_clear`
   **parked**: `ping`'s only consumer already guards on `hasMethod`, and no log exists for
   `log_clear` to clear — serving it alone would advertise a capability that does nothing.
   Stepping leads rather than cheapest-first because **stepping and breakpoints are one mechanism**;
   building breakpoints first designs the run-stop surface twice, under a blocked contract question.
   **Only 3 of 21 are contract-blocked.** The real bottlenecks are `wait_for_break`'s blocking-
   transport question (unanswered, and it dominates the aeon date) and three genuinely absent core
   capabilities: FFT, channel mask, layer mask.
   **Two fixes found and deliberately NOT taken** (correct — surveying is not implementing):
   `resolve_target` does not enforce the `addr`/`symbol` `oneOf`, which is a **live unregistered
   request-side divergence on `run_to` today**; and `schema_conformance.rs:6,222` carries stale
   prose ("9 of the 21 we advertise", "all 25"). Both are queued, not lost.
   ⚠ **Method note, and the reason the survey paid for itself:** it was dispatched to price work and
   its most valuable output was **correcting the overseer's own boot derivation**. Three of the
   nine wrong facts were pointers *I* supplied with confidence. A brief's stated facts get
   transcription-grade trust from the agent receiving them — the same failure the protocol's
   provenance cluster names — so **an agent that checks its brief instead of executing it is doing
   the job right**, and briefs should say so explicitly.

   **CR-A DISPATCHED (`cr-a-breakpoints`, off `6ad68ac`, docs-only/no cargo) — aeon consulted
   FIRST and answered decisively. Five rulings, banked here because they are contract-shaping and
   must survive a session boundary even if the draft never lands.** aeon's claims were verified
   firsthand at *their* `origin/master` before use, not taken on trust:
   1. **Handles are the addressing primitive.** Their argument, adopted with attribution:
      address-keyed clear is ambiguous the moment two subscribers arm the same PC and **silently
      kills another subscriber's breakpoint**; the workspace trends toward more concurrent lanes
      against fewer emulators. Load-bearing check: they claim no dependence on breakpoint identity
      (only on **stop-PC** identity, asserted separately) — **verified present in BOTH gates**,
      `raster_source_gate.py:176` and `snapshot_poison_gate.py:70`, mismatch = SETUP FAILURE exit 2.
      That assertion is the whole reason handles are cheap for them.
   2. **`clear {all: true}` SURVIVES as a distinct teardown primitive** — theirs, and the half we
      would not have reached alone. **A gate that crashed mid-flow cannot enumerate what it armed**,
      so teardown must not require handle tracking. *Clear-all is the teardown primitive; handles
      are the addressing primitive; collapsing them breaks crash-path cleanup.* Recorded with its
      reason precisely because a future editor will try to simplify it away.
   3. **The `stopped` event names the fired handle — PROMOTED to REQUIRED** over their
      nice-to-have. The pre-release window for REQUIRED additions shuts at first ship and an event
      field is materially harder to add later; and without it *"wrong breakpoint fired"* and
      *"right breakpoint, wrong PC"* are one indistinguishable failure message.
   4. **Stop precision — the clause that generalizes past breakpoints.** Their property, adopted:
      **either the stop PC is exact, or the server says it isn't.** The failure that hurts is
      *imprecise stops presenting as precise*. Concrete precedent, verified verbatim at
      `raster_source_gate.py:32-40`: the legacy server under `deterministic=True` stops at commit
      granularity, landing one instruction early — *before* an `adda.w` — so a register still holds
      a plausible unmodified value that would make their gate **PASS on code that never applied the
      offset**. A false PASS, not a crash, which is strictly worse. Ours offers exact stops; any
      imprecise mode needs explicit opt-in **and** must carry granularity in the reply.
   5. **`wait_for_break` resolves against an EVENT; it must not block the connection.** Decisive
      argument: a blocking call lets a wedged emulator take the socket with it, making the
      **client-side timeout unenforceable — destroying the property the call exists for.** Their
      120 s is a *wedge detector*, not a performance budget. *A wedge detector that cannot give up
      is not a detector.* Connection stays usable during an outstanding wait; cancel available.
   ⚠ **One aeon citation NOT confirmed → RESOLVED, and the resolution is the interesting part.**
   Their "documented oracle MCP press deadlock" is **not in our docs** (the only deadlock reference,
   `2026-08-14-tooling-frontier-recon.md:272`, describes a hunt *now fixed*). Reported back as
   **did not find**, never "does not exist" — and that phrasing is why it got resolved instead of
   quietly dropped. **It exists in THEIR tree**: aeon `docs/superpowers/2026-08-14-next-session-
   handoff.md:122`, *"`emulator_press` wedges intermittently (StopSystem race) and blocks ALL MCP
   until…"*, last touched by `54089d8c`. **Verified firsthand: commit exists and
   `--is-ancestor origin/master` = YES.** Their citation was defective in a way our search could
   not have resolved — *"a documented oracle MCP press deadlock"* never said **whose**
   documentation, and it is an aeon-side observation **of** oracle's behaviour. Durable form:
   **name the tree, not just the document** — a cross-repo citation that omits whose docs it lives
   in sends the receiver to search the wrong tree, where a correct search returns a correctly empty
   result.
   **STILL EXCLUDED FROM CR-A, at their own argument and with our agreement.** It is dated
   2026-08-14, describes a wedge class **neither lane has re-tested**, and their
   `docs/research/phase_harness/phase_notes.md` marks adjacent wedge/park classes as later fixed
   (the reload-park drain workflow is OBSOLETE outright). Honest status is **unknown-today, not
   live**. *Putting an unverified historical hazard into a contract document as motivation is how a
   stale caveat outlives the defect it described* — the precedent-perishability failure, twice hit
   in this workspace today. Ruling 5 stands on the wedge-detector logic, which needs no emulator
   archaeology. **The CR-A brief never carried the citation, so no correction was needed** — checked
   rather than assumed. Their own answer also self-corrected: they had enumerated by **key
   spelling**, which found `timeout_ms` and so found the method carrying the key rather than the
   flow it belonged to — the same family as my `[a-z_]` class, one by the wrong attribute and one
   by the wrong alphabet.

   ⚑ **INDEPENDENT CORROBORATION OF THE `write_vram` HAZARD, from a changed frame — worth more
   than the finding itself.** `docs/2026-08-14-tooling-frontier-recon.md:274-277` (ours, eight days
   old, a legacy-C++ recon) already flags `write_vram` as *"a genuine landmine to fix if ported"*:
   it writes straight into the VRAM buffer, **bypassing the VDP port path, autoincrement, FIFO and
   DMA**, with nothing in its docstring saying so — *"an agent will 'verify' tile data and conclude
   the bug is elsewhere having proven nothing about the real path."* Today's survey, reading the
   **Rust** source via `vram_mut` with no knowledge of that doc, found the same hazard and
   independently proposed **the same name, `poke_vram`.** Two derivations, different frames, same
   identifier — this is the agreement that counts, as against the shared-frame kind that burned
   both lanes today. **The recon is strictly ahead of the survey on one point: it requires the reply
   to flag `bypasses_vdp_port: true`.** Folded into the `write_vram` parcel as a requirement.
   *Method note: this surfaced only because a peer's transport citation sent me into an old recon
   doc for an unrelated reason. Grep the repo's own history before pricing a parcel — this one had
   been sitting in `docs/` for eight days and no session had looked.*
   aeon's addition, adopted: **the reply flag is the part that protects an agent; the name only
   helps someone who already suspects something.**

   ⚑ **PROPOSED UPSTREAM (sent to empyrean 2026-08-22): a DISCRIMINATOR for shared-frame
   convergence — bar 8's missing half.** Bar 8 says mutual verification cannot catch a shared frame,
   only a changed frame can; it does **not** say how to tell a genuine convergence from a
   shared-frame one, and the two are indistinguishable from inside. Today gave four instances —
   three negative (my `[a-z_]` class; aeon's enumerate-by-key-spelling; empyrean's file-presence
   inference) and **one positive** (the `write_vram` convergence above). aeon's framing of the pair,
   which is the sharp part: **theirs enumerated by the wrong ATTRIBUTE, mine by the wrong ALPHABET**,
   and in both cases *the enumeration parameter was too small to be recorded as a choice at all* —
   nobody writes down "I chose a character class". **An unrecorded choice cannot be varied on a
   re-check, which is exactly why running the check twice cannot help.**
   **Proposed test: the discriminator is not independence of AGENTS, it is independence of the
   ENUMERATION PARAMETER.** Before treating agreement as corroboration, name the parameter each
   derivation enumerated over and check they differ; if you cannot name it, the agreement is
   untested. **Delegation corollary, directly load-bearing for this role: two agents given the same
   brief share its frame BY CONSTRUCTION**, so a second agent confirming the first mostly re-measures
   the brief — today's most valuable agent result was the one that **contradicted** its brief on nine
   points, three of them facts I had supplied with confidence.
   ⚠ Sent with its own weakness stated: **the proposal is itself an agreement between two lanes who
   talked before writing it down**, i.e. a shared frame by the very mechanism it describes — flagged
   to empyrean as one lane's proposal with a second's endorsement, not two independent findings.
   Theirs to phrase or reject; the protocol changes in empyrean and is never forked into a repo copy.

   **CR-A DRAFTED + MERGED `1265995`** (`docs/2026-08-22-cr-a-breakpoints.md`, 953 lines + a 14th
   §, docs-only). **Un-framed adjudication DISPATCHED** (`ruling-cr-a`, fresh Fable, no steer).
   Shapes proposed: opaque-string handle in a field named `breakpoint` (not `watch`'s abbreviation —
   argued, not diverged silently); `breakpoint_add {addr|symbol, label?}` → `{breakpoint, addr,
   stopPrecision, label?}`; `clear {breakpoint|all}` → `{removed}`; `wait_for_break {timeoutMs?}` →
   `{pc?, …, timeoutReached?, cancelled?, waitedMs?}`; a **new** `wait_cancel` keyed on the JSON-RPC
   **request id** (under a non-blocking wait there is no reply to carry a handle, so the id is the
   only identifier the client holds), same-connection-only *or one client can end another's wait —
   the address-keyed-clear hazard rebuilt on the wait surface*. 58 → 59 fragments.
   **Four drafter departures from my rulings, all argued at the point of use** — `stopped` carries a
   **plural** `breakpoints` array (duplicate adds create distinct breakpoints, so a singular field
   would force the server to name one as *the* cause; `watch` has the same latent multiplicity and
   was deliberately **not** changed); `breakpoint_set_enabled` **rejected** and `enabled` struck;
   **D-12 ruled AGAINST the audit's recommendation** (the idempotent reading was reasoned inside an
   address-keyed model, and under handles collapsing duplicates rebuilds the very two-subscriber
   hazard the CR exists to fix); a third reading of D-14 neither of its two offered.
   **Ruling 3's sub-question answered by inspection and verified here: §3's `reason` enum ALREADY
   contains `breakpoint`** (first member; `["breakpoint","watchpoint","step","runTo",
   "runToScanline","runFrames","pause","entry"]`). No new reason value needed — the same situation
   §11.8 found with `watchpoint`: an enum member no catalogued method could produce.
   **⚑ THE DRAFT'S BEST FINDING, verified verbatim at `contract/protocol.md:158-160` — D12 reaches
   this method by name** (*"any future `wait_for_break`-shaped op … MUST accept a `maxFrames` bound"*)
   **and cannot do the job.** A `maxFrames` bound is a bound in **emulated** time; a wedge is the
   state in which emulated time stops advancing — so it cannot trip in exactly the failure it exists
   to catch. Not a weak bound on a wedge; **structurally incapable of being one.** D12's second
   mandate misfits too: `reached` is specified *"beside its echo of the target"* and a wait for any
   breakpoint has no target. **The precedent the draft missed and I added (bar 12 again — the rule
   was in the contract we already owned): `protocol.md:1432-1433` already scopes D12 out of
   `play_input`** (*"D12 does not apply — the stop condition is an exhausted count, not a predicate"*),
   so the ask is the **second instance of an established pattern**, not a novel exception.
   **§14 addendum added by me before adjudication** (so the adjudicator rules on the strongest
   version): (1) **the prior question RESOLVES — CR-A is in order.** The drafter flagged that the
   audit's *"for D-10, D-13 and D-17 the answer belongs to the legacy server"* might reserve ruling
   authority, and called its own narrower reading self-serving. It resolves on the audit's **own
   text**: D-13's Recommendation at `:193-195` literally says *"raise a change request that brings
   the breakpoint surface up to the watchpoint surface's shape"* — **the audit commissions this CR
   by name** — and `:481` presupposes *"the eventual amendment"*. The clause is a binding on
   **scope** (nothing we rule binds the legacy server), not a reservation of authority. ⚠ Flagged
   for the adjudicator: §7.2 rejects `breakpoint_set_enabled`, which is **named in the very
   Recommendation that authorises the CR** — declining a commissioned item is a higher bar than an
   open design choice. (2) the D12 precedent above. (3) currency + a correction.
   **Currency checked AT TIP** (the correct direction for a drift question): the CR anchors empyrean
   at `9d6ab1f` — real, ancestor of `origin/main`, and **zero commits have touched
   `contract/protocol.md` since**, so every protocol citation is current even though their tip has
   moved to `56f469b` (churn elsewhere in their tree).
   ⚠ **Third overseer brief-fact corrected by its own agent today.** I told the drafter the handle
   migration costs aeon *"a trivial rewrite"*; the correct answer is **nothing at all** — both gates
   clear only with `{all: true}`, never pass an address to clear, `breakpoint_add`'s params are
   unchanged, and its new reply key is simply ignored. My weaker claim was underived; theirs was
   derived. **Owed to aeon as a correction.**

   ⏸ **ADJUDICATION BLOCKED — PARKED FOR THE OWNER, 2026-08-22 evening.** The un-framed Fable
   adjudicator was dispatched (`ruling-cr-a`) and **died immediately on the Fable 5 account limit**.
   No ruling exists; the CR is drafted, merged and pushed but **UNADJUDICATED**, and per the
   standing bar (*"a ruling authorizes the change; adjudication is what authorizes the TEXT"*)
   **nothing in it may be implemented yet.**
   **Deliberately NOT silently substituted.** Swapping in another model is exactly the quiet
   degradation the protocol bars, and here the overseer would be the one doing it — the owner
   **ratified** the Fable seat on 2026-08-21 with the cost questioned and answered (*the smartest
   model sits at the ruling and nowhere in the bulk work*), so re-spending that decision is the
   owner's, not mine.
   **The two properties of the seat are separable, and that is what makes a substitution
   defensible if the owner wants one:** *un-framed* (no drafting participation, no steer) is the
   load-bearing half and is preserved by any fresh agent; *Fable-tier* is the second half. An Opus
   adjudication would keep the first and lose the second. **If substituted, the ruling doc must
   name the model that ruled**, so a later reader knows which standard was applied rather than
   assuming the ratified one.
   Options for the owner: (a) top up Fable credits and re-dispatch unchanged; (b) authorise a
   **named** Opus substitution; (c) hold CR-A unadjudicated — costs nothing, since the stepping
   trio explicitly does not depend on it and the acceptance ordering puts breakpoints fifth.
   **(c) is the honest default and blocks no other work.**
   **Tagged for FOREGROUND runtime follow-up (never a subagent):** §6.5's claim that our core stops
   exactly *by construction* is read off `bus.rs:305-318` **only** — it must NOT be asserted in a
   handshake capability until something has actually been run. That is the `stopPrecision` field's
   whole credibility.

   **▶ TRIO SERVED — merged `56cc545`. THE ACCEPTANCE CONTRACT MOVED FOR THE FIRST TIME: 21 → 18.**
   (`schematized-but-unadvertised` pin dropped by exactly three; that pin is item 7's D-33 guard, so
   the count is enforced, not asserted.) Commits: `da7c1a5` core (`return_pop_bytes` — RTS/RTR/RTE
   frame sizes as one `const fn` beside `control_flow_of`, pinned exhaustively over all 65,536
   opcodes; the profiler's 3 private constants folded into it rather than left as a drifting
   mirror), `a05e34c` aether (three handlers + a `StepStop` shadow-stack sink; `advance_until`'s
   Fanout **factored** into `advance_with`/`attribute` rather than copied), `f737f94` tests
   (`crates/oracle-aether/tests/step.rs`, 16 tests).
   **⚑ BOTH OF MY BAD POINTERS WERE CAUGHT BY THE AGENT INDEPENDENTLY, BEFORE MY CORRECTION
   ARRIVED.** It found `contract/protocol.md` via `git ls-tree` when `docs/protocol.md` failed, and
   it **rejected `step_instruction` on the function's own doc** — *"it does **not** advance the
   master clock (the caller owns time)"* — building instead on `System::run_frames_with_sink`, which
   is also the only path that re-anchors the frame grid (bypassing it would corrupt every later
   `run_frames`).
   **THE WARNED-OF FAILURE IS NOW PINNED, NOT ARGUED.** Mutation **M3** rebuilt the server on
   `step_instruction` and failed with **`mclk moved by 0`** — a step that advances the PC and not the
   machine, which is exactly the plausible-looking wrong answer the brief named. Mutation **M1**
   pins the parcel's other named failure: `step_over` silently degraded to `step` lands at
   `768`(PROF_LEAF) where the caller's next instruction is `836`. **16 mutations, 16 named failing
   assertions.** Three were killed **by the contract rather than by the test file** — a `symbol` of
   `"Leaf+$2"` and a stray `pc` on a no-result-key row both died in `common/schema.rs` on
   `$defs/symbolName` and item 20's closure.
   **Two self-corrections the agent made from reading the core, both real:** count **retires**, not
   boundaries (the boundary hook double-fires on the stopping PC, so a boundary counter is off by
   one *exactly when a caller resumes*); and never `sp >= sp0` (the `move.l/rts` dispatch idiom
   returns while leaving the stack where it found it, and a user-mode interrupt switches A7 to a
   different stack entirely). Returns are matched on the profiler's **exact** rule.

   **OVERSEER RULINGS on the returned work:**
   - **`deadlineReached` on a `step` stop — RATIFIED.** §3 scopes it to the run-shaped reasons in
     *prose*; the schema permits it unconditionally. The agent emits it because silence when the
     bound was hit is a **believable wrong answer** — which is aeon's ruling-4 property (*either the
     answer is exact or the server says it isn't*) arriving on a different surface. Ratified, and
     **registered as a CR item** so §3's scope is made explicit rather than left as our reading.
   - **`capabilities` left unchanged — RATIFIED.** The schema has no step-related capability key, so
     adding one is §8's invention ban. `breakpoints` correctly stays `false`; this parcel built none.
   - **`lookup_symbol` caveat overwrite — confirmed, correctly NOT fixed** (`engine.rs` ~`:2932-2944`;
     the displacement and ambiguity `if`s are independent, so at `Macro+$6` with an ambiguous
     readable name the second assignment discards the more load-bearing warning). Stays registered;
     `step` uses `symbol_at` and touches no caveat, so the pattern was not propagated.
   - **F-STEP-FRAME-BOUND registered:** the 600-frame bound is **server policy and undiscoverable** —
     no contract key exists for it. Rides the D-02 CR below.
   - **Untested paths, named honestly and accepted as such:** the `lost_track` suppression (needs
     16,384 nested calls; suppresses the stop rather than guessing, degrading to `deadlineReached`),
     and `step_out` outside any subroutine (correctly runs to the bound).
   **CR TEXT BANKED FOR D-02/D-03 — both improve on the audit, which is the point of serving a
   fragment you think is imperfect rather than bending it:**
   - **D-02:** `count? (≥0, def 1, ≤ maxStepCount)`, **refused** above the ceiling on `press.frames`'
     pattern rather than clamped, ceiling advertised as `initialize.limits.maxStepCount`. **Floor
     stays 0, against the audit's `≥1`** — zero is definitional for a count, is a useful *where am I
     without moving the machine* probe, and raising it would break a client already using it.
     **The half the audit misses:** a bounded `step` needs somewhere to report a **short** one — add
     `reached` to the result and lift the `caveat` prohibition when it is false. Today the one case a
     caller most needs the truth (*did my 10,000 steps happen?*) is the case the result **cannot
     express** — §11.5's `run_to.stoppedAtFrame` defect, inverted.
   - **D-03:** give `step_over`/`step_out` `step`'s `pc`/`symbol?`/`symbolDisp?`. **Implementation
     evidence the audit did not have:** because these two must be frame-bounded (no param can carry a
     budget), the caller's key question is *did the frame return, or did the run hit its bound?* —
     and today that answer exists **only on the event channel**, so a conformant client that did not
     negotiate `events` cannot obtain it at all. The asymmetry is not inelegance; for one class of
     conformant client it makes the method **unanswerable**.
   ✅ **VERIFIED FIRSTHAND ON THE MERGED TREE, all four gates with real exit codes** (not the
   branch-side run, and not the agent's report — which matched exactly, as they always have):
   `cargo fmt --check` **exit 0**; aggregate **`LEGS=50 PASSED=1787 FAILED=0 IGNORED=6`, exit 0**;
   `cargo clippy --workspace --all-targets` cached **exit 0 / 0 warnings**; fresh after
   `cargo clean -p oracle-core -p oracle-aether` (33,933 files, 8.3 GiB removed, so it genuinely
   rebuilt) **exit 0 / 0 warnings**. Baseline was `49/1770/0/6`: **+1 leg, +17 passed, fully
   accounted** — 16 in the new `step.rs` leg + 1 in oracle-core's lib leg
   (`return_pop_bytes_is_non_zero_on_exactly_the_return_classes`). Currency: **zero-file-diff on
   `crates/oracle-core/tests/`**, the default expectation for bus work, met without a named exception.
   *Ops note re-earned: a `pgrep` for the cargo lane returned a hit that was **my own shell's command
   line**, and a second returned a stale hit from the just-finishing run — the lane check is racy at
   an agent's tail. Check exit status (`pgrep` exit 1 = clean) rather than reading output presence.*
   **TAGGED for FOREGROUND runtime follow-up (never a subagent):** drive the three against a real ROM
   through the live server. Nothing in this parcel has touched a running machine.

   **✅ CR-B DRAFTED + MERGED + PUSHED `37a06f9`** (`docs/2026-08-22-cr-b-z80.md`, 1028 lines,
   docs-only — **zero non-docs files across `HEAD~1...cr-b-z80`, three-dot**, so aggregate and
   currency are unmoved *by construction*; no cargo run because the `run_to_scanline` agent held
   the lane, recorded as reasoning rather than skipped silently). **UNADJUDICATED** — the Fable
   seat is still parked, so **two** CRs now stack behind that owner decision, which is the first
   time the park has had a cost.
   **Scope ruling: all three defects in one CR, B4 severable.** D-09 (`z80_read`'s `len` has no
   default) and D-10 (`z80_write`'s `value` has no width/order, leaving the reply's `len`
   underdetermined) are provably **one missing paragraph** — `len`-in and `len`-out. D-11 (the row
   is absent from §6's run-control state rule) earned inclusion on a different argument: *a server
   decides the run-state gate in the handler's first ten lines, so leaving the contract silent does
   not defer the question — it answers it in unreviewed code.* But the audit pairs D-11 with D-16,
   so B4 is written severable and the split is handed over as Q4.
   **⚑ THE FINDING THE QUEUE DID NOT ASK FOR, and the CR's highest severity — verified firsthand
   at `oracle-old d629771`, reading the lines around every cite.** Both legacy Z80 handlers bound
   **only the start address** (`ControlSocket.cpp:700`/`:724` refuse `addr > 0x3FFF`, then loop
   `addr + i` with no end check), and `WriteRamByte` (`:298-320`) folds `off = addr & (bytes-1)`
   over an 8 KiB device and **returns `true` unconditionally**. So `z80_write {addr:"0x3FFF",
   bytes:<16>}` clobbers `$0000` and replies `len:16`, success. **The refinement matters and the
   agent stated it against its own headline:** folding `$2000→$0000` is the **hardware mirror and
   correct** (the read handler's own comment says so); only the fold *past* `$3FFF` is wrong,
   because `$4000` is the YM2612, not RAM. Recorded as a *silent wrong answer in a running
   implementation*, which is the class that outranks every under-specification in the CR.
   **⚑ D-10 RESHAPED BY EVIDENCE — the legacy server did not fail to pick a width, it declined to
   have one ON PURPOSE and left a comment saying why** (`:732-737`: *"Single byte — the Z80 bus is
   8-bit… no endianness guesswork"*, verified verbatim). **Consequence the brief did not have and
   the audit does not state: adopting the audit's (b) verbatim — `width` REQUIRED — refuses every
   bare-`value` invocation on record.** Proposal is (a)'s default with (b)'s ceiling: optional
   `width` ∈ {1,2}, default 1, little-endian.
   **The load-bearing joint, checked because nobody cited it (bar 8's cheap frame-changer):
   byte order.** It is argued from the RULE, not the sibling's consequence — `write_memory` says
   *"big-endian, **as the 68000 stores**"*, so the clause after the comma is the rule and
   big-endian is its consequence on that machine; the Z80 stores low byte first, so **copying
   `write_memory`'s consequence would land a pointer backwards**. Symmetry with the 68000 row is
   the one thing that would be actively wrong here.
   **Precedent-before-invention paid out again (bar 12, third time in two days):** narrowing the
   Z80 bound to `$1FFF` **was already proposed and ruled AGAINST in this repo**
   (`docs/2026-08-16-ruling-cr20.md`, whose §"the factual error" is precisely a CR narrowing a
   catalogued bound *twice*). The agent kept `0–$3FFF` and stated the mirror instead — i.e. our own
   history stopped us repeating a mistake we had already paid for.
   **Consumer sweep: ZERO programmatic consumers** (quoted-key form exits 1 on all four non-empyrean
   trees; sigil's 32 hits are its own assembler identifiers). **48 written-down mentions** across
   aeon/seraph/empyrean, all prose. **The honesty that makes it usable: the doc asserts no call-site
   count at all** — *"a mention is not an invocation, and no grep separates them reliably"* — and
   asserts instead the **form** of every invocation that appears: single-byte `value` or `bytes`,
   **no `width`, no `value` above `0xFF`, every address in `$0000–$1FFF`**. That form claim, not a
   count, is what makes "default 1 preserves every recorded meaning" true. Three spellings live
   (`emulator/z80_write`, `emulator_z80_write`, bare), so a prefixed grep would have missed ~half.
   **THREE MORE BRIEF-FACTS OF MINE CORRECTED, all confirmed here** (that is now nine in one day,
   across two agents): CR-A is **1114 lines, not 953** (it grew the §14 addendum I wrote myself);
   substantial prior legacy analysis of D-10 already existed at
   `docs/2026-08-22-peer-schema-defect-answers.md:482-608` and I did not mention it (the agent
   re-derived at `d629771` rather than inheriting, and cites it as corroboration); and I presented
   the audit's (b) as its position without flagging that it contradicts the shipping implementation.
   The agent also self-corrected an aggregation it could not defend ("26 call sites") **before**
   reporting — the count-the-rows-a-tool-printed error, caught on its own side this time.
   **Adjudicator questions Q1–Q7 stated, plus six items listed as SETTLED so the adjudicator can
   object to the settling too** — the right shape; a CR that hides its own closed questions
   forecloses them silently.


## [orig lines 901-933] CR-28 pause/resume, and aeon's consumption verdict

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

## [orig lines 969-977] F-HUD-FILTER-LABEL, done

- ~~**F-HUD-FILTER-LABEL**~~ **DONE 2026-08-29 (SCREEN-HONESTY parcel)** — was: the F3 status line
  printed the console **audio** output stage as a bare `MODEL1-VA0-VA2` with no label (`overlay.rs`
  `status_text`, between `VOL` and the aspect / native-resolution / frame fields), so most of its
  neighbours were video facts and **the owner read it as a VDP/board revision** (through aurora
  2026-08-29 — the misreading was ours, not his). Now `AUDIO VA0-VA2`, via a frontend-local
  `filter_label` rather than `ConsoleModel::name`: that identifier round-trips through `from_name`
  (it is what `ORACLE_CONSOLE_FILTER` parses) and so is not free to be shortened for a readout.
  `Unfiltered` renders `RAW`, deliberately not `OFF` — `AUDIO OFF` reads as *there is no sound*.
  **Shorter than what it replaced**, which mattered: see the width finding below.

## [orig lines 1014-1045] F-IDENTITY-JOIN-UNASSERTED, found and fixed

- ~~**F-IDENTITY-JOIN-UNASSERTED**~~ **FOUND AND FIXED 2026-08-29** — *found by the aurora lane by reading
  our three identity tests AGAINST EACH OTHER, which is a thing no single test's author is positioned to do.*
  **Every component of §2.1's non-forgeability guarantee was proven and the SEAM between them was not.**
  `_COMPILE_TIME_OR_NOTHING` proved the constants are compile-time; the config test proved their *names*
  appear only in `build_info.rs` and `engine.rs`; `the_compiled_in_build_id_still_names_this_tree` proved the
  *constant* names this tree; the wire test proved the *wire* value was a non-empty string with a registered
  `source`. **Nothing asserted the string on the wire was that constant** — and `engine.rs` is an ALLOWED
  file, so an override written there keeps the constant compile-time, keeps the name in an allowed file, and
  emits a perfectly valid string.
  **Demonstrated rather than argued, which is why it was worth an hour:** replacing the emitted `id` with the
  literal `"forged-not-this-tree+profile=…"` — an identity naming no tree in existence — left **all 5 tests
  in that file green and all 413 in the crate green, exit 0.** Fixed by asserting the join on all three
  fields (`id`, `source`, `dirty`), each poisoned red-first with its own message: forged id, a `vcs` build
  reporting `source: "declared"`, and an inverted `dirty`. The `implementation` join is recorded **in the
  test itself as not currently load-bearing** — the registry has one value, so the schema enum catches
  divergence first (measured: it fails at `common/schema.rs:485`) — and kept for when the registry grows.
  **The durable lesson, and it generalises past this file: a test per component and none across the seam is
  how a chain of individually sound links holds nothing.** It is bar 8's shared frame in its most seductive
  form, because here every individual check is genuinely strong, recently written, and correct — the
  strength of the parts is exactly what makes nobody ask what sits between them. **A seam has no author**:
  each test's writer was inside one component, and the gap was visible only from outside all three, which is
  the argument for a reader who did not write any of them.
  **The reciprocal bar, aurora's, banked against themselves and it guards the pair this fix created:
  *when a check looks redundant, name each of the two claims before collapsing them.*** Their proposal was
  to *replace* the `implementation` literal with the constant; taken, that would have deleted the registry
  pin while reading as a strengthening. The `assert_eq!` pair now sitting in
  `initialize_names_the_implementation_and_the_build` looks exactly like a duplicate and is not — literal
  pins the registry value, constant pins the join — so **a future tidy-up of that pair is the live risk this
  paragraph exists to stop.** The comment at the site says so too; this is the second copy on purpose,
  because a code comment is where a perishable rule goes to be read by nobody (this file's own 2026-08-22
  bar).


## [orig lines 1195-1315] F-REPLAY-READS-AEONS-BUILD, the whole arc, closed

**✅ F-REPLAY-READS-AEONS-BUILD — CLOSED 2026-08-30, merged `79b4c32`, suite 60/1975/0/6 on the MERGED
tree, fmt clean, clippy ×2 clean. The entry below is kept in full because TWO OF ITS INSTRUCTIONS WERE
WRONG, and how they were wrong is the reusable part.**

* **The recipe named ONE file; the coupling was in TWO crates.** `crates/oracle-core/tests/symbols_real_lst.rs`
  resolves the same `ORACLE_AEON_DIR` default and reads the listings, **both ROMs, and the demo pair** —
  six artifacts, not the four booked below. A four-file freeze would have left that test **silently
  skipping**, the exact failure this parcel existed to prevent. The replay file's own header says *"this
  mirrors symbols_real_lst.rs exactly"*, so the pointer sat in the text the whole time. **Bar 14: the
  consumer set is the enumeration, and prose naming one consumer is not a survey.**
* **⚑ THE BRIEFED REVISION WOULD HAVE FROZEN THE BREAKAGE.** Everything below says freeze sigil
  `dd371e3b` (chain 187, `aeon_rev ec6a4791`). **That ROM is byte-identical to aeon's 22:35 build — the
  one that reddened us** (`951cf960…62707d`, verified against the golden blob *before* dispatch).
  Measured, not argued: chain 187 gives **9 passed / 4 failed** with our code unchanged. Pinned **chain
  186** (sigil `5af70797`, `aeon_rev def98ee5`), the last freeze whose embedded fixture is coherent —
  **13 passed / 0 failed**. The agent tested the briefed revision before departing from it and reported
  the departure; **deviation ratified.**
  **The durable shape: a recipe can be perfectly specific, correctly cited, agreed by two lanes, and
  still name an artifact that does not do what the recipe wants — because it was written from the
  artifact's PROVENANCE (newest attested freeze) rather than from the PROPERTY it needed (a coherent
  fixture).** Nothing about it looked wrong. It named a revision, and the revision existed.
* **The open question is ANSWERED and is NOT ours** — aeon's stale fixture; mechanism at their
  `replay.emp:374`; booked aeon `0b612953`; all four checks re-verified firsthand here. Detail lives in
  `fixtures/aeon/PROVENANCE.md`, the artifact of record.
* **⚠ THE CONSEQUENCE THAT INVERTS THE REFLEX — DO NOT PIN A SUPERSEDE.** aeon's superseding freeze does
  **not** re-record the fixture (re-recording was unbooked until this question was asked), so a newer
  attested freeze **will still desync us**. *Wait for the attested freeze, then pin it* would reintroduce
  this exact red **with a fresher-looking revision attached**. The pin moves only on aeon's explicit
  signal that a coherent fixture exists.

**▶ F-REPLAY-READS-AEONS-BUILD, registered 2026-08-30 — OUR SUITE'S GREEN DEPENDS ON ANOTHER LANE NOT
REBUILDING, and nothing in either repo says so.** Found by chasing a red I assumed was mine.

`crates/oracle-replay/tests/replay_real_artifacts.rs:50-52` defaults to `/home/volence/sonic_hacks/aeon`
(overridable by `ORACLE_AEON_DIR`) and reads **aeon's live build products** — `s4.debug.bin`, `s4.bin`,
`s4.debug.lst`, `s4.lst` — pinning hashes derived from them. aeon rebuilt at **22:35:38 on 2026-08-29**
(parallax BG V-scroll clamp, section head clamp), so four rows now fail; `the_negative_control_trips_the_gate`
wants `490164326` and gets `221728870`.

**Established as foreign rather than assumed:** the same four fail identically on a **clean checkout of our
own `origin/main`** in a throwaway worktree containing none of the session's work.

⚠ **Do NOT repin.** Goldens never regenerate silently is the standing bar, and here it is worse than usual:
the repinned number would silently mean *"whatever aeon last built"*, a pin that cannot fail and therefore
detects nothing — bar 9's corollary, an instrument adopted as a gate after being bent to fit. **The fix is
ours**: either freeze our own artifact, or **skip LOUDLY** when the artifact's identity is not the one the
tests were written against. Loudly is the operative word — a silent skip and a pass are the same artifact
(bar 25, aeon's).

**The durable form, and it generalises past this file: a claim about a peer's file has a shelf life, and
here it arrives as a TEST DEPENDENCY rather than as prose.** The doc version of this bar is already in this
file; the test version has no reader at all, because nothing in either repo announces the coupling and the
red presents as local. Measured shelf life: about a day.
⚑ **aeon's sharpening, and it is the half worth keeping** (2026-08-30, banked by them): **prose claims
about a peer's tree get citation discipline in this suite — SHAs, `--stat`, committed revisions — and a
file path in a test constant gets NONE**, despite being the same claim with a stronger consequence.

**▶ THE FIX, shaped with aeon and NOT taken tonight (their freeze is mid-flight; anything pinned in the
next hour is stale again).** They offered exactly the identity this needs and it is worth recording in
full:
- **Read sigil's frozen goldens, never aeon's working tree** — `sigil/crates/sigil-harness/golden/`
  (`s4.bin`, `s4.debug.bin`, `demo*.bin`, …). Those are **committed artifacts**, changed only at a
  **freeze**: a deliberate, ritualised, recorded event, not the incidental rebuild that broke us.
- **`golden/provenance.toml` is the build identity** — 186 `[[entry]]` records, each with `name`, a full
  `aeon_rev`, an `ab` evidence field and per-target CRC sets. So a test can assert *"this is the ROM frozen
  at `aeon_rev` X"* rather than *"this hash"*, **and when it legitimately changes the entry says which
  parcel changed it and why. That is a pin that can fail for the RIGHT reason**, which is the property our
  goldens-never-repin bar exists to protect.
- ⚠ **Their own caveat, volunteered first: the goldens still move at every freeze** — a few times a day on
  an active night. Pinning them keeps a dependency on their lane; what it buys over the working tree is
  that every change is **announced, attributable and dated, in the same file as the artifact.**

⚑ **THE DIAGNOSIS SHARPENED BEFORE HANDOFF — IT IS NOT A STALE HASH CONSTANT, AND THAT CHANGES THE FIX.**
`the_negative_control_trips_the_gate` pins **nothing**: it reads `was` out of the ROM (*"never pinned: it
is the ring-0 hash of THIS build's curated state, and a re-record moves it"*) and asserts the trap reports
that same value back. It fails because **the replay fixture is embedded IN aeon's ROM** — `Replay_OJZ_Fixture`
is a symbol in it — so their rebuild moved the recorded stream our tests replay, not a number in our tree.
**Consequence, and it is the good one: freezing our own ROM copy freezes the FIXTURE with it**, because the
fixture travels inside the ROM. The plan works; it just works for a different reason than "re-pin a hash".
*(`replay_real_artifacts.rs:190` does pin two tick counts — `Ojz` 1721, `OjzSlide` 2350 — which are
ROM-derived and may need re-deriving once, with the cause named. That is the only genuine re-pin.)*

**▶ EVERYTHING THE NEXT SESSION NEEDS, verified here 2026-08-30 — aeon sent the trigger SHAs and they check
out** (all three reachable at their `origin/master`, subjects matching their description):
- **aeon `ec6a4791db346ec8c6672632109f85415b873e49`** · sigil freeze **`dd371e3bab16782318f803211072f6af9e7e79bc`**
  (*"freeze: scroll-and-section-clamps (chain 187), aeon_rev ec6a4791"*) · attest
  **`6d665688e9161bbc22f573badeee591e009c1312`**.
- **Take the ROMs as COMMITTED BLOBS, not from aeon's worktree**:
  `sigil dd371e3b:crates/sigil-harness/golden/s4.debug.bin` (736315) and `s4.bin` (719315). **Verified
  byte-identical to aeon's worktree** (`sha256 951cf9604f3249d7…` both), so freezing from the golden freezes
  exactly the build that reddened us — the tests will not move again from the ROM side.
- ⚠ **THE GAP THAT WILL TRIP YOU: the `.lst` listings are NOT frozen.** Only the ROMs are goldens. Our tests
  need `s4.debug.lst`, and it lives only in aeon's working tree, where it moves. **So a ROM-only freeze
  leaves half the coupling in place.** Solve that explicitly — freeze the listing beside the ROM as our own
  artifact — or the parcel is not finished.
- ⚠ **Chain 187 is FROZEN-BUT-UNATTESTED**: aeon's strict suite went **RED, 8 failures, 7 of them one
  cross-seam symbol**, and the attest commit says so in its own subject. A **superseding freeze is coming**;
  aeon expects the ROM bytes to be identical across it (the fix is in sigil's test compositions, not aeon
  source) but **would not promise it**, and has promised to say so unprompted if they move. **Freeze anyway
  — aeon's own argument, adopted: waiting for their chain to settle is depending on their cadence one last
  time, which is the exact property the freeze exists to end.** Record the unattested status in our
  provenance note so a later reader knows what they have.

**This seat's lean, recorded as a lean and not a ruling: freeze OUR OWN copy, and use `provenance.toml`'s
`aeon_rev` to record which revision we froze from.** That takes both properties — a pin that moves only
when *we* decide, plus full attribution when we do — and it is the only shape where our suite's green
stops depending on another lane's cadence at all. aeon said explicitly they would not argue against it and
will carry nothing either way. **Revival condition: aeon's in-flight freeze lands and they send the SHAs
(promised, copied to us).**

⚠ **Consequence for every aggregate this lane quotes from now until it is fixed:** `cargo test --workspace`
carries **4 foreign failures**, and a run reporting `FAILED=0` after 2026-08-29 22:35 is measuring
something other than what it claims. Report the aggregate with the foreign four named, never as a bare
total. *(Recorded because my own earlier greens tonight were taken before that rebuild and are not
comparable to later ones.)*

**RETRACTED 2026-08-30 — the freeze landed and this warning is now itself the stale claim.** The aggregate
is **60/1975/0/6, `FAILED=0` honestly**, on the merged tree. Kept visible rather than deleted, per the
supersession rule: it was correct for about six hours, and a reader meeting it cold needs to see that it
**expired**, not that it never applied.


## [orig lines 1321-1323] window-check rows fixed 2026-09-02: picker filter marker, rom-open doc, toast truncation

| **F-PICKER-FILTER-MARKER** | ✅ **FIXED 2026-09-02 (`f336658`, parcel/player-polish) — and the seam test derives its filter query FROM `LOADED_MARKER` minus the label's own letters, so it cannot agree with a stale copy of the marker.** The ROM picker's filter matches the `[loaded]` **decoration**: `rom_browser.rs:115` bakes the marker into the label (`format!("{}   [loaded]", …)`) and `Picker::visible()` (`palette.rs:58-62`) `subseq_match`es that composed string, so every letter of `l,o,a,d,e` is free for the already-open ROM and it survives filters that should exclude it. **Cosmetic — Enter still runs the correct visible row, proven on live data.** The instructive half: this is model-level, tests already assert over those labels (`rom_browser.rs:251`), and nothing asserts the **seam between the marker feature and the filter feature** — this morning's *a test that asserts what you added is blind to what you displaced* bar, one step out. | Next open of `rom_browser.rs`; two-line fix (match `entry.label`, render the marker separately) plus the seam test. |
| **F-ROMOPEN-C-DOC** | ✅ **FIXED 2026-09-02 (`e5f57c4`) — the CODE moved to the doc, not the reverse:** the listing now lives in `RomBrowser { dir, entries }`, survives a failed scan, and the picker re-opens on the retained entries beside the toast. `docs/2026-08-28-rom-open.md` §5 promises an unreadable folder *"leaves the previous listing up"*. It **dismisses the picker** instead — `open_rom_picker` early-`return`s before `open_picker` (`main.rs:494-501`) while Enter has already closed the palette. Safe and loud (toast names the path, ROM unchanged, player alive), but the doc's own acceptance criterion is not met. | Correct the doc at the next pass; optionally re-open on the previous directory before notifying. |
| **F-TOAST-TRUNCATES** | ✅ **FIXED 2026-09-02 (`5d2978d`).** `fit_marked` reserves the mark's own width; reasons precede paths; the trailing ` (os error N)` is stripped as redundant next to the text. `notify_err` toasts cut from the right with no ellipsis and **lose the reason**: `open ROM: cannot read {dir} ({e})` rendered as `…/LOCKED (PE`, dropping `Permission denied`. The path survives, the reason does not — and the reason is the half a person needs. Today's SCREEN-HONESTY parcel fixed exactly this on the status line; toasts were out of scope. | Any parcel touching toast rendering — and assert on the **whole** rendered string, per this file's own 2026-08-29 bar. |

## [orig lines 1325-1325] window-check row fixed 2026-09-02: font glyphs

| **F-FONT-BACKTICK** / **F-FONT-EMDASH** | ✅ **FIXED 2026-09-02 (`d845e1e`) — four glyphs, not two** (tilde and ellipsis came out of the sweep). Durable half: `every_string_literal_the_frontend_can_show_is_drawable` lexes the literals out of each module's production region rather than restating a list; **red-first at 62 undrawable literals.** The player has **no glyph for `` ` `` or `—`**, so `Canvas::text` substitutes a hollow box. Its own **first toast contains a backtick** (`main.rs:1207`) and six live toasts carry em dashes — the window has been showing boxes at the owner all along. `font.rs`'s guard test restated its own input, so it could not catch either. **Verified under a positive control** (`'A'` present, both absent). **Now ASSERTABLE rather than merely known**: `screen_text`'s `unrenderable[]` is the instrument, and the glyph tests carry premise assertions that fail loudly if the font ever gains these characters. | Any parcel touching `font.rs`. Adding the two glyphs is a few table rows; it was deliberately NOT bundled into the `screen_text` parcel, which made the defect observable rather than fixing it. |

## [orig lines 1327-1327] window-check row fixed 2026-09-02: schema reads live empyrean

| **F-SCHEMA-READS-LIVE-EMPYREAN** | ✅ **FIXED 2026-09-02 (`parcel/stopprecision`, `7308e967`) — to the shape the suite ratified an hour later, `empyrean/contract/SUITE_PATHS.md` at `38f6df4`, which cites this finding by name.** The walk from `CARGO_MANIFEST_DIR` to a peer's live working tree is gone. `schema_conformance.rs` now hashes the vendored bytes as a **git blob** against `pin.blob` in `PROVENANCE.md` (step 0, never skipped, needs no peer); `$AETHER_CONTRACT_SCHEMA` (a file) and `$AETHER_CONTRACT_REPO` (a checkout, read only through `cat-file`/`rev-parse`/`merge-base`) confirm the pin against the contract repo; and with neither set the run prints a banner naming both variables and both halves — **no walk**. The resolver prints which step answered first. `PROVENANCE.md` now pins revision + blob + bytes as parseable markers, which was the other half of this row. Red-first four ways (appended byte, one-byte edit at constant length, a repo without the revision, a deleted pin marker); all three steps also exercised green. Findings: `cat-file blob` ALONE is nearly vacuous — pointed at THIS repo it passes, because vendoring put the same blob here — so the revision + ancestry checks are what make step 2 mean anything; and a default run no longer notices upstream moving on its own, a deliberate trade recorded in `docs/2026-09-02-stopprecision.md` §6. | — **↪ ANSWERED 2026-09-02, and my framing was wrong: this is the suite's DEFAULT shape, not ours.** The hub's first enumeration searched for gates reading a contract FILE and found only ours; **sigil corrected it within the hour** — the population to enumerate is *what READS a peer's tree, not what NAMES one*, and by that measure it is everywhere: sigil `test_support.rs:601` `LIVE_TREE_FALLBACK` behind 247 `aeon_dir()` call sites plus three committed scripts (two on active systemd timers), aeon `test.sh:286`, and **our own `crates/oracle-core/examples/common/rom_source.rs:44` `LIVE_AEON_DIR` and `tools/aeon_pin_report.py:145`** — both confirmed at our tip here. Aurora and seraph clean. The resolver case carries a hazard the contract-file case lacks: **the revision moves under a single run**, so a pass is attributable to whatever the tree happened to contain. **Ratified fix shape** (`contract/SUITE_PATHS.md`, empyrean `38f6df4`, verified an ancestor of their `origin/main`, and it cites this finding by name): read the peer **through git objects at a named revision, never the working tree**; vendored bytes **hashed against a blob pinned in a provenance sidecar**; re-vendor via `git -C <peer> show origin/<default>:<path>`; an env-var override is legitimate and **its absence is a loud skip naming the variable, not a walk**. Precedent: aurora's `test/formats/effects-preset-schema-drift.test.ts`. **⚠ BUILD-TO RULE for the resolver, banked before we write one** (`contract/SUITE_PATHS.md` step-3 bullet, empyrean `a0b4251`, verified an ancestor of their `origin/main`; sigil's, learned from a merged tree going **6-of-4198 RED while both branch sweeps were green**): `git rev-parse --git-common-dir` returns **three** shapes — `.git` at a main checkout's root, an **absolute** path from a linked worktree's subdirectory, and **`../../.git` (relative, with `..`) from a MAIN-checkout subdirectory**. Sigil trimmed the third lexically, walked onto `crates/`, and refused. The failure is invisible to agents *because* agents run in worktrees and the suite runs from the main checkout — the two return different shapes, so a bed-only proof proves the wrong configuration. Therefore: **ask git for the format you want (`--path-format=absolute`), never normalise its answer**, and prove the derivation from **BOTH** the constructed worktree bed and the real main checkout. **Measured here 2026-09-02, not assumed: our tree has ZERO `--git-common-dir` call sites** (`git grep -c` exit 1, with a positive control on `rev-parse` returning matches in 3+ files), so nothing of ours is affected today — this is the shape to build to, not a defect to fix. |

## [orig lines 1332-1374] the sigil relink check, and the residue retracted within the hour

**⚑ SIGIL RELINK 2026-08-30T00:33:36Z — CHECKED AGAINST OUR CORPUS, AND WE ARE IMMUNE BY CONSTRUCTION.**
sigil broadcast that the shared `sigil/target/release/sigil` was relinked to their master `85a5726c`
(19 crate-commits stale beforehand), carrying a placement-path retirement (`0ab72a5a`) and a region
end-contract change (`821cbbf1`), with the standing ask: *pin figures to the revision they were exported at
rather than assuming they still reproduce.* **Verified here rather than assumed, because this lane does hold
sigil-derived figures** (the §11.5 short-row work, the 180.0/154.0 hand derivations): the corpus artifacts at
`/home/volence/sonic_hacks/corpus-rom-d22dda85/` are intact, `s4.debug.bin` still hashes to its recorded
`ad289eae947b2dd4…`, and sigil's golden **at the PINNED revision** `7b46f075` (blob `633f5f88…`) hashes to
the same value. A relinked binary cannot move either — **a pinned blob equals itself by construction**, which
is the whole reason those were frozen as artifacts instead of kept as a recipe.
⚠ ~~**The one residue: the `.lst` REBUILD RECIPE is now unverified.** §1.3's pinned-toolchain recipe claims
*"a rebuild that reproduces those bytes proves the listing beside it is the corpus listing"* — a standing
claim about a binary that has just changed underneath it. Revival condition: anyone about to re-run the §1.3
rebuild.~~ **RETRACTED WITHIN THE HOUR — THERE IS NO RESIDUE, AND THE RETRACTION IS THE INSTRUCTIVE PART.
Original kept visible per this repo's supersession rule.**

**The recipe was ALREADY pinned by exactly the move that saved the artifacts, and I would have known that by
opening it.** `corpus-rom-d22dda85/PROVENANCE.md` §2 builds sigil from a **worktree at `7b46f075` into a
scratch `CARGO_TARGET_DIR` outside both repos**, then passes `SIGIL_BUILD=<scratch>/sigil-target/release/sigil`
— **it never references the shared `sigil/target/release/sigil` at all**, so a relink of that binary cannot
reach it. Better still, the document had already anticipated this exact class in prose: *"the **current**
sigil refuses with `[map.undeclared-island] ROM section at 0x99F0` … and **is why sigil is pinned to
`7b46f075` rather than to its head**."*

**My error, and it is one this file has a bar for: I asserted an exposure from OVERSEER.md's own ONE-LINE
SUMMARY of the recipe (*"the §1.3 pinned-toolchain recipe"*) without opening the recipe — which lives in
PROVENANCE.md, not here.** *Read the artifact, not the story.* The word **pinned** was sitting in the
summary I was reading, and I wrote a hazard notice about the thing it names being unpinned.

⚑ **And the half that makes this worth keeping rather than deleting: I exported the wrong claim to sigil,
and they reasoned soundly on it and sent back a correction that was RIGHT ON THE RULE AND WRONG ON THE
INSTANCE** (*"the claim is now false-by-default rather than merely unverified … a recipe pinned to a
revision degrades into a historical note; an unpinned one degrades into a wrong instruction"*). **Their rule
is good and is adopted below. It simply does not bite here, and it could only mislead them because my
premise was wrong.** This is the delegation corollary arriving between lanes: a confident mechanism from
this seat overwrote a question they had no way to check, and the sound reasoning they applied to it made the
error look corroborated rather than caught. Same shape as the 52-method circuit — *a claim of ours came back
wearing a peer's confidence.*

**ADOPTED FROM SIGIL, as a general rule this lane owes its docs:** a recipe carried in prose either **names
the revision it was true at** or it degrades into a wrong instruction rather than a historical note. Our
corpus recipe already satisfies it — by construction, not by intent — and that is the shape to copy.


## [orig lines 1615-1736] cutover mechanism, precondition met, day-one breakage, the server-identity CR

**MECHANISM, measured — three of the relayed observations were wrong:**
- `/run/user/1000/oracle.sock` **does not exist**. `oracle-aether` is **not running** (binary exists;
  the reported pid did not). The **only** listening oracle socket is
  `/tmp/oracle-harness-4av2i47x/oracle.sock`, held by the **legacy GUI** under a **private
  `XDG_RUNTIME_DIR`** — harness-local, not on any normal session's chain. **There is no incumbent to
  displace; the shared socket is empty**, so nothing is reachable at `mcp__oracle__*` today at all.
- **The registration is not only `~/.claude.json`.** Live sessions carry `--mcp-config` on their own
  command line, which **overrides it**, and they disagree: the user config points at
  `oracle-old/...` (correct), while at least one live session points at
  **`oracle/linux-port/mcp/oracle-mcp`, a path that does not exist.** **This is the rename fallout's
  FOURTH consumer class and it is invisible to a config audit** — the file is correct while a
  session started under a pre-rename override is permanently broken. *This session is one of them*:
  my `emulator_status` returned `Errno 2` and I first read it as "no emulator running".
- **Split of hands:** running the server is **ours** (cheap, and reversible by stopping it — the
  socket returns to empty). The **config/session correction is the OWNER'S** — a peer relaying an
  owner instruction never authorises this seat to edit `~/.claude.json` or a session's MCP config.

**✅ PRECONDITION MET 2026-08-22 21:52 — REBUILT AND VERIFIED BY RUNNING IT, NOT BY GREPPING SOURCE.**
Foreground, on a private socket, nothing shared disturbed; server stopped and socket removed after.
**Over the wire, from `initialize`: `methods` = 41**, with `step`/`step_over`/`step_out`/
`run_to_scanline` all present and `write_vram`/`breakpoint_add` absent. Server banner agrees
(*"aether: 41 methods advertised"*). **Five items tagged ⟨RUNTIME⟩ and unrun in CR-C are now RUN:**
- `serverName` = `"oracle-next"` **on the wire** — the config default, exactly as CR-C argues.
- `serverVersion` = `"0.0.0"` **on the wire** — confirming it is a pinned literal, not a slow-moving
  version. CR-C's P1 demonstration is now measured rather than reasoned.
- `status.romPath` = `"../aeon/s4.debug.bin"` — **relative, confirming BOTH aurora's observation and
  our own `protocol.md:1799` SHOULD-violation**, firsthand and on the wire.
- **An unserved method returns `-32601` naming it** (`"no such method: emulator/write_vram"`). **The
  loud-failure property the cutover ruling depends on is now demonstrated, not asserted.**
- Bonus, unplanned: my first handshake was malformed and the server **refused it by name with the
  reason** — *"`protocolVersion` must be an integer (D5)"* — at the dispatch choke. The strictness
  argued in bar 15 caught its own overseer within a minute of being pointed at it.
*Method note: my first two attempts returned `0 methods` and `serverName: None`, which looked exactly
like a broken server. It was a malformed request. **An empty result is not a finding** — printing the
raw reply instead of inferring from the empty parse is what separated the two, and that is bar 16(d)
paid for a third time today.*

**⚠ HARD PRECONDITION — ~~REBUILD BEFORE CUTOVER~~ (MET ABOVE; kept for the reasoning). Found by the aurora lane, and it would have shipped
a lie to every consumer.** `target/release/oracle-aether` is dated **21 Aug 22:11** while
`engine.rs` is **22 Aug 20:31**; aurora ran the binary and it **banners "37 methods advertised"**.
Source serves **41**. So the built artifact **predates the step trio AND `run_to_scanline`** and would
present four served methods as absent. **A consumer measuring our surface against an installed binary
gets the old answer with nothing announcing it** — bar 4's converse arriving from the opposite side:
there the artifact could not witness its own freshness, here it cannot witness its own staleness.
*Corroborates the 37→40→41 correction from an enumeration parameter neither source-side derivation
had: stale RUNTIME behaviour rather than a grep.*

**DAY-ONE BREAKAGE — measured, replacing the "breakpoints and Z80" guess:**
- ~~**Breakpoints: YES, and it is the whole exposure.**~~ **⚠ REFUTED 2026-08-24, see the
  cutover handoff. NO day-one breakage exists.** aeon's gates spawn their **own** emulator on their
  **own** socket (`launcher.py:11` → the **legacy C++** `oracle_gui`, `mkdtemp`, isolated
  `XDG_RUNTIME_DIR`); 9 gate files, 0 dialing a shared socket, no default-socket `BusClient()` in
  their tools. **Inferred from which methods the gates call, never from which server answers them** —
  the invocation-chain joint this repo booked on 2026-08-22 and then broke on its own headline claim,
  under the word *measured*. **Breakpoints have no consumer today.** Original retained below. aeon's `raster_source_gate.py` and
  `snapshot_poison_gate.py` run **arm → wait → clear** (`breakpoint_add` → `wait_for_break` →
  `breakpoint_clear{all}`) — three of the 17, one flow, **at least one path an unattended nightly**.
  Cannot migrate piecemeal: serving `wait_for_break` alone leaves them nothing to arm.
- **Z80: NO.** CR-B's sweep found **zero programmatic consumers**; 48 mentions, all prose.
- **`ping`/`log_clear`: no impact.** The remaining 12 have no consumer of any kind.

**SEQUENCING (my call, stated to empyrean):** cut over now; the breakpoint trio + `wait_for_break`
ship as **ONE parcel** alongside it. **The collision to see clearly:** that parcel's design is
**CR-A, which is UNADJUDICATED** because the Fable seat is held — so cutting over now means building
to an unadjudicated contract. **Resolution: build it and BOOK it.** The hold ruling arrived *with*
the ledger obligation precisely so such decisions are auditable when the limit lifts; L-01 is already
CR-A. Building under an unadjudicated CR **and recording it** honours both rulings — building it and
not recording it honours neither.

**⚑ THE SCOPING CHANGES AGAIN — the aurora lane, and this one is a suite-wide correctness hazard, not
a cutover detail.** **The MCP shim and the Aether server are INDEPENDENT, and only the shim is
config.** `mcp__oracle__*` runs **oracle-old's Python shim**, which is a **client**: it dials the same
socket chain everything else does. So **a legacy shim has been driving our Rust core, end to end,
today, unknowingly** — nobody had ever tested that pairing and it works.
**Their proof, which is the good kind:** they launched our Rust binary with a **relative** argv
(`../aeon/s4.debug.bin`) and `emulator/status` echoed `romPath: "../aeon/s4.debug.bin"`, while the
C++ `oracle_gui` on this machine carries the **absolute** path. Only one process could have produced
that reply. *That also certifies their two other reports as genuinely OUR binary's behaviour: the
`write_vram → -32601` and the "37 methods" banner.*
**In our favour:** the client half may already be done — the cutover's client-side cost is not "swap
every lane's shim".
**⚠ AGAINST IT, AND THIS IS THE ONE TO CARRY: WHICH SERVER ANSWERS IS DECIDED BY WHOEVER LAUNCHED ON
THE SOCKET CHAIN FIRST — NOT BY ANY CONFIG.** So **a session can silently change which
implementation it is talking to, with no config change and no signal.** Aurora's own framing, against
their earlier one: they had assumed a swap required somebody to change something. It does not. Every
measurement any lane takes carries an unstated assumption about which implementation answered, and
**nothing in the transport makes that assumption checkable.**
**▶ NEXT PARCEL, and it is the counter-measure: SERVER IDENTITY + BUILD PROVENANCE in `initialize`.**
Today we advertise `serverName` (**config-supplied**, so it proves nothing an impostor could not
claim) and `server_version` = `CARGO_PKG_VERSION` (**a crate version that does not move per commit**,
so the 21-Aug binary and the 22-Aug binary are indistinguishable by it — which is exactly the pair
that differed by four served methods). The advertised `methods` list *is* currently the only real
discriminator, and reading it requires a repo to diff against. **Embed the git SHA + build timestamp
at compile time and surface them**, so staleness and identity are **self-announcing rather than
inferable**. This single parcel closes BOTH of today's independent findings — the silent-swap hazard
and the stale-binary hazard — and it is the handshake record aurora raised with empyrean. Needs a CR
(new `initialize` keys are contract surface; **do not invent them unilaterally**).
**THREE REQUIRED PROPERTIES, to be written into the CR as PROPERTIES rather than mechanisms** (the
third is aurora's and is the one a later refactor will quietly break):
1. **`implementation` distinct from `build`** — they fail independently. The 21-Aug and 22-Aug
   binaries differ by four served methods and are **identical** under `CARGO_PKG_VERSION`.
2. **Unforgeable-by-config** — `serverName` is config-supplied today and proves nothing an impostor
   could not also claim, so a value read from config would reproduce the exact defect it fixes.
**⚑ BOTH EXISTING IDENTITY FIELDS ARE CONFIG-OVERRIDABLE — verified at HEAD, and this is the CR's
working demonstration rather than a hypothetical.** `server_name` is a **config struct field**
(`engine.rs:150`) with a **hardcoded default** `"oracle-next"` (`:190`); `:1344`/`:1345` ship
`self.config.server_name` **and** `self.config.server_version`. aurora's agent called it a hardcoded
literal and I called it config-supplied; **both readings are true and the reconciliation is the
point** — *it is already a config value that happens to be unset.* Today it discriminates the two
servers perfectly (legacy answers `"oracle"`/`"2.1-linux"`), **which is exactly what makes it
dangerous**: a lane testing `serverName == "oracle-next"` stays green for months and **inverts
silently the day anyone sets that config.** So `serverName` must NOT be the identity field however
well it would pass today's tests, and `serverVersion` cannot rescue it (`CARGO_PKG_VERSION` does not
move per commit). *aurora's client leans on `methodCount` and stores `capabilities` raw rather than
branching — a workaround for precisely the gap this parcel closes.*
3. **⚑ STRUCTURALLY EMITTED, NEVER SELF-REPORTED.** The check that actually worked today made two
   candidate processes **emit different observable output** — a *relative* argv echoed back through
   `status.romPath`, which a server can neither fake nor be wrong about **because it never chose the
   value**. A *reported* build identity is a self-report again, merely better-sourced. Compile-time
   embedding is right precisely because **the binary cannot have an opinion about it** — so state the
   property, or a later "read it from a generated config at startup" refactor reads as a tidy-up
   instead of the regression it is. **Aurora reviews this CR as the consumer; ping them when drafted.**

## [orig lines 1752-1849] there is no socket chain

## ⚑ THERE IS NO SOCKET CHAIN — measured 2026-08-24, and it corrects all three lanes at once

**Three sessions spent an evening reasoning about "the chain". Nobody had read the function that
defines it.** Bar 8's cheap frame-changer exactly: the load-bearing step nobody cited.

`empyrean/clients/python/aether.py:36-48`, `resolve_socket_path()` — the transport **the oracle MCP
shim actually uses** (the shim is a 26-line `/bin/sh` wrapper that re-execs
`oracle_mcp.py`, which imports `BusClient` from empyrean's reference client via
`parents[3]/empyrean/clients/python`, overridable by `$EMPYREAN_CLIENTS_PYTHON`):

```python
env = os.environ.get("ORACLE_SOCKET") or os.environ.get("EXODUS_SOCKET")
if env: return env
xdg = os.environ.get("XDG_RUNTIME_DIR")
if xdg and Path(xdg).is_dir():        # <-- tests the DIRECTORY, not the socket
    return f"{xdg}/oracle.sock"
return "/tmp/oracle.sock"
```

**The guard tests whether `$XDG_RUNTIME_DIR` is a directory, never whether the socket exists.** On any
normal login `XDG_RUNTIME_DIR=/run/user/1000` and that directory always exists, so the function
**returns `/run/user/1000/oracle.sock` and stops. `/tmp/oracle.sock` is unreachable dead code for
every lane.** There is no chain and no walk; there is one path, chosen on a directory test.

**⚠ ~~THE DOCSTRING DIRECTLY ABOVE IT IS WRONG~~ — INVERTED BY EMPYREAN, AND THEY ARE RIGHT.
Correction kept visible over the original per this repo's supersession rule.** I reported the
docstring as a stale comment using fallback vocabulary for code with no fallback. **The docstring is
FAITHFUL; the CODE is the divergence.** Verified firsthand at empyrean `origin/main` `21af8b3`,
`contract/protocol.md:1896` §7.1: *"path `$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock`, client
override `$ORACLE_SOCKET` (the legacy `$EXODUS_SOCKET` / `exodus.sock` are still honored as a
**transitional fallback**)"*. The arrow is a fallback arrow **in a sentence that uses the word
"fallback" explicitly** for the legacy pair, so the reading is not strained. **The spec specifies a
chain and the reference client never implemented one.** That makes it a **conformance gap**, not a
stale comment, and it inverts who owes the fix: under this repo's own rule tools conform to the spec
and never the reverse. *My error was the exact class I spent the evening correcting in others — a
correct observation with the wrong cause attached — and it was load-bearing, because "wrong comment"
gets a one-line fix while "unimplemented spec clause" gets a CR.*

**⚑ AND THE FIX WAS CORRECTLY NOT TAKEN TONIGHT, on evidence from this lane.** Making the resolver
actually walk would hand every consumer **ECONNREFUSED off the stale `/tmp/oracle.sock` corpse** where
they currently get a clean **ENOENT** — silently inverting the very discriminator that explained this
session's `Errno 2`. A live behaviour change to the transport all six lanes' tooling sits on. empyrean
landed docstring honesty with a **KNOWN DIVERGENCE** block and queued **which side moves as a CR**.
Correct call: either direction is a contract change.

**~~The original finding, retained because the observation was sound and only its cause was wrong:~~**
`aether.py:39-40` and the shim's own header (`oracle_mcp.py:17-19`) both read *"…then
`$XDG_RUNTIME_DIR/oracle.sock`, then `/tmp/oracle.sock`"* — the vocabulary of a **fallback chain**,
which is what all three lanes then reasoned about. The code has no fallback. **This is a live defect
in empyrean's reference client, not ours, and it is worth more than the cutover question it came out
of**: every Aether consumer in the suite reads that docstring to learn the transport.

**What each of us had right and wrong:**
- **aurora, right on the fact and wrong on its consequence.** `/tmp/oracle.sock` genuinely exists and
  is genuinely stale (socket file, 20 Aug 01:55; `ss -lx` shows one listening oracle socket on this
  machine and it is not that one). But it is **not** "the last link of the chain my client walks" for
  the Python client, because that client never reaches it. Their ROADMAP-36 question — *does it probe
  by existence and commit to a dead path, or attempt connect and fall through?* — is **answered for
  the reference client: neither. It commits on a directory test.** *Their own probe used `[ -S ]` and
  reported a corpse as a server; they caught it themselves and booked it. The measurement stands and
  the inference from it does not.*
- **empyrean, right that I over-claimed, right about the live GUI, and right about the cause of its
  frozen argv.** Verified here: `gui.log` heartbeat at 21:47 tonight, `system_running=0`; and
  `/proc/580713/cwd` resolves to **`/home/volence/sonic_hacks/oracle-old`** while its cmdline names
  `oracle/linux-port/...`, confirming their pre-rename hypothesis and that the thing holding a socket
  is the **legacy C++ core**. Their inference that the config binding is evidence about the *server*
  does not survive: it is evidence about the *client*, which is aurora's point and is measured.
- **mine, the over-claim they named.** *"No socket in `/run/user/1000`"* became *"the socket chain is
  EMPTY"* became *"no lane can reach any emulator"*. The conclusion is true and now mechanically
  proven; **the scope in the middle step was asserted, not measured.** Correct statement, narrower and
  stronger: **the single path every lane resolves to has nothing on it.**

**MY OWN `Errno 2`, NOW EXPLAINED BY MECHANISM RATHER THAN INFERENCE.** The resolver returns
`/run/user/1000/oracle.sock`; that file does not exist; `open_unix_connection` raises **ENOENT**. A
stale-but-present socket would have raised **ECONNREFUSED**. The error code was the discriminator all
along, and it points at link 3 — which is why the reachable-but-dead `/tmp/oracle.sock` was never
implicated.

**▶ F-CHAIN-QUOTED registered, from aurora turning the derived-never-copied bar on themselves.** They
found their own `OVERSEER.md` and session memory both publishing the four-step chain **in the
docstring's exact vocabulary**, with nobody having read their own resolver — so the chain this lane
has been quoting to peers for months may be a quotation rather than a measurement. **This tree has
the same exposure and it is registered, not audited tonight**: `docs/2026-08-14-tooling-frontier-recon.md`
and `docs/plans/2026-08-14-tooling-track2-overnight.md` both name the socket paths and neither was
written from the resolver. Revival condition: before any doc here is cited to a peer as the transport's
behaviour. *Scope note, honestly: I am declining to sweep two historical recon docs at 2am rather than
pretending they are clean.*

⚠ **And one instance against this seat, caught by the same check.** I called aurora's client a **Rust**
client in a message tonight. It is **TypeScript** (`src/main/aether/`, Electron main), and **this repo
already knew** — `docs/2026-08-22-cr-c-server-identity.md:143` names their `socket-path.ts` by file,
written six days ago. *I had the correct fact in my own tree and asserted from memory instead of
reading it.* Nothing in the tree needed correcting, which is the part that makes it worth recording:
the error existed only in outbound mail, where no later reader would ever meet the contradiction.
**That line also settles a real question:** aurora resolves through **their own** `socket-path.ts`, not
through empyrean's Python reference client, so tonight's finding does **not** transfer to them and
their item 36 is genuinely open.


## [orig lines 1855-1904] the shim half does not need him either

## ⚑ THE SHIM HALF DOES NOT NEED HIM EITHER — measured 2026-08-24, and it closes the cutover question

**empyrean raised the right challenge and it failed.** Their caution, carrying aurora's split: the
cutover is *two* changes, the server half is a socket-chain race needing nobody, but **the shim half
is config on the process command line and needs a full session restart, not a `/clear`** — so a card
telling the owner "it needs nothing" would be wrong in the direction that costs most, because he
stops looking and the half that needs him never gets done. **The principle is correct and is kept.
It does not trigger here, and that is now measured rather than argued.**

**A shim swap is only needed if the shim cannot reach something the server serves.** Diffed the two
surfaces directly:

- **Our served set = 41** (`[A-Za-z0-9_]` class on `"emulator/*"` literals in `engine.rs`: 44 hits
  minus the three events `stopped`/`resumed`/`romReloaded`). Matches the over-the-wire 41 at
  `12cc17e` exactly, from a different enumeration parameter than the handshake used.
- **The legacy shim exposes 63 MCP tools.**
- **`comm -23 served shim` = EMPTY.** Every one of our 41 is reachable through the shim already.
- `comm -13` = **22 tools that will return `-32601`**: the 16 schematized-unserved the shim carries
  (`ping` is the 17th and the shim exposes no tool for it) plus the 6 unschematized
  (`call_stack`, `log_tail`, `object_list`, `object_slot`, `player_state`, `z80_registers`).

**So the shim needs no swap, therefore no restart, therefore the shim half needs him no more than the
server half does.** The three-way decomposition, all four legs measured this session: (1) which server
answers — a race, and **the chain is empty**, so no contest; (2) which shim runs — config needing a
restart **in general**, moot here because the installed shim already covers our whole surface;
(3) broken shim paths from the rename — real, but the only live offender is a session in `~/work`
from 08-19, **not a suite lane**.

**⚠ CORRECTION TO THIS DOCUMENT, made against its own text.** The cutover section above says *"This
session is one of them"* — a Dominion-launched lane whose `emulator_status` returned `Errno 2`, read
as a broken `--mcp-config` shim path. **Wrong for a console-launched lane.** This session (PID
1905622) carries **no `--mcp-config` override at all**, inherits `~/.claude.json`, and that shim file
is **present**. The `Errno 2` is the **absent socket** (`/run/user/1000/oracle.sock` does not exist,
no `oracle-aether` running). A process sweep found exactly two overrides live: one correct
(`oracle-old/...`, empyrean, 08-21), one broken (`oracle/linux-port/mcp/oracle-mcp`, `~/work`, 08-19,
not a lane). *The original reading was a correct observation with the wrong cause attached, and it
was one turn from being repeated to the owner as fact.*

**Residue, stated rather than glossed:** name-for-name coverage is not param-for-param coverage. The
shim sends legacy `snake_case` on the 4 known param conflicts (`fftSize`, `maxHz`, `timeoutMs`,
`maxFrames`) — and **all four sit on methods we do not serve** (`audio_spectrum`, `wait_for_break`,
`call_stack`), so none can bite today. That leans on the 25-conflict enumeration being complete; if it
missed one on a served method the call fails `-32602` **naming the key**, which is loud. **Worst case
is a loud failure, which is the property the whole cutover ruling rests on.**

*Method note: this is what the challenge was for. empyrean neither adjudicated nor relayed it as
settled, and said so; the finding is stronger for having been attacked than it was when I first
reported it, and I could have been wrong — a single method in `comm -23` would have handed the shim
half straight back to him.*


## [orig lines 1977-2016] the LAYER-MASK parcel's resume path; it landed

## ▶ IN FLIGHT — LAYER-MASK parcel, resume path (written 2026-08-26 for a relaunch)

**Branch `parcel/layer-mask`, tip `28a5f5f`, base `903a08f`, worktree clean — every commit is in git, so
a killed agent costs only an unfinished gate re-run.** Serves `emulator/get_layer_states` +
`emulator/set_layer_enabled`; both fragments were final upstream, so it is conformance with no contract
fork. Takes the acceptance delta **18 → 16**.

Commits: `311e077` core (`LayerMask` as a *parameter*, never a field) · `f565d9d` the two handlers +
`tests/layers.rs` · `e6d61c2` fixture strengthening · `09ce88f` handoff doc · `bfa64a7` screenshot-path
fix · `6b642e3` the state_hash disclosure (the overseer review finding) · `28a5f5f` docs.

**Design calls, all RATIFIED at review** — read `docs/2026-08-26-layer-mask.md` before touching any of
them: a masked layer is removed from **candidacy**, never blanked afterwards (blanking paints backdrop
over dots plane B was visible at); shadow/highlight is computed from the **unmasked** pixels, so masking
plane A reveals plane B in the colour it already had; masking `window` falls through to **plane B, not
plane A**, because they share the slot and substituting A would synthesise a picture the hardware cannot
produce; a masked layer is **absent** from `pixel_attribution.candidates` rather than given an invented
verdict; `state_hash` hashes at `LayerMask::ALL` **and now discloses that in its caveat**.
**⚑ The parcel's central safety property is structural, not tested: `render_scanline` — the one render
that commits sprite-overflow/collision latches and the R10 carry — takes no mask and has no masked
twin, so "a display mask cannot perturb emulation" is enforced by the type system.** Do not add a mask
parameter to it to "finish" anything.

**WHAT IS LEFT, in order.** (1) Merge to `main` and re-verify on the **merged** tree, never the branch
side: fmt, clippy `--workspace --all-targets` ×2, and the aggregate. (2) **Run the aggregate several
times** — the agent saw ONE unreproducible `FAILED=1` early on, lost the name to an `awk` pipe, then had
two clean full runs and 25 clean repeats; it is an **open flag, not a closed one**, and nothing may be
weakened or serialized to make it go away. (3) **⟨RUNTIME⟩ the parcel has never touched a running
machine** — spawn a lane-owned `oracle-aether` on a short `/tmp/<short>` socket (never the scratchpad
path: `SUN_LEN`) and drive both methods; **do NOT use `mcp__oracle__*` while the stale-shim hazard
stands**, since it reaches the owner's player. (4) Rebuild `target/release/oracle-aether` — a merge does
not rebuild it and the consumer spawns that binary. (5) **Only then tell aurora**, who registered as the
first consumer on exactly that condition.

**Known gap, named not hidden:** the player GUI has no layer-mask surface — `oracle-frontend` draws its
own window and `pick.rs` calls `pixel_attribution` unmasked, so a bus-set mask changes the bus's answers
and not the window's. `pick.rs`'s "this panel and `emulator/pixel_attribution` must never disagree"
invariant is now conditional on no mask being set. A GUI toggle plus a masked `pick::resolve` is the
follow-up, and it is the natural home for the owner's unqueued *click an object and be told what it is*.


## [orig lines 2067-2125] the three parcels of 2026-08-30

## ▶ 2026-08-30 — THREE PARCELS LANDED, ALL PUSHED (read this first after a /clear)

Everything below in the restamp and REPLAY-NET-BLIND-3 sections is **DONE**; they are kept for the
method, not as open work. Landed in order, each verified firsthand on the merged tree before push:

1. **The replay net can see again** — `857d55e` (log `e876f10a`). Pin moved aeon chain **186 → 189**
   across all six artifacts; ROMs from sigil's committed goldens at `39c34fd2`, four listings from an
   aeon worktree at `aeon_rev 3f143178` supplied by that lane. The three `#[ignore]`d playthroughs
   became `#[cfg_attr(debug_assertions, ignore)]`, so **release runs them with no flag to remember**
   (`tools/replay_playthroughs.sh` + CI job `replay-playthroughs`, 8.43 s / 16 tests). Non-gating
   currency reporter `tools/aeon_pin_report.py` (asks at TIP, per-file rows, loud UNMEASURABLE).
   `the_standing_fixture_runs_green`, which FAILED that morning, passes.
   ⚑ **The release `s4.bin` carries the fixture stream too** — nine stale payloads at
   `$0A4A2C…$0A4A80`, measured. The pairwise fallback would have shipped a stale release ROM while
   reporting a clean debug one. **The convenient answer was false in the direction that does not
   announce itself.**
2. **The last live-tree readers** — `2aa1704` (log `5693e94`). Eleven executable sites, **two found
   only by the identifier grep and not the path grep**. Rule for unfrozen artifacts: repoint where a
   frozen copy exists, else keep the live default and **announce it at startup**. `s4.soundtest.bin`
   BLOCKED (absent from sigil's goldens); `demo.bin` freezable, declined as a reversible judgement.
   ⚑ **Four pinned expectations had ALL gone stale** (`symbolCount 2129→2310`, `romBytes
   696836→719315`, `Player_1 0x00FF8CFA→0x00FF8E48`, raw `0xFFFF8CFA→0xFFFF8E48`) — the comment above
   two of them reads *"D7: resolve, never hardcode."* They are now derived from what the server says
   it loaded, so they cannot rot on the next pin move.
   ⚑ **And a check that could not fail, three lines from one that failed every run.** `screenshot`
   moved PPM→PNG; one check never followed, and its companion counted distinct bytes of a
   **compressed** stream, so a fully black frame passed. **The permanently-red check was camouflage
   for the vacuous one** — the tool already exited 1, so nobody looked at the second. Red-first on a
   synthetic all-black frame: old form PASSES at 38 distinct, new form correctly FAILS at 1.
3. **CR-I filed, adopted, served** — filed `5808d8c`, adopted by the hub as **protocol §11.30**
   (empyrean `e7e94fa6`), served + re-vendored `d90a806` (log `c1e44cd`). `absolutise` now on the
   symbols routes at the load boundary and on `screenshot`'s echoed path; **refusals still quote the
   caller's spelling, and that is tested rather than merely written down.** Live proof: server spawned
   with both paths **relative on one command line**, both returned absolute.
   ⚑ **The re-vendor's green witnesses nothing.** The whole schema delta is three `description`
   strings, and `description` has no validation force — the byte-identity gate went green against a
   server still putting a relative path on the wire. The witness is `tests/symbols_path.rs`, landed
   **before** the re-vendor and red-first at 4-of-8 failing.

**Two verification defects of this seat's own, recorded because they are the transferable part.**

* **Segmenting a test run changed WHAT WAS TESTED, and the suite count did not move.** Two
  full-workspace runs were killed (unexplained; no exit code, the *wrapping shell* was taken, which
  excludes both the kernel and a process-name kill — thread closed at empyrean `d935e1a1`), so this
  seat segmented per package per the shared-machine rule. That produced **63 suites / 1943 passed /
  0 failed / 6 ignored** against a true **63 / 2000 / 0 / 6**. **Suite count matched exactly. Ignored
  matched. 57 tests had not run.** `oracle-core`'s `synth` is off in that crate but
  `oracle-frontend`'s default `audio` enables it, so **cargo feature unification runs it under
  `--workspace` and not under `-p`**. Found by arithmetic — summing `passed` against an
  independently-reported figure, then diffing per suite against the pre-parcel baseline to
  `oracle_core::src/lib.rs`, **913 vs 856**. Closed with `-p oracle-core --features synth` → 913.
  **`-p` is not a partition of `--workspace`. When you segment, reconcile the total.**
* **A `head`-truncated listing nearly became a finding.** Ruling this lane out of the killed-run
  window, the worktree being looked for sorted below a `head` cut, and the next sentence would have
  been *"it is gone, so cleanup is the cause."* Then the *replacement* probe's control returned 0 for
  everything since midnight — impossible — so its clean answer meant nothing either. **For an
  absence, the control IS the measurement**; the absence carries no information until the probe is
  shown able to speak. (Banked suite-wide in this seat's words.)


## [orig lines 2134-2240] the restamp A/B, and REPLAY-NET-BLIND-3

## ▶ DONE 2026-08-30 (was: COMMITTED TO AEON, TRIGGERED) — the restamp A/B (booked 2026-08-30)

**This is a cross-lane COMMITMENT, and it is booked here because an offer that lives only in mail does
not survive a `/clear`** (protocol bar 20's sending-side half). aeon accepted; they have booked it their
side too. **Nothing to build now** — the runner already produces every field below, verified before the
offer was made rather than after.

**⚑ TRIGGER FIRED 2026-08-30, CANDIDATE VERIFIED, RUN IS QUEUED BEHIND THE CARGO LANE — NOT DEFERRED
BY JUDGEMENT.** aeon sent it and **corrected their own over-specification in the same message, which is
the part to keep.** They had said the candidate would come from *"a branch carrying the re-record work,
not the supersede and not master"*. That is right for the **restamp** phase and **inverted for the
proof** phase, which comes first: the measurement decides *what* to re-record, so the candidate is the
ROM with the **new clamps and the OLD fixture** — master's build. **A re-record branch cannot exist
until our answer does; had we waited for it, each lane would have been waiting on the other.**

**The candidate, both sides committed blobs so the A/B has NO working-tree dependency:**
* **new side** — sigil **`e38295d2`** (chain 188, attested PASSED), `crates/sigil-harness/golden/s4.debug.bin`,
  sha256 `951cf960…62707d`, len **736315**. Verified here: reachable at sigil `origin/master`, hash and
  length exactly as aeon stated, **and byte-identical to chain 187** — the identity they predicted and
  explicitly would not promise. It held.
* **baseline** — our own `fixtures/aeon/s4.debug.bin`, sha256 `75e9f4d4…19fcf7a` (chain 186).

**⚑ CANDIDATE SUPERSEDED 2026-08-30, AND THE BLOCKER HAS CLEARED — hub ruled (under the owner's standing
delegation, banked at empyrean HEAD) to run this FIRST, L-09 after.** The candidate moved from chain 188 to
**chain 189**: sigil **`39c34fd2`** (*attest: chain 189 strict-clean — 4170 passed, 0 failed, 0 skips*),
golden sha256 **`4ee7ac79…a9a0b3`**, len **736315**; aeon rev **`3f143178`**. Both verified here firsthand as
genuine ancestors of their lanes' `origin/master`.
**⚠ 188 → 189 IS *NOT* BYTE-IDENTICAL, and the length is unchanged (736315 both), so a length check passes a
different ROM.** That is bar 4 arriving on a candidate pin rather than on a build artifact: the previous hop
(187 → 188) *was* byte-identical, which is exactly what would train a reader to skim this one.
**The listing is the snag.** sigil's golden dir carries `s4.debug.bin` but **no `.lst`**, and our frozen
`fixtures/aeon/s4.debug.lst` is chain 186's — wrong for this ROM. The matching listing exists only as an
**untracked file in aeon's live working tree** (`aeon/s4.debug.lst`, mtime beside their `s4.debug.bin`),
which is the precise dependency `fixtures/aeon/` was created to remove. **Snapshotted out of the live tree
before dispatch** to `/home/volence/sonic_hacks/restamp-ab-chain189/` (read-only, `SHA256SUMS` beside it):
ROM `4ee7ac79…a9a0b3` — **verified equal to sigil's committed blob, which is the authority, not the copy** —
and listing `81a11102…845a2f`. Record the listing hash in the result: the pair is only attributable together.

**⚠ WHY IT WAS NOT RUN EARLIER, kept so a fresh session does not read it as forgotten:** the replay A/B needs
cargo, and this repo's standing rule is **never two cargo runs at once** — the `parcel/screen-text` agent
holds the lane. **Run it when the lane frees.** aeon explicitly asked that it wait rather than be run
tired, on their own record of five instrument errors that night; the serialization rule and their request
point the same way, but **the rule is the binding one.**

**THE TRIGGER, and only this:** aeon messages us with a candidate ROM's `aeon_rev` and the sigil freeze
SHA **and the words "candidate for the restamp A/B"**. It will be a ROM from a **branch carrying the
re-record work — not the supersede, and not their master**. They will separately send the supersede's
SHAs *labelled as NOT the candidate*, precisely so that message cannot be misread as the trigger. **Do
not act on a supersede landing; do not pre-build anything.**

**WHAT THEY GET BACK — the moved SET, never a count.** Their words: *"the count is the headline and the
set is the evidence"*, because their prove-then-restamp ruling turns on saying **why each mover moved**.
`Ojz` (27 checkpoints) is what the ruling needs; `OjzSlide` (37) if it falls out of the same walk, which
it does. Our `RestampPlan` (`crates/oracle-replay/src/restamp.rs:447`) already carries exactly this:
`stale: Vec<StaleCheckpoint>` in stream order plus `total_checkpoints` for *"3 of 27"*, and each
`StaleCheckpoint` (`:432`) carries `index`, `ring`, **`logic_tick`**, `payload`, `fixture_offset`,
`expected`, `actual`. **`logic_tick` is the field that makes their prediction testable rather than
merely reportable** — it is what lets a mover be correlated with camera position.

**⚑ THEIR FALSIFIER, STATED BY THEM BEFORE THE RUN — this is the part to protect.** Their mechanism
predicts the movers are the **early** checkpoints, where `cam_col < 16`, and that checkpoints deep into
the run — once the camera has travelled past column 16 — **hold**. *"If checkpoints deep into the run
also moved, my mechanism is incomplete and the restamp must not proceed on it."* They asked explicitly
for the moved set **whether or not it confirms them**.
**So: report the set verbatim, including the shape of it, and do NOT summarise it into a verdict that
agrees with them.** A confirming summary is exactly what would let an incomplete mechanism authorise a
restamp — and a restamp that proceeds on an incomplete mechanism destroys the net's only claim, because
their fold is deliberately address-free so that *a desync means real behaviour moved*.

**Why this arrangement is worth its cost to us:** it is the instrument co-development lane — they need a
per-checkpoint delta, we already emit one, and standing the comparison up their side would cost them more
than running it costs us. Our frozen chain-186 copy is what makes the A/B clean: same runner, same code,
one side pinned and attributable.

## ▶ DONE 2026-08-30 (was: QUEUED) — REPLAY-NET-BLIND-3, and the technique note is the load-bearing half

aeon's scoped ask (booked 2026-08-30, `b103a47`), in **dependency order, not as a menu**: (1) run the two
`#[ignore]`d playthroughs somewhere that is **not** the default debug suite — ~9 s in release against ~83 s
in debug, and a suite that slow gets reverted by the first person it annoys; (2) make the fixture pin's
staleness visible (`fixtures/aeon/PROVENANCE.md` records the pin, nothing reports when their master moves
past it); (3) have the suite **name the pinned chain in its output**, so a green cannot be read as a
statement about their master. There is also a **third** ignored playthrough at `:716` (~100 s) that neither
lane's booking had named.

**⚑ WHY ITEM 1 ALONE IS A TRAP, and this seat would have fallen into it.** Since `090784a` the tests default
to our OWN frozen `fixtures/aeon/`, pinned at **chain 186**. Un-ignoring the playthroughs leaves the net
blind to everything after 186 — including the clamps. **Two independent blinders; removing either one does
not clear it.** Un-ignoring and reporting the net fixed would have produced a green suite that is still
blind, which is the *more convincing artifact* failure this file already carries three times.

**⚑ THE TECHNIQUE NOTE, aeon's, and it is why the second blinder stayed invisible: CHECK THE FIXTURE PIN
FROM THE DATA SIDE, NEVER FROM THE TEST SIDE.** Their agent found it only because it was sweeping recorded
chains for an unrelated reason and hit the pin from the data end. **Approached from the test file — which is
how both lanes would naturally have come at it — it is invisible: the tests look correctly configured, the
fixtures look correctly frozen, and nothing in either place connects them.** Their own read, adopted: that is
the enumeration parameter doing the work again rather than anyone being sharp. Banked here because it lived
only in mail and would not have survived a `/clear` on either side.

**Precision caveat that applies to BOTH lanes' write-ups** (aeon's, and they have corrected theirs as we
corrected ours): checkpoints fire **every 64 ticks**, so *"checkpoint 18 / tick 1154"* **bounds** the
divergence to ticks 1091–1154 rather than locating it.

**Their diagnosis, for whoever takes this** — one commit, `fde35b2f`, whose entire diff is two collision
files; attributed by **byte identity** (its build's `s4.debug` CRC `a9676c6b` IS chain 181's frozen golden)
rather than by inference; chain 180 clean, 181 stale at exactly {18–26}; mechanically proven restampable
with inputs untouched. Their measurement, cited as theirs — not re-derived here.


## [orig lines 2495-2554] commentary on protocol bars 8-15 read at boot anyway, and a currency check the hermetic gate superseded

> As of 2026-08-22 evening the protocol also carries **bar 12** (a doc's universally-quantified
> rules bind every actor they describe, not just the party in its title — grep the contract you own
> before treating a cross-tool question as an open fork), **bar 13** (a reachability argument is an
> enumeration problem — "this can never be live when that fires" is a claim about *every* caller;
> phrase it as a discipline both sides run, never as "the overseer checks reachability"), a
> **precedent-perishability preamble** (the bar is durable, its narrative is not — precedents cite
> recently-churned code, which is the likeliest thing in the repo to be refactored next; when
> narrative and bar disagree, **the bar wins**), bar 8's cheap frame-changer (**find the load-bearing
> step nobody cited and check that** — the shared frame lives in the uncited joint), and bar 4's
> converse from aeon (**on a byte-neutral parcel a matching CRC cannot witness that the build RAN** —
> directly load-bearing here, since zero-file-diff is our default expectation for bus work, so that
> expectation now needs a witness that the run happened, not an unchanged hash).
> **Our own sequencing call became bar 9's corollary** (*validate first, adopt second*) with the
> declining-to-vendor above as its precedent, and "**the tell is not the red build, it is who is
> expected to move**" is in the protocol verbatim in substance.
>
> **2026-08-26, out of this lane's stale-shim day: a new sentence in Shared-machine cautions** (empyrean
> `5ad6108`, verified here as a reachable ancestor of their `origin/main` and as a docs SHA carrying docs)
> — a shared-machine hazard goes to the lane BEARING the risk in a form it can act on, not only to
> whoever owns the fix. **Not transcribed here; read it there.** sigil's proposal and sigil's framing (a
> remedy ends the condition, a warning prevents the trip while the condition lasts); this lane endorsed
> after speaking with them first, so it is one finding with a second lane's endorsement and the text says
> `oracle endorsing` for exactly that reason. The pid/started/no-child example in the shipped sentence is
> this lane's measurement, generalised.
>
> The shared protocol gained review bars 8–10 and two SHA-citation rules on 2026-08-22 (empyrean
> `dc629a5`, `c2c81e2`, `00334b6`, `43fbfc9`, `9b604f0`+`e650b96`+`aadf63f`, `20a8e81`). **Not transcribed here — read them there**; the
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
> **Bar 11** (`20a8e81`, out of this lane — aeon's formulation): *a confidently-offered weak point is
> a misdirection, even in good faith*; the operational line is that **a citation is a pointer into
> code that keeps executing past the line you were shown**. Read the lines *around* a cited line
> before accepting what it proves. It is **not** a rule against caveats — the flag must keep doing
> its job, and a session hedging less to avoid aiming scrutiny is this bar's own failure mode.
> **Placement note worth inheriting:** empyrean put it in the review bars rather than beside
> verify-firsthand, over my suggestion, because **the bars are what agent briefs inherit and an
> agent's report carries the identical shape** — "my weak assumption is X" steers an overseer's
> review exactly as a peer's message does. In this lane that is the *more* common case: we review
> agent reports far more often than peer claims, and both of 2026-08-22's agents delivered reports
> with volunteered open items. Apply it to returned work, not just to cross-session mail.
>
> **Our currency check, run at tip 2026-08-22** (the operation `aadf63f` names): the vendored
> `crates/oracle-aether/tests/contract/bus-protocol.schema.json` is `f038672daf6eb2b8`, and
> empyrean `origin/main` `aadf63f`:`contract/schema/bus-protocol.schema.json` is the same
> `f038672daf6eb2b8` — byte-identical, with **zero commits touching that path** since our CR-28
> vendor at `70c7bb4`. The day's seven empyrean commits are all `OVERSEER-PROTOCOL.md`, not the bus
> contract, which is why. TRACKED_REVISION stays retired at None.
