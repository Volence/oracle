//! Display geometry: how the native `width x 224` Genesis frame is placed inside the host window, how it is
//! scaled up into the presented buffer, and the exact inverse a mouse click needs.
//!
//! ## Why the frontend does its own scaling
//!
//! `minifb` can scale for us ([`ScaleMode`](minifb::ScaleMode)), but only into aspect ratios it chooses, and
//! it never tells the caller where it put the image — so the click-to-watch inverse had to *assume* the
//! geometry (a fixed window at a fixed integer scale). That assumption is what stopped the window being
//! resizable. Here the frontend computes the destination rectangle itself, blits into a window-sized buffer,
//! and hands minifb a 1:1 [`ScaleMode::Stretch`](minifb::ScaleMode::Stretch) present. Three things fall out
//! of that:
//!
//! * the window can be **resized** (and maximised / fullscreened by the window manager) with the picture
//!   staying correct, because the rectangle is recomputed from `Window::get_size` every frame;
//! * the click inverse is [`window_to_native`] — the exact inverse of the blit, not a re-derivation;
//! * **overlays draw at window resolution** (see [`crate::font`]), so on-screen text is crisp at any size
//!   instead of being an 8-pixel-tall smudge scaled up with the game.
//!
//! ## Aspect
//!
//! A Mega Drive does not have square pixels. H40 puts 320 dots and H32 puts 256 dots across the *same*
//! physical width of a 4:3 television, so the correct picture is "the active area, letterboxed to 4:3" in
//! both modes — H32 is stretched wider, not pillarboxed. [`Aspect::Tv`] is that, and it is the default
//! because this is a *player*. [`Aspect::Square`] and [`Aspect::Integer`] preserve the pixel grid instead,
//! which is what you want when you are counting pixels rather than playing; `Integer` additionally refuses
//! any non-integer scale, so no row or column is ever duplicated unevenly.

/// How the native frame is fitted into the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Aspect {
    /// 4:3 — what the console's output looked like on the television it was designed for. Non-square pixels
    /// in both H40 (320 dots stretched to 4:3) and H32 (256 dots stretched to the same 4:3 box).
    #[default]
    Tv,
    /// Square pixels: the native `width:height` ratio, scaled to fill the window at any (fractional) scale.
    Square,
    /// Square pixels at the largest **integer** scale that fits — no unevenly duplicated rows or columns.
    Integer,
}

impl Aspect {
    /// Parse the `--aspect` value. Accepts the obvious spellings; `None` for anything else (the caller turns
    /// that into a usage error rather than guessing).
    pub fn from_name(s: &str) -> Option<Aspect> {
        match s.to_ascii_lowercase().as_str() {
            "tv" | "4:3" | "43" | "correct" => Some(Aspect::Tv),
            "square" | "1:1" | "pixel" | "native" => Some(Aspect::Square),
            "integer" | "int" | "sharp" => Some(Aspect::Integer),
            _ => None,
        }
    }

    /// Short name for the on-screen HUD and the startup log.
    pub fn name(self) -> &'static str {
        match self {
            Aspect::Tv => "4:3",
            Aspect::Square => "square",
            Aspect::Integer => "integer",
        }
    }
}

/// Where the scaled native frame lands inside the window, in device pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

/// Place a `src_w x src_h` native frame inside a `win_w x win_h` window under `aspect`, centered, never
/// larger than the window. A degenerate input (a zero dimension anywhere) yields a zero-sized rect rather
/// than a panic — a window manager really can hand out a 0-height window mid-resize.
///
/// All three modes are integer arithmetic, so the result is exactly reproducible in tests, and the ratio is
/// **exact** rather than approximate: the target is a reduced fraction `an:ad` ([`reduced_ratio`]) and the
/// picture is the largest whole multiple of it that fits. Deriving the height from a maximal width instead
/// would leave the picture a pixel or two off 4:3 at most window sizes.
pub fn dest_rect(win_w: usize, win_h: usize, src_w: usize, src_h: usize, aspect: Aspect) -> Rect {
    if win_w == 0 || win_h == 0 || src_w == 0 || src_h == 0 {
        return Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
    }
    let (w, h) = match aspect {
        Aspect::Integer => {
            // Largest integer scale that fits; at least 1, so a window smaller than the frame still shows
            // (cropped by the blit's clipping) instead of vanishing.
            let s = (win_w / src_w).min(win_h / src_h).max(1);
            (src_w * s, src_h * s)
        }
        Aspect::Tv | Aspect::Square => {
            // The largest exact multiple of the target ratio that fits. Working in whole `an:ad` units
            // rather than "widest width that fits, then derive the height" is what makes the result an
            // *exact* ratio: the derived-height form leaves the picture a pixel or two off 4:3 whenever the
            // width is not a multiple of 4, which is most window sizes.
            let (an, ad) = reduced_ratio(aspect, src_w, src_h);
            let k = (win_w / an).min(win_h / ad).max(1);
            (an * k, ad * k)
        }
    };
    // Center. `saturating_sub` keeps an oversized Integer-mode frame anchored at 0 rather than wrapping.
    Rect {
        x: win_w.saturating_sub(w) / 2,
        y: win_h.saturating_sub(h) / 2,
        w,
        h,
    }
}

/// The target aspect as a *reduced* fraction `an:ad`. Reducing matters: the unreduced native ratio (320:224)
/// would make the `an:ad` unit an entire 320-pixel step, collapsing [`Aspect::Square`] into
/// [`Aspect::Integer`]; reduced to 10:7 the step is 10 pixels and the fit is fine-grained again.
fn reduced_ratio(aspect: Aspect, src_w: usize, src_h: usize) -> (usize, usize) {
    let (an, ad) = match aspect {
        Aspect::Tv => (4, 3),
        _ => (src_w, src_h),
    };
    let mut a = an;
    let mut b = ad;
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    let g = a.max(1);
    (an / g, ad / g)
}

/// A source frame to present: packed `0x00RR_GGBB` pixels and the dimensions they are laid out with. Carried
/// together because they are only ever meaningful together, and a transposed pair would silently blit
/// nonsense rather than fail to compile.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub px: &'a [u32],
    pub w: usize,
    pub h: usize,
}

/// Nearest-neighbour blit of the native frame into a window-sized presentation buffer.
///
/// `dst` is resized to `win_w * win_h` and cleared to `bg`, then `src` (`src_w x src_h`, row-major packed
/// `0x00RR_GGBB`) is scaled into `rect`. The per-column source index is computed once into `xmap` — a
/// caller-owned scratch buffer so the steady-state path allocates nothing — because the alternative is a
/// division per output pixel, and there are ~650k of those at 960x672.
///
/// Anything of `rect` that falls outside the window is clipped. A `src` too short for `src_w * src_h` (which
/// cannot happen from `blit_capture`, but is cheap to survive) leaves the missing rows as background.
pub fn scale_into(
    dst: &mut Vec<u32>,
    win_w: usize,
    win_h: usize,
    src: Frame<'_>,
    rect: Rect,
    bg: u32,
    xmap: &mut Vec<usize>,
) {
    let Frame {
        px: src,
        w: src_w,
        h: src_h,
    } = src;
    dst.clear();
    dst.resize(win_w * win_h, bg);
    if rect.w == 0 || rect.h == 0 || src_w == 0 || src_h == 0 {
        return;
    }
    // Visible span of the destination rect, in rect-local coordinates.
    let vis_w = rect.w.min(win_w.saturating_sub(rect.x));
    let vis_h = rect.h.min(win_h.saturating_sub(rect.y));
    if vis_w == 0 || vis_h == 0 {
        return;
    }
    xmap.clear();
    xmap.reserve(vis_w);
    for dx in 0..vis_w {
        xmap.push((dx * src_w / rect.w).min(src_w - 1));
    }
    for dy in 0..vis_h {
        let sy = (dy * src_h / rect.h).min(src_h - 1);
        let row_start = sy * src_w;
        let Some(row) = src.get(row_start..row_start + src_w) else {
            continue;
        };
        let out_start = (rect.y + dy) * win_w + rect.x;
        let out = &mut dst[out_start..out_start + vis_w];
        for (o, &sx) in out.iter_mut().zip(xmap.iter()) {
            *o = row[sx];
        }
    }
}

/// The inverse of [`scale_into`]: map a window-pixel click to the native VDP dot under it, or `None` when the
/// click landed in the letterbox / pillarbox rather than on the picture.
///
/// This is an inverse of the *actual* blit, not an independent derivation of it, which is what makes
/// click-to-watch survive an arbitrary window size — the bug the old fixed-scale version had waiting for the
/// first person to resize the window.
pub fn window_to_native(
    mx: f32,
    my: f32,
    rect: Rect,
    src_w: usize,
    src_h: usize,
) -> Option<(u16, u16)> {
    if mx < 0.0 || my < 0.0 || rect.w == 0 || rect.h == 0 || src_w == 0 || src_h == 0 {
        return None;
    }
    let (mx, my) = (mx as usize, my as usize);
    let dx = mx.checked_sub(rect.x)?;
    let dy = my.checked_sub(rect.y)?;
    if dx >= rect.w || dy >= rect.h {
        return None; // in the box around the picture, or past the window
    }
    let sx = (dx * src_w / rect.w).min(src_w - 1);
    let sy = (dy * src_h / rect.h).min(src_h - 1);
    Some((sx as u16, sy as u16))
}

/// The **forward** map — game-pixel rect to the window pixels that show it, the inverse of
/// [`window_to_native`] wherever an inverse can exist. Lenses that mark things *in the picture*
/// (sprite outlines, a hover callout anchored to a dot) need this; nothing needed it before,
/// because everything drawn so far was anchored to the picture's corners rather than to a pixel
/// inside it.
///
/// Derived from [`scale_into`]'s blit rather than guessed: that maps window column `dx` to source
/// column `floor(dx * src_w / rect.w)`, so the window columns showing game column `gx` are
/// `[ceil(gx * rect.w / src_w), ceil((gx + 1) * rect.w / src_w))`. **The ceiling is not
/// cosmetic** — the floor form is off by one on most non-integer scales, which is one column of
/// outline sitting beside the sprite instead of on it.
///
/// **The round-trip is an *upscale* property.** It holds when the picture is at least 1:1
/// (`rect.w >= src_w`, `rect.h >= src_h`) — the normal case, and the one
/// `the_forward_map_is_the_inverse_of_the_click_map` pins. [`dest_rect`] really can go the other
/// way: a 200x150 window in [`Aspect::Tv`] gives `rect.w = 200 < 320`, and there several game
/// pixels share one window pixel, so [`window_to_native`] on the returned span names whichever of
/// them the blit kept. Below 1:1 the guarantee is weaker and deliberate: a non-empty rect **inside
/// the picture**, near the right place, rather than nothing. The `min`/`max` clamps below are what
/// deliver it — without them a whole sprite maps to a zero-width rect and vanishes, and widening
/// that back to one pixel without also pulling the start edge in puts the last game column's
/// hairline one pixel into the letterbox.
///
/// `g` is in game pixels and must be non-negative (callers clip against `active_display` first).
/// Returns `None` when the rect is empty, the geometry is degenerate, or `g` starts outside the
/// picture; the result is always at least 1x1 so a hairline never vanishes at small scales.
///
/// The result is clipped to the **picture**, not to the **window** — those are different claims.
/// [`dest_rect`] can hand back a rect larger than the window ([`Aspect::Integer`] clamps its scale
/// at 1, so a window smaller than 320x224 still gets a 320x224 rect, anchored at 0 and cropped by
/// the blit), and then a returned rect runs off the window edge too. Drawing is still safe:
/// [`font::Canvas`](crate::font::Canvas) clips at the buffer edge and never panics.
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
    // The near edge is clamped one short of the far side, and the far edge is then held at least one
    // past it. Both clamps are identity while the picture is at least 1:1 — every span is non-empty
    // there and the last game column's span starts at `rect.w - 1` at the latest — and both bind
    // below 1:1, where several game pixels share a window pixel and the last column's span is
    // *empty*, starting exactly at `rect.w`. Widening that to one pixel without first pulling the
    // start back would put the hairline in the letterbox, one column outside the picture it is
    // supposed to be marking.
    let x0 = edge(g.x, rect.w, src_w).min(rect.w - 1);
    let y0 = edge(g.y, rect.h, src_h).min(rect.h - 1);
    let x1 = edge(gx1, rect.w, src_w).max(x0 + 1);
    let y1 = edge(gy1, rect.h, src_h).max(y0 + 1);
    Some(Rect {
        x: rect.x + x0,
        y: rect.y + y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

/// The window size to open for `scale` (an integer multiple of the native frame's *height*) under `aspect`.
/// The height is always `src_h * scale`; only the width differs, because that is the axis the aspect choice
/// is about — so `--scale 3` gives the same picture size vertically in every mode.
pub fn initial_window_size(
    scale: usize,
    src_w: usize,
    src_h: usize,
    aspect: Aspect,
) -> (usize, usize) {
    let scale = scale.max(1);
    let h = src_h * scale;
    let w = match aspect {
        Aspect::Tv => h * 4 / 3,
        Aspect::Square | Aspect::Integer => src_w * scale,
    };
    (w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: usize = 224;

    /// 4:3 is 4:3 in **both** display modes — that is the whole point of `Tv`. A real console puts H32's 256
    /// dots and H40's 320 dots across the same television width, so the presented rectangle must be identical
    /// for the two, and H32 must be stretched rather than pillarboxed.
    #[test]
    fn tv_aspect_is_four_by_three_in_both_h32_and_h40() {
        let (ww, wh) = (1000, 700);
        let h40 = dest_rect(ww, wh, 320, H, Aspect::Tv);
        let h32 = dest_rect(ww, wh, 256, H, Aspect::Tv);
        assert_eq!(h40, h32, "H32 and H40 fill the same 4:3 box");
        assert_eq!(h40.w * 3, h40.h * 4, "the box really is 4:3");
        assert!(h40.w <= ww && h40.h <= wh, "and it fits inside the window");
        // Height-bound here (700 * 4/3 = 933 < 1000), so the picture is 933x700 centered horizontally.
        assert_eq!(
            h40,
            Rect {
                x: 34,
                y: 0,
                w: 932,
                h: 699
            }
        );
    }

    /// A window wider than 4:3 pillarboxes; a window taller than 4:3 letterboxes. Both centered.
    #[test]
    fn the_picture_is_centered_in_whichever_axis_has_slack() {
        // Very wide window: width is limited by the height.
        let wide = dest_rect(2000, 600, 320, H, Aspect::Tv);
        assert_eq!(wide.h, 600, "height fills");
        assert!(wide.x > 0 && wide.y == 0, "pillarboxed, not letterboxed");
        assert_eq!(wide.x, (2000 - wide.w) / 2);
        // Very tall window: width fills, black bars top and bottom.
        let tall = dest_rect(640, 2000, 320, H, Aspect::Tv);
        assert_eq!(tall.w, 640, "width fills");
        assert!(tall.y > 0 && tall.x == 0, "letterboxed");
        assert_eq!(tall.y, (2000 - tall.h) / 2);
    }

    /// Square mode preserves the *native* pixel grid's ratio, so H32 and H40 differ — and neither is 4:3.
    #[test]
    fn square_aspect_preserves_the_native_ratio() {
        let (ww, wh) = (960, 672);
        let h40 = dest_rect(ww, wh, 320, H, Aspect::Square);
        assert_eq!(
            h40,
            Rect {
                x: 0,
                y: 0,
                w: 960,
                h: 672
            },
            "320x224 at 3x fills exactly"
        );
        let h32 = dest_rect(ww, wh, 256, H, Aspect::Square);
        assert_eq!(h32.h, 672);
        assert_eq!(h32.w, 768, "H32 keeps its 8:7 grid → pillarboxed");
        assert_eq!(h32.x, (960 - 768) / 2);
    }

    /// Integer mode picks a whole scale and never a fraction, so no row or column is duplicated unevenly.
    #[test]
    fn integer_aspect_snaps_to_a_whole_scale() {
        // 3.4x would fit by area; integer mode takes 3x and leaves a border.
        let r = dest_rect(1100, 780, 320, H, Aspect::Integer);
        assert_eq!(r.w, 320 * 3);
        assert_eq!(r.h, H * 3);
        assert_eq!((r.x, r.y), ((1100 - 960) / 2, (780 - 672) / 2));
        // Exactly 2x fits exactly.
        assert_eq!(dest_rect(640, 448, 320, H, Aspect::Integer).w, 640);
        // A window smaller than one native frame still shows something at 1x rather than vanishing.
        let tiny = dest_rect(100, 100, 320, H, Aspect::Integer);
        assert_eq!((tiny.w, tiny.h), (320, H));
    }

    /// Degenerate window/frame sizes (a 0-height window during a drag-resize) yield an empty rect, and the
    /// blit and the click inverse both survive it.
    #[test]
    fn degenerate_sizes_are_empty_not_a_panic() {
        for r in [
            dest_rect(0, 600, 320, H, Aspect::Tv),
            dest_rect(800, 0, 320, H, Aspect::Tv),
            dest_rect(800, 600, 0, H, Aspect::Tv),
            dest_rect(800, 600, 320, 0, Aspect::Integer),
        ] {
            assert_eq!(r.w, 0);
            assert_eq!(r.h, 0);
        }
        let mut dst = Vec::new();
        let mut xmap = Vec::new();
        let empty = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        scale_into(
            &mut dst,
            8,
            8,
            Frame {
                px: &[1u32; 4],
                w: 2,
                h: 2,
            },
            empty,
            0,
            &mut xmap,
        );
        assert!(
            dst.iter().all(|&p| p == 0),
            "an empty rect draws only background"
        );
        assert_eq!(window_to_native(1.0, 1.0, empty, 320, H), None);
    }

    /// The blit fills exactly the destination rect and leaves the surrounding box at the background colour,
    /// and it magnifies by replication (a 2x2 source at 3x is 3x3 blocks).
    #[test]
    fn the_blit_fills_its_rect_and_only_its_rect() {
        let (ww, wh) = (10usize, 10usize);
        let src = vec![0x11u32, 0x22, 0x33, 0x44]; // 2x2
        let rect = Rect {
            x: 2,
            y: 2,
            w: 6,
            h: 6,
        };
        let mut dst = Vec::new();
        let mut xmap = Vec::new();
        scale_into(
            &mut dst,
            ww,
            wh,
            Frame {
                px: &src,
                w: 2,
                h: 2,
            },
            rect,
            0x99,
            &mut xmap,
        );
        assert_eq!(dst.len(), ww * wh);
        // Corners of the window are background.
        assert_eq!(dst[0], 0x99);
        assert_eq!(dst[ww * wh - 1], 0x99);
        // Inside the rect: each source pixel becomes a 3x3 block.
        for y in 0..3 {
            for x in 0..3 {
                assert_eq!(dst[(2 + y) * ww + 2 + x], 0x11, "top-left block");
                assert_eq!(dst[(2 + y) * ww + 5 + x], 0x22, "top-right block");
                assert_eq!(dst[(5 + y) * ww + 2 + x], 0x33, "bottom-left block");
                assert_eq!(dst[(5 + y) * ww + 5 + x], 0x44, "bottom-right block");
            }
        }
    }

    /// A rect that runs past the window edge is clipped, not wrapped — no row of the picture may reappear on
    /// the far side of the buffer.
    #[test]
    fn a_rect_past_the_window_edge_is_clipped() {
        let (ww, wh) = (8usize, 8usize);
        let src = vec![0xAAu32; 16]; // 4x4
        let rect = Rect {
            x: 6,
            y: 6,
            w: 8,
            h: 8,
        };
        let mut dst = Vec::new();
        let mut xmap = Vec::new();
        scale_into(
            &mut dst,
            ww,
            wh,
            Frame {
                px: &src,
                w: 4,
                h: 4,
            },
            rect,
            0,
            &mut xmap,
        );
        assert_eq!(dst.len(), ww * wh);
        // Only the 2x2 corner is inked.
        for y in 0..wh {
            for x in 0..ww {
                let want = if x >= 6 && y >= 6 { 0xAA } else { 0 };
                assert_eq!(dst[y * ww + x], want, "({x},{y})");
            }
        }
    }

    /// The click inverse round-trips the blit: for every destination pixel of the rect, the inverse names the
    /// source pixel the blit actually copied there. This is the property that keeps click-to-watch honest at
    /// any window size, which is what the old fixed-`--scale` assumption could not do.
    #[test]
    fn the_click_inverse_matches_the_blit_pixel_for_pixel() {
        for (ww, wh, sw, aspect) in [
            (960usize, 672usize, 320usize, Aspect::Tv),
            (1000, 700, 256, Aspect::Tv),
            (960, 672, 320, Aspect::Square),
            (1100, 780, 256, Aspect::Integer),
            (533, 400, 320, Aspect::Tv), // an awkward, non-integer size
        ] {
            let rect = dest_rect(ww, wh, sw, H, aspect);
            // A source frame whose every pixel encodes its own coordinates.
            let src: Vec<u32> = (0..sw * H)
                .map(|i| ((i / sw) << 16 | (i % sw)) as u32)
                .collect();
            let mut dst = Vec::new();
            let mut xmap = Vec::new();
            scale_into(
                &mut dst,
                ww,
                wh,
                Frame {
                    px: &src,
                    w: sw,
                    h: H,
                },
                rect,
                0xDEAD,
                &mut xmap,
            );
            for dy in (0..wh).step_by(7) {
                for dx in (0..ww).step_by(11) {
                    let got = window_to_native(dx as f32, dy as f32, rect, sw, H);
                    let painted = dst[dy * ww + dx];
                    match got {
                        Some((sx, sy)) => assert_eq!(
                            painted,
                            (u32::from(sy) << 16) | u32::from(sx),
                            "({dx},{dy}) in {aspect:?} {ww}x{wh}: the inverse must name the pixel the blit copied"
                        ),
                        None => assert_eq!(
                            painted, 0xDEAD,
                            "({dx},{dy}) in {aspect:?}: the inverse said 'letterbox', so nothing was painted"
                        ),
                    }
                }
            }
        }
    }

    /// Clicks in the letterbox and outside the window are rejected; the picture's own corners are accepted.
    #[test]
    fn clicks_outside_the_picture_are_rejected() {
        let rect = dest_rect(1200, 600, 320, H, Aspect::Tv); // pillarboxed
        assert!(rect.x > 0);
        assert_eq!(window_to_native(0.0, 300.0, rect, 320, H), None, "left bar");
        assert_eq!(
            window_to_native(1199.0, 300.0, rect, 320, H),
            None,
            "right bar"
        );
        assert_eq!(window_to_native(-1.0, 10.0, rect, 320, H), None, "negative");
        assert_eq!(
            window_to_native(rect.x as f32, rect.y as f32, rect, 320, H),
            Some((0, 0)),
            "the top-left of the picture is native (0,0)"
        );
        let last = window_to_native(
            (rect.x + rect.w - 1) as f32,
            (rect.y + rect.h - 1) as f32,
            rect,
            320,
            H,
        );
        assert_eq!(
            last,
            Some((319, 223)),
            "the bottom-right is the last native dot"
        );
    }

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
                    let g = Rect {
                        x: gx,
                        y: gy,
                        w: 1,
                        h: 1,
                    };
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
        assert_eq!(
            native_rect_to_window(
                Rect {
                    x: 320,
                    y: 0,
                    w: 1,
                    h: 1
                },
                rect,
                320,
                224
            ),
            None
        );
        assert_eq!(
            native_rect_to_window(
                Rect {
                    x: 0,
                    y: 224,
                    w: 1,
                    h: 1
                },
                rect,
                320,
                224
            ),
            None
        );
        assert_eq!(
            native_rect_to_window(
                Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 1
                },
                rect,
                320,
                224
            ),
            None
        );
        let degenerate = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        assert_eq!(
            native_rect_to_window(
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1
                },
                degenerate,
                320,
                224
            ),
            None
        );
    }

    /// Below 1:1 the map cannot be an inverse — several game pixels share one window pixel —
    /// but it must still answer a non-empty rect inside the picture rather than a zero-width
    /// one that would draw nothing.
    ///
    /// This is the regime `the_forward_map_is_the_inverse_of_the_click_map` cannot reach: every
    /// geometry it sweeps upscales, so both clamps in the map are identity there and could be
    /// deleted with the whole suite green. A 200x150 window is not hypothetical — it is what a
    /// window manager hands out while somebody drags a corner in.
    ///
    /// The containment half is the load-bearing one, and it caught a real bug the first time it
    /// ran: with only a `w >= 1` floor, game column 319 mapped to window column 200 of a 200-wide
    /// picture, one pixel out into the letterbox.
    #[test]
    fn the_forward_map_still_places_a_rect_when_the_picture_is_downscaled() {
        let rect = dest_rect(200, 150, 320, 224, Aspect::Tv);
        assert!(rect.w < 320, "this geometry really does downscale");
        assert!(
            rect.h < 224,
            "on both axes, so the height floor is measured too"
        );
        for gx in [0usize, 1, 159, 319] {
            let out = native_rect_to_window(
                Rect {
                    x: gx,
                    y: 100,
                    w: 1,
                    h: 1,
                },
                rect,
                320,
                224,
            )
            .expect("a visible game pixel always places");
            assert!(out.w >= 1 && out.h >= 1, "never a zero-area rect at {gx}");
            assert!(
                out.x >= rect.x && out.x + out.w <= rect.x + rect.w,
                "inside the picture horizontally at {gx}: {out:?} vs {rect:?}"
            );
            assert!(
                out.y >= rect.y && out.y + out.h <= rect.y + rect.h,
                "inside the picture vertically at {gx}: {out:?} vs {rect:?}"
            );
        }
    }

    /// A rect running past the right/bottom edge is clipped to the picture, not dropped: a sprite
    /// half off-screen must still show the half that is on-screen.
    #[test]
    fn the_forward_map_clips_a_rect_that_overruns_the_picture() {
        let rect = dest_rect(640, 480, 320, 224, Aspect::Tv);
        let out = native_rect_to_window(
            Rect {
                x: 300,
                y: 210,
                w: 32,
                h: 32,
            },
            rect,
            320,
            224,
        )
        .expect("a partly-visible rect still maps");
        assert!(
            out.x + out.w <= rect.x + rect.w,
            "clipped to the picture's right edge"
        );
        assert!(
            out.y + out.h <= rect.y + rect.h,
            "clipped to the picture's bottom edge"
        );
    }

    /// `--scale N` means the same vertical size in every aspect mode; only the width follows the aspect.
    #[test]
    fn the_initial_window_size_scales_the_height_and_lets_aspect_pick_the_width() {
        assert_eq!(initial_window_size(3, 320, H, Aspect::Square), (960, 672));
        assert_eq!(initial_window_size(3, 320, H, Aspect::Integer), (960, 672));
        let (w, h) = initial_window_size(3, 320, H, Aspect::Tv);
        assert_eq!(h, 672);
        assert_eq!(w, 896, "672 * 4/3");
        assert_eq!(w * 3, h * 4, "the opened window is exactly 4:3");
        assert_eq!(
            initial_window_size(0, 320, H, Aspect::Tv).1,
            H,
            "scale 0 clamps to 1x"
        );
    }

    /// The `--aspect` spellings map as documented, and an unknown one is refused rather than silently
    /// defaulting (a typo must not present as "the flag does nothing").
    #[test]
    fn aspect_names_parse_and_unknown_ones_are_refused() {
        assert_eq!(Aspect::from_name("tv"), Some(Aspect::Tv));
        assert_eq!(Aspect::from_name("4:3"), Some(Aspect::Tv));
        assert_eq!(Aspect::from_name("SQUARE"), Some(Aspect::Square));
        assert_eq!(Aspect::from_name("integer"), Some(Aspect::Integer));
        assert_eq!(Aspect::from_name("widescreen"), None);
        assert_eq!(Aspect::default(), Aspect::Tv);
        assert_eq!(Aspect::Tv.name(), "4:3");
    }
}
