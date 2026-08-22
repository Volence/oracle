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

7. **⏳ PARTLY DONE (opened 2026-08-22 afternoon) — the peer schema-fragment arc.**
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
