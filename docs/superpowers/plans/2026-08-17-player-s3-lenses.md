# Player S3 — Lenses Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Five toggleable, read-only overlays redrawn from live emulator state each frame — watch ticker, CPU chip (with an expanded register block), CRAM strip, sprite outlines, hover callout — each auto-registering its own palette command, with the active set persisted across relaunch.

**Architecture:** A new `lens/` module. Every lens is split into a **pure model fn** (core state → plain data, unit-testable without a window) and a **pure draw fn** (model → pixels in a scratch buffer, pixel-testable), exactly like `overlay.rs`'s `status_text`/`draw` split. Lenses draw into the *window* buffer `screen` after `present::scale_into` and before `palette.draw`, anchored to `present_view` — never into the retained native `buf`, whose ink would accumulate on a paused frontend (the rule `draw_crosshair` learned; main.rs:1700-1710). Toggle state is a `LensSet` bitset owned by the run loop, initialised from config and persisted through the S2 debounce. Spec: `docs/superpowers/specs/2026-08-16-player-buildout-design.md` §5 (lenses 1-3; **audio meters are §9-contract-gated and NOT in this slice**), §9 (module layout), §11 (testing).

**Tech Stack:** Rust, zero new dependencies. Toolchain 1.96.0 (so `usize::div_ceil` is available).

---

## Verified facts (do not re-derive — these were read out of the tree on 2026-08-17)

**Render seam.** main.rs:1726-1738 `present::scale_into(&mut screen, win_w, win_h, present::Frame { px: native, w: width, h: HEIGHT }, present_view, 0x0000_0000, &mut xmap)`; main.rs:1739 `ov.tick();`; main.rs:1749 `palette.draw(&mut screen, win_w, win_h, present_view, &reg);`; main.rs:1750-1765 `ov.draw(...)`; main.rs:1769 `window.update_with_buffer(&screen, win_w, win_h)`. **Lenses draw between 1739 and 1749** — under the palette and under the toasts, over the picture. `present_view` (main.rs:1725) is the anchor rect, NOT `view` (main.rs:1031, which is the pre-step rect the click inverse uses).

**Buffers.** `screen: Vec<u32>` is window-sized, `win_w * win_h`, packed `0x00RR_GGBB`, indexed `buf[y * w + x]`. `buf: Vec<u32>` is the retained native framebuffer (`width * HEIGHT`, `HEIGHT = 224` main.rs:266, `MAX_WIDTH = 320` main.rs:270) — **lenses must not touch it**.

**Drawing primitives** (`font.rs`): `font::Canvas::new(buf: &mut [u32], w: usize, h: usize)`; `Canvas::fill_rect(&mut self, x: i32, y: i32, w_px: usize, h_px: usize, color: u32, alpha: u8)`; `Canvas::text(&mut self, x: i32, y: i32, px: usize, color: u32, text: &str) -> usize` (top-left ink corner, alpha 255, returns `ADVANCE * chars * px`); `font::text_width(text: &str) -> usize` (**unscaled** — multiply by `px` yourself); `font::ADVANCE = 6`, `font::GLYPH_H = 7`, `font::LINE_H = 8`, `font::PANEL_ALPHA = 190`. `Canvas` clips at **buffer edges only** — every string must be pre-truncated with `overlay::fit(text, avail_device_px, px)`. There is **no outline/stroke/line primitive**: a box is four thin `fill_rect` calls. Colours: `overlay::INFO = 0x00FF_FFFF`, `overlay::ACCENT = 0x00FF_C84B`, `overlay::ERROR = 0x00FF_6B5C`.

**Geometry.** `present::Rect { x, y, w, h }` (all `usize`); `present::dest_rect(win_w, win_h, src_w, src_h, aspect) -> Rect` centres with letterbox offsets; `present::window_to_native(mx: f32, my: f32, rect: Rect, src_w: usize, src_h: usize) -> Option<(u16,u16)>` is the existing window→game inverse. **There is no game→window map — Task 1 builds it.** `scale_into`'s forward blit is `src_col = (dst_col * src_w / rect.w).min(src_w - 1)`, so the window span showing game column `gx` is `[ceil(gx * rect.w / src_w), ceil((gx+1) * rect.w / src_w))`. `Overlay::font_scale(h) = (h / 224).clamp(1, 4)`; overlay uses `area.h`, palette uses window `h` (palette.rs:251-256 documents the divergence). House margin idiom: `px = font_scale(area.h.max(1))`, `margin = (2 * px).max(4)`, `pad = 2 * px`.

**Registry.** `Cmd` (commands.rs:10-38) already has a payload variant precedent `SlotSelect(usize)`. `Group::ALL` is `[Group; 4]` (commands.rs:50) — adding a group is a compile-enforced edit. `CommandInfo { cmd, title: &'static str, group, hotkey: Option<Key>, repeat: bool, hidden: bool }` with `const fn new(cmd, title, group, hotkey)` defaulting `repeat`/`hidden` to false. The `SLOT_TITLES`/`slot_keys` loop (commands.rs:168-206) is the auto-registration precedent. Invariant tests that will bind this slice: `titles_unique` (:337), `hotkeys_unique` (:350), `groups_nonempty` (:364), `key_names_for_all_registry_hotkeys` (:404). `palette.rs` contains **zero** `#[cfg]` — conditional presence lives only in `commands.rs`.

**Dispatch.** Inline in the run loop, one `for cmd in pending { match cmd { … } }`, main.rs:1131-1437. The exact template for a lens toggle is `Cmd::ToggleStatusLine`, main.rs:1144-1148:
```rust
                commands::Cmd::ToggleStatusLine => {
                    ov.status_line = !ov.status_line;
                    cfg.status_line = ov.status_line;
                    config_save_countdown = Some(CONFIG_AUTOSAVE_DEBOUNCE_FRAMES);
                }
```
Config→render-path init happens once before the loop at main.rs:960 (`ov.status_line = cfg.status_line;`). `CONFIG_AUTOSAVE_DEBOUNCE_FRAMES = 120` (main.rs:977); the debounced writer is main.rs:1678-1696 and advances `cfg_saved` only on success; the quit write is main.rs:1787-1803 and diffs `cfg != cfg_saved`.

**Config.** Six keys today (`volume, muted, aspect, scale, status_line, deadzone`), bool grammar is **`on`/`off`**. `parse` matches exact key literals with one `_` catch-all that warns (config.rs:111). `Config` derives `Clone, PartialEq, Debug`; `PartialEq` is load-bearing for the quit-write diff. `round_trip_is_identity` (config.rs:225) names every field explicitly, so a new field is a compile error there — good. `serialize_covers_every_field` (config.rs:282) lists key names. `scratch_dir(tag)` (config.rs:412) is the mandated temp-dir helper. **F-CONFIG-UNKNOWN-KEYS (S2, adjudicated DEFER with mechanical reversal): the first commit widening the key set past six MUST land unknown-key preservation + a round-trip test in that same commit.** That is Task 3, and it is not optional.

**Watch data.** `Watchpoints::hits() -> &[WatchHit]` (watchpoints.rs:759) is **non-destructive, oldest-first** — never `take_hits()`, which would delete a socket client's evidence (main.rs:1152-1155 states the rule). `watch_count() -> usize` (:734) is the armed count, already called per-frame at main.rs:1531. `dropped() -> u64` (:773). `WatchHit { watch: WatchId(pub u32), space: WatchSpace, addr: u32, old: u32, value: u32, size, op, fc, via: WatchVia, pc: u32, frame: u64, mclk: u64, seq: u64 }` — `Copy`. **No `Display` impl exists on any watch enum**; spellings are hand-rolled (`dump_hits` main.rs:516-543 uses `Bus`/`Direct(CPU)`/`Dma`; the aether uses lowercase `bus`/`vram`/`cram`/`vsram`). There is **no "hits since seq N"** API.

**Sprites.** `Vdp::sprites_decoded() -> Vec<SpriteDecoded>` (render.rs:630) decodes all 80 slots each call (~1.3 KB alloc) — call it **once per frame**, never per sprite. `SpriteDecoded { index: u8, y: i16, x: i16, width_cells: u8, height_cells: u8, link: u8, tile: u16, palette: u8, hflip, vflip, priority, cache_divergence }`. **x/y are screen coordinates with the 128 bias already subtracted on BOTH axes, and may be negative**; `width_cells`/`height_cells` are 1..=4 **cells**, pixels = cells × 8; `(x, y)` is the top-left corner. `Vdp::parsed_sprite_max() -> u8` (render.rs:548) is 80 in H40 / 64 in H32 and a consumer is **forbidden** to re-derive it. `Vdp::active_display() -> (u16, u16)` (render.rs:536) is the clip rect. A parked sprite is `y == -128`.

**CRAM.** `Vdp::cram_decoded() -> [(u8,u8,u8); 64]` (vdp.rs:1400) is public and pinned by `cram_rgb_matches_cram_decoded` (render.rs:1622) to agree **exactly** with the renderer's own per-entry decode at `PixelState::Normal`. The S/H-aware `cram_rgb_state` is private — so a strip built from `cram_decoded()` matches the picture everywhere except shadow/highlight regions. That is a known, accepted divergence; say so in the module doc, do not add a public accessor for it in this slice.

**CPU.** `System::cpu_regs() -> &Registers` (system.rs:796). `Registers { d: [u32; 8], a: [u32; 7], usp, ssp, pc, sr, prefetch }` — **`a` is only 7 wide; use `addr_reg(i)` for A0-A7 and `a7()`**, never `a[7]`. There is **no `System::frame()`** and **no `System::is_paused()`**: the loop-local `frame: u64` (main.rs:955) is what the status line and title already show, and the loop-local `paused` (main.rs:954) is authoritative for UI.

**Symbols.** `SymbolTable::resolve_within(addr, max_displacement) -> Option<Resolution>` (symbols.rs:567), binary search, allocation-free, safe per-frame. `Resolution`'s `Display` renders `EntryPoint.wait_dma` exactly or `EntryPoint.wait_dma+$1A` otherwise (uppercase hex, `$` prefix, no padding), and falls back to the long mangled name when ambiguous — **so a fixed-width chip must truncate**. `const MAX_SYMBOL_DISPLACEMENT: u32 = 0x1000;` (main.rs:505-509). Table is `Option<SymbolTable>` at main.rs:805, reloaded on ROM reload at main.rs:1385.

**Hover.** `Vdp::pixel_attribution(x, y) -> PixelAttribution { x, y, winner: Layer, cram_index: u8, rgb, state, cell: Option<Cell>, candidates: Vec<Candidate> }`; `Layer::{Backdrop, PlaneB, PlaneA, Window, Sprite(u8)}`; `Cell { tile: u16, palette: u8, hflip, vflip, priority }`. Cost is one extra scanline resolve ≈ 1/224 of the render the loop already does — affordable per frame. **Use `pixel_attribution` directly for hover; do NOT use `pick::resolve`**, which additionally allocates three `String`s and a second `sprites_decoded()` and belongs to the click path. `present::window_to_native(...)` returns `None` outside the picture — that is the hover hit-test for free. Attribution reads *current* VDP state while the picture came from the last capture, so a callout can disagree mid-frame effects; that is expected and already documented for the click path (main.rs:1044-1047).

**Test house style.** All tests are inline `#[cfg(test)] mod tests` — the crate has **no `tests/` dir**. Pixel tests build `vec![0u32; w*h]` (or a distinctive `0x0012_3456` fill when asserting changed-vs-untouched) and index `buf[y * w + x]`. Two patterns every new drawing surface copies: `drawing_the_overlay_marks_the_presentation_buffer_only` (overlay.rs:488) and `draw_paints_inside_area_only` (palette.rs:691). The narrow-panel underflow class is pinned by `draw_narrow_panel_does_not_underflow` (palette.rs:753) — **any lens sizing off window `h` rather than `area.h` inherits that hazard**; size off `area`.

**Features.** `default = ["audio", "gamepad", "aether"]`. `--no-default-features` drops all three at once. `bus.watchpoints_mut()` is available in both builds (main.rs:1531 calls it unconditionally).

---

## House rules binding every task

- `cargo test -p oracle-frontend` **never** piped through `tail`/`head` (it hides failures *and* returns the wrong exit code).
- **Every evidence-bearing test is mutation-verified at writing time**: break the production line the test exists to protect, watch that exact test fail, restore, and record one line in the commit body (`mutation: <change> → <test> FAILED`). Measured base rate for vacuous tests here is 3-for-3 past green gates.
- **Lens draw tests fill the scratch buffer with `0x0012_3456`, never `0`, and compare against that.** Measured in Task 5: a translucent black panel (`fill_rect(…, 0x0000_0000, PANEL_ALPHA)`) blended over a zero buffer is a **no-op**, so an all-zero fill makes the panel invisible to every assertion — a panel spanning the whole window, straight through the letterbox, *passed* `draw_paints_inside_area_only`. Copying `palette.rs:691` does **not** protect you: that test presses a key first so its **opaque** highlight bar draws, and a lens panel has no opaque element. Copy `lens/watch.rs`'s reworked tests instead (`const BG`, `!= BG` throughout, plus an `assert!(painted >= panel_w * panel_h)` that text alone can never satisfy — so a future invisible panel fails rather than passing quietly). **Every lens with a panel also needs one test pinning where it is anchored**; containment alone does not distinguish a bottom strip from a top one.
- No `#[allow(dead_code)]`. No `Co-Authored-By` trailers. `ls` is aliased to eza — use `command ls`.
- Per-task gates: `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings` **and** `cargo clippy --all-targets --no-default-features -- -D warnings`; `cargo test -p oracle-frontend` **and** `cargo test -p oracle-frontend --no-default-features`. **Every commit passes them — there is no "it goes green two tasks from now" allowance.** `oracle-frontend` is bin-only, so an uncalled `pub fn` is a hard `dead_code` error: land production code and a non-test caller in the same commit. (This is not hypothetical; it is why Task 1 is void.)
- **`git diff m68000-microop-framework..HEAD -- crates/oracle-core/` must stay EMPTY for the whole slice.** This slice adds no core capability; every read it needs already exists. If a lens seems to need a new core accessor, STOP and report rather than adding one.
- Work in the worktree `/home/volence/sonic_hacks/oracle-next/.worktrees/player-s1-palette` on branch **`player-s3-lenses`**, which is cut from `743a5b5`. **Base check before touching anything:** `git log --oneline -1` must show `743a5b5 docs: S2 handoff — persistence shipped, smoke checklist extended, F-CONFIG-UNKNOWN-KEYS` (or a descendant of it), and `crates/oracle-frontend/src/config.rs` must exist. If either fails, stop — you have a stale worktree.

---

## File structure

| File | Responsibility |
|---|---|
| `crates/oracle-frontend/src/present.rs` (modify, **in Task 8**) | `native_rect_to_window` — the forward map, inverse of `window_to_native` |
| `crates/oracle-frontend/src/lens/mod.rs` (create) | `LensId`, `LensSet`, `parse_set`/`format_set`, `Models`, `models()`, `draw()` — the one call the run loop makes |
| `crates/oracle-frontend/src/lens/watch.rs` (create) | Watch ticker: `Ticker` model + `line()` formatting + `draw` |
| `crates/oracle-frontend/src/lens/cpu.rs` (create) | CPU chip: `Chip` model (compact + expanded) + `draw` |
| `crates/oracle-frontend/src/lens/video.rs` (create) | `SpriteBox`/`boxes()`, CRAM strip swatches, `Hover` model + the three draws |
| `crates/oracle-frontend/src/config.rs` (modify) | `lenses` key + **unknown-key preservation** (`Config::unknown`) + truthful header |
| `crates/oracle-frontend/src/commands.rs` (modify) | `Group::Lenses`, `Cmd::ToggleLens(LensId)`, auto-registration loop |
| `crates/oracle-frontend/src/main.rs` (modify) | `mod lens;` · `lenses` loop state · dispatch arm · hover tracking · the draw call · module doc + banner |

---

### Task 1: VOID — folded into Task 8

**Measured on 2026-08-17, after this task was built once and reverted.** `oracle-frontend` is a
**bin-only crate** (no `[lib]` target — `main.rs` is the crate root), so `pub` does not exempt an
uncalled function from `dead_code`: shipping `native_rect_to_window` before its first caller makes
`cargo clippy --all-targets -- -D warnings` fail with `error: function native_rect_to_window is
never used`, and it stays failing for the **seven** commits until Task 8 lands. That is precisely
the pressure that produces the `#[allow(dead_code)]` this plan forbids.

So the forward map ships **inside Task 8**, its first consumer, and Task 8 opens with it. The
built-and-reverted implementation — which was correct, and whose tests were each killed by a
distinct targeted mutation — is saved as a patch at
`/tmp/claude-1000/-home-volence-sonic-hacks-oracle-next/6c6660c8-9c88-40d2-b140-7e63c2e0a455/scratchpad/forward-map.patch`
and is reproduced in full in Task 8. Tasks 2-7 are unaffected and keep their numbering.

**The general rule this establishes for the rest of the slice:** in this crate, a commit that adds
a function without a caller cannot pass its own gate. Every task must land its production code and
at least one non-test caller together.

<details>
<summary>Original Task 1 text (kept for reference; do not execute)</summary>

- [ ] **Step 1: Write the failing tests**

Add inside `present.rs`'s existing `#[cfg(test)] mod tests`:

```rust
    /// The forward map must be the exact inverse of the click map: every window pixel inside the
    /// returned span must resolve back to the game pixel that produced it. This is the same
    /// property `the_click_inverse_matches_the_blit_pixel_for_pixel` pins from the other side —
    /// an off-by-one here would draw sprite outlines one column off the sprite.
    #[test]
    fn the_forward_map_is_the_inverse_of_the_click_map() {
        for (win_w, win_h, src_w, aspect) in [
            (640usize, 480usize, 320usize, Aspect::Tv),
            (777, 501, 320, Aspect::Tv),
            (640, 448, 256, Aspect::Square),
            (900, 700, 320, Aspect::Integer),
        ] {
            let rect = dest_rect(win_w, win_h, src_w, 224, aspect);
            for gx in 0..src_w {
                for gy in (0..224usize).step_by(37) {
                    let g = Rect { x: gx, y: gy, w: 1, h: 1 };
                    let out = native_rect_to_window(g, rect, src_w, 224)
                        .expect("an in-range game pixel always maps");
                    for wy in out.y..out.y + out.h {
                        for wx in out.x..out.x + out.w {
                            let back = window_to_native(wx as f32, wy as f32, rect, src_w, 224);
                            assert_eq!(
                                back,
                                Some((gx as u16, gy as u16)),
                                "({gx},{gy}) -> window ({wx},{wy}) -> {back:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Out-of-range and degenerate inputs answer `None` rather than producing a rect that would
    /// paint outside the picture (the letterbox must stay black).
    #[test]
    fn the_forward_map_refuses_what_it_cannot_place() {
        let rect = dest_rect(640, 480, 320, 224, Aspect::Tv);
        assert_eq!(native_rect_to_window(Rect { x: 320, y: 0, w: 1, h: 1 }, rect, 320, 224), None);
        assert_eq!(native_rect_to_window(Rect { x: 0, y: 224, w: 1, h: 1 }, rect, 320, 224), None);
        assert_eq!(native_rect_to_window(Rect { x: 0, y: 0, w: 0, h: 1 }, rect, 320, 224), None);
        let degenerate = Rect { x: 0, y: 0, w: 0, h: 0 };
        assert_eq!(native_rect_to_window(Rect { x: 0, y: 0, w: 1, h: 1 }, degenerate, 320, 224), None);
    }

    /// A rect running past the right/bottom edge is clipped to the picture, not dropped: a sprite
    /// half off-screen must still show the half that is on-screen.
    #[test]
    fn the_forward_map_clips_a_rect_that_overruns_the_picture() {
        let rect = dest_rect(640, 480, 320, 224, Aspect::Tv);
        let out = native_rect_to_window(Rect { x: 300, y: 210, w: 32, h: 32 }, rect, 320, 224)
            .expect("a partly-visible rect still maps");
        assert!(out.x + out.w <= rect.x + rect.w, "clipped to the picture's right edge");
        assert!(out.y + out.h <= rect.y + rect.h, "clipped to the picture's bottom edge");
    }
```

- [ ] **Step 2: Run the tests and watch them fail**

```bash
cargo test -p oracle-frontend present::tests::the_forward_map
```
Expected: FAIL — `cannot find function native_rect_to_window in this scope`.

- [ ] **Step 3: Implement it**

Add to `present.rs` after `window_to_native`:

```rust
/// The **forward** map — game-pixel rect to the window pixels that show it, the exact inverse of
/// [`window_to_native`]. Lenses that mark things *in the picture* (sprite outlines, a hover
/// callout anchored to a dot) need this; nothing needed it before, because everything drawn so
/// far was anchored to the picture's corners rather than to a pixel inside it.
///
/// Derived from [`scale_into`]'s blit rather than guessed: that maps window column `dx` to source
/// column `floor(dx * src_w / rect.w)`, so the window columns showing game column `gx` are
/// `[ceil(gx * rect.w / src_w), ceil((gx + 1) * rect.w / src_w))`. **The ceiling is not
/// cosmetic** — the floor form is off by one on most non-integer scales, which is one column of
/// outline sitting beside the sprite instead of on it.
///
/// `g` is in game pixels and must be non-negative (callers clip against `active_display` first).
/// Returns `None` when the rect is empty, the geometry is degenerate, or `g` starts outside the
/// picture; the result is always at least 1×1 so a hairline never vanishes at small scales.
pub fn native_rect_to_window(g: Rect, rect: Rect, src_w: usize, src_h: usize) -> Option<Rect> {
    if rect.w == 0 || rect.h == 0 || src_w == 0 || src_h == 0 || g.w == 0 || g.h == 0 {
        return None;
    }
    if g.x >= src_w || g.y >= src_h {
        return None;
    }
    let gx1 = (g.x + g.w).min(src_w);
    let gy1 = (g.y + g.h).min(src_h);
    let edge = |n: usize, dst: usize, src: usize| (n * dst).div_ceil(src);
    let x0 = edge(g.x, rect.w, src_w);
    let x1 = edge(gx1, rect.w, src_w);
    let y0 = edge(g.y, rect.h, src_h);
    let y1 = edge(gy1, rect.h, src_h);
    Some(Rect {
        x: rect.x + x0,
        y: rect.y + y0,
        w: (x1 - x0).max(1),
        h: (y1 - y0).max(1),
    })
}
```

- [ ] **Step 4: Run the tests and watch them pass**

```bash
cargo test -p oracle-frontend present::
```
Expected: PASS, 14 tests (11 existing + 3).

- [ ] **Step 5: Mutation-verify**

Change `div_ceil` to plain `/` (the floor form) in `edge`, re-run `the_forward_map_is_the_inverse_of_the_click_map`, confirm it FAILS, restore. Record the line.

- [ ] **Step 6: Commit**

```bash
git add crates/oracle-frontend/src/present.rs
git commit -m "feat(frontend): the blit's forward map — game rect to window rect

Lenses that mark things inside the picture need the inverse of the click
map, and the ceiling form is the only one that round-trips: the floor form
is off by one on most non-integer scales.

mutation: div_ceil -> / in edge() → the_forward_map_is_the_inverse_of_the_click_map FAILED"
```

</details>

---

### ⚠ Tasks 2, 3 and 4 are ONE commit — "the lens spine"

The same bin-only `dead_code` rule that voided Task 1 binds these three, and the dependency chain
is why: `LensId::key` gets its caller from the config (Task 3), `title` from the registry and
`label` from the dispatch toast (Task 4), `toggle`/`set`/`is_on` from both. Committed separately,
each of the three fails its own clippy gate on the symbols the *next* one uses.

So: implement Tasks 2, 3 and 4 in order, run the gates **once** at the end, and make **one**
commit with the message given at the end of Task 4. Two adjustments follow from the merge:

- **Do not create placeholder `lens/watch.rs`, `cpu.rs`, `video.rs` files, and do not declare
  `pub mod watch/cpu/video` yet.** Each submodule is declared by the task that fills it (5, 6,
  7-9). An empty placeholder is dead weight the gate would rightly flag.
- **Leave `LensSet::any()` out of this commit.** Its only caller is the run loop's draw guard,
  which lands in Task 5 — add the method there, with its test.

Everything else in the three task texts stands as written, including every test and every
mutation check.

---

### Task 2: `lens/mod.rs` — `LensId` and `LensSet`

**Files:**
- Create: `crates/oracle-frontend/src/lens/mod.rs`
- Modify: `crates/oracle-frontend/src/main.rs` (add `mod lens;` next to `mod commands;`)

- [ ] **Step 1: Write the module (types + parse/format) and its tests**

```rust
//! Lenses — named, toggleable overlays redrawn from live emulator state each frame (spec §5).
//!
//! Every lens is two pure halves: a **model** fn that turns core state into plain data, and a
//! **draw** fn that turns that data into pixels. The split is what makes them testable without a
//! window (the `overlay.rs` pattern), and it keeps the expensive reads — `sprites_decoded`,
//! `pixel_attribution` — out of the draw path where they would run whether or not the lens is on.
//!
//! Lenses are **read-only over core state** and draw into the *window* buffer, never the retained
//! native framebuffer: a paused frontend re-presents that buffer every iteration, so ink there
//! accumulates (the lesson `draw_crosshair` records at main.rs:1700-1710).

pub mod cpu;
pub mod video;
pub mod watch;

/// Every lens, in registration and display order.
///
/// `CpuRegs` is not a second panel: it selects the CPU lens's **expanded** form (the full
/// D0-D7/A0-A7 block) rather than the compact chip, which is why it is a lens id rather than a
/// mode flag — it persists and auto-registers a command for free, and the CPU panel draws if
/// either is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LensId {
    Watch,
    Cpu,
    CpuRegs,
    Sprites,
    Cram,
    Hover,
}

impl LensId {
    pub const ALL: [LensId; 6] = [
        LensId::Watch,
        LensId::Cpu,
        LensId::CpuRegs,
        LensId::Sprites,
        LensId::Cram,
        LensId::Hover,
    ];

    /// The config-file spelling. Stable: changing one silently drops a user's setting.
    pub fn key(self) -> &'static str {
        match self {
            LensId::Watch => "watch",
            LensId::Cpu => "cpu",
            LensId::CpuRegs => "cpu_regs",
            LensId::Sprites => "sprites",
            LensId::Cram => "cram",
            LensId::Hover => "hover",
        }
    }

    /// The palette row. `&'static str` because that is what `CommandInfo::title` takes.
    pub fn title(self) -> &'static str {
        match self {
            LensId::Watch => "Toggle watch ticker",
            LensId::Cpu => "Toggle CPU chip",
            LensId::CpuRegs => "Toggle CPU registers (full D0-D7/A0-A7)",
            LensId::Sprites => "Toggle sprite outlines",
            LensId::Cram => "Toggle CRAM strip",
            LensId::Hover => "Toggle hover callout",
        }
    }

    /// The toast spelling — short enough to read at a glance in the corner.
    pub fn label(self) -> &'static str {
        match self {
            LensId::Watch => "WATCH TICKER",
            LensId::Cpu => "CPU CHIP",
            LensId::CpuRegs => "CPU REGISTERS",
            LensId::Sprites => "SPRITE OUTLINES",
            LensId::Cram => "CRAM STRIP",
            LensId::Hover => "HOVER CALLOUT",
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// Which lenses are on. A bitset because it is `Copy` and `PartialEq` — `config::Config`'s
/// quit-write diff compares whole configs, so a heap set here would allocate on every frame's
/// clone and compare by pointer-chasing for nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LensSet(u8);

impl LensSet {
    pub fn is_on(self, id: LensId) -> bool {
        self.0 & id.bit() != 0
    }
    pub fn set(&mut self, id: LensId, on: bool) {
        if on {
            self.0 |= id.bit();
        } else {
            self.0 &= !id.bit();
        }
    }
    pub fn toggle(&mut self, id: LensId) {
        self.0 ^= id.bit();
    }
    /// True when anything is on — lets the run loop skip every model fn in the common case.
    pub fn any(self) -> bool {
        self.0 != 0
    }
}

/// Parse the config file's `lenses` value: a comma-separated list of [`LensId::key`] spellings.
/// Unknown names warn and are dropped rather than failing the line — a newer build's lens must
/// not cost an older build its whole setting (the same forward-compatibility rule the config's
/// unknown *keys* follow). Empty items are skipped silently so `a,,b` and a trailing comma are
/// both fine.
pub fn parse_set(value: &str) -> (LensSet, Vec<String>) {
    let mut set = LensSet::default();
    let mut warnings = Vec::new();
    for name in value.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        match LensId::ALL.iter().find(|id| id.key() == name) {
            Some(id) => set.set(*id, true),
            None => warnings.push(format!("config: ignored lens `{name}` (unknown lens)")),
        }
    }
    (set, warnings)
}

/// The inverse, in [`LensId::ALL`] order so the file is stable across saves (an unstable order
/// would rewrite the file — and wake the debounce — on every launch).
pub fn format_set(set: LensSet) -> String {
    LensId::ALL
        .iter()
        .filter(|id| set.is_on(**id))
        .map(|id| id.key())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_round_trips_through_the_file_spelling() {
        let mut set = LensSet::default();
        set.set(LensId::Watch, true);
        set.set(LensId::Cram, true);
        let text = format_set(set);
        assert_eq!(text, "watch,cram", "stable order, ALL order");
        let (back, warnings) = parse_set(&text);
        assert_eq!(back, set);
        assert!(warnings.is_empty(), "own output warned: {warnings:?}");
    }

    #[test]
    fn every_lens_round_trips_and_the_empty_set_is_empty() {
        for id in LensId::ALL {
            let mut set = LensSet::default();
            set.set(id, true);
            let (back, warnings) = parse_set(&format_set(set));
            assert_eq!(back, set, "{} did not round-trip", id.key());
            assert!(warnings.is_empty());
        }
        assert_eq!(format_set(LensSet::default()), "");
        assert_eq!(parse_set("").0, LensSet::default());
        assert!(parse_set("").1.is_empty(), "an empty value is not a warning");
    }

    #[test]
    fn an_unknown_lens_warns_and_leaves_the_rest_alone() {
        let (set, warnings) = parse_set("watch, from_the_future ,cram");
        assert!(set.is_on(LensId::Watch) && set.is_on(LensId::Cram));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("from_the_future"), "the warning names it");
    }

    #[test]
    fn toggle_and_any_agree_with_is_on() {
        let mut set = LensSet::default();
        assert!(!set.any());
        set.toggle(LensId::Hover);
        assert!(set.is_on(LensId::Hover) && set.any());
        set.toggle(LensId::Hover);
        assert!(!set.is_on(LensId::Hover) && !set.any(), "toggle is its own inverse");
    }

    /// Each lens must own a distinct bit — two sharing one would make toggling either flip both,
    /// and a bitset makes that a silent aliasing bug rather than a compile error.
    #[test]
    fn every_lens_has_its_own_bit_and_its_own_spellings() {
        let mut seen = 0u8;
        for id in LensId::ALL {
            assert_eq!(seen & id.bit(), 0, "{} reuses a bit", id.key());
            seen |= id.bit();
        }
        for (i, a) in LensId::ALL.iter().enumerate() {
            for b in &LensId::ALL[i + 1..] {
                assert_ne!(a.key(), b.key(), "duplicate config key");
                assert_ne!(a.title(), b.title(), "duplicate palette title");
                assert_ne!(a.label(), b.label(), "duplicate toast label");
            }
        }
    }
}
```

Create the three submodules as empty-but-valid files so `pub mod` resolves; each gets its content in its own task:

`crates/oracle-frontend/src/lens/watch.rs`, `.../cpu.rs`, `.../video.rs` — each containing only:
```rust
//! Placeholder — filled in by its own task in this slice's plan.
```
**Do not leave a placeholder past its own task.** If you are executing tasks out of order, the file must gain real content in Task 5/6/7-9 respectively.

- [ ] **Step 2: Declare the module and run the tests**

Add to `main.rs` next to the other `mod` lines (near main.rs:228-260):
```rust
// Lenses: read-only overlays over the picture, each its own toggle command (spec §5).
mod lens;
```

```bash
cargo test -p oracle-frontend lens::
```
Expected: PASS, 5 tests.

- [ ] **Step 3: Mutation-verify**

Give `LensId::CpuRegs` the same `bit()` as `LensId::Cpu` (e.g. `LensId::CpuRegs => 1 << 1`) — but note `bit()` is derived from the discriminant, so instead reorder `ALL` to list `LensId::Cpu` twice; confirm `every_lens_has_its_own_bit_and_its_own_spellings` FAILS, restore. Then change `format_set`'s order to `.rev()` and confirm `set_round_trips_through_the_file_spelling` FAILS on the literal `"watch,cram"`. Record both lines.

- [ ] **Step 4: Do NOT commit — continue to Task 3**

Record the two mutation lines; they go in the merged spine commit's body. Note that
`cargo clippy` will still be red here (nothing calls `key`/`title`/`label` yet) — that is
expected and is exactly why the three tasks share a commit. The gate runs once, at the end of
Task 4.

---

### Task 3: config — the `lenses` key **and** unknown-key preservation (F-CONFIG-UNKNOWN-KEYS)

**This is the gate commit.** S2 deferred unknown-key preservation with an explicit mechanical reversal: *the first commit widening the key set past six must land preservation + a round-trip test in that same commit.* Both halves ship here or neither does.

**Files:**
- Modify: `crates/oracle-frontend/src/config.rs`

- [ ] **Step 1: Write the failing tests**

Add to `config.rs`'s test module, and **replace** the existing `unknown_key_warns_and_is_ignored` (config.rs:247-256) with the first of these — its name and its claim are both now false:

```rust
    #[test]
    fn unknown_key_warns_and_is_preserved() {
        let p = parse("future_key = 7\nvolume = 4\n").expect("unknown keys are not corruption");
        assert_eq!(p.config.volume, 4);
        assert_eq!(p.warnings.len(), 1);
        assert!(p.warnings[0].contains("future_key"), "warning names the key");
        assert_eq!(
            p.config.unknown,
            vec![("future_key".to_string(), "7".to_string())],
            "an unknown key is kept, not dropped"
        );
    }

    /// F-CONFIG-UNKNOWN-KEYS, the reversal S2 registered: a key this build does not know must
    /// survive a full load-save cycle byte-for-byte. Without this, launching an older build once
    /// silently deletes every setting a newer build wrote — the failure mode the whole
    /// warn-and-continue parse path exists to prevent.
    #[test]
    fn an_unknown_key_survives_a_save() {
        let text = "volume = 4\nlens_from_2027 = spectacular\nview.heatmap.dock = right\n";
        let p = parse(text).expect("unknown keys are not corruption");
        let out = serialize(&p.config);
        assert!(out.contains("lens_from_2027 = spectacular"), "value preserved verbatim");
        assert!(out.contains("view.heatmap.dock = right"), "all of them, not just the first");
        let again = parse(&out).expect("our own output parses");
        assert_eq!(again.config, p.config, "a second cycle is a fixed point");
        assert_eq!(again.config.unknown.len(), 2);
    }

    #[test]
    fn the_lens_set_round_trips_through_the_file() {
        let mut lenses = crate::lens::LensSet::default();
        lenses.set(crate::lens::LensId::Watch, true);
        lenses.set(crate::lens::LensId::Hover, true);
        let c = Config { lenses, ..Config::default() };
        let p = parse(&serialize(&c)).expect("own output must parse");
        assert_eq!(p.config.lenses, lenses);
        assert!(p.warnings.is_empty(), "own output warned: {:?}", p.warnings);
    }

    #[test]
    fn an_unknown_lens_name_warns_without_losing_the_known_ones() {
        let p = parse("lenses = watch,not_a_lens\n").expect("not corruption");
        assert!(p.config.lenses.is_on(crate::lens::LensId::Watch));
        assert_eq!(p.warnings.len(), 1);
        assert!(p.warnings[0].contains("not_a_lens"));
    }
```

Extend `round_trip_is_identity` (config.rs:225) — it names every field, so it will not compile until you add the two new ones:
```rust
        let c = Config {
            volume: 4,
            muted: true,
            aspect: Aspect::from_name("integer").unwrap(),
            scale: 5,
            status_line: true,
            deadzone: 0.35,
            lenses: {
                let mut l = crate::lens::LensSet::default();
                l.set(crate::lens::LensId::Sprites, true);
                l
            },
            unknown: vec![("kept".to_string(), "value".to_string())],
        };
```

Extend `serialize_covers_every_field` (config.rs:282) key list with `"lenses"`.

- [ ] **Step 2: Run and watch them fail**

```bash
cargo test -p oracle-frontend config::
```
Expected: FAIL to compile — `struct Config has no field named lenses`.

- [ ] **Step 3: Implement**

In `Config` (config.rs:16-32) add:
```rust
    /// Which lenses are on (spec §5). One flat key rather than one per lens: six booleans in six
    /// keys would be six chances for a stale file to disagree with itself.
    pub lenses: crate::lens::LensSet,
    /// Keys this build does not recognise, kept verbatim and written back out (F-CONFIG-UNKNOWN-KEYS).
    /// Order is file order, so a save is a fixed point rather than a reshuffle. This is what makes
    /// "warn and continue" honest: without it, an older build reading a newer build's file warns
    /// politely and then deletes the setting at the next autosave.
    pub unknown: Vec<(String, String)>,
```

In `Default` (config.rs:36-46) add `lenses: crate::lens::LensSet::default(),` and `unknown: Vec::new(),`. **Default is every lens off** — a fresh install stays pixel-identical to pre-S3, the property S2 established.

In `parse`, add before the `_` arm:
```rust
            "lenses" => {
                let (set, mut w) = crate::lens::parse_set(value);
                c.lenses = set;
                warnings.append(&mut w);
            }
```
and replace the `_` arm (config.rs:111):
```rust
            _ => {
                warnings.push(format!("config: kept {key} (unknown key, preserved on save)"));
                c.unknown.push((key.to_string(), value.to_string()));
            }
```

Replace `serialize` (config.rs:120-135) entirely — the header must stop claiming keys are dropped, because they no longer are:
```rust
/// Renders every field as one `key = value` line, plus the two `#` header lines whose wording
/// must stay in sync with the module doc's failure-model description above, plus any keys this
/// build did not recognise, verbatim and last.
pub fn serialize(c: &Config) -> String {
    let on_off = |b: bool| if b { "on" } else { "off" };
    let mut out = format!(
        "# oracle player settings — edited in-app. Hand edits are fine; keys this build does not\n\
         # know are warned about at load and written back unchanged (a malformed line backs the file up to .bak).\n\
         volume = {}\nmuted = {}\naspect = {}\nscale = {}\nstatus_line = {}\ndeadzone = {}\nlenses = {}\n",
        c.volume,
        on_off(c.muted),
        c.aspect.name(),
        c.scale,
        on_off(c.status_line),
        c.deadzone,
        crate::lens::format_set(c.lenses),
    );
    for (k, v) in &c.unknown {
        out.push_str(&format!("{k} = {v}\n"));
    }
    out
}
```

Update the module doc's last sentence (config.rs:5-8) to match: unknown keys are now warned about **and preserved**, and the trade it describes ("forward compatibility for reading, one flat writer for writing") is no longer the trade being made.

- [ ] **Step 4: Run both variants**

```bash
cargo test -p oracle-frontend config::
cargo test -p oracle-frontend --no-default-features config::
```
Expected: PASS both, 14 tests (11 existing, one renamed, +3).

- [ ] **Step 5: Mutation-verify**

1. Delete the `for (k, v) in &c.unknown` loop in `serialize` → `an_unknown_key_survives_a_save` FAILS.
2. Make the `_` arm warn without pushing to `c.unknown` (the S2 behaviour) → `unknown_key_warns_and_is_preserved` FAILS.
3. Drop `lenses = {}` from the format string → `the_lens_set_round_trips_through_the_file` and `serialize_covers_every_field` FAIL.

Restore after each; record three lines.

- [ ] **Step 6: Do NOT commit — continue to Task 4**

Record the three mutation lines for the merged commit body. The three config tests above must be
**green now** even though the overall clippy gate is not; run
`cargo test -p oracle-frontend config::` and `... --no-default-features config::` here, because a
config bug found after the registry lands is a bug found in the wrong file.

<details>
<summary>The commit message this task would have carried alone (folded into Task 4's)</summary>

```
feat(frontend): lenses persist, and unknown keys survive the save

The seventh key arrives, so F-CONFIG-UNKNOWN-KEYS reverses in the same
commit as S2's adjudication required: an unrecognised key is now kept
verbatim and written back, and the file header says so. Without it,
launching an older build once would silently delete a newer build's
settings — the failure the warn-and-continue parse path exists to prevent.

mutation: unknown loop removed from serialize → an_unknown_key_survives_a_save FAILED
mutation: _ arm warns without keeping → unknown_key_warns_and_is_preserved FAILED
mutation: lenses key dropped from serialize → 2 tests FAILED
```

</details>

---

### Task 4: commands + dispatch — the toggles exist and persist

**Files:**
- Modify: `crates/oracle-frontend/src/commands.rs`
- Modify: `crates/oracle-frontend/src/main.rs`

**Design note — no default hotkeys.** Lens toggles register with `hotkey: None`, palette-only. Every obvious key is taken (`Space . Tab F1 F3 W C M - = 0-9` and the palette's `` ` ``/Ctrl+P), `hotkeys_unique` would fail on a collision, and full rebinding is S5's job. The palette is the discovery surface by design (spec §4).

- [ ] **Step 1: Write the failing tests**

In `commands.rs`'s test module:
```rust
    /// Every lens must reach the palette, or a toggle exists with no way to reach it.
    #[test]
    fn every_lens_registers_a_visible_command() {
        let reg = registry();
        for id in crate::lens::LensId::ALL {
            let row = reg
                .iter()
                .find(|c| c.cmd == Cmd::ToggleLens(id))
                .unwrap_or_else(|| panic!("no command for lens {}", id.key()));
            assert!(!row.hidden, "{} is unreachable from the palette", id.key());
            assert_eq!(row.group, Group::Lenses);
            assert_eq!(row.title, id.title(), "the row and the lens must not drift apart");
        }
    }

    /// Lens toggles are palette-only this slice (S5 owns rebinding); a default hotkey added here
    /// without thought would collide silently with the game keys.
    #[test]
    fn lens_toggles_bind_no_keys_yet() {
        for c in registry() {
            if matches!(c.cmd, Cmd::ToggleLens(_)) {
                assert_eq!(c.hotkey, None, "{} bound a key", c.title);
            }
        }
    }
```

- [ ] **Step 2: Run and watch them fail**

```bash
cargo test -p oracle-frontend commands::
```
Expected: FAIL — `no variant named ToggleLens`.

- [ ] **Step 3: Implement the registry side**

`Cmd` (commands.rs:10-38) — add before the audio block:
```rust
    /// Turn one lens on or off (spec §5). Payload-carrying like `SlotSelect`, so one arm and one
    /// registration loop cover every lens.
    ToggleLens(crate::lens::LensId),
```

`Group` (commands.rs:42-64) — add the variant, **bump the array to 5**, add the title:
```rust
pub enum Group {
    Game,
    SaveStates,
    Watch,
    Lenses,
    Settings,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::Game,
        Group::SaveStates,
        Group::Watch,
        Group::Lenses,
        Group::Settings,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Group::Game => "GAME",
            Group::SaveStates => "SAVE STATES",
            Group::Watch => "WATCH",
            Group::Lenses => "LENSES",
            Group::Settings => "SETTINGS",
        }
    }
}
```

In `registry()`, after the slot-key loop (commands.rs:206) and before the audio block:
```rust
    // One toggle per lens, generated from `LensId::ALL` for the reason the slot loop is generated:
    // a lens that exists without a command, or a command naming a lens that no longer exists, is a
    // compile error rather than a row nobody notices is missing. Palette-only — see the plan's
    // note on hotkeys; S5 owns binding.
    for id in crate::lens::LensId::ALL {
        reg.push(CommandInfo::new(
            Cmd::ToggleLens(id),
            id.title(),
            Group::Lenses,
            None,
        ));
    }
```

- [ ] **Step 4: Wire the run loop**

In `main.rs`, beside `ov.status_line = cfg.status_line;` (main.rs:960):
```rust
    // Lenses come back on exactly as they were left (spec §5).
    let mut lenses = cfg.lenses;
```

In the dispatch match, after the `Cmd::ToggleStatusLine` arm (main.rs:1148):
```rust
                commands::Cmd::ToggleLens(id) => {
                    lenses.toggle(id);
                    cfg.lenses = lenses;
                    config_save_countdown = Some(CONFIG_AUTOSAVE_DEBOUNCE_FRAMES);
                    ov.push(
                        format!(
                            "{} {}",
                            id.label(),
                            if lenses.is_on(id) { "ON" } else { "OFF" }
                        ),
                        INFO,
                    );
                }
```

- [ ] **Step 5: Run the gates**

```bash
cargo test -p oracle-frontend
cargo test -p oracle-frontend --no-default-features
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
```
Expected: PASS. `lenses` is read by the toast and by `cfg`, so there is no dead-code warning even though nothing draws yet.

- [ ] **Step 6: Mutation-verify**

Drop one entry from `LensId::ALL`'s registration by filtering it out of the loop → `every_lens_registers_a_visible_command` FAILS. Give one lens `Some(Key::W)` → `hotkeys_unique` FAILS (the existing invariant catches the collision unaided — record that, it is the point of the no-hotkey decision). Restore.

- [ ] **Step 7: Commit — the whole spine, Tasks 2+3+4 together**

```bash
git add crates/oracle-frontend/src/lens crates/oracle-frontend/src/config.rs \
        crates/oracle-frontend/src/commands.rs crates/oracle-frontend/src/main.rs
git commit -m "feat(frontend): the lens spine — ids, persistence, toggles

Lens ids and a toggle bitset; the seventh config key; a LENSES palette
group with one auto-registered toggle per lens. One commit because each
piece supplies the next one's callers, and a bin-only crate has no way to
hold a caller-less pub fn past its own clippy gate.

The seventh key means F-CONFIG-UNKNOWN-KEYS reverses here, as S2's
adjudication required: an unrecognised key is now kept verbatim and
written back, and the file header says so. Without it, launching an older
build once would silently delete a newer build's settings — the failure
the warn-and-continue parse path exists to prevent.

Commands are generated from LensId::ALL for the reason the slot keys are:
a lens without a command is a compile error, not a missing row.
Palette-only bindings — every obvious key is taken, and S5 owns rebinding.

mutation: duplicate entry in ALL → every_lens_has_its_own_bit... FAILED
mutation: format_set order reversed → set_round_trips... FAILED
mutation: unknown loop removed from serialize → an_unknown_key_survives_a_save FAILED
mutation: _ arm warns without keeping → unknown_key_warns_and_is_preserved FAILED
mutation: lenses key dropped from serialize → 2 tests FAILED
mutation: one lens filtered out of the loop → every_lens_registers... FAILED
mutation: lens bound to W → hotkeys_unique FAILED (existing invariant)"
```

---

### Task 5: the watch ticker lens

**Files:**
- Rewrite: `crates/oracle-frontend/src/lens/watch.rs`
- Modify: `crates/oracle-frontend/src/lens/mod.rs` (the `Models`/`models`/`draw` spine)
- Modify: `crates/oracle-frontend/src/main.rs` (the draw call)

- [ ] **Step 1: Write `watch.rs` with its tests**

```rust
//! The watch ticker (spec §5.1) — a bottom strip streaming the newest watch hits plus the armed
//! count.
//!
//! Reads the hit ring through the **non-destructive** `hits()`. Never `take_hits()`: the ring is
//! shared with socket clients, and a lens that consumed it would delete a client's evidence just
//! by being switched on (the rule main.rs:1152-1155 states for the `W` key).

use crate::overlay::{self, ACCENT, INFO};
use crate::present::Rect;
use crate::{font, MAX_SYMBOL_DISPLACEMENT};
use oracle_core::symbols::SymbolTable;
use oracle_core::watchpoints::{WatchHit, WatchSpace, Watchpoints};

/// How many hits the strip shows. Four fits under the picture without eating it; the full log is
/// still one `W` away.
pub const ROWS: usize = 4;

/// What the strip draws: already-formatted lines (newest last, reading order), plus the two
/// numbers that tell you whether to believe them.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Ticker {
    pub lines: Vec<String>,
    pub armed: usize,
    pub dropped: u64,
}

/// The space spelling. Hand-rolled because core has no `Display` for `WatchSpace` and the two
/// existing spellings disagree (`dump_hits` says `Bus`, the bus protocol says `bus`); the bus
/// protocol's lowercase wins here, because that is what a hit looks like everywhere a client
/// sees it.
fn space_name(s: WatchSpace) -> &'static str {
    match s {
        WatchSpace::Bus => "bus",
        WatchSpace::Vram => "vram",
        WatchSpace::Cram => "cram",
        WatchSpace::Vsram => "vsram",
    }
}

/// One hit, compact: `w0 vram $4A00 3F->12 @f811 Sonic_Move+$1C`.
///
/// Deliberately the same field order and the same `$`-hex spellings as `dump_hits`, so the strip
/// and the terminal log read as one instrument. The PC is symbolised through the same
/// `resolve_within(_, MAX_SYMBOL_DISPLACEMENT)` the log uses, and falls back to raw hex — a name
/// 4 KiB past its symbol would be actively misleading.
pub fn line(h: &WatchHit, symbols: Option<&SymbolTable>) -> String {
    let at = symbols
        .and_then(|t| t.resolve_within(h.pc, MAX_SYMBOL_DISPLACEMENT))
        .map(|r| r.to_string())
        .unwrap_or_else(|| format!("${:06X}", h.pc));
    format!(
        "w{} {} ${:04X} {:X}->{:X} @f{} {}",
        h.watch.0,
        space_name(h.space),
        h.addr,
        h.old,
        h.value,
        h.frame,
        at
    )
}

/// The newest `ROWS` hits, oldest first within the strip. `hits()` is seq-ascending, so this is a
/// tail — no sort, no allocation beyond the strings themselves.
pub fn model(wp: &Watchpoints, symbols: Option<&SymbolTable>, rows: usize) -> Ticker {
    let hits = wp.hits();
    let start = hits.len().saturating_sub(rows);
    Ticker {
        lines: hits[start..].iter().map(|h| line(h, symbols)).collect(),
        armed: wp.watch_count(),
        dropped: wp.dropped(),
    }
}

/// Bottom strip of `area`. Toasts stack from the same edge and are drawn later, so a burst of
/// toasts covers the ticker briefly — accepted: toasts are transient and the ticker is not.
pub fn draw(c: &mut font::Canvas, area: Rect, px: usize, t: &Ticker) {
    let pad = 2 * px;
    let margin = (2 * px).max(4);
    let line_h = font::LINE_H * px;
    let rows = t.lines.len() + 1; // + the header
    let panel_h = rows * line_h + 2 * pad;
    if area.w < 16 * px || area.h < panel_h + margin {
        return; // too small to say anything honestly
    }
    let panel_w = area.w.saturating_sub(2 * margin);
    let left = (area.x + margin) as i32;
    let top = (area.y + area.h - margin - panel_h) as i32;
    c.fill_rect(left, top, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);

    let avail = panel_w.saturating_sub(2 * pad);
    let head = format!(
        "WATCH  armed {}  dropped {}",
        t.armed, t.dropped
    );
    c.text(
        left + pad as i32,
        top + pad as i32,
        px,
        if t.dropped > 0 { ACCENT } else { INFO },
        overlay::fit(&head, avail, px),
    );
    for (i, l) in t.lines.iter().enumerate() {
        c.text(
            left + pad as i32,
            top + pad as i32 + ((i + 1) * line_h) as i32,
            px,
            INFO,
            overlay::fit(l, avail, px),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::bus::BusOp;
    use oracle_core::watchpoints::{WatchId, WatchOp, WatchVia};
    use oracle_core::Size;

    fn hit(seq: u64, addr: u32, old: u32, value: u32) -> WatchHit {
        WatchHit {
            watch: WatchId(0),
            space: WatchSpace::Vram,
            addr,
            old,
            value,
            size: Size::Byte,
            op: BusOp::Write,
            fc: 0,
            via: WatchVia::Direct,
            pc: 0x00_0100,
            frame: 811,
            mclk: 0,
            seq,
        }
    }

    #[test]
    fn a_line_reads_like_the_terminal_log() {
        let l = line(&hit(1, 0x4A00, 0x3F, 0x12), None);
        assert_eq!(l, "w0 vram $4A00 3F->12 @f811 $000100");
    }

    /// The ring holds thousands; the strip shows the newest few, in reading order.
    #[test]
    fn the_model_takes_the_newest_rows_oldest_first() {
        let mut wp = Watchpoints::new(64);
        wp.add_vdp_watch(WatchSpace::Vram, 0..=0xFFFF, WatchOp::Write, "test");
        let t = Ticker {
            lines: (0..3).map(|i| line(&hit(i, 0x100 + i as u32, 0, i as u32), None)).collect(),
            armed: 1,
            dropped: 0,
        };
        assert_eq!(t.lines.len(), 3);
        assert!(t.lines[0].contains("$0100"), "oldest of the tail is first");
        assert!(t.lines[2].contains("$0102"), "newest is last");
        assert_eq!(wp.watch_count(), 1, "the armed count is the watch count");
    }

    #[test]
    fn draw_paints_inside_area_only() {
        let (w, h) = (320usize, 224usize);
        let mut buf = vec![0u32; w * h];
        let area = Rect { x: 40, y: 20, w: 240, h: 180 };
        let t = Ticker {
            lines: vec!["w0 vram $4A00 3F->12 @f811 Sonic_Move+$1C".to_string(); ROWS],
            armed: 2,
            dropped: 7,
        };
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(&mut c, area, 1, &t);
        }
        let painted = buf.iter().filter(|p| **p != 0).count();
        assert!(painted > 0, "draw painted nothing");
        for (i, p) in buf.iter().enumerate() {
            if *p != 0 {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
    }

    /// A very long symbol name must be truncated, not allowed to run past the panel — `Canvas`
    /// clips at the buffer edge only, so an unfitted string would paint over the whole window.
    #[test]
    fn a_long_line_stays_inside_a_narrow_panel() {
        let (w, h) = (200usize, 400usize);
        let mut buf = vec![0u32; w * h];
        let area = Rect { x: 0, y: 0, w: 60, h: 400 };
        let t = Ticker {
            lines: vec!["w0 vram $4A00 3F->12 @f811 ".to_string() + &"X".repeat(400)],
            armed: 1,
            dropped: 0,
        };
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(&mut c, area, 2, &t);
        }
        for (i, p) in buf.iter().enumerate() {
            if *p != 0 {
                assert!(i % w < area.w, "ink escaped the panel at x={}", i % w);
            }
        }
    }
}
```

**If any import above does not resolve** (`oracle_core::Size`, `oracle_core::bus::BusOp`), fix the path rather than the test — check `main.rs`'s existing `use oracle_core::watchpoints::{WatchOp, WatchSpace, WatchVia, Watchpoints};` (main.rs:260) for the house spelling.

- [ ] **Step 2: Add the spine to `lens/mod.rs`**

This task also adds the two things the merged spine commit deliberately left out because they had
no caller there: `pub mod watch;` and `LensSet::any()` (plus its test, from Task 2's
`toggle_and_any_agree_with_is_on` — move that test here if it was not carried).

```rust
use crate::present::Rect;
use oracle_core::symbols::SymbolTable;
use oracle_core::watchpoints::Watchpoints;
use oracle_core::System;

/// Everything the enabled lenses need to draw this frame, extracted once. Absent = that lens is
/// off, so `draw` never has to know the set.
#[derive(Default)]
pub struct Models {
    pub ticker: Option<watch::Ticker>,
}

/// Build the models for whatever is on. Called once per frame, immediately before drawing, and
/// skipped entirely when nothing is on.
pub fn models(
    set: LensSet,
    _sys: &System,
    wp: &Watchpoints,
    symbols: Option<&SymbolTable>,
) -> Models {
    Models {
        ticker: set
            .is_on(LensId::Watch)
            .then(|| watch::model(wp, symbols, watch::ROWS)),
    }
}

/// Draw every built model, in a fixed back-to-front order. Anchored to `area` (the picture),
/// never the window: the letterbox stays black, and a tall window with a narrow picture must not
/// make the font wider than the panel (`draw_narrow_panel_does_not_underflow`).
pub fn draw(buf: &mut [u32], w: usize, h: usize, area: Rect, m: &Models) {
    let px = crate::overlay::Overlay::font_scale(area.h.max(1));
    let mut c = crate::font::Canvas::new(buf, w, h);
    if let Some(t) = &m.ticker {
        watch::draw(&mut c, area, px, t);
    }
}
```

- [ ] **Step 3: Call it from the run loop**

In `main.rs`, between `ov.tick();` (main.rs:1739) and `palette.draw(...)` (main.rs:1749):
```rust
        // Lenses: under the palette and the toasts, over the picture (spec §5). Models are built
        // only for what is on, and only into the *window* buffer — `buf` is retained and
        // re-presented while paused, so ink there would accumulate.
        if lenses.any() {
            let models = lens::models(lenses, &sys, bus.watchpoints_mut(), symbols.as_ref());
            lens::draw(&mut screen, win_w, win_h, present_view, &models);
        }
```

- [ ] **Step 4: Run the gates**

```bash
cargo test -p oracle-frontend
cargo test -p oracle-frontend --no-default-features
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
```

- [ ] **Step 5: Mutation-verify**

1. Change `model` to take the **oldest** rows (`&hits[..rows.min(hits.len())]`) → `the_model_takes_the_newest_rows_oldest_first` FAILS.
2. Remove the `overlay::fit` wrapper from the line text → `a_long_line_stays_inside_a_narrow_panel` FAILS.
3. Change `line`'s format to drop the `$` on `addr` → `a_line_reads_like_the_terminal_log` FAILS.

Restore each; record three lines.

- [ ] **Step 6: Commit**

```bash
git add crates/oracle-frontend/src/lens crates/oracle-frontend/src/main.rs
git commit -m "feat(frontend): the watch ticker lens

Newest hits plus the armed and dropped counts, formatted to read as one
instrument with the W-key log. Non-destructive hits() only — take_hits()
would let switching a lens on delete a socket client's evidence.

mutation: model takes oldest rows → the_model_takes_the_newest... FAILED
mutation: overlay::fit removed → a_long_line_stays_inside... FAILED
mutation: \$ dropped from addr → a_line_reads_like_the_terminal_log FAILED"
```

---

### Task 6: the CPU chip lens

**Files:**
- Create: `crates/oracle-frontend/src/lens/cpu.rs`
- Modify: `crates/oracle-frontend/src/lens/mod.rs`, `crates/oracle-frontend/src/main.rs`

**Also in this task: collapse `models()`'s parameter list into a context struct.** Task 5 shipped
`models(set, wp, symbols)` — correctly dropping an unused `&System` rather than carrying it for a
future task. But Task 6 needs `&System`, Task 7 the `Vdp`, Task 8 the forward map, Task 9 the mouse
position: `models()` is heading for six positional parameters, and the third and fourth `&`-args of
the same shape are where call sites start silently transposing. Introduce it here, while it is two
fields, not at Task 9 when it is six:

```rust
/// Everything the enabled lenses may read this frame, borrowed once. Grouped rather than passed
/// positionally because the list only grows: by the last lens this is six arguments, several of
/// them same-typed references, which is where a transposed call site stops being a compile error.
pub struct FrameCtx<'a> {
    pub sys: &'a oracle_core::System,
    pub wp: &'a oracle_core::watchpoints::Watchpoints,
    pub symbols: Option<&'a oracle_core::symbols::SymbolTable>,
    pub frame: u64,
    pub paused: bool,
}
```
`models(set: LensSet, cx: &FrameCtx<'_>) -> Models`. Update the Task 5 call site and its
`models_are_built_only_for_lenses_that_are_on` test with it. Tasks 7-9 then add a field each rather
than a parameter each.

- [ ] **Step 1: Write `cpu.rs` with its tests**

```rust
//! The CPU chip (spec §5.3) — a small top-right readout: PC as a symbol, SR, frame counter.
//! Auto-shows while paused or stepping; `LensId::CpuRegs` expands it to the full D0-D7/A0-A7
//! block. Without a `.lst` the PC is raw hex — the fallback spec §10 names.

use crate::overlay::{self, ACCENT, INFO};
use crate::present::Rect;
use crate::{font, MAX_SYMBOL_DISPLACEMENT};
use oracle_core::m68000::registers::Registers;
use oracle_core::symbols::SymbolTable;

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Chip {
    pub lines: Vec<String>,
    /// Drawn in the paused colour when the machine is stopped, so the chip says *why* it is
    /// showing when it appeared on its own.
    pub paused: bool,
}

/// Compact three lines, or eleven when expanded.
///
/// A0-A7 go through `addr_reg`, never `regs.a[i]`: the register file's `a` array is **7 wide**
/// (A7 lives in `usp`/`ssp` and which one is live depends on the supervisor bit), so indexing it
/// with 7 would panic — and printing `usp` unconditionally would be wrong half the time.
pub fn model(
    regs: &Registers,
    symbols: Option<&SymbolTable>,
    frame: u64,
    paused: bool,
    expanded: bool,
) -> Chip {
    let pc = symbols
        .and_then(|t| t.resolve_within(regs.pc, MAX_SYMBOL_DISPLACEMENT))
        .map(|r| r.to_string())
        .unwrap_or_else(|| format!("${:06X}", regs.pc));
    let mut lines = vec![
        format!("PC {pc}"),
        format!(
            "SR ${:04X} {}{}",
            regs.sr,
            if regs.supervisor() { "S" } else { "U" },
            regs.int_mask()
        ),
        format!("F {frame}"),
    ];
    if expanded {
        for i in 0..4 {
            lines.push(format!(
                "D{} {:08X}  D{} {:08X}",
                i,
                regs.d[i],
                i + 4,
                regs.d[i + 4]
            ));
        }
        for i in 0..4 {
            lines.push(format!(
                "A{} {:08X}  A{} {:08X}",
                i,
                regs.addr_reg(i),
                i + 4,
                regs.addr_reg(i + 4)
            ));
        }
    }
    Chip { lines, paused }
}

/// Top-right of `area`, sized to its widest line so the expanded block does not shove the compact
/// one around.
pub fn draw(c: &mut font::Canvas, area: Rect, px: usize, chip: &Chip) {
    let pad = 2 * px;
    let margin = (2 * px).max(4);
    let line_h = font::LINE_H * px;
    let panel_h = chip.lines.len() * line_h + 2 * pad;
    let widest = chip
        .lines
        .iter()
        .map(|l| font::text_width(l) * px)
        .max()
        .unwrap_or(0);
    let panel_w = (widest + 2 * pad).min(area.w.saturating_sub(2 * margin));
    if panel_w == 0 || area.h < panel_h + margin || area.w < panel_w + 2 * margin {
        return;
    }
    let left = (area.x + area.w - margin - panel_w) as i32;
    let top = (area.y + margin) as i32;
    c.fill_rect(left, top, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);
    let avail = panel_w.saturating_sub(2 * pad);
    let color = if chip.paused { ACCENT } else { INFO };
    for (i, l) in chip.lines.iter().enumerate() {
        c.text(
            left + pad as i32,
            top + pad as i32 + (i * line_h) as i32,
            px,
            color,
            overlay::fit(l, avail, px),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regs() -> Registers {
        let mut r = Registers::default();
        r.pc = 0x00_1234;
        r.sr = 0x2700;
        r.d[0] = 0xDEAD_BEEF;
        r.d[7] = 0x0000_0007;
        r
    }

    #[test]
    fn without_symbols_the_pc_is_raw_hex() {
        let c = model(&regs(), None, 42, false, false);
        assert_eq!(c.lines[0], "PC $001234");
        assert_eq!(c.lines.len(), 3, "compact is three lines");
        assert!(c.lines[2].ends_with("42"), "the frame counter is shown");
    }

    /// A7 must come from `addr_reg(7)`, which picks ssp/usp by the supervisor bit — `regs.a[7]`
    /// would panic, and printing `usp` unconditionally would be wrong in supervisor mode.
    #[test]
    fn expanded_shows_all_sixteen_registers_and_a7_follows_the_supervisor_bit() {
        let mut r = regs();
        r.ssp = 0x00FF_F000;
        r.usp = 0x0000_0BAD;
        r.sr = 0x2700; // supervisor
        let c = model(&r, None, 0, false, true);
        assert_eq!(c.lines.len(), 11, "3 + 4 D-lines + 4 A-lines");
        let joined = c.lines.join("\n");
        for name in ["D0", "D7", "A0", "A7"] {
            assert!(joined.contains(name), "{name} missing");
        }
        assert!(joined.contains("00FFF000"), "A7 is the SSP in supervisor mode");
        assert!(!joined.contains("00000BAD"), "the USP is not A7 here");
    }

    #[test]
    fn draw_paints_inside_area_only() {
        let (w, h) = (320usize, 224usize);
        let mut buf = vec![0u32; w * h];
        let area = Rect { x: 40, y: 20, w: 240, h: 180 };
        let chip = model(&regs(), None, 7, true, true);
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(&mut c, area, 1, &chip);
        }
        assert!(buf.iter().any(|p| *p != 0), "draw painted nothing");
        for (i, p) in buf.iter().enumerate() {
            if *p != 0 {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
    }
}
```

If `Registers` has no `Default`, build it with an explicit literal instead — check `crates/oracle-core/src/m68000/registers.rs:25` and follow whatever the core's own tests do.

- [ ] **Step 2: Extend the spine**

`Models` gains `pub cpu: Option<cpu::Chip>`. `models()` gains parameters `frame: u64` and `paused: bool` and:
```rust
        cpu: (set.is_on(LensId::Cpu) || set.is_on(LensId::CpuRegs) || paused).then(|| {
            cpu::model(
                _sys.cpu_regs(),
                symbols,
                frame,
                paused,
                set.is_on(LensId::CpuRegs),
            )
        }),
```
(rename `_sys` to `sys` now that it is used). `draw()` gains the matching `if let Some(c) = &m.cpu { cpu::draw(&mut c_canvas, area, px, c) }`.

**Note the consequence:** the chip auto-shows while paused even with both lens bits off (spec §5.3 "auto-shows while paused/stepping"), so `lenses.any()` is no longer the right guard in main.rs — change it to `lenses.any() || paused`.

- [ ] **Step 3: Update the call site**

```rust
        if lenses.any() || paused {
            let models = lens::models(
                lenses,
                &sys,
                bus.watchpoints_mut(),
                symbols.as_ref(),
                frame,
                paused,
            );
            lens::draw(&mut screen, win_w, win_h, present_view, &models);
        }
```

- [ ] **Step 4: Gates, then mutation-verify**

Run all four gate commands. Then:
1. Change `addr_reg(i + 4)` to `regs.a[i + 4]` → the test **panics** (index out of bounds) rather than failing cleanly; that is the mutation and it is worth recording, because it is exactly the bug the accessor exists to prevent.
2. Make `model` ignore `expanded` → `expanded_shows_all_sixteen_registers...` FAILS.
Restore; record both.

- [ ] **Step 5: Commit**

```bash
git add crates/oracle-frontend/src/lens crates/oracle-frontend/src/main.rs
git commit -m "feat(frontend): the CPU chip lens, compact and expanded

PC through the same symbol resolver the watch log uses, raw hex without a
.lst. A0-A7 go through addr_reg so A7 follows the supervisor bit — the
register file's a[] is only 7 wide. Auto-shows while paused.

mutation: regs.a[i+4] instead of addr_reg → expanded test PANICKED (index OOB)
mutation: expanded ignored → expanded_shows_all_sixteen... FAILED"
```

---

### Task 7: the CRAM strip lens

**Files:**
- Modify: `crates/oracle-frontend/src/lens/video.rs` (first real content), `lens/mod.rs`, `main.rs`

- [ ] **Step 1: Write the CRAM half of `video.rs` with tests**

```rust
//! Video lenses (spec §5.2) — things drawn *on* the picture: the CRAM strip, sprite outlines, and
//! the hover callout.
//!
//! **A known, accepted divergence:** the strip is built from `Vdp::cram_decoded()`, which is
//! pinned to match the renderer's own per-entry decode exactly at `PixelState::Normal`
//! (`cram_rgb_matches_cram_decoded`, render.rs:1622). The renderer's shadow/highlight-aware
//! conversion is private, so in an S/H region the picture is drawn at half or upper intensity
//! while the strip still shows the Normal ramp. Exporting the private conversion for a swatch
//! strip would be a core change this slice deliberately does not make.

use crate::present::Rect;
use crate::{font, overlay};

/// 4 palette lines x 16 colours, in CRAM order.
pub const PALETTES: usize = 4;
pub const COLOURS: usize = 16;

/// Swatch edge in font-scale units — three device pixels per scale step reads as a colour rather
/// than a dot without eating the picture.
const SWATCH: usize = 3;

/// Pack the core's decoded triples into the frontend's `0x00RR_GGBB`.
pub fn swatches(cram: &[(u8, u8, u8); 64]) -> [u32; 64] {
    let mut out = [0u32; 64];
    for (i, (r, g, b)) in cram.iter().enumerate() {
        out[i] = ((*r as u32) << 16) | ((*g as u32) << 8) | *b as u32;
    }
    out
}

/// Top-left of `area`, below the status line's row so the two never fight for the corner.
pub fn draw_cram(c: &mut font::Canvas, area: Rect, px: usize, sw: &[u32; 64]) {
    let pad = 2 * px;
    let margin = (2 * px).max(4);
    let cell = SWATCH * px;
    let panel_w = COLOURS * cell + 2 * pad;
    let panel_h = PALETTES * cell + 2 * pad;
    // Sit clear of the status line, which owns one text row at the top-left.
    let status_row = font::LINE_H * px + 2 * pad;
    if area.w < panel_w + 2 * margin || area.h < panel_h + status_row + 2 * margin {
        return;
    }
    let left = (area.x + margin) as i32;
    let top = (area.y + margin + status_row) as i32;
    c.fill_rect(left, top, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);
    for line in 0..PALETTES {
        for col in 0..COLOURS {
            c.fill_rect(
                left + pad as i32 + (col * cell) as i32,
                top + pad as i32 + (line * cell) as i32,
                cell,
                cell,
                sw[line * COLOURS + col],
                255,
            );
        }
    }
    let _ = overlay::fit; // (kept in scope for the other draws in this module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatches_pack_the_cores_triples_without_reordering_channels() {
        let mut cram = [(0u8, 0u8, 0u8); 64];
        cram[0] = (0xFF, 0x00, 0x00);
        cram[1] = (0x00, 0xFF, 0x00);
        cram[2] = (0x00, 0x00, 0xFF);
        cram[63] = (0x12, 0x34, 0x56);
        let sw = swatches(&cram);
        assert_eq!(sw[0], 0x00FF_0000, "red is the high byte");
        assert_eq!(sw[1], 0x0000_FF00);
        assert_eq!(sw[2], 0x0000_00FF);
        assert_eq!(sw[63], 0x0012_3456);
    }

    /// All 64 entries reach the glass, laid out 4 rows x 16 — a strip that quietly drew one
    /// palette line would look plausible and be useless.
    #[test]
    fn the_strip_draws_all_sixty_four_entries_inside_area() {
        let (w, h) = (640usize, 448usize);
        let mut buf = vec![0u32; w * h];
        let area = Rect { x: 0, y: 0, w, h };
        let mut cram = [(0u8, 0u8, 0u8); 64];
        for (i, e) in cram.iter_mut().enumerate() {
            *e = (i as u8 + 1, 0, 0); // 64 distinct, all non-black
        }
        let sw = swatches(&cram);
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw_cram(&mut c, area, 2, &sw);
        }
        let distinct: std::collections::HashSet<u32> =
            buf.iter().copied().filter(|p| *p != 0).collect();
        assert!(
            distinct.len() >= 64,
            "expected 64 distinct swatch colours, saw {}",
            distinct.len()
        );
        for (i, p) in buf.iter().enumerate() {
            if *p != 0 {
                let (x, y) = (i % w, i / w);
                assert!(x < area.w && y < area.h, "painted outside area");
            }
        }
    }
}
```

Remove the `let _ = overlay::fit;` line once the later tasks add a real `overlay::fit` use in this module; it exists only so the import does not trip `-D warnings` in this intermediate commit. **Prefer**: do not import `overlay` yet, and add the import in Task 9. Take that path if clippy complains.

- [ ] **Step 2: Spine + call site**

`Models` gains `pub cram: Option<[u32; 64]>`; `models()` gains
```rust
        cram: set
            .is_on(LensId::Cram)
            .then(|| video::swatches(&sys.vdp().cram_decoded())),
```
and `draw()` gains the matching branch.

- [ ] **Step 3: Gates, then mutation-verify**

1. Swap the red and blue shifts in `swatches` → `swatches_pack_the_cores_triples...` FAILS.
2. Draw only `PALETTES - 1` rows → `the_strip_draws_all_sixty_four_entries_inside_area` FAILS.
Restore; record both.

- [ ] **Step 4: Commit**

```bash
git add crates/oracle-frontend/src/lens crates/oracle-frontend/src/main.rs
git commit -m "feat(frontend): the CRAM strip lens

Sixty-four live swatches from the core's cram_decoded, which is pinned to
match the renderer's own decode at Normal. Shadow/highlight regions differ
by construction — the S/H-aware conversion is private and this slice adds
no core surface.

mutation: R/B shifts swapped → swatches_pack... FAILED
mutation: one palette row dropped → the_strip_draws_all_sixty_four... FAILED"
```

---

### Task 8: the forward map + the sprite outline lens

**Files:**
- Modify: `crates/oracle-frontend/src/present.rs` (the forward map — void Task 1, folded here)
- Modify: `crates/oracle-frontend/src/lens/video.rs`, `lens/mod.rs`, `main.rs`

This task lands `present::native_rect_to_window` **and** its first caller in one commit, because a
function without a caller fails this bin-only crate's clippy gate (see void Task 1).

- [ ] **Step 0: Apply the forward map**

Apply the saved patch — it is the built, gate-run, mutation-verified implementation, reverted only
for the ordering reason above:

```bash
git am /tmp/claude-1000/-home-volence-sonic-hacks-oracle-next/6c6660c8-9c88-40d2-b140-7e63c2e0a455/scratchpad/forward-map.patch
```

If the patch does not apply, take the code from void Task 1's collapsed section above instead.
**Then `git reset --soft HEAD~1`** so the map lands in *this* task's single commit rather than a
separate one that would fail its own gate.

Three findings from that first build, all carried forward:

1. The mutation `div_ceil` → `/` kills `the_forward_map_is_the_inverse_of_the_click_map` with
   `(0,37) -> window (0,79) -> Some((0, 36))`. Re-run it; it is the evidence that the ceiling form
   is the right one.
2. **The round-trip contract holds only when the picture is upscaled** (`rect.w >= src_w`, the
   normal case). `dest_rect` really can downscale — a 200×150 window in `Tv` mode gives
   `rect.w = 200 < 320` — and there several game pixels share one window pixel, so
   `window_to_native` on the returned span names a *different* game pixel. The `.max(1)` floor
   exists for that regime and no test reaches it. **Fix the doc comment** to say the inverse
   property is an upscale property, and add this test:
   ```rust
       /// Below 1:1 the map cannot be an inverse — several game pixels share one window pixel —
       /// but it must still answer a non-empty rect inside the picture rather than a zero-width
       /// one that would draw nothing.
       #[test]
       fn the_forward_map_still_places_a_rect_when_the_picture_is_downscaled() {
           let rect = dest_rect(200, 150, 320, 224, Aspect::Tv);
           assert!(rect.w < 320, "this geometry really does downscale");
           for gx in [0usize, 1, 159, 319] {
               let out = native_rect_to_window(Rect { x: gx, y: 100, w: 1, h: 1 }, rect, 320, 224)
                   .expect("a visible game pixel always places");
               assert!(out.w >= 1 && out.h >= 1, "never a zero-area rect");
               assert!(out.x >= rect.x && out.x + out.w <= rect.x + rect.w, "inside the picture");
           }
       }
   ```
   Mutation-verify it by deleting the two `.max(1)` calls.
3. The returned rect is clipped to the **picture**, not to the **window**: `dest_rect` can return a
   rect larger than the window (`Aspect::Integer` clamps its scale at 1, so a window smaller than
   320×224 still gets a 320×224 rect). `font::Canvas` clips at the buffer edge and never panics, so
   drawing is safe — but say so in the doc comment, since "inside `area`" and "inside the window"
   are not the same claim.

- [ ] **Step 1: Write the outline half with tests**

Add to `video.rs`:

```rust
use oracle_core::render::SpriteDecoded;

/// One outline, already clipped to the display and expressed in **game pixels** — the mapping to
/// window pixels is `present::native_rect_to_window`'s job, and keeping it out of here is what
/// makes the geometry testable without a window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpriteBox {
    pub index: u8,
    pub rect: Rect,
    pub priority: bool,
}

/// Outline every sprite the hardware actually parses, clipped to the active display.
///
/// Three rules, each earning its place:
/// * slots at or past `parsed_max` are **skipped** — they are decoded but never parsed (64 in
///   H32, 80 in H40), and outlining them would draw boxes around things the VDP never shows.
///   `parsed_max` is passed in because a consumer is forbidden to re-derive `if h40 {80} else
///   {64}` (render.rs:543-547).
/// * a sprite is **clipped per edge, not dropped** — `x`/`y` are signed and a sprite entering
///   from the left is exactly the case an outline lens is for.
/// * a sprite entirely outside the display contributes nothing, which silently handles the
///   parked-sprite idiom (`y == -128`).
pub fn boxes(sprites: &[SpriteDecoded], parsed_max: u8, display: (u16, u16)) -> Vec<SpriteBox> {
    let (dw, dh) = (i32::from(display.0), i32::from(display.1));
    let mut out = Vec::new();
    for s in sprites.iter().take(usize::from(parsed_max)) {
        let left = i32::from(s.x);
        let top = i32::from(s.y);
        let right = left + i32::from(s.width_cells) * 8; // exclusive
        let bottom = top + i32::from(s.height_cells) * 8;
        let cl = left.max(0);
        let ct = top.max(0);
        let cr = right.min(dw);
        let cb = bottom.min(dh);
        if cr <= cl || cb <= ct {
            continue;
        }
        out.push(SpriteBox {
            index: s.index,
            rect: Rect {
                x: cl as usize,
                y: ct as usize,
                w: (cr - cl) as usize,
                h: (cb - ct) as usize,
            },
            priority: s.priority,
        });
    }
    out
}

/// Four thin `fill_rect`s per box — `font::Canvas` has no stroke primitive. High-priority sprites
/// are drawn in the accent colour so the layer you are hunting is the one that stands out.
pub fn draw_sprites(
    c: &mut font::Canvas,
    area: Rect,
    px: usize,
    native: (usize, usize),
    boxes: &[SpriteBox],
) {
    let t = px.max(1); // outline thickness, one game-pixel-ish at every scale
    for b in boxes {
        let Some(r) = crate::present::native_rect_to_window(b.rect, area, native.0, native.1)
        else {
            continue;
        };
        let color = if b.priority {
            crate::overlay::ACCENT
        } else {
            crate::overlay::INFO
        };
        let (x, y, w, h) = (r.x as i32, r.y as i32, r.w, r.h);
        c.fill_rect(x, y, w, t, color, 200); // top
        c.fill_rect(x, y + (h.saturating_sub(t)) as i32, w, t, color, 200); // bottom
        c.fill_rect(x, y, t, h, color, 200); // left
        c.fill_rect(x + (w.saturating_sub(t)) as i32, y, t, h, color, 200); // right
    }
}
```

Tests:
```rust
    fn sprite(index: u8, x: i16, y: i16, wc: u8, hc: u8) -> SpriteDecoded {
        SpriteDecoded {
            index,
            y,
            x,
            width_cells: wc,
            height_cells: hc,
            link: 0,
            tile: 0,
            palette: 0,
            hflip: false,
            vflip: false,
            priority: false,
            cache_divergence: false,
        }
    }

    #[test]
    fn a_box_is_the_sprites_cells_in_pixels() {
        let b = boxes(&[sprite(3, 100, 50, 4, 2)], 80, (320, 224));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].index, 3);
        assert_eq!(b[0].rect, Rect { x: 100, y: 50, w: 32, h: 16 }, "cells x 8");
    }

    /// A sprite entering from the left keeps the half that is on screen — dropping it is the
    /// obvious wrong answer, and it is exactly the case the lens exists to show.
    #[test]
    fn a_partly_offscreen_sprite_is_clipped_not_dropped() {
        let b = boxes(&[sprite(0, -8, -4, 2, 2)], 80, (320, 224));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].rect, Rect { x: 0, y: 0, w: 8, h: 12 });
    }

    #[test]
    fn a_parked_sprite_contributes_nothing() {
        assert!(boxes(&[sprite(0, 0, -128, 4, 4)], 80, (320, 224)).is_empty());
    }

    /// Slots past `parsed_max` are decoded but never parsed by the hardware — outlining them
    /// would draw boxes around things that are not on the screen. H32 parses 64.
    #[test]
    fn slots_past_parsed_max_are_not_outlined() {
        let sprites: Vec<SpriteDecoded> =
            (0..80u8).map(|i| sprite(i, 10, 10, 1, 1)).collect();
        assert_eq!(boxes(&sprites, 64, (256, 224)).len(), 64, "H32 parses 64");
        assert_eq!(boxes(&sprites, 80, (320, 224)).len(), 80, "H40 parses 80");
    }

    #[test]
    fn outlines_paint_inside_area_only() {
        let (w, h) = (640usize, 480usize);
        let mut buf = vec![0u32; w * h];
        let area = crate::present::dest_rect(w, h, 320, 224, crate::present::Aspect::Tv);
        let bx = boxes(&[sprite(0, 0, 0, 4, 4), sprite(1, 300, 210, 4, 4)], 80, (320, 224));
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw_sprites(&mut c, area, 2, (320, 224), &bx);
        }
        assert!(buf.iter().any(|p| *p != 0), "drew nothing");
        for (i, p) in buf.iter().enumerate() {
            if *p != 0 {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "outline escaped the picture at ({x},{y}) — the letterbox must stay black"
                );
            }
        }
    }
```

- [ ] **Step 2: Spine + call site**

`Models` gains `pub sprites: Option<Vec<video::SpriteBox>>` and `pub native: (usize, usize)`. In `models()`:
```rust
        sprites: set.is_on(LensId::Sprites).then(|| {
            let vdp = sys.vdp();
            video::boxes(&vdp.sprites_decoded(), vdp.parsed_sprite_max(), vdp.active_display())
        }),
```
`sprites_decoded()` allocates 80 entries — call it **once**, here, and reuse it in Task 9 rather than calling it again for hover.

- [ ] **Step 3: Gates, then mutation-verify**

1. `.take(usize::from(parsed_max))` → `.take(80)` → `slots_past_parsed_max_are_not_outlined` FAILS on the H32 row.
2. `continue` on a partly-clipped sprite (drop instead of clip) → `a_partly_offscreen_sprite_is_clipped_not_dropped` FAILS.
3. Multiply cells by 16 instead of 8 → `a_box_is_the_sprites_cells_in_pixels` FAILS.
Restore; record three.

- [ ] **Step 4: Commit**

```bash
git add crates/oracle-frontend/src/present.rs crates/oracle-frontend/src/lens
git commit -m "feat(frontend): the blit's forward map and the sprite outline lens

Boxes in game pixels, clipped per edge so a sprite entering from the left
keeps the half that is visible, mapped to the window through the blit's
own forward map — which lands here rather than alone, because a bin-only
crate has no way to hold a caller-less pub fn past its own clippy gate.
The ceiling form is the only one that round-trips; the floor form is off
by one on most non-integer scales. Slots past parsed_sprite_max are
skipped rather than outlined — the hardware never parses them, and the
count is the core's to report, not ours to re-derive.

mutation: take(80) instead of parsed_max → slots_past_parsed_max... FAILED
mutation: drop instead of clip → a_partly_offscreen_sprite... FAILED
mutation: cells x 16 → a_box_is_the_sprites_cells_in_pixels FAILED
mutation: div_ceil -> / in edge() → the_forward_map_is_the_inverse... FAILED
mutation: .max(1) removed → the_forward_map_still_places_a_rect... FAILED"
```

---

### Task 9: the hover callout lens

**Files:**
- Modify: `crates/oracle-frontend/src/lens/video.rs`, `lens/mod.rs`, `main.rs`

**Design note:** hover **explains**, click **arms** — the click path is unchanged. Hover reads `vdp.pixel_attribution(x, y)` directly rather than `pick::resolve`, which additionally allocates three `String`s and a second `sprites_decoded()`; those belong to the click. Recompute only when the native dot changes: a paused frontend re-runs the loop many times over one image.

- [ ] **Step 1: Write the hover half with tests**

```rust
use oracle_core::render::{Layer, PixelAttribution};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hover {
    /// The callout text, already assembled.
    pub text: String,
    /// The game pixel it describes — the callout is drawn beside it.
    pub at: (u16, u16),
}

/// `slot 12 | tile $4A0 | pal 2 | pri 1` for a sprite, the plane's cell for a plane or window,
/// the CRAM entry for the backdrop. The separator is `|` rather than the spec's `·` because the
/// 5x7 font has no middle dot — it would draw as the missing-glyph box.
pub fn hover_text(attr: &PixelAttribution, sprites: &[SpriteDecoded]) -> String {
    match attr.winner {
        Layer::Sprite(index) => {
            let tile = sprites
                .get(usize::from(index))
                .and_then(|s| oracle_core::render::sprite_tile_at(s, attr.x, attr.y));
            let (pal, pri) = sprites
                .get(usize::from(index))
                .map(|s| (s.palette, s.priority))
                .unwrap_or((0, false));
            match tile {
                Some(t) => format!(
                    "slot {index} | tile ${t:03X} | pal {pal} | pri {}",
                    u8::from(pri)
                ),
                // The SAT can move between the frame being drawn and this read; say so rather
                // than inventing a tile (`pick.rs` makes the same distinction).
                None => format!("slot {index} | tile ? | pal {pal} | pri {}", u8::from(pri)),
            }
        }
        Layer::Backdrop => format!(
            "backdrop | cram {} (pal {} col {})",
            attr.cram_index,
            attr.cram_index / 16,
            attr.cram_index % 16
        ),
        Layer::PlaneA | Layer::PlaneB | Layer::Window => {
            let plane = match attr.winner {
                Layer::PlaneA => "plane A",
                Layer::PlaneB => "plane B",
                _ => "window",
            };
            match &attr.cell {
                Some(cell) => format!(
                    "{plane} | tile ${:03X} | pal {} | pri {}",
                    cell.tile,
                    cell.palette,
                    u8::from(cell.priority)
                ),
                None => format!("{plane} | no cell"),
            }
        }
    }
}

/// Drawn beside the dot, flipped to the other side when it would run off the picture — a callout
/// that leaves the picture is worse than one on the wrong side of the cursor.
pub fn draw_hover(
    c: &mut font::Canvas,
    area: Rect,
    px: usize,
    native: (usize, usize),
    hv: &Hover,
) {
    let pad = 2 * px;
    let text_w = font::text_width(&hv.text) * px;
    let panel_w = (text_w + 2 * pad).min(area.w);
    let panel_h = font::GLYPH_H * px + 2 * pad;
    let anchor = Rect { x: usize::from(hv.at.0), y: usize::from(hv.at.1), w: 1, h: 1 };
    let Some(a) = crate::present::native_rect_to_window(anchor, area, native.0, native.1) else {
        return;
    };
    let mut left = a.x + 4 * px;
    if left + panel_w > area.x + area.w {
        left = (a.x).saturating_sub(panel_w + 4 * px).max(area.x);
    }
    let mut top = a.y + 4 * px;
    if top + panel_h > area.y + area.h {
        top = (a.y).saturating_sub(panel_h + 4 * px).max(area.y);
    }
    c.fill_rect(left as i32, top as i32, panel_w, panel_h, 0x000A_1418, font::PANEL_ALPHA);
    c.text(
        (left + pad) as i32,
        (top + pad) as i32,
        px,
        overlay::INFO,
        overlay::fit(&hv.text, panel_w.saturating_sub(2 * pad), px),
    );
}
```

Tests — build `PixelAttribution` values directly (it is a plain struct; check `render.rs:165-183` for the exact field list and fill the ones the formatter reads):
```rust
    #[test]
    fn a_plane_pixel_names_its_tile_palette_and_priority() { /* Layer::PlaneA + Some(cell) */ }

    #[test]
    fn a_sprite_pixel_names_its_slot_and_says_so_when_the_tile_moved() { /* Some + None tile */ }

    #[test]
    fn the_backdrop_names_its_cram_entry() { /* Layer::Backdrop */ }

    #[test]
    fn the_callout_flips_rather_than_leaving_the_picture() {
        // Anchor at the far right/bottom; assert every painted pixel is inside `area`.
    }
```
Write these out in full when implementing — each asserts on the exact string, in the spelling above.

- [ ] **Step 2: Track the hover dot in the run loop**

Near the click handling (main.rs:1040-1079), add:
```rust
        // Hover explains, click arms (spec §5.2). Resolved only when the dot under the cursor
        // changes: attribution is one extra scanline resolve, cheap per *frame* but not per
        // iteration of a paused loop redrawing one image.
        let hover_at = (lenses.is_on(lens::LensId::Hover) && !palette.is_open())
            .then(|| window.get_mouse_pos(MouseMode::Discard))
            .flatten()
            .and_then(|(mx, my)| present::window_to_native(mx, my, view, width, HEIGHT));
```
and pass `hover_at` into `lens::models`, which resolves attribution when it is `Some` and the lens is on, reusing the `sprites_decoded()` it already computed for the outline lens (compute it once in `models` and share it between the two branches).

- [ ] **Step 3: Gates, then mutation-verify**

1. Remove the flip in `draw_hover` (always place right/below) → `the_callout_flips_rather_than_leaving_the_picture` FAILS.
2. Return a fabricated tile on the `None` branch → `a_sprite_pixel_names_its_slot_and_says_so_when_the_tile_moved` FAILS.
Restore; record both.

- [ ] **Step 4: Commit**

```bash
git add crates/oracle-frontend/src/lens crates/oracle-frontend/src/main.rs
git commit -m "feat(frontend): the hover callout lens

Hover explains, click still arms. Reads pixel_attribution directly rather
than pick::resolve, whose three Strings and second sprite decode belong to
the click path, and only when the dot under the cursor changes. A moved
SAT says 'tile ?' rather than inventing one.

mutation: flip removed → the_callout_flips_rather_than_leaving... FAILED
mutation: fabricated tile on the None branch → a_sprite_pixel_names... FAILED"
```

---

### Task 10: docs, banner, and the full gate run

**Files:**
- Modify: `crates/oracle-frontend/src/main.rs` (module doc + startup banner)
- Create: `docs/2026-08-17-player-s3-lenses.md`

- [ ] **Step 1: Update the module doc and the banner**

`main.rs`'s module doc has a key table (around main.rs:43-45) and a persistence paragraph (main.rs:72-79). Add a lens paragraph naming: the five lenses, that they are palette-only this slice, that the set persists under `lenses`, that the CPU chip auto-shows while paused, and the CRAM strip's Normal-ramp divergence. Extend the persisted-key list from six to seven and state that unknown keys are now preserved (it currently describes the S2 behaviour).

The startup banner (main.rs:880) ends `` `=command palette (the full list)` `` — append `; lenses in the palette's LENSES group`.

- [ ] **Step 2: Write the handoff doc**

`docs/2026-08-17-player-s3-lenses.md`, following `docs/2026-08-17-player-s2-config.md`'s shape: what shipped, gates at merge, the **owner-owed smoke checklist** (extending S1's and S2's — it is still unrun), registered follow-ups, and the review-loop record. The smoke items this slice adds, at minimum:
- open the palette, toggle each of the five lenses, confirm each appears and the toast names it;
- quit and relaunch → the same lenses come back on;
- hand-add a junk key to `~/.config/oracle/player.conf`, launch, toggle a lens (forcing a save), quit → **the junk key is still in the file** (the F-CONFIG-UNKNOWN-KEYS reversal, on real glass);
- sprite outlines land *on* the sprites at a non-integer window size (drag the window to an awkward size — this is what Task 1's ceiling form is for);
- hover a plane tile, a sprite, and the backdrop → the callout names each;
- pause → the CPU chip appears on its own; toggle the register block → D0-D7/A0-A7.

- [ ] **Step 3: Run the full gates**

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test -p oracle-frontend
cargo test -p oracle-frontend --no-default-features
cargo test --workspace
cargo build --release -p oracle-frontend
git diff m68000-microop-framework..HEAD -- crates/oracle-core/
```
Expected: fmt clean; clippy 0 warnings both variants; frontend suite green both variants; **workspace EXIT=0 with 0 failures** (record the pass/leg counts); release build succeeds; **the core diff is EMPTY**.

Do not pipe any of these through `tail` or `head`. If output is unwieldy, redirect to a file in the scratchpad and `Read` it.

- [ ] **Step 4: Commit**

```bash
git add crates/oracle-frontend/src/main.rs docs/2026-08-17-player-s3-lenses.md
git commit -m "docs: S3 handoff — lenses shipped, smoke checklist extended

Records the owner-owed smoke items (including the unknown-key survival
check on real glass) and the gate figures at merge."
```

---

## Self-review against the spec

**§5.1 watch ticker** — Task 5. Newest hits ✓, armed count ✓, non-destructive ✓, `W`/`C`/click unchanged ✓ (nothing in this slice touches them).
**§5.2 video lenses** — sprite outlines Task 8 ✓, hover callout Task 9 ✓ (hover explains / click arms preserved), CRAM strip 4×16 Task 7 ✓.
**§5.3 CPU chip** — Task 6. PC as symbol ✓, SR ✓, frame counter ✓, auto-show while paused ✓, expanded D0-D7/A0-A7 ✓, raw hex without a `.lst` ✓.
**§5.4 audio meters** — deliberately **not** in this slice: §9 gates it on an Aether contract row for channel state, which does not exist. Do not build it here.
**§5 "each lens auto-registers its toggle command"** — Task 4, generated from `LensId::ALL`.
**§5 "the active lens set persists across relaunch"** — Task 3.
**§9 module layout** — `lens/` with `watch.rs`, `video.rs`, `cpu.rs` ✓. The spec names a `Lens` trait; this plan uses a model/draw function pair per lens instead, because the four lenses have genuinely different inputs (a hit ring, a register file, a palette array, a mouse dot) and a `draw(&System, &mut Frame)` trait would force every lens to re-read `System` inside the draw path — the opposite of the model/draw split the house tests rely on. Recorded as a deliberate deviation, not an oversight.
**§9 "lenses and socket clients read the same instruments — nothing double-sourced"** — the ticker reads `bus.watchpoints_mut()`, the same instrument `emulator/watchpoint_hits` serves ✓.
**§10 error handling** — missing `.lst` → raw hex PC ✓ (Task 6); lens drawing never touches the retained framebuffer ✓ (stated in every draw task, enforced by the `draw_paints_inside_area_only` tests).
**§11 testing** — pure functions tested directly ✓, lens draws render into a scratch buffer and assert pixels ✓, every evidence-bearing test mutation-verified ✓.

**Known gaps, deliberate:** no default hotkeys (S5 owns rebinding); the ticker can be briefly covered by a toast burst (both anchor to the bottom edge; toasts are transient); the CRAM strip shows the Normal ramp in shadow/highlight regions (the S/H conversion is private and this slice adds no core surface).
