# VDP push 5 plan: the output stage — priority ordering, shadow/highlight, `pixel_attribution`, golden frames

> **For agentic workers:** implement this plan slice-by-slice (superpowers:executing-plans). Each slice is
> one gated commit: TDD, full gate, fmt + clippy `--all-targets -D warnings`, one commit per slice.

**Status: PLANNED 2026-07-16.** The fifth VDP push (design brief `docs/2026-07-01-vdp-design.md` §3 steps 5–6
+ §4, build-order item 5): the **per-pixel priority resolution** (RR9, replacing push 3/4's transparency-only
compositing), **shadow/highlight** (R11, fully pinned), **`pixel_attribution`** (§4 — "which layer won and
why", derived from the same resolve path), and **golden frames going live** (fixture scenes → pinned
framebuffer hashes, plus the pixel-level known-differences ledger). Also: wiring `render_scanline` into the
`Scanline` event so the sprite overflow/collision status flags evolve during normal runs, and a bounded
BlastEm frame-capture feasibility spike.

**Goal.** Turn the transparency-only compositor into the real Genesis output stage — correct inter-layer
priority + shadow/highlight per pixel — with the attribution API and a self-consistent golden-frame regression
harness that pins every accumulated interim model.

**Architecture.** Everything in this push is **derived, not state** (design §1): all new logic is pure
`impl Vdp` methods + value types in `crates/oracle-core/src/render.rs`, reading latched state through the
existing public accessors. **No new serialized `Vdp` field is added** — so the two frozen currencies are
byte-identical by construction for slices 1–6 and *proven* at the one slice that touches `system.rs`. The
render pipeline is refactored so `render_line` / `render_line_report` / `pixel_attribution` / the golden frames
all derive from one `resolve_dot` — attribution **is** the render (design §1, the push-3 invariant).

**Tech stack.** Rust (`oracle-core`), integer-only arithmetic (no floats — foundations rule), FNV-1a for the
frame hashes (the same hash family the export golden uses), PPM for the eyeball artifact.

---

## Scope guard

**In:** RR9 priority ordering (the real per-pixel layer resolution: high-sprite > high-A > high-B > low-sprite
> low-A > low-B > backdrop, transparent pixels skipped); R11 shadow/highlight (default state from the planes'
priority bits, sprite shadow rules + the color-14 quirk, palette-3 operators, sprite-layer flattening, the
pinned integer intensity ramps); `pixel_attribution(x, y)` with the ordered losing-candidate list (design §4);
a golden-frame regression harness (fixture scenes → pinned hashes as tests) + the pixel known-differences
ledger (the frame-level analogue of `known_differences.py`); wiring `render_scanline` into the `Scanline`
event; a **timeboxed** BlastEm frame-capture spike (findings either way, not a gate); a `frame_dump` update
showing priority layering + shadow/highlight in one frame.

**Out (later pushes):** DMA + FIFO + `frame_report`'s DMA section (push 6); the Aether JSON-RPC wire wrapping
of the introspection ops (the Oracle-parity op work); s4.bin/Exodus golden frames (validation-ladder rung 2 —
needs a real ROM, later); DAC-calibrated RGB (R11 deferred remainder — our introspection reports CRAM values +
our fixed integer ramp). The CPU core `m68000/*` is **frozen** and the two frozen currencies stay
**byte-identical**.

## The load-bearing invariant: currency neutrality is *stronger* than push 4's

Push 4 added `Vdp` fields (the SAT cache) and had to prove neutrality per-slice. **Push 5 adds no serialized
state at all** — priority, shadow/highlight, and attribution are pure functions of `regs / vram / cram / vsram`
+ the already-serialized push-4 sprite fields + the line number. Therefore:

- **Slices 1–5 touch only `render.rs`** (pure `impl Vdp` + value types) and never `vdp.rs`, `state_hash.rs`,
  `system.rs`, `export_state`, the golden constant, or the SST harness. `git diff -- crates/oracle-core/src/vdp.rs`
  = **0 lines** across those slices; `state_hash` and the export golden `0x22F80ECF29ED3AD4` are byte-identical
  trivially.
- **Slice 6 (the `Scanline` wiring) touches `system.rs` only.** It makes `sprite_overflow` / `sprite_collision`
  / `sprite_dot_overflow_carry` evolve during `run_frames`. **None of those three is in either frozen
  currency** (`state_hash` hashes `vram/cram/vsram/regs`; `export_state` serializes `version → m68k regs →
  work RAM → Z80 RAM → Z80 regs → (VRAM+CRAM+VSRAM+regs) → FM → PSG` — the sprite flags/carry are in neither),
  **and the testrom drives no sprites** (empty SAT → the walk finds nothing on-line → overflow/collision stay
  `false`, carry stays `false`), so the status word is byte-identical and work RAM is unchanged. The export
  golden and every `state_hash` fingerprint hold **byte-identical**. This is proven the standard way: the
  isolated slice-6 commit shows `export_state_v1` + `oracle_differential` + `determinism_gate` green with the
  existing constants. **No golden regeneration anywhere in this push.**

**If any commit moves either hash, stop** — a currency leak, not a rebase. (There is no legitimate reason for a
hash to move this push; there is no new serialized field and no serialization-order change.)

## Ground rules (verifier-enforced, unchanged)

- SST threshold exactly `ran >= 1_000_058`; harness untouched (`FlatBus` has no VDP; the renderer is never on
  the SST path). `m68000/*` diff = **0 lines** across the whole push.
- Determinism gate + proptests + `export_state_v1` golden + `oracle_differential` green at **every** slice;
  every commit fmt-clean; clippy `--all-targets -D warnings` (examples included); conventional commits, no
  co-author trailer.
- Clean-room absolute: behavior enters only from the pinned recon (R11 in `docs/2026-07-16-vdp-recon.md`, RR9
  added this push to `docs/2026-07-16-vdp-render-recon.md`, the ratified design brief) — **never emulator
  source**. BlastEm appears only as a black-box screenshot instrument in the spike.
- **No floats anywhere** — the shadow/highlight intensity ramps are integer (a shared 0..14 quantization).
- The SST sweep is ~400–900 s — background it; if `m68000/*` stays zero-diff, run SST at **key commits + HEAD**
  (the push-4 precedent), stating which trees it ran on.

---

## Recon: RR9 — inter-layer priority resolution (pinned this push, recon-lite)

The design brief (§3 step 5) states the order but the render-recon doc (RR1–RR8) never wrote it down as a
citable pin. **Slice 1's first commit adds RR9 to `docs/2026-07-16-vdp-render-recon.md`**, before the resolver:

> **RR9 — Inter-layer priority resolution (§3 step 5). PINNED.** At each dot the displayed layer is the first
> present in this fixed order: **high-priority sprite > high-priority plane A > high-priority plane B >
> low-priority sprite > low-priority plane A > low-priority plane B > backdrop.**
> - "high-priority" = the layer's priority bit set (plane cell bit 15 / sprite attribute bit 15).
> - Only **opaque** pixels (colour nibble ≠ 0) are candidates; a transparent pixel loses "by transparency" and
>   the next layer is considered. The **backdrop** (reg $07 & 0x3F) is the always-present floor.
> - **Window replaces plane A in its region** — the window pixel occupies plane A's priority slot, carrying the
>   window cell's own priority bit.
> - Sprite-vs-sprite is already resolved (first-come-wins in the SAT line buffer, RR8) *before* this step —
>   the sprite layer is a single flattened pixel per dot.
> - **Evidence:** Plutiedev "Priority" (plutiedev.com/vdp-primer — the sprite-high/plane-high/sprite-low/
>   plane-low ladder), Sega Genesis Software Manual layer-priority section, design brief §3 step 5 (ratified).
>   **Confidence:** high (the canonical, universally-documented Mega Drive order). **Classification:**
>   behavioral. **Open remainder:** none for the ordering — shadow/highlight is R11; sprite-vs-sprite is RR8.

R11 is already fully pinned in `docs/2026-07-16-vdp-recon.md` (§R11, high confidence) — no new R11 recon is
needed; the plan restates its algorithm below for the implementer.

---

## Design

### The pipeline refactor: one `resolve_dot`, three consumers

Today `resolve_line` composites by transparency and returns per-pixel `PixelResolution` (the winner) + the
`SpriteLine`. Push 5 replaces the transparency compositing with priority resolution + shadow/highlight, and
adds attribution — but keeps the **single-source** guarantee. The new internal shape:

```text
resolve_line(line) -> ResolvedLine {
    pixels: Vec<PixelResolution>,   // the winner projection (drives render_line + the report)
    dots:   Vec<DotResolution>,     // the full per-dot attribution (drives pixel_attribution)
    sprite: SpriteLine,             // unchanged (push 4)
}
```

`resolve_dot(line, x, ...)` computes one dot's full resolution (candidates in RR9 order, the winner, and the
shadow/highlight state), and `resolve_line` calls it per pixel. `render_line` maps `winner.cram_index` +
`winner.state` → RGB; `render_line_report` reads `pixels`; `pixel_attribution(x, y)` reads `dots[x]`. There is
no parallel path — attribution is the same computation (design §1).

### Value-type changes (all derived; none serialize) — `render.rs`

```rust
/// Per-pixel shadow/highlight state (R11). Normal == the plain ramp; the enabled S/H modes shift intensity.
pub enum PixelState { Shadow, Normal, Highlight }

/// PixelResolution gains the S/H state (additive — §4 stability contract: existing fields unchanged).
pub struct PixelResolution {
    pub x: u16, pub layer: Layer, pub cram_index: u8,
    pub tile: u16, pub palette: u8, pub priority: bool,
    pub state: PixelState,      // NEW: the resolved shadow/highlight state
}

/// One candidate layer at a dot, in RR9 order, with why it won or lost (design §4 losing-candidate list).
pub struct Candidate {
    pub layer: Layer,
    pub opaque: bool,           // did this layer have an opaque pixel at the dot
    pub priority: bool,         // the layer's priority bit
    pub cram_index: u8,         // what it would have shown
    pub verdict: CandidateVerdict,
}
pub enum CandidateVerdict { Won, LostToPriority, Transparent }

/// The full per-dot resolution (design §4 pixel_attribution). Winner + ordered candidates + S/H state.
pub struct PixelAttribution {
    pub x: u16, pub y: u16,
    pub winner: Layer,
    pub cram_index: u8,         // the winner's CRAM index (post-operator, if any)
    pub rgb: (u8, u8, u8),      // cram_index decoded at the resolved state
    pub state: PixelState,
    pub cell: Option<Cell>,     // the winning plane/window cell (None for backdrop/sprite-flattened)
    pub candidates: Vec<Candidate>,  // RR9 order; the winner is Won, lower opaque layers LostToPriority,
                                     // transparent layers Transparent
}
```

`SpritePixel` (internal) gains `nibble: u8` so the resolver can detect **operators** (palette 3 & nibble ∈
{14, 15}) and the **colour-14-never-shadowed** quirk (nibble == 14, any palette).

### Priority resolution (RR9) — replaces transparency compositing

For each dot, gather the four physical candidates with their opacity + priority + CRAM index:

- **Sprite:** the flattened `SpriteLine.buffer[x]` (already opaque-only, first-come-wins). Its `priority` bit,
  its `cram_index`, and its `nibble`/`palette` (for operator/colour-14 detection).
- **Plane A / Window:** the window pixel (in the window span) *or* the plane-A pixel (with the R9 reuse) — the
  A-slot. `plane_pixel` already returns `PlanePixel { nibble, palette, priority, tile }` even when transparent,
  so the priority bit is available for the S/H default-state calc regardless of opacity.
- **Plane B:** `plane_pixel(Plane::B, …)`.
- **Backdrop:** always opaque, priority `false`, `cram_index = reg $07 & 0x3F`.

Winner = the first present in the RR9 order (high-sprite, high-A, high-B, low-sprite, low-A, low-B, backdrop),
where "present" = opaque. **Operators are excluded from being a displayed winner** — see the S/H section: if
the winning sprite pixel is an operator, the winner is recomputed *without the sprite* (planes+backdrop only)
and the operator shifts that pixel's state.

The **display-disable** (reg $01 bit 6 clear) and **leftmost-column-blank** (reg $00 bit 5) output rules are
unchanged from push 3 (backdrop-only / force-leftmost-8-to-backdrop), applied around the new resolver.

### Shadow/highlight (R11) — the full table, every row tested

S/H is gated by **reg $0C bit 3** (the mode enable). When disabled, every pixel is `Normal` (the plain ramp) —
so all pre-push-5 render tests are unaffected. When enabled, per dot:

1. **Sprite-layer flattening first** (R11.4): already done — the SAT line buffer resolved sprites first-come-
   wins, and operators (palette 3, nibble 14/15) count as opaque inside it (they overwrite lower sprites). No
   change; the resolver just needs the winning sprite pixel's `nibble`/`palette`.
2. **Default state** (R11.1): `default = Shadow` iff **both** plane A's and plane B's priority bits are 0 at the
   dot; else `Normal`. **Transparent plane pixels still contribute their tile's priority bit** (the
   Bloodlines/Ranger-X light-ray trick) — so the default uses the A-slot and B priority bits even where those
   planes are transparent. (In the window region the A-slot priority is the window cell's — a consistent
   extension flagged for the golden differential.) The backdrop is shadowed too when `default == Shadow`.
3. **Winner is a plane/window/backdrop pixel** (non-sprite): displayed state = `default`. (Planes render
   colour 14 normally — no plane carve-out.)
4. **Winner is a normal (non-operator) sprite pixel** (R11.2):
   - high-priority sprite → **never shadowed** → `Normal`.
   - low-priority sprite → `Shadow` iff `default == Shadow`, else `Normal`.
   - **Quirk:** a sprite pixel with colour nibble == **14** (any palette) is **never shadowed** → forced
     `Normal` (overrides the low-priority-shadow above). (Palette-3 nibble 14 is an operator, handled next, so
     this quirk bites for palettes 0–2.)
5. **Winner is a sprite operator** (palette 3, nibble 14 = highlight-op / nibble 15 = shadow-op) (R11.3):
   - the operator is **not displayed**. Recompute the winner **without the sprite layer** (planes+backdrop by
     RR9) → the underlying pixel + its `default` state. Then **shift** by the operator:
     - represent states as levels `Shadow = -1, Normal = 0, Highlight = +1`; highlight-op adds +1, shadow-op
       adds −1; **clamp to [−1, +1]**. This realizes exactly: normal+highlight = highlight; normal+shadow =
       shadow; shadow+highlight = normal (undoes); shadow+shadow = shadow (no double-shadow); and the
       symmetric highlight+highlight = highlight (clamp), highlight+shadow = normal.
   - the operator only takes effect **when the sprite layer wins the RR9 resolution** — a low-priority operator
     under an opaque **high-priority plane** loses the priority battle (the plane wins), so it has **no
     effect** (the R11.3 "visible-layer" pin — no special-case needed; it falls out of RR9).

**Intensity ramps** (R11.5, integer, no floats): decode the CRAM word to three 3-bit channel levels (0..7),
then map `(level, state)` through a shared 0..14 quantization:

```rust
fn intensity(level: u8, state: PixelState) -> u8 {
    let step = match state {                 // 0..=14
        PixelState::Shadow    => level,      // 0..7   → Min..½Max
        PixelState::Normal    => level * 2,  // 0..14  → Min..Max   (== the existing ramp3)
        PixelState::Highlight => level + 7,  // 7..14  → ½Max..Max
    };
    (step as u16 * 255 / 14) as u8
}
```

Check: `Normal` = `level*2*255/14` = `level*255/7` = the existing `ramp3` (so S/H-disabled output is
byte-identical). `Shadow`: 0→0, 7→127 (½Max). `Highlight`: 0→127 (½Max), 7→255 (Max). Exactly the pinned
"normal Min→Max, shadow Min→½Max, highlight ½Max→Max". Exact DAC calibration is the R11 deferred remainder
(our introspection reports CRAM values + this fixed ramp — nothing downstream blocks).

### `pixel_attribution(x, y)` (design §4)

`resolve_dot` already produces the candidate list; `pixel_attribution` returns `dots[x]` for line `y`. The
candidates are the physical layers ordered by RR9 rank; the winner's verdict is `Won`, opaque layers ranked
below the winner are `LostToPriority`, transparent layers are `Transparent`. For a plane/window winner the
decoded `Cell` is attached; the RGB is the winner's CRAM index at the resolved S/H state. The **attribution =
render** invariant is a test: `cram_rgb_state(attr.cram_index, attr.state) == render_line(y)[x]` for every
pixel of a fixture.

### Wiring `render_scanline` into the `Scanline` event (design §5 — flags evolve during runs)

`System::deliver_event`'s `Scanline` arm computes `line` and calls `vdp.on_line_start(line)`. Push 5 adds, for
**active lines only** (`line < 224`), `self.vdp.render_scanline(line as u16)` — committing the sprite
overflow/collision latches + the masking carry every rendered line, so a game polling the status word sees the
flags evolve (real games poll them). This is currency-safe (proven above) but proven the standard way:
`export_state_v1` + `oracle_differential` + `determinism_gate` green with the existing constants at the
isolated slice-6 commit. (The `Scanline` chain is a `System`/`MegaDriveBus` construct; the SST uses `FlatBus`
with no scheduler, so SST is structurally unaffected — `m68000/*` diff = 0.)

## Slicing (gated commits, one per slice, full gate each)

Slices 1→5 are strictly ordered (each builds on the prior `resolve_dot`); slice 6 is independent (System
wiring); slice 7 depends on 1–5. Only slice 6 touches `system.rs`; **no slice touches `vdp.rs`,
`state_hash.rs`, the `export_state` layout, the golden constant, the SST harness, or `m68000/*`.**

### Slice 1 — RR9 priority resolution (recon-lite + the resolver)

**Files:** `docs/2026-07-16-vdp-render-recon.md` (add RR9), `crates/oracle-core/src/render.rs`
(`resolve_dot` refactor, RR9 winner selection, `PixelState`/`state` field with everything `Normal` for now),
`crates/oracle-core/src/lib.rs` (re-export `PixelState`).

Content: pin RR9 in the recon doc; refactor `resolve_line` to build candidates + select the RR9 winner
(replacing the transparency overwrite chain); add `PixelState` (all `Normal` this slice — S/H is slice 2/3) and
the `state` field on `PixelResolution`; `render_line` maps `(cram_index, Normal)` via the new `cram_rgb_state`
(== `cram_rgb` for `Normal`). Display-disable + LCB unchanged.

Tests (traced to RR9):
- high-priority plane B wins over an opaque low-priority plane A (rewrites the old push-3
  `priority_bit_is_decoded_but_does_not_reorder`, which asserted the *opposite* — the scope boundary moved;
  disclosed).
- high-priority plane A beats a low-priority sprite (rewrites push-4 `low_priority_sprite_overlays_a_high_
  priority_plane`, which asserted opacity-wins — same boundary move; disclosed).
- high-priority sprite beats a high-priority plane A; low-priority sprite beats a low-priority plane A (the two
  sprite/plane tiers).
- high-A beats high-B; low-A beats low-B; a transparent higher layer is skipped for the next opaque one; all
  transparent → backdrop.
- window occupies the A-slot: a high-priority window pixel beats a low-priority plane B; a low-priority window
  pixel loses to a high-priority plane B.
- the attribution-invariant test still holds (`render_line_report` winners reproduce `render_line`).

Commit: `docs(recon RR9)` folded with `feat(vdp): RR9 per-pixel priority resolution` — **two commits**:
`docs(vdp): RR9 inter-layer priority order recon` then `feat(vdp): RR9 priority resolution replaces
transparency compositing`.

### Slice 2 — shadow/highlight: default state, plane/sprite shadowing, ramps (R11 non-operator)

**Files:** `crates/oracle-core/src/render.rs`.

Content: the S/H enable gate (reg $0C bit 3); compute `default` from both A-slot + B priority bits (transparent
planes contribute); apply the non-operator rules — plane/window/backdrop winner → `default`; high sprite →
`Normal`; low sprite → `Shadow` iff `default == Shadow`; the colour-14-never-shadowed quirk; the integer
`intensity` ramp on `cram_rgb_state`. `SpritePixel` gains `nibble`.

Tests (one per R11 row):
- **S/H disabled** (reg $0C bit 3 = 0): every pixel `Normal`, output byte-identical to a non-S/H fixture.
- **default shadow**: both planes low-priority (opaque) + S/H on → the plane pixel renders `Shadow` (the shadow
  ramp); a **high-priority** plane pixel renders `Normal`.
- **transparent-plane priority contributes**: plane A opaque low-priority wins, plane B **transparent but
  high-priority** → `default == Normal` → the plane-A pixel is `Normal`, not shadowed (light-ray trick).
- **backdrop shadowed**: both planes low + all-transparent dot → backdrop renders `Shadow`.
- **high sprite never shadowed**: high-priority sprite over both-low planes → `Normal`.
- **low sprite shadowed by default**: low-priority sprite, both planes low → `Shadow`; low sprite with a
  high-priority plane present → `Normal`.
- **colour-14 quirk**: a low-priority sprite pixel of nibble 14 (palette 0) over both-low planes → `Normal`
  (never shadowed), while nibble 13 in the same setup → `Shadow`.
- **ramps**: assert the exact RGB of a known CRAM colour under `Normal` / `Shadow` / `Highlight` (`intensity`
  table values), and that `Normal` == the plain `cram_rgb`.

Commit: `feat(vdp): shadow/highlight default state + sprite/plane shadowing + ramps (recon R11)`.

### Slice 3 — shadow/highlight operators (R11 palette-3 14/15)

**Files:** `crates/oracle-core/src/render.rs`.

Content: detect the winning sprite as an operator (palette 3, nibble 14/15); when the sprite operator wins RR9,
recompute the underlying winner (planes+backdrop, no sprite) + its `default` state, then apply the clamped
state shift; operators are not displayed (the underlying CRAM index shows). The under-a-high-plane-no-effect
case falls out of RR9 (the plane wins, the operator never fires).

Tests (one per R11.3 rule):
- highlight-op (palette 3, nibble 14) over a `Normal` background → `Highlight`; over a `Shadow` background →
  `Normal` (shadow+highlight undoes).
- shadow-op (palette 3, nibble 15) over a `Normal` background → `Shadow`; over a `Shadow` background → `Shadow`
  (no double-shadow).
- the operator pixel is **not displayed**: the shown CRAM index is the underlying plane/backdrop's, not the
  operator sprite's.
- **no effect under a high-priority plane**: a low-priority operator with a high-priority opaque plane A → the
  plane A pixel shows at its own `default` state, unshifted (the operator lost RR9).
- a **high-priority** operator does fire over a low-priority plane (it wins the sprite slot).

Commit: `feat(vdp): shadow/highlight sprite operators (recon R11)`.

### Slice 4 — `pixel_attribution` + the ordered candidate list (design §4)

**Files:** `crates/oracle-core/src/render.rs`, `crates/oracle-core/src/lib.rs` (re-export `PixelAttribution` /
`Candidate` / `CandidateVerdict`).

Content: `resolve_dot` retains the RR9 candidate list per dot into `ResolvedLine.dots`;
`pixel_attribution(&self, x, y) -> PixelAttribution` returns `dots[x]` with winner, RGB at the resolved state,
the winning `Cell` (for plane/window winners), and the candidates in RR9 order with verdicts.

Tests (traced to §4):
- **attribution = render**: for a mixed fixture (planes + sprites + S/H), `cram_rgb_state(attr.cram_index,
  attr.state) == render_line(y)[x]` at every x (the design §4 invariant, extended to carry S/H).
- **candidate order + verdicts**: a dot with an opaque high-priority sprite over an opaque low-priority plane A
  over the backdrop → winner sprite `Won`, plane A `LostToPriority`, backdrop `LostToPriority`; a transparent
  plane B in the same dot → `Transparent`.
- **winning cell attached**: a plane-A winner reports the decoded `Cell` (tile/palette/flips/priority); a
  sprite/backdrop winner reports `None`.
- **S/H state reported**: an operator-shifted pixel reports the shifted `state` and the underlying layer.

Commit: `feat(vdp): pixel_attribution — per-pixel winner + losing candidates (design §4)`.

### Slice 5 — golden-frame harness + pixel known-differences ledger

**Files:** `crates/oracle-core/tests/golden_frames.rs` (new integration test), `docs/2026-07-16-vdp-pixel-known-differences.md`
(new ledger), possibly a small `#[doc(hidden)]` fixture-builder in `render.rs` or the test file.

Content:
- A handful of **fixture scenes** (each a `Vdp` built through the real control/data ports, like the render
  unit-test fixtures) exercising the accumulated interim models, each rendered to a full framebuffer
  (`render_line` over the active height) and FNV-1a-hashed; the hashes are **pinned constants** in the test
  (self-consistency — an amendment requires evidence). Scenes:
  1. **priority + S/H**: overlapping high/low planes + sprites + an operator + S/H on (locks RR9 + R11).
  2. **R8 partial column**: 2-cell v-scroll + `hscroll & 15 != 0` (locks the leftmost-column extent).
  3. **R9 window bug**: left window + plane-A fine scroll (locks the sub-tile reuse alignment).
  4. **mode-01/10 h-scroll**: per-line and per-cell h-scroll offsets (locks `(L&7)*4` / `(L&!7)*4`).
  5. **R5 cache window**: byte-granular SAT pokes + an H40 base with bit 0 set (locks the mask + byte writes).
  6. **no-mid-sprite-cut**: a sprite straddling the pixel budget draws fully (locks the push-4 approximation).
- The **pixel known-differences ledger** (`docs/2026-07-16-vdp-pixel-known-differences.md`) — the frame-level
  analogue of `tools/blastem-differential/known_differences.py`: each interim model as a row {id, what,
  why-divergent-by-design, the pin mechanism, confirm-when}. Entries: no-mid-sprite-cut (push-4 must-ledger),
  R5 window-base H40 bit-0 mask, R5 byte-granular SAT pokes, R8 partial-column extent, R9 sub-tile alignment,
  mode-01/10 h-scroll offsets, plane-size `0b10` clamp, S/H DAC calibration. This is what a future cross-
  emulator differential consults so the by-design divergences are attributed, not false-alarmed.
- **Confirm the interim models**: each golden scene's hash is derived from — and pins — the current model; the
  ledger records that these are self-consistency pins today, upgraded to cross-emulator confirmations if the
  BlastEm spike (below) or a later s4.bin golden lands. State this honestly (no overclaim of "confirmed").

Tests: the golden-frame hashes (each scene → its pinned FNV-1a); a test that a trivially-altered fixture
produces a *different* hash (the harness actually discriminates — no constant-folding).

Commit: `test(vdp): golden-frame regression harness + pixel known-differences ledger`.

### Slice 5b — BlastEm frame-capture feasibility spike (bounded, not gated)

**Files:** `tools/blastem-differential/frame_capture_spike.md` (findings), possibly a throwaway script under
`tools/blastem-differential/` (kept only if it works).

Content (timeboxed — one focused session, report either way, do **not** sink the push): investigate whether
BlastEm 0.6.2 under `xvfb-run` can be driven to emit a framebuffer screenshot for a fixture ROM (its
screenshot key/command over the SDL surface, or a headless capture), to upgrade a golden scene from
self-consistency to a cross-emulator confirmation. Record: does it work, the mechanism if so, the blockers if
not, and whether it is worth wiring as a differential instrument later. Clean-room: BlastEm stays a black-box
screenshot producer (no source read). **If it does not yield a usable capture within the timebox, ledger the
finding and move on** — the goldens remain self-consistency pins, which is the honest, sufficient state for
this push.

Commit: `docs(vdp): BlastEm frame-capture spike findings` (or fold into slice 5's commit if empty-handed).

### Slice 6 — wire `render_scanline` into the `Scanline` event (isolated currency proof)

**Files:** `crates/oracle-core/src/system.rs` (the `Scanline` arm of `deliver_event`).

Content: for `line < 224`, call `self.vdp.render_scanline(line as u16)` in the `Scanline` arm so the sprite
status flags + masking carry evolve during `run_frames`. **Isolated commit** — nothing else changes.

Tests (currency proof + behavior):
- **both goldens byte-identical at this commit** (the standard proof): `export_state_v1`, `oracle_differential`
  (`state_hash`), and `determinism_gate` all green with the existing constants — asserted in the run + in-test
  where practical. (The testrom has an empty SAT → flags/carry stay `false` → status word + work RAM
  unchanged.)
- a `System` behavior test: after loading a small **sprite-bearing** ROM (or poking a SAT + running), the
  status word's sprite-overflow / collision bits reflect the rendered lines (the flags now evolve), and reading
  the status word clears them.

Commit: `feat(vdp): drive render_scanline from the Scanline event (sprite flags go live)`.

**Decision surfaced (see below):** wire the full `render_scanline` (matches the owner's brief, obviously
currency-safe, exercised end-to-end) vs a lean sprite-flags-only commit path. Recommendation: the full
`render_scanline` for simplicity + fidelity; the lean path is a deferred perf option.

### Slice 7 — `frame_dump` update (not gated)

**Files:** `crates/oracle-core/examples/frame_dump.rs`.

Content: extend the fixture ROM so one frame shows the new output stage: set reg $0C bit 3 (S/H on), add a
**high-priority** plane cell region + a **low-priority** sprite over it (so the plane wins by RR9 — visible
priority ordering), and a **shadow/highlight operator** sprite (palette 3, colour 14/15) over the stripes (a
visible highlighted/shadowed band). The PPM now shows priority layering + shadow/highlight in one frame — the
picture the owner looks at. Run it, record the output path, confirm it is clippy-clean (`-D warnings` covers
examples).

Commit: `feat(vdp): frame_dump shows priority ordering + shadow/highlight (dev tool)`.

## Anti-cheating / invariants

- **Currency neutrality (headline):** no new serialized `Vdp` field this push. `git diff -- vdp.rs
  state_hash.rs` = 0 across slices 1–5; slice 6 touches only `system.rs` and asserts both goldens
  byte-identical with the existing constants. **No golden regen.** The verifier greps for any new
  `state_hash::compute` input or `export_state` region — there are none.
- **SST:** 112 tests / `ran >= 1_000_058`; `m68000/*` diff empty; harness untouched. Run at key commits + HEAD
  (zero-diff CPU ⇒ invariant), stating which trees.
- **Attribution = render** (design §1): `render_line` / `render_line_report` / `pixel_attribution` all derive
  from the one `resolve_dot`; the §4 invariant test proves the winner (with S/H state) reproduces the pixel.
- **Every S/H table row is a test** (slices 2–3): default state, transparent-plane priority contribution,
  backdrop shadow, high/low sprite shadowing, the colour-14 quirk, both operators, shadow+highlight undo, no
  double-shadow, under-high-plane no-effect, and the three ramps. R11's "every row gets a test" is literal.
- **The two rewritten tests are disclosed** (slice 1): the push-3/4 "priority decoded but not ordered" tests
  asserted the pre-push-5 boundary; push 5 crosses it, so they are rewritten to assert the correct RR9
  ordering. This is the scope boundary moving, not a test weakened to pass — the report names both.
- **Interim models trace to the ledger:** every by-design divergence (no-mid-sprite-cut, R5/R8/R9 remainders,
  mode-01/10 offsets, `0b10` clamp, DAC calibration) is a row in the new pixel known-differences doc + a code
  comment citing it; the golden hashes are self-consistency pins, stated honestly.
- **No floats:** grep the S/H code for `f32`/`f64`/`as f` — none; the ramps are the integer 0..14 quantization.
- **No overclaim:** golden frames are self-consistency until the BlastEm spike / s4.bin golden upgrades them;
  the report says exactly that.

## Risks

- **Rewriting the existing priority tests reads as weakening tests.** Mitigation: the two rewrites are the
  *behavior legitimately changing* at the push-4/5 boundary (opacity-wins → priority-wins); the new assertions
  are strictly stronger (they pin the real order). Both are named in the report with before/after.
- **The S/H operator model is subtle.** Pinned high-confidence (R11) but with a load-bearing "operator not
  displayed → recompute underlying → clamp-shift" mechanism. Mitigation: the clamp-to-[−1,+1] integer model is
  proven against all four combine rows (normal±, shadow±) as explicit tests; the under-high-plane no-effect is
  a test (it must fall out of RR9, not a special case).
- **Golden-frame brittleness.** A hash pins the *entire* framebuffer, so any model refinement re-hashes.
  Mitigation: the scenes are small and each targets one quirk; the ledger explains which model each locks; a
  re-hash is a deliberate, evidenced amendment (never a silent regen), and the discriminator test proves the
  harness isn't constant-folded.
- **The BlastEm spike rabbit-holing.** Mitigation: hard timebox, findings-either-way, the goldens stand as
  self-consistency pins without it (the spike is an *upgrade*, not a dependency).
- **Slice-6 wiring moving a golden.** Mitigation: the empty-SAT testrom keeps the flags `false`; the isolated
  commit proves both goldens byte-identical or the wiring is wrong — fail loud.

## Decisions surfaced (not defaulted)

1. **RR9 order pinned as recon-lite before the resolver.** The design brief states the order but the render-
   recon doc never wrote it as a citable pin; slice 1's first commit adds RR9 (Plutiedev + Sega manual + §3
   step 5). **Recommendation: pin RR9, then build** — the owner's explicit instruction and the clean-room rule
   (behavior enters from a pin, not from code).
2. **Window occupies plane A's priority slot** (RR9 + R11 default-state). The manual talks about "planes A/B";
   the window replaces A. Treating the window cell's priority as the A-slot priority (for both RR9 and the S/H
   default state) is the consistent extension. **Recommendation: A-slot = window in its region**, flagged in
   the ledger for the golden differential (the exotic window-region S/H cells are a ledger row, not a blocker).
3. **The integer S/H ramp** = a shared 0..14 quantization realizing "normal Min→Max, shadow Min→½Max, highlight
   ½Max→Max" with `Normal == ramp3` exactly. **Recommendation: this model** — it is integer (no floats),
   matches the pinned description exactly, and keeps every S/H-disabled test byte-identical; exact DAC levels
   are the R11 deferred remainder (introspection reports CRAM + this ramp).
4. **Wire the full `render_scanline` vs a lean sprite-flags-only path** (slice 6). The owner named
   `render_scanline`; it is obviously currency-safe and exercised end-to-end, but it does a full pixel composite
   + `LineReport` allocation per active line purely for the side-effect flags (~13 k discarded renders per
   60-frame golden run). **Recommendation: wire the full `render_scanline`** (simplest, matches the brief, perf
   is a non-issue at fixture scale); note a `commit_sprite_flags(line)` lean path (sprite walk + commit, no
   composite/allocation) as an available deferred optimization if a future headless throughput consumer cares.
5. **Golden frames are self-consistency pins this push; the BlastEm spike is a bounded upgrade attempt.** The
   validation-ladder rung-2 (s4.bin vs Exodus) needs a real ROM (later); rung-1/3 (semantic + framebuffer self-
   consistency) is what's available now. **Recommendation: ship the self-consistency goldens + the ledger,
   timebox the BlastEm cross-check, and state the confidence level honestly** — do not overclaim "confirmed".
6. **`pixel_attribution` returns the four physical candidates in RR9 rank order** (not all seven priority
   slots) with per-candidate verdicts. **Recommendation: physical layers + verdict** — it is the faithful,
   compact answer to "why this pixel, what each lower layer would have shown and why it lost" (§4) without
   duplicating a layer into two rows for its two priority tiers.

## Introspection API status after this push

| Op | Status |
|---|---|
| `tile_pixels`, `cram_decoded` | live (push 2) |
| `plane_decoded` | live (push 3) |
| `render_line_report` (plane + sprite) | live (push 3–4) |
| `sprites_decoded` (+ cache-divergence) | live (push 4) |
| **`pixel_attribution` (winner + losing candidates + S/H)** | **live (slice 4)** |
| `frame_report`, `cram_diff` | push 6 / later (DMA + snapshot-diff ops) |

Priority ordering + shadow/highlight are now **real** in `render_line` (no longer opacity-only). The Aether
JSON-RPC wire wrapping stays with the Oracle-parity op work (out of scope, noted).
