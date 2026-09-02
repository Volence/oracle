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
Fable agent, no steer — recorded 2026-08-21 as cost-questioned-and-ratified by the owner, on the
reasoning that the ruling is where one judgment becomes permanent contract text, so the smartest
model sits there and nowhere in the bulk work. ⚠ **PROVENANCE, audited 2026-08-22 after empyrean
flagged the class:** that ratification is recorded in `7fd201d`'s prose and commit message and
**nowhere else — no citation of the granting act exists.** It is *stronger* than empyrean's
parallel case (their string was a spec's own self-declared status field; this one carries a
rationale responsive to a cost objection, which is the shape of a real exchange) and it is *weaker
than a cited ruling*, which is the only thing that settles it. Git authorship proves nothing here:
every commit in this repo carries the owner's identity whoever wrote it. **Standing rule adopted
from empyrean: never record an approval whose granting act you have not seen — cite the ruling,
not a status field.** Treat the seat as owner-confirmable, not owner-confirmed. **The action it was
used to justify survives the doubt anyway** — declining to spend a premium-model budget without the
owner is his call whether or not a prior ratification exists — but the *justification* was
over-claimed, twice to him and once across the fence, and is corrected here); verify every gate
firsthand before accepting a slice; make the design
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

   **NEXT (not yet dispatched):** open from the survey and **not lost**: stale prose at
   `schema_conformance.rs:6,222` and the `resolve_target` `oneOf` divergence (**both folded into
   the in-flight `run_to_scanline` parcel** — remove from here once that lands), and a proposed
   **error-surface gate** — since no fragment declares error conditions, a suite validating only
   replies is blind to every error obligation.
   **FOREGROUND runtime follow-ups, never a subagent** (the emulator MCP deadlocks from background
   agents): the `step` frame-budget truncation; the `write_vram` SAT-cache desync; and **two new
   ones from CR-B** — the tail wrap (`z80_write {addr:"0x3FFC", bytes:<8>}` then read `$0000`;
   predicted from source: bytes 5–8 land at `$0000–$0003`) and the silent `len` clamp
   (`z80_read {len:10000}` → `8192`, no error). ⚠ **ATTEMPTED 2026-08-22 evening and correctly
   ABANDONED, not deferred for convenience:** my MCP client found no socket in my own
   `$XDG_RUNTIME_DIR` (`Errno 2` — a *failing lookup*, which says nothing about the world, bar
   16(d)). A `pgrep` showed an emulator IS live, but it is **another lane's harness** with its own
   `XDG_RUNTIME_DIR=/tmp/oracle-harness-4av2i47x` running `aeon/s4.debug.bin`. Writing into a
   peer's harness Z80 RAM to demonstrate a wrap bug is the shared-machine hazard itself. These stay
   open until a lane-owned instance exists; **the CR does not depend on them** — it stands on the
   source read, and the runtime pass would only upgrade it from derived to demonstrated.
   **AEON OBLIGATION — SCOPE WAS WRONG, and the correction makes it bigger.** Item 7 recorded it
   as a dated heads-up before serving `emulator/wait_for_break`, because their gates send
   `timeout_ms`. **The survey found it covers THREE methods, not one, and I verified it firsthand
   at `origin/master` (not their working tree):** both scripts run an **arm → wait → clear** flow —
   `raster_source_gate.py:161/168/173` and `snapshot_poison_gate.py:62/64/68` call
   `emulator/breakpoint_add {addr}` → `emulator/wait_for_break {timeout_ms}` →
   `emulator/breakpoint_clear {all:true}`.
   **Consequence, and it is the load-bearing one: the migration CANNOT be piecemeal.** Serving
   `wait_for_break` alone would leave their flow with nothing to arm — so `wait_for_break` and the
   breakpoint trio ship as ONE parcel or the notice is worthless. The `timeout_ms` spelling was
   never the whole exposure; it was the part visible from a param grep.
   **This also gives the obligation a live reader BEFORE any date exists.** Their call sites bet on
   a specific breakpoint shape — `{addr: "0x…"}` to arm, `{all: true}` to clear, i.e. **address-
   keyed, no handles** — and **CR-A (D-13) is about to decide exactly that handle discipline.**
   Their input window is *now, before adjudication*, not when we ship. Note also
   `raster_source_gate.py:33`: under `deterministic=True` the legacy server answers `breakpoint_add`
   with a "det-mode stop" behaviour — a documented interaction our fragments say nothing about.
   The **date** still waits on the survey's pricing of that parcel; the **design consultation**
   does not, and holding it until a date existed would have consulted them after the ruling.
   If this session ends first, **the next one owes both**.

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

**Registered 2026-08-29, from aurora's relay of the owner's R8 question:**

- **F-R8-LATE-REVISION** — a *copy-of-column-0* toggle for the R8 leftmost-partial-column quirk, so a
  fix can be seen under both hardware behaviours. **Declined as a booking, and the reason is not
  "noise".** Under the later behaviour the leftmost column takes column 0's vscroll, so the defect
  simply **disappears** and aeon's column-19 write becomes an inert no-op rather than something the
  differential validates — the whole verdict is derivable without building it. Against that we have
  **no hardware-tested rule for the late revision**, only Plutiedev/Stef descriptions, where the early
  rule pinned at `render.rs` `plane_vscroll` (H40 `VSRAM[$4C] & VSRAM[$4E]`, H32 `0`, same value both
  planes) matches Genesis Plus GX's *"verified on PAL MD2"*. Shipping a second model whose fidelity
  cannot be established, and then letting a consumer validate a fix against it, is bar 9's corollary
  exactly: an unvalidated instrument adopted as a gate returns a **confident wrong verdict**.
  *Revival condition:* a hardware-tested rule for the later revision appears, **or** a second scene
  turns up where the fork changes a design decision. The divergence ledger already records the fork,
  so nothing is hidden meanwhile.
- ~~**F-HUD-FILTER-LABEL**~~ **DONE 2026-08-29 (SCREEN-HONESTY parcel)** — was: the F3 status line
  printed the console **audio** output stage as a bare `MODEL1-VA0-VA2` with no label (`overlay.rs`
  `status_text`, between `VOL` and the aspect / native-resolution / frame fields), so most of its
  neighbours were video facts and **the owner read it as a VDP/board revision** (through aurora
  2026-08-29 — the misreading was ours, not his). Now `AUDIO VA0-VA2`, via a frontend-local
  `filter_label` rather than `ConsoleModel::name`: that identifier round-trips through `from_name`
  (it is what `ORACLE_CONSOLE_FILTER` parses) and so is not free to be shortened for a readout.
  `Unfiltered` renders `RAW`, deliberately not `OFF` — `AUDIO OFF` reads as *there is no sound*.
  **Shorter than what it replaced**, which mattered: see the width finding below.

- **F-BANNER-INVITES-A-PIN** — *found from the other end, by a consumer breaking on it (aurora's O26,
  2026-08-29).* Our startup banner prints `aether: N methods advertised`, and `Bus::start` prints the same
  total on the serving line. **A published total is an invitation to pin it**, and a consumer did: their
  `classic-playtest-harness.mjs:171` pinned `methods === '35'` and *threw* `stale oracle-aether binary` on
  anything else — so the guard written to detect staleness became the stale thing and rejected every
  correct binary. **Measured firsthand at `6031020`, two ways: the banner says 52, and `initialize`'s
  `methods` array has length 52** (spawned and called, not read off a schema).
  **The defect is theirs; the surface that manufactures it is ours.** A count changes for reasons unrelated
  to freshness and is identical across binaries that differ, so it is the wrong observable for the question
  every consumer actually asks — *is this binary current?* We already serve the right answer and do not point
  at it: `initialize.serverBuild` carries `{id: "<sha>+profile=…+target=…+features=…", source, dirty}`.
  **Cheap fix, and it is a documentation-and-adjacency fix, not a removal:** name `serverBuild` in the same
  breath as the count, so the number a reader meets first is not the only identity on offer. Do **not**
  simply delete the total — it is genuinely useful at a glance, and aurora's lesson is about what a consumer
  should *key on*, not about what we may print. Our own side is clean: grepped, every use is `METHODS.len()`
  and no literal total is pinned anywhere (`crates/oracle-aether/tests/params_closure.rs` closes over
  `METHODS.len()`, which is the derived form).
  **The durable line, aurora's: a total was the wrong observable.** Same family as this file's own
  name-is-not-behaviour bar — a number that *correlates* with the property being tested, standing in for the
  property, and reading exactly like a real check until the correlation breaks.

  ⚠ **AMENDED SAME DAY, and the amendment corrects THIS BOOKING, not the consumer** *(aurora, 2026-08-29,
  who class-checked the SHA out of habit and found the thing I had just recommended people point at)*. The
  paragraph above says "name `serverBuild` in the same breath as the count". **That advice is incomplete in
  a way that walks a consumer back into the same trap from the other side.** `serverBuild.id` names whatever
  HEAD was at build time — and the id they measured, `6031020`, is a **docs-only commit** (`lane-status.json`,
  +10/−18). That is correct behaviour for a build identity and is exactly what staleness wants, but it means
  **the id moves for reasons that have nothing to do with the code**: binaries built at `acf41f5` and at
  `6031020` contain identical code and report different ids.
  **So the field answers *"is this the same binary I measured before?"* and MUST NOT be compared for equality
  to answer *"does this build contain feature X?"*** — that second question belongs to `capabilities` and
  `methods` membership, which is the derived form this register already recommends. Whenever we point a
  consumer at `serverBuild`, we owe them that sentence in the same breath; a pinnable identifier offered as
  the cure for a pinnable count is the same defect in better clothes.

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

- **F-SERVERNAME-PREDATES-THE-RENAME** — `EngineConfig::default()` sets `server_name: "oracle-next"`
  (`crates/oracle-aether/src/engine.rs:205`, read at `fee8f12`), so every `initialize` still answers with the
  **pre-rename** repo name; `serverVersion` is `"0.0.0"`. Spotted by aurora 2026-08-29 while assessing what on
  our wire is an identity.
  **Not a wire-correctness bug, and say so plainly:** §2.1 deliberately demotes `serverName` to a *deployment
  label* and moves identity to `implementation` (`"oracle-rs"`) and `serverBuild`, both read from `build_info`
  and — verified firsthand, not from the comment — **barred from configuration by a source-level test**
  (`tests/server_build.rs::neither_identity_value_is_reachable_from_configuration`). A consumer reading
  `serverName` for identity is reading the field the contract told it not to.
  **But the value is still a stale name we publish on every handshake**, and "it is only a label" is exactly
  how a wrong string survives a rename. **Changing it is wire-visible**, so it does not get a drive-by edit:
  it needs bar 14's consumer-set enumeration first — grep every sibling tree for the literal `oracle-next`
  with real client context — because the failure mode of a consumer matching on it is silent. Revival
  condition: do it as part of any deliberate handshake pass, never alone.
  **ONE CONSUMER CLEARED, AND THEIR OWN CAVEAT IS THE REASON IT IS STILL NOT A GREEN LIGHT** *(aurora,
  2026-08-29, asked for exactly this input)*: they grepped `src/`, `test/` and their harnesses — 40
  references to `serverName`, **zero** comparisons/branches/`includes`/`startsWith`/`match` on its value;
  every use is display or pass-through. **But they also volunteered that the literal `'oracle-next'` appears
  21 times across 8 of their test files as fixture INPUT**, with assertions derived from the payload
  (`expect(s.serverName).toBe(PAYLOAD.serverName)`) — so a rename leaves their suite green **while their
  fixtures quietly describe a server that does not exist.** Their formulation, worth keeping verbatim in
  spirit: *our green is not evidence your rename is safe; it is evidence we do not look.* That is the
  clearest statement of this register's own no-consumer-broke hazard anyone has offered, and it came from
  the consumer. **Four repos remain unenumerated** (aeon, seraph, sigil, empyrean), so the booking stands;
  aurora has asked to be told if it moves, so they can re-point their fixtures.

- **F-RSP-XVFB-ORPHAN** — *audited into existence by a peer's warning, and the audit came back clean on
  the thing they warned about.* aurora relayed their O16 finding 2026-08-29: 28 of their harnesses tore
  down with `pkill -f '<dist path>'`, an argv pattern that matched **other sessions' processes** and had
  killed a peer's Electron mid-run three times. **It does not apply here, and that is a measurement, not
  an assumption.** Enumerated by what *touches process teardown* rather than by the token (protocol bar
  8): the executable surface is 16 `.sh`/`.py` files plus every `.rs`; spawn sites are five
  (`rsp.py:43`, and four `Command::new("git")` that are `output()`-style and never outlive the call);
  **teardown sites are exactly one — `rsp.py:157 self.p.kill()`, on the `Popen` handle that object
  itself spawned.** Ownership by construction: no pattern, no process name. Every `pkill`/`killall`
  string in this repo is in **docs prose warning against it** (5 in `docs/`, plus stale copies inside
  three dead `.claude/worktrees/`), zero in code. The control run (165 bare `kill` hits) confirms the
  grep could see what was there, so the empty result is a measurement rather than a broken pattern.
  ⚠ **The one residue their audit did surface here, and it is the OPPOSITE failure — sent to them
  hedged, as a reading rather than a finding, because I have not run it.** `rsp.py` spawns through
  `xvfb-run -a`, so `self.p` is the **wrapper's** pid, not BlastEm's. `close()` sends the RSP `k`
  (kill) to BlastEm first and that is the normal path, but when the stub is wedged — the case the
  watchdog exists for — the backstop `SIGKILL` lands on a shell that cannot trap it, so BlastEm and
  Xvfb may be left **orphaned**. That endangers nobody else's session (a stranger surviving, never a
  stranger dying), which is why it is a register entry and not a hazard notice. **Revival condition:**
  the differential harness is run in anger again, or a stray `blastem`/`Xvfb` is found outliving it —
  fix is a process group (`start_new_session=True` + `killpg`), not a wider pattern.
  ⚑ **STRENGTHENED 2026-08-30 by aurora's measurement (aurora `055bff40`), which supplies the consequence
  this booking lacked.** `/usr/bin/xvfb-run` **has no trap**, so a SIGTERM to the wrapper skips all of its
  cleanup — and each skipped cleanup leaks an X lock, a socket **and a tempdir**, where **every leaked lock
  permanently burns a display number**, so every later run on the box scans longer. My entry had the
  orphaned-process half and read the cost as "a stray process survives"; the real cost compounds across the
  machine and outlives the session that caused it. Their portable fix: reap the wrapper's **children** by
  PID first, then the wrapper.
  *Two honest scope notes.* (a) aurora **retracted** the suite-wide version of this (aurora `81ebf173`) —
  they fingerprinted the leak to their own harness, so nothing here is currently leaking; measured at the
  time: `/tmp/.X*-lock` = 0, `/tmp/.X11-unix` = 0. (b) **This lane nearly reported "no xvfb-run here" on a
  grep that had errored on shell globbing.** Run correctly there are four hits — `rsp.py:10`/`:41` and
  `nightly_differential.py:145`/`:217`. Second instance in one night of bar 16(d) at this seat: **a failing
  command and an empty world print the same thing**, and only a positive control separates them.
  *Not applicable to the player-window fixtures* (`docs/2026-08-29-window-runtime-checks.md`): those run
  their own `Xvfb :N` and kill it by recorded PID, so the wrapper is out of the loop entirely — which was
  an accident of needing a known `DISPLAY` for XTEST, not foresight, and is recorded that way.

**▶ NEW BAR, 2026-08-30 — EVERY CITATION RULE THIS SUITE OWNS IS WRITTEN FOR THE RECEIVING SIDE, AND
BOTH OF TONIGHT'S FAILURES WERE ON THE EMITTING SIDE, WHERE NO RULE REACHES.** aeon's formulation, banked
by them at aeon `4fae2d8d`; two instances, one from each lane, hours apart.

**The pair.** This lane sent sigil a confident wrong claim about **its own tree** (the `.lst` recipe
"residue", retracted above). aeon reported a commit hash **typed from memory of a commit they had made
four minutes earlier**, which resolved in no object store anywhere. Different artifacts, one mechanism.

**Why no existing rule catches either.** `--stat` what you are handed; check the SHA's class; verify the
anchor; do not trust the paraphrase — **every one of them presupposes an incoming artifact.** There is no
incoming artifact when the claim is about your own work, so the whole apparatus is structurally
inapplicable. aeon's diagnosis of *why* it goes unchecked is the sharp part: **a claim about someone
else's tree feels like a claim and gets verified; a claim about your own feels like recall.**

**And the amplifier, which is this lane's half:** a careful peer reasoning soundly on your wrong premise
makes the error look **corroborated rather than caught**. Their care is what hardens it. Same circuit as
the 52-method number that went out from here, came back as a peer's, and outranked our own measurement.

**Their operational form, for hashes:** emit every SHA from the command that proves it, in the same
invocation as the thing it anchors. **Ours is about claims rather than hashes, and is this:** a statement
about our own tree that is going OUT — to a peer, to the owner, into a doc — is read out of the file at
send time, or it is sent hedged. Not because it is likely wrong, but because *nothing downstream can
check it*, and the more competent the receiver the more thoroughly it will be built upon.

⚑ **AND THE PATTERN BEHIND THE INSTANCES, 2026-08-30 — THIS SEAT KEEPS READING A SOUND OBSERVATION AT ONE
NOTCH TOO WIDE A SCOPE.** Three in a night is a habit, not luck, so it is booked as one entry rather than
three slips:

| observed (true) | asserted (too wide) | what settled it |
|---|---|---|
| no socket in `/run/user/1000` | *"the socket chain is EMPTY, no lane can reach any emulator"* | reading the resolver — one path, chosen on a directory test |
| a grep for `xvfb-run` returned nothing | *"this repo has no xvfb-run site"* | the grep had **errored** on shell globbing; four real hits |
| `ls /tmp/.X*-lock \| wc -l` = 0 | *"the box has zero leaked locks"* | `ls` is aliased to `eza`, which **exited**; `find` says 108 |
| two `cargo` processes running | *"two cargo runs in THIS repo"* — the serialized-cargo hazard | `/proc/<pid>/cwd`: the other is in `.sigil-clamps`, a sigil worktree, under aeon's `refreeze --attest`. Different target dir, no lock contention, rule not engaged |

**The shape is constant: the measurement is right, the SCOPE is asserted rather than measured, and the
wider claim is the one that gets said out loud.** Each was settled by exactly one command — read the
function, run the grep correctly, use `find`, read `/proc/<pid>/cwd`. **Two of the four are the SAME
mechanism** (a command that failed, with its error suppressed or swallowed by an alias, counted as an
empty world), which is bar 16(d) and which this file already carried when all three happened.
**Operational form, and it is narrower than "be careful": before stating a scope — *no lane*, *this repo*,
*the box*, *nowhere* — name the command that establishes THAT WORD, not the one that established the
observation.** They are rarely the same command, and the second one is usually cheap.

⚑ **THE INSTANCE, SAME NIGHT, AND IT IS A VERIFICATION I REPORTED AS CLEAN.** aeon found a blocker with no
decision card; this lane ran the same check over its own board and told a peer *"no missing cards here"*.
**That emitted claim had two independent defects, and a peer found each — neither was found here.**
1. **Wrong enumeration parameter.** It read blockers from `blockedOnOwner` only, and `AUDIO-DULL`'s owner
   blocker was sitting in a queue row's `blockedBy` **free text**, where the enumeration could not see it.
   The correct form enumerates owner-blocking claims from `blockedOnOwner` **and** every `blockedBy`
   string, because prose is where one hides. (aurora, 2026-08-30.)
2. **Wrong data shape.** It built `{id: entry}` over `docs/decisions.jsonl` — the dict-by-id that
   empyrean `52519fd` now forbids outright, ledger tooling being **line-addressed**. Measured here after
   the amendment landed: 20 lines, 20 distinct ids, **0 entries dropped** — so the answer was right and
   the method was wrong, and it was *guaranteed* to break at the first 8c closure, which appends
   duplicate content by design. **A check that is correct by luck reports exactly like one that is
   correct**, which is why the amendment is a shape rule rather than advice.
**Both defects were in a claim about our OWN board, sent outward, where nothing downstream could check
it** — the bar above, arriving on the verification written to enforce a neighbouring bar. Nothing was
committed carrying the bad shape (only two docs mention the ledger; no scripts), so this is a habit note,
not a repair. **Do not transcribe the contract clause itself** — read `contract/DECISIONS.md` there.

**▶ F-CR28-CALLERS-DANGLING, registered 2026-08-30 — an unmerged commit in a leftover worktree, found
while earning an `atBoundary: true` claim rather than asserting one.**

Branch **`cr28-callers`** holds **one commit not on `main`** — `22d57ca` *"docs: CR-28 ruling applied —
ADOPT WITH CHANGES, M1-M7 and S1-S4"*, a **418+/145− revision of `docs/2026-08-21-cr28-callers.md`**
whose blob **differs from `main`'s copy of the same path**. Its own message ends *"Nothing merged,
nothing pushed"*, which is an agent's honest close-out, not a verdict on whether the controller wanted it.

**Deliberately NOT merged and NOT deleted.** Queue item 5 records CR-28 as fully done — ruling
adjudicated, applied and served on 08-21 — so this is *probably* a superseded intermediate. **Probably is
not knowledge**, and merging a docs revision into a closed arc on a guess is worse than leaving it.

**Why it is registered rather than mentioned:** the two sibling worktrees (`parcel/gui-layers`,
`profiler-shortrow-residual`) are genuinely merged — **zero-ahead AND ancestors of `main`, both
conditions checked**, because zero-ahead alone is the two-valued reading bar 16(a) was written for. This
one is neither, and a dangling branch is invisible to every reader who does not run `git worktree list`.

**Revival condition:** anyone reopening CR-28, or the next session that prunes worktrees. Resolve by
diffing `22d57ca:docs/2026-08-21-cr28-callers.md` against `main:` and deciding whether the revision was
superseded by the applied ruling or dropped by accident — then merge it or delete the branch **with the
reason recorded**, so the next reader is not asked the same question a third time.


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

**Registered 2026-08-29 by the two window checks** (`docs/2026-08-29-window-runtime-checks.md` — both
gates discharged; the `LIVE-OBJECTS-CARD` sequencing blocker is cleared):

| id | what | revival condition |
|---|---|---|
| **F-PICKER-FILTER-MARKER** | ✅ **FIXED 2026-09-02 (`f336658`, parcel/player-polish) — and the seam test derives its filter query FROM `LOADED_MARKER` minus the label's own letters, so it cannot agree with a stale copy of the marker.** The ROM picker's filter matches the `[loaded]` **decoration**: `rom_browser.rs:115` bakes the marker into the label (`format!("{}   [loaded]", …)`) and `Picker::visible()` (`palette.rs:58-62`) `subseq_match`es that composed string, so every letter of `l,o,a,d,e` is free for the already-open ROM and it survives filters that should exclude it. **Cosmetic — Enter still runs the correct visible row, proven on live data.** The instructive half: this is model-level, tests already assert over those labels (`rom_browser.rs:251`), and nothing asserts the **seam between the marker feature and the filter feature** — this morning's *a test that asserts what you added is blind to what you displaced* bar, one step out. | Next open of `rom_browser.rs`; two-line fix (match `entry.label`, render the marker separately) plus the seam test. |
| **F-ROMOPEN-C-DOC** | ✅ **FIXED 2026-09-02 (`e5f57c4`) — the CODE moved to the doc, not the reverse:** the listing now lives in `RomBrowser { dir, entries }`, survives a failed scan, and the picker re-opens on the retained entries beside the toast. `docs/2026-08-28-rom-open.md` §5 promises an unreadable folder *"leaves the previous listing up"*. It **dismisses the picker** instead — `open_rom_picker` early-`return`s before `open_picker` (`main.rs:494-501`) while Enter has already closed the palette. Safe and loud (toast names the path, ROM unchanged, player alive), but the doc's own acceptance criterion is not met. | Correct the doc at the next pass; optionally re-open on the previous directory before notifying. |
| **F-TOAST-TRUNCATES** | ✅ **FIXED 2026-09-02 (`5d2978d`).** `fit_marked` reserves the mark's own width; reasons precede paths; the trailing ` (os error N)` is stripped as redundant next to the text. `notify_err` toasts cut from the right with no ellipsis and **lose the reason**: `open ROM: cannot read {dir} ({e})` rendered as `…/LOCKED (PE`, dropping `Permission denied`. The path survives, the reason does not — and the reason is the half a person needs. Today's SCREEN-HONESTY parcel fixed exactly this on the status line; toasts were out of scope. | Any parcel touching toast rendering — and assert on the **whole** rendered string, per this file's own 2026-08-29 bar. |
| **F-WINDOW-BUS-FRAME-OFFBYONE** | ✅ **DIAGNOSED 2026-08-30, and the registered reason below was WRONG.** ~~*A completed-vs-presenting convention difference would explain it and would not be a defect.*~~ **It is a real, accumulating divergence.** Bus `frame` is DERIVED from the clock (`engine.rs:2223`, `now() / MCLK_PER_FRAME`); the window's `F` is a COUNTER bumped after every run iteration whether or not a frame completed (`main.rs:1929`). A breakpoint stopping mid-frame is a **permanent +1** that never self-corrects, and a **state load diverges them without bound** in the other direction (`main.rs:1571` prints *"frame counter continues at {frame}"* while the restored clock rewinds). `engine.rs:2237` had already named "a UI counter" as the thing it refused to serve — the window IS that counter. **DO NOT JOIN THESE NUMBERS**; `emulator/screen_text`'s fragment carries the disclaimer in its own description so no consumer tries. **Fixing the counter is a SEPARATE item and deliberately not bundled** — it is a behaviour change and a contract ruling must not be made contingent on one. | **RULED 2026-08-30 (L-08, `docs/2026-08-22-unadjudicated-decision-ledger.md`): RELABEL, do not sync the counter.** The engine had already settled it from the other end — `engine.rs:2349-2351` refused to serve a UI counter and named the cost (*three hand-rolled realignments*), so syncing ours is that refusal re-litigated from the losing side. A still-incrementing `F` at a breakpoint halt is how a person sees the render loop is alive while the machine is not; a clock-derived number freezes there. What is left is the WORDING, for whichever parcel touches the status line. The *joining* hazard is already closed by the fragment. |
| **F-FONT-BACKTICK** / **F-FONT-EMDASH** | ✅ **FIXED 2026-09-02 (`d845e1e`) — four glyphs, not two** (tilde and ellipsis came out of the sweep). Durable half: `every_string_literal_the_frontend_can_show_is_drawable` lexes the literals out of each module's production region rather than restating a list; **red-first at 62 undrawable literals.** The player has **no glyph for `` ` `` or `—`**, so `Canvas::text` substitutes a hollow box. Its own **first toast contains a backtick** (`main.rs:1207`) and six live toasts carry em dashes — the window has been showing boxes at the owner all along. `font.rs`'s guard test restated its own input, so it could not catch either. **Verified under a positive control** (`'A'` present, both absent). **Now ASSERTABLE rather than merely known**: `screen_text`'s `unrenderable[]` is the instrument, and the glyph tests carry premise assertions that fail loudly if the font ever gains these characters. | Any parcel touching `font.rs`. Adding the two glyphs is a few table rows; it was deliberately NOT bundled into the `screen_text` parcel, which made the defect observable rather than fixing it. |
| **F-SCREEN-TEXT-PALETTE-LENS** | `emulator/screen_text` serves **three of the five** adopted `kind`s — `titleBar`, `statusLine`, `toast`. `palette` and `lens` are not served (reasons in the module doc); both are **additive and need no contract change**. Separately, the layer badge and the PAUSED banner have **no `kind` in the adopted enum at all**, so they cannot be reported without a contract amendment. | A consumer asking for palette/lens text, or a CR that widens the enum. |
| **F-SCHEMA-READS-LIVE-EMPYREAN** | ✅ **FIXED 2026-09-02 (`parcel/stopprecision`, `7308e967`) — to the shape the suite ratified an hour later, `empyrean/contract/SUITE_PATHS.md` at `38f6df4`, which cites this finding by name.** The walk from `CARGO_MANIFEST_DIR` to a peer's live working tree is gone. `schema_conformance.rs` now hashes the vendored bytes as a **git blob** against `pin.blob` in `PROVENANCE.md` (step 0, never skipped, needs no peer); `$AETHER_CONTRACT_SCHEMA` (a file) and `$AETHER_CONTRACT_REPO` (a checkout, read only through `cat-file`/`rev-parse`/`merge-base`) confirm the pin against the contract repo; and with neither set the run prints a banner naming both variables and both halves — **no walk**. The resolver prints which step answered first. `PROVENANCE.md` now pins revision + blob + bytes as parseable markers, which was the other half of this row. Red-first four ways (appended byte, one-byte edit at constant length, a repo without the revision, a deleted pin marker); all three steps also exercised green. Findings: `cat-file blob` ALONE is nearly vacuous — pointed at THIS repo it passes, because vendoring put the same blob here — so the revision + ancestry checks are what make step 2 mean anything; and a default run no longer notices upstream moving on its own, a deliberate trade recorded in `docs/2026-09-02-stopprecision.md` §6. | — **↪ ANSWERED 2026-09-02, and my framing was wrong: this is the suite's DEFAULT shape, not ours.** The hub's first enumeration searched for gates reading a contract FILE and found only ours; **sigil corrected it within the hour** — the population to enumerate is *what READS a peer's tree, not what NAMES one*, and by that measure it is everywhere: sigil `test_support.rs:601` `LIVE_TREE_FALLBACK` behind 247 `aeon_dir()` call sites plus three committed scripts (two on active systemd timers), aeon `test.sh:286`, and **our own `crates/oracle-core/examples/common/rom_source.rs:44` `LIVE_AEON_DIR` and `tools/aeon_pin_report.py:145`** — both confirmed at our tip here. Aurora and seraph clean. The resolver case carries a hazard the contract-file case lacks: **the revision moves under a single run**, so a pass is attributable to whatever the tree happened to contain. **Ratified fix shape** (`contract/SUITE_PATHS.md`, empyrean `38f6df4`, verified an ancestor of their `origin/main`, and it cites this finding by name): read the peer **through git objects at a named revision, never the working tree**; vendored bytes **hashed against a blob pinned in a provenance sidecar**; re-vendor via `git -C <peer> show origin/<default>:<path>`; an env-var override is legitimate and **its absence is a loud skip naming the variable, not a walk**. Precedent: aurora's `test/formats/effects-preset-schema-drift.test.ts`. **⚠ BUILD-TO RULE for the resolver, banked before we write one** (`contract/SUITE_PATHS.md` step-3 bullet, empyrean `a0b4251`, verified an ancestor of their `origin/main`; sigil's, learned from a merged tree going **6-of-4198 RED while both branch sweeps were green**): `git rev-parse --git-common-dir` returns **three** shapes — `.git` at a main checkout's root, an **absolute** path from a linked worktree's subdirectory, and **`../../.git` (relative, with `..`) from a MAIN-checkout subdirectory**. Sigil trimmed the third lexically, walked onto `crates/`, and refused. The failure is invisible to agents *because* agents run in worktrees and the suite runs from the main checkout — the two return different shapes, so a bed-only proof proves the wrong configuration. Therefore: **ask git for the format you want (`--path-format=absolute`), never normalise its answer**, and prove the derivation from **BOTH** the constructed worktree bed and the real main checkout. **Measured here 2026-09-02, not assumed: our tree has ZERO `--git-common-dir` call sites** (`git grep -c` exit 1, with a positive control on `rev-parse` returning matches in 3+ files), so nothing of ours is affected today — this is the shape to build to, not a defect to fix. |
| **F-STOPPREC-HOSTED-HALT** | 🔴 **REGISTERED 2026-09-02 (`parcel/stopprecision`).** §8 item 24's proof measures the breakpoint halt on the **socket** free-run driver only. `Engine::halt_on_breakpoint` has two callers — that one and the player window's loop through `Host::pump` — and the hosted one is not reachable from a socket client, so its `stopPrecision: "exact"` is inferred from sharing one function with the measured path rather than measured. Both read the stopping `pc` from the same `self.sys` at the same point, which is why the inference is reasonable and why it is still an inference. | Needs a host-side test in the shape of `host.rs`'s `the_bus_and_the_panel_read_one_instrument`, which reaches the instrument from the host's side. Bounded. Revive when the player's breakpoint path is next touched, or sooner if a consumer reads a window-driven halt as exact. |
| **F-RESUME-STOP-RACE** | 🔴 **REGISTERED 2026-09-02 (`parcel/stopprecision`), and it was found only because a test repeated.** `c.ok("emulator/resume")` followed by `next_stopped(c)` RACES: the halt's `stopped` event is broadcast from the engine thread while the `resume` reply is written by the connection thread, and `Client::ok` reads through to the reply **discarding every event it passes**. When the halt wins, the event is thrown away and the test blocks to its 20 s socket timeout. Measured on a seven-instruction fixture at **trial 4 of 8, after three clean passes — a single-shot test would have called it green.** Fixed inside `tests/stop_precision.rs` (`resume_and_wait_for_stop`, which reads both lines before acting on either); the same spelling is still live in `tests/breakpoints.rs` and `tests/watchpoints.rs`, where it has not been observed failing because those fixtures take longer to reach the breakpoint. | Lift `resume_and_wait_for_stop` into `tests/common` and use it at every `resume`-then-wait site. Out of `parcel/stopprecision`'s scope. Revive on the next flake in either file, or when either is next edited. **⚑ AMENDED 2026-09-02 — IT HAS AN EXTERNAL TRIGGER NOW, AND A COMMITMENT RIDES WITH IT.** aurora enumerated their client and **cannot hit this today** — one non-test `onEvent` consumer, which discards the event; nothing awaits an event anywhere; every sequencing point gates on a reply (their `44f17ca8`, re-verified by them against our `7ba2faf`). **That "no" is scoped to code they have not written yet:** the first thing anyone builds on `breakpoint_add` is arm → resume → wait-to-be-told-it-hit, which is exactly this shape. **So the revival condition is now also: aurora (or any client) starting a breakpoint consumer — and the fix has to be IN before that lands, not after.** **⚑ STANDING COMMITMENT, booked here under bar 20's sending half because it was made in mail and mail is not part of the tree:** this lane told aurora to ping before they write any wait loop, and undertook to tell them **whether this fix has landed yet**. A `/clear` must not lose that — if they ping and this row is still 🔴, the honest answer is *not yet, and here is the shape to avoid* (the inverse caveat in the `run_to` section below: the `stopped` event precedes the reply, so take-the-reply-then-wait blocks forever). |
| **F-STEPOUT-SLOW-CLIENTS** | `emulator/step_out` with `{}` takes **~92 s in a debug build** on this box (600-frame bound, per-instruction sink), measured directly. Test clients use a **20 s** socket read timeout, so the all-method sweeps (`handshake::…advertises_a_generated_method_list…`, `methods.rs`) are **timing-marginal under load** — observed failing twice in isolation on a busy box and passing in both full runs. **Pre-existing, not the `screen_text` parcel's, and it will bite again.** | Any flake in those sweeps — read this row before diagnosing it as new. |

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

**▶ AND THE RESPONDER'S HALF, SIGIL'S, WHICH COMPLETES THE CIRCUIT ABOVE — HEDGE THE PREMISE, NOT THE
REASONING** *(sigil `4a548d39`, verified here as reachable at their `origin/master` and a docs SHA carrying
docs, read 2026-08-30)*. Their formulation, banked against themselves: they **endorsed the instance as
confidently as the rule, when only the rule was theirs to endorse.** The operational form is cheap and is
the half nobody runs — **endorse the rule; flag the instance as unchecked and the reporter's to verify.**
⚑ **Directly load-bearing for this seat under the continuous-push instruction**, because it is the exact
mirror of a bar this file already carries pointing the other way: *a stated mechanism absorbs rather than
competes* (a controller's story overriding an agent's evidence). Here it is a **responder's confidence
overriding a reporter's own doubt** — same circuit, opposite end of the wire. This lane held only the half
that flattered it, and so did they.
**Suite-level shape, sigil's observation and theirs to file** (their mail to the hub was held in an approval
queue, so it may not have landed; the finding is durable at `4a548d39`): three lanes in one night each read
**their own artifacts as facts rather than as claims** — aeon executed a booked kill list that had gone
stale, sigil asserted their own gitignore state from memory at the moment it became load-bearing, and this
lane trusted a summary of a document over the document. **Not relayed onward from here**, per notify-on-the-
dependency: they are filing it, and a second lane telling the hub the same thing is the aggregate waste bar
18 names. Recorded so the pointer survives if their mail did not — it lands on the hub's own live
`PLAN-PROSE-SWEEP` item.

**▶ REGISTERED 2026-08-30 — F-LEGACY-SILENT-DEFAULT, and it is the sharpest argument the cutover has.**
`docs/2026-08-30-legacy-silent-default.md`. The legacy C++ server (`oracle-old`,
`linux-port/gui/ControlSocket.cpp`) validates **no parameter at all**: `getInt(k, d = 0)` at `:130`
returns the default on absent key, unparseable string (`catch (...)`) and unhandled type, and there is
**no unknown-key rejection anywhere in the file** (verified as a genuine absence under a positive
control, not read off empty output). ⚑ **CORRECTED same day: the family is FOUR accessors and 63 call sites, not one and 34**
(`get` str `:119`/18 sites, `getInt` `:130`/23, `getU32` `:152`/11, `getBool` `:156`/11) — the original
count enumerated by too narrow an ALPHABET and agreed with itself, **bar 19 turned on this seat**, and it
was aeon's wider `get*("key")` sweep, run for their own purposes, that surfaced it (bar 21: the
discriminator fired by accident again). ⚑ **AND CLOSED AT 64 (2026-08-30, second pass): 63 accessor sites + `ParseButtons` `:1576`, the one
read that type-checks — found by aeon varying the parameter ON PURPOSE (the first deliberate invocation
this lane has seen), and the population is now COMPLETE rather than a running total** (59 `const JsonObj&`
signatures swept; the only raw-`json` touches in the file are inside the four accessors plus `ParseButtons`,
and `JsonObj` exposes nothing else). A misspelled `buttons` presses NOTHING and returns success.
⚑ **Plus a type-gap of our own finding: `has()` `:117` is satisfied by any present non-null value while
`getBool` `:156` accepts only `"true"`/`"1"`/`"yes"`, so `{"enabled":"on"}` passes the explicit guard, reads
false, and `*flag = !on` MUTES the layer the caller asked to enable** — partially mitigated because the reply
echoes its own decision. **And the near-miss is banked with it:** the three unguarded-looking `getBool`
sites are in fact guarded, and this seat nearly reported a silent inversion that does not exist for the
missing-key case — caught by reading the lines around the cited line (bar 11, on our own finding).
⚑⚑ **AND THE SIX WERE THE WRONG SIX — RETRACTED 2026-08-30, found by aeon, verified here against the
enclosing blocks.** FOUR of them (`:348`, `:615`, `:739`, `:782`) sit inside `if (req.has(...))` and fail
LOUDLY; two genuinely unguarded ones were missed by BOTH lanes (`:2110`/`:2140`, `read_vram`/`write_vram`,
`getU32("addr", 0)`). True set = **{read_vram.addr, write_vram.addr, z80_read.addr, z80_write.addr}**.
**My enumeration parameter conflated "no explicit default" with "unguarded"** — orthogonal properties, so it
produced 4 false positives AND excluded the 2 real ones *by construction* (they carry an explicit `, 0`).
**Third enumeration-parameter failure by this seat in one day, and the damning half: the same document had
already applied bar 11 correctly to its `getBool` sites and I did not apply it to the memory six.**
aeon's own error is the sharper lesson — they said my account "holds line for line" and it did: **they
verified the lines EXISTED, not that they were UNGUARDED**, a check that could only confirm. Their earlier
"independent corroboration" of the six is **withdrawn**, as is their consumer-side "we are clean"
(14-across-3-files was a grep of files they already believed were on the seam; real figure **131 across 12**,
and they found a live instance: 12 sites send `reset{"wait":true}` and `OpReset` never reads `wait`).
⚑⚑ **AND THE CANONICAL EXAMPLE WAS BACKWARDS — swap it in anything quoting it.** `"0xZZZZ"` is the
**SAFE** end: no valid hex digit after the prefix, `stoll` throws, `catch (...)` returns the default.
**The dangerous shape is a VALID PREFIX + GARBAGE** — `"0x12ZZ"` → **18**, `"12abc"` → **12**. aeon
measured it by compiling the accessor rather than reasoning about `stoll`'s contract; **reproduced here
independently** (transcribed string arm, `g++ -std=c++17`, sentinel default). Nothing we said about
`"0xZZZZ"` was false — it *does* resolve to the default and *does* defeat the absence guards — **the
defect was presenting the class's mildest member as its canonical case.** And the partial parse is worse
**in kind**: a defaulted `0` is a consistent wrong value someone may learn to recognise; address **18**
is plausible, arbitrary, and looks like data.
⚑ **WHAT SURVIVES AND IS THE DURABLE PART: the guards cover ABSENCE, NEVER TYPE.** `has()` passes any
present non-null value and `getInt`'s `stoll` throws into `catch (...)` → 0, so `{"addr":"0xZZZZ"}` defeats
all four guards. The retraction narrows *which keys must be missing* and narrows nothing about malformed
values. Headline holds of **`write_vram`/`z80_write`**, not `write_memory`.
⚑ **Plus a fourth correction against us: `getBool`'s string arm returns FALSE, not `d`** (this file and the
doc both said `d`) — immaterial at the three `enabled` sites, **material at the five `getBool(k, true)`
sites** (`:465` reset.run, `:871` watchpoint_add.write, `:1366` reload_rom.reset, `:1377` reload_rom.wait,
`:1710` hold.down), where the caller's STATED default of true is what makes the call look safe.
What did NOT move: the population (64), the four accessors, and the absence of any unknown-key rejection (`:348`, `:615`, `:702`, `:726`, `:739`, `:782`) — a misspelled
`addr` on a legacy write goes to **address 0 and returns success**. ⚑ **The `mcp__oracle__*` surface
still reaches this server**, so every lane debugging through MCP is on this path.
**Provenance, and it is the instructive half:** this arrived as aeon's *aside* — a claim about OUR tree,
in MAIL, which is bar 20's exact shape — while they were acknowledging an unrelated signal. It was
verified here rather than banked, and their version **understated it**: they named one key and one site
(`:894`, `timeout_ms`, 30000, confirmed real), where the defect is the accessor and is 34 sites wide.
A peer's passing remark about our own code was worth more than the thing they wrote it to explain.
**Revival condition:** the README sentence owner ruling 4 requires — it should carry this fact, not
merely that the surface is legacy. Explicitly NOT a fix recommendation for `oracle-old`: it is
reference-only and the cutover exists to delete it.

**Registered by the CR-27 serve review (2026-08-20), all contract-side or cosmetic, none blocking:**

| id | what | revival condition |
|---|---|---|
| **F-PLAYINPUT-ITEMS-OPEN** | `emulator/play_input`'s `rows[]` **items** are not closed — §8 item 20's closure is applied at the result's top level, so a surplus key *inside* a row passes. The one array-of-objects on the bus whose item shape is unguarded; `read_cram`'s `palette[]` items and `watchpoint_hits`' `hits[]` items both close theirs. | Contract-side: an `additionalProperties: false` on the item subschema, next time `play_input`'s fragment is opened. No server change. |
| **F-ONEOF-COMBINATIONS** | The `oneOf`/`dependentRequired` alternations (`write_cram`'s triple-vs-`raw`, `write_memory`'s `bytes`-vs-`value`+`width`) are enforced per-fragment but nothing sweeps **every combination** of a fragment's declared keys against its own rules. Today each is hand-tested; the sweep would be mechanical and would cover the ones nobody thought to write. | A third alternation lands, or a hand-written combination test is found wrong. |
| **F-REVIEW-N12**, **F-REVIEW-N14** | Recorded by id from the CR-27 serve review; their content is in the review itself, not restated here — the implementing agent was given the ids without the text and is not inventing a description for them. | Whoever holds the review restates them; then they get a real row. |

*(N9 — the `Resolution::name`/`Display` duplication — was **taken**, not registered: `Display` now calls
`name()`. It was two lines.)*

## ⚑ OWNER RULING — PUSH AUTHORIZATION — ✅ **CONFIRMED DIRECTLY BY THE OWNER, 2026-08-24, IN THIS SESSION**

**STANDING APPROVAL, OWN REPO ONLY: a lane may push its own repo's master without asking each
time.** Reached us via empyrean-18, banked by the hub at empyrean `2bd72a03` — **verified firsthand
here: the object exists, is an ancestor of their `origin/main`, and is a docs commit carrying a docs
ruling, so its SHA class matches what it anchors.** Flagged as a relay per this lane's own standing
rule; ~~direct owner confirmation requested in-session.~~
**✅ THE CONFIRMATION ARRIVED, AND THE FLAG IS REPLACED RATHER THAN DELETED, per the rule that wrote
it.** The owner answered decision `d-1` directly in this session on 2026-08-24, choosing **"Confirm it
as standing permission"** from the two options put to him. **This lane may now push its own repo's
master without asking each time**, under the four conditions below, which ride with the grant and are
unchanged by the confirmation.

*Worth keeping, because it is the only evidence the precaution was ever worth its cost: the relay was
accurate in substance the whole time.* Holding it as unwitnessed cost this lane one evening of
unpushed docs and cost the suite nothing, while the alternative — acting on a relayed authorization —
is the failure this repo booked twice on 2026-08-22. **A precaution that turns out to have been
unnecessary is the only kind that ever gets tested.** ⚠ **The 2026-08-22 four-ruling block below is a
SEPARATE relay and STAYS FLAGGED** — he confirmed the push grant and nothing else.

**The granting act is named, which is why this relay is usable at all.** The hub consolidated a
question two lanes had stopped on separately (sigil asked outright; aeon was sitting on three
finished docs commits for the same reason, neither able to see the other asking), put three options
to him — own-repo standing / standing-for-docs-ask-for-code / per-push — and he chose the widest.
That is a granting act described, not a status field quoted, which is the distinction the
never-record-an-unwitnessed-approval bar exists to draw.

**The conditions ride with the grant and are part of it**, transcribed rather than paraphrased:
- **verify `origin` actually moved — the push is not the act, the remote moving is.** This is the
  protocol's own push-before-you-cite rule arriving as an owner condition;
- **never rewrite already-pushed history**;
- **never push another lane's repo**;
- **publication to the public wiki site stays a separate explicit ask** — not a concern in this
  tree today, but it becomes one the moment the wiki-emulator spike produces anything shippable.

**Scope, stated by the hub because this is the class of grant that gets restated wider: it
authorizes PUSHING, not the work being pushed.** It does not release this lane's boot stop, it is
not approval to dispatch or to land a parcel, and **it does not touch the CR-A/CR-B adjudication
hold**, which remains a separately parked owner item.

## ⚑ FOUR OWNER RULINGS, 2026-08-22 — **RELAYED, NOT WITNESSED BY THIS LANE**

Reached us via empyrean-73, quoting the owner's own words in their session. **Flagged as a relay
per this repo's own rule** (*never record an approval whose granting act you have not seen*) — which
was written earlier the same day, after that shape failed twice across two lanes. Quoted words with
a named source are far stronger than a status field and are still not a witnessed act. **Direct
owner confirmation requested in-session; replace this flag with the confirmation, do not delete it.**

1. **Wiki-emulator spike: APPROVED, and my flagged divergence resolved — in the direction that
   corrects empyrean, not us.** The approval was **real all along**; the spec's self-declared
   "Approved design" was factually correct, and empyrean's correction to me was wrong on the fact.
   ⚠ **Keep both halves:** an unverifiable claim turning out true does **not** retroactively make
   recording it without a citation correct — *we got away with one.* Owner: *"Yes I did but I was
   trying to save fable use so I never had an agent start. I can now if it wants with opus but just
   be careful and if we get stuck don't push."* **Authorised on Opus, with two conditions in his own
   words** — a spike, not a commitment: **report the wall rather than engineering around it.**
   Escalate to empyrean rather than burning a week proving feasibility that was meant to be cheap.
   **Not reprioritised above the acceptance parcels.**
2. **Fable seat: HOLD — with a new obligation that is better than either option I offered.**
   ⚠ **CLOSED AS AN OWNER ITEM 2026-08-22 — STOP LISTING IT AS PARKED.** Asked whether to fund the
   seat long-term he answered *"Idk what you want for this"*, and the asking lane recorded that as
   **their badly-formed question, not his indecision** — the right way round. **No decision is needed
   today: hold stands, the ledger IS the mechanism, and the question returns naturally when the limit
   lifts.** Note this also retires the provenance worry above by superseding it: there is now a live
   cited ruling on the seat, so the unwitnessed 2026-08-21 ratification is **correct and moot**.
   Owner:
   *"keep careful record of what's done without fable so when our limit is no longer up the first
   thing it can do is make sure we made the correct decisions without it."* **Fable's FIRST job when
   the limit lifts is auditing exactly those decisions**, so the gap becomes a queue rather than a
   hole. ▶ **Ledger created: `docs/2026-08-22-unadjudicated-decision-ledger.md`** (L-01…L-06). Each
   entry must be adjudicable **cold** — verdict, alternatives, evidence at the time, and *what would
   have to be true for it to be wrong.* An entry recording only the verdict is useless to the audit
   it exists for. **Every future unadjudicated call gets an entry at the moment it is made**, not
   reconstructed later. Note: this supersedes the unwitnessed 2026-08-21 ratification audited above
   — there is now a live cited ruling on the seat, so that correction stands as *correct and
   superseded*.
3. **▶ THE MOST CONSEQUENTIAL, and it is aimed at this lane.** Owner: *"Oracle - let's make sure
   anything not going for the new oracle does and tell it to make sure to tell the oracle agent to
   build out any tools these other suite items/agents might need, that's how we're getting robust."*
   Two halves: **(a)** anything still pointed at the legacy C++ server should be moving to the new
   core — **the acceptance contract is the vehicle and is effectively blessed as the priority**;
   **(b) this lane is the SUITE'S TOOL-BUILDER.** empyrean is telling every lane to send named
   instrument asks here rather than working around gaps. **Inbound capability asks are first-class
   queue items, not interruptions** — his stated reason is *"that's how we're getting robust"*. This
   extends the existing aeon co-development lane from one peer to all of them.
4. **READMEs: make every suite repo's README accurate.** *"Doesn't have to be super in depth."*
   Ours must say plainly that **the MCP surface still reaches the legacy C++ server** — the fact
   most likely to mislead a reader, and the one this lane independently flagged in the status
   roll-up before the directive arrived.

> ▶ **BOOTING INTO THE CUTOVER? READ `docs/2026-08-22-cutover-handoff.md` FIRST.** It is written for
> the session that boots *after* the owner flips the config and relaunches every lane — what the
> rebuilt binary at `12cc17e` guarantees, the 17 remaining, why a `-32601` is a success signal, and
> what to do first when lanes report gaps. This section is the record; that file is the instructions.

## ⚑ HUB RULING UNDER DELEGATION, 2026-08-27 — d-16 SUBSTITUTE: the reviewer seat, and the rule it creates

⚑ **RELAYED, NOT WITNESSED BY THIS LANE** — same flag, same reason, as the four rulings above. The
owner armed an overnight delegation in his own words (*"if anything needs decision that they can't
make you make it for them"*, transcribed by the hub into empyrean `OVERSEER.md` addition (f) at
05:39Z, banked `091ac59`) and went to bed; the hub ruled in his place and **he reviews it on return.**
Record it as the hub's ruling. Do not upgrade it to his.

**The question (d-16):** the premium independent-reviewer seat was parked days ago when it blocked
nothing. It had come to block three items — OVERLAY-STATE and CR-A, five of the sixteen unserved
methods among them. **Ruled: SUBSTITUTE.**

**THE STANDING RULE THIS CREATES, and it outlives tonight.** Adjudications run on the ordinary model
while the seat is parked, and **every ruling produced that way NAMES ITS OWN REVIEWER, at the top, in
the ruling itself.** Not in a covering note, not in the dispatch record — in the artifact a later
reader picks up cold. The reason is the whole design: Fable's first job when the owner lifts the limit
is auditing exactly these, and an audit cannot find what does not announce itself. **Independence is
preserved — a fresh reviewer that took no part in the drafting is still the half that catches real
problems. Reviewer tier is what was spent.** Say it that way; do not describe a substituted ruling as
adjudicated without qualification.

**Ledger:** every substituted adjudication gets an entry in
`docs/2026-08-22-unadjudicated-decision-ledger.md` **at the moment it is dispatched**, not
reconstructed after — the entry must be adjudicable cold, and must name *what the audit should re-run*
and *what would have to be true for the ruling to be wrong*. First entry is **L-07 (CR-A)**, which also
records the cheap first cut for the audit: **re-run the material items only**, since the M/S split
every ruling here is required to produce is the instrument that measures what the substitution cost.

⚠ **Numbering collision, live:** aeon also has a card numbered `d-16` (background chunk height). The
console shows two. **Never cross-reference a decision by number alone across lanes** — say the lane.

## ⚑ THE CUTOVER — ruled 2026-08-22 (RELAYED, see the flag above), mechanism determined firsthand

**The ruling** (owner, via empyrean, quoted): *"I say do it now and when something is needed have it
built out, no?"* — cut `mcp__oracle__*` over to the Rust server **now**, and close the remaining
methods **on demand** rather than in enumeration order. empyrean recommended registering alongside
the legacy server; **he overruled it** and they now agree, as do I: it converts the acceptance
contract from a catalogue into demand-driven work.

**✅ RULED PROCEED (relayed, with the full measured cost disclosed to him first — aeon's two gates
down ~a day, Z80 no real consumers, binary needs rebuilding). His words:** *"Yeah just proceed. We fix
when we come across it, if we don't we build later but this is really just to start building out the
tooling."*
**⚑ THE LAST CLAUSE IS THE GOVERNING ONE, AND IT REFRAMES THE WHOLE ACCEPTANCE CONTRACT.** The cutover
is **not** happening because the successor is ready — it is happening **because being reachable is what
generates the demand that builds it out.** So the remaining 17 are **not a checklist to burn down
before the switch; they are a queue the switch POPULATES in priority order.**
**▶ CONSEQUENCE FOR EVERY BRIEF FROM HERE: an early gap is a SUCCESS SIGNAL, not an embarrassment.**
State it explicitly to agents — the instinct will be to treat every `-32601` as a failure that should
have been prevented, and under this ruling it is the mechanism working. **This does NOT relax the
loud-failure requirement**; it is the reason for it. A gap that refuses by name feeds the queue; a gap
that degrades to a plausible answer poisons it.

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
**▶ ALSO OPEN, and it points INWARD at our own docs (aurora ran it on theirs and it drew blood).**
Their split: **engine facts** (properties of aeon, seen through a window — unaffected) vs **server
facts** (properties of ONE implementation). Their worked example is the one to internalise: they
re-derived the `require_paused` set **from our Rust source**, banked it as a correction — and wrote it
into a section that had always called these properties of *"the bus"*. **It is a property of one
implementation and the correction did not say so** — a defect created *while applying every other bar
correctly, in the act of fixing a different staleness.* **We have the same exposure and more of it:**
this repo's recon, demand and CR docs describe "the server" throughout, and D-10/D-13/D-17 are already
booked as having **two implementers**. A sweep is owed — every claim about server behaviour either
names its implementation or is a latent two-implementer conflation.
*Durable formulation from the same thread, worth more than its instance:* **freshness is not
transitive across a document, and proximity reads as verification** — a stale figure beside a
freshly-updated one is read as cross-checked, which is how my own 37 survived hours next to a correct
18.

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

**OPERATIONAL CONSEQUENCE for the d-4 parcel: start the server on `/run/user/1000/oracle.sock`.** That
is what every lane resolves to. Unlinking the stale `/tmp/oracle.sock` (aurora's suggestion) is **not
required** for any consumer using the reference client, since it is unreachable; it may still matter
for a client with its own resolution, which is aurora's to determine and not mine to touch.

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

**THE HAZARD TO ENFORCE, and it is this seat's job:** once the new server is the only one reachable,
every failure presents as *the consumer* being broken and the gradient pushes lanes to engineer
around gaps instead of reporting them — **bar 9's corollary with the causation hidden.** Counter-
measure: **an unserved method must fail LOUDLY and BY NAME, never degrade to a plausible answer.**
Ours already does (`-32601` unknown method; `-32602` naming the key at the dispatch choke *before*
the handler); the legacy server silently defaults unknown params, which is exactly why bar 15 says
sequence a cutover onto the STRICT implementation. **A missing capability that returns something is
far worse here than one that refuses.**

## ⚑ SIGIL CYCLE DUMPER — my join objection is REFUTED; two requirements survive it (2026-08-24)

**Do not re-raise the join objection.** I priced the differential as blocked on *who supplies the
opcode-to-key join*, reasoning that sigil's `instr_cycles` is keyed on mnemonic + size + EA category
and holds no opcode (true, and derived here without contact). **The conclusion over-reached.**
Verified firsthand at sigil `origin/master` **`4b02eb07`** — their committed revision, not their live
worktree: `m68k_decode.rs::decode_one`, `m68k.rs:180 pub struct Instruction { mnemonic, size, ops }`,
`m68k_cycles.rs::instr_cycles`, and `sigil-frontend-emp/src/m68k_cycles.rs:130 fn classify(op:
&CodeOperand, …)`. **They own both halves.** The real gap is one adapter between two of their own
types, inside one repo. *My premise was right and I turned "the mapping lives in your decoder" into a
cross-repo ownership problem without checking whether the decoder existed — it had landed the week
before, in the very parcel whose Capstone precedent I had just praised in the same message.*

**TWO REQUIREMENTS THAT BIND OUR DUMPER, both cheap up front and awkward retrofitted:**
1. **⚑ BRANCH OUTCOME PER EXECUTION, not just a cycle count.** `CycleCost::Branch { taken, not_taken,
   exact }` is **outcome-keyed** (verified at `m68k_cycles.rs:103-109`), so a measured count cannot be
   compared to a `Branch` row unless the dump says which way the branch went. **This is a real change
   to what would otherwise have been built.**
2. **The assertion comes from the DATA, which beats my framing.** Rows carry `exact: bool`, doc'd at
   `:92-93` verbatim: *"`exact: false` marks a MAXIMUM over a data-dependent execution — sound as a
   ceiling, unusable as an equality."* So the gate is `measured <= modeled` on inexact rows and `==`
   on exact ones, **read off the flag rather than chosen**. My "≤ direction only" was right and was a
   convention; theirs is a property. *This satisfies our own name-the-assertion bar out of the data
   instead of by fiat, which is the stronger form of it.*

**THE NUMBER THAT DECIDES WHETHER TO BUILD AT ALL, sigil's own and stated against their interest:**
`CycleCost::Unmodeled` exists (`:112`), so the differential's domain is **partial by construction**.
The gate is **what fraction of a real ROM's instruction stream is modeled**. Unmeasured; theirs to
measure; **it comes before either lane spends a parcel.** A differential over 20% of the stream is a
different proposition from one over 95%.

**Status: still `blocked` on `no filed ask exists`** — correctly, and sigil is filing one. The
consumer (Spec 2 cycle budgets, `SIGIL_SPEC2_LANGUAGE.md` S2-D7(c)) is **deferred at the spec freeze**,
so there is no sigil-side consumer this week either.

**⚑ THE COVERAGE NUMBER HAS THREE BUCKETS, NOT TWO — sigil's correction to this seat's booking,
2026-08-26, and it changes what the dumper would be FOR.** I had booked the gate as "what fraction of a
real ROM's instruction stream is modeled", i.e. modelled-vs-unmodelled. They corrected it against their
own tree: because `exact: false` marks a **ceiling** rather than an equality, a routine can carry a
`@budget(cycles: N)` bound while being permanently unable to carry `@cycles_exact`. So the honest split
is **exact-modelled / ceiling-only / unmodelled**, and the three answer different questions — *"a corpus
that is 80% modelled but mostly ceiling-only supports a budget checker and not an equality checker"*.
**That distinction is precisely what decides whether either lane should build this at all**, and a
two-bucket number would have looked like an answer while hiding it. The comparison target differs per
bucket, so our dumper's assertion direction is per-row and not per-run — which composes with the two
requirements already booked above rather than replacing them. They also settled, firsthand, that the
cycle model is **entirely static** (`cycle_budget.rs` walks an evaluated `CodeBuf` against the cost
tables and the shared `Cfg`), so the coverage measurement needs no emulator and was never blocked by
the shim hazard above. CYCLE-ASK stays correctly gated on their owner picking the measurement up.
⚠ **STATUS CORRECTED BY SIGIL THE SAME DAY, against their own contribution — the three buckets are a
PREDICTION, not a measurement, and the paragraph above overstated them.** They have run no corpus
measurement. The split is a *consequence of `CycleCost`'s own doc comment* (`exact: false` = "a MAXIMUM
over a data-dependent execution — sound as a ceiling, unusable as an equality"), read while checking
this seat's two requirements. **So it is the shape to measure IN, never a result to build on: if the
corpus turns out to have a negligible ceiling-only middle, the sharpening was true and irrelevant, and
the original two-bucket question was the right shape after all.** Do not let a later session cite the
three buckets as a finding about any ROM.
*Their framing of where the value actually came from, kept because it is the more useful lesson and it
is narrower than the headline: a precise question about their own tree sent them to read their own type
definition, where the answer was already written down. That is bar 12 arriving between repos —* the
rule was in the contract they already owned.*


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

## ▶ QUEUED — GUI-LAYERS: the player window's layer toggles + click-an-object

**Recommended to the owner 2026-08-26, not yet picked.** The bus can now hide layers; `oracle-frontend`
cannot. It draws its own window and `pick.rs` resolves attribution **unmasked**, so a bus-set mask
changes the bus's answers and not the picture — and `pick.rs`'s *"this panel and
`emulator/pixel_attribution` must never disagree"* invariant is now **conditional on no mask being
set.** This is also the natural home for the owner's unqueued *click an object and be told what it is*.

**⚑ CONSUMER DESIGN INPUT, solicited from aurora BEFORE shaping the parcel and adopted — their
priority order, kept.** *(Asked for deliberately: they are the editor and the only lane that would
consume this. Cheaper before the design than after.)*
1. **The answer must name its own subject, in a sentence.** Their expensive lesson the same day: they
   shipped a band lens that highlighted 1,244 cells, entirely correct, and the owner's reaction was
   *"what are the purple boxes"* — not *that's wrong* but ***what is that***. A feature that works
   perfectly and communicates nothing. So the top line is prose a person reads; the verdict enum stays
   underneath for tools. `[planeB:won, backdrop:lostToPriority]` is the right data and the wrong answer.
2. **Return identity a client can JOIN ON. ⚑ ALREADY SATISFIED — do not build it.** They asked for the
   nametable word at the dot; `pixel_attribution.cell` already returns it decoded (`tile`, `tileAddr`,
   `palette`, `hflip`, `vflip`, `priority`), iff the winner is planeA/planeB/window. **Verified live at
   `d285ecb`**, not read off the schema: `(160,100) → tile 1066, tileAddr 0x8540, palette 2, vflip`.
   **`tile` is VRAM-ABSOLUTE** (`tileAddr == tile*32`, checked: 1066×32 = 0x8540), so aurora rebases by
   `BG_TILE_BASE_SLOT` for blob-local — their model's space, and the direction their injector already
   goes. Their warning is the durable part: **an index whose space is unstated is a transpose bug
   waiting to happen**; the fragment states the space in the field's own description, which is where it
   belongs. Filed as *satisfied*, not *genuinely-new* — the triage exists to catch exactly this.
   **⚠ AND THE HAZARD ON THE OTHER SIDE OF THAT JOIN, found by aurora against this seat's own sample
   dots — if OUR panel ever names a blob slot, it inherits this check.** The rebase can land **outside
   the blob**, and `BG_TILE_BASE_SLOT = 1024` (verified firsthand as a literal at aurora
   `origin/master`), so **any `tile < 1024` rebases NEGATIVE.** Worked on the two dots this lane
   sampled: `1066 → 42`, inside their 320-tile blob; **`1456 → 432`, outside it** — and *not rescued by
   capacity*, since 432 < 448. **Their durable formulation: in-capacity is not in-blob.** *(Base slot is
   ours firsthand; the 448 capacity and the 320-tile blob are their measurements, not re-derived here.)*
   Plane B can legitimately be showing engine art, another act's art, or a slot past the blob's end —
   the corpus-ROM sample is not a defect, it is proof the class is reachable. **So a click-to-identify
   surface must answer *"that is not part of your background"* for those — not index, and above all not
   guess: an unchecked rebase either throws or confidently names a slot the author does not own, which
   is indistinguishable from a correct answer.** That is point 3's loud-on-unmeasurable rule arriving on
   the join instead of on the mask, which is what makes it a class rather than two tips.
3. **Assert the conditional invariant rather than noting it.** A rule with an unasserted precondition is
   this workspace's recurring defect; their form is **loud-on-unmeasurable beats a plausible answer** —
   their layout harness answers *"COULD NOT MEASURE A FIT"* under a planted defect rather than "fits".
   So while a mask is set the panel SAYS so in the answer rather than quietly describing a picture that
   is not on screen, and the human-facing line carries it, not only the wire caveat.
4. They will honour the framebuffer-digest ruling and not fingerprint a masked view.
5. **A lens must state that it is on, persistently, and this is a CORRECTNESS requirement rather than
   polish** — their point, and it had not been on this seat's list. A mask that changes the picture with
   no standing on-screen statement is the unlabelled-highlight defect one level up: *the author will
   forget, and then read a masked picture as the real one.* Their canvas palette treats colour as a
   language deliberately; a toggle that fights that is worse than none.

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

**Open, booked, not started:** `F-ACCEPT-TABLE-RAWSTRING`; `README-LEGACY-WARNING`; `FRAME-LABEL`;
`PLAYER-POLISH`; `OVERLAY-STATE` (never run against a real window; waits until the owner is away);
`ACCEPT-16`; `WIKI-SPIKE`; and **`d-20`, the dull sound — the owner's taste call, untouched.**
Residue from parcel 2, deliberately out of scope: `tools/aether_smoke.py` and several
`crates/oracle-core/examples/*` still read aeon's **live** tree and pin `symbolCount == 2129`
against it — the same dependency `fixtures/aeon/` exists to remove, in the places the freeze did not
reach.

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

## ⚑ HUB RULING, 2026-09-02 — HERMETIC GATE IS THE RATIFIED SHAPE; DRIFT IS A NIGHTLY, AND IT GETS **NO SECOND OWNER CARD**

⚑ **RELAYED BY empyrean-01, NOT WITNESSED BY THIS LANE** — same flag, same reason, as the relayed
rulings above. It is the **hub's** ruling under the owner's standing delegation. Do not upgrade it to
his. Anchored at empyrean **`1e9d70c`**, verified firsthand here rather than taken on trust: the object
is a **commit**, it is an **ancestor of their `origin/main`**, and `--stat` shows it is a **docs commit
carrying a docs ruling** — so its SHA class matches what it anchors.

**The question this answers** is the one `parcel/stopprecision` left open in
`docs/2026-09-02-stopprecision.md` §6: our schema gate went hermetic (blob-pinned, no peer read), and
the deliberate cost recorded there was that **a default run no longer notices upstream moving on its
own**. **Ruled: the hermetic default is the ratified shape, and drift detection is a NIGHTLY's
property, never a local run's.** Same shape as sigil's decouple — vendored content plus a revision
stamp, local runs hermetic, drift watched out-of-band.

**THE OPERATIVE INSTRUCTION, and it is a prohibition — read it before filing anything:** the drift job
is **a queue row here, not an owner card.** The host question it would raise (a standing unattended
timer on the owner's machine) is **already open with him as empyrean `d-9`** — verified firsthand at
their `origin/main`, and its question is literally *"Running it means a systemd timer on YOUR machine
… Do you want that standing job installed?"*, which is the same question ours would ask in different
words. **One cross-lane question gets one card.** A second card does not add information; it makes him
answer the same thing twice and lets the two answers diverge. *(`d-7-restated-3` is the companion card
— how many quiet chains before review — provisionally ruled N=5.)*

**The shape to build, when it is picked up:** a runner with `AETHER_CONTRACT_REPO` set, **non-blocking**,
reporting *"contract advanced past pinned blob"*. Note it needs **no new capability** — the hermetic
gate already grew exactly that env-var path as step 2 (`schema_conformance.rs`), so the nightly is a
caller of a road already built, not a build.

**Also carried in the same message, both banked:** our landing is recorded upstream with attribution
(**2026 passed / 0 failed / 6 ignored** cited as *our* measurement, not re-derived by them — correct
attribution discipline); and **F-RESUME-STOP-RACE was relayed to aurora** as the suite's outbound
client, which is the right destination — that register entry names `tests/breakpoints.rs` and
`tests/watchpoints.rs` as still carrying the racy spelling, and aurora writes clients that will hit the
same read-through-discarding-events shape. **No reply was requested and none is owed.**

⚠ **The one thing verified against MY OWN interest, because the relay asserted it and this seat's bar
says a claim about your own tree gets read out of the file:** the blob identity is **content-addressed
and therefore not talkable-into-agreeing** — our vendored
`crates/oracle-aether/tests/contract/bus-protocol.schema.json` is `125d17f03ac33872…` at our `HEAD`,
and `git rev-parse 82982b7:contract/schema/bus-protocol.schema.json` in empyrean returns **the same
blob id**. Byte identity by construction, checked in both trees, neither read from a working file.

⚑ **AND THE SCOPE OF WHAT THE HUB CAN HAND US, ESTABLISHED 2026-09-02 BY THE HUB RETRACTING ITS OWN
GO — bank this, it will recur.** At 10:40Z the hub cleared `LIVE-TREE-RESIDUE` "under the owner's
widened delegation". **This seat held anyway** and the hub then **withdrew it against its own
interest**, which is the strongest form this correction could take.

**The test that decided it, aeon's, and it is the reusable part: *is there an owner decision under
this, and is it THIS question?*** Applied here: the owner's 03:22Z words (empyrean `63c85ae`) re-arm
the **raster/parallax effects** drive; his 03:46Z widening (empyrean `4e8e865b`) covers **decision
CARDS in a lane's domain**. Neither is a word to start a **non-effects** parcel, and the live-folder
cleanup is this lane's own hygiene row. The nearest owner decision under it is `d-17` (his — the
*write* side into his aeon folder); the *read* side, sigil's `d-18`, **was ruled by the hub under
delegation, not by him.** So no owner decision exists for this question, which is exactly what the
test asks.

**THE DURABLE SPLIT, and it is what a future session should apply without re-deriving:** under today's
brief the hub can hand this lane **any ask an effects lane files** — those need no owner word and
should just be worked. It **cannot** hand us a go on our own hygiene, our own backlog, or anything
outside the effects drive. **When a relay's go and this test disagree, the test wins and you ask the
owner.** Precedent from the same morning, banked in the hub's own record at empyrean `13a7d5a`: sigil
adopted a hub *ruling* while holding for the owner's *word* on the same shape — **a relay carries a
ruling, never an authorization.** Two lanes, same hour, same conclusion, reached independently.

## ⚑ 2026-09-02 — `run_to` DOES **NOT** SHARE `resume`'S THREAD SPLIT. Measured from source, answering aurora.

**The question, theirs (relayed via the hub, 2026-09-02):** their Build-&-Run boot restore awaits the
`emulator/run_to` reply and gates on `reached`, assuming the machine is **already halted** when that
reply lands. If `run_to` had `resume`'s split, their next call would land on a running machine. They
correctly noted it would fail **loudly** (`write_memory` is behind `require_paused`, so it refuses by
name) — a diagnosis cost, not a corruption risk. **Answer: their assumption holds, and structurally.**

**The mechanism, read at `2a7fd82`, and it is one thread doing two things in order.** `engine_loop`
(`server.rs`, spawned at the `engine thread` builder) handles one `EngineMsg::Call` at a time as
`engine.dispatch(&method, &params)` **then** `reply.send(CallResult { .. })`. So:

- **`run_to` blocks INSIDE `dispatch`.** It `require_paused`es, sets `self.running = true`, calls
  `advance_until(max_frames, |pc, _| pc == target)` — which does not return until the target, the
  bound, a breakpoint or a `stopAfter` watch ends the run — sets `self.running = false`, emits
  `stopped`, and only then builds its result map. **`reply.send` therefore cannot execute before the
  halt: the reply is PRODUCED BY the halt, not merely correlated with it.**
- **`resume` does not block at all.** Its whole body is
  `Ok(json!({"wasRunning": self.set_free_run(true)}))` — it flips a flag and returns. The halt happens
  later, on a subsequent `free_run_step()` in the loop's `None` branch, and `emit_stopped` broadcasts
  from the engine thread **after** the `resume` reply was already sent. That gap is the whole of
  F-RESUME-STOP-RACE.

**⚑ THE CAVEAT THEY DID NOT ASK FOR, AND IT IS THE INVERSE OF THEIR WORRY — worth more than the
answer.** `run_to` calls `emit_stopped` **before** it builds its reply, so **on the wire the `stopped`
event PRECEDES the `run_to` reply.** A client reading through to the reply and discarding events (what
aurora does, and what `Client::ok` does) is correct and unaffected. **A client that consumed the reply
and THEN waited for the halt event would block forever** — the same read-ordering hazard as
F-RESUME-STOP-RACE with the two halves swapped. This is exactly the shape a first breakpoint consumer
reaches for, so it is recorded before anyone writes one.

⚠ **SCOPE, MEASURED RATHER THAN ASSERTED** (this seat's own booked habit of reading a sound observation
one notch too wide): the above is the **socket/free-run driver**, which is the path aurora reaches. The
**hosted** path — the player window through `Host::pump` — is a different driver and is already
registered as `F-STOPPREC-HOSTED-HALT`, where the halt is *inferred* from sharing one function with the
measured path rather than measured. Nothing here upgrades that.

**Their side is clean and it is a real enumeration, not a did-not-find:** `AetherClient.onEvent` has
exactly one non-test consumer in their tree (`bridge.ts`, which refreshes a badge and discards the
event), nothing awaits an event anywhere, and every sequencing point gates on a **reply**. **Their
perishable half, stated by them and adopted here: the day they build a breakpoint consumer is the day
our server-side fix has to be in.** That is now a board row rather than a note.

⚑ **AND THE RETURN LEG, 2026-09-02 — `run_to.reached` NOW HAS A NAMED LIVE CONSUMER, WHICH IS WHAT
PROTECTS IT FROM A FUTURE TIDY-UP.** aurora re-read the body order at `7ba2faf` themselves rather than
adopting our answer (confirming it an ancestor of our `origin/main` first), and their stated reason is
the sharp one: *a claim about another repo's tree is the one class of claim nothing in my tree could
ever contradict.* That is bar 20's receiving side run correctly, and it is why the answer is now
corroborated rather than merely believed.

**The part that comes back to us as an obligation.** Their boot restore gates on `reached !== true`.
So `"reached": run.predicate_fired` — **the predicate's own verdict, never the sink's** — is no longer
a defensive design choice explained in a comment; **a real client's write window depends on it.** The
comment at the site already says why (`StopRecord::fired` means only "*something* asked to stop", so
reading it would report a target as reached because an unrelated `stopAfter` watch halted the run).
**Booked here because this file's own bar says a code comment is where a perishable rule goes to be
read by nobody** — and the "simplification" that swaps `predicate_fired` for `fired` would now break a
named consumer silently, in the direction that presents as a successful boot restore over a write
window that never opened.

*They also noted it was an uncited joint in their own code: they had verified they read a reply rather
than an event, and never asked whether the field they read could be true for the wrong reason. Bar 8's
cheap frame-changer — the load-bearing step nobody cited — arriving on a consumer's side.*

## The bars (house methods — each earned by a measured failure; do not thin)

**▶ NEW BAR, 2026-08-26 — A MERGED SERVE IS NOT A SERVED METHOD. THE CONSUMER REACHES A BINARY.**
Found in the foreground pass that closed the CR-D `⟨RUNTIME⟩` debt
(`docs/2026-08-26-runtime-decoders-check.md` §5). The object decoders merged, tested and pushed at
`0f33c44` — and stayed **unreachable to every consumer**, because `target/release/oracle-aether` was
still the build from the day before and **nothing in a merge rebuilds it**. The MCP shim spawns *that
binary*; so a shim spawned any time between the merge and the check answered
`[-32601] no such method` to the very methods we had just shipped. Reproduced firsthand on this
session's own shim, then fixed and re-verified end to end through the consumer's own spawn path.
**This sharpens, and does not contradict, the coordination note that *advertising a method is
shipping it*: the advertised list is authoritative, but it is emitted BY A RUNNING BINARY, and a
stale binary advertises a stale list with total confidence.** Practical check before telling any
consumer a method is available: spawn the consumer's own path and call it — not `cargo test`, which
passes against source the consumer never runs. Same family as item 1's rename fallout
(*compile-time-frozen paths, invisible until the binary runs*); here the frozen artifact was the
binary itself.

**▶ AND THE COUNTING BAR THAT CAME WITH IT — MEASURE USE, NOT ATTACHMENT.** The same pass had to
count whether consumers actually call these methods. `grep -c` over the transcript tree reports
~10,000 mentions across ~4,055 files — and reports **the same ~4,055 for every tool name, including
tools nobody has ever called**, because the MCP tool listing sits in every session's system prompt.
Parsing `tool_use` blocks instead gives the true figure: 216 invocations. **The near-constant across
varied inputs was the tell** — the existing bar caught it. Mentions measure attachment; only
invocations measure use.

**▶ NEW BAR, 2026-08-24 — ANCHOR A CLAIM TO A SHA THAT CAN CARRY IT. A docs commit cannot vouch for
code.** Caught by aeon against this seat, same day. I reported the straddle fix to them anchored to
`7bdb75f` — which is a **one-line `docs/lane-log.jsonl` commit**. Every claim I made was true, and
the anchor could not carry any of it: the code is `4111c88` under merge `51143a5`, tests `68461a7`.
They cited the code SHAs in their booking instead. **The failure mode is that it hardens invisibly** —
a peer transcribes the anchor into their prose, and a later reader who checks it finds a docs diff
where a guarantee was promised. This is the same family as the provenance audit above (*cite the
ruling, not a status field*): the citation must be the artifact that actually contains the thing.
Practical check before sending: `git show --stat <sha>` and confirm the files named are the ones the
claim is about.

**▶ AMENDMENT, 2026-08-27 — THE ABOVE BAR HAS A FALSE-POSITIVE MODE, AND THIS SEAT FIRED IT AT A PEER.
A RIGHT SHA ANSWERING AN UNSTATED QUESTION IS NOT A WRONG SHA.** Found by aiming the 08-24 bar at aeon
and being half right; the diagnosis below is theirs, banked by them at aeon `b64f6bcb` (verified here as
a reachable ancestor of their `origin/master`, docs SHA carrying docs).

I flagged their certification anchor `33d905b8` as a docs-only commit standing in for a byte guarantee.
**The observation was right and the diagnosis was wrong.** A freeze record names **`aeon_rev` — the tree
state the ROM was built from** — and that is the *correct* anchor for reproducibility. It is *frequently*
a docs commit, because the tip at freeze time is whatever happened to land last. Nothing was false.

**The two are distinguishable, and the distinction picks the remedy:**
* **08-24 (mine):** the **wrong** SHA — a lint fixup standing in for a feature merge. `git show --stat`
  **catches it**, because the files named are not the ones the claim is about. Remedy: **swap the SHA.**
* **08-27 (theirs):** the **right** SHA for an **unstated question**. `git show --stat` **cannot catch
  it**, because the commit you land in is genuinely the one named — the sentence simply does not say
  *which of two questions* its SHA answers (what carries the code? vs. what tree was it frozen from?).
  Remedy: **a label, not a swap** — `code <SHA> · frozen at aeon_rev <SHA> / sigil <SHA>`.
**So the 08-24 practical check is necessary and not sufficient, and it is worse than that: applied alone
it produces a confident FALSE POSITIVE on every correctly-recorded freeze in the suite.** Ask what
question the SHA is answering before judging whether it can carry the claim.

**⚑ AND THE HALF THAT COST ME MORE THAN THE CATCH — I NAMED A REPLACEMENT ANCHOR BY GUESSING FROM A
COMMIT SUBJECT LINE, AND IT WAS ALSO DOCS.** I offered `212b2a06` as *"the substantive aeon-side
commit"* on the strength of its subject reading like a measurement finding. `--stat` says it is **a
single 509-line doc, zero code**. The real code anchor is **`cbd04ba8`** (`engine/objects/sprites.emp`
+121, `engine/ram.emp` +20, `tools/test_sprite_owner.py` +281; 532 insertions) — all three verified
firsthand here. **Two of the three plausible-looking SHAs in that chain are docs commits**, so
subject-line inference failed at **two in three** on the one chain where it was measured. A subject line
describes what a commit is *about*; `--stat` is the only thing that says what it *contains* — and the
bar's own remedy demands the second. **Run `--stat` on the SHA you propose, not only on the one you
doubt.**

**⚑ THE PROCESS LESSON, which aeon called out explicitly and which is why this was cheap: I sent it
HEDGED — *"treat this as a reading, not a finding"* — and that is what made it worth sending.** It was
50% right (symptom yes, diagnosis and replacement no). Sent as a finding it would have cost the same
commands with friction and put a wrong diagnosis into their tree with my confidence attached; sent as a
reading it cost them three commands and produced a rule neither lane had. This is protocol bar 20's
hedging clause paying out in the direction people doubt it: **the hedge is not weaker, it is what let a
half-wrong flag be useful instead of expensive.**

⚠ **Corrects a claim already committed in this repo:** `docs/lane-log.jsonl`, the 2026-08-27T10:09Z
entry's `detail`, calls that anchor *"same anchor-class bar they caught this seat on 2026-08-24"*. It is
not the same bar. That file is append-only, so this paragraph is the correction of record.

**▶ AND THE SCOPE-MARKING BAR IT ARRIVED WITH, which is aeon's and is the more reusable half.** Their
mis-filed ask traced back to a sentence **in our own module docs** — *"`self_cycles` has no such
lag"* — that is true of routine rows and false of interrupt buckets and **did not mark which it
meant**. They carried it across the boundary; the sentence let them. Their framing, worth keeping
verbatim: *"a relayed premise inherits no more scrutiny than the claim it supports."* **A rule that
is true of one kind and silently false of another must say which at the point it is stated**, not in
a later paragraph a reader may never reach. Fixed at source in `profiler.rs`. Also theirs, and
sharp: they sorted the gap **from our wire schema** (no such key, therefore genuinely-new) rather
than **from the quantity they needed measured** — and a schema can only tell you whether a *name*
exists, so sorting from it lands in the expensive bucket by construction.

**▶ NEW BAR, 2026-08-24 — `docs/lane-status.json` is the OVERSEER'S file. Never let a dispatched
agent edit it, and say so in the brief.** Earned the same day: the Q-PROF-STRADDLE agent did
excellent work and, closing out, marked its queue item `"state": "done"` — an enum the suite
contract does not define. The Dominion console **rejects the whole document on one bad enum**, so
that single word would have made this lane invisible on the owner's board for the second time in
one night. The agent could not have known: the valid states live in `empyrean/contract/LANE_STATUS.md`,
not in this repo, and nothing it could read locally would have told it. **The fix is structural, not
educational** — a live operational file that the console parses is not part of any work product, and
handing it to an agent puts a contract the agent cannot see in the path of a commit it must make.
Agents report their queue outcome *in their report*; the overseer transcribes it. Related: a
finished item **leaves** the queue — `done` is not a state, it is an absence.

> ⚠ **READ THE PROTOCOL AT A COMMITTED REVISION, NOT THROUGH THE PATH** (seraph's rule, empyrean
> `origin/main` — the most upstream rule in that document): `../empyrean/docs/OVERSEER-PROTOCOL.md`
> is **one peer's live working tree**, so booting by path delivers the suite's shared contract by
> reading somebody's uncommitted directory. Use
> `git -C ../empyrean fetch -q origin && git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md`.
> Correct citation discipline applied to a bad source produces a **more** convincing artifact, not a
> less convincing one. **This session's own boot is the measured case**: it read the file by path
> while empyrean held sixteen unpushed commits, and got the right bytes only because their worktree
> happened to be clean at that minute — 59 lines landed in that path minutes later, and the file
> reached **422 lines by day's end against the 245-line snapshot the session booted on**. Right
> answer, by timing luck, with nothing in the output saying so.
>
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
- **⚑ RED-FIRST IS NECESSARY AND NOT SUFFICIENT — a poison can come back GREEN with the guard
  perfectly sound** *(2026-08-22, aurora; three green poisons in one parcel, none of them a bad
  guard)*. The three classes: (1) the row aimed at a branch **a pre-check makes unreachable**;
  (2) the row proving only *"it refused"*, which **two independent code paths** both satisfy — so
  deleting the guard under test leaves the *other* mechanism holding it green (the matcher clause,
  but the collision is two **paths** producing one observable, not two messages sharing a phrase);
  (3) **the row measuring the WRONG OBSERVABLE** — the fixture left the thing resolvable, so the
  catch site the test was *named after* was never entered. **Planting a violation could not have
  revealed the third; only asking whether it measured the right quantity could.**
  **⚑ THE TEST FOR WHETHER A SPLIT LIKE THIS IS REAL — aurora's, and it generalises past poisons:
  do the two classes have DIFFERENT FIXES?** A matcher collision is repaired by re-pointing at wording
  only that rule uses. Two-paths-one-observable **is not repaired by touching the matcher at all** —
  the matcher can be perfectly precise and the row still worthless; it is repaired by asserting
  **which path ran**. *A bar that cannot tell them apart sends you to the wrong repair*, which is the
  cost of collapsing them. **Their tell for the confusable pair: is the observable UNIQUE to the
  rule?** Unique → the assertion is too loose (matcher). Not unique → the assertion may be exact and
  still prove nothing (two paths).
  **Operational form, to be asked per assertion:** *if this row went green for a reason OTHER than
  the rule holding, what would that reason be?* — then check that specific reason, and report the
  alternative green-path considered and how it was ruled out. **A `None`/absent/empty on either side
  of a comparison must be LOUD, never green**; that is where all three hid, each reading as healthy.
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
- **⚑ A CONFIDENT MECHANISM FROM THIS SEAT IS A HYPOTHESIS, AND THE RECEIVER'S OWN ALREADY-RUN
  COMMAND OUTRANKS IT** *(2026-08-22, found by the sigil lane against themselves; proposed upstream
  to empyrean, which is where it belongs — do not treat this entry as the rule's home)*. I sent a
  peer a confident mechanism for why three stale citations survived (*"they resolve into a different
  real repo and hand you a plausible wrong file"*). It was **wrong** — the leaves 404. My error was
  reusing a real lesson on an instance I had not measured. **Theirs was worse and is the durable
  half: they had ALREADY RUN the refuting command in the same session** — a directory listing and a
  probe that printed `No such file or directory` — **read the output, used it to conclude the cite
  was stale, and then wrote a row asserting my mechanism anyway**, because mine was a better-sounding
  story and arrived with a post-mortem attached.
  **Why this is a bar and not an anecdote: the second failure does not need a peer to be wrong — it
  only needs a peer to supply the frame.** A confident mechanism overwrites a measurement the
  receiver already holds, silently, and nothing in either session looks like a conflict because the
  measurement was never re-read.
  **▶ THE DELEGATION COROLLARY, which is the operative half for this seat and is strictly worse than
  the peer case.** A peer has standing to push back; **an agent has almost none.** Four of my stated
  facts were corrected today by agents who checked them — and every one of those was a *fact*, which
  is checkable. **A stated MECHANISM is far more dangerous than a stated fact**, because it explains
  the evidence rather than competing with it: an agent that measures something inconsistent with my
  mechanism will tend to reconcile the measurement *to* the story instead of reporting the conflict.
  **So: state mechanisms as hypotheses in briefs, explicitly labelled, and say in every dispatch that
  the agent's own command output outranks anything I asserted.** The instance that saved us here is
  the shape to demand — the README agent verified all three targets **individually before writing**,
  rather than performing the search-and-replace my framing implied.
  **Second clause, sigil's, and it prevents the over-correction:** when a correction lands, **check
  which half of the claim actually moved before discarding the whole thing.** The rename shape and
  the code-comment rule both survived my wrong mechanism intact, and retracting them along with it
  would have destroyed two sound rules to fix one bad sentence.
- **Dispatch ahead of a survey only when you can name what the survey could change ABOUT THAT
  PARCEL** — never on the argument that it changes nothing downstream *(2026-08-22; I asked
  empyrean to challenge the step-trio call and they ratified the instance, rejected the
  generalisation)*. The trio was sound: its fragments were final upstream, so the survey could only
  reorder what followed. The generalisation fails because **the survey's most valuable output was
  correcting nine of my own brief-facts, and that value is uncorrelated with whether the fragments
  were final.** Pricing is the stated reason to survey; fact-checking the controller is the one
  that actually pays, and it is exactly the one a "it can only reorder what follows" argument
  discards without noticing.
- **Never record an approval whose granting act you have not seen — cite the ruling, not a status
  field** *(2026-08-22, from empyrean, who found it in their own doc; see the Fable-seat audit in
  The role above, which is this lane's instance)*. Boot docs are snapshots that age while logs
  accumulate, and an owner ruling lands in the middle where head-and-tail reading never sees it —
  so **grep the history for an item before putting it to the owner OR funding work off it.** Both
  directions are failures: re-asking a settled question wastes his time, and acting on a
  self-declared approval spends his money on a decision he never made. The nastiest form is a
  document's description of ITSELF hardening into an owner decision and then into an instruction
  *not to check*, inside the one file every cold session reads and nobody re-reads.
- **Dedicated adversarial review** for load-bearing slices (the slice that carries an arc's central
  claim gets its own reviewer with explicit targets and required explicit negatives).
- **Better-than-the-floor** on every request; improvements additive so the migrating consumer
  loses nothing; the pre-release window for REQUIRED additions shuts at first ship — spend it
  deliberately, once.
- **A gate described for someone else to carry must name its ASSERTION, not its shape.** Earned
  2026-08-23 with aeon, on both sides in one exchange. Our reconciliation identity is a **loss**
  detector, not a correctness proof: a suppressed interrupt bucket *conserves* its cycles into
  `unattributedCycles`, so **the identity closes with that term arbitrarily large** and closure alone
  is satisfied by exactly the case the gate exists to catch — only the explicit `== 0` assertion
  fires. A peer booked the requirement as *"carries the identity check"*, having read the proof, and
  would have shipped a **correctly-described gate whose teeth were gone**: not a wrong gate, not a
  missing one, a gate whose shape a porter inherits with no reason to look under it. Note where this
  bit: inside the very booking written to argue that a mechanism beats a remembered rule. **The
  mechanism only beats the rule if the assertion survives transcription** — so when writing a gate
  into prose for a consumer, name the thing that fails, and re-derive rather than paraphrase when
  carrying someone else's.

**▶ NEW BAR, 2026-08-26 — CHECK THE VINTAGE OF THE PROCESS, NOT THE VERSION OF THE FILE. A long-lived
interpreter is a stale artifact class, and no in-tree check can see it.** Found by this seat while
reaching for an unrelated ⟨RUNTIME⟩ debt; corroborated independently by aurora, sigil, seraph and
dominion within the hour. `oracle-old` `07314aa` (08-25 21:09) made the MCP shim spawn its own private
`oracle-aether` and stop dialling the well-known socket. **Every suite lane's shim process started
08-25 19:53–20:29 — before that commit — and Python reads its source at process start.** So all six
lanes were executing the pre-ruling version, wired straight into `/run/user/1000/oracle.sock`, held by
the OWNER'S live `oracle-frontend` player. Proven by socket-inode pairing for oracle and aeon; aeon had
already `reload_rom`'d his running window onto a worktree build at ~18:20Z in perfect good faith,
**on a banked note that said a fresh session gets a private instance** — a note that was true of the
file and false of the process.
**This is yesterday's *a merged serve is not a served method* bar on a NEW artifact class.** That one
names compiled binaries and compile-time-frozen paths. This is neither: the file on disk is correct,
the fix is merged AND pushed, `git log` looks finished, and the defect exists only in the memory of a
running process. **No sweep, no audit, no cold read of the tree can reach it** — the tree is right.
**⚑ AND THE REMEDY IS NOT THE OBVIOUS ONE: a `/clear` does NOT fix it; only a session relaunch does.**
The shim is spawned by the session process, so clearing the conversation leaves the same interpreter
running. Measured firsthand here, and this is the cheap corroboration worth copying: **this session was
`/clear`ed and its shim's start time did not move** (shim 287372 at 20:29:19, one second after its own
session process at 20:29:18). aurora reached the same conclusion from the *other* direction — that the
shim is on the process command line — which is bar 19's genuine corroboration rather than echo, because
neither derivation could have shared the other's parameter.
**aurora's one-command discriminator, adopted: `pgrep -P <shim-pid>`.** A post-fix shim owns a child
`oracle-aether` on a `/tmp/oracle-mcp-*` mkdtemp socket; a pre-fix one has no child. Both kinds appear
in a single `ss -lxp`/`pgrep` listing, so pre- and post-fix sessions are **visibly different in one
command** with nothing to reason about.
**The failure that nearly happened to three separate lanes' documentation, and it is the durable half:
aurora's own `OVERSEER.md` asserted the opposite** — *"`mcp__oracle__*` in this session SPAWNS A PRIVATE
EMULATOR by default — it is NOT the window the owner is watching"* — written that same day, correctly,
**from the file on disk**, and false for every interpreter older than 21:09. They fixed it at
`83fcb64`. **A claim about RUNNING STATE banked as though it were a property of the code** is the
perishability preamble's sharpest instance yet: the anchor was valid, the source was authoritative, and
the sentence was still wrong the moment it was written.
**Operational form: before trusting any tool that dials something, ask when its PROCESS started
relative to the fix you are relying on** — and write the vintage condition into the note, never the
conclusion alone.


**▶ NEW BAR, 2026-08-27 — THE OPS LINE THAT IS NOT IN THE DISPATCH IS NOT IN THE DISPATCH. Carry the
worktree `vendor` symlink into every brief that will run cargo.** The Ops section below has said *"fresh
worktrees: `ln -s <repo>/vendor vendor`"* for weeks. It was **still missed on a dispatch this morning**,
because the brief is composed from the invariant block and the parcel's own grounding — and an Ops line
sitting in this file is not either of those. The agent lost time on a baseline that would not reproduce:
eight `save_state::tests::*` rows **panic** (not skip) on the missing vendored ROM, and the resulting
`exit 101` is indistinguishable at the aggregate line from two other causes this repo has recorded.
**The fix is structural, not educational** — an overseer who has read this file every session still omitted
it, so the rule is that the vendor line is part of the *brief template* for any cargo-running dispatch,
alongside the base check. Related and already booked: the same class as *a merged serve is not a served
method* — knowing a thing in the tree is not the thing reaching the process that needs it.


**▶ NEW OPS LINE, 2026-08-30 — NEVER CITE THE TIP. CITE THE COMMIT THAT CARRIES THE ARTIFACT, EMITTED
FROM THE PATH.** Third instance of the anchor-class family against this seat, caught by the hub.

I sent the hub `d5baac7` as CR-H's anchor. **It is a `docs/lane-status.json`-only commit.** The CR is
carried by **`d907fae`**. Both verified with `--stat` after the fact; the hub caught it before I did.

**The mechanism, and it is new — the two previous instances do not describe it.** 08-24 was the *wrong*
SHA (a lint fixup for a feature merge) and 08-27 was the *right* SHA for an unstated question. **This is
neither: I cited the SHA I had just pushed.** I committed the CR, then committed a status update on top,
then pushed, then quoted the push's result — so the tip was the status commit, and the *act of being
diligent about pushing before citing* is what put the wrong object in my hand. The push-before-you-cite
rule and the cite-the-carrying-commit rule pull in opposite directions at exactly this moment, and
nothing warns you.

**The corrective is constructive, not verifying**, because `--stat`-after-the-fact is what the existing
bar already prescribes and it did not fire — I had no reason to doubt a hash I had watched go out:

```sh
git log -1 --format=%h -- docs/proposed/2026-08-30-cr-h-screen-text.md   # the commit that carries it
```

**Never type or paste the output of `git push` / `rev-parse HEAD` as an anchor for an artifact.** Ask the
path which commit carries it. That form cannot produce this error, where checking can only catch it.

⚠ **And the reason it was cheap: the bad anchor lived ONLY IN MAIL.** `grep -rn d5baac7 docs/` returns
nothing — no in-tree reader could ever have met the contradiction, exactly as protocol bar 20 describes,
and the recipient was the only party able to catch it. **They did, which is the argument for citing a SHA
the receiver can actually resolve rather than one that merely exists.**


**▶ NEW OPS LINE, 2026-08-30 — A KILLED SUITE LEAVES A LOG THAT AGGREGATES CLEAN. COUNT THE LEGS, NOT THE
FAILURES.** Nearly quoted as a merge verdict by this seat.

A merged-tree verification was **killed at 46 of 61 legs** — cause unidentified (no rotation was due, no
peer announced a `pkill`, and this box has a recorded history of both). The log it left behind is the
hazard: `grep -E "^test result" | awk` over it prints a **confident `LEGS=46 … FAILED=0`**, which is
exactly the shape of a healthy result and is three quarters of a suite. **Nothing in the aggregate line
says how many legs there should have been**, so the one number that would reveal the truncation is the
one the house aggregate does not carry.

**This is the green-log-and-absent-run bar arriving on a PARTIAL run rather than an absent one**, and it
is worse than the absent case: an absent run leaves nothing to misread, while this leaves a page of
genuine passes. Every one of those 46 legs really did pass. The log is not lying; it is answering a
narrower question than the one being asked of it.

**Corrective, and it is cheap because it is one more line in the same command:** a verification asserts
its own **completeness** before its verdict —

```sh
LEGS=$(grep -cE '^test result' "$LOG")
[ "$LEGS" -lt 61 ] && echo "INCOMPLETE: $LEGS legs, expected 61 — NOT A VERDICT"
```

and the runner prints an explicit `CARGO_EXIT=` and `COMPLETE` marker, so **the absence of the marker is
itself readable**. Never take an aggregate from a log whose run you did not watch terminate.
⚠ Compounding factor recorded because it hid the first instance tonight: an earlier verification of mine
ended in `grep -c 'SKIP:'`, which **exits 1 when it finds zero matches** — the desired outcome — so the
harness reported the whole command as failed while the suite was genuinely green. **One command reported
failure on success, and hours later another reported success on a partial run.** A pipeline's exit status
and its subject's verdict are different facts; make the command say which one it is reporting.


**▶ CORRECTION, 2026-08-30 — OUR HEADLESS RECIPE'S "BOTH GUARDS" ARE ONE GUARD TWICE, AND IT IS THE
GUARD A PEER JUST MEASURED AS INEFFECTIVE.** Prompted by aurora's O36 finding (relayed by the hub);
the defect below is ours and was found by reading our own source, not theirs.

The banked recipe reads `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:N … --x11`, and its prose
calls these **"both guards, because the failure is silent and lands on somebody else's screen"**.
**They are not two guards.** `--x11` is `force_x11`, and `main.rs:1040-1043` implements it as exactly
`std::env::remove_var("WAYLAND_DISPLAY")`. So the flag and the `env -u` do **the same single thing**,
and the recipe has no independent second guard at all — it reads as belt-and-braces and is one belt.

⚠ **And aurora measured that mechanism failing.** On this box `WAYLAND_DISPLAY=wayland-0`, its socket
exists under `XDG_RUNTIME_DIR`, and an Electron app started with the variable deleted **still reached
the owner's two real monitors** — the toolkit fell back to the literal `wayland-0` path rather than to
X11. **If minifb does the same, `--x11` does not do what its own `--help` says it does**, and every
windowed fixture this lane has run may have been on his desktop.

**NOT yet established for us, and the distinction matters:** aurora's measurement is **Electron/Ozone**,
a different toolkit from our **minifb** (we are not winit-class, which is what their message assumed —
`Cargo.toml:10`). Their result does not transfer by itself; minifb's Wayland backend is separate code
and may genuinely fail the connect when the variable is gone. **So this is a live hypothesis about our
player, not a finding about it.**

**The discriminator, aurora's and adopted — it needs no window on his screen:** ask the app for its
screen size from inside the fixture and compare against the Xvfb geometry actually requested. A match
proves the fixture owns the display; a mismatch proves it is talking to the real compositor. **Do NOT
measure the windowed case while the owner is logged in.**

**Consequence for the `screen_text` parcel, and it is the reason this matters today:** its T1 runtime
item is precisely *"never executed against a real window"*. **That item may not be dischargeable by the
recipe in this file**, and discharging it against an unvalidated recipe would put a window on his
desktop to prove a feature about windows. Settle the discriminator first, or leave T1 open and say so.


**▶ NEW OPS LINE, 2026-08-30 — DO NOT COMMIT WHILE A VERIFICATION RUN IS IN FLIGHT. IT INVALIDATES THE
BUILD-ID GATE AND THE FAILURE LOOKS LIKE THE PARCEL'S.** Third instrumentation failure in one night, and
the only one that produced a red that was entirely mine.

A merged-tree run came back **61/1988/1**, failing `server_build::the_compiled_in_build_id_still_names_this_tree`
— *"the compiled-in build id names a different commit than HEAD"*. **It was right.** The binary was
compiled at `8e88e2b`; I then landed two `docs/OVERSEER.md`-only commits **while the suite was running**,
so HEAD had moved by the time the assertion read it. Re-run on a stable tree: **61/1989/0/6, exit 0,
`HEAD_AT_START == HEAD_AT_END`.**

**Why this is worth a line rather than a shrug: the gate exists to catch a stale build-script product,
and a docs commit is exactly the change a person feels safe making during a long run** — it touches no
code, it cannot affect a test, and it is the natural way to use twelve idle minutes. The failure it
produces names a *build-script caching defect*, which is a plausible and completely wrong diagnosis; a
session that trusted the message would have gone looking at `cargo:rerun-if-changed` declarations.

**Corrective:** a verification prints `HEAD_AT_START` and `HEAD_AT_END` and **they must be equal for the
verdict to count**. Bank findings *after* the run, never during — the twelve minutes are not free time,
they are part of the measurement.

*Tonight's three instrumentation failures, kept together because the pattern is the point: one command
reported **failure on success** (`grep -c` exiting 1 on the desired zero matches); one reported **success
on a partial run** (a killed suite aggregating clean at 46 of 61 legs); one reported **a real failure
with a misleading cause** (this). In all three the suite itself was honest and the harness around it was
not. **The instrument that reports on the instrument is the one nobody tests.***


**▶ SCOPE CORRECTION, 2026-08-30 — `screen_text` DOES NOT END AEON'S EYEBALL REQUESTS, AND THIS FILE SAID
IT WOULD.** Corrected by aeon against a claim this seat made to them, which was taken verbatim from our
own queue row.

The OVERLAY-STATE row read *"the item that would stop aeon asking you to eyeball things"*, and I repeated
it to them as a headline. **It is wrong. `screen_text` reads the emulator's own CHROME — status line,
toasts, palette, lens, title bar — and every eyeball request aeon has outstanding is GAME PIXELS**: the
right-edge price of a column borrow, a background wrap, colour bands. **No `kind` in the adopted enum can
see any of them.** Their words, and the reason they sent it: they would otherwise have planned a parcel
around a capability the tool does not have.

**Where it genuinely helps, per aeon: the booked SCENE-READOUT row** — the owner counts button presses to
know which of twenty effects is on screen. **If that readout is drawn as debugger chrome, `screen_text`
reads it and the row is cheap; if it is drawn as game graphics, the tool cannot see it. Which one is
UNMEASURED**, and aeon has written it into their booking as a thing to check *before* planning around it
rather than assuming the convenient half.

**The durable shape, and it is why this is an ops line rather than a typo fix: the over-claim was in a
QUEUE TITLE, which is the one place nobody re-derives.** It was written when the item was a sketch,
inherited by every status file since, and finally exported across the fence with a seat's confidence
attached — where the only reader who could refute it happened to be the party it was about. **A queue
row's justification ages exactly like a precedent narrative and nothing re-reads it.** When an item's
design lands, re-read its own queue title against what was actually built.

**▶ F-ACCEPT-TABLE-CROSSCHECK-BLIND (registered 2026-08-30, emitter behaviour change, NEEDS A RULING).**
`tools/legacy_accept_table.py`'s axis-A/axis-B reconciliation adds to `claimed_lines` **before the row is
written**, so it is **structurally blind to a row-level drop**. Measured firsthand: with the four unguarded
`addr` rows dropped cleanly, `--fail-on-gap` prints `cross-check : AGREES`, `parse complete : yes` and
**exits 0**, while `UNGUARDED reads` silently falls **43 → 39**. ⚑ **So the tool's own headline safety line
is not evidence of the thing a reader takes it for** — it witnesses that every *access* was claimed, never
that every *row* survived. The 57-test suite catches this by named assertion; `--fail-on-gap` does not, and
`--fail-on-gap` is what a CONSUMER would wire into a gate. **Revival: aeon wiring the table into their gate
— they must be told the gate flag is weaker than the suite** (told 2026-08-30). Fix would be `--fail-on-gap`
independently verifying row presence against a source-derived expectation; that is an emitter behaviour
change and was correctly kept out of the hardening parcel.

⚑ **AND THE LESSON ABOUT THE VERIFIER, WHICH IS THIS SEAT'S: A POISON THAT CRASHES IS THE EASY VERSION.**
My dropped-row poison returned `None` from `_record`, which crashed `hazard_views` and produced
`setUpClass` errors — I read "exit 1" as caught. The **non-crashing** form of the *same* failure
(`continue` before the row is written) went through the pre-hardening suite with **one bare `KeyError`
against a synthetic fixture and nothing against real source**. Both reproduced firsthand at the merge.
**A poison must be built to survive its own blast** — if it takes the program down, you have measured that
the program crashes, not that the suite noticed. The agent built the harder variant unprompted after being
told the easy one, which is the deviation-with-evidence this file's bars ask for.

## Ops (each line is a paid-for lesson)

**▶ `lane-status.json` — THE BOOT CURL VALIDATES THE FILE YOU WROTE AT BOOT AND NOTHING AFTER IT** (2026-08-30,
this seat, measured). I wrote `"state": "done"` on a landed row after a merge. **`done` is not in the
vocabulary** (`doing | next | open | blocked`; a landed row LEAVES the queue and its landing goes to
`lane-log.jsonl`). **One bad enum in one row rejects the WHOLE file**, so the owner's card for this lane went
dark — with every true thing in it — and stayed dark for about an hour. **Nothing about it is visible from
this side:** the file writes fine, `git` is happy, and the lane goes on reporting accurately to itself.
**The defect was not the word, it was that I ran the verification curl ONCE, at boot.** Every later write is
unverified unless re-checked, and I made a dozen. **Re-run the boot step's curl after ANY write to
`lane-status.json`** — it is two seconds and it is the only thing that can tell you.
⚑ **THIS RULE IS NOW CONTRACT AND EMPYREAN GOVERNS IT** — `contract/LANE_STATUS.md` §*"Verify after EVERY
write, not only at boot"*, at empyrean `origin/main` (verified here by content at `97c4f72`; the hub cited it
as *"commit after 1489413"*, which is a coordinate rather than a SHA, so it was resolved by reading the
section, not by trusting the pointer). **The text below is this repo's PRECEDENT NARRATIVE, not a second
copy of the rule** — on any disagreement the contract wins, and the rule is not to be restated here as it
drifts. Read it at a committed revision, never through `../empyrean/`.
⚑ **n=2, AND THE SECOND INSTANCE READS SHARPER THAN A REPEAT** (aurora, verified here: contract line at
empyrean `origin/main`, carrying commit `10c87ba` — a real contract+docs commit, `--stat`-checked, and this
time the hub emitted the SHA from git rather than naming a neighbouring coordinate). **sigil wrote `closed`
the same night, an hour apart, neither lane aware of the other, both having read the warning shortly
before.** ⚑ **The part worth more than the count: we reached for TWO DIFFERENT WORDS.** That is not two
lanes making the same slip — it is two lanes independently reaching for a terminal state **the vocabulary
does not have**, and picking different plausible names for it. **The error is INVITED by the design, not
merely permitted by it**: the natural word for a finished row does not exist, because the contract's answer
is that a finished row *leaves the queue* — correct, and not what a writer's hand reaches for. A rule
against `done` would not have caught `closed`, and a rule against both would not catch `complete`.
⚑ **The skill's own boot text warned about exactly this** (*"three lanes wrote `done`, which is not in the
vocabulary, in three days"*) and I did it anyway, which is the argument for the mechanism over the warning:
a rule you have read does not fire, a curl does. Found by the aurora lane reading the console, not by me —
**this lane cannot detect its own invisibility, so it depends on a peer looking.** Worth knowing when no
peers are up: the verdict is only ever one curl away, and nothing else will surface it.



**▶ NEW BAR, 2026-08-29 — VALIDATE AN ARTIFACT AGAINST THE SCHEMA IT TARGETS BEFORE CALLING IT READY.
A "ready to merge" IS a completeness claim, and it is the one nobody thinks to check because it reads
as a status rather than an assertion.** Earned against this seat, on the same submission where it was
lecturing about unchecked residues.

**The instance.** This lane authored 11 contract vectors for CR-F, verified *programmatically* that
every case cited its clause, wrote a README handing them over as ready, and shipped them. **Nine of the
eleven could not have passed**: every result case carried `"layout": {}` and `$defs.decoderLayout`
requires five fields. The applying lane filled them in. One command — running the cases against the
schema — would have caught it, and **the schema was readable at a committed revision the whole time and
this lane had already read other parts of it.**

**Why it is a bar and not a slip: the same document argued that an unrecorded residue "reads as guarded
to everyone who sees a green schema run", while containing vectors that could not have produced a green
run at all.** The rigour was spent entirely on the interesting half (which rules are testable, which are
behavioural and therefore not) and none on the mechanical half. That is the failure mode of a lane that
has been reasoning well for hours: **scrutiny follows what looks like an argument, and a field you filled
in as a placeholder does not look like one** — the provenance-feeling-material bar arriving on your own
output instead of on a citation.

**Corrective, and it is mechanical because vigilance already failed:** an artifact authored against a
schema is **run against that schema before it is handed over**, and the run's output is what the handover
cites — never "these conform". If the validator cannot be run from here, say so in the handover and name
what was not checked, rather than letting a confident cover note stand in for it.

**Second defect, same submission, different class: a change introduced between two of your OWN artifacts
needs a named delta.** CR-F as filed said `owner.raw` is served *always*; the fragment written three
hours later makes it absent when `kind == "unavailable"`. That is the better rule and it was adopted —
but it changed a filed artifact silently in a second one, and the receiving lane had to catch it and ask
for it to be noted. **An improvement introduced without a flag is indistinguishable from an
inconsistency**, and the cost lands on the reader who has to work out which it was. Name the delta at
submission time; it costs a sentence.

**▶ NEW BAR, 2026-08-29 — A TEST THAT ASSERTS WHAT YOU *ADDED* IS STRUCTURALLY BLIND TO WHAT YOU
*DISPLACED*, AND A FIXED-SIZE SURFACE MAKES EVERY ADDITION A DISPLACEMENT.** Earned on the
SCREEN-HONESTY parcel, against this seat's own green test.

**The instance.** Adding an `AETHER ON/OFF` field to the F3 status line came with a test asserting the
field survives truncation. It passed. It was also true and useless: `fit` cuts the line from the right
with **no ellipsis and no error**, so the test proved the new field was present while three older ones
— aspect, native resolution, frame counter — had been pushed off the glass. Measured rather than
reasoned about, by printing the fitted string at each real window size: **34 characters available at
224px, 448px, 672px and 896px alike**, because the font scale tracks the picture height, so the budget
in *characters* is very nearly constant no matter how large the window. The line wanted 51.

**The part that makes it a bar rather than a slip: the surface was ALREADY over budget and nobody
knew.** Before this parcel the line ran to 41 characters, so `F1234` had never been visible at any
size and the resolution was being cut mid-number to `320X2`. A field that silently truncates cannot
report its own overflow, and no test asserted on the *whole* line — every test asserted on the field
its author had just added. That is bar 16(d)'s absence surface on a **positive** artifact: a status
line that is present, legible, and quietly missing its right-hand third.

**Correctives, in the order they are cheap.** (1) When adding to a fixed-width surface, assert on the
**whole** rendered string, not on the field you added — `assert_eq!(rendered, full)` is the form, and
it fails for the person who adds the *next* field too. (2) **Print it and look at it** before believing
a boolean; the arithmetic here was mine and was wrong twice before the probe settled it. (3) Ordering
is a design decision on any surface that truncates: the fields that answer *"is this window lying to
me"* go first, and that ordering wants its own test with an **anti-vacuity clause** — there must exist
a width that drops a late field while keeping an early one, or the ordering test passes on a line that
never truncates at all.

**And the fix that bought the room back is worth stealing: the status line now draws one font step
smaller than the rest of the overlay** (`Overlay::status_font_scale`). A toast is a *message* and wants
reading across the room; a status line is a **readout** consulted deliberately by someone who pressed
F3. One step down roughly halves its width cost, and the whole 51-character line now fits at every size
from 448px up — including the frame counter, which had never been visible. At 224px there is no step
left to drop and it still truncates; that floor is **asserted in the test** rather than papered over, so
a change that makes it worse fails instead of passing quietly.

**Second finding from the same parcel, on the silent arm.** The bug being fixed was that `Bus::start`
printed nothing when no `--aether` was given — an absence, which is why it was never noticed. **No test
over that file could have caught its removal either**: a unit test cannot read `println!`, and a test
that shelled out to grep its own stdout would be testing the harness. So the guard is structural — the
`if let Some(..) else` became a `match`, and **deleting the quiet arm is now a compile error.** Where
the observable is unreachable to a test, reach for the type system before settling for a comment. The
wording is pinned separately as a constant, and the two builds' twins pin a shared opening so
`bus.rs` and `bus_stub.rs` cannot start describing one state in two vocabularies.

**Runtime is what closed the wiring, and it was not optional.** Poisoning `filter: filter_label(..)`
back to the core identifier left the entire suite green — the tests call the function, and the call
site sits inside `setup_audio`, which needs a real output device. That half was settled by running the
player under `xvfb-run` with `XDG_CONFIG_HOME` pointed at a scratch `player.conf` carrying
`status_line = on` (which is how you get the F3 line up with no keyboard, and without touching the
owner's config), then reading the screenshot. All three readings confirmed on the glass: `AETHER OFF`,
`AETHER ON` under `--socket`, and `AUDIO RAW` under `ORACLE_CONSOLE_FILTER=off`. **The test's own doc
now says which half it does not cover**, rather than leaving a later reader to assume it covers both.

**▶ NEW BAR, 2026-08-27 — A CLAIM IN OUR DOCS ABOUT A PEER'S FILE HAS A SHELF LIFE, AND NOTHING IN THIS
REPO CAN EVER TELL YOU IT HAS EXPIRED. MEASURED SHELF LIFE: FORTY MINUTES.** Third instance in one day,
which is what makes it a bar rather than three slips.

**The instance, dated end to end because the dating is the argument.** `docs/2026-08-27-breakpoints.md`
§7b landed at **03:11:37** stating that aeon's `evict_witness.py:97` reads snake_case
`timeout_reached` and would silently misreport a timeout. **It was TRUE.** aeon fixed it at
**03:51:24** (`6e4751c3`, verified here as a code commit on their `origin/master`). The claim was then
**re-cited three times inside this repo** (`bp-disclose-recon.md` ×3, `lane-log.jsonl`) and **exported
to aeon at ~13:00 as a live exposure**, where they *"had the brief half-written"* for an agent before
opening the file by luck. **Not one link in that chain re-read the source.** Every re-citation was a
faithful copy of a sentence that had been true.

**The other two instances the same day**: the ROM-disappearance *mechanism* asserted about their build
(invented, not observed), and the 52-method figure that went out from here, was banked there, and came
back to outrank our own measurement. **Different shapes, one property: a statement about a peer's tree
living in our tree, where no reader of ours can meet the contradiction.**

**Why the existing bars do not cover it.** *Verify firsthand* is satisfied — the author did read the
file. *Check the SHA class* is satisfied. *Re-read at send time* (protocol bar 22) is the closest and
is written for **peer status files**, which announce their own staleness with a timestamp; **our own
committed prose does not**, and reads as settled fact precisely because it is in our tree and we wrote
it. **A doc has no `updatedAt`.**

**The remedy, and it is cheap because it is the protocol's verified-at anchor pointed at our own
docs:** when a doc here asserts something about a sibling repo's file, **record the peer revision it
was read at, inline** — `(aeon `6e4751c3`, read 2026-08-27)`. That converts an unfalsifiable sentence
into a one-command currency check for the next reader, which is exactly what the three instances above
each lacked. **And before exporting any such claim across the fence, re-read the file at their tip** —
not the doc that quotes it.

**⚑ The sharpest form, from aeon's side of this one: a peer's warning about YOUR OWN tree is the class
you must verify before acting on, and it is the one that feels least like it needs checking** — it
arrives as help, about your own code, from someone with no motive to be wrong. They nearly briefed an
agent on our stale premise. **Our confident claim about their tree almost became their agent's
instruction**, which is the delegation corollary reaching one repo further than it was written for.


**▶ NEW BAR, 2026-08-27 — DO NOT GREP A RELEASE BINARY FOR A SHORT STRING. THE OPTIMIZER INLINES IT AS
AN IMMEDIATE AND IT IS SIMPLY NOT THERE AS A CONTIGUOUS SEQUENCE.** Earned by nearly reporting a
stale-binary emergency to a peer who was about to make a decision on it.

Chasing whether `target/release/oracle-aether` was current, I grepped it for `timeoutReached`,
`oracle-rs` and `serverBuild`. **All absent.** I re-ran it four ways — `grep -a`, `LC_ALL=C grep -a`,
`strings | grep`, `strings -a -n 4 | grep` — plus a raw byte `bytes.find()` in Python that removes both
tools from the question. **All five agreed on absence, with a negative control returning zero and
positive controls returning hits.** Every check this file demands, and the conclusion was still wrong.

**aeon settled it by spawning the binary and reading the wire: all three strings are served.** Then the
mechanism, measured rather than guessed — the prediction was that an 8-byte immediate leaves the first
eight bytes contiguous and orphans the rest:

| literal | len | release, whole | release, 8-byte prefix | debug, whole |
|---|---|---|---|---|
| `oracle-rs` | 9 | **0** | 1 | 1 |
| `oracle-next` | 11 | **0** | 1 | 1 |
| `serverBuild` | 11 | **0** | 1 | 1 |
| `timeoutReached` | 14 | **0** | 2 | 1 |
| `profile=release` | 15 | 1 | 1 | 0 |

**Short literals are materialized as 8-byte `mov` immediates in the optimized build; only the first
eight bytes survive as a searchable run.** The 15-byte control stays whole, and **every debug build on
this machine carries all of them** — which is the discriminator to reach for.

**Why this is a bar and not a curiosity: it fails in the FALSE-ALARM direction, and it attacks this
lane's own cheap shortcut.** Our 2026-08-26 bar — *a merged serve is not a served method; spawn the
consumer's own path and call it* — is correct, and grepping the shipped binary is the tempting cheap
substitute for it. **That substitute manufactures confident evidence of staleness in a binary that is
perfectly current.** It is bar 16(d)'s absence surface with the sharpest possible teeth: an absence
reproduced by five instruments still is not evidence, because all five shared one frame — *that a
string literal in the source exists as that string in the artifact.* **Nobody records that as a choice,
so re-running cannot vary it** (bar 19 exactly).

**Operational form:** to ask whether a binary contains a symbol or wire key, (1) **spawn it and call
it** — the bar already says this and it is the only answer that cannot be fooled; (2) failing that,
grep the **debug** build, or the **8-byte prefix**; (3) never read a short-string absence in a release
binary as staleness. And if a static read contradicts a live measurement, **the live one wins** —
which is our own *the receiver's already-run command outranks a confident mechanism from this seat*,
arriving with the seat on the losing side.

**▶ AND THE ONE THAT COST MORE — A NUMBER OF OURS CAME BACK AS A PEER'S AND OUTRANKED OUR OWN
MEASUREMENT. FIRSTHAND VERIFICATION DID NOT PROTECT US; IT IS WHAT LAUNDERED IT.** Found by aeon
against this seat, same hour, and it is the reason the absence above went unbelieved for as long as it
did.

The chain: **this lane** measured *52 methods advertised, `capabilities.breakpoints: true`* and told
aeon. aeon banked it in **their** `docs/OVERSEER.md` rung-2 note. I then **read that number firsthand
out of their committed blob** and used it as *independent corroboration* to discount my own static
finding — reasoning that a binary missing its handshake identity could not report 52. **The
corroboration was our own claim wearing another lane's confidence.** Bar 19's echo-versus-corroboration
at its purest: there were never two derivations, there was one, and it had gone round a circuit.

⚠ **And the detail that makes it worse rather than better, verified at their `origin/master`:** the
sentence beside it — the *41*-method reading — is explicitly labelled *"Measured, both binaries as
shipped"*, i.e. **theirs**. The 52 carries **no attribution at all** (*"the same server reports 52
methods…"*). So the number I trusted sat in a peer's tree, in a paragraph whose neighbouring figure
announced its own provenance, silently missing its own. aeon's own account was that they had attributed
it in their docs and dropped it only in the message; **at the revision I read, the durable record does
not carry it either** — offered to them hedged, because it changes the remedy.

**Which is the durable half: `verify firsthand` does not reach this.** Reading a committed blob
confirms the **transcription**, never the **claim** — and this suite's primary defense is firsthand
verification, so the failure rides in on the exact discipline meant to stop it. **Provenance does not
survive transcription into another lane's document**, and the reader cannot recover it, because a
faithfully-copied number looks identical to an independently-measured one. So the remedy is not only
*say whose measurement it is when you repeat it in a message* — it is **the attribution must survive
into the durable record**, since that is what peers read firsthand and trust.
**Before letting any peer's figure outrank your own measurement, ask where it came from originally.**

*⚑ Keep the sting, because the tidy lesson is wrong: **the laundered number was pointing at the TRUTH
and this seat's correctly-executed measurement was pointing at a FALSEHOOD.** The binary really was
current. Had I discounted the circular 52 and trusted my own five-instrument absence, I would have
raised a false staleness alarm at a peer mid-decision. Two defective inputs happened to cancel. So
resist *"trust your own measurement over a peer's number"* — the actual rule is that **a number with no
provenance and an absence with no live control are both unfinished**, and the fix for each is the same:
go and look at the running thing.*


**▶ THE COMMIT-MESSAGE BAR NOW HAS TWO INSTANCES, AND BOTH FAILED BY THE SAME MECHANISM: A LINE WRAP.**
Protocol bar 23 (*a commit message is a claim about a diff, and nothing checks it*) came out of this
lane on 2026-08-23, from a scripted edit that **silently failed on a line wrap** while the shell let the
commit run anyway. On 2026-08-27 aeon produced the second instance in their own tree, hours after
banking the bar — a message asserting two changes, one of which *"matched nothing, because the sentence
wraps across two lines and my pattern assumed one"* (their `c136fc3c`, corrected at `95c39449`, both
verified here as reachable ancestors of their `origin/master`; they corrected the **record**, not the
history, since the first was public).

**Different repo, different operator, different tooling, SAME mechanism** — which by bar 19's test is
corroboration rather than echo, because neither derivation could have shared the other's parameter. So
the durable statement is sharper than the bar's own: **the dominant failure mode of a scripted edit in
these repos is prose that WRAPS defeating a pattern that assumed one line.** Every lane here is
docs-heavy and every doc wraps, so this is the common case, not the corner.

**Two corrections to how the bar is stated, both from their instance:**
1. **`;` is a rung BELOW the `&&` the bar already calls insufficient.** Bar 23 warns that `edit && commit`
   does not protect you, since a replace matching nothing still exits zero. Theirs was weaker still —
   the commit *"sat after the failed edit in the same block rather than behind it"*, so the exit status
   was **never consulted at all.** When a block does both, the commit must be `&&`-behind the edit *and*
   behind a verification, because `&&` alone is known-insufficient.
2. **Match on a short fragment that CANNOT wrap, or read the blob back.** A multi-word prose pattern is a
   bet that the author's wrapping matches yours. Prefer a distinctive short token, and then
   `git show <sha>:<path> | grep -c` the committed blob before writing the message — the assertion in the
   message and the check that earns it are separable, and the message is the cheap one.

*Their framing, kept because it is the honest half: they banked the bar and produced its textbook
instance twenty minutes later. **Rehearsal is not protection** — which this suite's protocol already says
about SHAs, arriving here in a different field.*

**▶ STATUS: PROPOSED, ACCEPTED, QUEUED — DO NOT RE-PROPOSE IT.** The hub ledgered all three sharpenings
as **Q-23** in `empyrean:docs/OVERSEER.md`'s pending protocol queue, verified here firsthand on the
pushed blob at empyrean `e27362c` (which is `origin/main` itself; `grep -c '^Q-23\.'` = 1, and the entry
carries this lane's bar-21 self-discount as stated rather than dropping it). Per the owner's batching
rule it lands **inside bar 23's text as an amendment, not as a new bar**, in the next batched protocol
pass. **Nothing is owed by this lane.** The paragraph above stays as lane-local ops guidance and is
correct whether or not the protocol pass ever runs — but a session that reads *"proposed to empyrean"*
and re-sends it is spending a peer's attention on a closed item, which is the notify-on-the-dependency
bar failing from the other end.


**▶ NEW BAR, 2026-08-29 — THE HEADLESS PLAYER RECIPE IN THIS FILE WAS INCOMPLETE, AND FOLLOWING IT PUTS A
WINDOW ON THE OWNER'S DESKTOP.** Measured firsthand while discharging the two window checks; it happened on
the first launch. The banked recipe (from the SCREEN-HONESTY parcel, above) is *"the player under `xvfb-run`
with `XDG_CONFIG_HOME` pointed at a scratch `player.conf`"*. **`minifb` prefers Wayland when
`WAYLAND_DISPLAY` is set, and every lane session on this box inherits `WAYLAND_DISPLAY=wayland-0`** — so
`DISPLAY=:91` was honoured by nothing, the log said `Wayland window`, and `python-xlib` found **zero windows
on the Xvfb**. The window was on his real screen. Killed inside the minute by recorded PID.
**Corrected recipe — both guards, because the failure is silent and lands on somebody else's screen:**
`env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:N target/release/oracle-frontend --x11 …`, with your
own `Xvfb :N` whose PID you recorded. **Then verify placement before driving anything**: enumerate windows
on the display you believe you own, and treat an empty list as the finding rather than as a slow start.
**Why the note was wrong in a way nothing could surface:** it was correct *for its author's purpose* — they
were reading a screenshot, and any window would do. It is the **isolation** claim that was never true, and
the note never said which of the two it was promising. Same class as *check the vintage of the process*: a
sentence true of the file and false of the situation. *(`xdotool` is absent on this machine; `python-xlib`
0.33 is present and gives XTEST, which is what drove the keystrokes. `import` from ImageMagick grabs the
screen; `scrot`/`xwd` are absent.)*

`cd` to the absolute repo path before ANY branch operation (a persisted cwd nearly checked out
under a live agent). Fresh worktrees: `ln -s <repo>/vendor vendor`, verify 17 TestRoms entries, and
open every dispatch with a base check (commit-message string + a file that must exist). Exact-path
`git add` only; `git show --stat` per commit; no Co-Authored-By trailers. Never `cargo test | tail`.
`pkill -f`/`pgrep -f` self-match (the waiting shell's own command line contains the pattern) — bracket the first character: `pgrep -f "[c]argo test"`. ⚠ **Bracketing is not enough when the SAME command carries the literal string elsewhere** — a heredoc writing a doc that quotes the socket path made `pkill -f "[o]rc-p/o.sock"` match its own shell and kill it mid-command (exit 144, 2026-08-26). Kill by the PID you recorded at launch, not by pattern, whenever the command also contains the text. Aether sockets live under `$XDG_RUNTIME_DIR`. `/tmp` is quota'd —
free space is not the signal. The frontend is bin-only (`pub fn` with no caller = hard error).
`ls` is aliased to eza. Owner tests run `aeon/s4.debug.bin`.
A probe socket must NOT live under the session scratchpad — that path exceeds `SUN_LEN` and the
server refuses with `cannot bind the Aether socket: path must be shorter than SUN_LEN`; use a short
`/tmp/<short>` dir. The MCP shim (`oracle-old/linux-port/mcp/oracle_mcp.py`) **SPAWNs its own
`oracle-aether` by default** (private `mkdtemp` socket, `ORACLE_ROM` default `aeon/s4.debug.bin`) and
**ATTACHes only when `$ORACLE_SOCKET`/`$EXODUS_SOCKET` is set** — it does NOT use empyrean's
`resolve_socket_path()`, so do not reason about the shim from that resolver (this seat did, and was
wrong about whose emulator it was talking to).

## Coordination (when peers are up; all optional to progress)

- **seraph** (DAW): **the first FILED DEMAND against the unserved-method list, shaped and dated
  2026-08-26 — and it is the reason the cutover ruling works.** Their S2 verification gate
  (`plans/2026-07-03-s2-verification-gate.md:10-16`; grounds anchor **`a02c77a5`**, which supersedes the
  `9b2c5a77` first cited — both verified here as reachable ancestors of their `origin/main`; the S2
  conclusion hardened rather than moved, so nothing banked off the earlier one needs changing) builds
  **side B entirely out of `emulator_vgm_start`/`stop` → `vgm2wav`**, so **S2 as banked is NOT
  executable against the new core** — VGM capture is not one instrument among several there, it is the
  whole of side B. Their triage, taken as given: **`vgm_{start,status,stop}` is the one that matters**
  (realtime and foreground is fine; what it must be is deterministic enough to capture twice and
  compare); **`audio_spectrum` is explicitly NOT wanted** — their compare is scripted seraph-side
  against the rendered WAV, so do not build it on their account; **channel masks are wanted at S3, not
  S2**, and they declined to charge us the synth-into-the-bus-server cost yet. **Firing condition: S1
  landing**, when a compiled blob exists to capture. They deliberately did NOT file it as a dated queue
  item today, citing bar 18 — the dependency is two packages away and a board entry for a consumer that
  does not exist yet is a cost with no reader. **Treat VGM as demand-ordered-with-a-condition rather
  than unqueued**: when the acceptance list is next triaged, VGM is the only one of the eighteen with a
  named consumer, a named artifact, and a stated trigger. Do not pre-build it; do not renumber it away.
  ⚑ **CONFIDENCE SPLIT, corrected by seraph against this seat's own over-hedge and verified here at
  `main`: the THAT is machine-enforced; only the WHY is a reading.** That these six are unserved is not a
  booked opinion — `schema_conformance.rs:403` pins `SCHEMATIZED_NOT_ADVERTISED` at **18 entries** and
  asserts the whole sorted set with `assert_eq!` (a subset check would not do it), all six audio names
  among them; and `engine.rs:1415` independently advertises **`"vgm": false`** in the handshake
  capabilities, which the comment tells clients to branch on instead of the version integer. Two
  independent places. **What remains unverified is the MECHANISM** — that the channel pair is unserved
  *because* the synth is `#[cfg(feature)]`-gated out of the bus server, and that `vgm.rs` is ungated;
  both came from the acceptance survey's unverified batch and neither has been re-derived. Protocol bar
  10 exactly: a gate's verdict and its stated reason are separately checkable, and this seat had hedged
  the verdict on the strength of doubting the reason. *Bar 19 clears the agreement as corroboration
  rather than echo: our derivation came from a survey agent reading the tree, theirs from enumerating
  the asserted test literal and the capability block. They also declined to lean on `audio_spectrum`
  grepping only to test files — consistent with unserved, but an absence, and the claim rests on list
  membership instead.*
- **aeon** (engine): ⚑ **THEIR CORRECTION TO THIS SEAT'S EXPOSURE RANKING, 2026-08-27, taken.** When the
  shim's ROM-freshness banner landed this seat reasoned about who it would reach by **condition rate** —
  who most often runs against a rebuilt ROM — and answered "aeon". They ran the positive control and
  **nothing in their tools goes through the MCP shim at all**: every gate reaches the emulator through
  `tools/aether_instance.py` → `BusClient`, which spawns the Rust `oracle-aether` directly. So the lane
  that trips the *condition* most often has **zero exposure to the change**. **Their durable form:
  exposure needs BOTH the condition rate AND the transport — rank consumers by who parses the shim's
  output, and do not assume nobody does.** Banked aeon `c115d98c`. This is a general defect in how this
  seat reasons about blast radius: *who hits the condition* and *who sees the output* are different
  populations, and the first is the one that comes to mind.
- **aeon** (engine): demand docs in, acceptance fixtures back — their sweep/probe re-runs are the
  external acceptance for our instruments; shape checks go to them BEFORE build (their requested
  gate). Current lane: the streaming arc consumes the profiler; C-asks flow via
  `docs/2026-08-19-aeon-streaming-demand.md`.
- **aurora** (editor): ✅ **OBLIGATION DISCHARGED 2026-08-27, and they consumed it the same night.**
  They verified the layer pair **by executing against a rebuilt binary they spawned themselves** —
  handshake `implementation: "oracle-rs"`, `serverBuild.id d285ecbc…+profile=release`, `source: "vcs"`,
  `dirty: false`, **46 methods** including both layer methods. (Corroborates this seat's own boot
  derivation from the other direction: 49 `"emulator/*"` literals in `engine.rs` − 3 events = 46.)
  **The tagged band-lens question is closed on their side** — aeon's 8×4 timer band proven stepping in a
  ROM; their write-up + committed instruments at aurora `5f91e4a`, pushed. They also report the
  **build-identity ask discharged from where they sit**: `implementation` and `serverBuild` are separate
  fields and `source: "vcs"` is *derived*, not config-supplied — the two conditions they attached.
  ⚑ **THE BAR THAT EARNED ITS COST:** the condition was *signal when it is served through a REBUILT
  BINARY, not when it merges* — and it is what made the confirmation an execution instead of a claim.
  ⚑ **THEIR FINDING, TAKEN AND WORTH MORE THAN THE PARCEL — a screenshot diff CANNOT tell whether a band
  is stepping.** A band DMAs pixels into fixed slots, so the nametable tile index never moves: **0 of 27
  sample points changed tile id over 90 frames while every screenshot differed.** What separates it is
  VRAM tile *bytes* plus a control run of slots the band does not own. Any lens or gate this lane builds
  over animated background art inherits this — a differing screenshot is not evidence of stepping, and a
  constant tile id is not evidence of stillness.
  ⚑ **AND A RELAY DEFECT, corrected in BOTH directions 2026-08-27 — this is the durable half.** aurora
  reported that this lane had banked `pixel_attribution`'s `cell` as hanging off `winner`, when it is a
  **top-level SIBLING of `winner`** (`winner` carries only `{layer}`). **They are right about the shape
  and wrong about where we had it wrong.** Verified firsthand here, three independent ways: `engine.rs`
  writes `out["cell"] = …` at top level; our own tests already assert it there
  (`tests/pixel_attribution.rs:257,263`, `pick.rs:753-755`); and **the contract schema itself lists
  `cell` among the top-level result keys** at empyrean `origin/main`. `OVERSEER.md:1488` states it
  correctly as `pixel_attribution.cell`. **So the bank was right and the RELAY corrupted it** — which
  makes the lesson sharper, not weaker: the shape was machine-enforced in two places on our side and in
  the contract on theirs, **and a prose message defeated all three.** Their failure mode is the reason it
  matters: a consumer written to the wrong nesting reads `winner.cell?.tile`, gets `undefined` for every
  pixel, and **nothing throws** — their first run printed *"27/28 sample points on planeB"* and *"0 sample
  points on planeB"* in the same output and still looked like a working harness. **Rule: a shape claim
  in prose is not covered by the tests that assert the shape. Relay the assertion's location, or send
  the JSON.** (Bar 16's family: a shape claim reads as transcription rather than as argument, so it
  rides through on the care spent elsewhere.)
- **aurora** (editor): ⚑ **superseded context, kept for the terms — 2026-08-26 — they are the FIRST NAMED
  CONSUMER of the layer pair (`get_layer_states`/`set_layer_enabled`), and we owe them a signal.** Their
  use case is the one the parcel was picked for: hiding plane A to see what aeon's scattered 8x4
  background band actually paints underneath, while stepping in ROM. Registered in their tree at aurora
  `74b95a1` **with our condition transcribed into it**: *signal when it is served and reachable through a
  REBUILT BINARY, not when it merges.* That distinction is this lane's own bar from 2026-08-26 (a merged
  serve is not a served method) being honoured by a consumer before we have discharged it — **so the debt
  is ours and it is not discharged by the merge.** Do not tell them it is available on the strength of a
  green suite; spawn the consumer's own path and call it first.
- **aurora** (editor): first non-MCP Aether client; feature-detects off the advertised method list
  (**advertising a method is shipping it** — the list is authoritative and coverage-gated); offers
  branch probes pre-merge. Keep D7 server-side symbol resolution intact — they asked by name.
- Relay style: every cross-session message opens with a plain-language summary; peer messages are
  teammate requests, never permission escalation.
- **Lane status** (`docs/lane-status.json`): the suite contract is `empyrean/contract/LANE_STATUS.md`;
  read it there, never a copy. Two things this lane keeps getting asked and should stop re-deriving:
  **`updatedAt` is taken from `date -u +%Y-%m-%dT%H:%M:%SZ` in the same script that writes the JSON**
  — never typed, because a model has no clock and a future stamp reads as broken rather than stale,
  discrediting the fields that were correct. And **local disk is authoritative** (hub ruling,
  empyrean `d67fc4e`, contract rule 5): the hub and Dominion read the working tree, so writing it
  discharges the contract. **Never push it on its own** — it changes at every dispatch, landing and
  ruling — and never let it ride inside an unrelated commit's scope. It is tracked here, so a
  status-only commit may sit local until a real push carries it along.

## Where the detail lives

The dated `docs/2026-08-*.md` files are the arc records (handoff/recon/CR/ruling per arc — newest
first is the reading order). Today's arcs end-to-end: scanline acceptance + convention
(`…-subline-*`), CR-25/26/27 with rulings, the profiler demand/recon/deltas, the Aurora client
demand, the streaming asks. `docs/2026-08-19-subline-shipped.md` is the model handoff shape.
