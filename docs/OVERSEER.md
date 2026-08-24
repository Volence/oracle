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

## The bars (house methods — each earned by a measured failure; do not thin)

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
