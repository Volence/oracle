//! **The Planes tab — plane A, plane B and the window drawn WHOLE, from the nametable and the tiles,
//! with no scroll applied.**
//!
//! # Why this panel exists, in one sentence
//!
//! An evening of this lane was spent answering *"what does that floor actually look like"* by hand:
//! decoding 4bpp tiles in Python, walking a nametable word by word, and reconstructing a plane with the
//! scroll left off. Every one of those steps is what this panel does on screen in one look. It is the
//! instrument whose absence cost the time, and the legacy C++ port had it.
//!
//! # ⚑ The bases are the RENDERER's, not this panel's
//!
//! Nothing here decodes `(reg $02 & 0x38) << 10`. [`Vdp::plane_base`], [`Vdp::plane_grid`],
//! [`Vdp::plane_decoded`], [`Vdp::plane_scroll_report`] and [`Vdp::window_span_at`] are the renderer's own
//! expressions, exported for this panel, and this module calls them. That is deliberate and it is the
//! whole point: **a debug view that decodes the base itself can disagree with the picture it claims to
//! explain, and it is the debug view nobody checks.** The evening was lost to a guessed base; a second
//! spelling of the base decode in this file would be the same defect with a nicer font.
//!
//! The register file is *shown* beside the picture, read from `Vdp::regs()` — the identical array
//! `emulator/read_vdp_registers` serves as `raw[]` — so the operator can see the bases the renderer used
//! rather than take them on trust. The panel reads the machine in-process, on the precedent
//! [`crate::screen_pick`] set with `pick::resolve(sys.vdp(), ..)` and [`crate::memory`] set with
//! `sys.vdp().cram()`. A JSON round trip of 64 KB of VRAM per repaint would be the alternative, and it
//! would be strictly worse and strictly slower for identical bytes.
//!
//! # ⚑ THE TRAP: a register read is a peek, and this panel says so
//!
//! Reg `$0B` says what the scroll mode is **now**. A game may switch it part way down the frame off a
//! horizontal interrupt, and a single read cannot see that it did. This was measured on a commercial ROM
//! this week: `$0B` read "one value for the whole screen" while the floor was demonstrably corrected per
//! line, because the mode is switched at line 174 by an H-interrupt.
//!
//! So [`ScrollNote::unestablished`] is a **stated line on the panel** whenever reg `$00` bit 4 says an
//! H-interrupt is armed. The picture is still drawn, because refusing to draw would be worse; what is
//! refused is the *claim* that one register read established the scroll the frame was drawn with. Loud on
//! unmeasurable is this repo's standing rule, and this is the exact defect the panel exists to prevent.
//!
//! # Rendered on change, never on the frame clock
//!
//! A plane is up to 128 by 128 cells, and even the ordinary 64 by 32 is 512 by 256 pixels. Rasterising
//! that per repaint and re-uploading it would spend real budget against a toolkit measured at 0.22 ms of a
//! 16.67 ms frame. So [`Panel::refresh`] gathers the inputs, [fingerprints](Inputs::fingerprint) them, and
//! rasterises **only when the fingerprint moves**. The fingerprint covers exactly what the picture is a
//! function of and nothing else:
//!
//! * the decoded nametable cells (so a map edit redraws),
//! * the bytes of **the tiles those cells reference** (so an art edit redraws, and an edit to a tile the
//!   plane does not use does not),
//! * CRAM (so a palette change redraws),
//! * the per-line scroll and window spans (so the viewport outline follows),
//! * the plane selected, the toggles, the grid size, the base, and the ink.
//!
//! Everything is mixed eight bytes at a time rather than one, which is what keeps the skip cheaper than
//! the work it skips: the raster it avoids is 131072 pixel decodes plus a 512 KB texture upload, and the
//! fingerprint over a fully-referenced plane is 8 K mixes. `egui_dock` draws only the active tab of a
//! leaf, so a hidden Planes tab costs exactly zero: its body is not called at all.
//!
//! The panel **shows its own redraw count** beside the picture. A claim that something rasterises rarely
//! is worth nothing if a person cannot see it not happening.
//!
//! # ⚑ CLICK TO IDENTIFY A CELL, AND THE QUESTION IT ANSWERS IS NOT THE SCREEN TAB'S
//!
//! [`identify`] is the second slice. It reads like the Screen tab's click and it is **a different
//! question**, which is the one thing about it a person must not have to discover:
//!
//! * Clicking the **game picture** asks *which layer won at this dot*. That is attribution, through
//!   scroll, priority and the sprite order, and [`crate::screen_pick`] answers it.
//! * Clicking **this** picture asks *which cell of this plane's nametable is this, and what word is in
//!   it*. It is a map read. This plane, this map, whether or not anything on screen is showing it.
//!
//! They legitimately disagree, and a panel implying they are the same answer is wrong **even when both
//! numbers are right**: the cell you click here can be off screen entirely, or covered by a sprite, or
//! ranked under the other plane. So [`CellReading`] carries a sentence of its own about the screen, and
//! it is derived from [`covered_mask`] rather than asserted: the same mask the viewport outline is drawn
//! from, so the words and the outline in front of you cannot disagree.
//!
//! What the two must **never** disagree about is the nametable word itself, and that is asserted rather
//! than hoped for. `the_viewer_and_pixel_attribution_agree_about_the_word` takes screen dots plane A won,
//! asks `Vdp::pixel_attribution` for its `cell`, and asks this module for the cell under the same dot; a
//! difference is an indexing bug in one of two surfaces that both claim to read one map. Both bottom out
//! in the core's `nametable_cell`, so the row is measuring the **addressing on top of it**, which is
//! where a transpose bug lives.
//!
//! ## ⚑ The tile index is VRAM-absolute, and it says so in the same breath
//!
//! `OVERSEER.md`'s GUI-LAYERS entry, point 2: *an index whose space is unstated is a transpose bug
//! waiting to happen*, and the hazard on the other side of that join is worse. An editor rebases a tile
//! index into a blob-local slot with a base constant it owns; the rebase can land **outside the blob and
//! not be rescued by capacity** (*in-capacity is not in-blob*). So this panel names
//! [`oracle_frontend::pick::TILE_SPACE`] beside every index, uses the identical constant the Screen tab
//! does rather than a second string that agrees today, and **never guesses a slot in anybody else's
//! space.**
//!
//! ## What is deliberately NOT here
//!
//! **A tool for "the cell at column c, row r of plane P".** Considered and declined, as a decision rather
//! than an omission. `emulator/pixel_attribution` already returns the decoded word for a dot on screen
//! (the GUI-LAYERS entry files that consumer ask as *already satisfied, do not build it*), and the map is
//! `Vdp::plane_decoded`, the renderer's own. A third spelling would be the thing this module's header
//! exists to refuse. The residual gap is real and named: a socket client wanting an **off-screen** cell
//! must read the base out of `emulator/read_vdp_registers`, address the entry itself and decode the word
//! itself. That is `F-PLANE-CELL-TOOL`, and it is a whole parcel about a wire method rather than a line
//! in this one.

use egui::Color32;
use oracle_core::render::{Cell, Plane, PlaneScroll, VScroll, WindowSpan};
use oracle_core::state_hash::VRAM_SIZE;
use oracle_core::vdp::Vdp;
use oracle_frontend::pick::{tile_range, TILE_SPACE};

/// The three planes in the order the selector offers them, with the word each is called on the button.
///
/// Lowercase because these are labels in running prose on a control, not proper nouns, and the tab bar
/// already carries the capitalised title.
pub const CHOICES: [(Plane, &str); 3] = [
    (Plane::A, "plane A"),
    (Plane::B, "plane B"),
    (Plane::Window, "window"),
];

/// **The three colours the raster paints that are not a game's own.**
///
/// Passed in rather than read from [`crate::theme`] inside the raster for two reasons: the raster is a
/// pure function under test and must not need a theme, and the ink is part of
/// [`Inputs::fingerprint`], so a family change has to be able to move it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ink {
    /// Half the transparent checker.
    pub empty_a: Color32,
    /// The other half.
    pub empty_b: Color32,
    /// The viewport outline.
    pub outline: Color32,
}

impl Ink {
    /// The panel's ink from the theme family in use. The outline is the accent, which
    /// [`crate::theme::ACCENT`] holds fixed across families on purpose.
    pub fn of(f: crate::theme::Family) -> Self {
        Ink {
            empty_a: f.void,
            empty_b: f.surface,
            outline: crate::theme::ACCENT,
        }
    }
}

/// The side of the transparent checker, in plane pixels.
///
/// **4, not 8.** At 8 it lands exactly on the cell grid and a fully transparent cell reads as a solid
/// square of colour, which is the one thing the checker exists to not look like.
const CHECKER: usize = 4;

// ---------------------------------------------------------------------------------------------------
// What the picture is a function of
// ---------------------------------------------------------------------------------------------------

/// **Everything gathered from the VDP for one repaint**, plus the fingerprint over it.
///
/// Gathered whether or not a raster follows: the fingerprint cannot be computed without the inputs, and
/// the facts beside the picture (base, grid, scroll mode) are drawn every repaint regardless.
pub struct Inputs {
    /// Which plane this describes.
    pub plane: Plane,
    /// Whether the raster is the plane in its own space, or the plane sampled through the live scroll.
    /// Always `false` for the window, which does not scroll.
    pub scrolled: bool,
    /// The nametable's VRAM byte address, from [`Vdp::plane_base`].
    pub base: usize,
    /// The grid in cells, from [`Vdp::plane_grid`].
    pub cols: u16,
    /// Rows in cells.
    pub rows: u16,
    /// The active display in pixels, from `Vdp::active_display`.
    pub display: (u16, u16),
    /// The decoded map, row major, `rows * cols` long.
    pub cells: Vec<Cell>,
    /// The effective scroll of this plane on each display line. Empty for the window.
    pub scroll: Vec<PlaneScroll>,
    /// The window's covered span on each display line. Empty for planes A and B.
    pub spans: Vec<Option<WindowSpan>>,
    /// Reg `$0B` bits 1 and 0: the horizontal scroll mode.
    pub hmode: u8,
    /// Reg `$0B` bit 2: per-16-pixel-column vertical scroll.
    pub vcolumns: bool,
    /// The horizontal scroll table's VRAM byte address, from reg `$0D`.
    pub htable: usize,
    /// The line reg `$0A` reloads the H-interrupt counter with, when reg `$00` bit 4 arms it.
    pub hint_line: Option<u8>,
    /// The mix over every field above plus the referenced tiles, CRAM and the ink.
    pub fingerprint: u64,
}

/// A 2048-bit set of tile indices, one bit per pattern the VDP can address.
type TileSet = [u64; 32];

/// Gather one repaint's inputs and fingerprint them.
///
/// `want_scroll` is the toggle; the window ignores it, because the window plane has no scroll to apply
/// and pretending otherwise would draw a confident nothing.
pub fn gather(vdp: &Vdp, plane: Plane, want_scroll: bool, ink: Ink) -> Inputs {
    let base = vdp.plane_base(plane);
    let (cols, rows) = vdp.plane_grid(plane);
    let display = vdp.active_display();
    let cells = vdp.plane_decoded(plane, None);
    let regs = vdp.regs();

    let window = plane == Plane::Window;
    let scroll: Vec<PlaneScroll> = if window {
        Vec::new()
    } else {
        (0..display.1)
            .map(|l| vdp.plane_scroll_report(plane, l))
            .collect()
    };
    let spans: Vec<Option<WindowSpan>> = if window {
        (0..display.1).map(|l| vdp.window_span_at(l)).collect()
    } else {
        Vec::new()
    };

    let mut h = Fnv::new();
    h.mix(match plane {
        Plane::A => 1,
        Plane::B => 2,
        Plane::Window => 3,
    });
    h.mix(u64::from(want_scroll && !window));
    h.mix(base as u64);
    h.mix((cols as u64) << 16 | rows as u64);
    h.mix((display.0 as u64) << 16 | display.1 as u64);
    h.mix(u64::from(regs[0x0B]));
    h.mix(u64::from(regs[0x0D]));
    h.mix(u64::from(regs[0x00]));
    h.mix(u64::from(regs[0x0A]));
    for c in [ink.empty_a, ink.empty_b, ink.outline] {
        h.mix(u64::from(u32::from_le_bytes(c.to_array())));
    }

    // The map, and the set of tiles it reaches, in one pass.
    let mut used: TileSet = [0; 32];
    for c in &cells {
        h.mix(u64::from(encode_cell(c)));
        used[(c.tile >> 6) as usize] |= 1u64 << (c.tile & 63);
    }
    // Only the tiles the map reaches. An edit to a pattern this plane does not draw does not move the
    // picture, so it must not move the fingerprint, and this is also what keeps the mix far below the
    // 64 KB of VRAM the plane could in principle span.
    for (word, chunk) in used.iter().enumerate() {
        let mut bits = *chunk;
        while bits != 0 {
            let bit = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            let at = (word * 64 + bit) * 32;
            h.bytes(&vdp.vram()[at..at + 32]);
        }
    }
    h.bytes(vdp.cram());
    for s in &scroll {
        h.mix(u64::from(s.hscroll));
        match &s.vscroll {
            VScroll::Full(v) => h.mix(u64::from(*v) << 1),
            VScroll::TwoCell(v) => {
                for x in v {
                    h.mix(u64::from(*x) << 1 | 1);
                }
            }
        }
    }
    for s in &spans {
        h.mix(match s {
            None => 0,
            Some(w) => 1 | (u64::from(w.start_x) << 8) | (u64::from(w.end_x) << 24),
        });
    }

    Inputs {
        plane,
        scrolled: want_scroll && !window,
        base,
        cols,
        rows,
        display,
        cells,
        scroll,
        spans,
        hmode: regs[0x0B] & 0x03,
        vcolumns: regs[0x0B] & 0x04 != 0,
        htable: ((regs[0x0D] & 0x3F) as usize) << 10,
        hint_line: (regs[0x00] & 0x10 != 0).then_some(regs[0x0A]),
        fingerprint: h.0,
    }
}

/// The 16-bit nametable word a [`Cell`] came from, rebuilt so the fingerprint mixes the entry rather than
/// five separate fields. Bit layout is the hardware's: priority 15, palette 14 to 13, vflip 12, hflip 11,
/// tile 10 to 0.
fn encode_cell(c: &Cell) -> u16 {
    (c.tile & 0x07FF)
        | (u16::from(c.hflip) << 11)
        | (u16::from(c.vflip) << 12)
        | ((c.palette as u16 & 3) << 13)
        | (u16::from(c.priority) << 15)
}

impl Inputs {
    /// The plane's size in pixels.
    pub fn pixels(&self) -> (usize, usize) {
        (self.cols as usize * 8, self.rows as usize * 8)
    }

    /// The raster's size in pixels: the plane, or the display when the scroll is applied.
    pub fn raster_size(&self) -> (usize, usize) {
        if self.scrolled {
            (self.display.0 as usize, self.display.1 as usize)
        } else {
            self.pixels()
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// What a peek can and cannot establish
// ---------------------------------------------------------------------------------------------------

/// The scroll as read, and what reading it once could not settle.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScrollNote {
    /// The horizontal mode in the reader's own words.
    pub horizontal: String,
    /// The vertical mode in the reader's own words.
    pub vertical: String,
    /// ⚑ **Set whenever one read of the registers cannot establish the scroll the frame was drawn with.**
    /// Rendered in the warning colour, and never omitted when set.
    pub unestablished: Option<String>,
}

/// Read the scroll mode into sentences, and say plainly when a single read was not enough.
///
/// The condition for `unestablished` is measured, not guessed: reg `$00` bit 4 arms the horizontal
/// interrupt, which is the mechanism by which a scroll mode changes part way down a frame. Absent that
/// interrupt, the registers can only move between frames and a peek taken now describes the frame now.
pub fn scroll_note(inp: &Inputs) -> ScrollNote {
    if inp.plane == Plane::Window {
        return ScrollNote {
            horizontal: "the window plane does not scroll".into(),
            vertical: "its map sits at screen coordinates".into(),
            unestablished: None,
        };
    }
    ScrollNote {
        horizontal: match inp.hmode {
            0 => "horizontal: one value for the whole screen".into(),
            1 => "horizontal: one value per line, cycling every 8 lines (a mode the chip documents as prohibited)".into(),
            2 => "horizontal: one value per cell row, so every 8 lines".into(),
            _ => "horizontal: one value per line".into(),
        },
        vertical: if inp.vcolumns {
            "vertical: one value per 2 cell columns, so every 16 pixels".into()
        } else {
            "vertical: one value for the whole screen".into()
        },
        unestablished: inp.hint_line.map(|line| {
            format!(
                "A horizontal interrupt is armed at line {line}. The scroll mode is read once, now, so a \
                 mode the game switches part way down the frame cannot be seen from here. This view \
                 applies the mode above to every line, which is the drawn scroll only if the game left \
                 the registers alone."
            )
        }),
    }
}

// ---------------------------------------------------------------------------------------------------
// The raster
// ---------------------------------------------------------------------------------------------------

/// Draw the plane.
///
/// Two shapes, chosen by [`Inputs::scrolled`]:
///
/// * **off** (the primary view, and the reason the panel exists): the whole plane in its own space, with
///   the region the screen is currently showing outlined on it. The outline is derived by marking every
///   plane pixel the display samples and then keeping the marked pixels that touch an unmarked one, which
///   is one code path that degenerates to a rectangle exactly when the scroll is uniform and shows the
///   real shape when it is not. Neighbours wrap with the plane, so a band that runs off the right edge
///   and back on at the left has no false edge down the middle of it.
/// * **on**: the display-sized region the scroll cuts out of that plane, this plane alone, with no other
///   plane and no sprites over it. That is what the toggle buys over the Screen tab.
pub fn raster(vdp: &Vdp, inp: &Inputs, ink: Ink, outline: bool) -> egui::ColorImage {
    let (pw, ph) = inp.pixels();
    let cram = vdp.cram_decoded();
    let vram = vdp.vram();
    let (w, h) = inp.raster_size();
    let mut pixels = Vec::with_capacity(w * h);

    if inp.scrolled {
        // No outline here, and its absence is the fact rather than an omission: this raster IS the
        // viewport, so an outline round it would trace the border of the image.
        for line in 0..h {
            let sc = &inp.scroll[line];
            for x in 0..w {
                let (sx, sy) = sample(sc, x, line, pw, ph);
                pixels.push(dot(inp, vram, &cram, ink, sx, sy));
            }
        }
        return image(w, h, pixels);
    }

    for py in 0..ph {
        for px in 0..pw {
            pixels.push(dot(inp, vram, &cram, ink, px, py));
        }
    }
    if outline {
        for i in covered_edges(inp, pw, ph) {
            pixels[i] = ink.outline;
        }
    }
    image(pw, ph, pixels)
}

fn image(w: usize, h: usize, pixels: Vec<Color32>) -> egui::ColorImage {
    egui::ColorImage {
        size: [w, h],
        source_size: egui::vec2(w as f32, h as f32),
        pixels,
    }
}

/// Which plane pixel the display samples at (`x`, `line`) under `sc`.
fn sample(sc: &PlaneScroll, x: usize, line: usize, pw: usize, ph: usize) -> (usize, usize) {
    let v = match &sc.vscroll {
        VScroll::Full(v) => *v,
        // One value per 16-pixel column, left to right. A column past the end of the list is the last
        // one rather than a panic: the list is sized from the display width the same read gave us, so
        // they cannot disagree, and clamping is the harmless answer if they ever do.
        VScroll::TwoCell(cols) => cols
            .get(x / 16)
            .copied()
            .or_else(|| cols.last().copied())
            .unwrap_or(0),
    };
    let sx = (x as isize - sc.hscroll as isize).rem_euclid(pw as isize) as usize;
    let sy = (line as isize + v as isize).rem_euclid(ph as isize) as usize;
    (sx, sy)
}

/// One plane pixel's colour: the tile's nibble through the cell's palette line, or the checker where the
/// nibble is 0.
///
/// **Nibble 0 is transparent on real hardware and it is drawn as transparent here**, rather than as CRAM
/// entry 0 of the line. Painting it as a colour would make an empty plane look like a filled one, and
/// *where the art is* is the first question this panel is asked.
fn dot(
    inp: &Inputs,
    vram: &[u8],
    cram: &[(u8, u8, u8); 64],
    ink: Ink,
    px: usize,
    py: usize,
) -> Color32 {
    let cell = &inp.cells[(py / 8) * inp.cols as usize + (px / 8)];
    let n = nibble(vram, cell, (px % 8) as u8, (py % 8) as u8);
    if n == 0 {
        if ((px / CHECKER) + (py / CHECKER)).is_multiple_of(2) {
            ink.empty_a
        } else {
            ink.empty_b
        }
    } else {
        let (r, g, b) = cram[(cell.palette as usize & 3) * 16 + n as usize];
        Color32::from_rgb(r, g, b)
    }
}

/// The 4-bit pixel at (`tx`, `ty`) of `cell`'s tile, flips applied.
///
/// 32 bytes per tile, 4 bytes per row, **high nibble first**: the left pixel of a byte pair is the top
/// nibble. Getting that backwards mirrors every tile in the plane by one pixel pair and looks almost
/// right, which is why it has a test of its own.
fn nibble(vram: &[u8], cell: &Cell, tx: u8, ty: u8) -> u8 {
    let tx = if cell.hflip { 7 - tx } else { tx };
    let ty = if cell.vflip { 7 - ty } else { ty };
    let at = (cell.tile as usize * 32 + ty as usize * 4 + (tx as usize >> 1)) & (VRAM_SIZE - 1);
    let byte = vram[at];
    if tx & 1 == 0 {
        byte >> 4
    } else {
        byte & 0x0F
    }
}

/// **Which plane pixels the display is currently showing**, as a `pw` by `ph` mask.
///
/// Held apart from [`covered_edges`] because it is the fact and the outline is a presentation of it: a
/// test that wants to know whether the viewport is sheared has to compare the *region*, and the edge
/// count cannot answer that. A sheared band and a rectangle have **the same perimeter** to within the
/// two rows at top and bottom, which is a real trap and cost this parcel a red run to find.
pub fn covered_mask(inp: &Inputs, pw: usize, ph: usize) -> Vec<bool> {
    let mut covered = vec![false; pw * ph];
    let (dw, dh) = (inp.display.0 as usize, inp.display.1 as usize);
    if inp.plane == Plane::Window {
        // The window map is drawn at screen coordinates, so the region on screen is the band regs $11
        // and $12 switch on, taken from the renderer rather than re-decoded here.
        for (line, span) in inp.spans.iter().enumerate().take(dh.min(ph)) {
            let Some(s) = span else { continue };
            for x in (s.start_x as usize)..(s.end_x as usize).min(pw) {
                covered[line * pw + x] = true;
            }
        }
    } else {
        for line in 0..dh {
            let Some(sc) = inp.scroll.get(line) else {
                break;
            };
            for x in 0..dw {
                let (sx, sy) = sample(sc, x, line, pw, ph);
                covered[sy * pw + sx] = true;
            }
        }
    }
    covered
}

/// **The viewport outline**, as indices into a `pw` by `ph` raster.
///
/// A covered pixel is kept when one of its four neighbours is uncovered. Neighbours wrap modulo the
/// plane, because the plane does, so a band that runs off the right edge and back on at the left has no
/// false edge down the middle of it.
pub fn covered_edges(inp: &Inputs, pw: usize, ph: usize) -> Vec<usize> {
    let covered = covered_mask(inp, pw, ph);
    let mut out = Vec::new();
    for y in 0..ph {
        for x in 0..pw {
            if !covered[y * pw + x] {
                continue;
            }
            let l = covered[y * pw + (x + pw - 1) % pw];
            let r = covered[y * pw + (x + 1) % pw];
            let u = covered[((y + ph - 1) % ph) * pw + x];
            let d = covered[((y + 1) % ph) * pw + x];
            if !(l && r && u && d) {
                out.push(y * pw + x);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// The click
// ---------------------------------------------------------------------------------------------------

/// **What one click on the plane picture turned out to be.**
///
/// Four parts rather than one paragraph, on [`crate::screen_pick::Readout`]'s reasoning and for the same
/// reason: they are four different kinds of statement and they must not arrive at one weight. Every part
/// is composed by whoever knows it and handed over whole; nothing downstream recovers structure by looking
/// at the text, and nothing downstream decides a colour by looking at it either.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CellReading {
    /// **The sentence a person reads**, and the first thing drawn: which cell of which plane, and the
    /// pattern its word names, in words and in the space that index is in.
    ///
    /// Prose first is the editor lane's expensive precedent, adopted across this window: they shipped a
    /// lens that highlighted 1,244 cells, entirely correctly, and the reaction was *"what are the purple
    /// boxes"*. Not *that's wrong* but ***what is that***.
    pub head: String,
    /// **What this cell has to do with the screen**, which is the part this surface is most likely to be
    /// misread about. Derived from [`covered_mask`], the same mask the viewport outline was drawn from, so
    /// this sentence and the picture behind it cannot disagree.
    ///
    /// It also says, once and on the covered case only, that *which layer you actually see there* is a
    /// different question with a different tab. On the uncovered case that clause would be noise, because
    /// there is no dot on the game picture to confuse this answer with.
    pub screen: String,
    /// The addressing behind the sentence: the nametable entry, the word in it, and where the pattern
    /// lives. Monospace, in the spelling a reader would compare against a tool's reply.
    pub detail: String,
    /// **What this reading could not establish**, warning-coloured and never omitted when present.
    ///
    /// A list, because a click can earn more than one and dropping either because the other fired would be
    /// a silent choice made on the reader's behalf. Empty on an ordinary frame, which is what keeps it
    /// worth reading: a caveat on every reply is a caveat nobody reads.
    pub unestablished: Vec<String>,
}

impl CellReading {
    /// The whole reading as one string, **for tests only**.
    ///
    /// The tab draws the parts at four weights and never joins them, so this is not what any reader sees.
    /// It is the seam an assertion works across, because a panel this crate cannot screenshot is only
    /// checkable where it becomes text. Deliberately not offered to the renderer, on
    /// [`crate::screen_pick::Readout`]'s reasoning: a joined string beside a laid-out one is two spellings
    /// of one answer.
    #[cfg(test)]
    pub fn text(&self) -> String {
        let mut s = format!("{}\n{}\n{}", self.head, self.screen, self.detail);
        for c in &self.unestablished {
            s.push('\n');
            s.push_str(c);
        }
        s
    }
}

/// The word this plane is called on its own selector button, so the answer and the control agree.
///
/// Read out of [`CHOICES`] rather than spelled again: a plane renamed on the button and not in the readout
/// is a small lie that survives every test which does not compare the two.
fn plane_label(plane: Plane) -> &'static str {
    CHOICES
        .iter()
        .find(|(p, _)| *p == plane)
        .map(|(_, l)| *l)
        .unwrap_or("this plane")
}

/// **Which plane pixel the raster pixel (`rx`, `ry`) is showing**, or `None` when it is off the raster.
///
/// The whole reason this exists as a function is that the answer is not the identity in both views, and
/// the wrong one is invisible:
///
/// * unscrolled, the raster **is** the plane, so the plane pixel is the raster pixel;
/// * scrolled, the raster is the display and every pixel of it was fetched through [`sample`], so the
///   inverse is [`sample`] itself, **called here rather than re-derived**. The Screen tab learned the same
///   lesson one crate over: `dot_at` inverts the rectangle that was actually drawn instead of computing a
///   second opinion about what should have been drawn, and it survived a change of fit with no edit. A
///   second spelling of `x - hscroll` here would put the readout on a different cell from the pixel under
///   the cursor, at exactly the scroll values a per-line effect produces and never at zero.
pub fn plane_pixel_at(inp: &Inputs, rx: usize, ry: usize) -> Option<(usize, usize)> {
    let (rw, rh) = inp.raster_size();
    if rx >= rw || ry >= rh {
        return None;
    }
    let (pw, ph) = inp.pixels();
    if pw == 0 || ph == 0 {
        return None;
    }
    if !inp.scrolled {
        return Some((rx, ry));
    }
    // Sized from the same read that gave the display height, so this cannot be short; `get` rather than an
    // index because a missing line is worth no answer instead of a panic.
    let sc = inp.scroll.get(ry)?;
    Some(sample(sc, rx, ry, pw, ph))
}

/// **Where a cell's nametable entry is in VRAM**, as a byte address.
///
/// This is the one address in this module that is not handed over by the renderer, and it is the second
/// half of an expression the core owns: [`Vdp::plane_base`] gives the base (never re-decoded here), and
/// this adds the entry offset the core's own `nametable_cell` adds, in the wrap the core takes it in. A
/// second spelling is exactly what the module header refuses, so it is **gated rather than trusted**:
/// `the_entry_address_names_the_word_the_cell_came_from` walks every cell of a plane of distinct words and
/// asserts the two bytes here are the word `plane_decoded` returned. A transposed stride, a dropped `* 2`
/// or the wrong wrap all move that row.
fn entry_addr(inp: &Inputs, col: usize, row: usize) -> usize {
    (inp.base + (row * inp.cols as usize + col) * 2) & (VRAM_SIZE - 1)
}

/// **Identify the cell under a raster pixel.** `None` when the pointer was off the picture.
///
/// Pure over [`Inputs`], which is this repaint's gather and also what the texture on screen was rasterised
/// from, so the answer and the picture are the same frame's facts. That equality is the render-on-change
/// gate doing a second job: [`Panel::refresh`] only leaves a texture standing while its fingerprint is
/// this gather's.
///
/// What it does **not** do is resolve attribution. See this module's header: naming a winning layer here
/// would be answering the Screen tab's question with this tab's data, which is the failure the
/// [`CellReading::screen`] sentence exists to head off.
pub fn identify(inp: &Inputs, rx: usize, ry: usize) -> Option<CellReading> {
    let (px, py) = plane_pixel_at(inp, rx, ry)?;
    let (pw, ph) = inp.pixels();
    let (col, row) = (px / 8, py / 8);
    let cell = inp.cells.get(row * inp.cols as usize + col)?;

    let plane = plane_label(inp.plane);
    let word = encode_cell(cell);
    let (lo, hi) = tile_range(cell.tile);
    let entry = entry_addr(inp, col, row);

    // The head names the subject, the space and the index, in that order and in one sentence. "the map
    // holds" is doing real work: it says this is a read of the nametable and not a claim about the glass.
    let head = format!(
        "{plane}, column {col} row {row}: the map holds {TILE_SPACE} tile ${:03X}.",
        cell.tile
    );

    // Derived from the mask the outline was drawn from, never from a second coverage test.
    let covered = covered_mask(inp, pw, ph);
    let on_screen = covered.get(py * pw + px).copied().unwrap_or(false);
    let screen = if on_screen {
        "The screen was showing this cell when you clicked. Which layer you actually see at those dots \
         is a different question, because a sprite or the other plane can cover it, and the Screen tab \
         is the one that answers it."
            .to_string()
    } else {
        "The screen was not showing this cell when you clicked, so no dot on the game picture \
         corresponds to it."
            .to_string()
    };

    let flips = match (cell.hflip, cell.vflip) {
        (false, false) => "",
        (true, false) => " hflip",
        (false, true) => " vflip",
        (true, true) => " hflip+vflip",
    };
    let detail = format!(
        "{plane} cell ({col},{row}) @ VRAM ${entry:04X} = ${word:04X}: tile ${:03X} @ VRAM \
         ${lo:04X}-${hi:04X}, pal {}{flips}{}, plane pixel ({px},{py})",
        cell.tile,
        cell.palette,
        if cell.priority { " hi-pri" } else { "" },
    );

    Some(CellReading {
        head,
        screen,
        detail,
        unestablished: unestablished(inp, on_screen),
    })
}

/// **What a click could not establish**, on the same armed-H-interrupt condition [`scroll_note`] measures
/// and for the same reason: a register read is a peek, and reg `$00` bit 4 is the mechanism by which a
/// game moves the registers part way down a frame.
///
/// It is deliberately **not** the note beside the picture repeated. The picture's caveat is about the
/// scroll it drew with; a click's caveat has to be about the part of *this answer* the peek actually
/// undermines, and that differs by view:
///
/// * **Scrolled**, the pointer was mapped to a cell *through* that scroll, so the peek can put the reading
///   on the wrong cell. The identity is in doubt.
/// * **Unscrolled**, the identity is a direct read of the map and cannot be wrong for this reason at all.
///   What the peek undermines is the other sentence: whether the screen is showing the cell.
/// * **The window plane** never takes the scroll, and [`scroll_note`] correctly says nothing about it. But
///   its coverage is not scroll-free: it comes from regs `$11`/`$12`, which the core's `window_span_at`
///   documents as *"a peek in the same sense"*. So the caveat is carried, naming those registers rather
///   than a scroll the window does not have.
///
/// The uncovered case is caveated too, and that is not an oversight: the doubt runs both ways, and a peek
/// that could hide a cell which is on screen is exactly as wrong as one that shows a cell which is not.
fn unestablished(inp: &Inputs, on_screen: bool) -> Vec<String> {
    let Some(line) = inp.hint_line else {
        return Vec::new();
    };
    if inp.scrolled {
        return vec![format!(
            "A horizontal interrupt is armed at line {line}. This picture is sampled through a scroll \
             read once from the registers, so if the game moves the scroll part way down the frame the \
             pointer landed on a different cell from the one named above."
        )];
    }
    let source = if inp.plane == Plane::Window {
        "the window band registers ($11 and $12), read once, now"
    } else {
        "the same single register read the viewport outline is drawn from"
    };
    let claim = if on_screen {
        "whether the screen is showing it"
    } else {
        "whether the screen is showing it, which this answer says it is not,"
    };
    vec![format!(
        "A horizontal interrupt is armed at line {line}. The cell named above is a direct read of the \
         map and does not depend on that, but {claim} comes from {source}, and a game that moves it part \
         way down the frame cannot be seen from here."
    )]
}

// ---------------------------------------------------------------------------------------------------
// The fingerprint's mixer
// ---------------------------------------------------------------------------------------------------

/// FNV-1a's constants over 64-bit words rather than bytes.
///
/// Eight bytes a step, deliberately: this hash runs on every repaint the tab is visible, and the whole
/// argument for hashing at all is that it costs less than the raster it skips. Byte at a time over a
/// fully-referenced plane would be 64 K rounds; this is 8 K.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn mix(&mut self, v: u64) {
        self.0 ^= v;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }
    /// Zero-padded to a multiple of 8. The length is mixed too, so a run of trailing zero bytes cannot
    /// collide with the same run one byte shorter.
    fn bytes(&mut self, b: &[u8]) {
        self.mix(b.len() as u64);
        for c in b.chunks(8) {
            let mut w = [0u8; 8];
            w[..c.len()].copy_from_slice(c);
            self.mix(u64::from_le_bytes(w));
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// The panel's own state
// ---------------------------------------------------------------------------------------------------

/// What this tab keeps between repaints: the selection, the toggles, the uploaded texture, and the
/// fingerprint that texture was drawn from.
///
/// Not persisted, on [`crate::screen_pick::Panel`]'s reasoning: which plane you were looking at is a
/// *looking at it* choice, and the layout store is a separate question.
pub struct Panel {
    /// Which plane is on screen. Plane A by default: it is the one a game puts its foreground on.
    ///
    /// **Set through [`Panel::select`], not by hand**, because it is not the only state a plane change
    /// invalidates: a standing reading about plane A drawn beside a picture of plane B is a correct
    /// sentence in front of the wrong picture, which is the shape of answer this window is least able to
    /// afford. The field stays public so a reader (`is_window`, the selector's own highlight) does not go
    /// through an accessor for a fact.
    pub plane: Plane,
    /// Whether the live scroll is applied to the raster.
    pub apply_scroll: bool,
    /// Whether the viewport outline is drawn. On by default, and offered off because an outline over the
    /// exact cells you are reading is an outline in the way.
    pub outline: bool,
    /// The texture, and the fingerprint of the inputs it was rasterised from.
    tex: Option<egui::TextureHandle>,
    drawn: Option<u64>,
    /// How many rasters this panel has done, and how many repaints it has survived without one. Shown,
    /// because a claim about not doing work is worth nothing unless it can be watched.
    rasters: u64,
    repaints: u64,
    /// **The standing answer to the last click.** Standing rather than a toast, on the Screen tab's rule:
    /// a toast expires and the fact you asked what a cell was does not.
    ///
    /// Cleared by [`Panel::select`] alone. The scroll and outline toggles deliberately do **not** clear
    /// it: they change how the picture is drawn, not what is in the cell the reading names, and a reading
    /// that vanished when you reached for a checkbox would be a worse surface than a slightly old one.
    reading: Option<CellReading>,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            plane: Plane::A,
            apply_scroll: false,
            outline: true,
            tex: None,
            drawn: None,
            rasters: 0,
            repaints: 0,
            reading: None,
        }
    }
}

impl Panel {
    /// How many times the picture has been rasterised, out of how many repaints asked for it.
    pub fn work(&self) -> (u64, u64) {
        (self.rasters, self.repaints)
    }

    /// **Choose a plane**, and drop any standing reading with it.
    ///
    /// The drop is the whole reason this is a method. [`CellReading`] names its plane in every one of its
    /// parts, so a stale one is not *false* after a switch; it is a true sentence about plane A sitting
    /// beside a picture of plane B, which a person reads as an answer about what they are looking at. A
    /// switch back to the same plane keeps the reading, because nothing about it went stale.
    pub fn select(&mut self, plane: Plane) {
        if self.plane != plane {
            self.plane = plane;
            self.reading = None;
        }
    }

    /// The standing answer to the last click, if there has been one.
    pub fn reading(&self) -> Option<&CellReading> {
        self.reading.as_ref()
    }

    /// **A click at raster pixel (`rx`, `ry`).**
    ///
    /// A click that lands off the picture leaves the previous reading where it was rather than clearing
    /// it, which is [`crate::screen_pick`]'s behaviour for the same gesture and for the same reason: the
    /// pointer leaving the image is not an answer, and replacing an answer with nothing is worse than
    /// leaving the last one standing.
    ///
    /// `inp` is this repaint's gather, so the reading describes the picture that is on the glass rather
    /// than a re-read of the machine taken at click time. The Screen tab has to refuse when those two
    /// separate; here they cannot, because there is one gather per repaint and the texture is a function
    /// of it.
    pub fn click(&mut self, inp: &Inputs, rx: usize, ry: usize) {
        if let Some(r) = identify(inp, rx, ry) {
            self.reading = Some(r);
        }
    }

    /// Gather, fingerprint, and rasterise **only if the fingerprint moved**. Returns the inputs it
    /// gathered so the caller can draw the facts beside the picture.
    ///
    /// The texture is created on the first raster and `set` afterwards, so a plane that changes size does
    /// not leak a handle per size.
    pub fn refresh(&mut self, ctx: &egui::Context, vdp: &Vdp, ink: Ink) -> Inputs {
        self.repaints += 1;
        let inp = gather(vdp, self.plane, self.apply_scroll, ink);
        // The outline is baked into the raster rather than painted over it, so it belongs to the same
        // change decision as the pixels under it.
        let want = if self.outline {
            inp.fingerprint
        } else {
            // A distinct value for the same inputs with the outline off, so toggling it redraws.
            inp.fingerprint ^ 0x5EED_0111_0000_0001
        };
        if self.drawn != Some(want) {
            let img = raster(vdp, &inp, ink, self.outline);
            let opts = egui::TextureOptions::NEAREST;
            match self.tex.as_mut() {
                Some(t) => t.set(img, opts),
                None => self.tex = Some(ctx.load_texture("planes", img, opts)),
            }
            self.drawn = Some(want);
            self.rasters += 1;
        }
        inp
    }

    /// The texture, once there has been a raster.
    pub fn texture(&self) -> Option<&egui::TextureHandle> {
        self.tex.as_ref()
    }
}

// ---------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::rng::SplitMix64;

    fn ink() -> Ink {
        Ink {
            empty_a: Color32::from_rgb(1, 1, 1),
            empty_b: Color32::from_rgb(2, 2, 2),
            outline: Color32::from_rgb(255, 0, 255),
        }
    }

    fn set_reg(v: &mut Vdp, r: u8, val: u8) {
        v.control_write(0x8000 | ((r as u16) << 8) | val as u16, 0);
    }

    fn put_cell(v: &mut Vdp, addr: usize, word: u16) {
        v.vram_mut()[addr] = (word >> 8) as u8;
        v.vram_mut()[addr + 1] = (word & 0xFF) as u8;
    }

    /// Arm a data-port write of `code` at VRAM/CRAM/VSRAM byte `addr`, the two control words the
    /// hardware takes. Lifted from `render.rs`'s own fixtures so these tests drive the real ports.
    fn setup_write(v: &mut Vdp, code: u16, addr: u16) {
        v.control_write(((code & 0x03) << 14) | (addr & 0x3FFF), 0);
        v.control_write((((code >> 2) & 0x0F) << 4) | (addr >> 14), 0);
    }

    fn write_cram(v: &mut Vdp, index: usize, word: u16) {
        setup_write(v, 0x03, (index * 2) as u16);
        v.data_write(word);
    }

    fn write_vsram(v: &mut Vdp, word_index: usize, word: u16) {
        setup_write(v, 0x05, (word_index * 2) as u16);
        v.data_write(word);
    }

    /// A plane-A fixture: H32 (256 px), plane A at `$C000`, 32 by 32 cells, h-scroll table at `$8000`,
    /// full h and full v scroll, tile 1 solid colour 1, CRAM entry 1 red.
    fn fixture() -> Vdp {
        let mut v = Vdp::power_on(&mut SplitMix64::new(1));
        v.control_write(0x8104, 0); // reg 1 = mode 5
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x01, 0x44); // display on + M5
        set_reg(&mut v, 0x02, 0x30); // plane A base $C000
        set_reg(&mut v, 0x04, 0x07); // plane B base $E000
        set_reg(&mut v, 0x03, 0x28); // window base $A000
        set_reg(&mut v, 0x10, 0x00); // 32 x 32
        set_reg(&mut v, 0x0D, 0x20); // h-scroll table $8000
        set_reg(&mut v, 0x0B, 0x00); // full h, full v
        for i in 0..32 {
            v.vram_mut()[32 + i] = 0x11; // tile 1 solid nibble 1
        }
        write_cram(&mut v, 1, 0x000E); // entry 1 = red
        v
    }

    fn gathered(v: &Vdp, plane: Plane, scrolled: bool) -> Inputs {
        gather(v, plane, scrolled, ink())
    }

    /// The pixel order inside a tile is the hardware's: **high nibble is the left pixel**, and a tile row
    /// is 4 bytes. A viewer that reads the low nibble first mirrors every pixel pair in the plane, which
    /// looks almost right and is exactly the class of error this panel exists to remove rather than add.
    #[test]
    fn a_tile_row_is_four_bytes_high_nibble_first() {
        let mut v = fixture();
        // Tile 2, row 0 = bytes $12 $34 $56 $78 → pixels 1,2,3,4,5,6,7,8.
        for (i, b) in [0x12u8, 0x34, 0x56, 0x78].into_iter().enumerate() {
            v.vram_mut()[2 * 32 + i] = b;
        }
        let cell = Cell {
            tile: 2,
            palette: 0,
            hflip: false,
            vflip: false,
            priority: false,
        };
        let row: Vec<u8> = (0..8).map(|x| nibble(v.vram(), &cell, x, 0)).collect();
        assert_eq!(row, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    /// Both flips mirror the pixel the cell's bits say they mirror, and they compose.
    #[test]
    fn the_flips_mirror_the_axis_they_name() {
        let mut v = fixture();
        // Tile 3: row 0 = 1,0,0,0,0,0,0,0 ; row 7 = 0,...,0,2.
        v.vram_mut()[3 * 32] = 0x10;
        v.vram_mut()[3 * 32 + 7 * 4 + 3] = 0x02;
        let base = Cell {
            tile: 3,
            palette: 0,
            hflip: false,
            vflip: false,
            priority: false,
        };
        assert_eq!(nibble(v.vram(), &base, 0, 0), 1);
        let hf = Cell {
            hflip: true,
            ..base
        };
        assert_eq!(
            nibble(v.vram(), &hf, 7, 0),
            1,
            "hflip moves it to the right edge"
        );
        let vf = Cell {
            vflip: true,
            ..base
        };
        assert_eq!(
            nibble(v.vram(), &vf, 0, 7),
            1,
            "vflip moves it to the bottom"
        );
        let both = Cell {
            hflip: true,
            vflip: true,
            ..base
        };
        assert_eq!(nibble(v.vram(), &both, 7, 7), 1);
        assert_eq!(
            nibble(v.vram(), &both, 0, 0),
            2,
            "and brings row 7 to row 0"
        );
    }

    /// The unscrolled raster is **the whole plane**, not the screen: 32 by 32 cells is 256 by 256 pixels
    /// even on a 256 by 224 display, and the two must not be confused because confusing them is precisely
    /// the view this panel replaces.
    #[test]
    fn the_unscrolled_raster_is_the_whole_plane() {
        let v = fixture();
        let inp = gathered(&v, Plane::A, false);
        assert_eq!(inp.raster_size(), (256, 256));
        assert_eq!(v.active_display(), (256, 224));
        let img = raster(&v, &inp, ink(), true);
        assert_eq!(img.size, [256, 256]);
    }

    /// With the scroll applied the raster is display sized, because per-line scroll is only defined for
    /// the lines the display has.
    #[test]
    fn the_scrolled_raster_is_display_sized() {
        let v = fixture();
        let inp = gathered(&v, Plane::A, true);
        assert_eq!(inp.raster_size(), (256, 224));
        assert_eq!(raster(&v, &inp, ink(), true).size, [256, 224]);
    }

    /// The window plane never takes the scroll, however the toggle is set.
    #[test]
    fn the_window_refuses_the_scroll_toggle() {
        let v = fixture();
        let inp = gathered(&v, Plane::Window, true);
        assert!(!inp.scrolled);
        let note = scroll_note(&inp);
        assert!(note.horizontal.contains("does not scroll"));
        assert!(note.unestablished.is_none());
    }

    /// Nibble 0 is drawn as the checker and never as a CRAM colour, and the checker alternates on a
    /// 4-pixel grid rather than the 8-pixel cell grid.
    #[test]
    fn transparent_is_the_checker_not_a_colour() {
        let mut v = fixture();
        write_cram(&mut v, 0, 0x0EEE); // entry 0 = white, so a colour-0 read would be obvious
        put_cell(&mut v, 0xC000, 0x0000); // cell 0 uses tile 0, which is all zero
        let inp = gathered(&v, Plane::A, false);
        // Outline off: the viewport covers the origin, and this row is about the colour under it.
        let img = raster(&v, &inp, ink(), false);
        assert_eq!(img.pixels[0], ink().empty_a);
        assert_eq!(
            img.pixels[CHECKER],
            ink().empty_b,
            "the checker turns over at 4"
        );
        assert_eq!(img.pixels[2 * CHECKER], ink().empty_a);
    }

    /// A cell's palette line selects the CRAM row, so the same tile draws in four colours.
    #[test]
    fn the_palette_line_selects_the_cram_row() {
        let mut v = fixture();
        write_cram(&mut v, 1, 0x000E); // line 0 entry 1 = red
        write_cram(&mut v, 17, 0x0E00); // line 1 entry 1 = blue
        put_cell(&mut v, 0xC000, 0x0001); // cell 0: tile 1, palette 0
        put_cell(&mut v, 0xC002, 0x2001); // cell 1: tile 1, palette 1
        let inp = gathered(&v, Plane::A, false);
        let img = raster(&v, &inp, ink(), false);
        assert_ne!(img.pixels[0], img.pixels[8]);
    }

    /// ⚑ **The render-on-change gate.** An edit to a tile the plane draws moves the fingerprint.
    #[test]
    fn the_fingerprint_moves_when_a_drawn_tile_moves() {
        let mut v = fixture();
        put_cell(&mut v, 0xC000, 0x0001);
        let before = gathered(&v, Plane::A, false).fingerprint;
        v.vram_mut()[32] = 0x22;
        assert_ne!(before, gathered(&v, Plane::A, false).fingerprint);
    }

    /// ⚑ **And the other half of it, which is what makes the first half worth anything.** An edit to a
    /// tile no cell references does not move the picture, so it must not move the fingerprint. Without
    /// this row a fingerprint over all of VRAM would pass the test above and rasterise on every frame.
    #[test]
    fn the_fingerprint_holds_when_an_undrawn_tile_moves() {
        let mut v = fixture();
        for i in 0..(32 * 32) {
            put_cell(&mut v, 0xC000 + i * 2, 0x0001); // every cell draws tile 1
        }
        let before = gathered(&v, Plane::A, false).fingerprint;
        v.vram_mut()[900 * 32] = 0x77; // tile 900, referenced by nothing
        assert_eq!(before, gathered(&v, Plane::A, false).fingerprint);
    }

    /// A map edit, a palette edit and a scroll move each redraw.
    #[test]
    fn the_fingerprint_moves_on_map_palette_and_scroll() {
        let mut v = fixture();
        let before = gathered(&v, Plane::A, false).fingerprint;
        put_cell(&mut v, 0xC000, 0x0001);
        let after_map = gathered(&v, Plane::A, false).fingerprint;
        assert_ne!(before, after_map, "map");
        write_cram(&mut v, 1, 0x0EE0);
        let after_cram = gathered(&v, Plane::A, false).fingerprint;
        assert_ne!(after_map, after_cram, "palette");
        put_cell(&mut v, 0x8000, 0x0040); // h-scroll table line 0, plane A = 64
        assert_ne!(
            after_cram,
            gathered(&v, Plane::A, false).fingerprint,
            "scroll, because the outline follows it"
        );
    }

    /// Nothing moving means no redraw. The idle case is the one the budget rests on.
    #[test]
    fn the_fingerprint_holds_when_nothing_moves() {
        let v = fixture();
        assert_eq!(
            gathered(&v, Plane::A, false).fingerprint,
            gathered(&v, Plane::A, false).fingerprint
        );
    }

    /// The three planes fingerprint differently even when their maps happen to agree, so switching the
    /// selector always redraws.
    #[test]
    fn each_plane_has_its_own_fingerprint() {
        let v = fixture();
        let a = gathered(&v, Plane::A, false).fingerprint;
        let b = gathered(&v, Plane::B, false).fingerprint;
        let w = gathered(&v, Plane::Window, false).fingerprint;
        assert_ne!(a, b);
        assert_ne!(b, w);
        assert_ne!(a, w);
    }

    /// Every row of the covered region, as the x positions it covers.
    fn rows_of(inp: &Inputs, pw: usize, ph: usize) -> Vec<Vec<usize>> {
        let m = covered_mask(inp, pw, ph);
        (0..ph)
            .map(|y| (0..pw).filter(|x| m[y * pw + x]).collect())
            .collect()
    }

    /// ⚑ **The outline is a rectangle exactly when the scroll is uniform.** One code path, and this is
    /// the half of it that has to look ordinary.
    #[test]
    fn a_uniform_scroll_outlines_a_rectangle() {
        let mut v = fixture(); // h-scroll table is all zero, VSRAM zero
                               // ⚑ A 64-cell-wide plane, deliberately. At 32 cells the plane is exactly as wide as the display,
                               // the covered band wraps into itself, and its left and right sides vanish: the assertion would
                               // then be green on a degenerate shape that cannot tell a rectangle from a shear.
        set_reg(&mut v, 0x10, 0x01); // 64 by 32 cells
        let inp = gathered(&v, Plane::A, false);
        let (pw, ph) = inp.pixels();
        assert_eq!((pw, ph), (512, 256));
        let edges = covered_edges(&inp, pw, ph);
        let xs: Vec<usize> = edges.iter().map(|i| i % pw).collect();
        let ys: Vec<usize> = edges.iter().map(|i| i / pw).collect();
        assert_eq!(*xs.iter().min().unwrap(), 0);
        assert_eq!(*xs.iter().max().unwrap(), 255, "the display is 256 wide");
        assert_eq!(*ys.iter().min().unwrap(), 0);
        assert_eq!(*ys.iter().max().unwrap(), 223, "and 224 tall");
        // A rectangle's outline is its perimeter, corners counted once.
        assert_eq!(edges.len(), 2 * 256 + 2 * (224 - 2));
        // ⚑ And the region itself, which is the claim the perimeter cannot make: every one of the 224
        // displayed rows covers the same 256 columns, and the rows below the display cover none.
        let rows = rows_of(&inp, pw, ph);
        let want: Vec<usize> = (0..256).collect();
        assert!(
            rows[..224].iter().all(|r| *r == want),
            "every row identical"
        );
        assert!(rows[224..].iter().all(Vec::is_empty));
    }

    /// ⚑ **And it is not a rectangle when the scroll is per line.** This is the shape the evening was
    /// spent establishing by hand, and this row is the reason a glance would have answered it.
    #[test]
    fn a_per_line_scroll_outlines_a_shape_that_is_not_a_rectangle() {
        let mut v = fixture();
        set_reg(&mut v, 0x10, 0x01); // 64 by 32 cells, so the band does not fill the plane
        set_reg(&mut v, 0x0B, 0x03); // horizontal: one value per line
        for line in 0..224usize {
            // A different h-scroll on every line: the staircase a per-line corrected floor draws.
            put_cell(&mut v, 0x8000 + line * 4, (line as u16) & 0x03FF);
        }
        let inp = gathered(&v, Plane::A, false);
        assert_eq!(inp.hmode, 3);
        assert_eq!(inp.scroll[100].hscroll, 100, "line 100 reads its own entry");
        let (pw, ph) = inp.pixels();
        let rows = rows_of(&inp, pw, ph);
        // ⚑ **The region, not the perimeter.** A band sheared by one pixel per line has the SAME edge
        // count as the rectangle to within the two rows at top and bottom, which this parcel discovered
        // by writing the count assertion first and watching it pass on both shapes. So the claim is made
        // where the difference actually is: row 100 covers different columns from row 0, shifted left by
        // its own scroll, and the plane wraps.
        assert_ne!(rows[0], rows[100], "a per-line scroll is not one band");
        let shifted: Vec<usize> = rows[0]
            .iter()
            .map(|x| (x + pw - 100) % pw)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(rows[100], shifted, "row 100 is row 0 moved by 100 pixels");
    }

    /// ⚑ **THE TRAP.** With an H-interrupt armed, one read of reg $0B cannot establish the mode the frame
    /// was drawn with, and the panel says so by name and by line. Without one it says nothing, because a
    /// caveat on every reply is a caveat nobody reads.
    #[test]
    fn an_armed_h_interrupt_is_stated_and_names_its_line() {
        let mut v = fixture();
        assert!(
            scroll_note(&gathered(&v, Plane::A, false))
                .unestablished
                .is_none(),
            "no interrupt armed, so the peek is the frame"
        );
        set_reg(&mut v, 0x0A, 174); // the line the commercial ROM switches at
        set_reg(&mut v, 0x00, 0x10); // reg $00 bit 4: arm the horizontal interrupt
        let note = scroll_note(&gathered(&v, Plane::A, false));
        let said = note
            .unestablished
            .expect("an armed h-interrupt must be stated");
        assert!(said.contains("174"), "it names the line: {said}");
        assert!(said.contains("cannot be seen from here"), "{said}");
    }

    /// The mode sentences are the reader's, not the register's: every one of the four horizontal codes
    /// gets its own words, and none of them quotes a specification section at the person reading it.
    #[test]
    fn every_scroll_mode_says_what_it_is() {
        let mut v = fixture();
        let mut said = Vec::new();
        for code in 0..4u8 {
            set_reg(&mut v, 0x0B, code);
            let n = scroll_note(&gathered(&v, Plane::A, false));
            assert!(!n.horizontal.contains('\u{2014}') && !n.horizontal.contains('\u{2013}'));
            assert!(!n.horizontal.contains('§'));
            said.push(n.horizontal);
        }
        said.sort();
        said.dedup();
        assert_eq!(said.len(), 4, "four modes, four sentences");
    }

    /// Two-cell vertical scroll is sampled per 16-pixel column, so two columns with different VSRAM
    /// words sample different plane rows.
    #[test]
    fn two_cell_vertical_scroll_is_sampled_per_column() {
        let mut v = fixture();
        set_reg(&mut v, 0x0B, 0x04); // vertical: per 2-cell column
        write_vsram(&mut v, 0, 0); // column 0, plane A
        write_vsram(&mut v, 2, 64); // column 1, plane A
        let inp = gathered(&v, Plane::A, true);
        assert!(inp.vcolumns);
        let (pw, ph) = inp.pixels();
        let sc = &inp.scroll[0];
        assert_eq!(sample(sc, 0, 0, pw, ph).1, 0);
        assert_eq!(sample(sc, 16, 0, pw, ph).1, 64);
    }

    /// The plane wraps, so a scroll past its width comes back at the left rather than reading past the
    /// end of the raster.
    #[test]
    fn the_sample_wraps_with_the_plane() {
        let v = fixture();
        let inp = gathered(&v, Plane::A, true);
        let (pw, ph) = inp.pixels();
        let sc = PlaneScroll {
            hscroll: 1,
            vscroll: VScroll::Full(0),
        };
        assert_eq!(sample(&sc, 0, 0, pw, ph).0, pw - 1);
        let sc = PlaneScroll {
            hscroll: 0,
            vscroll: VScroll::Full((ph - 1) as u16),
        };
        assert_eq!(sample(&sc, 0, 1, pw, ph).1, 0);
    }

    /// The base and the grid this panel reports are the renderer's own answers, never a second decode.
    #[test]
    fn the_base_and_grid_come_from_the_renderer() {
        let v = fixture();
        for plane in [Plane::A, Plane::B, Plane::Window] {
            let inp = gathered(&v, plane, false);
            assert_eq!(inp.base, v.plane_base(plane));
            assert_eq!((inp.cols, inp.rows), v.plane_grid(plane));
            assert_eq!(inp.cells.len(), inp.cols as usize * inp.rows as usize);
        }
    }

    /// ⚑ **The whole render-on-change claim, at the seam that makes it**, rather than at the fingerprint
    /// alone: three repaints with nothing moving rasterise **once**, and the fourth, after a byte of a
    /// drawn tile changes, rasterises again. A fingerprint that moved correctly but was not consulted
    /// would pass every test above this one and still upload 512 KB per frame.
    #[test]
    fn repaints_with_nothing_moving_rasterise_once() {
        let mut v = fixture();
        put_cell(&mut v, 0xC000, 0x0001);
        let ctx = egui::Context::default();
        let mut p = Panel::default();
        for _ in 0..3 {
            p.refresh(&ctx, &v, ink());
        }
        assert_eq!(p.work(), (1, 3), "three repaints, one raster");
        v.vram_mut()[32] = 0x22; // tile 1 is drawn by cell 0
        p.refresh(&ctx, &v, ink());
        assert_eq!(
            p.work(),
            (2, 4),
            "the art moved, so the picture was drawn again"
        );
        // And a toggle is a change too: the outline is baked into the pixels.
        p.outline = false;
        p.refresh(&ctx, &v, ink());
        assert_eq!(p.work(), (3, 5));
    }

    // -----------------------------------------------------------------------------------------------
    // The click
    // -----------------------------------------------------------------------------------------------

    /// A 64-by-32-cell plane A whose every cell holds a **different word**, so an indexing mistake cannot
    /// hide behind two cells that happen to agree.
    ///
    /// 64 wide on purpose, as `a_uniform_scroll_outlines_a_rectangle` argues for the outline and for the
    /// same reason: at 32 cells the plane is exactly as wide as the H32 display, everything is covered,
    /// and half of what these rows measure degenerates.
    fn distinct_map() -> Vdp {
        let mut v = fixture();
        set_reg(&mut v, 0x10, 0x01); // 64 by 32 cells
        for i in 0..(64 * 32) {
            // Word == index: tile bits are 10..0, so 2048 cells map onto 2048 distinct words with no
            // palette, flip or priority bit set, and `encode_cell` round trips them exactly.
            put_cell(&mut v, 0xC000 + i * 2, i as u16);
        }
        v
    }

    /// **The plain case.** A click on the unscrolled picture names the cell whose eight-by-eight square
    /// the pixel is in, and the tile that cell's word holds.
    #[test]
    fn a_click_names_the_cell_under_it() {
        let v = distinct_map();
        let inp = gathered(&v, Plane::A, false);
        // Raster pixel (100,60) is column 12, row 7 of the map; its word is 7 * 64 + 12 = 460.
        let r = identify(&inp, 100, 60).expect("a pixel inside the raster identifies a cell");
        assert!(r.head.contains("column 12 row 7"), "{}", r.head);
        assert!(r.head.contains("$1CC"), "460 is $1CC: {}", r.head);
        assert!(r.detail.contains("cell (12,7)"), "{}", r.detail);
        assert!(r.detail.contains("plane pixel (100,60)"), "{}", r.detail);
    }

    /// ⚑ **The entry address is the core's own, and this is what says so.**
    ///
    /// `entry_addr` is the one address this module spells that the renderer does not hand over, and the
    /// module's whole standing rule is that a second spelling of an address is how a debug view and the
    /// picture it explains come to describe different bytes. So every cell of the plane is walked and the
    /// two bytes at the computed address are compared against the word `plane_decoded` returned for that
    /// cell. A transposed stride, a dropped `* 2` and the wrong wrap each move this row, and none of them
    /// moves any other row in the file.
    #[test]
    fn the_entry_address_names_the_word_the_cell_came_from() {
        let v = distinct_map();
        let inp = gathered(&v, Plane::A, false);
        assert_eq!(inp.cells.len(), 64 * 32);
        for row in 0..inp.rows as usize {
            for col in 0..inp.cols as usize {
                let at = entry_addr(&inp, col, row);
                let word = ((v.vram()[at] as u16) << 8) | v.vram()[at + 1] as u16;
                let cell = &inp.cells[row * inp.cols as usize + col];
                assert_eq!(
                    word,
                    encode_cell(cell),
                    "cell ({col},{row}) at ${at:04X} is not the word the decode came from"
                );
            }
        }
    }

    /// **A click on the scrolled view goes through the scroll the raster went through**, and the test
    /// spells that arithmetic itself rather than calling [`sample`], so the two are independent.
    #[test]
    fn a_click_on_the_scrolled_view_goes_through_the_scroll_the_raster_did() {
        let mut v = distinct_map();
        set_reg(&mut v, 0x0B, 0x03); // horizontal: one value per line
        for line in 0..224usize {
            put_cell(&mut v, 0x8000 + line * 4, (line as u16) & 0x03FF);
        }
        let inp = gathered(&v, Plane::A, true);
        assert!(inp.scrolled);
        let (pw, _) = inp.pixels();
        assert_eq!(pw, 512);

        let (rx, ry) = (100usize, 60usize);
        // The arithmetic, written out here: line 60 scrolls by 60, and the plane wraps.
        let px = (rx + pw - 60) % pw;
        let (col, row) = (px / 8, ry / 8);
        let want = row * 64 + col;

        let r = identify(&inp, rx, ry).expect("a pixel inside the raster identifies a cell");
        assert!(
            r.head.contains(&format!("column {col} row {row}")),
            "want column {col} row {row}: {}",
            r.head
        );
        assert!(
            r.head.contains(&format!("${want:03X}")),
            "want tile ${want:03X}: {}",
            r.head
        );
        // And the unscrolled view of the same raster pixel is a *different* cell, so the row above is
        // measuring the scroll rather than passing on an identity that happened to be right.
        let flat = identify(&gathered(&v, Plane::A, false), rx, ry).unwrap();
        assert_ne!(flat.head, r.head, "the scroll has to change the answer");
    }

    /// ⚑⚑ **THE PARITY ROW, and the reason this surface is safe to put beside the Screen tab.**
    ///
    /// The two clicks answer **different questions** — the game picture is asked *which layer won at this
    /// dot*, and this picture is asked *what word is in this cell of this map* — and they legitimately
    /// disagree about plenty. What they must **never** disagree about is the nametable word itself.
    ///
    /// So: take screen dots plane A actually won, ask `Vdp::pixel_attribution` (the derivation
    /// `emulator/pixel_attribution` serves) for the cell it says drew each one, and ask this module for
    /// the cell under the same dot of the scrolled view. Every decoded field must match.
    ///
    /// Both sides bottom out in the core's own `nametable_cell`, which is deliberate and is also exactly
    /// why this row is worth writing: what it measures is **the addressing on top of that decode**, in two
    /// independently written paths (`plane_sample`'s masked wrap against `plane_scroll_report` plus
    /// `sample`'s `rem_euclid`, and a flat row-major index against a stride multiply). That is where a
    /// transpose lives, and a transpose is invisible to every row above.
    #[test]
    fn the_viewer_and_pixel_attribution_agree_about_the_word() {
        use oracle_core::render::Layer;

        let mut v = distinct_map();
        // Opaque art for the tiles the sampled dots reach, so plane A wins the dot rather than the
        // backdrop: without this the attribution is about the backdrop and the row measures nothing.
        for t in 1..256usize {
            for b in 0..32 {
                v.vram_mut()[t * 32 + b] = 0x11;
            }
        }
        // Cells 1..=255 only, so every sampled cell has art. Distinct within any screen-sized window,
        // which is what makes a transpose visible.
        for i in 0..(64 * 32) {
            put_cell(&mut v, 0xC000 + i * 2, 1 + (i as u16 % 255));
        }
        // A scroll with structure in both axes: per-line horizontal, and a whole-screen vertical.
        set_reg(&mut v, 0x0B, 0x03);
        for line in 0..224usize {
            put_cell(&mut v, 0x8000 + line * 4, ((line * 3) as u16) & 0x03FF);
        }
        write_vsram(&mut v, 0, 40);

        let inp = gathered(&v, Plane::A, true);
        assert!(inp.scrolled);

        let mut compared = 0;
        for y in (0..224u16).step_by(17) {
            for x in (0..256u16).step_by(13) {
                let attr = v.pixel_attribution(x, y);
                // Only the dots plane A won: the other winners are answering a question this panel does
                // not ask, which is the whole point of the surface being separate.
                if attr.winner != Layer::PlaneA {
                    continue;
                }
                let cell = attr.cell.expect("a plane winner reports its cell");
                let (px, py) = plane_pixel_at(&inp, x as usize, y as usize)
                    .expect("a screen dot is inside the scrolled raster");
                let mine = &inp.cells[(py / 8) * inp.cols as usize + (px / 8)];
                assert_eq!(
                    (
                        mine.tile,
                        mine.palette,
                        mine.hflip,
                        mine.vflip,
                        mine.priority
                    ),
                    (
                        cell.tile,
                        cell.palette,
                        cell.hflip,
                        cell.vflip,
                        cell.priority
                    ),
                    "dot ({x},{y}) -> plane pixel ({px},{py}): the viewer and pixel_attribution \
                     disagree about the word"
                );
                // ⚑ **And the SHIPPED path, not only the derivation under it.** The comparison above
                // indexes `inp.cells` the way `identify` does; if `identify` indexed it some other way,
                // that row would still be green while the panel named a different cell. Measured: a
                // transpose of `identify`'s own index left the assertion above passing. So the expected
                // fragment is composed from **the tool's answer** and looked for in the panel's sentence.
                // Composed, never parsed: this reads the panel's output for a string built out of the
                // other surface's reply, which is the opposite of recovering structure from prose.
                let r = identify(&inp, x as usize, y as usize).expect("the dot is on the raster");
                assert!(
                    r.head
                        .ends_with(&format!("{TILE_SPACE} tile ${:03X}.", cell.tile)),
                    "dot ({x},{y}): the panel's own sentence does not name the word \
                     pixel_attribution reports: {}",
                    r.head
                );
                compared += 1;
            }
        }
        // ⚑ The control on the measurement itself. A `continue` that skipped every dot would leave the
        // loop above green having asserted nothing, which is this lane's most-repeated failure: a
        // decorated tool's failure is indistinguishable from its empty result.
        assert!(
            compared >= 100,
            "the fixture must actually put plane A on the screen; compared {compared} dots"
        );
    }

    /// **A cell the screen is not showing is said not to be shown**, and a cell it is showing gets the
    /// sentence that keeps this answer apart from the Screen tab's.
    ///
    /// The coverage comes from [`covered_mask`], which is the same mask the viewport outline on the
    /// picture is drawn from, so the words and the outline in front of a person cannot disagree.
    #[test]
    fn a_cell_off_the_screen_is_said_to_be_off_the_screen() {
        let v = distinct_map(); // 512 pixels wide, on a 256-pixel display, no scroll
        let inp = gathered(&v, Plane::A, false);

        let inside = identify(&inp, 100, 8).unwrap();
        assert!(
            inside.screen.contains("was showing this cell"),
            "{}",
            inside.screen
        );
        assert!(
            inside.screen.contains("Screen tab"),
            "the covered case has to say whose question the other one is: {}",
            inside.screen
        );

        let outside = identify(&inp, 400, 8).unwrap();
        assert!(
            outside.screen.contains("was not showing this cell"),
            "{}",
            outside.screen
        );
        assert!(
            !outside.screen.contains("Screen tab"),
            "there is no dot to confuse this with, so the clause would be noise: {}",
            outside.screen
        );
    }

    /// ⚑ **The tile index says which space it is in, in the same breath as the index**, and it never
    /// names a slot in anybody else's.
    ///
    /// `OVERSEER.md`'s GUI-LAYERS point 2: an editor rebases this index into a blob-local slot with a base
    /// constant it owns, the rebase can land outside the blob, and it is *not* rescued by a capacity
    /// check. An unchecked rebase is indistinguishable from a correct answer, so this panel states the
    /// space it does own and does not do that arithmetic. The constant is
    /// `oracle_frontend::pick::TILE_SPACE`, shared with the Screen tab rather than a second string that
    /// agrees today.
    #[test]
    fn the_reading_names_the_space_the_tile_index_is_in() {
        let v = distinct_map();
        let inp = gathered(&v, Plane::A, false);
        let r = identify(&inp, 100, 60).unwrap();
        assert!(r.head.contains(TILE_SPACE), "{}", r.head);
        let all = r.text();
        assert!(
            !all.to_lowercase().contains("slot"),
            "no surface here may name a slot in an editor's space: {all}"
        );
        // House style, on the rows `every_scroll_mode_says_what_it_is` already holds the notes to: no em
        // or en dash standing in for a full stop, and no specification citations at a reader.
        assert!(
            !all.contains('\u{2014}') && !all.contains('\u{2013}'),
            "{all}"
        );
        assert!(!all.contains('§'), "{all}");
    }

    /// ⚑ **THE TRAP, arriving on a click.** With an H interrupt armed, the caveat a click earns is about
    /// the part of *this answer* the peek undermines, and that is a different part in each view.
    #[test]
    fn an_armed_h_interrupt_reaches_a_click_and_says_what_it_undermines() {
        let mut v = distinct_map();
        // Unarmed: silence, because a caveat on every reply is a caveat nobody reads.
        assert!(identify(&gathered(&v, Plane::A, false), 100, 60)
            .unwrap()
            .unestablished
            .is_empty());

        set_reg(&mut v, 0x0A, 174);
        set_reg(&mut v, 0x00, 0x10); // reg $00 bit 4 arms the horizontal interrupt

        // Unscrolled: the identity is a direct map read and cannot be wrong for this reason. What the
        // peek undermines is the coverage sentence.
        let flat = identify(&gathered(&v, Plane::A, false), 100, 60).unwrap();
        let said = flat.unestablished.first().expect("armed, so stated");
        assert!(said.contains("174"), "it names the line: {said}");
        assert!(said.contains("direct read of the map"), "{said}");
        assert!(said.contains("whether the screen is showing it"), "{said}");

        // Scrolled: the pointer was mapped through that very scroll, so the named cell itself is in
        // doubt, and the caveat has to say so instead of repeating the one above.
        let scrolled = identify(&gathered(&v, Plane::A, true), 100, 60).unwrap();
        let said = scrolled.unestablished.first().expect("armed, so stated");
        assert!(said.contains("174"), "{said}");
        assert!(
            said.contains("a different cell from the one named above"),
            "{said}"
        );
        assert!(
            !said.contains("direct read of the map"),
            "the scrolled case must not claim the identity is safe: {said}"
        );

        // The window never takes the scroll, so `scroll_note` says nothing; but its coverage is read off
        // regs $11 and $12, which are as rewritable mid frame as the scroll registers are. The caveat
        // names those rather than a scroll the window does not have.
        let win = identify(&gathered(&v, Plane::Window, true), 8, 8).unwrap();
        let said = win.unestablished.first().expect("armed, so stated");
        assert!(said.contains("$11"), "{said}");
        assert!(
            scroll_note(&gathered(&v, Plane::Window, true))
                .unestablished
                .is_none(),
            "and the picture's own note is unchanged"
        );
    }

    /// **Switching plane drops the reading.** A true sentence about plane A beside a picture of plane B
    /// is read as an answer about what you are looking at, which is the one kind of wrongness a correct
    /// string can still be.
    #[test]
    fn switching_plane_drops_the_reading() {
        let v = distinct_map();
        let inp = gathered(&v, Plane::A, false);
        let mut p = Panel::default();
        p.click(&inp, 100, 60);
        assert!(p.reading().is_some());
        p.select(Plane::A);
        assert!(p.reading().is_some(), "the same plane changes nothing");
        p.select(Plane::B);
        assert!(p.reading().is_none());
    }

    /// **A click that lands off the picture leaves the last answer standing**, which is what the Screen
    /// tab does with the same gesture: the pointer leaving the image is not an answer, and replacing an
    /// answer with nothing is worse than leaving an old one up.
    #[test]
    fn a_click_off_the_picture_leaves_the_last_answer_standing() {
        let v = distinct_map();
        let inp = gathered(&v, Plane::A, false);
        assert_eq!(inp.raster_size(), (512, 256));
        assert!(identify(&inp, 512, 0).is_none(), "one past the right edge");
        assert!(identify(&inp, 0, 256).is_none(), "one past the bottom");

        let mut p = Panel::default();
        p.click(&inp, 100, 60);
        let kept = p.reading().cloned().unwrap();
        p.click(&inp, 9000, 9000);
        assert_eq!(p.reading(), Some(&kept));
    }

    /// The readout calls the plane what its own selector button calls it, rather than spelling the name a
    /// second time.
    #[test]
    fn the_reading_calls_the_plane_what_the_button_does() {
        let v = distinct_map();
        for (plane, label) in CHOICES {
            let inp = gathered(&v, plane, false);
            let r = identify(&inp, 8, 8).unwrap();
            assert!(r.head.starts_with(label), "{label}: {}", r.head);
        }
    }

    /// The plane size register moves the grid, and the map is reshaped with it rather than read at the
    /// old width.
    #[test]
    fn the_grid_follows_the_plane_size_register() {
        let mut v = fixture();
        set_reg(&mut v, 0x10, 0x01); // 64 wide, 32 tall
        let inp = gathered(&v, Plane::A, false);
        assert_eq!((inp.cols, inp.rows), (64, 32));
        assert_eq!(inp.raster_size(), (512, 256));
    }
}
