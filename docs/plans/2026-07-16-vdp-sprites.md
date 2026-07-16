# VDP push 4 plan: sprites — the SAT cache, the walk, and the masking latch

> **For agentic workers:** implement this plan task-by-task (superpowers:executing-plans). Each slice is
> one gated commit: TDD, full gate, fmt + clippy `-D`, one commit per slice.

**Status: PLANNED 2026-07-16.** The fourth VDP push (design brief §6 build-order item 4,
`docs/2026-07-01-vdp-design.md` §3 step 4 + §4): the **SAT cache** (recon R5), the **sprite evaluation walk**
(link list, per-line limits — R10 / RR8), **render-time X/tile fetch** (RR8), the **R10 x=0 masking latch**,
the **sprite-overflow / collision status bits going real**, and the **sprite half of the introspection
surface** (`sprites_decoded` + the sprite section of `render_line_report` — *"which sprites dropped on line N
and why"*, the product differentiator). Recon is complete: behavioral quirks are pinned in
`docs/2026-07-16-vdp-recon.md` (R5/R10), and the SAT byte format + sprite render geometry in
`docs/2026-07-16-vdp-render-recon.md` (RR8, added this push). No new recon is needed to build this.

**Scope guard.** In: the SAT cache (R5 — serialized `Vdp` state, write-through window, no reg-5-change
invalidation), `sprites_decoded` with the cache-divergence flag, the evaluation walk (link list + termination,
per-line 20/16-sprite + 320/256-px limits, Y-range on-line test), render-time X + tile/attr fetch (RR8),
column-major multi-cell tile addressing + flips, the **R10 x=0 masking latch** (serialized carry state),
sprite-over-plane compositing **by opacity**, sprite-overflow + collision status bits computed by the
pipeline (going real), and the sprite section of `render_line_report`. Out (later pushes, per the brief's
build order): **priority-bit ordering** — high-sprite > high-A > … (push 5, R11 lead-in); **shadow/highlight**
(reg 12 b3 — R11, push 5); `pixel_attribution` / `frame_report` / golden frames (push 5); **DMA + FIFO**
(push 6 — the SAT cache × DMA fill/copy remainder rides with it). The CPU core `m68000/*` is **frozen** and
the two frozen currencies stay **byte-identical** (see below).

## The load-bearing invariant: currency neutrality holds even though this push touches `vdp.rs`

Push 3 kept `vdp.rs` at a zero diff. **Push 4 deliberately modifies `struct Vdp`** — the SAT cache and the
R10 masking carry are **real hardware state** (the stale-cache Bloodlines behavior means the cache is *not*
derivable from VRAM; the dot-overflow carry crosses line/frame boundaries), so they become **serialized
`Vdp` fields that round-trip snapshots.** This is allowed *because both frozen currencies read explicit
regions, not a bincode of the whole struct*:

- **Oracle `state_hash`** (`state_hash.rs`) hashes exactly `vram / cram / vsram / regs` — the new fields are
  in none of them, so all five fingerprints are **byte-identical**.
- **`export_state`** (`system.rs`) serializes exactly `version → m68k regs → work RAM → Z80 RAM → Z80 regs →
  (VRAM+CRAM+VSRAM+regs) → FM → PSG` — again the new `Vdp` fields are in no region, so the golden
  **`0x22F80ECF29ED3AD4`** holds **byte-identical at every slice**. The `export_state_v1` test pins total
  length + per-region offsets + hash, all derived from the region *sizes* (`VRAM_SIZE` etc.), which are
  unchanged.
- The bincode **snapshot** of `Vdp` *does* grow (new fields) — that is fine: it is not a frozen currency, and
  every new field derives `bincode::Encode/Decode` so snapshot→restore→snapshot stays equal (the proptest +
  a per-field round-trip test prove it).

**Prove it the slice-1 way (verifier-enforced):** the commit that adds the fields must show the
`export_state_v1` golden, `oracle_differential` (state_hash), and `determinism_gate` all **green with the
existing constants** — no golden regeneration anywhere in this push. If a diff moves either hash, a new field
leaked into a currency — stop and re-read this section. (The new fields are a **v2 export-currency candidate**,
noted for whenever the cross-backend differential wants the cache in the frame image — not this push.)

## Ground rules (unchanged, verifier-enforced)

- SST threshold exactly `ran >= 1_000_058`; harness untouched (`FlatBus` has no VDP; the renderer + SAT cache
  are never on the SST path — the CPU core cannot tell they exist). `m68000/*` diff = **0 lines** across the
  whole push.
- Determinism gate + proptests + `export_state_v1` golden + `oracle_differential` green at **every** slice;
  every commit fmt-clean; clippy `--all-targets -D warnings` (examples included); conventional commits, no
  co-author trailer.
- Clean-room: behavior enters only from the pinned recon (`docs/2026-07-16-vdp-recon.md` R5/R10 +
  `docs/2026-07-16-vdp-render-recon.md` RR8 + the ratified design brief) — never emulator source.
- **No floats anywhere** (foundations rule). Sprite geometry is integer/bitwise throughout.
- All new serialized fields **round-trip snapshots** (bincode + a round-trip test).
- The SST sweep is ~600–900 s — background it or use a long timeout; re-run per slice **and** at HEAD.

## Design

### What is real state (in `vdp.rs`) vs derived (in `render.rs`)

**New serialized `Vdp` fields (real hardware state, round-trip):**

- `sat_cache: Vec<u8>` — **320 bytes = 80 entries × 4 cached bytes** (Y word + size/link word; R5/RR8). The
  render-fetched half (tile/attr + X) is never cached. Power-on = **all zero** (the real cache is undefined at
  power-on; games write the SAT before enabling display, and our fixtures/tests write it through the port —
  documented interim; not currency-relevant).
- `sprite_dot_overflow_carry: bool` — the R10 masking carry: **did the previously-rendered line end in a
  sprite-pixel (dot / pixel-budget) overflow.** Seeds the next line's masking latch so a first-on-line x=0
  sprite masks (Nemesis's previous-line-dot-overflow exception; the "any line/frame" reach of Kabuto's
  single-latch formulation). Persists across lines *and* frames (line 0 inherits line 261's carry).
  Power-on = `false`.
- `sprite_overflow` / `sprite_collision` already exist (push 2, read-only). **Push 4 makes them real**: the
  pipeline sets them; `control_read_status` clears them (Sega manual — the status read clears both flags).

**Derived, not state (in `render.rs`, pure `impl Vdp` unless noted):** the evaluation walk, X/tile fetch, the
per-line masking output-latch (a within-line working variable seeded from the carry field), collision
detection, the sprite line buffer, the composite, and every introspection report. Rendering stays a **pure
function of latched state + line** (design §1) — see "pure vs committing" below.

### SAT cache write-through (R5, `vdp.rs`)

- `sat_base()` = `(regs[5] & mask) << 9`, `mask = 0x7E` (H40, bit-0 masked) / `0x7F` (H32) — RR8/R5.
- Factor the VRAM byte store into `write_vram_byte(&mut self, addr, byte)`: it stores the byte **and** checks
  the SAT window. Route `write_target`'s VRAM arm (both bytes of the word, honoring the existing odd-address
  byte-swap) through it. This makes the write-through **byte-granular**, which is exactly RR8 open-remainder 2
  (odd-address SAT writes update the cache by construction).
- Window check: `off = addr.wrapping_sub(sat_base())`; entry `e = off / 8`, `byte_in_entry = off % 8`; if
  `byte_in_entry < 4` and `e < entries` (`entries = 80` H40 / `64` H32 — the H32 window is only `base+512`,
  so entries 64–79 never update in H32, a faithful R5 detail) → mirror to `sat_cache[e*4 + byte_in_entry]`.
- **No other refresh path**: changing reg 5 does **not** invalidate or reload (R5 / Bloodlines). Evaluation
  reads only the cache; render fetches X/tile from VRAM at the *current* `sat_base()` slot. The window base
  uses the same H40 bit-0 mask as evaluation (RR8 open-remainder 1, interim).
- `vram_mut()` stays a raw poke (no cache update) — documented: it is a test/`System` backdoor with no
  hardware analogue; exercise the cache through the **data port** (the real write path). Sprite tests write
  the SAT via `setup_write`/`data_write` so the cache updates through `write_vram_byte`.

### Sprite value types (all derived; none serialize) — `render.rs`

```rust
/// A decoded SAT entry (RR8). Y/size/link come from the SAT cache; X/tile/attr from VRAM at read time.
pub struct SpriteDecoded {
    pub index: u8,          // SAT index 0..=79
    pub y: i16,             // screen Y = (Yfield & 0x3FF) - 128
    pub x: i16,             // screen X = (Xfield & 0x1FF) - 128
    pub width_cells: u8,    // 1..=4
    pub height_cells: u8,   // 1..=4
    pub link: u8,           // 0..=127
    pub tile: u16, pub palette: u8, pub hflip: bool, pub vflip: bool, pub priority: bool,
    pub cache_divergence: bool, // SAT cache Y/size/link disagree with VRAM at the current base (R5 stale)
}

/// Per-sprite outcome on one line (design §4 `render_line_report`, brief "why each sprite dropped").
pub enum SpriteOutcome {
    Rendered,             // on-line, within limits, output not masked
    OffLine,              // parsed but this line is outside the sprite's Y span (design "offscreen")
    DroppedLineLimit,     // on-line but beyond the per-line sprite count (20 H40 / 16 H32) — "limit"
    DroppedPixelBudget,   // on-line but the per-line pixel budget (320/256) was exhausted (dot overflow)
    Masked,               // on-line, in budget, but R10 x=0 masking suppressed its pixel output — "masking"
}

/// One walked sprite's evaluation outcome (in link-walk order).
pub struct SpriteEval {
    pub index: u8, pub y: i16, pub x: i16,
    pub width_cells: u8, pub height_cells: u8, pub link: u8,
    pub outcome: SpriteOutcome,
}

/// Why the link walk ended (the brief's "link-cut" reason — sprites past this are unreachable).
pub enum SpriteWalkEnd { LinkZero, MaxCount } // link==0, or the 80/64 parse cap
```

`Layer` gains `Sprite` (index carried on `PixelResolution`). `LineReport` grows additively (§4 stability
contract): `sprites: Vec<SpriteEval>`, `sprite_walk_end: SpriteWalkEnd`, `sprite_overflow: bool`,
`sprite_collision: bool`. (The `line`/`h40`/scroll/window/`pixels` fields are unchanged — push-3 introspection
is forward-compatible.)

### The evaluation walk (R10 / RR8) — reads only the cache

For line `L`, limits `(max_sprites, max_px, cap) = (20,320,80)` H40 / `(16,256,64)` H32:

1. Walk from **sprite 0**, following the cached `link`; stop at `link == 0` (→ `LinkZero`) or after `cap`
   parses (→ `MaxCount`). A visited-set guards a pathological cyclic list within the cap.
2. For each parsed sprite: decode Y + size from the **cache**. On-line iff `y ≤ L < y + height_cells*8`.
   Off-line sprites → `OffLine` (they consume neither the sprite-count nor the pixel budget — hardware only
   spends budget on sprites that touch the line).
3. On-line sprites, in walk order, spend budget: the `(max_sprites+1)`-th on-line sprite →
   `DroppedLineLimit` (+ overflow); once the running pixel total (`+= width_cells*8` per on-line sprite)
   would exceed `max_px` → `DroppedPixelBudget` for that and every later on-line sprite (+ overflow, + this
   line's dot-overflow carry). This is the scanline-model reading of R10's "mask sprites still consume slot +
   pixel budget; parsing continues" — evaluation continues to the terminator, budget just stops admitting.

### Render-time fetch + masking + collision (R10 / RR8) — `resolve_line`

For each in-budget, on-line sprite (walk order), fetch **X + tile/attr from VRAM** at the current `sat_base()`
slot (RR8), then:

- **Masking (R10):** maintain a within-line `seen_nonzero`, **seeded from `self.sprite_dot_overflow_carry`**
  (read-only in the pure path). Before drawing a sprite: if `x_field & 0x1FF == 0` and `seen_nonzero` → this
  sprite's output is suppressed (`Masked`) and — R10 — **masking stays on for the rest of the line** (later
  sprites also `Masked`), though they still spend budget. If `x_field & 0x1FF != 0` → `seen_nonzero = true`.
  (X is only read here at render — evaluation never sees it, per R10 "masking is a render-phase effect".)
- **Draw** the sprite into a per-line sprite line buffer, left→right, **first-come-wins** (a pixel already
  written by an earlier walk-order sprite is not overwritten). Transparent nibble (0) writes nothing.
  **Collision**: any attempt to write an already-opaque sprite-buffer pixel sets `sprite_collision`.
- **Tile addressing (RR8, column-major):** cell `(cx, cy)` → tile `base + cx*height_cells + cy`; `hflip`
  mirrors `cx → W-1-cx` and within-cell px, `vflip` mirrors `cy → H-1-cy` and within-cell py; nibble via the
  existing `tile_nibble` (RR2). Sprites use `palette*16 + nibble` like planes.
- **Composite by opacity (owner scope):** an opaque sprite-buffer pixel overlays the plane result
  (`Layer::Sprite`). The **priority bit is decoded into `PixelResolution.priority` but does not reorder** —
  a low-priority sprite still overlays a high-priority plane pixel this push (the push-5 boundary; identical
  precedent to push 3's plane priority bit).

### Pure `render_line` / `render_line_report` vs the committing `render_scanline`

Design §1 requires rendering to be a **pure function of latched state + line**, and `render_line(&self,line)`
already is. Push 4 keeps it pure:

- `resolve_line(&self, line)` (pure) seeds masking from the **current** `sprite_dot_overflow_carry` field
  (read-only), fully resolves within-line masking + collision + the composite, and computes the *would-be* new
  carry / overflow / collision — but **does not write them back**. `render_line` / `render_line_report` wrap
  it. Safe to call in any order (frame_dump, a debugger) — no hidden per-line ordering dependency.
- `render_scanline(&mut self, line) -> LineReport` (the committing path) calls `resolve_line`, then commits:
  `sprite_dot_overflow_carry = <this line's dot overflow>`, `sprite_overflow |= …`, `sprite_collision |= …`.
  This is what makes the status bits "go real" and drives the cross-line masking carry; it is the hook the
  eventual per-frame render loop / push-5 differential uses. Tested directly this push; **not** wired into
  `System::run` (no per-line VDP render driver yet — that stays out, protecting the goldens: the testrom
  drives no rendering, so overflow/collision stay `false` through it and the export golden is untouched).

`sprites_decoded(&self) -> Vec<SpriteDecoded>` (pure) decodes all 80 entries — Y/size/link from the cache,
X/tile/attr from VRAM at the current base — with the per-entry `cache_divergence` flag (design §4). This is
the *stale-cache made visible*: after a reg-5 change + a Bloodlines-style setup, `cache_divergence` is `true`
where the cached Y/size/link differ from what VRAM now holds at the new base.

## Slicing (gated commits, one per slice, full gate each)

### Slice 1 — SAT cache state + write-through + `sprites_decoded` (**the currency-neutrality proof**)

**Files:** `crates/oracle-core/src/vdp.rs` (new fields, `sat_base`, `write_vram_byte`, write-through, power-on,
`control_read_status` unchanged this slice), `crates/oracle-core/src/render.rs` (`SpriteDecoded`,
`sprites_decoded`), `lib.rs` (re-export `SpriteDecoded`).

Content: add `sat_cache` + `sprite_dot_overflow_carry` serialized fields (power-on seeds them); `sat_base()`
with the H40 bit-0 mask; `write_vram_byte` + route `write_target`'s VRAM arm through it (byte-granular
write-through window); `sprites_decoded()` reading cache Y/size/link + VRAM X/tile with the divergence flag.

Tests (traced to pins):
- `sat_base`: `(reg5 & 0x7F) << 9` H32; H40 masks bit 0 (`0x7E`) (RR8/R5).
- write-through: a data-port VRAM write inside the window updates the matching cache byte; a write outside it
  does not; an **odd-address** byte write updates the swapped cache byte (RR8 remainder 2).
- **reg-5 change does not invalidate** (R5 / Bloodlines): populate the cache at base A, change reg 5 to base
  B, assert the cache still holds A's Y/size/link (only a fresh write at B updates it).
- `sprites_decoded`: Y/size/link from the cache, X/tile/attr from VRAM at the current base; `cache_divergence`
  true exactly where they disagree (the stale-cache case), false in the coherent case.
- **round-trip**: a bincode snapshot with a populated cache + a set carry restores byte-identically.
- **currency neutrality (the headline):** `export_state_hash` == `0x22F80ECF29ED3AD4` and `state_hash`
  unchanged after adding the fields (assert in-test + the gate suites green).

Commit: `feat(vdp): SAT cache write-through + sprites_decoded (recon R5/RR8)`.

### Slice 2 — sprite evaluation walk + `render_line_report` sprite section

**Files:** `crates/oracle-core/src/render.rs`; `lib.rs` (re-export `SpriteEval` / `SpriteOutcome` /
`SpriteWalkEnd`).

Content: the pure evaluation walk (link list + termination, per-line 20/16 sprite + 320/256 px limits,
Y-range on-line test, drop-reason classification — cache-only for Y/size/link); grow `LineReport` with
`sprites` / `sprite_walk_end` / `sprite_overflow` / `sprite_collision`; `render_line_report` populates the
sprite section. (Masking output + collision are refined in slice 3 when X/pixels exist; slice 2 reports
`OffLine` / `DroppedLineLimit` / `DroppedPixelBudget` and the overflow flag from the count/budget.)

Tests (traced to pins):
- link walk order + termination: `LinkZero` on a `link==0`; `MaxCount` on a cyclic list capped at 80/64
  (RR8 — link-cut).
- Y-range: on-line vs off-line (`OffLine`) for a multi-cell-tall sprite (RR8 `y ≤ L < y+H*8`).
- per-line **sprite limit**: the 21st (H40) / 17th (H32) on-line sprite → `DroppedLineLimit` + overflow.
- per-line **pixel budget**: on-line sprites summing past 320/256 px → `DroppedPixelBudget` + overflow.
- H40 vs H32 limits differ (RR8/R10).

Commit: `feat(vdp): sprite evaluation walk + render_line_report sprite section (recon R10/RR8)`.

### Slice 3 — sprite pixel compositing + masking + collision

**Files:** `crates/oracle-core/src/render.rs`.

Content: in `resolve_line`, build the sprite line buffer (in-budget on-line sprites, walk order,
first-come-wins), fetch X + tile/attr from VRAM (RR8), column-major tile addressing + flips, R10 masking
(seeded from the carry field, read-only), collision detection; composite opaque sprite pixels over the plane
result **by opacity** (`Layer::Sprite`, priority decoded-not-ordered). `render_line` now shows sprites;
`SpriteOutcome::Masked` is now produced.

Tests (traced to pins):
- an opaque sprite pixel overlays plane A; a transparent sprite pixel shows the plane beneath (opacity).
- **column-major** multi-cell tile layout (a 2×2 sprite maps cells to `base + cx*H + cy`); hflip / vflip
  mirror cells + within-cell (RR8/RR1).
- **masking (R10):** on a line where sprite 0 has x≠0 (arms `seen_nonzero`) and a later sprite has x=0 → the
  x=0 sprite (and later ones) are `Masked` (no output) but still spend budget; a first-on-line x=0 with the
  carry `false` does **not** mask.
- **collision:** two opaque sprite pixels overlapping on a line set `sprite_collision` in the report.
- **priority decoded-not-ordered:** a low-priority sprite overlays a high-priority plane pixel (the push-5
  boundary), and its `.priority` is still reported.

Commit: `feat(vdp): sprite compositing + x=0 masking + collision (recon R10/RR8)`.

### Slice 4 — `render_scanline` commit path + status-read clear (status bits go real)

**Files:** `crates/oracle-core/src/render.rs` (`render_scanline`), `crates/oracle-core/src/vdp.rs`
(`control_read_status` clears overflow/collision).

Content: `render_scanline(&mut self, line) -> LineReport` commits `sprite_dot_overflow_carry` +
`sprite_overflow |=` + `sprite_collision |=` from `resolve_line`'s computed deltas; `control_read_status`
clears the two sprite flags (Sega manual — status read clears them). Prove the goldens still green (the
testrom drives no sprite rendering, so both flags stay `false` through it → `status_word` byte-identical →
export golden unchanged).

Tests (traced to pins):
- `render_scanline` sets `sprite_overflow` on a limit/budget-exceeding line and `sprite_collision` on an
  overlap line; a clean line sets neither.
- it advances `sprite_dot_overflow_carry` on a dot-overflow line, and that carry makes a **first-on-line x=0
  mask** on the next `render_scanline` (the R10 cross-line carry, end-to-end).
- `control_read_status` clears both sprite flags (Sega manual); the pending toggle behavior is unchanged.
- **currency neutrality still holds**: `export_state_hash` == `0x22F80ECF29ED3AD4`, `state_hash` unchanged.

Commit: `feat(vdp): render_scanline commit path + status-read clears sprite flags (recon R10)`.

### Slice 5 — frame_dump sprites over the striped background (not gated)

**Files:** `crates/oracle-core/examples/frame_dump.rs`.

Content: extend the fixture ROM to set reg 5 (SAT base), write a recognizable sprite tile, and write a small
SAT (a handful of sprites at distinct screen positions, `field = screen + 128`) **through the data port** (so
the write-through populates the cache) over the existing red/white stripes; keep display-enable. `render_line`
(pure) already composites sprites by opacity, so the PPM now shows sprites on the stripes — the picture the
owner looks at. Clippy-clean (examples are covered by `-D warnings`); run it and record the output path.

Commit: `feat(vdp): frame_dump sprites over the striped background (dev tool)`.

Slices 1→4 are strictly ordered (each builds on the prior state / `resolve_line`); slice 5 depends on 1–4. No
slice touches `state_hash`, the `export_state` region layout, the golden constant, the SST harness, or
`m68000/*`. Only slices 1 + 4 touch `vdp.rs` (both proven currency-neutral).

## Anti-cheating / invariants

- **Currency neutrality is the headline invariant** — and this push *adds* `Vdp` fields, so it is asserted
  harder: the `export_state_v1` golden `0x22F80ECF29ED3AD4` and every `state_hash` fingerprint are asserted
  byte-identical at **each** slice (in-test + the gate suites). The verifier greps the new fields and confirms
  neither `state_hash::compute`'s inputs nor `export_state`'s region list gained them. **No golden regen.**
- **SST**: 112 tests / `ran >= 1_000_058`, re-run per slice and at HEAD; `m68000/*` diff empty.
- **Attribution = render** (design §1): `render_line` / `render_line_report` both derive from the single pure
  `resolve_line`; the sprite section of the report is the same walk the pixels come from.
- **Cache is real state, not re-read from VRAM**: the reg-5-no-invalidation test is the Bloodlines proof; if
  it passes only because the code re-reads VRAM, `cache_divergence` would be permanently false — the
  divergence test guards against that.
- **Behavioral facts trace to pins**: every rule cites R5 / R10 / RR8 in a code comment; the interim models
  (RR8 remainders 1 + 2, the power-on cache = 0) are flagged in code pointing at the recon docs,
  `confirm-by-golden-differential in push 5`.
- **No floats**: grep the sprite code for `f32`/`f64`/`as f` — none.
- **New serialized fields round-trip**: the bincode round-trip test covers `sat_cache` + the carry (+ the
  existing overflow/collision/odd-frame).

## Risks

- **A new field leaking into a currency.** Mitigation: both currencies read explicit regions (not a `Vdp`
  bincode); slice-1 + slice-4 assert both goldens byte-identical, failing loudly if a region list changed.
- **Scope creep into push 5.** Sprites composite **by opacity only**; the priority-bit ordering
  (high-sprite > high-A > …) and shadow/highlight are **out** (decision 1). The compositor overlays opaque
  sprite pixels on the plane result — no priority reorder, no S/H operator.
- **The R10 masking model.** Pinned behaviorally (R10 / Kabuto single-latch + the dot-overflow carry) but with
  a scanline-approximation reading of the pixel-budget cut. Implemented as the documented model; tests assert
  the pinned observables (x=0-after-x≠0 masks; carry makes first-on-line mask; masked sprites still spend
  budget), not an unpinned exact mid-sprite cut. The mid-line display-disable budget cut (R10 remainder) stays
  Phase-3 timing territory.
- **The two R5 cache-window remainders.** Implemented as the consistent extension of the pinned write-through
  rule (masked H40 base; byte-granular writes), flagged for the push-5 golden-frame differential — the R8/R9
  precedent (decision 2).
- **The fixture ROM.** Adding a SAT to the hand-assembled fixture is fiddly (the +128 offset, the reg-5 base,
  writing the SAT through the port so the cache updates). Mitigation: a small SAT (a few sprites) and the
  data-port write path, matching the existing fixture style.

## Decisions surfaced (not defaulted)

1. **Sprites composite by opacity only; the priority-bit ordering is OUT (push 5).** The owner's scope brief:
   "Priority-bit ordering and shadow/highlight stay push 5 — composite sprites over planes by opacity, decode
   the priority bit into the report without reordering (the push-3 precedent)." So `Layer::Sprite` overlays
   opaque sprite pixels on the plane result regardless of priority; the priority bit is decoded into
   `PixelResolution.priority` / the report but does not reorder. **Recommendation: opacity compositing;
   priority ordering deferred** — squarely the push-4 boundary the reviewer enforces (identical to push 3's
   plane priority bit).
2. **The two R5 open remainders are DEFERRED with concrete reasons, not experimented this push** (RR8 open
   remainder). The internal SAT cache is not observable over the ratified BlastEm GDB-RSP instrument (it
   exposes CPU + memory, not the VDP framebuffer / internal cache), and the only CPU-readable proxies (sprite
   overflow/collision status) don't discriminate a one-address window-base shift or a sub-word SAT poke. Both
   get the **consistent extension of the pinned write-through rule** as a deterministic interim model
   (H40-masked window base; byte-granular writes), flagged for the **push-5 golden-frame differential** — the
   same pin mechanism as R8/R9, and legitimate per [[feedback-unknowns-timing-vs-behavior]] (a named pin
   mechanism + a deterministic model, not an xfail). **Recommendation: defer with the reasons above**; the
   owner/reviewer can direct the experiment now if they want it, but the differential is the cheaper, more
   direct oracle. DMA fill/copy × cache stays push 6 (no DMA engine yet; interim = fill/copy hit the window).
3. **`render_line` / `render_line_report` stay pure; a separate `render_scanline(&mut self)` commits the
   status bits + masking carry.** Design §1 mandates a pure render; the status bits "going real" needs a
   mutating path. Keeping them separate preserves the pure snapshot API (any-order calls: frame_dump,
   debugger) while giving overflow/collision/carry a real driver + tests. **Recommendation: the split** — it
   is the minimal way to honor both "pure render" (§1) and "status bits go real" (scope) without wiring a
   per-line VDP loop into `System::run` (which would risk the export golden).
4. **`control_read_status` clears sprite overflow + collision** (Sega manual). Cheap + correct, and
   golden-safe (the testrom drives no sprite rendering, so both flags are `false` through it → `status_word`
   is byte-identical → the export golden is untouched). **Recommendation: include it now**; if the reviewer
   prefers it with the rest of the output-stage semantics (push 5), it moves — low cost either way.
5. **Power-on SAT cache = all zero** (the real cache is undefined at power-on; games + fixtures + tests write
   the SAT before it matters). Deterministic, not currency-relevant. **Recommendation: zero, documented** as
   an interim; flagged if a differential ever cares about pre-first-write cache reads.

## Introspection API status after this push

| Op | Status |
|---|---|
| `tile_pixels`, `cram_decoded` | live (push 2) |
| `plane_decoded` | live (push 3) |
| `render_line_report` (plane stages) | live (push 3) |
| `render_line_report` (sprite section) | **live (slice 2–3)** |
| `sprites_decoded` (+ cache-divergence flag) | **live (slice 1)** |
| `pixel_attribution`, `frame_report`, `cram_diff` | later pushes (5–6) |

The wire-protocol (Aether JSON-RPC) wrapping stays with the Oracle-parity op work (out of scope, noted so the
API doesn't accrete as "a later layer").
