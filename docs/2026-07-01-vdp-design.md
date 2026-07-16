# VDP design brief — scanline-first model + the render-decode introspection API

**Status: RATIFIED 2026-07-01 (owner).** Settled ground for the VDP work; the
**[recon]**-tagged details below may still be amended with evidence by the recon push.
Closes finding 1 of `docs/2026-07-01-plan-audit.md`: the VDP is the declared #1 schedule
risk and the render-decode introspection API is the product differentiator — both need
foundations-grade design *before* the scanline renderer exists, because Phase 3 promises
to upgrade scanline→dot-accurate **behind this unchanged API**.

Facts below are stated at two confidence levels: **[settled]** (safe to build on) and
**[recon]** (verify during the VDP recon push — via official Sega docs, Plutiedev,
SpritesMind, and *behavioral* test-ROM/differential experiments only; BlastEm and
jgenesis source are study-only GPL and stay closed per the clean-room rule).

> **Amendment (2026-07-16, recon push — §6.1 complete):** every [recon] tag below has been
> burned down; the evidence (verbatim citations, confidence, the pin-vs-defer disposition of
> each remainder) is in **`docs/2026-07-16-vdp-recon.md`** (items R1–R12). Tags in this brief
> are updated in place to **[settled — Rn]**; one correction is flagged inline (sprite
> masking "two modes" — mode 2 does not exist). The control-port toggle rules include one
> instrument-sourced pin (BlastEm experiment `tools/blastem-differential/run_vdp_pending.py`,
> STOP×trace precedent). The interrupt section now also carries the pending/IPL-deassert
> model that closes the standing set_ipl docket item.

## 1. The core design principle

**The introspection API is defined against decoded *semantics* — evaluation results,
resolution outcomes, and the reasons for them — never against renderer internals.**
"Which sprites dropped on line N and why" is a semantic fact about the machine; "what
the line buffer contained at slot 12" is an implementation detail. Phase 1's scanline
renderer and Phase 3's dot-accurate renderer both *produce* the same semantic reports;
dot-accuracy only refines *when* state changes take visible effect, never the shape of
an answer. Every API item below must pass this test.

Corollary: attribution data is **derived, not state**. The hashed/serialized `Vdp` state
is registers + memories + timing counters; render output and attribution reports are
recomputed on demand from a snapshot (free, because snapshots are cheap and rendering a
line is a pure function of latched state).

## 2. The machine model (what the `Vdp` struct owns)

All plain owned data, `Clone` + bincode, inside `System` — per the foundations rules
(no floats, no `HashMap`, no threads).

- `regs[24]` (8-bit) **[settled]**, `vram[0x10000]`, `cram[0x80]` (64×9-bit colors),
  `vsram[0x50]` (40 entries: 20 two-cell columns × 2 planes) **[settled]**.
- **Control-port state:** address/code latches, first/second-write flag, address
  increment (reg 15) **[settled]**; pending-command semantics **[settled — R1]**: no input
  latch (word 1 applies CD1–CD0 + A13–A0 to the live registers immediately; word 2 applies
  CD5–CD2 + A15–A14); a `$8xxx` first word is a register write and never arms the toggle;
  the armed toggle is cleared by the second word, by a **data-port write**, and by a
  **control-port (status) read** (instrument-sourced pin), but **not** by an HV-counter
  read. Data-port read with a half-rewritten read command = hardware lockup (model as a
  documented deterministic stall, never a host hang).
- **Timing FSM:** h/v counters driven per-mclk (granularity C — ratified for the VDP in
  `docs/decisions/2026-06-24-cycle-granularity.md`). NTSC V28: 262 lines/frame, 3420
  mclk/line, 224 active lines **[settled]**. H32 = 256 px / H40 = 320 px **[settled]**.
  H-counter jumps **[settled — R2]**: H32 `0x00–0x93` → `0xE9–0xFF`; H40 `0x00–0xB6` →
  `0xE4–0xFF`; V (NTSC V28) `0x00–0xEA` → `0xE5–0xFF`, vblank flag sets at `0xDF→0xE0`;
  phase anchors: HBlank flag set/clear at H `0x92→0x93`/`0x04→0x05` (H32),
  `0xB2→0xB3`/`0x05→0x06` (H40); V increments at H `0x84→0x85` (H32) / `0xA4→0xA5` (H40).
- **Status word bits:** PAL, DMA busy, FIFO full/empty, VINT pending, sprite overflow,
  sprite collision, odd frame, vblank, hblank **[settled]** — must be coherent at any
  read, which the per-mclk FSM gives for free.
- **FIFO:** 4 entries, modeled as *data* from day 1 (contents serialize), with **coarse
  stall accounting** in Phase 2 and slot-exact timing deferred to Phase 3 **[settled
  policy]**. Slot counts **[settled — R3]**: external slots/line = 16 (H32 active) / 18
  (H40 active) / 167 (H32 blanked) / 205 (H40 blanked); one slot = one VRAM byte (a FIFO
  word to VRAM = 2 slots; CRAM/VSRAM = 1 word/slot). Each FIFO entry carries a copy of the
  code/address registers; reads bypass the FIFO via a pre-cache read buffer; FIFO full
  stalls the 68k via the wait channel.
- **DMA unit:** three modes — 68k→VDP transfer, VRAM fill, VRAM copy **[settled]** —
  with per-line bandwidth budgeting (coarse, Phase 2). Bus interaction **[settled — R4]**:
  68k→VDP takes the whole 68k bus for the transfer (total halt window through the wait
  channel, words flow through the FIFO); fill and copy leave the 68k running (fill data =
  last FIFO entry, mid-fill data-port writes suspend + redirect the fill; copy bypasses the
  FIFO, one byte read + one byte write per slot). Mid-fill control-write variants → pinned
  by experiment in the DMA push.
- **SAT cache:** the VDP caches the Y + size/link half of each sprite entry internally;
  X + tile/attr are fetched from VRAM at render time **[settled]**. Cache-update rules
  **[settled — R5]**: write-through window — every VRAM write (incl. 68k→VRAM DMA) is
  checked against the *current* reg-5 window (base..base+512 H32 / +640 H40) and a Y or
  size/link hit updates the cache; there is **no other refresh path** — changing reg 5
  never invalidates/reloads (stale-cache mixing: cached Y/size/link from the old table +
  VRAM X/tile at the new base — Castlevania Bloodlines relies on it). Evaluation reads only
  the cache; H40 masks reg-5 bit 0. Open (behavioral, sprite-push experiment): DMA
  fill/copy × cache — interim model: fill/copy steps hit the window compare like any write.
- **Interrupts:** VINT (level 6) pending set at line `0xE0`, H≈`0x02` (V28 NTSC), also
  driving the Z80 IRQ — pulse **[settled — R6]**: exactly one line (deassert next line at
  the same H position, ≈228 Z80 clocks; not maskable by any VDP reg; unlatched on the Z80
  side, so a masked Z80 misses the frame's interrupt). HINT (level 4) from the reg-10 line
  counter **[settled — R7]**: reloaded from reg 10 on every vblank line (225–261) and
  immediately on underflow; decremented once per line on lines **0..=224** (line 224
  included — an HINT can fire on the first line after active); reg-10 writes take effect
  at the next reload; reg10=0 fires every line; display-blank has no effect on HINT.
  **Pending/IPL model [settled — R12]** (closes the set_ipl deassert docket item): two
  pending latches (`hint_pending`, `vint_pending`); IPL is combinational —
  `6 if vint_pending && IE0 else 4 if hint_pending && IE1 else 0`; **only the 68k IACK
  cycle clears a latch** (the acknowledged level's flag; the VDP then re-drives IPL from
  the remaining flag). Latches are NOT cleared by status reads, enable clears, frame
  start, or display on/off; clearing an enable only drops IPL, and re-enabling with the
  flag still set re-asserts (Sesame Street Counting Cafe depends on the re-assert with the
  68k's one-instruction latency — instruction-boundary IPL sampling provides it).

**Explicit non-goals until Phase 3 (the deferral ledger):** slot-exact FIFO/VRAM-access
timing, visible CRAM dots, mid-line raster effects, the VDP debug register, 128K-VRAM
mode. **Interlace mode 2** (double-res; Sonic 2's 2P mode) is Phase-2-if-needed — flag
it the moment the Sonic-4 hack's 2P mode matters, else it slides to Phase 3.

## 3. Scanline render semantics

**Latch point:** one per line, at line start: the render of line N is a pure function of
(regs, VSRAM, h-scroll table entries, SAT cache + VRAM) *as of line N's start*. Writes
that land mid-line take effect from line N+1. This is the whole scanline approximation,
stated once — every known divergence from hardware (CRAM dots, mid-line scroll splits)
is this one sentence, and Phase 3 removes it by moving the latch to slot granularity
behind the same API. **Sprite evaluation** for line N nominally happens during line N−1
on hardware **[settled]**; Phase 1 evaluates at line-N start from the SAT cache (same
inputs, earlier visible effect of late SAT writes) and records this in the divergence
ledger. Sonic-2-class content (per-line h-scroll, HINT-driven water palettes — CRAM
writes between lines) is exactly what this model renders correctly.

**Per-line pipeline (semantic order, [settled] unless marked):**
1. Backdrop color (reg 7).
2. Plane B: nametable base (reg 4), h-scroll per mode (reg 11: full / per-cell /
   per-line, from the reg-13 table), v-scroll full or per-2-cell-column from VSRAM.
   Leftmost-column v-scroll quirk in 2-cell mode **[settled — R8, model choice]**: when
   `hscroll % 16 != 0`, the partial left column's v-scroll = `VSRAM[$4C] & VSRAM[$4E]`
   (AND of the two last entries, same value both planes) in H40, and fixed 0 in H32 —
   Eke's hardware-tested rule (Model 2 / 315-5660); cross-revision variance is a
   divergence-ledger entry.
3. Plane A / Window: window (regs 17/18) *replaces* A in its region and does not
   scroll; the "window bug" **[settled — R9, officially documented]**: left-side window +
   plane-A `hscroll & 15 != 0` → the first 2-cell column right of the boundary reuses the
   window's last-column tile fetch (right-side windows never glitch; V-only scroll never
   triggers). Interim sub-tile alignment: the reused tiles read at plane A's fine-scroll
   offset — pinned exactly by golden-frame differential in the planes push.
4. Sprites: link-list walk from sprite 0; per-line limits 20 sprites & 320 px (H40) /
   16 & 256 px (H32); 80/64 total; overflow + collision status bits; x=0 masking
   **[settled — R10, CORRECTED: "masking mode 2" does not exist** (Nemesis tests 7/8
   verify its absence)**]**: one latch — pixel output disables when an x=0 sprite
   (`x & 0x1FF == 0`) is read after a previous sprite read with x≠0 (any line/frame), and
   re-enables at line start (Kabuto's formulation; subsumes Nemesis's
   not-first-on-line rule + the previous-line dot-overflow exception). Mask sprites still
   consume slot + pixel budget; parsing continues.
5. Priority resolution: high-sprite > high-A > high-B > low-sprite > low-A > low-B >
   backdrop.
6. Shadow/highlight (reg 12 bit 3) **[settled — R11, full table]**: default shadow iff
   both planes' priority bits are 0 at the dot (backdrop included; transparent *plane*
   pixels still contribute priority, transparent sprite pixels don't); sprite pixels
   shadowed only if sprite + both planes low-priority; sprite color 14 of any palette is
   never shadowed; sprite palette-3 entries 14/15 are undrawn operators shifting the
   underlying pixel one step (highlight/shadow; shadow+highlight = normal;
   shadow+shadow = shadow); operators are opaque within the sprite layer (flattened
   line-buffer) and act only on the background result; ramps: normal Min→Max, shadow
   Min→½, highlight ½→Max.

Each stage, while producing pixels, also produces the **attribution record** (§4) —
attribution is the same computation, not a parallel implementation that could drift.

## 4. The render-decode introspection API (the differentiator)

Wire form: new `emulator/<op>` methods on the existing bus protocol (Aether JSON-RPC),
same conventions as the current 52; where the current Oracle surface already has an op
(`emulator_get_layer_states`, layer enable/disable, VRAM/CRAM/VSRAM reads), keep those
shapes and *add* the decoded ops alongside. All ops run between deterministic steps
(the determinism firewall) and read a quiesced machine.

- **`render_line_report(line)`** → the latched inputs and evaluation outcomes for one
  line: effective h/v-scroll per plane (post-mode-resolution), window span, and the
  sprite evaluation list — for each SAT index walked: `{index, y, x, size, link,
  outcome: rendered | dropped(line_limit | pixel_budget | masked | offscreen)}`, plus
  the overflow/collision flags for that line.
- **`pixel_attribution(x, y)`** → why this pixel is this color: the winning layer
  (`sprite(index)/plane_a/plane_b/window/backdrop`), nametable-entry address + decoded
  entry (tile index, palette line, flips, priority), color index → CRAM entry → RGB,
  shadow/highlight applied, and the **ordered list of losing candidates** (what each
  lower layer would have shown and why it lost: priority, transparency).
- **`sprites_decoded()`** → the SAT decoded (all 80 entries: position, size, link,
  tile, palette, priority, flips) with a per-entry **cache-divergence flag** (SAT cache
  vs. VRAM disagree — the stale-cache state made visible).
- **`plane_decoded(plane, rect?)`** → decoded nametable grid for A/B/window.
- **`frame_report()`** → per-frame rollup: dropped-sprites-per-line summary, lines with
  overflow/collision, DMA transfers performed (source/dest/length/mode), HINT/VINT
  lines fired.
- **`cram_decoded()` / `cram_diff(snapA, snapB)`** → palettes as RGB + per-entry diff
  between two snapshots ("diff CRAM frame A vs B" from the charter — snapshots are
  cheap, so diffing is snapshot-native, not a special recording mode).
- **`tile_pixels(index)`** → one tile decoded to pixel indices (the VRAM viewer
  primitive; tile × 32 = byte address).

**API stability contract:** these signatures and semantics are the frozen surface.
Phase 3 may add fields (e.g. sub-line timestamps on attribution) but never changes the
meaning of an existing field. This is what "dot-accurate behind the unchanged API" costs
up front — and it is the *only* thing it costs.

## 5. Validation ladder (VDP-specific rungs)

1. **Semantic unit tests** per pipeline stage (scroll resolution, sprite walk + limits,
   priority table, window regions) — table-driven, no framebuffer needed.
2. **Golden frames on `s4.bin`**: framebuffer FNV-1a vs. Oracle (Exodus) at chosen
   checkpoints (title, level, water line in a HINT zone, boss). Measure Exodus's *real*
   behavior first — it is the oracle only where it is actually right.
3. **Attribution invariants** (proptest-style): for every pixel, rendering the winner's
   reported source reproduces the pixel; losing candidates are consistent with the
   priority table; `frame_report` drop counts equal the per-line report sums.
4. **Test ROMs, measured honestly**: Nemesis sprite-masking ROM (record which cases the
   scanline model passes — the SAT-cache cases are the interesting ones), 240p suite
   subset. **VDPFIFOTesting is an explicit Phase-3 non-goal** (charter line — hold it).
5. **Differential vs BlastEm** (behavioral, over the bus): per-frame framebuffer hash +
   the VDP section of the canonical `export_state` on the s4.bin TAS replay.

## 6. Build order (one push each, the proven cadence)

1. **Recon push** — burn down every **[recon]** tag above via docs + test ROMs +
   differential experiments; write the findings doc (mirror the m68000 recon style).
   **DONE 2026-07-16** — `docs/2026-07-16-vdp-recon.md` (R1–R12, incl. the pending-toggle
   BlastEm experiment and the R12 IPL-deassert model).
2. **Timing skeleton** — h/v counter FSM, status bits, HINT/VINT + scheduler events,
   control/data ports, VRAM/CRAM/VSRAM access; gated on HV/status/interrupt tests.
   (This alone lets the 68000 run ROM loops that poll status — the integration pivot's
   natural companion.)
3. **Planes** (B, then A+window) with full scroll semantics + `plane_decoded` +
   attribution for plane pixels.
4. **Sprites** — SAT cache, walk, limits, masking, `sprites_decoded` +
   `render_line_report`.
5. **Priority + shadow/highlight + `pixel_attribution`** end-to-end; golden frames go
   live here.
6. **DMA + FIFO (coarse)** — transfers, fill, copy, per-line budget, 68k stall via the
   deferred-write seam; `frame_report` DMA section.

Each push lands its introspection ops *with* the feature — the API is not a later layer.
