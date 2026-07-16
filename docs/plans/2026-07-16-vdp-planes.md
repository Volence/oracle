# VDP push 3 plan: planes rendering — the first pixels

> **For agentic workers:** implement this plan task-by-task (superpowers:executing-plans). Each slice is
> one gated commit: TDD, full gate, fmt + clippy `-D`, one commit per slice.

**Status: PLANNED 2026-07-16.** The third VDP push (design brief §6.3,
`docs/2026-07-01-vdp-design.md` §3 steps 1–3): backdrop → plane B → plane A + window, with full h-scroll
(reg-13 table, all modes), v-scroll (VSRAM full / 2-cell incl. the R8 leftmost-column quirk), the R9 window
bug, transparency-based layer compositing, and the `render_line_report` / `plane_decoded` introspection ops for the plane
stages. **This is the first visible output of the project.** Recon is complete: the behavioral quirks are
pinned in `docs/2026-07-16-vdp-recon.md` (R8/R9), and the standard render byte-formats in
`docs/2026-07-16-vdp-render-recon.md` (RR1–RR7). No new recon is needed to build this.

**Scope guard.** In: backdrop (reg 7), plane B, plane A, window (regs 17/18), h-scroll (reg 11 mode / reg 13
table), v-scroll (VSRAM full + 2-cell + R8), the R9 window bug, transparency-based layer compositing, leftmost-column
blank (reg 0 b5), display-enable (reg 1 b6), and the plane-stage introspection (`plane_decoded`,
`render_line_report`). Out (later pushes, per the brief's build order): **sprites** (SAT walk, limits,
masking — R5/R10, push 4); **shadow/highlight** (reg 12 b3 — R11, push 5); `pixel_attribution` /
`sprites_decoded` / `frame_report` / golden frames (push 5); DMA + FIFO (push 6). The CPU core `m68000/*` is
**frozen** and the two frozen currencies are **untouched** (see below).

## The load-bearing invariant: this push is currency-neutral

**Rendering is derived, not state (design §1).** A rendered line is a pure function of latched `Vdp` state +
the line number; **no render-related field serializes.** Concretely: the `struct Vdp` field list is **not
touched** — the renderer is new `impl Vdp` methods + free types in a new module, reading only the existing
state through `&self`. Therefore, throughout the whole push:

- The Oracle `state_hash` is **byte-identical** (its regions — VRAM/CRAM/VSRAM/regs — are unchanged).
- The `export_state` golden **`0x22F80ECF29ED3AD4`** holds **byte-identical at every slice**.
- **No golden is regenerated in this push.** If a diff moves either hash, the derived-not-state design is
  being violated — stop and re-read design §1. The verifier greps `struct Vdp { … }` for field changes and
  runs the golden + `oracle_differential` suites at each slice; both must be green with the **existing**
  constants.

## Ground rules (unchanged, verifier-enforced)

- SST threshold exactly `ran >= 1_000_058`; harness untouched (`FlatBus` has no VDP; the renderer is never on
  the SST path — the CPU core cannot tell it exists). `m68000/*` diff = **0 lines** across the whole push.
- Determinism gate + proptests + golden green at every slice; every commit fmt-clean; clippy
  `--all-targets -D warnings` (examples included); conventional commits, no co-author trailer.
- Clean-room: behavior enters only from the pinned recon facts (`docs/2026-07-16-vdp-recon.md` R8/R9 +
  `docs/2026-07-16-vdp-render-recon.md` RR1–RR7 + the ratified design brief) — never emulator source.
- **No floats anywhere in the renderer** (foundations rule). All geometry is integer/bitwise: plane pixel
  dimensions are powers of two (256/512/1024) so `mod` is a mask and tile addressing is shifts; the intensity
  ramp is the existing integer `ramp3` (`level × 255 / 7`).
- The SST sweep is ~600–900 s — background it or use a long timeout; re-run per slice **and** at HEAD.

## Design

### Where the code lives — a new `crate::render` module (zero `Vdp` field churn)

New file `crates/oracle-core/src/render.rs`; `mod render;` added to `lib.rs`. It holds the render value
types and `impl Vdp { … }` blocks with the rendering + introspection methods. An inherent `impl Vdp` may live
in any module of the defining crate; it reads `Vdp` state through the **existing public accessors**
(`vram()`, `cram()`, `vsram()`, `regs()`) — so `struct Vdp` in `vdp.rs` is **not modified at all** (no new
fields, no field-visibility changes), which is what makes the push currency-neutral by construction. Any
register predicate the renderer needs (e.g. H40) is recomputed locally from `self.regs()` rather than reaching
into private helpers.

> Decision (surfaced below): a separate module rather than growing `vdp.rs` (already 1104 lines) or converting
> it to a directory. The trade-off is the renderer uses `self.regs()[i]` instead of the private `self.regs`;
> the win is `vdp.rs` — the file that owns the frozen currencies — has a **zero diff** this push, so currency
> neutrality is trivially auditable.

### Value types (all derived; none serialize)

```rust
/// Which layer produced a resolved pixel. Sprites join this enum in push 4; S/H is push 5.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer { Backdrop, PlaneB, PlaneA, Window }

/// A decoded plane/window nametable cell (recon RR1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell { pub tile: u16, pub palette: u8, pub hflip: bool, pub vflip: bool, pub priority: bool }

/// One resolved screen pixel + its provenance. Attribution IS the render computation (design §1),
/// so this is produced by the pipeline itself, never recomputed by a parallel path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PixelResolution {
    pub x: u16,
    pub layer: Layer,
    pub cram_index: u8, // 0..=63: winning cell PAL*16 + nibble, or reg7&0x3F for backdrop
    pub tile: u16,      // winning cell's tile index (0 for backdrop)
    pub palette: u8,
    pub priority: bool,
}

/// Post-mode-resolution scroll for one plane on one line (design §4 "effective h/v-scroll per plane").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlaneScroll { pub hscroll: u16, pub vscroll: VScroll }
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VScroll { Full(u16), TwoCell(Vec<u16>) } // TwoCell = per-16px-column values, left→right

/// The window's horizontal span on a line (design §4 "window span").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WindowSpan { pub start_x: u16, pub end_x: u16, pub full_line: bool }

/// The semantic line report for the plane stages (design §4 render_line_report). Push 4 adds the
/// sprite-evaluation list + overflow/collision fields; this push documents that section as absent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LineReport {
    pub line: u16,
    pub h40: bool,
    pub display_enabled: bool,
    pub backdrop: u8,
    pub plane_a: PlaneScroll,
    pub plane_b: PlaneScroll,
    pub window: Option<WindowSpan>,
    pub pixels: Vec<PixelResolution>, // len = active width (256 H32 / 320 H40)
}
```

### The pipeline — one shared `resolve_line`, everything derives from it

Private `fn resolve_line(&self, line: u16) -> LineReport` is the single source of truth; `render_line`
(RGB) and `render_line_report` (public introspection) both wrap it, so **attribution cannot drift from
render**. Steps (design §3 order):

1. **Geometry.** `h40 = regs[0x0C] & 0x81 == 0x81`; `width_px = if h40 {320} else {256}`;
   `backdrop = regs[0x07] & 0x3F` (RR4). Plane size (RR3): `w_cells/h_cells` from reg $10 bits 1–0 / 5–4
   (`0→32,1→64,3→128`; the invalid `2` clamped to 64, flagged); `plane_w_px = w_cells*8`, `plane_h_px`
   likewise (all powers of two).
2. **Display off** (`regs[0x01] & 0x40 == 0`, RR4): every pixel = `{Backdrop, backdrop, …}`; skip planes.
3. **Backdrop fill** (design §3 step 1): initialise all pixels to the backdrop CRAM index.
4. **Plane fetch** (design §3 steps 2–3, shared for A and B), for each screen `x` in `0..width_px`:
   - `hscroll` = plane h-scroll for this line (RR5): table base `(regs[0x0D] & 0x3F) << 10`; mode from
     `regs[0x0B] & 3` → byte offset `{00:0, 01:(line&7)*4, 10:(line&!7)*4, 11:line*4}`; word at
     `base+offset` = Scroll A, `+2` = Scroll B; mask `& 0x3FF`.
   - `vscroll` = plane v-scroll for the 16-px column of `x` (RR6): full (`regs[0x0B]&4==0`) → VSRAM word 0
     (A) / word 1 (B); 2-cell → VSRAM word `(x/16)*2` (A) / `+1` (B); **R8**: if `hscroll & 15 != 0`, the
     leftmost partial column uses `VSRAM[$4C] & VSRAM[$4E]` (H40) / `0` (H32), same value both planes
     (interim extent = the leftmost 16-px column; flagged).
   - `plane_x = (x.wrapping_sub(hscroll)) & (plane_w_px-1)` (RR5 sign: increasing hscroll ⇒ plane right);
     `plane_y = (line + vscroll) & (plane_h_px-1)` (RR6 sign: increasing vscroll ⇒ plane up).
   - Cell word at `base + ((plane_y/8)*w_cells + plane_x/8)*2`; decode (RR1); nibble via the tile fetch
     with flips (`px = plane_x&7 ^ (hflip?7:0)`, `py = plane_y&7 ^ (vflip?7:0)`; same 32-byte layout as
     `tile_pixels`, RR2). `opaque = nibble != 0`.
5. **Window replaces plane A** (design §3 step 3, RR4-adjacent): compute the window region for this line
   (regs $11/$12): if the **vertical** window band covers the line (DOWN=0 ⇒ line `< WVP*8`; DOWN=1 ⇒
   `>= WVP*8`), the **whole line** is window; else the **horizontal** split (RIGT=0 ⇒ cells `[0, WHP*2)`;
   RIGT=1 ⇒ `[WHP*2, w)`) is window, the complement is plane A (union L-shape model, Sega manual §J). Window
   pixels do **not** scroll: `plane_x = x`, `plane_y = line`, base `(regs[0x03] & wd_mask) << 10`
   (`wd_mask = 0x3C` H40 / `0x3E` H32). **R9 bug** (interim, recon R9): if a **left** window is active on the
   line (RIGT=0, not full-line) **and** plane-A `hscroll & 15 != 0`, the first 2-cell (16-px) column of
   plane A right of the boundary reuses the window's last-column tile, sampled at plane A's fine-scroll
   offset — a documented interim, code comment → recon R9, confirm-by-golden-differential.
6. **Layer compositing by transparency** (RR7): per pixel, the first **opaque** (nibble ≠ 0) layer of
   `A/window → B`, else backdrop. Record the winner into `PixelResolution`; the cell's **priority bit is
   decoded and stored** (`.priority`) but does **not** affect ordering — the priority-bit ordering + sprites
   join the full step-5 order in push 5 (owner scope). Fixed layer order A-over-B-over-backdrop.
7. **Leftmost-column blank** (RR4): if `regs[0x00] & 0x20`, force `x in 0..8` to backdrop.

`render_line(&self, line) -> Vec<(u8,u8,u8)>` maps `resolve_line(line).pixels[i].cram_index` through the
CRAM decode (reusing `cram_decoded`'s per-entry integer path). `render_line_report(&self, line)` returns the
`LineReport` directly.

### Introspection ops this push owes (design §4)

- **`plane_decoded(plane, rect?) -> Vec<Cell>`** (slice 1): the decoded nametable grid for A / B / window.
- **`render_line_report(line) -> LineReport`** (slice 4): the plane-stage semantic report — effective h/v
  scroll per plane, window span, per-pixel winner + provenance. The sprite-evaluation list is push 4; this
  push carries no sprite fields and documents the omission (the struct grows additively in push 4, per the
  §4 stability contract).

`pixel_attribution` / `sprites_decoded` / `frame_report` / `cram_diff` land in later pushes (noted so the
API doesn't accrete as "a later layer", per the brief's closing rule). `tile_pixels` / `cram_decoded` already
exist (push 2).

## Slicing (gated commits, one per slice, full gate each)

### Slice 1 — decode primitives + `plane_decoded`

**Files:** create `crates/oracle-core/src/render.rs`; modify `crates/oracle-core/src/lib.rs` (add
`mod render;` + re-export the render types).

Content: `decode_cell(word) -> Cell` (RR1); `tile_nibble(&self, tile, px, py) -> u8` (RR2, the flip-free
byte fetch reused by the pipeline); the geometry helpers `plane_size(reg16) -> (u16,u16)`,
`plane_a_base()/plane_b_base()/window_base()` and `backdrop()` (RR3/RR4) as private `impl Vdp` helpers; the
public `plane_decoded(plane, rect)` introspection op returning the `Cell` grid.

Tests (traced to pins):
- `decode_cell` splits bit 15 / 14–13 / 12 / 11 / 10–0 exactly (RR1).
- `plane_size` table: `0x00→32×32`, `0x01→64×32`, `0x11→128×32`, `0x10→32×64`, `0x11? …`, invalid `2`
  clamped (RR3).
- base addresses on stock Sonic-2 register values: reg 2 `0x30 → 0xC000`, reg 4 `0x07 → 0xE000`, window
  H40 masks WD11 (RR3).
- `plane_decoded` reads the right nametable words for a hand-seeded VRAM grid.

Commit: `feat(vdp): plane/tile decode primitives + plane_decoded (recon RR1/RR3)`.

### Slice 2 — backdrop + plane B with full scroll

**Files:** modify `crates/oracle-core/src/render.rs`.

Content: the h-scroll resolver (all four modes, RR5), the v-scroll resolver (full + 2-cell + R8 partial
column, RR6/R8), the shared per-pixel plane fetch, and `resolve_line` producing **backdrop + plane B only**
(plane A / window treated as fully transparent this slice — a documented stub filled in slice 3);
`render_line` mapping to RGB; display-enable → backdrop (RR4). `PixelResolution` provenance is produced here
(so attribution exists from the start).

Tests (traced to pins):
- h-scroll **full** and **line** modes move a known plane-B tile by the expected pixel count; sign is
  right (increasing hscroll ⇒ plane shifts right — a pixel run relocates rightward) (RR5).
- v-scroll **full** and **2-cell** move a known tile vertically; sign right (RR6).
- **R8**: 2-cell mode + plane-B `hscroll & 15 != 0` ⇒ the leftmost column samples v-offset
  `VSRAM[$4C]&VSRAM[$4E]` (H40) / `0` (H32), not `VSRAM[0]` — hand fixture asserts the leftmost pixel's
  source row (recon R8).
- display-disabled ⇒ whole line is backdrop (RR4).
- a hand-authored VRAM/CRAM/nametable fixture yields an exact expected pixel run (RGB) for a mid-screen
  span (RR1/RR2/RR6).

Commit: `feat(vdp): backdrop + plane B rendering with full scroll (recon RR5/RR6/R8)`.

### Slice 3 — plane A + window + transparency compositing

**Files:** modify `crates/oracle-core/src/render.rs`.

Content: plane A fetch (reuse the slice-2 machinery); the window region computation (regs $11/$12, union
L-shape model); window-replaces-A; the R9 window-bug interim; transparency compositing A-over-B-over-backdrop (RR7); leftmost-column
blank (RR4). Completes `resolve_line`.

Tests (traced to pins):
- window span: left (RIGT=0), right (RIGT=1), top (DOWN=0), bottom (DOWN=1), and the L-shape union; a
  vertical-window line is `full_line` (RR4/Sega §J).
- window pixels come from the window nametable and do **not** scroll (a plane-A hscroll change does not move
  window pixels).
- **R9**: left window + plane-A `hscroll & 15 != 0` ⇒ the first 16-px plane-A column right of the boundary
  shows the window's last-column tile (interim model, recon R9).
- compositing by transparency: an opaque plane-A pixel wins over plane B; a **transparent** plane-A pixel
  shows plane B beneath; backdrop shows through two transparent planes; the priority bit is **decoded into
  `.priority` but does not reorder** (a high-priority B pixel still loses to an opaque low-priority A pixel —
  the push-5 boundary) (RR7).
- **LCB**: reg 0 b5 set ⇒ `x in 0..8` are backdrop regardless of plane content (RR4).

Commit: `feat(vdp): plane A + window + transparency compositing (recon RR7/R9)`.

### Slice 4 — `render_line_report` semantic report

**Files:** modify `crates/oracle-core/src/render.rs`.

Content: `render_line_report(line) -> LineReport` — populate the `PlaneScroll` (effective hscroll +
`VScroll::Full`/`TwoCell`) and `WindowSpan` metadata around the already-computed `pixels`. (The per-pixel
provenance already exists from slices 2–3; this slice adds the plane-level summary + the public op.)

Tests (design §4):
- **attribution invariant**: for every `x`, mapping `report.pixels[x].cram_index` to RGB equals
  `render_line(line)[x]` — the winner's reported source reproduces the pixel (proptest-style over several
  hand fixtures + scroll settings). This is the §4 "rendering the winner's reported source reproduces the
  pixel" rung, at plane granularity.
- reported `plane_a.hscroll` / `plane_b.hscroll` match the resolver; `VScroll::TwoCell` has the right per
  column length (16 H32 / 20 H40); `window` span matches slice-3's computation.
- the report carries no sprite fields (documented) — a comment + a test asserting the plane stages are
  complete without them.

Commit: `feat(vdp): render_line_report for the plane stages (design §4 introspection)`.

### Slice 5 — the frame-dump dev tool (not gated)

**Files:** create `crates/oracle-core/examples/frame_dump.rs`.

Content: a small example (like `microop_perf`) that builds a **hand-authored fixture ROM** (68000 machine
code emitted in Rust, in the `testrom.rs` style: reset vectors + a setup routine that programs the VDP over
the control/data ports — a few registers, a handful of CRAM colours, one or two tiles, a recognisable
nametable pattern, display-enable — then branches to itself), runs `System::run_frames(n)`, and writes the
active display (`vdp().render_line` for lines 0..=223) to a binary PPM (P6) file. Prints the output path.
Usage `cargo run --release --example frame_dump -- [frames] [out.ppm]`.

Not a gate artifact, but `cargo clippy --all-targets -D warnings` covers examples, so it must be clippy-clean
and compile. **This is the project's first visible output** — the fixture must put something clearly
recognisable on screen (e.g. colour bars from CRAM + a checkerboard/smiley tile in the nametable). The
report to the owner includes the output PPM path.

Commit: `feat(vdp): frame_dump example — first rendered frame to PPM (dev tool)`.

Slices 1→4 are strictly ordered (each builds on the prior `resolve_line`); slice 5 depends on 1–4. No slice
touches `vdp.rs`'s `struct Vdp`, `state_hash`, `export_state`, the golden constant, the SST harness, or
`m68000/*`.

## Anti-cheating / invariants

- **Currency neutrality is the headline invariant.** `git diff <base> -- crates/oracle-core/src/vdp.rs`
  shows **no change to `struct Vdp`** (only, at most, an added `pub` read accessor if strictly needed — none
  is expected). The golden `0x22F80ECF29ED3AD4` and the Oracle `state_hash` are asserted byte-identical at
  every slice (run `export_state_v1` + `oracle_differential` + `determinism_gate`). No golden is regenerated.
- **SST**: 112 tests / `ran >= 1_000_058`, re-run per slice and at HEAD; `m68000/*` diff empty (verifier
  greps it). The renderer is never reachable from `FlatBus` / the SST harness.
- **Attribution = render** (design §1): `render_line` and `render_line_report` both derive from the single
  `resolve_line`; slice 4's attribution-invariant test is the compiler-independent proof they can't diverge.
- **Behavioral facts trace to pins**: every rule cites RR1–RR7 or R8/R9 in a code comment; the interim models
  (R8 partial-column extent, R9 sub-tile, invalid plane-size code, the `01`/`10` hscroll offsets) are
  flagged in code pointing at the recon docs, `confirm-by-golden-differential in push 5`.
- **No floats**: grep the renderer for `f32`/`f64`/`as f` — none. The ramp is integer `ramp3`.
- **New types are not serialized**: they are render outputs; none is added to `Vdp`, `snapshot`, or
  `export_state` (a test/inspection confirms `size_of::<Vdp>` and the snapshot length are unchanged).

## Risks

- **Accidentally moving a currency.** Mitigation: the render module never imports `bincode` for `Vdp` and
  never adds a `Vdp` field; slice-by-slice golden + state_hash assertions fail loudly if it does.
- **Scope creep into push 4/5.** Push 3 composites by **transparency only** (A-over-B-over-backdrop); the
  **priority-bit ordering**, **sprites**, and **shadow/highlight** are all explicitly out (decision 1). The
  renderer produces plane pixels only; the compositor has exactly A/window, B, backdrop — no priority
  reorder, no sprite slot.
- **The R8/R9 interim geometry.** Both are pinned behaviorally but have an open sub-pixel remainder (recon
  R8 revision variance / R9 sub-tile alignment). Implemented as documented interim models, flagged for the
  golden-frame differential (push 5); tests assert the pinned observable (which VSRAM entry / which tile),
  not the unpinned exact extent.
- **The fixture ROM.** Hand-assembling 68000 VDP-setup code is fiddly. Mitigation + fallback in decision 5.
- **Renderer perf** (262×320 `PixelResolution`/frame). Not gated; fine for a dev tool. If it ever matters,
  `microop_perf` is the standing instrument and `resolve_line` can be specialised without changing the API.

## Decisions surfaced (not defaulted)

1. **Push 3 composites by transparency only; the priority-bit ordering is OUT (push 5).** The owner's scope
   brief lists "priority / shadow-highlight are pushes 4–5 — do NOT pull them in", so the priority *ordering*
   — including the plane priority bit that lets a high-priority plane B win over a low-priority plane A — is
   **deferred to push 5**, where it joins sprites + S/H in the full step-5 ordering. Push 3 composites the
   three layers in a fixed order by **opacity**: first opaque of `A/window → B`, else backdrop. The priority
   bit **is** decoded into `Cell.priority` / `PixelResolution.priority` and reported (so the introspection
   surface is forward-compatible and push 5 only changes *ordering*, never the report shape), but it does not
   reorder pixels this push. **Recommendation: transparency compositing (RR7), priority-bit ordering deferred.**
   This keeps the compositor to exactly `{A/window, B, backdrop}` with no priority reorder and no sprite slot,
   squarely inside the push-3 boundary the reviewer enforces.
2. **Leftmost-column blank (reg 0 b5) is IN scope** (RR4): a one-line post-pass, cheap, part of correct plane
   output. If the reviewer prefers it with the rest of the output-stage table (push 5), it moves — low cost
   either way. Recommendation: include it now.
3. **R8 partial-column extent + R9 sub-tile alignment + the `01`/`10` hscroll-mode byte offsets + the invalid
   plane-size code `0b10`** are implemented as **deterministic interim models**, each flagged in code at the
   recon doc, to be confirmed by the golden-frame differential in push 5. This mirrors the R9 "pin-by-
   differential-later" instruction and the timing-skeleton's ledgered interim models. Fixtures/tests exercise
   only the fully-pinned paths (full/line hscroll, full/2-cell vscroll) plus the pinned *observable* of the
   interim cases.
4. **The renderer is a separate `crate::render` module using `Vdp`'s public accessors**, not new methods
   inside `vdp.rs` and not a `vdp/` directory split. Rationale: keeps `vdp.rs` (the frozen-currency file) at a
   **zero diff** so currency neutrality is trivially auditable; the cost (public-accessor reads instead of
   private fields) is negligible for a read-only pure function.
5. **The frame-dump fixture is a hand-authored 68000 ROM** (exercises the full CPU→bus→VDP→render path — the
   first real integration of the CPU driving the VDP, and the more valuable demonstration). **Fallback**
   (only if hand-assembly proves too costly for a non-gated dev tool): program the VDP directly via
   `system.vdp_mut().control_write()/data_write()` in the example, still calling `run_frames` and the real
   render path. The report states which was used. Recommendation: ROM.

## Introspection API status after this push

| Op | Status |
|---|---|
| `tile_pixels`, `cram_decoded` | live (push 2) |
| `plane_decoded` | **live (slice 1)** |
| `render_line_report` (plane stages) | **live (slice 4)** |
| `pixel_attribution`, `sprites_decoded`, `frame_report`, `cram_diff` | later pushes (4–5) |

The wire-protocol (Aether JSON-RPC) wrapping of these ops stays with the Oracle-parity op work (out of scope,
noted so the API doesn't accrete as "a later layer").
