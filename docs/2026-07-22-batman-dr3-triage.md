# DR-3 Batman & Robin triage — post-CD5 re-diagnosis: the "garble" does not reproduce

**Status: TRIAGE (recon), 2026-07-22. Docs only — no code.** The CD5 stale-flag fix (`b05786e`) shifted
Batman's output, so DR-3 ("garbled tiles") was re-diagnosed **fresh** against the current tree, with no
assumption of a connection to the fixed bug. The finding is not what the docket expected: **across a
22-sample no-input sweep of the full ~70-second attract loop plus an input-driven run into real level-1
gameplay, every sampled frame renders a coherent, recognizable scene.** The one sequence that superficially
*looks* garbled (giant dithered blocks, frames ~590–740) is evidenced below to be an intended close-up
camera pan across the intro skyscraper's window-wall. DR-3's remaining risk is content-level (bytes subtly
wrong in a way that still looks plausible) or mid-frame-raster — both invisible to this method — so the doc
ends with a minimal matched-frame A/B request that settles it.

**Method.** The existing `boot_rom` example (no new code) dumped VRAM/CRAM/VSRAM/RAM/regs + a settled-frame
PPM at 22 frame counts (120–4200); the existing `motion_run` example drove a scripted Start/Right/C/B pad
into gameplay. A throwaway Python decoder in the session scratchpad (never in the repo) independently
re-rendered planes from the dumps to cross-check the render path. Tree untouched; all dumps live in the
session scratchpad (`…/scratchpad/batman/`). ROM: the user's on-disk copy (never copied/committed).

Items **B1–B7**. Each states the finding, its evidence, and its confidence.

---

## B1 — Batman runs a healthy, VInt-synced main loop

**PINNED.** The recurring end-of-frame PC pair `$9C10`/`$9C14` — the same PC the overseer measured post-fix —
is a plain vblank-flag wait: ROM bytes at `$9C10` are `4A 38 F6 FA 67 FA` = `tst.b ($FFF6FA).w` /
`beq.s $9C10`, with the flag cleared at `$9C02` (`42 38 F6 FA`) each pass. Every frame dump from f560 to
f4200 lands in this loop with SR `$2004`. The game is frame-locked and healthy, not wedged. **Evidence**:
ROM bytes quoted above; `regs.txt` PC/SR across all captures.

## B2 — The full attract loop renders coherent scenes at every sample — no gross garble post-fix

**PINNED (as far as settled-frame rendering can see).** No-input sweep, 22 samples: f120 SEGA logo → f300
DC-Comics copyright text → f400–f550 police-van/bank-doors intro shots → f760 3-D perspective wall fly-by →
f800 skyscraper against orange cloud sky → f1000/f1100 Batman & Robin atop the tower → f1200 title fade-in
(cat eyes + "the adventures of") → f1500 full title logo → f1800 moon-over-bridge level intro → f2400 Joker
4-panel story screens → f3000 bat-signal → f3600 asylum wall → f4200 loop back to copyright. Every one is a
recognizable, internally consistent picture: correct text, correct multi-palette art, correct sprite scenes.
**Evidence**: the PPM/PNG set in the scratchpad; nametable/CRAM decodes (B4) agree with the pixels.

## B3 — The one garble-lookalike (f≈590–740) is an *animating close-up window-wall pan*, not tile salad

**HIGH CONFIDENCE (the A/B target).** Frames ~590–740 show screen-filling dithered teal blocks with pink
stripes — exactly the kind of frame a sparse sweep would tag "garbled tiles". The state dumps say otherwise:

- **Scene geometry is deliberate.** VDP planes are 128×32 (`r16=$03`), A=`$C000`, B=`$E000`, hscroll table
  `$BC00`, full-screen scroll (`r11=$00`). Plane A's nametable has window-frame *structure*, not noise: row 0
  = `054A 034B 055C 0528 0528 … 0528 055E` — a left-edge tile, a repeated glass tile, a right-edge tile,
  repeating per window bay (79 distinct tiles, all pal 0).
- **The backdrop for the next shot is pre-staged behind it.** Plane B at f650 decodes to a coherent orange
  cloud sky (pal 3) — the exact sky the f800 skyscraper reveal then uses. Garbage uploads don't pre-stage the
  next scene's backdrop.
- **It animates like a camera move.** Plane-B vscroll ramps monotonically ~5 px/frame through the shot
  (VSRAM[1]: `$075D`@f560 → `$070B`@f580 ‖ cut ‖ `$0316`@f590 → `$0219`@f650 → `$0121`@f700 → `$0058`@f760),
  and per-frame VRAM deltas are confined to the plane-A nametable (`$C000–$DFFF`: 1,440 B f600→650, 1,287 B
  f650→700, 1,056 B f700→720) — tile-scroll streaming, with an art reload only at the f570→f580 shot cut
  (blocks `$4000–$6000`).
- **It's continuous with correct neighbors.** f560 (bank doors, coherent) zooms into it; f760 (coherent 3-D
  perspective wall with the same teal glass + pink mullions) comes out of it; f800 reveals the full tower.

Colors, palette, and geometry all belong to the surrounding — visibly correct — skyscraper sequence. The
remaining doubt (are the *streamed art bytes* exactly right?) is precisely what the comparison request below
settles. **Evidence**: `analyze.py`/`planes.py` decodes of `f650.*` dumps; VRAM byte-diffs across f560–f760.

## B4 — The render path is self-consistent with VRAM: an independent decode reproduces the composite pixel-exactly

**PINNED.** A clean-room Python decode of the f650 dumps (nametable → tile → CRAM via the pinned
`step*255/14` ramp) matches the tool's composite at every sampled point, including the suspicious black left
column: screen row 60 x0–6 = pink `(255,182,218)` = pal0[9] `$0CAE`, x7+ = black — and plane A's art there
is colour **15** = pal0[15] `$0000`, i.e. **opaque black content, not a transparency/render hole** (plane B
behind holds orange sky that is *correctly* occluded). Horizontal-offset correlation across 4 rows picks
shift 0 (hscroll `$FC00` & `$3FF` = 0, decoded correctly). **Evidence**: pixel-triples quoted from
`f650.ppm` vs nibble decode of `$C000` nametable + tile art.

## B5 — Input path works; gameplay itself renders clean (with one operator trap)

**PINNED.** Driving `motion_run` (sole input path `System::set_pad`): Start at the title opens OPTIONS,
double-Start reaches the player-select screen (Batman/Robin art, correct), the Joker birthday-cake cutscene
plays, and **level-1 combat renders coherently**: HUD (health wheel, "CREDIT 00" / "PRESS START" P2 prompt),
Batman sprite, three clown enemies, trash cans, multi-layer street background scrolling right under held-R —
all correct at f2800/f3300. *Trap*: a Start press landing after level start **pauses** the game — an earlier
run misread that as "input dead". **Evidence**: `walk2.f1900/f2300/f2800/f3300` captures.

## B6 — Verdict: DR-3 "garbled tiles" is not reproducible on the post-CD5 tree — reclassify

**HIGH CONFIDENCE.** Pre-fix, Batman garbled and its trajectory differed (f1200 was PC `$12D38`/0 px;
now f1200 = title fade-in at the healthy `$9C14` wait). Batman streams VRAM heavily every frame (B3), so the
pre-fix stale-CD5 spurious-DMA bug — which corrupted exactly this kind of control-port traffic — is the
natural explainer for the old garble, and its fix the natural explainer for the recovery. On today's tree the
evidence supports: **the CD5 fix fixed Batman too.** DR-3 should move from "OPEN — render thread" to
**"needs reference confirmation, presumed fixed by `b05786e`"** pending the A/B below. What this method
*cannot* rule out: byte-level art/CRAM divergence that still looks plausible, and mid-frame raster effects
(the settled-state renderer replays one end-of-frame state for all 224 lines, so per-line hscroll/CRAM
tricks are invisible — PPM-level judgments in raster-heavy scenes are unreliable in both directions; the
byte dumps are the ground truth).

## B7 — Open nits for the overseer

- **Pixel-count datum mismatch**: our f1200 non-black count = **2,005** (PC `$9C14` matches); the fix-outcome
  note recorded ~4,682. Same PC, same deterministic seed — likely a counting-method difference (threshold /
  backdrop handling), worth a one-line reconciliation, not a re-run.
- **CRAM filler**: at f650, pal3 entries 7–15 all read `$0E0E` (classic filler magenta). No on-screen pixel
  references them at that frame (sky uses pal3 1–6 and is occluded anyway) — benign if reference matches;
  the CRAM diff below catches it for free.
- Sprite table sits at `$0000` with one degenerate 1×1 link-0 sprite during the cutscene — normal
  "sprites parked" state, listed only so the A/B reader doesn't trip on it.

---

## Comparison request (the overseer's matched-frame A/B — minimal, two asks)

**Ask 1 — the decisive one (settles B3/B6): frame 650 from reset, no input.** From the reference emulator,
dump at end of frame 650: **full VRAM (64 KB), CRAM (128 B), VSRAM (80 B), VDP regs, PC** — and byte-diff
against our `f650.vram.bin` / `f650.cram.bin` / `f650.vsram.bin` (scratchpad `…/scratchpad/batman/`).
Specifically:

- **Plane A nametable `$C000–$DFFF`** and **art `$4000–$BFFF`** — if these match (mod alignment), the
  window-wall shot is confirmed intended and DR-3 closes as fixed-by-`b05786e`.
- **CRAM pal0/pal3** — catches the `$0E0E` filler question (B7) for free.

*Frame-alignment aid*: if reference frame counting differs from ours, don't bisect — **VSRAM entry 1
(plane-B vscroll) ramps monotonically ≈ −5 px/frame through this shot**; pick the reference frame where
VSRAM[1] = `$0219` (ours at f650) and note the offset. A ±2-frame residual shows up as a small uniform
nametable/vscroll delta, not art differences.

**Ask 2 — free static anchor: frame 300 (copyright screen).** The scene is static for dozens of frames, so
alignment is a non-issue: VRAM + CRAM should byte-match **exactly**. A clean Ask-2 with a dirty Ask-1 would
localize any divergence to the streaming path; both clean closes DR-3.

## Reproduction

```
# Captures (all output to the session scratchpad, never the repo):
cargo build --release --example boot_rom -p oracle-core
./target/release/examples/boot_rom "<Batman ROM>" 650  <scratch>/f650      # + 120,300,…,4200
# Gameplay (input) run:
cargo build --release --example motion_run -p oracle-core
#   script: 1500-1505 S / 1700-1705 S / 2000-3300 R / 2400-2404 C / 3000-3004 C
./target/release/examples/motion_run "<Batman ROM>" walk2_script.txt 1900,2300,2800,3300 <scratch>/walk2
# Key decodes: $9C10 = 4A 38 F6 FA 67 FA (tst.b $FFF6FA / beq.s); f650 regs: r2=$30 r4=$07
#   r13=$2F r16=$03 r11=$00; VSRAM[1] ramp $075D→$0058 over f560→f760; per-frame VRAM deltas
#   confined to $C000–$DFFF during the shot. Analysis scripts were scratchpad-only Python
#   (nametable/art/CRAM decode + plane renders); nothing added to or modified in the repo.
```

## Sources

- Primary: oracle-next's own `boot_rom`/`motion_run` dumps of the user's Batman & Robin ROM (this tree,
  post-`b05786e`); ROM byte inspection for the `$9C10` loop. No emulator source consulted (clean-room).
- `docs/2026-07-22-vdp-dma-cd5-recon.md` (Part 5 + fix-outcome addendum — the pre-fix Batman datum),
  `docs/2026-07-22-differential-rom-findings.md` (DR-3 row), `docs/2026-07-16-vdp-recon.md` (R11.5 ramp,
  nametable/scroll pins used by the clean-room decoder).
