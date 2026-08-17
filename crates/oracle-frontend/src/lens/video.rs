//! Video lenses (spec §5.2) — the things drawn *on* the picture rather than beside it. The CRAM
//! strip and the sprite outlines so far; the hover callout joins them in the task after this one.
//!
//! The outlines are the first thing in the frontend anchored to a pixel *inside* the picture rather
//! than to one of its corners, which is why they arrive together with
//! [`present::native_rect_to_window`](crate::present::native_rect_to_window). The split that keeps
//! them testable is the same one every lens uses, one level finer: [`boxes`] works entirely in game
//! pixels and knows nothing of the window, and [`draw_sprites`] maps at the last moment.
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
use oracle_core::render::SpriteDecoded;

/// The CRAM shape: four palette lines of sixteen colours, in CRAM order. The strip is laid out the
/// way the hardware is indexed — entry `n` at row `n / 16`, column `n % 16` — so a tile's palette
/// line is a row and a colour index is a column.
const PALETTES: usize = 4;
const COLOURS: usize = 16;

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

/// One outline, already clipped to the display and expressed in **game pixels** — the mapping to
/// window pixels is [`present::native_rect_to_window`](crate::present::native_rect_to_window)'s
/// job, and keeping it out of here is what makes the geometry testable without a window. It is
/// also what keeps this a *model*: something that knew about window pixels would have stopped
/// being one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpriteBox {
    /// The SAT slot, not the link-walk position — the same number the aether and `emulator/sprites`
    /// report, so a box on the glass can be matched against a dump.
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
///   {64}` (render.rs:543-547): the number is the core's to report.
/// * a sprite is **clipped per edge, not dropped** — `x`/`y` are signed screen coordinates with
///   the 128 bias already off both axes, and a sprite entering from the left is exactly the case
///   an outline lens is for. Dropping anything that pokes over an edge would blank the lens at the
///   moment it is most wanted.
/// * a sprite entirely outside the display contributes nothing, which silently handles the
///   parked-sprite idiom (`y == -128`) without naming it as a special case.
///
/// Slot order is preserved, so box `n` is slot `n`'s among the visible ones.
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

/// Outlines are drawn just short of opaque: solid enough to read against any background, sheer
/// enough that the sprite's own edge pixels stay visible underneath — which is what you are
/// looking at when you ask where a sprite's box actually is.
const OUTLINE_ALPHA: u8 = 200;

/// Four thin `fill_rect`s per box — [`font::Canvas`] has no stroke primitive, and a filled
/// rectangle would hide the very sprite it is pointing at. High-priority sprites are drawn in the
/// accent colour so the layer you are hunting is the one that stands out.
///
/// `native` is the **blit's** source size (`width x HEIGHT`, the frame that was actually
/// presented), not `active_display()`: the outline has to land where the picture is, and after an
/// H40→H32 switch since the last capture those two disagree for a frame. That is the same
/// one-frame skew the click path already documents, and the same direction — the geometry on the
/// glass wins.
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
        c.fill_rect(x, y, w, t, color, OUTLINE_ALPHA); // top
        c.fill_rect(
            x,
            y + (h.saturating_sub(t)) as i32,
            w,
            t,
            color,
            OUTLINE_ALPHA,
        ); // bottom
        c.fill_rect(x, y, t, h, color, OUTLINE_ALPHA); // left
        c.fill_rect(
            x + (w.saturating_sub(t)) as i32,
            y,
            t,
            h,
            color,
            OUTLINE_ALPHA,
        ); // right
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::Aspect;

    /// A decoded sprite with everything but geometry at its default. `x`/`y` are **signed screen**
    /// coordinates — the 128 bias is already off both axes by the time the core hands them over —
    /// so a negative value here is an ordinary sprite entering from the left or the top, not an
    /// error.
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
        assert_eq!(
            b[0].rect,
            Rect {
                x: 100,
                y: 50,
                w: 32,
                h: 16
            },
            "cells x 8"
        );
    }

    /// **The pairing test for the model.** Boxes come back in slot order, and box `n` must carry
    /// slot `n`'s own index, own geometry and own priority bit.
    ///
    /// Counting boxes, or checking that the set of rectangles is right, cannot see the bug that
    /// matters here: a `boxes` that paired every rectangle with the *next* sprite's index or
    /// priority produces the same count and the same set of rectangles, and then the outline lens
    /// paints the wrong sprite in the accent colour — a lens that lies about the thing it exists to
    /// point at. Every field is distinct per sprite so no two can be confused, and the sizes differ
    /// so a transposition shows up in the geometry too.
    #[test]
    fn each_box_carries_its_own_sprites_index_size_and_priority() {
        let mut s0 = sprite(7, 10, 20, 1, 4);
        let mut s2 = sprite(41, 200, 100, 3, 1);
        s0.priority = true;
        s2.priority = true;
        let s1 = sprite(19, 60, 70, 2, 2); // priority stays false
        let b = boxes(&[s0, s1, s2], 80, (320, 224));
        assert_eq!(
            b,
            vec![
                SpriteBox {
                    index: 7,
                    rect: Rect {
                        x: 10,
                        y: 20,
                        w: 8,
                        h: 32
                    },
                    priority: true,
                },
                SpriteBox {
                    index: 19,
                    rect: Rect {
                        x: 60,
                        y: 70,
                        w: 16,
                        h: 16
                    },
                    priority: false,
                },
                SpriteBox {
                    index: 41,
                    rect: Rect {
                        x: 200,
                        y: 100,
                        w: 24,
                        h: 8
                    },
                    priority: true,
                },
            ]
        );
    }

    /// A sprite entering from an edge keeps the part that is on screen — dropping it is the obvious
    /// wrong answer, and it is exactly the case the lens exists to show. Clipping is **per edge**:
    /// each of the four is exercised on its own, with the other three slack, so a fix that clipped
    /// only the near edges cannot pass on the far ones.
    #[test]
    fn a_partly_offscreen_sprite_is_clipped_per_edge_not_dropped() {
        for (label, s, want) in [
            (
                "entering from the left",
                sprite(0, -8, 40, 2, 2),
                Rect {
                    x: 0,
                    y: 40,
                    w: 8,
                    h: 16,
                },
            ),
            (
                "entering from the top",
                sprite(0, 40, -4, 2, 2),
                Rect {
                    x: 40,
                    y: 0,
                    w: 16,
                    h: 12,
                },
            ),
            (
                "leaving to the right",
                sprite(0, 312, 40, 2, 2),
                Rect {
                    x: 312,
                    y: 40,
                    w: 8,
                    h: 16,
                },
            ),
            (
                "leaving through the bottom",
                sprite(0, 40, 220, 2, 2),
                Rect {
                    x: 40,
                    y: 220,
                    w: 16,
                    h: 4,
                },
            ),
        ] {
            let b = boxes(&[s], 80, (320, 224));
            assert_eq!(b.len(), 1, "{label}: the sprite was dropped");
            assert_eq!(b[0].rect, want, "{label}");
        }
        // Both axes at once, since the two clips are independent code.
        let b = boxes(&[sprite(0, -8, -4, 2, 2)], 80, (320, 224));
        assert_eq!(b.len(), 1);
        assert_eq!(
            b[0].rect,
            Rect {
                x: 0,
                y: 0,
                w: 8,
                h: 12
            }
        );
    }

    /// The parked-sprite idiom (`y == -128`) falls out of the clip rather than being special-cased,
    /// and so does a sprite parked off the right-hand side.
    #[test]
    fn a_sprite_entirely_outside_the_display_contributes_nothing() {
        assert!(
            boxes(&[sprite(0, 0, -128, 4, 4)], 80, (320, 224)).is_empty(),
            "the parked idiom"
        );
        assert!(
            boxes(&[sprite(0, 320, 40, 4, 4)], 80, (320, 224)).is_empty(),
            "flush past the right edge"
        );
        assert!(
            boxes(&[sprite(0, 40, 224, 4, 4)], 80, (320, 224)).is_empty(),
            "flush past the bottom edge"
        );
        // H32's display is narrower, so a sprite visible in H40 can be entirely outside it.
        assert!(
            boxes(&[sprite(0, 260, 40, 1, 1)], 64, (256, 224)).is_empty(),
            "outside H32's 256-dot display"
        );
    }

    /// Slots at or past `parsed_max` are decoded but never parsed by the hardware — outlining them
    /// would draw boxes around things the VDP never shows. H32 parses 64, H40 80, and the number is
    /// the core's to report (`Vdp::parsed_sprite_max`), never ours to re-derive.
    #[test]
    fn slots_past_parsed_max_are_not_outlined() {
        let sprites: Vec<SpriteDecoded> = (0..80u8).map(|i| sprite(i, 10, 10, 1, 1)).collect();
        let h32 = boxes(&sprites, 64, (256, 224));
        assert_eq!(h32.len(), 64, "H32 parses 64");
        assert_eq!(h32[63].index, 63, "and stops at slot 63");
        assert_eq!(boxes(&sprites, 80, (320, 224)).len(), 80, "H40 parses 80");
    }

    /// Render `bx` into a `w * h` buffer over [`BG`] and hand back the buffer.
    fn render_sprites(
        w: usize,
        h: usize,
        area: Rect,
        px: usize,
        native: (usize, usize),
        bx: &[SpriteBox],
    ) -> Vec<u32> {
        let mut buf = vec![BG; w * h];
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw_sprites(&mut c, area, px, native, bx);
        }
        buf
    }

    /// **The outline lands on the sprite**, at three geometries including a non-integer scale — the
    /// whole reason the forward map exists.
    ///
    /// Every expected number is written out by hand rather than taken from
    /// `present::native_rect_to_window`: an expectation computed from the map under test moves with
    /// its bugs and pins nothing. Containment would not do either — a box inside the picture but in
    /// the wrong place is precisely the failure mode.
    ///
    /// The `500x350` rows are the load-bearing ones: that is a 1.5625x scale, where the blit's
    /// ceiling form and the floor form disagree (157 against 156), so an integer-scale-only test
    /// set would miss the map's entire reason for existing. Their non-zero origin also catches an
    /// anchor that ignored `area` and measured from the window corner.
    #[test]
    fn an_outline_lands_exactly_on_the_sprite_at_every_scale() {
        let (w, h) = (700usize, 520usize);
        let bx = boxes(&[sprite(0, 100, 50, 4, 2)], 80, (320, 224));
        // (label, area, px, left, top, right, bottom) — the ink's four extremes, inclusive.
        for (label, area, px, left, top, right, bottom) in [
            (
                "2x, flush at the origin",
                Rect {
                    x: 0,
                    y: 0,
                    w: 640,
                    h: 448,
                },
                2usize,
                200usize,
                100usize,
                263usize,
                131usize,
            ),
            (
                "1.5625x, offset picture, hairline",
                Rect {
                    x: 30,
                    y: 20,
                    w: 500,
                    h: 350,
                },
                1,
                187,
                99,
                236,
                123,
            ),
            (
                "1.5625x, offset picture, thick stroke",
                Rect {
                    x: 30,
                    y: 20,
                    w: 500,
                    h: 350,
                },
                3,
                187,
                99,
                236,
                123,
            ),
        ] {
            let buf = render_sprites(w, h, area, px, (320, 224), &bx);
            let got = ink_bounds(&buf, w).unwrap_or_else(|| panic!("{label}: painted nothing"));
            assert_eq!(
                got,
                (top, bottom, left, right),
                "{label}: the outline is at (top,bottom,left,right) {got:?}, not \
                 ({top},{bottom},{left},{right})"
            );
        }
    }

    /// An outline is an **outline**: four bars `px` thick with the picture showing through the
    /// middle. A filled rectangle would hide the sprite it is pointing at, and it would pass every
    /// bounds assertion above.
    ///
    /// The probes sit on the box's mid-row and mid-column, clear of the perpendicular bars, so each
    /// one measures exactly one edge's thickness. `px = 2` is the smallest scale at which "one
    /// pixel" and "`px` pixels" differ.
    #[test]
    fn an_outline_is_hollow_and_its_edges_are_one_font_scale_thick() {
        let (w, h) = (700usize, 520usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 640,
            h: 448,
        };
        let px = 2;
        let bx = boxes(&[sprite(0, 100, 50, 4, 2)], 80, (320, 224));
        let buf = render_sprites(w, h, area, px, (320, 224), &bx);
        // The window box is (200,100)..(264,132) — see the scale test above, same fixture.
        let (bl, bt, br, bb) = (200usize, 100usize, 264usize, 132usize);
        let (midx, midy) = (bl + (br - bl) / 2, bt + (bb - bt) / 2);
        let at = |x: usize, y: usize| buf[y * w + x];

        for d in 0..px {
            assert_ne!(at(midx, bt + d), BG, "the top bar is thinner than px");
            assert_ne!(
                at(midx, bb - 1 - d),
                BG,
                "the bottom bar is thinner than px"
            );
            assert_ne!(at(bl + d, midy), BG, "the left bar is thinner than px");
            assert_ne!(at(br - 1 - d, midy), BG, "the right bar is thinner than px");
        }
        assert_eq!(at(midx, bt + px), BG, "the top bar is thicker than px");
        assert_eq!(
            at(midx, bb - 1 - px),
            BG,
            "the bottom bar is thicker than px"
        );
        assert_eq!(at(bl + px, midy), BG, "the left bar is thicker than px");
        assert_eq!(
            at(br - 1 - px, midy),
            BG,
            "the right bar is thicker than px"
        );
        for y in bt + px..bb - px {
            assert_eq!(
                at(midx, y),
                BG,
                "the box is filled, not outlined (row {y} of its middle column)"
            );
        }
        for x in bl + px..br - px {
            assert_eq!(
                at(x, midy),
                BG,
                "the box is filled, not outlined (column {x} of its middle row)"
            );
        }
    }

    /// **The pairing test for the draw.** The priority colour must follow the *sprite*, not the
    /// slot: with the flag moved from one sprite to the other, the two boxes must swap colours.
    ///
    /// Asserting "two colours appear" would pass on a draw that coloured by slot index, by box
    /// order, or by nothing at all. Comparing the two renders pins the association itself, and it
    /// does so without recomputing `Canvas`'s alpha blend — which would be an expectation derived
    /// from the code under test.
    #[test]
    fn the_priority_colour_follows_the_sprite_not_the_slot() {
        let (w, h) = (640usize, 448usize);
        let area = Rect { x: 0, y: 0, w, h };
        let mut lo = sprite(0, 10, 10, 2, 2);
        let mut hi = sprite(1, 100, 100, 2, 2);
        let render = |first: SpriteDecoded, second: SpriteDecoded| {
            let bx = boxes(&[first, second], 80, (320, 224));
            assert_eq!(bx.len(), 2, "both sprites are on screen");
            render_sprites(w, h, area, 1, (320, 224), &bx)
        };
        // At 2x the boxes' top-left corners are (20,20) and (200,200).
        let (p0, p1) = (20 * w + 20, 200 * w + 200);

        hi.priority = true;
        let a = render(lo, hi); // slot 0 ordinary, slot 1 high priority
        hi.priority = false;
        lo.priority = true;
        let b = render(lo, hi); // the flag moved to slot 0

        for (label, buf, p) in [("a", &a, p0), ("a", &a, p1), ("b", &b, p0), ("b", &b, p1)] {
            assert_ne!(buf[p], BG, "render {label}: no outline at index {p}");
        }
        assert_ne!(
            a[p0], b[p0],
            "moving the priority flag onto slot 0 did not change slot 0's colour"
        );
        assert_eq!(
            a[p0], b[p1],
            "the ordinary colour did not move to the sprite that lost the flag"
        );
        assert_eq!(
            b[p0], a[p1],
            "the priority colour did not move to the sprite that gained the flag"
        );
    }

    /// Outlines stay on the picture. The letterbox must stay black, and a sprite hanging off the
    /// right or bottom edge is the case that would push ink into it.
    ///
    /// [`Aspect::Integer`] rather than the house-default `Tv`, and the assertions below say why:
    /// `Tv` fits a 4:3 picture into the window's *larger* axis, so one of `area.x`/`area.y` is
    /// always 0 — and a containment sweep against a zero offset is not an assertion at all, it is
    /// `x >= 0`, which every pixel in the buffer satisfies. At 700x520 the integer scale is 2 and
    /// the 640x448 picture is inset on **both** axes, so both halves of the sweep bite.
    #[test]
    fn outlines_paint_inside_area_only() {
        let (w, h) = (700usize, 520usize);
        let area = crate::present::dest_rect(w, h, 320, 224, Aspect::Integer);
        assert!(
            area.x > 0 && area.y > 0,
            "this window must letterbox on both axes or the sweep checks nothing: {area:?}"
        );
        let bx = boxes(
            &[
                sprite(0, -8, -8, 4, 4),
                sprite(1, 0, 0, 4, 4),
                sprite(2, 300, 210, 4, 4),
                sprite(3, 316, 220, 1, 1),
            ],
            80,
            (320, 224),
        );
        assert_eq!(
            bx.len(),
            4,
            "every fixture sprite is at least partly visible"
        );
        let buf = render_sprites(w, h, area, 2, (320, 224), &bx);
        assert!(buf.iter().any(|p| *p != BG), "drew nothing");
        for (i, p) in buf.iter().enumerate() {
            if *p != BG {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "outline escaped the picture at ({x},{y}) — the letterbox must stay black"
                );
            }
        }
    }

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

    /// The font scale must scale the strip **and carry its anchor with it** — position at three
    /// scales, not just size at two.
    ///
    /// Every other geometric assertion in this module is pinned at `px = 1`, which left the
    /// vertical anchor unpinned above it, and this test used to compare only the panel's *size*.
    /// `status_row` does not affect size at all, so it was unmeasured at every scale: writing it as
    /// `LINE_H * px + 4` instead of `+ 2 * pad` shipped green. That one is not academic — at
    /// `px = 4`, an ordinary 4x window, the correct offset is 48 device pixels, the mutant gives 36,
    /// and the F3 status line stands 44 tall. **The strip lands on top of the status line**, which
    /// is the exact collision the offset exists to prevent and the thing the anchor test above is
    /// named after.
    ///
    /// The `px = 4` row is the other half: `margin = (2 * px).max(4)` is 4 at both px 1 and px 2, so
    /// hardcoding `margin = 4` was identity everywhere this module looked. It first diverges at
    /// px 4, where the correct margin is 8.
    #[test]
    fn the_strip_anchors_and_scales_at_every_font_scale() {
        // Non-zero origin, so an anchor that ignored `area` entirely cannot pass.
        let (w, h) = (400usize, 400usize);
        let area = Rect {
            x: 16,
            y: 12,
            w: 380,
            h: 380,
        };
        let sw = ramp();
        // (px, left, top, panel_w, panel_h), every number written out. Derived by hand from the
        // house idiom — `margin = (2 * px).max(4)`, `pad = 2 * px`, `cell = 3 * px`,
        // `status_row = LINE_H * px + 2 * pad` — precisely so no assertion here can be recomputed
        // from the code it is checking. Note `margin` is 4 at both px 1 and px 2 (the `.max(4)`
        // floor still binds) and only starts tracking `2 * px` at px 4: that is why the third row
        // exists.
        for (px, left, top, panel_w, panel_h) in [
            (1usize, 20usize, 28usize, 52usize, 16usize),
            (2, 20, 40, 104, 32),
            (4, 24, 68, 208, 64),
        ] {
            let (t, b, l, r) = ink_bounds(&render(w, h, area, px, &sw), w)
                .unwrap_or_else(|| panic!("px {px} painted nothing"));
            assert_eq!(
                (l, t),
                (left, top),
                "px {px}: the strip is anchored at ({l},{t}), not ({left},{top})"
            );
            assert_eq!(
                (r - l + 1, b - t + 1),
                (panel_w, panel_h),
                "px {px}: the strip did not scale"
            );
        }
        assert_eq!(
            (PANEL_W_PX1, PANEL_H_PX1, STATUS_ROW_PX1),
            (52, 16, 12),
            "the px-1 constants the other tests use must agree with the table above"
        );
    }
}
