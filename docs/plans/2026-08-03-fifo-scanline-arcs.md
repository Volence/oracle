# VDPFIFOTesting + Per-Scanline Capture Arcs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development — fresh
> implementer subagent per slice, two-stage review (spec compliance, then code quality) after each.
> Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Close the two arcs left open by the test-ROM conformance ledger
(`docs/2026-07-25-testrom-conformance.md`): (Arc A) drive the `vdp_port_access`
(VDPFIFOTesting) scorecard row from 9/16 toward 16/16 by finishing the FIFO/DMA
timing model, and (Arc B) add a per-scanline capture hook that upgrades the
L1-limited rows (`color_1536`, and re-adjudicates `cram_flicker`,
`direct_color_dma`).

**Architecture:** Both arcs ride existing seams. Arc B: `System`'s `Scanline`
event already calls `vdp.render_scanline(line)` for lines 0..224 and discards
the RGB (`system.rs:764-768`) — the hook stops discarding it behind an opt-in
sink, default path byte-identical. Arc A: a real 4-entry FIFO with coarse
slot-budget drain and wait states already exists (`vdp.rs`); the arc replaces
the two fake status bits (EMPTY hardcoded 1, FULL never set) with live
`fifo_len`-derived values and then fixes whatever the per-row probe shows,
slice by slice, re-freezing the scorecard row same-commit each time.

**Tech stack:** Rust, `crates/oracle-core` (`vdp.rs`, `system.rs`, `bus.rs`,
`render.rs`), conformance harness `crates/oracle-core/tests/conformance_roms.rs`.

**Scope ruling:** `CHARTER.md:53,102` lists VDPFIFOTesting as a non-goal. The
owner explicitly requested this arc on 2026-08-03; that supersedes the charter
line for this arc. Record the supersession in the conformance ledger when the
first Arc A slice lands.

---

## Ground rules (every slice, every commit)

- [x] `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean (fmt is a HARD gate).
- [x] Full test suite green: `cargo test -p oracle-core` (+ `--release` for the slow conformance/golden suites as the repo's usual practice).
- [x] Conventional commits. **No `Co-Authored-By` trailers.**
- [x] Integer math only (foundations rule) — no floats in slot/cycle/pixel code.
- [x] **Clean-room:** never read `/home/volence/sonic_hacks/oracle/` (C++ Oracle) source for behavior. Permitted behavior sources: the test ROM's own source/README (public, SpritesMind/Nemesis), community hardware docs, in-repo recon docs, BlastEm experiments. Cite the source for every behavioral pin in the commit message or design note.
- [x] Currency gates that must NOT move (frozen values, byte-identical):
  - `export_state_v1.rs` `GOLDEN_HASH = 0xBF5D_1E1A_A727_143B`
  - `oracle_differential.rs` hashes (`ORACLE_REGS_HASH`, `ORACLE_CRAM_HASH`, `ORACLE_VSRAM_HASH`)
  - `golden_frames.rs` six scene hashes
  - `determinism_gate.rs`
  - `singlestep_m68000.rs` (threshold ≥ 1,000,058; `m68000/*` zero-diff)
  - every `VISUAL-BASELINE frame_hash=` row in `conformance_roms.rs` BASELINE **except** the row a slice deliberately upgrades — and any deliberate BASELINE change ships in the SAME commit as the code that moves it, plus a same-commit amendment to `docs/2026-07-25-testrom-conformance.md`.
- [x] New serialized state fields must round-trip bincode and be used in their introducing slice. NOTE: FIFO/DMA/wait fields are in neither `state_hash` nor `export_state` → Arc A is currency-neutral by construction; keep it that way (no new fields added to either currency without a version-bump decision).

## Execution model

- Overseer (this session) dispatches worktree-isolated implementer agents, one slice at a time per arc; two-stage review after each slice (spec reviewer, then code-quality reviewer); overseer independently re-runs the money commands before accepting a slice.
- Arcs run in parallel worktrees. **Arc B merges first** (small, additive); Arc A rebases over it. Both touch `conformance_roms.rs` BASELINE and the ledger — conflicts expected trivial (different rows).

---

## Arc B — per-scanline capture hook

### Slice B1: the capture seam

**Files:** Modify `crates/oracle-core/src/system.rs` (Scanline event arm ~757-779, plus a setter), possibly `crates/oracle-core/src/vdp.rs` only if a signature needs threading. Tests inline or in a new `crates/oracle-core/tests/scanline_capture.rs`.

- [x] Design constraint set (implementer picks the exact shape within these):
  - Opt-in sink receiving each rendered active line during the run: `(frame_or_line_context, line: u16, rgb: &[(u8,u8,u8)])`. Follow the `BusEventSink`/`AudioSink` seam precedent (additive trait with defaulted no-op, or an `Option<&mut dyn …>` threaded like the audio sink) — pick whichever matches `run_frames`' existing plumbing with the least API churn.
  - Default path (no sink installed) byte-identical: same event order, same rendered/discarded call, zero new allocations on the hot path (the line Vec is already built and discarded today — hand out a borrow).
  - No new serialized state. No `state_hash`/`export_state` change. Determinism gate untouched.
- [x] Failing test first: install a collecting sink, run 1 frame, assert 224 lines delivered in order 0..224 with plausible RGB; assert a second run without sink produces identical `export_state_hash` sequence to a pre-change baseline (determinism/no-behavior-change proof).
- [x] Implement minimal seam; run new test + full suite; fmt/clippy; commit `feat(vdp): per-scanline capture sink — Scanline event hands out the already-rendered line`.

### Slice B2: harness capture mode + `color_1536` upgrade

**Files:** Modify `crates/oracle-core/tests/conformance_roms.rs` (add `frame_hash_scanline`, re-point the `color_1536` row), amend `docs/2026-07-25-testrom-conformance.md` (L1 section + row), same commit.

- [x] Add `frame_hash_scanline(sys, settle_frames)`: run with a sink that keeps the LAST complete frame's 224 captured lines, FNV-1a over the same byte layout as `frame_hash` (r,g,b per pixel, line-major). Integer only.
- [x] Visual verification is MANDATORY, not hash-only: dump the captured frame (scratch PPM via a temp example or test-side write to target/) and LOOK at it — `color_1536` must visibly show the >61-color gradient effect that end-of-frame capture cannot show, i.e. captured frame ≠ end-of-frame frame. Keep the dump code out of the commit or behind the existing frame-dump example pattern.
- [x] Flip the `color_1536` BASELINE row to the new scanline-capture hash with a caption noting per-scanline capture; keep the end-of-frame hash noted in the ledger amendment for history. Same-commit ledger edit: L1 narrows (color_1536 upgraded; remaining rows listed with precise residual reasons).
- [x] Full suite green (all OTHER visual baselines unchanged — this proves default-path neutrality end-to-end). Commit `feat(conformance): color_1536 upgraded to per-scanline capture — L1 narrowed`.

### Slice B3: re-adjudicate `cram_flicker` + `direct_color_dma`

**Files:** `conformance_roms.rs`, ledger. Findings-first; code only if the probe shows the effect is capturable per-line.

- [x] Probe both ROMs under the scanline sink (scratch, uncommitted): does the per-line capture show the effect?
  - Expected per recon: `cram_flicker` is border-only (we render 224 active lines, no border) → likely still NOT-RENDERABLE, reason narrows from "end-of-frame" to "border/overscan not rendered". Do NOT build border rendering in this arc — ledger it as a named follow-up.
  - `direct_color_dma` needs sub-scanline (per-pixel) CRAM; CRAM writes carry no h-position and bus mclk is instruction-start-granular → still NOT-RENDERABLE, reason narrows to "sub-scanline CRAM state; write-time h-position not modeled". Ledger as named follow-up with the two missing prerequisites spelled out.
- [x] If (unexpectedly) either row IS capturable per-line: upgrade it exactly as B2 did for color_1536 (visual check + hash + same-commit ledger).
- [x] Commit `docs(conformance): cram_flicker/direct_color_dma re-adjudicated under scanline capture` (or feat, if a row upgraded).

## Arc A — VDPFIFOTesting FIFO/DMA timing

> **BOTH ARCS COMPLETE — 2026-08-03.** `vdp_port_access` reached **16/0/16** (page 1 `9/0/9`), from 9/7/16
> at the start. Boxes below are ticked to match; two things did not go as this plan described, and the
> ledger (`docs/2026-07-25-testrom-conformance.md`) is authoritative over this file where they differ:
>
> * **T16 took three slices, not one.** Slice A1 (live EMPTY/FULL flags) moved it 26/80 → 62/80 but did not
>   flip it. The rest split into two independent causes — **T16/S1** intra-line access-slot positions
>   (groups 2/3/5/6/8, 62 → 72 verdict bytes) and **T16/S2** post-DMA FIFO occupancy (groups 9/10,
>   72 → **80/80**), the latter answering design question Q1 of `docs/2026-08-03-a3-dma-fifo-design.md`.
>   Neither was the "Phase 3 per-line DMA cost" deferral T16 had been filed under; that work
>   (`Vdp::dma_cost` integrating across the lines a transfer spans) is **not** needed for T16 and stays
>   deferred.
> * **Slice A3 became A3a + A3b**, and T12 got its own slice after A2 left it as a named residual.
>
> Every slice landed currency-neutral: `export_state_v1::GOLDEN_HASH`, `oracle_differential`,
> `golden_frames`, `determinism_gate`, `singlestep_m68000` and every `VISUAL-BASELINE frame_hash=` row are
> byte-identical to their pre-arc values. Two owner rulings remain PARKED
> (`docs/plans/2026-08-03-PARKED-owner-ruling.md`) and are unrelated to this ROM's 16/16.


### Slice A0: per-row probe — DONE 2026-08-03

Per-row verdicts decoded from plane-A glyph colors (green ink `$0040` = byte
matched hardware, red `$000C` = mismatch); expected-value tables read from the
ROM binary (authoritative hardware answers — e.g. test 13's at ROM `$22FA`).
Red-byte counts reproduce the frozen aggregates exactly (6/3/9, 9/7/16).

PASS: 1 FIFO Buffer Size, 2 Separate FIFO Read/Write Buffer, 5 FIFO Write to
invalid target, 7/8/9 VRAM/CRAM/VSRAM Byteswapping, 11 Register Write Bit13
Masked, 14 CP Write Pending Reset, 15 Read target switching.

FAIL (7): 3 DMA Transfer using FIFO, 4 DMA Fill FIFO Usage, 6 8-bit VRAM Read
target 01100, 10 Partial CP Writes (slot 8 only), 12 Register Write Mode4
Mask, 13 Register Writes and Code Reg, 16 FIFO Wait States.

### Slice A1: live FIFO occupancy status flags — fixes T16 "FIFO Wait States"

**Files:** Modify `crates/oracle-core/src/vdp.rs` (`status_word` ~378-406; drain-to-now plumbing), inline tests. BASELINE row + ledger same commit.

- [x] Probe evidence: T16 expects status-flag groups `0100 0100 0000 0200` (bit 8 `$100`=FULL, `$200`=EMPTY, `$000`=partially filled); we return `0200 ffff ffff 0200` (EMPTY always; two probes per group hit the test's `ffff` sentinel). Today bit 9 is hardcoded 1 and bit 8 never set, while `fifo_len` is tracked live right next door for wait states.
- [x] Fix: status read drains the FIFO to `now` (coarse slot clock already supports this) then reports bit 9 = (`fifo_len == 0`), bit 8 = (`fifo_len == 4`). Chase the `ffff`-sentinel probes to their actual mechanism before declaring done.
- [x] Failing tests first: (a) after 1 enqueued word during active display, EMPTY=0 FULL=0; (b) after 4, FULL=1; (c) after draining past enough slots, EMPTY=1; (d) status read does not consume FIFO entries.
- [x] Coupling watch: live ROMs hammer `status_word` (TF4, this ROM). Full conformance scorecard — ONLY `vdp_port_access` may move; any other row moving = stop and triage. Currency suites untouched.
- [x] Commit `feat(vdp): live FIFO EMPTY/FULL status flags from fifo_len (A1, T16)`.

### Slice A2: control-port/code-register edges — fixes T13, T12, T10

**Files:** `crates/oracle-core/src/vdp.rs` (`control_write` ~609-642, `write_register` ~595-604, `target_of`/read path), inline tests. BASELINE + ledger same commit.

- [x] T13 "Register Writes and Code Reg" (expected table ROM `$22FA`): a `$8xxx` register write via the control port must also clobber the code register (data-port read afterward hits an invalid target → reads `ffff`); ours leaves the previous read target armed (slot 2 read `01234567`, expected `FFFFFFFF`).
- [x] T12 "Register Write Mode4 Mask" (`$20C8`): with Mode 5 disabled, register-number masking on `$8xxx` writes differs — our current always-Mode-5 masking reproduces test 11's pattern instead. Pin exact mode-4 masking from the ROM's expected table.
- [x] T10 "Partial CP Writes" (`$FC86`): 13/14 words already match; slot 8 expects `ffff` where we serve `0246` — one half-written-command sub-case must leave the read target invalid. Identify the exact sub-case from the ROM's write sequence.
- [x] Same recipe: pin from ROM tables → failing unit tests at the port level → minimal fix → scorecard (only this row moves) → commit `fix(vdp): control-port code-reg clobber, mode-4 reg masking, partial-CP invalid target (A2, T13/T12/T10)`.

### Slice A3: DMA routed through the FIFO — fixes T3, T4

**Files:** `crates/oracle-core/src/vdp.rs` (`run_fill` ~812-847, `dma_write_word` ~730-735, fifo plumbing), `bus.rs` `run_mem_dma`, inline tests. BASELINE + ledger same commit.

- [x] T3 "DMA Transfer using FIFO" (`$5DE8`): marker words stuffed into the FIFO then a 68k→VRAM DMA — on hardware the DMA payload passes through the FIFO and interleaves with the pending writes (expected `c800 c800 c000 c000 d800 … f111`); we commit queued writes verbatim and run DMA separately (got raw markers `3000×4 …`).
- [x] T4 "DMA Fill FIFO Usage" (`$DC30`): the fill's priming data-port write must land as a normal full-word FIFO write (both bytes) before the fill replicates the MSB, with the byte-placement quirk at the boundary (expected `1234`/`0012` where we got `1212`/`0000`).
- [x] This is the deepest slice — the implementer designs the minimal DMA-through-FIFO model that reproduces the ROM's expected tables WITHOUT breaking the currency-neutrality invariant (FIFO/DMA fields stay out of both currencies) or the existing DMA tests (CD5 semantics, TF4/Batman fixes). Design note first if the shape is non-obvious; overseer reviews before code.
- [x] Commit `feat(vdp): DMA transfers/fills route through the FIFO (A3, T3/T4)`.

### Slice A4: 8-bit VRAM read target CD=01100 — fixes T6

**Files:** `crates/oracle-core/src/vdp.rs` (`target_of` ~431-437, `data_read` ~910-929, read-buffer fill), inline tests. BASELINE + ledger same commit.

- [x] T6 (`$DEB0`): undocumented CD code `01100` = 8-bit VRAM read — returns one VRAM byte in the low half; the HIGH byte comes from stale FIFO/prefetch contents (expected `9922 9944 bb66 …` vs our full-word `1122 3344 …`). Depends on the FIFO state model, so runs after A1/A3.
- [x] Pin the exact byte-lane + stale-source rule from the ROM's expected table; failing unit tests; minimal fix; scorecard; commit `feat(vdp): 8-bit VRAM read target — low VRAM byte, stale high byte (A4, T6)`.

### Arc A exit

- [x] Scorecard row at its new maximum (target 16/16; any residual row gets a named, pinned reason in the ledger — no silent partials).
- [x] Ledger: conformance doc's `vdp_port_access` row rewritten (counts, remaining caveats, charter-supersession note).
- [x] Final whole-arc code review (both arcs merged), then push per owner's usual flow. *(Review DONE — two-stage per slice through T16, plus the T16 differential-ROM re-check: Gunstar / Thunder Force IV / Batman, 600 frames, all 18 comparisons byte-identical.)* **PUSHED 2026-08-13 (`0586dac..f123696`, 21 commits) after a full re-gate: `cargo fmt --check` clean, `cargo clippy --workspace --all-targets` 0 warnings, `cargo test --release --workspace` exit 0 = 929 passed / 0 failed / 16 suites, and the conformance scorecard re-run under `--nocapture` to confirm 17/17 ROMs actually ran (no silent vendor skip) with `vdp_port_access` 16/0/16 and memtest 13/13. THIS PLAN IS CLOSED.**

---

## Self-review notes (overseer)

- Spec coverage: Arc B slices cover seam + upgrade + re-adjudication of all three L1 rows; Arc A covers probe + the one known-fake mechanism + every category the ledger lists for the 7 failures, gated on A0 evidence.
- The only intentionally deferred content: exact A2+ slice list pending A0 (staging decision, amended before any A2+ dispatch — not a placeholder to be "filled in later" by an implementer).
- Type/name consistency: sink naming left as a constrained implementer choice in B1; B2 depends only on "a sink exists that yields (line, rgb)" — the B2 implementer must read B1's landed API before writing code.
