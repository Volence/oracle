# Aeon's acceptance sweep against `emulator/scanlines` — results, and the row-content ruling (2026-08-19)

Addendum to [`docs/2026-08-19-scanline-readback.md`](2026-08-19-scanline-readback.md). That doc shipped
the surface and ended by naming the one thing it could not do itself:

> Carried forward from Tier 1, and now the switchover's live finish line: **nobody has run the
> Aeon-side sweep against this method yet.**

Somebody has now. This addendum records the result, adopts the one protocol change the run earned, and
answers the row-indexing question the demand side asked back. **Nothing in the handoff doc is rewritten** —
it is the record of what shipped, and it was accurate when written.

## What ran, and where it lives

| | |
|---|---|
| Demand-side results | `/home/volence/sonic_hacks/aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-RESULTS.md` (aeon **master**, `810b6d90`, merged `239c6ac7`) |
| Spec it ran | `aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-SPEC.md`, substrate item 1b |
| Driver | `aeon/tools/hblank_window_sweep.py` |
| Server under test | `oracle-next/target/release/oracle-aether` built at **`fdb6903`** (this branch's merge base) |
| ROM | `aeon/s4.debug.bin`, md5 `249a3193cfa67ebd31b9894ad059f86a` (aeon master `1185f223`) |
| Volume | anchors 5 fixtures x 3 captures, content map 42 windows x 2 captures x 2 masks, A1 3, A2 22, five sweeps incl. one of 201 captures |

## A1 — determinism: **PASSED**, and that is the finish line closed

Three **separate `oracle-aether` processes** (not three resets in one process), each driven through an
identical schedule, produced **byte-identical** rows — the full RGB of six rows concatenated, 11,546
characters, compared as bytes rather than as a hash of a summary.

Two things ride on this beyond the pass itself:

- **`source == "raster"` never once failed.** The demand-side driver asserts it as a hard failure on
  *every* capture it takes — every anchor, every map window, every A1/A2 capture, all 201 captures of the
  wide sweep. It never fired, and `mode` (`h40`, 320 columns) never fired either. The `stateRender`
  fallback path — the one structural way this surface can answer blind — was never taken in the whole
  session. `caveat` was absent from every reply, as the contract requires on a raster answer.
- **A1 is the criterion three prior Aeon capture protocols failed on their own controls.** It now holds,
  which is what makes every cross-capture comparison in their document legitimate — and what retires the
  ~20-screenshot ritual this row was built to replace.

A3 (many rows in one call), A4 (rendered RGB with S/H applied) and A5 (320 active columns) also held;
out-of-range requests were refused, never clipped, as documented.

## A2 — restated, and the restated form is the acceptance protocol from here

**A2 as literally handed over does not discriminate on this surface, and that is not a defect in the
server.** The handed-over form was: *"N = 0 and N = 17 MUST produce different content on row 99."* Measured,
they produce **0 differing columns across all six rows** — because a 17-iteration spin difference (~170
cycles) is smaller than one scanline (488.6 cycles), so both landings fall inside **one quantum of the
instrument's resolution**. Limitation L1 (one render per line, at line-start — `system.rs:1031-1049`) is
the reason, and it was documented before the sweep ran.

Ruling the instrument blind on that pair would have been wrong, and the demand side did not: a post-hoc
render answers identically for **every** N, so they restated A2 as a poison that can tell coarse from
blind:

> **A2 (restated).** Scan the spin parameter over a range that spans at least one scanline, capture at
> each value, and count **distinct pictures**. A blind (post-hoc / `stateRender`) instrument yields
> **exactly 1**. Anything greater than 1 proves the capture is live.

Measured: **4 distinct pictures over N ∈ 0..57 step 3**, grouped `[0..21] [24] [27] [30..57]`. PASS.
Run against the spec's own literal fixture (CRAM `$4A`, header mask `$0002`) the restated form correctly
returns **1** and the driver stops with BLOCKED — that fixture is vacuous for an unrelated content-trap
reason they found and fixed (see their §"Fixture defects"; both defects are demand-side, neither implicates
this server).

**Adopted: the restated form is A2 in this acceptance protocol going forward.** The handed-over literal
pair is superseded — it named one row and one pair of N, and it fails on a working instrument whenever the
two values happen to share a quantum. Recording the supersession here rather than editing the handoff doc:
the handoff records what was handed over, this records what came back.

## Does our own suite gate need to change? **No — verified, with the anchor.**

`crates/oracle-aether/tests/scanlines.rs:302`, `a2_two_timings_differ_and_the_boundary_moves`, was **never**
the literal N-pair. It boots two ROMs whose landings are **100 scanlines apart** —
`build_cram_midframe(50)` vs `build_cram_midframe(150)` (`:303-304`) — i.e. ~100 quanta apart on a surface
whose quantum is one line, so the collapse that defeated the literal A2 cannot occur here. It then asserts
four independent things, and the restated A2 is subsumed by the second:

- `source == "raster"` on both replies (`:308-316`) — the same MUST the demand-side driver enforces per
  capture;
- **within one frame**, row 40 differs from row 160 (`:321-326`) — a post-hoc render draws the whole frame
  in the last colour written, so this alone fails a blind server. That is exactly "distinct pictures > 1",
  measured within a single reply instead of across a scan;
- the **boundary row exactly** (`:327-338`) — 51 for the 50-ROM, 151 for the 150-ROM;
- the **whole band 51..=150 swapped between the two ROMs** (`:341-352`) and the rows outside the band equal
  (`:355-361`), which is what makes the disagreement a *timing* difference rather than two unrelated
  pictures.

That is strictly stronger than the restated A2 (which counts distinct pictures; ours names which rows must
differ and which must not). **No change is proposed to it.** The one thing it does *not* cover is the
literal pair's original intent — sub-quantum discrimination — and nothing on this surface can cover that;
see the follow-up below.

## CR-24's adoption condition — where it now stands

Status only; `docs/2026-08-18-cr24-scanlines.md` is the CR's record and is not edited.

| Clause | State |
|---|---|
| 1 — a conformant reply passes the fragment closed, plus one refusal per catalogued bound | green since the shipping slice (`docs/2026-08-19-scanline-readback.md` §Gates) |
| 2 — **A1 determinism**, verbatim | **discharged 2026-08-19** by the demand-side run above: three separate processes, byte-identical |
| 3, suite gate (i) — the two-timings poison | green, unchanged, and re-verified as fit for purpose above |
| 3, suite gate (ii) — `color_1536` raster ≠ stateRender | green, unchanged |
| 3, acceptance protocol — the verbatim A1/A2 sweep against the Aeon fixture | **run 2026-08-19.** A1 verbatim held. **A2 verbatim did not discriminate** and is superseded by the restated form above — the surface is coarse, not blind, and the run proved which by measurement rather than by argument |

## The row-indexing question: the `+1` is our sampling convention, not a latency bug

The demand side observed the tint boundary at **authored line + 1** on every fixture here, where the GUI
oracle (the Exodus-derived C++ emulator) put the same fixtures' boundary at the **authored line**, with both
instruments agreeing on the window position in *cycle* space to within ~1 spin iteration. Their question:
convention, or real one-line latency?

**Verdict: a sampling convention, and one this repo already asserts as intended behaviour.** Chain:

1. **The capture fires at line-start.** The `Scanline` event for line N is scheduled at exactly
   `N * MCLK_PER_LINE` (seeded `system.rs:405`, self-rescheduled `system.rs:1071`), and its handler calls
   `self.vdp.render_scanline(line)` immediately — `system.rs:1043-1048`. The row handed to the sink is that
   one render, decoded to RGB; the sink stores it in arrival order and the Aether handler slices row `line`
   at offset `line * width` (`scanline_capture.rs:141-165`, `engine.rs:1749-1758`). **No ±1 anywhere in the
   row indices** — they are the renderer's own line numbers end to end.
2. **Events are delivered before the CPU step.** `run_until_with_sink` drains `pop_due(now)` at each
   instruction boundary *before* stepping (`system.rs:970-975`). So the render of line N strictly precedes
   any instruction that begins at or after `N * MCLK_PER_LINE`.
3. **The HV counter cannot report line N any earlier than that.** `$C00008` returns
   `hv_counter_read(now_mclk)` (`bus.rs:933`), whose V byte is
   `(mclk % MCLK_PER_FRAME) / MCLK_PER_LINE` remapped (`vdp.rs:339-352`), and `now_mclk` is the
   scheduler's clock **at the start of the executing instruction** (`system.rs:1099`, passed into
   `MegaDriveBus::new`). Therefore the first instruction that can *read* V == N is one that begins at
   `mclk >= N * MCLK_PER_LINE` — after step 2 has already rendered row N.
4. **Therefore a write following such a poll can only be visible from row N+1.** Not "usually" — by
   construction of the delivery order, for any fixture that polls HV.

That is stated in three places already, all written before the sweep: `bus.rs:91-95` (conformance
Limitation L1), `testrom.rs:425-427` (*"The line the write lands on has already been rendered (the Scanline
event renders line N at N's start), so the boundary sits at `line + 1`"*), and the gate's own doc comment
at `scanlines.rs:298-300`.

**Our own fixture lands at +1 too — there is no discrepancy between the two fixtures to reconcile.**
`build_cram_midframe(line)` (`testrom.rs:442`) spins on `$C00008` until V >= `line`, then writes CRAM
(`testrom.rs:534-536`), and the gate asserts row **51** is the boundary for `build_cram_midframe(50)`
(`scanlines.rs:327-338`): line 50 still colour A, line 51 colour B. So this server's own acceptance gate
encodes the same `+1` the demand side measured. Aeon's fixture reaches it by a different route — an
HInt-dispatched raster op whose burst their sweep places ~253 cycles into line 100's active display, at
pixel ≈222 — but the rule is the same one in both cases: *a write inside line N's period is first
expressible in row N+1.*

### Why the GUI oracle answers differently — and why that is not a defect on either side

Two independent differences, both pre-existing and both documented:

- **(a) Sub-line vs line-start rendering.** Exodus advances its render *"a single pixel clock cycle at a
  time"* (`oracle/Devices/315-5313/S315-5313_Rendering.cpp:285`, loop at `:322-345`), so a write landing
  mid-row recolours the remainder of **that** row — which is why R1 measured a partial row with a flip at
  x≈170, a shape this surface cannot express at all. Ours resolves the row once, atomically. For any write
  landing after a row's start, the two instruments differ by one row on "first row showing the new colour"
  **by construction**.
- **(b) V-counter phase.** Exodus increments V at `hscanSettings.vcounterIncrementPoint`, H40 internal
  `0x14A` (`S315-5313_Timing.cpp:117`, applied `Rendering.cpp:306/339`) = external H `$A5`, which is
  **15 pixels before the end of the previous line's active display** — about 89 of the line's 420 counter
  positions early, ~103 CPU cycles before the line boundary. Ours increments V **at** the line boundary,
  which `vdp.rs:339-343` names in its own doc comment as a known sub-line phase difference and a
  documented open item (recon R2). For an HV-polled fixture this alone is worth a whole row against a
  sub-line renderer: on Exodus the poll exits in the previous line's tail and the write lands before line N
  is drawn.

So for **HV-polled** fixtures (ours) the difference is (a) and (b) compounding; for **HInt-dispatched**
fixtures (Aeon's) it is (a) alone. Neither is a defect in the capture: the row content is exactly the
render, the render is exactly line-start state, and the row indices are exactly the renderer's line
numbers.

**Contract note drafted**: `empyrean` branch `scanline-row-convention-note` (`ba6ca8e`) — a prose-only,
non-normative clarification appended to §6's `emulator/scanlines` blockquote, stating that a row is an
atomic line-start sample, that a write during line N first appears in row N+1, that a mid-row landing is
inexpressible, and that the catalog does not pin the intra-line sampling point (so a sub-line server
showing a partial row on line N is a permitted difference, not a conformance dispute). Unmerged; the
vehicle — plain prose edit vs a numbered CR + §11.15 entry — is the controller's call.

## Registered follow-up

- **F-SCANLINE-SUBLINE** — *sub-line landing resolution.* Named by the demand side as **the** gap that
  blocks their sweep's own purpose: *"Aeon's raster work needs sub-line landing resolution — a landing
  inside a row, i.e. the row rendered from the CRAM state as it evolves across the row. Until then this
  surface can measure a landing to ±1 scanline, which is enough to bracket a window from one side and not
  enough to close it."* Concretely their §6 procedure could measure the window's **upper** edge (N = 21.5)
  but had to **derive** the lower one (15.21) from blanking width, and the px/cyc cross-check is
  unobtainable here — `flipX` was `0` at all 201 sampled N, because a partial row cannot exist. This is a
  **core renderer change** (resolve a line in segments, or re-resolve on CRAM writes within a line) with
  its own currency-neutrality scrutiny — the same reason `F-SCANLINE-INDEX` / `F-SCANLINE-SH` are out of
  the bus row — and it is larger than either. Registered, not scheduled; the owner picks.
- Complements, does not replace, **F-SCANLINE-INDEX** (per-pixel CRAM index) — that one answers *which
  entry* a pixel used, this one answers *when within a row* a write took hold. Aeon's sweep wants both, and
  ranked index first.

## What the sweep incidentally confirmed about this server

Recorded because it is independent corroboration of our timing model from a source that was not looking
for it:

- The sampling period was measured at **490.0 cycles** across four independent line boundaries
  (N = 22, 71, 119, 169), against an arithmetic H40 NTSC line of 3420/7 = **488.6** — a ratio of 1.0029,
  inside the sweep's own ±10-cycle quantization. Our `MCLK_PER_LINE = 3420` (`vdp.rs:17`) is what that
  measures.
- Within a burst, the three `move.w (a1)+, VDP_DATA` writes cross the sampling instant 3 N apart —
  **30.0 cycles per word**, averaged over 8 intervals — which is an independent measurement of a constant
  Aeon ships (`RASTER_STREAM_WORD_CYC = 30`) taken through our 68000 timing model.

## Verification note

**Docs-only change on this branch** — one new file under `docs/`, nothing under `crates/`, so per the
standing rule no `cargo test --workspace` run was required and none is claimed. No emulator MCP tooling was
touched. Every measurement quoted above is the demand side's, from the file cited at the top; every code
statement is from source read in this tree at `fdb6903`.
