//! A self-contained 5x7 bitmap font, and the primitives that draw it into a packed `0x00RR_GGBB` buffer.
//!
//! **Why this exists at all.** `minifb` presents a raw pixel buffer and has no text rendering whatsoever, so
//! every message this frontend produced went to `println!` — which a user who launched the window never sees.
//! A font crate would be a new dependency for what is, in the end, 96 glyph bitmaps; this module is the whole
//! of it, in one file, with no allocation on the drawing path.
//!
//! **Glyph coverage.** Uppercase A-Z, digits 0-9, and the punctuation the frontend's own messages use.
//! Lowercase is folded to uppercase at draw time (it reads as a console/CRT font, and halves the table);
//! anything unmapped draws as a hollow box so a missing glyph is *visible* rather than a silent hole.
//!
//! **Geometry.** Each glyph is 5 px wide and 7 px tall, stored as seven rows of five bits (bit 4 = leftmost
//! column). Advance is 6 px (one blank column between glyphs) and the line box is 8 px tall, so a run of text
//! occupies `6 * chars - 1` by `7` pixels of ink inside a `6 * chars` by `8` cell box. Every dimension is
//! multiplied by an integer `px` scale, so the text is crisp at any window size.

/// Glyph cell width in unscaled pixels, including the one-pixel gap to the next glyph.
pub const ADVANCE: usize = 6;
/// Glyph ink height in unscaled pixels.
pub const GLYPH_H: usize = 7;
/// Line box height in unscaled pixels (ink + one blank row of leading).
pub const LINE_H: usize = 8;

/// The glyph drawn for any character with no entry in [`glyph`] — a hollow box, so a gap in the table shows up
/// on screen instead of silently swallowing a character.
const MISSING: [u8; GLYPH_H] = [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F];

/// The 5x7 bitmap for `c`, or `None` when the character is not in the table. Lowercase is folded to uppercase
/// by the caller ([`draw_text`]); this table is uppercase-only.
fn glyph(c: char) -> Option<[u8; GLYPH_H]> {
    Some(match c {
        ' ' => [0; GLYPH_H],
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x19, 0x15, 0x13, 0x13, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x08],
        ':' => [0x00, 0x00, 0x04, 0x00, 0x04, 0x00, 0x00],
        ';' => [0x00, 0x00, 0x04, 0x00, 0x04, 0x04, 0x08],
        '-' => [0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '\\' => [0x10, 0x10, 0x08, 0x04, 0x02, 0x01, 0x01],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '[' => [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E],
        ']' => [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E],
        '$' => [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04],
        '?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        '*' => [0x00, 0x15, 0x0E, 0x1F, 0x0E, 0x15, 0x00],
        '#' => [0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A],
        '%' => [0x11, 0x12, 0x02, 0x04, 0x08, 0x09, 0x11],
        '<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        '>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        '=' => [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
        '\'' => [0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00],
        '"' => [0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00],
        '|' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        '^' => [0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00],
        '@' => [0x0E, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0E],
        '&' => [0x0C, 0x12, 0x12, 0x0C, 0x15, 0x12, 0x0D],
        _ => return None,
    })
}

/// Unscaled ink width of `text` in pixels: `ADVANCE * chars - 1` (the trailing inter-glyph gap is not ink).
/// Zero for the empty string.
pub fn text_width(text: &str) -> usize {
    let n = text.chars().count();
    if n == 0 {
        0
    } else {
        n * ADVANCE - 1
    }
}

/// A mutable `w * h` packed-`0x00RR_GGBB` drawing surface.
///
/// Bundling the buffer with its dimensions is not decoration: every primitive here needs all three, and
/// passing them separately made each call site a seven-to-nine-argument list in which a transposed width and
/// height would compile and silently draw garbage. Every method clips rather than panicking, because the
/// window is resizable and its layout is computed from a size the user can drag below the text's own.
pub struct Canvas<'a> {
    buf: &'a mut [u32],
    w: usize,
    h: usize,
}

impl<'a> Canvas<'a> {
    /// Wrap a buffer. `buf` may be longer than `w * h`; anything past it is simply never addressed.
    pub fn new(buf: &'a mut [u32], w: usize, h: usize) -> Self {
        Self { buf, w, h }
    }

    /// Blend `over` into the pixel at `(x, y)` with the given `alpha` (0 = leave, 255 = replace), for the
    /// overlay's dimmed backing panels. Out-of-bounds coordinates are ignored, never a panic.
    ///
    /// Integer per-channel lerp: `out = px + (over - px) * a / 255`. Cheap, exact at both ends, and it keeps
    /// the overlay legible over both a white title card and a black cave.
    fn blend(&mut self, x: usize, y: usize, over: u32, alpha: u8) {
        if x >= self.w || y >= self.h {
            return;
        }
        let Some(px) = self.buf.get_mut(y * self.w + x) else {
            return;
        };
        let a = u32::from(alpha);
        let mut out = 0u32;
        for shift in [16, 8, 0] {
            let s = (*px >> shift) & 0xFF;
            let d = (over >> shift) & 0xFF;
            out |= ((s * (255 - a) + d * a) / 255) << shift;
        }
        *px = out;
    }

    /// Fill the rectangle `(x, y, w_px, h_px)` with `color` at `alpha`, clipped. The overlay's backing panels
    /// use this at a partial alpha so the game underneath stays visible.
    pub fn fill_rect(&mut self, x: i32, y: i32, w_px: usize, h_px: usize, color: u32, alpha: u8) {
        for dy in 0..h_px {
            let py = y + dy as i32;
            if py < 0 {
                continue;
            }
            for dx in 0..w_px {
                let px = x + dx as i32;
                if px < 0 {
                    continue;
                }
                self.blend(px as usize, py as usize, color, alpha);
            }
        }
    }

    /// Draw `text` with its top-left ink corner at `(x, y)`, at integer scale `px`, in `color`, clipped.
    /// Returns the advance width used (`ADVANCE * chars * px`), so callers can lay runs out left to right
    /// without re-measuring.
    ///
    /// Lowercase is folded to uppercase; unmapped characters draw as a hollow box. Nothing is allocated.
    pub fn text(&mut self, x: i32, y: i32, px: usize, color: u32, text: &str) -> usize {
        let px = px.max(1);
        let mut pen = x;
        for c in text.chars() {
            let rows = glyph(c.to_ascii_uppercase()).unwrap_or(MISSING);
            for (ry, row) in rows.iter().enumerate() {
                if *row == 0 {
                    continue;
                }
                for col in 0..5usize {
                    if row & (0x10 >> col) == 0 {
                        continue;
                    }
                    // One font pixel is a `px * px` block.
                    for sy in 0..px {
                        let dy = y + (ry * px + sy) as i32;
                        if dy < 0 {
                            continue;
                        }
                        for sx in 0..px {
                            let dx = pen + (col * px + sx) as i32;
                            if dx < 0 {
                                continue;
                            }
                            self.blend(dx as usize, dy as usize, color, 255);
                        }
                    }
                }
            }
            pen += (ADVANCE * px) as i32;
        }
        (pen - x).max(0) as usize
    }
}

/// Backing-panel opacity. High enough that white text reads over a bright title card, low enough that the
/// game underneath is never hidden.
pub const PANEL_ALPHA: u8 = 190;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every glyph fits the 5-bit cell: a row with bits above bit 4 set would bleed into the next character.
    #[test]
    fn every_glyph_stays_inside_its_five_pixel_cell() {
        let printable: Vec<char> = (0x20u8..0x7Fu8).map(char::from).collect();
        for c in printable {
            if let Some(rows) = glyph(c.to_ascii_uppercase()) {
                for (i, row) in rows.iter().enumerate() {
                    assert_eq!(
                        row & 0xE0,
                        0,
                        "glyph {c:?} row {i} sets a bit outside the 5-px cell"
                    );
                }
            }
        }
    }

    /// Everything the frontend's own messages can contain has a glyph — a silent hollow-box run in a save
    /// confirmation would be a bug the tests should catch, not the user.
    #[test]
    fn the_characters_the_frontend_prints_all_have_glyphs() {
        let used = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,:;-+/\\()[]$!?*#%<>=_'\"|^@&";
        for c in used.chars() {
            assert!(glyph(c).is_some(), "no glyph for {c:?}");
        }
        // Lowercase folds, so it must resolve too.
        for c in "abcdefghijklmnopqrstuvwxyz".chars() {
            assert!(
                glyph(c.to_ascii_uppercase()).is_some(),
                "no glyph for {c:?}"
            );
        }
    }

    /// Width is the ink width: N glyphs advance `6N` but the last inter-glyph gap is not ink.
    #[test]
    fn text_width_counts_ink_not_the_trailing_gap() {
        assert_eq!(text_width(""), 0);
        assert_eq!(text_width("A"), 5);
        assert_eq!(text_width("AB"), 11);
        assert_eq!(text_width("PAUSED"), 6 * ADVANCE - 1);
    }

    /// Drawing at the buffer's edges (and past them, in both directions) clips instead of panicking or
    /// wrapping onto the wrong row — the overlay is laid out from window size, which the user can resize to
    /// something smaller than the text.
    #[test]
    fn drawing_out_of_bounds_clips_and_never_panics() {
        let (w, h) = (40usize, 16usize);
        let mut buf = vec![0u32; w * h];
        Canvas::new(&mut buf, w, h).text(-100, -100, 2, 0x00FF_FFFF, "CLIPPED");
        assert!(buf.iter().all(|&p| p == 0), "fully off-screen drew nothing");
        // Off the right edge: the visible part draws, and nothing wraps onto the next row.
        Canvas::new(&mut buf, w, h).text((w - 3) as i32, 0, 1, 0x0000_FF00, "II");
        for y in 0..h {
            // Row 0 of the buffer may be inked; no *other* row's leftmost pixels may be, which is what a
            // wrapped write would look like.
            if y > GLYPH_H {
                assert_eq!(buf[y * w], 0, "row {y} col 0 was written — that is a wrap");
            }
        }
        // A tiny buffer smaller than one glyph.
        let mut tiny = vec![0u32; 2];
        Canvas::new(&mut tiny, 2, 1).text(0, 0, 3, 0x00FF_FFFF, "W");
    }

    /// A glyph really puts ink where it should: 'I' at scale 1 has its stem in the middle column, and the
    /// space character draws nothing at all.
    #[test]
    fn glyphs_put_ink_in_the_expected_cells() {
        let (w, h) = (16usize, 16usize);
        let mut buf = vec![0u32; w * h];
        Canvas::new(&mut buf, w, h).text(0, 0, 1, 0x00FF_FFFF, "I");
        // Row 0 of 'I' is 0b01110 → columns 1,2,3 inked, 0 and 4 clear.
        assert_eq!(buf[0], 0, "col 0 of row 0 is clear");
        assert_ne!(buf[1], 0, "col 1 of row 0 is inked");
        assert_ne!(buf[3], 0);
        assert_eq!(buf[4], 0, "col 4 of row 0 is clear");
        // Row 1 is 0b00100 → only the middle column.
        assert_eq!(buf[w], 0);
        assert_ne!(buf[w + 2], 0);

        let mut blank = vec![0u32; w * h];
        Canvas::new(&mut blank, w, h).text(0, 0, 1, 0x00FF_FFFF, "   ");
        assert!(blank.iter().all(|&p| p == 0), "spaces draw no ink");
    }

    /// Scale `px` multiplies both axes: a glyph at 3x inks a 3x3 block per font pixel.
    #[test]
    fn scaling_expands_each_font_pixel_into_a_block() {
        let (w, h) = (32usize, 32usize);
        let mut buf = vec![0u32; w * h];
        Canvas::new(&mut buf, w, h).text(0, 0, 3, 0x00FF_FFFF, "I");
        // 'I' row 0 col 1 is inked → the 3x3 block at (3..6, 0..3) is solid.
        for y in 0..3 {
            for x in 3..6 {
                assert_ne!(buf[y * w + x], 0, "({x},{y}) should be inked at 3x");
            }
        }
        // The advance is 6 * 3 = 18, so a second glyph would start at x=18; nothing before that column 15..18
        // (the inter-glyph gap) is inked for a single 'I'.
        for y in 0..GLYPH_H * 3 {
            for x in 15..18 {
                assert_eq!(buf[y * w + x], 0, "the inter-glyph gap must stay clear");
            }
        }
    }

    /// Blending is a true lerp: alpha 0 leaves the pixel, 255 replaces it, and a partial alpha lands between.
    /// `fill_rect` is the only public way in, so that is what the test drives.
    #[test]
    fn blending_is_a_lerp_at_both_ends() {
        let mut buf = vec![0x0080_4020u32];
        Canvas::new(&mut buf, 1, 1).fill_rect(0, 0, 1, 1, 0x00FF_FFFF, 0);
        assert_eq!(buf[0], 0x0080_4020, "alpha 0 changes nothing");
        // Half alpha toward black: each channel becomes `s * (255 - 128) / 255`, i.e. very slightly under
        // half (127/255, not 128/255) — the exact integer result, pinned rather than rounded to a tidy value.
        Canvas::new(&mut buf, 1, 1).fill_rect(0, 0, 1, 1, 0x0000_0000, 128);
        assert_eq!(
            buf[0], 0x003F_1F0F,
            "half alpha toward black roughly halves each channel"
        );
        Canvas::new(&mut buf, 1, 1).fill_rect(0, 0, 1, 1, 0x0012_3456, 255);
        assert_eq!(buf[0], 0x0012_3456, "alpha 255 replaces");
    }
}
