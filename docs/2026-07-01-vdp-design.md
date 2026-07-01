# VDP design brief — scanline-first model + the render-decode introspection API

**Status: PROPOSED 2026-07-01 (Fable), for owner ratification before any VDP code.**
Closes finding 1 of `docs/2026-07-01-plan-audit.md`: the VDP is the declared #1 schedule
risk and the render-decode introspection API is the product differentiator — both need
foundations-grade design *before* the scanline renderer exists, because Phase 3 promises
to upgrade scanline→dot-accurate **behind this unchanged API**.

Facts below are stated at two confidence levels: **[settled]** (safe to build on) and
**[recon]** (verify during the VDP recon push — via official Sega docs, Plutiedev,
SpritesMind, and *behavioral* test-ROM/differential experiments only; BlastEm and
jgenesis source are study-only GPL and stay closed per the clean-room rule).

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
  increment (reg 15) **[settled]**; pending-command semantics on interleaved
  control/data access **[recon]**.
- **Timing FSM:** h/v counters driven per-mclk (granularity C — ratified for the VDP in
  `docs/decisions/2026-06-24-cycle-granularity.md`). NTSC V28: 262 lines/frame, 3420
  mclk/line, 224 active lines **[settled]**. H32 = 256 px / H40 = 320 px **[settled]**.
  The h-counter's mid-line jump values (readable via the HV counter port) **[recon]**.
- **Status word bits:** PAL, DMA busy, FIFO full/empty, VINT pending, sprite overflow,
  sprite collision, odd frame, vblank, hblank **[settled]** — must be coherent at any
  read, which the per-mclk FSM gives for free.
- **FIFO:** 4 entries, modeled as *data* from day 1 (contents serialize), with **coarse
  stall accounting** in Phase 2 and slot-exact timing deferred to Phase 3 **[settled
  policy]**. Slot counts per line type (active H40/H32, vblank, display-off) **[recon]**.
- **DMA unit:** three modes — 68k→VDP transfer, VRAM fill, VRAM copy **[settled]** —
  with per-line bandwidth budgeting (coarse, Phase 2) and the 68k bus-stall interaction
  crossing the deferred-write seam **[recon]**.
- **SAT cache:** the VDP caches the Y + size/link half of each sprite entry internally;
  X + tile/attr are fetched from VRAM at render time **[settled]**. Exact cache-update
  rules (writes landing in the SAT region vs. a moved SAT base — the stale-cache effect
  the Nemesis masking ROM exercises) **[recon]**.
- **Interrupts:** VINT (level 6) at vblank start, also driving the Z80 IRQ (pulse width
  ≈ one line **[recon]**); HINT (level 4) from the reg-10 line counter — reloaded during
  vblank, decremented per active line, fires on underflow **[settled]**, exact
  reload/edge lines **[recon]**.

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
   Leftmost-column v-scroll quirk in 2-cell mode **[recon]**.
3. Plane A / Window: window (regs 17/18) *replaces* A in its region and does not
   scroll; the plane-A fetch anomaly at the window boundary when h-scrolled ("window
   bug") **[recon]**.
4. Sprites: link-list walk from sprite 0; per-line limits 20 sprites & 320 px (H40) /
   16 & 256 px (H32); 80/64 total; overflow + collision status bits; x=0 masking (two
   modes, per the Nemesis masking test — exact rules **[recon]**).
5. Priority resolution: high-sprite > high-A > high-B > low-sprite > low-A > low-B >
   backdrop.
6. Shadow/highlight (reg 12 bit 3): low-priority-plane shadowing + sprite palette-3
   entries 14/15 as operators; the exact resolution table **[recon]**.

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
