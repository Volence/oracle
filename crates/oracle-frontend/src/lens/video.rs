//! Video lenses (spec §5.2) — the things drawn *on* the picture rather than beside it. The CRAM
//! strip so far; the sprite outlines and the hover callout join it in the tasks after this one.
//!
//! **A known, accepted divergence.** The strip is built from `Vdp::cram_decoded()`, which a core
//! test (`cram_rgb_matches_cram_decoded`, render.rs:1622) pins to agree *exactly* with the
//! renderer's own per-entry decode at `PixelState::Normal`. The renderer's shadow/highlight-aware
//! conversion is **private**, so inside an S/H region the picture is drawn at half or upper
//! intensity while the strip still shows the Normal ramp: the swatch is the palette entry, not the
//! pixel it produced there. Reading a shadowed sprite's colour off the strip therefore gives you
//! the entry, not what you can see — which is the useful half for "what is CRAM holding?", and the
//! wrong half for "why is this pixel that colour?" (the hover callout answers that one). Exporting
//! the private conversion to close the gap would be a core change this slice deliberately does not
//! make.

use crate::font;
use crate::present::Rect;

/// The CRAM shape: four palette lines of sixteen colours, in CRAM order. The strip is laid out the
/// way the hardware is indexed — entry `n` at row `n / 16`, column `n % 16` — so a tile's palette
/// line is a row and a colour index is a column.
pub const PALETTES: usize = 4;
pub const COLOURS: usize = 16;

/// The grid and the array must agree. `draw_cram` indexes a `[u32; 64]` by `line * COLOURS + col`,
/// so raising either constant on its own turns a layout tweak into an out-of-bounds panic in the
/// draw path — on hardware that has exactly four lines of sixteen, which is why these are `const`
/// rather than parameters at all.
const _: () = assert!(
    PALETTES * COLOURS == 64,
    "the CRAM grid must cover exactly the 64 entries cram_decoded returns"
);

/// Swatch edge in font-scale units. Three device pixels per scale step reads as a colour rather
/// than a dot, and still leaves the strip smaller than a line of text.
const SWATCH: usize = 3;

/// Pack the core's decoded triples into the frontend's `0x00RR_GGBB`.
pub fn swatches(cram: &[(u8, u8, u8); 64]) -> [u32; 64] {
    let mut out = [0u32; 64];
    for (slot, (r, g, b)) in out.iter_mut().zip(cram.iter()) {
        *slot = ((*r as u32) << 16) | ((*g as u32) << 8) | *b as u32;
    }
    out
}

/// Top-left of `area`, one text row below the top edge so the strip and the F3 status line never
/// fight for the same corner.
///
/// There is no degrading form the way the CPU chip has one: a strip is only useful whole, and a
/// clipped one would silently misreport CRAM by showing three palette lines as if they were four.
/// So a picture that cannot hold it draws nothing — which is also what keeps the `usize` geometry
/// below from underflowing on a tiny area (the `draw_narrow_panel_does_not_underflow` class).
pub fn draw_cram(c: &mut font::Canvas, area: Rect, px: usize, sw: &[u32; 64]) {
    let pad = 2 * px;
    let margin = (2 * px).max(4);
    let cell = SWATCH * px;
    let panel_w = COLOURS * cell + 2 * pad;
    let panel_h = PALETTES * cell + 2 * pad;
    // Sit clear of the status line (F3), which owns the top-left text row: `overlay` puts it at
    // `area.y + margin` and it stands `GLYPH_H * px + 2 * pad` tall, so dropping a whole `LINE_H`
    // row clears it with the font's own leading to spare. The offset is unconditional — the status
    // line latches and flashes on its own schedule, and a strip that jumped a row whenever a save
    // slot flashed would be worse than one sitting a row lower than it strictly needs to.
    let status_row = font::LINE_H * px + 2 * pad;
    if area.w < panel_w + 2 * margin || area.h < panel_h + status_row + 2 * margin {
        return;
    }
    let left = (area.x + margin) as i32;
    let top = (area.y + margin + status_row) as i32;
    c.fill_rect(left, top, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);
    for line in 0..PALETTES {
        for col in 0..COLOURS {
            // Opaque, unlike the panel behind them: a swatch blended over the picture would be a
            // different colour from the entry it is reporting, which is the one thing it must not
            // be.
            c.fill_rect(
                left + (pad + col * cell) as i32,
                top + (pad + line * cell) as i32,
                cell,
                cell,
                sw[line * COLOURS + col],
                255,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The buffer fill every draw test below starts from, and **the reason it is not `0`** — the
    /// same constant and the same reason as `lens/watch.rs` and `lens/cpu.rs`. The panel is
    /// `fill_rect(..., 0x0000_0000, PANEL_ALPHA)`, black alpha-blended, which over a black buffer
    /// is a *no-op*: a `!= 0` test cannot see it at all. The swatches themselves are opaque and
    /// would show against zero, which is exactly what makes the trap tempting here — the swatches
    /// would look tested while the panel under them spanned the whole window unnoticed.
    const BG: u32 = 0x0012_3456;

    /// The house margin idiom, re-derived so the assertions below can name the panel's own edges
    /// rather than the area's — the two differ by exactly this, and that difference is the bug
    /// class (ink in the letterbox) these tests exist to catch.
    fn margin_of(px: usize) -> usize {
        (2 * px).max(4)
    }

    /// Sixty-four distinct, opaque, non-[`BG`] colours in CRAM order: entry `i` is `(i + 1, 0, 0)`,
    /// so every swatch **names its own index**. With a flat or repeating fixture a strip that drew
    /// all the right colours in all the wrong places would be indistinguishable from a correct one.
    fn ramp() -> [u32; 64] {
        let mut cram = [(0u8, 0u8, 0u8); 64];
        for (i, e) in cram.iter_mut().enumerate() {
            *e = (i as u8 + 1, 0, 0);
        }
        swatches(&cram)
    }

    /// Render `sw` into a `w * h` buffer over [`BG`] and hand back the buffer.
    fn render(w: usize, h: usize, area: Rect, px: usize, sw: &[u32; 64]) -> Vec<u32> {
        let mut buf = vec![BG; w * h];
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw_cram(&mut c, area, px, sw);
        }
        buf
    }

    /// The (top, bottom, left, right) extremes of everything `draw_cram` changed.
    fn ink_bounds(buf: &[u32], w: usize) -> Option<(usize, usize, usize, usize)> {
        let mut b: Option<(usize, usize, usize, usize)> = None;
        for (i, p) in buf.iter().enumerate() {
            if *p == BG {
                continue;
            }
            let (x, y) = (i % w, i / w);
            b = Some(match b {
                None => (y, y, x, x),
                Some((t, bo, l, r)) => (t.min(y), bo.max(y), l.min(x), r.max(x)),
            });
        }
        b
    }

    /// The panel's size and the row it clears, written out rather than imported from the module:
    /// a bound computed from the code under test moves with the bug and cannot catch it. These are
    /// the `px = 1` numbers — 16 swatches of 3px plus 2px of padding either side, 4 rows likewise,
    /// and one `LINE_H` text row plus its padding above.
    const PANEL_W_PX1: usize = 52;
    const PANEL_H_PX1: usize = 16;
    const STATUS_ROW_PX1: usize = 12;

    #[test]
    fn swatches_pack_the_cores_triples_without_reordering_channels() {
        let mut cram = [(0u8, 0u8, 0u8); 64];
        cram[0] = (0xFF, 0x00, 0x00);
        cram[1] = (0x00, 0xFF, 0x00);
        cram[2] = (0x00, 0x00, 0xFF);
        cram[63] = (0x12, 0x34, 0x56);
        let sw = swatches(&cram);
        assert_eq!(sw[0], 0x00FF_0000, "red is the high byte");
        assert_eq!(sw[1], 0x0000_FF00, "green is the middle byte");
        assert_eq!(sw[2], 0x0000_00FF, "blue is the low byte");
        assert_eq!(sw[63], 0x0012_3456);

        // **Every** entry, not just the four probed above: a `swatches` that packed the first half
        // and left the rest black would pass a four-entry test and draw a half-blank strip.
        let sw = ramp();
        for (i, packed) in sw.iter().enumerate() {
            assert_eq!(
                *packed,
                (i as u32 + 1) << 16,
                "entry {i} was not packed in place"
            );
        }
    }

    /// **The pairing test.** All sixty-four entries reach the glass, each in its own place: entry
    /// `n` fills the cell at row `n / 16`, column `n % 16`, and fills exactly that cell.
    ///
    /// Membership — "all 64 colours appear somewhere" — is the assertion the plan shipped, and it
    /// is blind to the bug that actually matters: a strip drawn transposed, reversed, or column-
    /// major shows all sixty-four distinct colours and is useless, because reading a palette entry
    /// off it gives the wrong answer under the right-looking layout. The per-cell colour **and**
    /// its pixel count are both pinned, so a swatch drawn at the wrong size or drawn twice fails
    /// too.
    #[test]
    fn the_strip_lays_every_entry_out_in_cram_order() {
        let (w, h) = (320usize, 224usize);
        let area = Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        let (px, cell) = (1usize, 3usize);
        let pad = 2 * px;
        let sw = ramp();
        let buf = render(w, h, area, px, &sw);

        // The hardware's shape, pinned literally. The grid `const _` assert only fixes the
        // *product*, so an 8x8 reshape satisfies it — and this test would then derive `row`/`col`
        // from the very constants that moved and check an 8x8 strip against itself, quite happily.
        assert_eq!((PALETTES, COLOURS), (4, 16), "CRAM is 4 lines of 16");

        let left = area.x + margin_of(px) + pad;
        let top = area.y + margin_of(px) + STATUS_ROW_PX1 + pad;
        for (n, colour) in sw.iter().enumerate() {
            let (row, col) = (n / 16, n % 16);
            for dy in 0..cell {
                for dx in 0..cell {
                    let (x, y) = (left + col * cell + dx, top + row * cell + dy);
                    assert_eq!(
                        buf[y * w + x],
                        *colour,
                        "entry {n} should own the cell at row {row}, column {col}; ({x},{y}) is \
                         ${:06X}",
                        buf[y * w + x]
                    );
                }
            }
            assert_eq!(
                buf.iter().filter(|p| *p == colour).count(),
                cell * cell,
                "entry {n} was painted somewhere besides its own cell"
            );
        }
    }

    #[test]
    fn draw_paints_inside_area_only() {
        let (w, h) = (320usize, 224usize);
        let area = Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        let px = 1;
        let buf = render(w, h, area, px, &ramp());

        // The panel alone is `panel_w * panel_h` pixels. If this ever fails, the panel has gone
        // invisible against BG again and the containment sweep below is blind to it.
        let painted = buf.iter().filter(|p| **p != BG).count();
        assert!(
            painted >= PANEL_W_PX1 * PANEL_H_PX1,
            "the panel left no mark: {painted} changed, panel is {PANEL_W_PX1}x{PANEL_H_PX1}"
        );
        for (i, p) in buf.iter().enumerate() {
            if *p != BG {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
    }

    /// It is a **top-left** strip, and it sits clear of the status line's row. Containment cannot
    /// tell that from a bottom-right one — the watch ticker's own tests passed for a while with the
    /// strip anchored to the wrong edge — so the panel's four edges are pinned by equality, and the
    /// ink is additionally held out of the opposite halves of the picture.
    ///
    /// Equality, not `<=`: zero satisfies `<=`, so the looser form lets `top = area.y` through, and
    /// a strip sitting on top of the status line is precisely what the offset exists to prevent.
    #[test]
    fn the_strip_hugs_the_top_left_one_status_row_down() {
        let (w, h) = (320usize, 224usize);
        let area = Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        let px = 1;
        let margin = margin_of(px);
        let buf = render(w, h, area, px, &ramp());
        let (top, bottom, left, right) = ink_bounds(&buf, w).expect("draw painted nothing");

        assert_eq!(
            left,
            area.x + margin,
            "the left gutter is not exactly one margin (first ink at column {left})"
        );
        assert_eq!(
            top,
            area.y + margin + STATUS_ROW_PX1,
            "the strip is not exactly one status row below the top margin (first ink on row {top})"
        );
        assert_eq!(
            right - left + 1,
            PANEL_W_PX1,
            "the panel is not sixteen swatches wide"
        );
        assert_eq!(
            bottom - top + 1,
            PANEL_H_PX1,
            "the panel is not four swatches tall"
        );
        assert!(
            right < area.x + area.w / 2,
            "ink reached the right half — this is a left-hand strip (last ink at column {right})"
        );
        assert!(
            bottom < area.y + area.h / 2,
            "ink reached the bottom half — this is a top strip (last ink on row {bottom})"
        );
    }

    /// A picture too small for the strip draws **nothing** — rather than a clipped strip, which
    /// would misreport CRAM, or a `usize` underflow. Both clauses are exercised on their own: each
    /// case is comfortable on the other axis, so only the axis named can fire.
    #[test]
    fn a_picture_too_small_for_the_strip_draws_nothing() {
        let (w, h) = (320usize, 240usize);
        for (label, area) in [
            (
                "too narrow",
                Rect {
                    x: 0,
                    y: 0,
                    w: 40,
                    h: 224,
                },
            ),
            (
                "too short",
                Rect {
                    x: 0,
                    y: 0,
                    w: 320,
                    h: 20,
                },
            ),
        ] {
            let buf = render(w, h, area, 1, &ramp());
            assert!(
                buf.iter().all(|p| *p == BG),
                "{label}: a strip was drawn into a picture that cannot hold it"
            );
        }
    }

    /// The gutters hold **at every picture size**, not just the one the tests above happen to pick.
    ///
    /// A boundary probe cannot catch this class: slackening the fit guard from `2 * margin` to
    /// `margin` still leaves the strip inside the picture, so containment passes, and it only
    /// shows up as a lost gutter at the widths and heights just above the new threshold — which is
    /// the range a single-size test never visits. Both axes are swept with the other held
    /// generous, so each guard clause is measured on its own.
    #[test]
    fn the_gutters_hold_at_every_picture_size() {
        let (w, h) = (200usize, 200usize);
        let px = 1;
        let margin = margin_of(px);
        let sw = ramp();
        let mut sizes_that_drew = 0;
        for n in 0..=160usize {
            for area in [
                Rect {
                    x: 8,
                    y: 6,
                    w: n,
                    h: 180,
                },
                Rect {
                    x: 8,
                    y: 6,
                    w: 180,
                    h: n,
                },
            ] {
                let buf = render(w, h, area, px, &sw);
                let Some((top, bottom, left, right)) = ink_bounds(&buf, w) else {
                    continue; // too small for the strip: the test above owns that case
                };
                sizes_that_drew += 1;
                assert_eq!(left, area.x + margin, "left gutter lost at {area:?}");
                assert_eq!(
                    top,
                    area.y + margin + STATUS_ROW_PX1,
                    "top offset lost at {area:?}"
                );
                assert!(
                    right + 1 + margin <= area.x + area.w,
                    "right gutter lost at {area:?} (last ink at column {right})"
                );
                assert!(
                    bottom + 1 + margin <= area.y + area.h,
                    "bottom gutter lost at {area:?} (last ink on row {bottom})"
                );
            }
        }
        assert!(
            sizes_that_drew > 0,
            "the sweep never drew the strip at all, so it checked nothing"
        );
    }

    /// The font scale must actually scale the strip: at `px = 2` every swatch is twice the edge and
    /// the panel twice the size. A `draw_cram` that ignored `px` would pass every `px = 1` test
    /// above and then draw a postage stamp in a 4x window.
    #[test]
    fn the_strip_scales_with_the_font_scale() {
        let (w, h) = (400usize, 400usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        };
        let sw = ramp();
        let (t1, b1, l1, r1) = ink_bounds(&render(w, h, area, 1, &sw), w).expect("px 1 painted");
        let (t2, b2, l2, r2) = ink_bounds(&render(w, h, area, 2, &sw), w).expect("px 2 painted");
        assert_eq!((r1 - l1 + 1, b1 - t1 + 1), (PANEL_W_PX1, PANEL_H_PX1));
        assert_eq!(
            (r2 - l2 + 1, b2 - t2 + 1),
            (2 * PANEL_W_PX1, 2 * PANEL_H_PX1),
            "the strip did not scale with px"
        );
    }
}
