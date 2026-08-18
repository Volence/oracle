//! Video lenses (spec §5.2) — the things drawn *on* the picture rather than beside it: the CRAM
//! strip, the sprite outlines, and the hover callout.
//!
//! **Hover explains, click arms.** The callout only ever *reads*: clicking still arms a watch
//! (main.rs:1046-1082, `pick.rs`), and nothing here touches that path. The two answer different
//! questions about the same dot, which is why they do not share code — `pick::resolve` builds three
//! `String`s and decodes the whole SAT a second time to describe a watch it is about to arm, and
//! paying that every frame to label a pixel would be the tail wagging the dog. [`hover_text`] reads
//! `Vdp::pixel_attribution` directly instead.
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
use oracle_core::render::{sprite_tile_at, Layer, PixelAttribution, SpriteDecoded, SpriteEval};

// --- The CRAM strip (spec 5.2) -----------------------------------------------------------------

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
    // Sit clear of the status line (F3), which owns the top-left text row. The band is `overlay`'s
    // to report rather than ours to re-derive: this used to be a local `LINE_H * px + 2 * pad`, and
    // the CPU chip's not having its own copy of it is exactly how the chip ended up drawing under
    // the status line. One definition, two readers.
    let status_row = crate::overlay::status_row_height(px);
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

// --- The sprite outlines (spec 5.2) ------------------------------------------------------------

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

/// Outline every sprite the hardware actually **walks**, clipped to the active display.
///
/// **`walk` is the link-list walk, not the table**, and that distinction is the whole bug this
/// signature exists to prevent. `Vdp::sprites_decoded()` returns all 80 SAT entries; the hardware
/// only ever displays the sprites reachable by following `link` from slot 0 until a link of 0, so
/// every unreachable slot holds whatever bytes were last written there — a *ghost*, drawing
/// nothing, at coordinates that were real some frames ago. Outlining the table put amber boxes on
/// empty picture, which is what the owner saw on the glass. The reachable list is
/// `Vdp::render_line_report(line).sprites`.
///
/// Four rules, each earning its place:
/// * **reachability is the only filter — never `SpriteEval::outcome`.** A sprite that is `OffLine`
///   on the sampled line is simply not on *that* line; it is a real sprite drawing elsewhere on the
///   frame and it must keep its box. Narrowing to `Rendered` would make nearly every box vanish
///   the moment you sampled a line the sprite does not touch, and `DroppedLineLimit` /
///   `DroppedPixelBudget` are per-line facts in exactly the same way.
/// * the parse cap needs no filtering here: the walk itself stops at 80 (H40) / 64 (H32) and says
///   so with `SpriteWalkEnd::MaxCount`, so the old `parsed_max` argument would now be a second,
///   redundant place for the same rule to be got wrong.
/// * a sprite is **clipped per edge, not dropped** — `x`/`y` are signed screen coordinates with
///   the 128 bias already off both axes, and a sprite entering from the left is exactly the case
///   an outline lens is for. Dropping anything that pokes over an edge would blank the lens at the
///   moment it is most wanted.
/// * a sprite entirely outside the display contributes nothing, which silently handles the
///   parked-sprite idiom (`y == -128`) without naming it as a special case.
///
/// `decoded` is the frame's `sprites_decoded()`, and it is here for **one field**: `SpriteEval`
/// carries no priority bit, and priority is what picks the outline's colour. It is looked up by
/// `index` — the SAT slot, which is what both lists are keyed by — never by position, since the
/// walk visits slots in link order and its third entry is very rarely slot 2. The geometry
/// deliberately comes from the walk even though the two agree by construction today
/// (`render.rs:648-653` and `render.rs:1141-1148` compute `x`/`y`/`width_cells`/`height_cells`
/// from the same fields): one source per record beats two that happen to match.
///
/// Walk order is preserved, so box `n` is the `n`th sprite the hardware would have evaluated.
pub fn boxes(
    walk: &[SpriteEval],
    decoded: &[SpriteDecoded],
    display: (u16, u16),
) -> Vec<SpriteBox> {
    let (dw, dh) = (i32::from(display.0), i32::from(display.1));
    // The walk is the upper bound: every entry yields at most one box, and on an ordinary frame
    // most of them do.
    let mut out = Vec::with_capacity(walk.len());
    for s in walk {
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
            // Both lists are keyed by SAT slot, so a walked index always has a decoded entry; if
            // the two ever disagreed, the box still belongs on the glass — position is what an
            // outline conveys — and the ordinary colour is the honest default for a bit we did not
            // read.
            priority: decoded
                .get(usize::from(s.index))
                .is_some_and(|d| d.priority),
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

// --- The hover callout (spec 5.2) --------------------------------------------------------------

/// What the callout says, and the game pixel it says it about.
///
/// The text is assembled in the model rather than in the draw, like every other lens here: the draw
/// path gets no `&System`, and the whole point of the split is that `pixel_attribution` — the one
/// expensive read this lens makes — happens once, where it can be skipped when the lens is off.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hover {
    /// The callout text, already assembled.
    pub text: String,
    /// The game pixel it describes — the callout is drawn beside it.
    pub at: (u16, u16),
}

/// The **panel body** behind the callout: a dark blue-grey rather than the other lenses' black.
///
/// The callout is the only lens that moves, so it is also the only one a reader has to *find*; a
/// tint the other panels never use is what tells you at a glance that this is the thing following
/// the cursor rather than a corner readout that happens to have drifted.
const CALLOUT_PANEL: u32 = 0x000A_1418;

/// How far the panel sits from the dot, in font-scale units — far enough that the callout does not
/// sit on top of the pixel it is describing, close enough to read as attached to it.
const CALLOUT_GAP: usize = 4;

/// `slot 12 | tile $4A0 | pal 2 | pri 1` for a sprite, the plane's cell for a plane or the window,
/// the CRAM entry for the backdrop.
///
/// **The separator is `|`, not the spec's `·`.** The 5x7 font has no middle dot (`font.rs:31-98`),
/// and an unmapped character draws as a hollow box — so the spec's spelling would put three empty
/// rectangles in the middle of every callout.
///
/// `sprites` is the frame's one `sprites_decoded()`, indexed by SAT slot, exactly as `pick.rs`
/// indexes it. Nothing here allocates beyond the returned string.
pub fn hover_text(attr: &PixelAttribution, sprites: &[SpriteDecoded]) -> String {
    match attr.winner {
        Layer::Sprite(index) => match sprites.get(usize::from(index)) {
            // The decode is `parsed_sprite_max()` long at most; a winner past its end would mean
            // the two disagreed, and naming a palette and a priority we did not read would be the
            // same lie as inventing a tile.
            None => format!("slot {index} | out of range"),
            Some(s) => {
                let pri = u8::from(s.priority);
                match sprite_tile_at(s, attr.x, attr.y) {
                    Some(t) => {
                        format!(
                            "slot {index} | tile ${t:03X} | pal {} | pri {pri}",
                            s.palette
                        )
                    }
                    // The SAT can move between the frame being drawn and this read (the same
                    // one-frame skew the click path documents at main.rs:1044-1047), and then the
                    // winning sprite's box no longer contains the dot. Say so rather than
                    // inventing a tile — `pick.rs:131-133` makes exactly this distinction.
                    None => format!("slot {index} | tile ? | pal {} | pri {pri}", s.palette),
                }
            }
        },
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
                // `PixelAttribution::cell` is `None` for a blanked line (display off, or the
                // leftmost-column blank), where there is a winning *layer* but no nametable cell
                // behind it.
                None => format!("{plane} | no cell"),
            }
        }
    }
}

/// Drawn beside the dot, **flipped to the other side of it** when the panel would otherwise run off
/// the picture — a callout that leaves the picture is worse than one on the wrong side of the
/// cursor, and the letterbox has to stay black.
///
/// The flips are independent per axis, because the two edges are reached independently: a dot in
/// the bottom-left corner flips vertically and not horizontally. Each flipped edge is then held
/// inside `area` (`.max(area.x)` / `.max(area.y)`), which is what a picture narrower than twice the
/// panel needs — there the flip alone would put the panel out the *other* side.
///
/// `native` is the blitted frame's source size, the same pair [`draw_sprites`] takes and for the
/// same reason: the callout has to land where the picture is.
pub fn draw_hover(c: &mut font::Canvas, area: Rect, px: usize, native: (usize, usize), hv: &Hover) {
    let pad = 2 * px;
    let gap = CALLOUT_GAP * px;
    let text_w = font::text_width(&hv.text) * px;
    // Clamped to the picture, so a long callout never widens past the picture and into the
    // letterbox; `fit` below then truncates the text to whatever survived.
    let panel_w = (text_w + 2 * pad).min(area.w);
    let panel_h = font::GLYPH_H * px + 2 * pad;
    // A picture too short to hold one line of text draws nothing rather than a panel hanging out of
    // the bottom of it: `Canvas` clips at the *buffer* edge, not at `area`, so without this the
    // callout would paint into the letterbox on a window dragged very short. There is no
    // corresponding width guard because `panel_w` is already clamped above.
    if area.h < panel_h {
        return;
    }
    let anchor = Rect {
        x: usize::from(hv.at.0),
        y: usize::from(hv.at.1),
        w: 1,
        h: 1,
    };
    let Some(a) = crate::present::native_rect_to_window(anchor, area, native.0, native.1) else {
        return;
    };
    let mut left = a.x + gap;
    if left + panel_w > area.x + area.w {
        left = a.x.saturating_sub(panel_w + gap).max(area.x);
    }
    let mut top = a.y + gap;
    if top + panel_h > area.y + area.h {
        top = a.y.saturating_sub(panel_h + gap).max(area.y);
    }
    c.fill_rect(
        left as i32,
        top as i32,
        panel_w,
        panel_h,
        CALLOUT_PANEL,
        font::PANEL_ALPHA,
    );
    c.text(
        (left + pad) as i32,
        (top + pad) as i32,
        px,
        crate::overlay::INFO,
        crate::overlay::fit(&hv.text, panel_w.saturating_sub(2 * pad), px),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::ink_bounds;
    use crate::present::Aspect;
    use oracle_core::render::{Cell, PixelState, SpriteOutcome};

    // --- The sprite outlines: the model -------------------------------------------------------

    /// One entry of the **link walk**, with everything but geometry at a default. `x`/`y` are
    /// **signed screen** coordinates — the 128 bias is already off both axes by the time the core
    /// hands them over — so a negative value here is an ordinary sprite entering from the left or
    /// the top, not an error.
    ///
    /// `outcome` defaults to `OffLine`, which is the *awkward* default on purpose: it is what a
    /// perfectly ordinary sprite reports on a line it does not happen to touch, and a `boxes` that
    /// filtered by outcome would drop every fixture in this module rather than a hand-picked one.
    fn walked(index: u8, x: i16, y: i16, wc: u8, hc: u8) -> SpriteEval {
        SpriteEval {
            index,
            y,
            x,
            width_cells: wc,
            height_cells: hc,
            link: 0,
            outcome: SpriteOutcome::OffLine,
        }
    }

    /// A decoded SAT slot, geometry and all. Still needed by the hover callout, which reads `tile`
    /// and `palette` off the table rather than off the walk.
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

    /// Eighty ordinary decoded slots for the outline tests, which read exactly one field out of
    /// this table — `priority`.
    ///
    /// The geometry is deliberately **wrong on purpose**: every slot claims to be a 1x1 sprite at
    /// (-100, -100), which is entirely off screen. A `boxes` that took its rectangles from the
    /// table instead of from the walk therefore produces no boxes at all, rather than plausible
    /// ones — the fixture cannot quietly agree with the bug.
    fn plain_table() -> Vec<SpriteDecoded> {
        (0..80u8).map(|i| sprite(i, -100, -100, 1, 1)).collect()
    }

    #[test]
    fn a_box_is_the_sprites_cells_in_pixels() {
        let b = boxes(&[walked(3, 100, 50, 4, 2)], &plain_table(), (320, 224));
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

    /// **Reachability is the only filter.** Every outcome the core can report must still get a box:
    /// `OffLine` is the ordinary state of a sprite on a line it does not touch, and the two
    /// `Dropped*` outcomes plus `Masked` are per-line facts about *this* line only. Since the
    /// outlines sample one line per frame, filtering by outcome would blank almost every box.
    ///
    /// This is the guard against the tempting wrong fix for the ghost-box bug — "only outline what
    /// actually rendered" — which looks more correct and is much worse.
    #[test]
    fn every_walked_outcome_still_gets_a_box() {
        for outcome in [
            SpriteOutcome::Rendered,
            SpriteOutcome::OffLine,
            SpriteOutcome::DroppedLineLimit,
            SpriteOutcome::DroppedPixelBudget,
            SpriteOutcome::Masked,
        ] {
            let mut e = walked(4, 100, 50, 2, 2);
            e.outcome = outcome;
            assert_eq!(
                boxes(&[e], &plain_table(), (320, 224)).len(),
                1,
                "{outcome:?} lost its box — outcome is a per-line fact, not a reachability one"
            );
        }
    }

    /// **The pairing test for the priority lookup.** `SpriteEval` carries no priority bit, so the
    /// colour is fetched out of the decoded table — and it must be fetched by **SAT index**, never
    /// by position in the walk, because the walk visits slots in link order.
    ///
    /// The fixture makes those two orders disagree on every entry: the walk is slots 5, 2, 9 and
    /// the priority bits sit on slots 2 and 9. Indexing by position would read slots 0, 1, 2 and
    /// produce the wrong colour on all three boxes — while still producing three boxes in the right
    /// places, which is exactly the bug a count or a geometry assertion cannot see.
    #[test]
    fn priority_is_looked_up_by_sat_index_not_by_walk_position() {
        let mut table = plain_table();
        table[2].priority = true;
        table[9].priority = true;
        let walk = [
            walked(5, 10, 10, 1, 1),
            walked(2, 30, 10, 1, 1),
            walked(9, 50, 10, 1, 1),
        ];
        let b = boxes(&walk, &table, (320, 224));
        assert_eq!(
            b.iter().map(|s| (s.index, s.priority)).collect::<Vec<_>>(),
            vec![(5, false), (2, true), (9, true)],
            "the priority bit did not follow the SAT slot the walk named"
        );
    }

    /// **The pairing test for the model.** Boxes come back in walk order, and box `n` must carry
    /// the `n`th walked sprite's own index, own geometry and own priority bit.
    ///
    /// Counting boxes, or checking that the set of rectangles is right, cannot see the bug that
    /// matters here: a `boxes` that paired every rectangle with the *next* sprite's index or
    /// priority produces the same count and the same set of rectangles, and then the outline lens
    /// paints the wrong sprite in the accent colour — a lens that lies about the thing it exists to
    /// point at. Every field is distinct per sprite so no two can be confused, and the sizes differ
    /// so a transposition shows up in the geometry too.
    #[test]
    fn each_box_carries_its_own_sprites_index_size_and_priority() {
        let mut table = plain_table();
        table[7].priority = true;
        table[41].priority = true;
        let b = boxes(
            &[
                walked(7, 10, 20, 1, 4),
                walked(19, 60, 70, 2, 2), // priority stays false
                walked(41, 200, 100, 3, 1),
            ],
            &table,
            (320, 224),
        );
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
                walked(0, -8, 40, 2, 2),
                Rect {
                    x: 0,
                    y: 40,
                    w: 8,
                    h: 16,
                },
            ),
            (
                "entering from the top",
                walked(0, 40, -4, 2, 2),
                Rect {
                    x: 40,
                    y: 0,
                    w: 16,
                    h: 12,
                },
            ),
            (
                "leaving to the right",
                walked(0, 312, 40, 2, 2),
                Rect {
                    x: 312,
                    y: 40,
                    w: 8,
                    h: 16,
                },
            ),
            (
                "leaving through the bottom",
                walked(0, 40, 220, 2, 2),
                Rect {
                    x: 40,
                    y: 220,
                    w: 16,
                    h: 4,
                },
            ),
        ] {
            let b = boxes(&[s], &plain_table(), (320, 224));
            assert_eq!(b.len(), 1, "{label}: the sprite was dropped");
            assert_eq!(b[0].rect, want, "{label}");
        }
        // Both axes at once, since the two clips are independent code.
        let b = boxes(&[walked(0, -8, -4, 2, 2)], &plain_table(), (320, 224));
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
            boxes(&[walked(0, 0, -128, 4, 4)], &plain_table(), (320, 224)).is_empty(),
            "the parked idiom"
        );
        assert!(
            boxes(&[walked(0, 320, 40, 4, 4)], &plain_table(), (320, 224)).is_empty(),
            "flush past the right edge"
        );
        assert!(
            boxes(&[walked(0, 40, 224, 4, 4)], &plain_table(), (320, 224)).is_empty(),
            "flush past the bottom edge"
        );
        // H32's display is narrower, so a sprite visible in H40 can be entirely outside it.
        assert!(
            boxes(&[walked(0, 260, 40, 1, 1)], &plain_table(), (256, 224)).is_empty(),
            "outside H32's 256-dot display"
        );
    }

    // **`slots_past_parsed_max_are_not_outlined` was removed here, deliberately, and this note is
    // its receipt.** It asserted that `boxes` stopped at the 64/80 parse cap, which it enforced
    // with a `.take(parsed_max)` over the whole SAT. `boxes` no longer sees the table: it is handed
    // the link walk, and the walk stops at the cap itself (`render.rs:1091` iterates `0..cap` from
    // `sprite_limits`, reporting `SpriteWalkEnd::MaxCount`). Re-asserting the cap against a
    // hand-written `Vec<SpriteEval>` would test the fixture's length, not the production rule —
    // the test would pass with the cap deleted from the core.
    //
    // The behaviour is not left unguarded: `the_walk_stops_at_the_parse_cap_in_h32` in `lens/mod.rs`
    // drives it end to end through a real VDP, where the cap is genuinely in play, and it is
    // mutation-verified there.

    // --- The sprite outlines: the draw --------------------------------------------------------

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
        let bx = boxes(&[walked(0, 100, 50, 4, 2)], &plain_table(), (320, 224));
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
            let got = ink_bounds(&buf, w, BG).unwrap_or_else(|| panic!("{label}: painted nothing"));
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
        let bx = boxes(&[walked(0, 100, 50, 4, 2)], &plain_table(), (320, 224));
        let buf = render_sprites(w, h, area, px, (320, 224), &bx);
        // The window box is (200,100)..(264,132) — see the scale test above, same fixture.
        let (bl, bt, br, bb) = (200usize, 100usize, 264usize, 132usize);
        let (midx, midy) = (bl + (br - bl) / 2, bt + (bb - bt) / 2);
        let at = |x: usize, y: usize| buf[y * w + x];

        // Translucent, not opaque — and this is the only guard on that. `OUTLINE_ALPHA` at 255
        // passes every other assertion in this module, because they all ask "is this pixel BG?".
        // An opaque stroke would hide the sprite's own edge pixels, which are exactly what you are
        // looking at when you ask where a sprite's box really is. Pinned as an inequality against
        // the *unblended* colour, which is what an alpha of 255 would deposit; the blend itself is
        // never recomputed here, since an expected value computed from `Canvas` could not catch a
        // bug in the alpha handed to it.
        assert_ne!(
            at(midx, bt),
            crate::overlay::INFO,
            "the outline is fully opaque — it should be blended, so the sprite shows through"
        );
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
        // Two sprites, fixed; only which of them owns the priority bit moves between renders.
        let walk = [walked(0, 10, 10, 2, 2), walked(1, 100, 100, 2, 2)];
        let render = |flag_on: usize| {
            let mut table = plain_table();
            table[flag_on].priority = true;
            let bx = boxes(&walk, &table, (320, 224));
            assert_eq!(bx.len(), 2, "both sprites are on screen");
            render_sprites(w, h, area, 1, (320, 224), &bx)
        };
        // At 2x the boxes' top-left corners are (20,20) and (200,200).
        let (p0, p1) = (20 * w + 20, 200 * w + 200);

        let a = render(1); // slot 0 ordinary, slot 1 high priority
        let b = render(0); // the flag moved to slot 0

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
                walked(0, -8, -8, 4, 4),
                walked(1, 0, 0, 4, 4),
                walked(2, 300, 210, 4, 4),
                walked(3, 316, 220, 1, 1),
            ],
            &plain_table(),
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

    // --- The CRAM strip: its fixtures, then its model, then its draw ---------------------------

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
        let (top, bottom, left, right) = ink_bounds(&buf, w, BG).expect("draw painted nothing");

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
                let Some((top, bottom, left, right)) = ink_bounds(&buf, w, BG) else {
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
            let (t, b, l, r) = ink_bounds(&render(w, h, area, px, &sw), w, BG)
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

    // --- The hover callout: its model ----------------------------------------------------------

    /// An attribution with everything the formatter does not read left at a default. The two
    /// coordinates are read (they are what `sprite_tile_at` resolves against), so they are
    /// parameters rather than constants.
    fn attr(winner: Layer, x: u16, y: u16) -> PixelAttribution {
        PixelAttribution {
            x,
            y,
            winner,
            cram_index: 0,
            rgb: (0, 0, 0),
            state: PixelState::Normal,
            cell: None,
            candidates: Vec::new(),
        }
    }

    fn cell(tile: u16, palette: u8, priority: bool) -> Cell {
        Cell {
            tile,
            palette,
            hflip: false,
            vflip: false,
            priority,
        }
    }

    /// The whole string, not a substring. Membership — "it mentions $4A0 somewhere" — is blind to
    /// the pairing bugs that matter: a callout that printed the palette where the priority goes,
    /// or that named plane B while reading plane A's cell, contains every expected token and is
    /// still a readout that lies.
    ///
    /// All three cell-bearing layers are checked with the *same* cell, so a transposed match arm
    /// cannot hide behind a different fixture, and `pri`/`pal` are given distinct values so a
    /// swapped pair shows up.
    #[test]
    fn a_plane_pixel_names_its_tile_palette_and_priority() {
        for (layer, want) in [
            (Layer::PlaneA, "plane A | tile $4A0 | pal 2 | pri 1"),
            (Layer::PlaneB, "plane B | tile $4A0 | pal 2 | pri 1"),
            (Layer::Window, "window | tile $4A0 | pal 2 | pri 1"),
        ] {
            let mut a = attr(layer, 10, 10);
            a.cell = Some(cell(0x4A0, 2, true));
            assert_eq!(hover_text(&a, &[]), want, "{layer:?}");
        }

        // A low tile is zero-padded to three digits, matching every other `$`-hex spelling in the
        // frontend, and `pri 0` is a different glyph from `pri 1`.
        let mut a = attr(Layer::PlaneA, 10, 10);
        a.cell = Some(cell(0x00B, 0, false));
        assert_eq!(hover_text(&a, &[]), "plane A | tile $00B | pal 0 | pri 0");

        // A blanked line has a winning layer and no nametable cell behind it. Saying so beats
        // printing `tile $000`, which is a real tile.
        assert_eq!(
            hover_text(&attr(Layer::PlaneB, 10, 10), &[]),
            "plane B | no cell"
        );
    }

    /// The sprite branch, including the case the branch exists for: the SAT moved between the frame
    /// being drawn and this read, the winner's box no longer contains the dot, and there is no
    /// honest tile to name.
    ///
    /// The tile is resolved **per cell**, not read off the sprite's base, so the second probe sits
    /// one cell to the right and must come back one column of the pattern further on — a formatter
    /// that printed `s.tile` would pass the first probe and fail here.
    #[test]
    fn a_sprite_pixel_names_its_slot_and_says_so_when_the_tile_moved() {
        let mut s = sprite(12, 100, 50, 4, 2);
        s.tile = 0x120;
        s.palette = 3;
        s.priority = true;
        // Indexed by SAT slot, as `sprites_decoded` hands them over — so slot 12 must be at 12.
        let mut sat = vec![sprite(0, -128, -128, 1, 1); 13];
        sat[12] = s;

        assert_eq!(
            hover_text(&attr(Layer::Sprite(12), 100, 50), &sat),
            "slot 12 | tile $120 | pal 3 | pri 1",
            "the top-left dot is the sprite's base pattern"
        );
        assert_eq!(
            hover_text(&attr(Layer::Sprite(12), 108, 50), &sat),
            "slot 12 | tile $122 | pal 3 | pri 1",
            "one cell right is one column of the pattern on (column-major, 2 cells tall)"
        );
        // The dot is nowhere near the sprite: the SAT moved since the frame was drawn.
        assert_eq!(
            hover_text(&attr(Layer::Sprite(12), 10, 10), &sat),
            "slot 12 | tile ? | pal 3 | pri 1",
            "a tile was invented for a dot the winning sprite does not cover"
        );
        // A winner past the end of the decode: no tile, and no palette or priority either.
        assert_eq!(
            hover_text(&attr(Layer::Sprite(70), 100, 50), &sat),
            "slot 70 | out of range"
        );
    }

    /// The backdrop has no cell and no sprite — the only thing behind it is the CRAM entry reg $07
    /// selects, so that is what the callout names, in the same `(pal, col)` decomposition
    /// `pick.rs` prints.
    ///
    /// 37 is chosen so the three numbers are all different (37, 2, 5): with an entry like 32 the
    /// palette and the colour would both be readable as the wrong field.
    #[test]
    fn the_backdrop_names_its_cram_entry() {
        let mut a = attr(Layer::Backdrop, 10, 10);
        a.cram_index = 37;
        assert_eq!(hover_text(&a, &[]), "backdrop | cram 37 (pal 2 col 5)");
        // Entry 0 is the common case and must not collapse into something else.
        assert_eq!(
            hover_text(&attr(Layer::Backdrop, 10, 10), &[]),
            "backdrop | cram 0 (pal 0 col 0)"
        );
        // The last entry, where `/16` and `%16` are both at their maximum.
        let mut a = attr(Layer::Backdrop, 10, 10);
        a.cram_index = 63;
        assert_eq!(hover_text(&a, &[]), "backdrop | cram 63 (pal 3 col 15)");
    }

    // --- The hover callout: its draw -----------------------------------------------------------

    /// Render one callout into a `w * h` buffer over [`BG`] and hand back the buffer.
    fn render_hover(
        w: usize,
        h: usize,
        area: Rect,
        px: usize,
        native: (usize, usize),
        hv: &Hover,
    ) -> Vec<u32> {
        let mut buf = vec![BG; w * h];
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw_hover(&mut c, area, px, native, hv);
        }
        buf
    }

    /// The picture the placement tests below anchor to: a 2x blit of a 320x224 frame, offset on
    /// both axes so an anchor that ignored `area` and measured from the window corner cannot pass.
    const HOVER_AREA: Rect = Rect {
        x: 30,
        y: 20,
        w: 640,
        h: 448,
    };

    /// **Where the callout lands, on both axes, in all four flip combinations.**
    ///
    /// Containment is not enough here and never was: a callout pinned only to be "inside the
    /// picture" is satisfied by one that never flips at all, as long as it is clipped — and by one
    /// that flips both axes always. Every expected number is written out by hand rather than taken
    /// from `native_rect_to_window` or from the panel arithmetic, so no expectation moves with the
    /// bug it is supposed to catch.
    ///
    /// The two axes are exercised **independently**, which is the point of having four rows rather
    /// than two: a flip written as `if either edge would overflow, flip both` produces the right
    /// answer for the corner case and the wrong one for each edge on its own.
    ///
    /// Derivation, once, for the reader: at 2x, game dot `g` occupies window columns
    /// `[30 + 2g, 30 + 2g + 2)`. `"AB"` is 11 unscaled ink pixels, so at `px = 2` the panel is
    /// `11 * 2 + 2 * (2 * 2) = 30` wide and `7 * 2 + 2 * (2 * 2) = 22` tall, and the gap is
    /// `4 * 2 = 8`. The picture spans columns 30..670 and rows 20..468.
    #[test]
    fn the_callout_flips_rather_than_leaving_the_picture() {
        let (w, h) = (700usize, 520usize);
        let px = 2;
        // (label, dot, left, top) — the panel is always 30x22, so its far edges follow.
        for (label, at, left, top) in [
            (
                "neither edge is near: down and to the right",
                (10u16, 10u16),
                58usize,
                48usize,
            ),
            (
                "the right edge: the panel goes to the left of the dot",
                (315, 10),
                622,
                48,
            ),
            (
                "the bottom edge: the panel goes above the dot",
                (10, 220),
                58,
                430,
            ),
            ("the bottom-right corner: both flip", (315, 220), 622, 430),
        ] {
            let hv = Hover {
                text: "AB".to_string(),
                at,
            };
            let buf = render_hover(w, h, HOVER_AREA, px, (320, 224), &hv);
            let got = ink_bounds(&buf, w, BG).unwrap_or_else(|| panic!("{label}: painted nothing"));
            assert_eq!(
                got,
                (top, top + 21, left, left + 29),
                "{label}: the callout is at (top,bottom,left,right) {got:?}, not \
                 ({top},{},{left},{})",
                top + 21,
                left + 29
            );
        }
    }

    /// The callout stays inside the picture **at every dot, every scale, and every picture size** —
    /// the letterbox must stay black.
    ///
    /// A four-corner probe cannot stand in for this. The flip's two clamps only bind on a picture
    /// narrower (or shorter) than twice the panel, where flipping alone would push the panel out
    /// the *opposite* edge; that is a range no single comfortable geometry ever visits, so the
    /// third and fourth rows below exist to visit it. The `wide` rows do the same job for the
    /// width clamp, with a callout longer than the whole picture.
    ///
    /// [`Aspect::Integer`] for the two full-size rows rather than the house-default `Tv`: `Tv` fits
    /// a 4:3 picture into the window's larger axis, so one of `area.x`/`area.y` is always 0, and a
    /// containment sweep against a zero offset degenerates into `x >= 0` — which every pixel
    /// satisfies. At 700x520 the integer scale is 2 and the picture is inset on both axes.
    #[test]
    fn the_callout_stays_inside_the_picture_at_every_dot() {
        let (w, h) = (700usize, 520usize);
        let big = crate::present::dest_rect(w, h, 320, 224, Aspect::Integer);
        assert!(
            big.x > 0 && big.y > 0,
            "this window must letterbox on both axes or the sweep checks nothing: {big:?}"
        );
        let wide = "SLOT 12 | TILE $4A0 | PAL 2 | PRI 1 | AND THEN SOME MORE WORDS AGAIN";
        for (label, area, px, text) in [
            ("2x picture, short callout", big, 2usize, "AB"),
            ("2x picture, callout wider than the picture", big, 2, wide),
            ("4x text in a 2x picture", big, 4, "SLOT 12 | TILE $4A0"),
            (
                "a picture barely wider and taller than the callout",
                Rect {
                    x: 10,
                    y: 10,
                    w: 100,
                    h: 20,
                },
                1,
                wide,
            ),
        ] {
            let mut drew = 0usize;
            for gy in (0..224u16).step_by(5) {
                for gx in (0..320u16).step_by(7) {
                    let hv = Hover {
                        text: text.to_string(),
                        at: (gx, gy),
                    };
                    let buf = render_hover(w, h, area, px, (320, 224), &hv);
                    let mut any = false;
                    for (i, p) in buf.iter().enumerate() {
                        if *p == BG {
                            continue;
                        }
                        any = true;
                        let (x, y) = (i % w, i / w);
                        assert!(
                            x >= area.x
                                && x < area.x + area.w
                                && y >= area.y
                                && y < area.y + area.h,
                            "{label}: the callout for dot ({gx},{gy}) escaped the picture at \
                             ({x},{y}) — the letterbox must stay black"
                        );
                    }
                    drew += usize::from(any);
                }
            }
            assert!(drew > 0, "{label}: the sweep never drew anything at all");
        }
    }

    /// The panel and the text both reach the glass.
    ///
    /// The floor is the invisible-ink guard every lens draw test carries: text alone can never
    /// account for `panel_w * panel_h` changed pixels, so a panel that had gone invisible — drawn
    /// in a colour that happens to match, or not drawn at all — fails here rather than passing the
    /// containment sweep above untouched. The [`overlay::INFO`](crate::overlay::INFO) probe is the
    /// other half: the glyphs are drawn at alpha 255, so they land on the buffer *exactly*, and
    /// deleting the `c.text` call leaves the floor satisfied by the panel on its own.
    ///
    /// Both numbers are written out by hand: `"AB"` is 11 unscaled ink pixels, so at `px = 2` the
    /// panel is 30 x 22.
    #[test]
    fn the_callout_paints_both_its_panel_and_its_text() {
        let (w, h) = (700usize, 520usize);
        let hv = Hover {
            text: "AB".to_string(),
            at: (10, 10),
        };
        let buf = render_hover(w, h, HOVER_AREA, 2, (320, 224), &hv);
        let painted = buf.iter().filter(|p| **p != BG).count();
        assert!(
            painted >= 30 * 22,
            "the panel left no mark: {painted} changed, the panel is 30x22"
        );
        assert!(
            buf.contains(&crate::overlay::INFO),
            "no glyph reached the glass — the callout is a blank panel"
        );
    }

    /// The font scale must scale the panel **and carry its anchor with it**.
    ///
    /// One scale is not enough, and `px = 1` is the worst single choice available: `pad = 2 * px`
    /// and the `CALLOUT_GAP` are both 4 device pixels there, and `GLYPH_H * px + 2 * pad` is 11 —
    /// the same 11 that `GLYPH_H * px + 4` and `GLYPH_H + 2 * pad` also give. Three of the four
    /// ways to write the panel's height are identity at `px = 1` and diverge at `px = 2`.
    ///
    /// Every number below is derived by hand from the house idiom (`pad = 2 * px`,
    /// `gap = 4 * px`, ink width `6 * chars - 1` scaled by `px`) against [`HOVER_AREA`], whose 2x
    /// blit puts game dot 10 at window column `30 + 20 = 50` and row `20 + 20 = 40` at every font
    /// scale — the font scale moves the panel relative to the dot, never the dot itself.
    ///
    /// **The glyphs are measured separately from the panel**, and that is not belt and braces:
    /// the panel's own bounds are blind to where the text sits inside it, because the text is
    /// strictly within the panel either way. Drawing the run at the panel's corner instead of one
    /// `pad` in survived every other assertion in this module — measured — leaving the words
    /// jammed against the panel edge at every scale. The glyphs are alpha-255 `INFO`, so they can
    /// be located exactly, with no blend to recompute: `"AB"` inks columns 0..4 of both cells, so
    /// the run is `11 * px` by `GLYPH_H * px`, at `pad` in from the panel's top-left.
    #[test]
    fn the_callout_anchors_and_scales_at_every_font_scale() {
        let (w, h) = (700usize, 520usize);
        let hv = Hover {
            text: "AB".to_string(),
            at: (10, 10),
        };
        // (px, left, top, panel_w, panel_h)
        for (px, left, top, panel_w, panel_h) in [
            (1usize, 54usize, 44usize, 15usize, 11usize),
            (2, 58, 48, 30, 22),
            (4, 66, 56, 60, 44),
        ] {
            let buf = render_hover(w, h, HOVER_AREA, px, (320, 224), &hv);
            let (t, b, l, r) =
                ink_bounds(&buf, w, BG).unwrap_or_else(|| panic!("px {px} painted nothing"));
            assert_eq!(
                (l, t),
                (left, top),
                "px {px}: the callout is anchored at ({l},{t}), not ({left},{top})"
            );
            assert_eq!(
                (r - l + 1, b - t + 1),
                (panel_w, panel_h),
                "px {px}: the callout did not scale"
            );

            // The glyph run, located by its own colour rather than by "not BG".
            let mut glyphs: Option<(usize, usize, usize, usize)> = None;
            for (i, p) in buf.iter().enumerate() {
                if *p != crate::overlay::INFO {
                    continue;
                }
                let (x, y) = (i % w, i / w);
                glyphs = Some(match glyphs {
                    None => (y, y, x, x),
                    Some((gt, gb, gl, gr)) => (gt.min(y), gb.max(y), gl.min(x), gr.max(x)),
                });
            }
            let (gt, gb, gl, gr) =
                glyphs.unwrap_or_else(|| panic!("px {px}: no glyph reached the glass"));
            let pad = 2 * px;
            assert_eq!(
                (gl, gt),
                (left + pad, top + pad),
                "px {px}: the text starts at ({gl},{gt}), not one pad in from the panel's corner"
            );
            assert_eq!(
                (gr - gl + 1, gb - gt + 1),
                (11 * px, font::GLYPH_H * px),
                "px {px}: the text did not scale with the panel"
            );
        }
    }

    /// A picture too short for one line of text draws **nothing**, rather than a panel hanging out
    /// of the bottom of it: `Canvas` clips at the buffer edge, not at `area`, so there is no free
    /// containment here — the guard is the only thing keeping ink out of the letterbox.
    ///
    /// The comfortable row is the vacuity guard: without it a `draw_hover` that had stopped drawing
    /// altogether would pass this test outright.
    #[test]
    fn a_picture_too_short_for_the_callout_draws_nothing() {
        let (w, h) = (320usize, 240usize);
        let hv = Hover {
            text: "AB".to_string(),
            at: (10, 10),
        };
        for short in 0..11usize {
            let area = Rect {
                x: 0,
                y: 0,
                w: 320,
                h: short,
            };
            let buf = render_hover(w, h, area, 1, (320, 224), &hv);
            assert!(
                buf.iter().all(|p| *p == BG),
                "a callout was drawn into a picture {short} rows tall, which cannot hold its 11"
            );
        }
        let area = Rect {
            x: 0,
            y: 0,
            w: 320,
            h: 11,
        };
        assert!(
            render_hover(w, h, area, 1, (320, 224), &hv)
                .iter()
                .any(|p| *p != BG),
            "a picture exactly as tall as the callout must still draw it"
        );
    }
}
