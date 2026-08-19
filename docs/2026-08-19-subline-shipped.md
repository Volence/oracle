# F-SCANLINE-SUBLINE — shipped: the row stops being atomic in CRAM (2026-08-19)

Branch `subline-s4`, cut from `subline-s3` / `subline-s1` / `m68000-microop-framework`. Four
implementation slices plus three adversarial-review rounds. **Everything below is unmerged**, pending the
controller's merge window — see §Push state.

Demand: Aeon's first `emulator/scanlines` acceptance sweep
(`aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-RESULTS.md`), whose §"`flipX` is 0 at every single N
— that is the finding" reported a constant `0` at all 201 sampled N **because a partial row could not exist
on this surface**. It can now.

Design + rulings: `docs/2026-08-19-subline-recon.md` (adopted as designed),
`docs/2026-08-19-ruling-subline-recon.md`.

---

## 1. What shipped

| slice | commit | what |
|---|---|---|
| 1 | `8b87312` | the mclk reaches the VDP's write chokes — a `now_mclk` shadow set by the already-timed entry points |
| 1b | `01866a7` | a captured `VdpWrite` carries its own clock (retires the thrice-registered `F-TRACE-VDPWRITE-MCLK`) |
| 2 | `7304dfa` | `vdp::subline_x(d_mclk, h40)` — the mclk → pixel-x mapping, as a pure function, used by nobody yet |
| 3 | `081dae2` | **deferred emission with an empty journal** — the neutrality slice |
| 3-review | `a5a2274` | 3 doc-truth MUST-FIXes + 6 notes from adversarial review |
| 4 | `d1cd2d9` | **the journal, the segments, the behaviour** + the `a2` gate rewrite + the `color_1536` re-pin |
| 4-review | *this commit* | 3 MUST-FIXes, 7 notes, slice 5's prose |

### The mechanism, in one paragraph

`Vdp::resolve_line` **never reads CRAM**. Every input it takes works in the index domain; CRAM enters at
exactly one place, `pixels_rgb`, the *decode* stage. So a CRAM write landing inside line N cannot change
which index any pixel of line N resolved to — it can only change the colour those already-decided indices
decode to. Sub-line CRAM is therefore not "resolve the line in segments" (which would move the sprite latch
commit, every non-CRAM input, and the whole `LineReport` — rejected as option (i)); it is **decode the
already-resolved line in segments**. Row N is still resolved at line N's `Scanline` event — same instant,
same inputs, same `commit_scanline_sprites` — and is *emitted* at line N+1's, from a retained `LineReport`,
a 128-byte line-start CRAM snapshot, and a journal of the CRAM writes that landed inside line N. The
expensive, currency-loaded half of the renderer is not touched and does not move in time.

That single fact is why the arc came in far cheaper than its registration ("larger than either
F-SCANLINE-INDEX or F-SCANLINE-SH") anticipated, and why Exodus-style per-pixel-clock stepping was declined
rather than adopted.

### The invariants the implementation is held to

* **Segments are internal.** One complete row per `on_scanline`, always. ~10 length/ordering assertions
  across the capture tests depend on it, and the interface has to keep meaning "a row".
* **Coalescing by pixel is mandatory, not an optimisation.** `direct_color_dma` pushes 44,352 CRAM words
  inside one instruction; under decision C-6 they share one clock and one `x`. Landings at the same pixel
  are one segment; within a segment a later write to the same address replaces the earlier one.
* **The retained row is not machine state** (decision D-1). `ScanlineScaffold`'s `PartialEq` is constant
  true and its `Encode`/`Decode` move zero bytes, so it is invisible to `System: PartialEq`, to
  `state_hash`/`export_state`, and to the checkpoint byte format.
* **One decode, two arguments.** `cram_rgb_state_from(cram, index, state)` is the single CRAM decode;
  `Vdp::report_rgb` *is* `report_rgb_with_cram(self.cram(), report)`. "Decoding later against the snapshot
  equals decoding now against live CRAM" is one function applied to two arguments, not two functions that
  agree today.

---

## 2. Currency ledger — one mover, every mechanism named

Measured firsthand on this branch with a throwaway `BusEventSink` probe (CRAM writes bucketed by
in-active / blanking / off-screen, per frame, line and index, with `old` vs `new`, against the set of
indices the picture actually samples). The probe was deleted before commit; every number below is
reproducible from `cargo test` output plus that instrument.

| row | verdict | mechanism |
|---|---|---|
| **`color_1536`** | **MOVES** — `0x917371f07409cb25` → **`0x9ae4acc58d2a382d`** | In the hashed frame (119): **515 value-changing CRAM writes inside the active-display window**, to indices **4/5/6/7** (~129 each), over **131 active lines, 48→221** — and those four are among the six indices the picture samples (0,1,4,5,6,7). Each row now shows the palette evolving across its own width instead of one snapshot of it. A row that changes colour part-way across **is** the 1536-colour trick, so this is the first hash in the corpus that contains the effect the ROM exists to demonstrate. |
| `direct_color_dma` | TAG resolved — **does not move** | It *does* split now — blocker 1 of ledger L1b ("CRAM writes carry no h-position") is retired. But the whole 44,352-word burst shares one clock (C-6), coalesces to one landing at **pixel 82 of line 1**, and that landing's surviving word is the value index 0 **already held** at line 1's start: net effect over the line, in the hashed frame, **`0x0000 → 0x0000`**. Both spans decode to the same colour. Its 2026-08-03 justification was literally the assumption this arc removes; it is replaced in place. |
| `cram_flicker` | TAG resolved — **does not move** | **2,692** value-changing in-active-window writes per hashed frame, all to indices **4 and 36**; the picture samples only index **0**. Every segment decodes the same colour. Exactly §E.2's prediction, now measured. |
| 9 low-risk `IDENTICAL-TO-POST-HOC` rows | **unchanged** | the STOP condition never triggered |
| other 5 `LIVE-DIFFERS` hashes | **unchanged** | `io_sample`, `m68k_opcode_sizes`, `shadow_highlight`, `vdp_sprite_masking`, `window_distortion` — VRAM / zero-active-write / register-only causes, all excluded by C-2/C-3 |

**Re-pin hygiene (ruling Q2, with the controller's slice-4 adjustment).** Both `color_1536` literals moved
**together to one measured value** — `scanline_goldens.rs:130` and `conformance_roms.rs:83` — and the
cross-check between the two files is **preserved**, with a comment in each saying so. Mechanism written into
each file's own `cause:` style. Travelling docs updated in the same commit:
`docs/2026-07-25-testrom-conformance.md` (:37 row, the L1 PPM comparison, the whole L1b section) and
`docs/2026-08-15-scanline-golden-coverage.md` (verdict row, cross-check note, and the pixel-sensitivity
table — where the flipped-bit column was deliberately **not** re-derived, because inventing a number nobody
measured is what that table exists to prevent).

**Non-movers, stated as such.** `state_hash`, `export_state` and `export_state_v1::GOLDEN_HASH` are immune
by construction — no rendered pixel enters either currency, and this arc changes no CRAM/VRAM/VSRAM/register
content and no bus timing. `golden_frames`' six scene hashes build static `Vdp` scenes and hash post-hoc
`render_line`: no run loop, no CRAM write "inside" a line. `watchpoints`' bus-access counts over five
vendored ROMs are unchanged — no new bus accesses exist.

---

## 3. Mutation ledger

Every evidence-bearing test carries a recorded mutation: edit → `touch` (cargo's fingerprint is mtime-based)
→ confirm a "Compiling" line → named test FAILED → revert → green.

| # | slice | mutation | result |
|---|---|---|---|
| 1 | 3 | flush moved from the top of the `Scanline` arm to after `on_frame_boundary` | `frame_boundary_fires_exactly_once_…` FAILED; log became `[Boundary(0), Line(223), Boundary(1), …]` |
| 2 | 3 | `ScanlineScaffold::PartialEq` made structural | `a_retained_row_is_invisible_to_the_machine_and_to_the_checkpoint` FAILED on whole-machine equality |
| 3 | 3 | `Encode` made to write the pending line | same test FAILED on *"the checkpoint is byte-identical…"* |
| 3b | 3-review | mutation 3 **re-run** after `RetainedRow.cram` became `[u8; CRAM_SIZE]` | still FAILED — the blindness tests are non-vacuous under the array shape |
| 4 | 3 | decode against **live** CRAM instead of the snapshot | `scanline_golden_scorecard` FAILED — the neutrality result is not vacuous |
| 5 | 4 | coalescing dropped | `a_forty_thousand_write_burst_at_one_clock_collapses_to_one_segment` FAILED, `left: 44352, right: 1` |
| 6 | 4 | one landing filtered out of the drain | the working-CRAM guard fired, naming the row: *"row 176: the journal did not account for every CRAM write of its line"* |
| 7 | 4 | mutation 4 **re-run** under the segmented emitter | `scanline_golden_scorecard` FAILED |
| 8 | 4-review | journal never fed (`if false && …`) | `the_row_a_mid_line_cram_write_lands_on_is_the_row_that_splits` FAILED — but on the *guard*, not its own assertion, so it was replaced by #9 |
| 9 | 4-review | line-atomic decode with a **correctly folded** working CRAM (guard satisfied; only pixels can catch it) | that test FAILED on its own assertion, `left: 0, right: 1` transitions |
| 10 | 4-review | N8's new assert inverted | `a2_two_timings_differ_and_the_boundary_moves` FAILED |

**A disclosed dud, and its replacement.** The first attempt at a mutation for the `a2` band substituted
`h_counter`'s 342-position grid for the 8/10-mclk pixel axis — and **passed**, because at H32 the two grids
are exactly equal (`3420 / 342 == 2560 / 256 == 10`). The replacement fed H40's 8 mclk/px to the H32 row and
did fail (band moves to 57..=99, measured column 53). **Review then corrected that too** — see M2 in §5: the
H40 substitution maps column 53 to 66, which is *inside* the band 46..=79, so the mutation only failed
because it also shifted the band, and the note claiming the gate "bites on B-2" was arithmetically wrong.
The honest statement now recorded in the test: the band bounds a **gross** mclk→pixel derivation error and
discriminates **neither** B-1 nor B-2 at H32.

---

## 4. What the process caught

* **The §D consumer audit was wrong, and the tests found it.** §D listed
  `last_frame_resyncs_after_a_reset_that_interrupts_a_frame` (`:309`) as holding under deferred emission. It
  does not: that test ends its first run mid-frame by *exactly* the same mechanism as the one §D did name,
  so its count moved 100 → 99. Measured (`left: 99, right: 100`), surfaced as a deviation from the
  "one authorized pin edit" instruction rather than applied silently, and **ratified** by the controller.
  The audit line is struck through in place in the recon with a dated correction, plus a method note:
  *enumerate the tests that end a run mid-frame; do not read down the file.*
* **Three review rounds, each with real MUSTs.** Round 1 (slice 3): the `clear` doc claimed a guarantee the
  code does not give; "arms the emitter" described a flag that does not exist; the equality/encoding
  blindness was documented in only one direction. Round 2 (slice 4): two rustdocs still stated the old
  arming rule; the `a2` discrimination note was arithmetically impossible; the conformance ledger still
  carried the retired literal and prose.
* **An unfalsifiable guard was replaced by a falsifiable one.** Slice 3 shipped
  `debug_assert!(journal.is_empty())` and *said in the code* that it was a tripwire, not a tested invariant
  (one constructor, hard-coded empty). Slice 4 replaced it with the design's own: the emitter's working CRAM
  after the last segment must equal live CRAM at emit time. Mutation 6 fires it; the predicate it rests on
  is tested directly both ways by
  `render::tests::the_emit_time_guard_predicate_holds_only_when_every_landing_is_journalled`.
* **Two TAGs that "could not be told statically" were told by measuring.** Both came back as
  non-movers — and `direct_color_dma`'s reason was *not* the one anyone predicted.

---

## 5. Review rounds, itemised

**Round 1 (after slice 3)** — F1 `clear` doc corrected (an unarmed run drops the row; an armed one inherits
it, and two armed sinks in sequence *do* hand it across); F2 the arming rule restated with the constancy
contract added to `wants_scanlines` (*the answer must not change for the duration of a run*); F3 the
one-directional blindness written down; F5 `RetainedRow.cram` → `[u8; CRAM_SIZE]`; F6 `#[inline]` on the
flush; F7 `debug_assert_eq!` on the decode's CRAM length; F8 `Vdp::report_rgb` documented as
production-dead and dangerous on a retained report; F9/F10 wording; F4 the D-2 converse documented; F11/F12
recorded honestly in code *and* commit body.

**Round 2 (after slice 4)** — M1 the two remaining rustdocs that still said "arms iff the sink wants VDP
writes" (`bus.rs`, `vdp.rs`); **M2** the `a2` discrimination note corrected (see §3); M3 the conformance
ledger's literal and L1b prose. Notes taken: N1 (cheap half — see §6), N2 (absolute mclk into
`journal_cram` + a line-match assert), N3 (the Z80 ordering hazard comment at the drain), N4 (a core-level
plumbing test with no spawned server), N5 (the guard predicate tested both ways), N6 (the TAG comments now
name *which* measurement is load-bearing — `scanline_goldens`' live-vs-post-hoc verdict, not the
structurally-immune `conformance_roms` post-hoc hash), N8 (the two uniform neighbour rows `a2` had dropped
from its equality lists without asserting anywhere).

---

## 6. Follow-ups registered

| id | what | why it is out of this arc |
|---|---|---|
| **F-SUBLINE-HGRID** | `h_counter`'s uniform 422-position H40 grid disagrees with the 8-mclk pixel axis by ~33 mclk at active-end (decision B-1) | `h_counter` feeds `$C00008` reads, `hblank()`, `hint_offset()`, `vint_offset()` — moving it is an observable behaviour change on **every** ROM, a different currency conversation from an opt-in capture |
| **F-VCOUNT-PHASE** | our `v_counter` increments at the line boundary; hardware increments mid-line at H `$A5` (~107 CPU cycles earlier) | same reason — it changes what `$C00008` returns for every ROM. Until it lands, HV-*polled* fixtures still disagree with the GUI oracle, now over a row's first `x` pixels rather than a whole row. Aeon's fixtures are HInt-dispatched, where this arc is the only difference |
| **F-SUBLINE-ACCESSMCLK** | stamp the write with the access instant *inside* the instruction rather than the instruction start | a strict refinement of the same seam. Bounds today's resolution at one instruction (8–26 px at H40) — still a ~15–40× improvement on one whole row |
| **F-SUBLINE-DMASPREAD** | a DMA burst lands at one pixel, not smeared across the slots it really occupies (decision C-6) | needs sub-instruction clock advance through a DMA body. **This is the surviving half of `direct_color_dma`'s blocker list** |
| **F-SUBLINE-CAPTURE-SCRATCH** | reuse the drained capture `Vec` instead of `mem::take`ing a fresh one each step | the allocation half of N1; the target-filter half shipped (see below) |
| **B-2 gate gap** (named, alongside F-SUBLINE-HGRID) | decision B-2 — the pixel axis comes from the resolved row's own mode, not a live register read — is implemented and unit-tested at core level, but **no wire-level gate pins it**: discriminating it needs a landing past ≈px 136, i.e. an H40 fixture or a mid-line mode switch | the existing fixture is H32 and lands at column 53 |
| **C-7 (Z80 CRAM writes)** | IN by construction, untested; the drain runs before `catch_up_z80`, so a Z80 CRAM write could be journalled against the wrong row | recorded as a comment at the drain site; the `journal_cram` line assert turns it into a loud failure rather than a wrong picture. No corpus ROM does it |
| **N7** | record-only | one comment line |

**N1's cheap half shipped.** A run that wants rows but not writes now arms the capture **CRAM-only**
(`Vdp::set_write_capture_cram_only`), so a 64 KiB VRAM fill DMA pushes zero entries instead of 65,536 built,
drained and dropped every frame on the player's hot loop. A run that also wants writes on the wire gets the
full, unchanged capture, so the watchpoints path records exactly what it always did. **Disclosed cost:** this
adds one transient `bool` to `Vdp` — alongside `capture_armed` and `in_dma`, which are already there — so the
bincode snapshot grows by one byte. No stored fixture, no pinned snapshot length, no golden depends on it.
Slice 3's "zero bytes" claim is about the *retained row* and is unaffected.

### Priced down, not scheduled

* **F-CRAMDOT** — **unblocked-adjacent, and now genuinely half-done.** Its description asks to "timestamp
  CRAM writes with an h-position **and** advance the clock through a DMA body"; the first half shipped here.
  The second half, plus the dot painted at the beam position regardless of the resolved index, it
  deliberately does not do (decisions C-4, C-6). `cram_flicker` and `direct_color_dma` remain its rows, and
  both now have measured rather than assumed justifications.
* **F-SCANLINE-INDEX / F-SCANLINE-SH** — **markedly cheaper, neither folded in.** They were held out because
  "the renderer resolves indices and S/H internally and hands the sink only RGB". The emitter now retains
  the per-line `Vec<PixelResolution>`, which carries `cram_index` and `state` per pixel — exactly the two
  fields those follow-ups need, already alive at emit time and already surviving to the sink boundary. What
  remains is a sink-interface extension and a §6 fragment, not a renderer change. Deliberately out of scope:
  adding fields to `on_scanline` is a contract-shaped change with its own four-surface accounting.

---

## 7. Acceptance handoff

Aeon re-runs `aeon/tools/hblank_window_sweep.py` against the new server, unchanged in form. Two numbers are
the acceptance criteria, and both are predictions this design can be held to:

* **`flipX` becomes a direct measurement** instead of the constant `0` it was at all 201 sampled N. Predicted
  **≈ 222** for their row-100 fixture, from §B's independent cross-check (their own measured landing, 253.6
  CPU cycles into line 100, run through our mapping: `253.6 × 7 / 8 = 221.9`). Accept within **[205, 225]** —
  the width is the instruction-granularity limit (F-SUBLINE-ACCESSMCLK), not slack.
* **Restated-A2 distinct pictures over N ∈ 0..57 rise from 4.** Each spin iteration is ~10 CPU cycles = 70
  mclk = **8.75 px** at H40, so consecutive N should be distinguishable. Accept **≥ ~30 at step 1**;
  anything near 4 means the arc did not land. (At step 3 the sample count itself is only ~19.)

In-tree, the same claim is gated by `crates/oracle-aether/tests/scanlines.rs`'s rewritten `a2` — structure
(uniform-A prefix, uniform-B suffix, exactly one transition, wholly-A at `line-1`, first wholly-B at
`line+1`, the two-ROM band swap, outside-band equality) plus a **band** for the transition column derived in
the test from `build_cram_midframe`'s own instruction stream. Derived band **46..=79**; measured column
**53**. `a1` (three boots byte-identical) is untouched and still green — that is where determinism of the
column within one boot lives.

---

## 8. Push state

**Nothing is merged.** All of it sits on `subline-s4` awaiting the controller's merge window:

* `oracle-next`: `subline-s1` → `subline-s3` → `subline-s4`, tip = this commit. Gates green at every
  intermediate commit.
* `empyrean`: the sub-line amendment sits at **`9b06933`** and merges **in the same window**, together with
  a schema re-vendor. Ruling Q1's vehicle is a numbered CR plus a §11.15 amendment entry — the reference
  server's documented §6 convention (`contract/protocol.md:1165-1189`) says a row is atomic and a mid-row
  landing is "not expressible", and both sentences become false the moment this merges. Q6's one-sentence
  `pixel_attribution` divergence note (it answers from a live post-hoc resolve, so after this arc it
  disagrees with `emulator/scanlines` *within* a row, not merely between rows) rides the same CR.

The ordering matters in one direction only: `contract/protocol.md` must not describe a server that no longer
exists, so the empyrean merge must not lag the oracle-next one.
