//! The VDP scanline renderer — a **pure function of latched [`Vdp`](crate::vdp::Vdp) state + the line
//! number** (design brief `docs/2026-07-01-vdp-design.md` §1: rendering is *derived, not state*). Nothing
//! here serializes: these are `impl Vdp` methods + free value types that read the existing VDP state through
//! its public accessors, so the `Vdp` struct — which owns the frozen Oracle `state_hash` / `export_state`
//! currencies — is not touched at all. Attribution (`render_line_report`) is the *same* computation as the
//! render, never a parallel path that could drift.
//!
//! Behavioral facts are pinned in `docs/2026-07-16-vdp-render-recon.md` (cited RR1–RR7) and
//! `docs/2026-07-16-vdp-recon.md` (R8/R9); no emulator source informs this code (clean-room, audit policy 3).
//!
//! Scope (push 3, planes): backdrop, plane B, plane A + window, full h/v scroll, the R8/R9 quirks, and
//! transparency-based layer compositing. Sprites (push 4), the priority-bit ordering + shadow/highlight
//! (push 5), and DMA/FIFO (push 6) are out — see the plan `docs/plans/2026-07-16-vdp-planes.md`.

use crate::state_hash::VRAM_SIZE;
use crate::vdp::Vdp;

/// One of the three plane-stage nametables (design §4 `plane_decoded`). Sprites are a separate push.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plane {
    A,
    B,
    Window,
}

/// A decoded plane/window nametable cell — the 16-bit big-endian entry word (recon RR1):
/// bit 15 priority, bits 14–13 palette line, bit 12 vflip, bit 11 hflip, bits 10–0 tile index.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    /// Pattern index 0–2047 (× 32 = VRAM byte address).
    pub tile: u16,
    /// Palette line 0–3 (selects the CRAM row: colour index = `palette * 16 + nibble`).
    pub palette: u8,
    /// Horizontal flip.
    pub hflip: bool,
    /// Vertical flip.
    pub vflip: bool,
    /// Priority bit (decoded + reported this push; the priority *ordering* it drives is push 5).
    pub priority: bool,
}

/// A rectangular sub-region of a plane's cell grid (design §4 `plane_decoded(plane, rect?)`), in **cells**.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellRect {
    /// Leftmost cell column.
    pub col: u16,
    /// Topmost cell row.
    pub row: u16,
    /// Width in cells.
    pub cols: u16,
    /// Height in cells.
    pub rows: u16,
}

/// Which layer produced a resolved screen pixel (recon RR7). Sprites join this enum in push 4; the
/// shadow/highlight operators are push 5.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Backdrop,
    PlaneB,
    PlaneA,
    Window,
}

/// One resolved screen pixel + its provenance. Attribution **is** the render computation (design §1): the
/// pipeline produces this directly, so `render_line` and `render_line_report` cannot drift.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PixelResolution {
    /// Screen x (0-based).
    pub x: u16,
    /// The winning layer.
    pub layer: Layer,
    /// The winning CRAM index (0..=63): `palette * 16 + nibble`, or `reg $07 & 0x3F` for the backdrop.
    pub cram_index: u8,
    /// The winning cell's tile index (0 for the backdrop).
    pub tile: u16,
    /// The winning cell's palette line (0 for the backdrop).
    pub palette: u8,
    /// The winning cell's priority bit (decoded + reported; the ordering it drives is push 5).
    pub priority: bool,
}

/// A fetched plane pixel before compositing (internal).
#[derive(Clone, Copy)]
struct PlanePixel {
    nibble: u8,
    palette: u8,
    priority: bool,
    tile: u16,
}

impl PlanePixel {
    fn opaque(&self) -> bool {
        self.nibble != 0
    }
    fn cram_index(&self) -> u8 {
        self.palette * 16 + self.nibble
    }
}

/// The fixed introspection colour ramp: a 3-bit channel level (`0..=7`) → 8-bit, linear (`level × 255 / 7`,
/// integer — no floats). Matches `Vdp::cram_decoded`'s ramp (guarded by a test).
fn ramp3(level: u8) -> u8 {
    (level as u16 * 255 / 7) as u8
}

/// Decode a raw 16-bit nametable entry word into a [`Cell`] (recon RR1).
pub fn decode_cell(word: u16) -> Cell {
    Cell {
        tile: word & 0x07FF,
        palette: ((word >> 13) & 0x03) as u8,
        hflip: word & 0x0800 != 0,
        vflip: word & 0x1000 != 0,
        priority: word & 0x8000 != 0,
    }
}

/// Decode a plane's dimensions in **cells** from register $10 (recon RR3): horizontal size = bits 1–0,
/// vertical size = bits 5–4, each `0→32 / 1→64 / 3→128`. The invalid code `2` (`0b10`) is not in any
/// permitted source; it is clamped deterministically to 64 (flagged, plan decision 3 — confirm by the
/// golden-frame differential in push 5).
pub fn plane_size(reg10: u8) -> (u16, u16) {
    fn field(bits: u8) -> u16 {
        match bits & 0x03 {
            0 => 32,
            1 => 64,
            3 => 128,
            _ => 64, // invalid 0b10 — deterministic clamp (RR3 open remainder)
        }
    }
    (field(reg10 & 0x03), field((reg10 >> 4) & 0x03))
}

impl Vdp {
    /// H40 (40-cell / 320 px) mode: reg $0C bits RS0 (bit 0) + RS1 (bit 7) both set (recon RR3, matching the
    /// timing FSM's `h40`). Recomputed from `regs()` so the renderer never reaches into private VDP state.
    fn render_h40(&self) -> bool {
        self.regs()[0x0C] & 0x81 == 0x81
    }

    /// Plane A nametable base VRAM byte address (recon RR3): `(reg $02 & 0x38) << 10`.
    fn plane_a_base(&self) -> usize {
        ((self.regs()[0x02] & 0x38) as usize) << 10
    }

    /// Plane B nametable base VRAM byte address (recon RR3): `(reg $04 & 0x07) << 13`.
    fn plane_b_base(&self) -> usize {
        ((self.regs()[0x04] & 0x07) as usize) << 13
    }

    /// Window nametable base VRAM byte address (recon RR3): `(reg $03 & mask) << 10`, mask `0x3C` in H40
    /// (WD11 must be 0) / `0x3E` in H32.
    fn window_base(&self) -> usize {
        let mask = if self.render_h40() { 0x3C } else { 0x3E };
        ((self.regs()[0x03] & mask) as usize) << 10
    }

    /// The row stride (cells per row) of the window nametable: the display width in cells — 64 (H40) / 32
    /// (H32). The window is not a scrollable plane; its map is sized to the screen.
    fn window_stride(&self) -> u16 {
        if self.render_h40() {
            64
        } else {
            32
        }
    }

    /// Read one plane/window nametable cell at grid position (`col`, `row`) given its `base` and row `stride`
    /// (both in the plane's own units); the grid wraps modulo the plane dimensions the caller passes via
    /// `stride`/`rows`. Returns the decoded [`Cell`] (recon RR1).
    fn nametable_cell(&self, base: usize, stride: u16, col: u16, row: u16) -> Cell {
        let addr = base + (row as usize * stride as usize + col as usize) * 2;
        let a = addr & (VRAM_SIZE - 1);
        let word = ((self.vram()[a] as u16) << 8) | self.vram()[a | 1] as u16;
        decode_cell(word)
    }

    /// The decoded nametable grid for a plane (design §4 `plane_decoded`). `rect = None` returns the whole
    /// plane row-major (`rows × cols` cells); a `rect` returns just that cell sub-region (also row-major).
    /// Pure introspection — recomputed on demand, never stored.
    pub fn plane_decoded(&self, plane: Plane, rect: Option<CellRect>) -> Vec<Cell> {
        let (base, cols, rows) = match plane {
            Plane::A => {
                let (w, h) = plane_size(self.regs()[0x10]);
                (self.plane_a_base(), w, h)
            }
            Plane::B => {
                let (w, h) = plane_size(self.regs()[0x10]);
                (self.plane_b_base(), w, h)
            }
            // The window map is stride-wide (64/32) and 32 rows tall (it covers the V28/V30 display).
            Plane::Window => (self.window_base(), self.window_stride(), 32),
        };
        let r = rect.unwrap_or(CellRect {
            col: 0,
            row: 0,
            cols,
            rows,
        });
        let mut out = Vec::with_capacity(r.cols as usize * r.rows as usize);
        for row in r.row..r.row + r.rows {
            for col in r.col..r.col + r.cols {
                // Wrap within the plane so an over-range rect still reads meaningful cells.
                out.push(self.nametable_cell(base, cols, col % cols, row % rows));
            }
        }
        out
    }

    // --- Scanline rendering (design §3 steps 1–3; recon RR2/RR4/RR5/RR6/R8/RR7) ---------------------------

    /// The backdrop / background CRAM index (recon RR4): reg $07 bits 5–0 (bits 5–4 palette, 3–0 colour).
    fn backdrop_index(&self) -> u8 {
        self.regs()[0x07] & 0x3F
    }

    /// Fetch the 4-bit colour index of pixel (`px`, `py`) within tile `tile` (recon RR2): a tile is 32 bytes
    /// (8 rows × 4 bytes), each byte two pixels, high nibble = left. `px`/`py` are 0..=7 (flips are applied
    /// by the caller). Pure VRAM read.
    fn tile_nibble(&self, tile: u16, px: u8, py: u8) -> u8 {
        let byte = self.vram()
            [(tile as usize * 32 + py as usize * 4 + (px as usize >> 1)) & (VRAM_SIZE - 1)];
        if px & 1 == 0 {
            byte >> 4
        } else {
            byte & 0x0F
        }
    }

    /// Read VSRAM word entry `idx` (recon RR6): big-endian, wrapped into the 80-byte region.
    fn vsram_word(&self, idx: usize) -> u16 {
        let b = (idx * 2) % self.vsram().len();
        ((self.vsram()[b] as u16) << 8) | self.vsram()[b | 1] as u16
    }

    /// The (base, width-cells, height-cells) geometry of a scrollable plane (recon RR3). Window is sized to
    /// the screen (stride × 32); it is not scrolled through this path.
    fn plane_geometry(&self, plane: Plane) -> (usize, u16, u16) {
        match plane {
            Plane::A => {
                let (w, h) = plane_size(self.regs()[0x10]);
                (self.plane_a_base(), w, h)
            }
            Plane::B => {
                let (w, h) = plane_size(self.regs()[0x10]);
                (self.plane_b_base(), w, h)
            }
            Plane::Window => (self.window_base(), self.window_stride(), 32),
        }
    }

    /// The horizontal scroll amount for `plane` on `line` (recon RR5): table base `(reg $0D & 0x3F) << 10`;
    /// mode from reg $0B bits 1–0 → byte offset `{00:0, 01:(line&7)*4, 10:(line&!7)*4, 11:line*4}`; Scroll A
    /// at `+0`, Scroll B at `+2`; masked to the 10-bit field.
    fn plane_hscroll(&self, plane: Plane, line: u16) -> u16 {
        let base = ((self.regs()[0x0D] & 0x3F) as usize) << 10;
        let line_off = match self.regs()[0x0B] & 0x03 {
            0 => 0,
            1 => ((line & 0x0007) as usize) * 4,
            2 => ((line & 0xFFF8) as usize) * 4,
            _ => (line as usize) * 4,
        };
        let plane_off = if plane == Plane::B { 2 } else { 0 };
        let a = (base + line_off + plane_off) & (VRAM_SIZE - 1);
        (((self.vram()[a] as u16) << 8) | self.vram()[a | 1] as u16) & 0x03FF
    }

    /// The vertical scroll amount for `plane` at screen pixel `x` on `line` (recon RR6 + R8): full mode →
    /// VSRAM word 0 (A) / 1 (B); 2-cell mode → per-16px-column word `(x/16)*2 (+1 for B)`; the R8 leftmost
    /// partial-column quirk (`hscroll & 15 != 0`) → `VSRAM[$4C] & VSRAM[$4E]` (H40) / 0 (H32), same value
    /// both planes (interim extent = the leftmost 16-px column — recon R8, confirm by golden diff push 5).
    fn plane_vscroll(&self, plane: Plane, x: usize, h40: bool, hscroll: u16) -> u16 {
        if self.regs()[0x0B] & 0x04 == 0 {
            self.vsram_word(if plane == Plane::B { 1 } else { 0 })
        } else if hscroll & 0x0F != 0 && x < 16 {
            // R8 partial left column: shared value both planes.
            if h40 {
                self.vsram_word(38) & self.vsram_word(39) // VSRAM $4C (col-19 A) & $4E (col-19 B)
            } else {
                0
            }
        } else {
            self.vsram_word((x / 16) * 2 + if plane == Plane::B { 1 } else { 0 })
        }
    }

    /// Fetch the plane pixel covering screen (`x`, `line`) (recon RR5/RR6 sign conventions: increasing
    /// hscroll ⇒ plane right, `plane_x = x − hscroll`; increasing vscroll ⇒ plane up, `plane_y = line +
    /// vscroll`; both wrap modulo the plane's power-of-two pixel size). Tile pixel via RR1/RR2 with flips.
    fn plane_pixel(&self, plane: Plane, line: u16, x: usize, h40: bool) -> PlanePixel {
        let (base, w_cells, h_cells) = self.plane_geometry(plane);
        let plane_w = w_cells as usize * 8;
        let plane_h = h_cells as usize * 8;
        let hscroll = self.plane_hscroll(plane, line);
        let vscroll = self.plane_vscroll(plane, x, h40, hscroll);
        let plane_x = x.wrapping_sub(hscroll as usize) & (plane_w - 1);
        let plane_y = (line as usize + vscroll as usize) & (plane_h - 1);
        let cell = self.nametable_cell(base, w_cells, (plane_x / 8) as u16, (plane_y / 8) as u16);
        let mut tpx = (plane_x & 7) as u8;
        let mut tpy = (plane_y & 7) as u8;
        if cell.hflip {
            tpx ^= 7;
        }
        if cell.vflip {
            tpy ^= 7;
        }
        PlanePixel {
            nibble: self.tile_nibble(cell.tile, tpx, tpy),
            palette: cell.palette,
            priority: cell.priority,
            tile: cell.tile,
        }
    }

    /// Decode one CRAM index (0..=63) to RGB at the fixed integer ramp — the same layout/ramp as
    /// `Vdp::cram_decoded` (guarded by `cram_rgb_matches_cram_decoded`).
    fn cram_rgb(&self, index: u8) -> (u8, u8, u8) {
        let i = (index as usize & 0x3F) * 2;
        let word = ((self.cram()[i] as u16) << 8) | self.cram()[i + 1] as u16;
        (
            ramp3(((word >> 1) & 0x07) as u8),
            ramp3(((word >> 5) & 0x07) as u8),
            ramp3(((word >> 9) & 0x07) as u8),
        )
    }

    /// Resolve one scanline to per-pixel [`PixelResolution`] — the single source both `render_line` and (in a
    /// later slice) `render_line_report` derive from (design §1: attribution is the render). This slice
    /// composites backdrop + plane B; plane A + window join in the next slice.
    fn resolve_line(&self, line: u16) -> Vec<PixelResolution> {
        let h40 = self.render_h40();
        let width = if h40 { 320 } else { 256 };
        let backdrop = self.backdrop_index();
        let mut out: Vec<PixelResolution> = (0..width)
            .map(|x| PixelResolution {
                x: x as u16,
                layer: Layer::Backdrop,
                cram_index: backdrop,
                tile: 0,
                palette: 0,
                priority: false,
            })
            .collect();
        // Display disabled (reg $01 bit 6 clear, RR4): the active area is the backdrop only.
        if self.regs()[0x01] & 0x40 == 0 {
            return out;
        }
        for (x, px) in out.iter_mut().enumerate() {
            let b = self.plane_pixel(Plane::B, line, x, h40);
            if b.opaque() {
                *px = PixelResolution {
                    x: x as u16,
                    layer: Layer::PlaneB,
                    cram_index: b.cram_index(),
                    tile: b.tile,
                    palette: b.palette,
                    priority: b.priority,
                };
            }
        }
        out
    }

    /// Render one scanline to RGB (design §3): each pixel is `resolve_line`'s winning CRAM index decoded at
    /// the fixed ramp. Length = the active width (256 H32 / 320 H40). Pure function of latched state + line.
    pub fn render_line(&self, line: u16) -> Vec<(u8, u8, u8)> {
        self.resolve_line(line)
            .iter()
            .map(|p| self.cram_rgb(p.cram_index))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    fn fresh() -> Vdp {
        Vdp::power_on(&mut SplitMix64::new(1))
    }

    /// Set VDP register `r` to `val` via a real `$8xxx` control-port register write (keeps `vdp.rs` at a
    /// zero diff — the renderer never needs a mutable register accessor).
    fn set_reg(v: &mut Vdp, r: u8, val: u8) {
        v.control_write(0x8000 | ((r as u16) << 8) | val as u16, 0);
    }

    /// Emit the two control words that arm a data-port write of `code` at VRAM/CRAM/VSRAM byte `addr`.
    fn setup_write(v: &mut Vdp, code: u8, addr: u16) {
        let w1 = (((code & 0x03) as u16) << 14) | (addr & 0x3FFF);
        let w2 = ((((code >> 2) & 0x0F) as u16) << 4) | (addr >> 14);
        v.control_write(w1, 0);
        v.control_write(w2, 0);
    }

    /// Write CRAM colour word `word` at entry `index` (0..=63) through the data port.
    fn write_cram(v: &mut Vdp, index: usize, word: u16) {
        setup_write(v, 0x03, (index * 2) as u16);
        v.data_write(word);
    }

    /// Write VSRAM scroll word `word` at word entry `word_index` through the data port.
    fn write_vsram(v: &mut Vdp, word_index: usize, word: u16) {
        setup_write(v, 0x05, (word_index * 2) as u16);
        v.data_write(word);
    }

    /// Fill tile `tile` with the solid 4-bit colour `nibble` (all 64 pixels the same).
    fn fill_tile(v: &mut Vdp, tile: usize, nibble: u8) {
        let byte = (nibble << 4) | nibble;
        for i in 0..32 {
            v.vram_mut()[tile * 32 + i] = byte;
        }
    }

    /// Write a raw nametable cell word at VRAM byte `addr` (big-endian).
    fn put_cell(v: &mut Vdp, addr: usize, word: u16) {
        v.vram_mut()[addr] = (word >> 8) as u8;
        v.vram_mut()[addr + 1] = (word & 0xFF) as u8;
    }

    /// A plane-B render fixture: cleared VRAM, display on, plane B at `$E000`, 32×32, h-scroll table at
    /// `$8000`, full h + full v scroll, a solid red tile 1 + CRAM entry 1 = red. `h40` selects the mode.
    fn pb_fixture(h40: bool) -> Vdp {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x01, 0x40); // display enable (reg 1 bit 6)
        if h40 {
            set_reg(&mut v, 0x0C, 0x81); // H40
        }
        set_reg(&mut v, 0x04, 0x07); // plane B base 0xE000
        set_reg(&mut v, 0x10, 0x00); // 32x32
        set_reg(&mut v, 0x0D, 0x20); // h-scroll table base 0x8000
        set_reg(&mut v, 0x0B, 0x00); // full h + full v scroll
        fill_tile(&mut v, 1, 1);
        write_cram(&mut v, 1, 0x000E); // entry 1 = red (R=7)
        v
    }

    // --- RR1: nametable cell decode -----------------------------------------------------------------------

    #[test]
    fn decode_cell_splits_every_field() {
        // priority=1, palette=2, vflip=1, hflip=0, tile=0x123:
        // 0x8000 | (2<<13)=0x4000 | 0x1000 | 0x123 = 0xD123.
        let c = decode_cell(0xD123);
        assert_eq!(c.tile, 0x123);
        assert_eq!(c.palette, 2);
        assert!(c.vflip);
        assert!(!c.hflip);
        assert!(c.priority);
        // hflip only, palette 0, tile 0x7FF:
        let c = decode_cell(0x0800 | 0x07FF);
        assert!(c.hflip && !c.vflip && !c.priority);
        assert_eq!(c.palette, 0);
        assert_eq!(c.tile, 0x7FF);
    }

    // --- RR3: plane geometry ------------------------------------------------------------------------------

    #[test]
    fn plane_size_table() {
        assert_eq!(plane_size(0x00), (32, 32));
        assert_eq!(plane_size(0x01), (64, 32)); // HSZ=1
        assert_eq!(plane_size(0x03), (128, 32)); // HSZ=3
        assert_eq!(plane_size(0x10), (32, 64)); // VSZ=1
        assert_eq!(plane_size(0x11), (64, 64)); // 64x64
        assert_eq!(plane_size(0x30), (32, 128)); // VSZ=3
        assert_eq!(plane_size(0x02), (64, 32)); // invalid HSZ=2 clamped to 64
    }

    #[test]
    fn plane_bases_match_stock_sonic_values() {
        let mut v = fresh();
        set_reg(&mut v, 0x02, 0x30); // Plane A → 0xC000
        set_reg(&mut v, 0x04, 0x07); // Plane B → 0xE000
        assert_eq!(v.plane_a_base(), 0xC000);
        assert_eq!(v.plane_b_base(), 0xE000);
    }

    #[test]
    fn window_base_masks_wd11_in_h40() {
        let mut v = fresh();
        set_reg(&mut v, 0x03, 0x3E); // WD15-11 all set
        assert_eq!(v.window_base(), 0x3E << 10, "H32 keeps WD11");
        set_reg(&mut v, 0x0C, 0x81); // H40
        assert_eq!(v.window_base(), 0x3C << 10, "H40 clears WD11 (bit 1)");
        assert_eq!(v.window_stride(), 64);
    }

    // --- design §4: plane_decoded -------------------------------------------------------------------------

    #[test]
    fn plane_decoded_reads_the_right_words() {
        let mut v = fresh();
        set_reg(&mut v, 0x02, 0x30); // Plane A base 0xC000
        set_reg(&mut v, 0x10, 0x00); // 32x32
                                     // Cell (col=1,row=2): addr = base + (2*32+1)*2 = 0xC000 + 130 = 0xC082.
        let addr = 0xC000 + (2 * 32 + 1) * 2;
        v.vram_mut()[addr] = 0xC1; // 0xC123 → tile 0x123, pal 2, pri 1
        v.vram_mut()[addr + 1] = 0x23;
        let grid = v.plane_decoded(Plane::A, None);
        assert_eq!(grid.len(), 32 * 32);
        let cell = grid[2 * 32 + 1];
        assert_eq!(cell.tile, 0x123);
        assert_eq!(cell.palette, 2);
        assert!(cell.priority);
    }

    #[test]
    fn plane_decoded_rect_is_a_subgrid() {
        let mut v = fresh();
        set_reg(&mut v, 0x04, 0x07); // Plane B base 0xE000
        set_reg(&mut v, 0x10, 0x00); // 32x32
        let base = 0xE000;
        // Seed a 2x2 block at (col 5, row 6) with distinguishable tiles.
        for (dr, dc, tile) in [(0, 0, 0x10u16), (0, 1, 0x11), (1, 0, 0x12), (1, 1, 0x13)] {
            let addr = base + ((6 + dr) * 32 + (5 + dc)) * 2;
            v.vram_mut()[addr] = (tile >> 8) as u8;
            v.vram_mut()[addr + 1] = (tile & 0xFF) as u8;
        }
        let rect = CellRect {
            col: 5,
            row: 6,
            cols: 2,
            rows: 2,
        };
        let g = v.plane_decoded(Plane::B, Some(rect));
        assert_eq!(g.len(), 4);
        assert_eq!(
            [g[0].tile, g[1].tile, g[2].tile, g[3].tile],
            [0x10, 0x11, 0x12, 0x13]
        );
    }

    // --- RR4/RR5/RR6/R8/RR2: backdrop + plane B rendering -------------------------------------------------

    #[test]
    fn cram_rgb_matches_cram_decoded() {
        // Guard: the renderer's per-entry decode must match Vdp::cram_decoded's ramp exactly (no drift).
        let mut v = fresh();
        write_cram(&mut v, 5, 0x0ECA);
        write_cram(&mut v, 63, 0x000E);
        let dec = v.cram_decoded();
        for (i, want) in dec.iter().enumerate() {
            assert_eq!(v.cram_rgb(i as u8), *want, "entry {i}");
        }
    }

    #[test]
    fn plane_b_renders_a_tile_row_exactly() {
        // Tile 2 row 0 = 0x12 repeated → pixels 1,2,1,2,… ; verify the exact RGB run (RR1/RR2).
        let mut v = pb_fixture(false);
        for i in 0..4 {
            v.vram_mut()[2 * 32 + i] = 0x12;
        }
        write_cram(&mut v, 2, 0x0E00); // entry 2 = blue
        put_cell(&mut v, 0xE000, 0x0002); // cell (0,0) = tile 2
        let l = v.render_line(0);
        assert_eq!(l[0], v.cram_rgb(1), "pixel 0 = colour 1");
        assert_eq!(l[1], v.cram_rgb(2), "pixel 1 = colour 2");
        assert_eq!(l[2], v.cram_rgb(1));
    }

    #[test]
    fn hscroll_full_shifts_plane_b_right() {
        // RR5 sign: increasing hscroll moves the plane right (a pixel run relocates rightward).
        let mut v = pb_fixture(false);
        put_cell(&mut v, 0xE004, 0x0001); // tile 1 at cell (col 2, row 0) → screen x 16..23
        let red = v.cram_rgb(1);
        let bg = v.cram_rgb(0);
        let l = v.render_line(0);
        assert_eq!(l[16], red, "hscroll 0: tile at x=16");
        assert_eq!(l[15], bg);
        put_cell(&mut v, 0x8002, 8); // Scroll B (+2) = 8, full mode
        let l = v.render_line(0);
        assert_eq!(l[24], red, "hscroll 8 shifts the plane right by 8");
        assert_eq!(l[16], bg, "old position now backdrop");
    }

    #[test]
    fn hscroll_line_mode_is_per_line() {
        // RR5 line mode: each scanline reads its own table entry.
        let mut v = pb_fixture(false);
        set_reg(&mut v, 0x0B, 0x03); // line h-scroll
        put_cell(&mut v, 0xE004, 0x0001); // cell (2,0), covers lines 0..7
        put_cell(&mut v, 0x8002, 0); // line 0 Scroll B = 0
        put_cell(&mut v, 0x8006, 8); // line 1 Scroll B = 8 (offset line*4 + 2)
        let red = v.cram_rgb(1);
        assert_eq!(v.render_line(0)[16], red);
        assert_eq!(
            v.render_line(1)[24],
            red,
            "line 1 shifted by its own scroll"
        );
        assert_ne!(v.render_line(1)[16], red);
    }

    #[test]
    fn vscroll_full_shifts_plane_b_up() {
        // RR6 sign: increasing vscroll moves the plane up (content appears at a lower line number).
        let mut v = pb_fixture(false);
        put_cell(&mut v, 0xE000 + (2 * 32) * 2, 0x0001); // tile 1 at cell (col 0, row 2) → plane_y 16..23
        let red = v.cram_rgb(1);
        assert_eq!(v.render_line(16)[0], red, "vscroll 0: red at line 16");
        write_vsram(&mut v, 1, 8); // plane B vscroll = 8
        assert_eq!(
            v.render_line(8)[0],
            red,
            "vscroll 8 shifts plane up to line 8"
        );
        assert_ne!(v.render_line(16)[0], red);
    }

    #[test]
    fn vscroll_two_cell_is_per_column() {
        // RR6 2-cell: column 1 (screen x 16..31) uses VSRAM word 3, independent of column 0's word 1.
        let mut v = pb_fixture(false);
        set_reg(&mut v, 0x0B, 0x04); // full h, 2-cell v
        put_cell(&mut v, 0xE000 + (2 * 32 + 2) * 2, 0x0001); // tile 1 at cell (col 2, row 2)
        write_vsram(&mut v, 1, 0); // column 0 B vscroll = 0
        write_vsram(&mut v, 3, 16); // column 1 B vscroll = 16
        let red = v.cram_rgb(1);
        let bg = v.cram_rgb(0);
        let l = v.render_line(0);
        // x=16 (col 1): plane_y = 0+16 = 16, plane_x = 16 → cell (2,2) → red.
        assert_eq!(l[16], red, "column 1 scrolled by its own word");
        assert_eq!(l[0], bg, "column 0 (word 1 = 0) shows backdrop");
    }

    #[test]
    fn r8_leftmost_column_h40_uses_and_of_last_two_vsram() {
        // R8 (H40): hscroll % 16 != 0 → leftmost column v-scroll = VSRAM[$4C] & VSRAM[$4E].
        let mut v = pb_fixture(true); // H40
        set_reg(&mut v, 0x0B, 0x04); // full h, 2-cell v
        for col in 0..32 {
            put_cell(&mut v, 0xE000 + (32 + col) * 2, 0x0001); // fill plane row 1 red → plane_y 8..15
        }
        put_cell(&mut v, 0x8002, 4); // plane B hscroll = 4 (→ hscroll & 15 != 0)
        write_vsram(&mut v, 1, 0); // normal col-0 word (must NOT be used) = 0
        write_vsram(&mut v, 38, 8); // VSRAM $4C (col-19 A)
        write_vsram(&mut v, 39, 8); // VSRAM $4E (col-19 B); AND = 8
        write_vsram(&mut v, 3, 0); // col 1 word = 0
        let red = v.cram_rgb(1);
        let bg = v.cram_rgb(0);
        let l = v.render_line(0);
        assert_eq!(l[0], red, "leftmost col vscroll = 8 → row 1 (red)");
        assert_eq!(l[16], bg, "col 1 vscroll = 0 → row 0 (empty)");
    }

    #[test]
    fn r8_leftmost_column_h32_is_fixed_zero() {
        // R8 (H32): the partial column cannot v-scroll — fixed 0, even though word 1 and the AND are non-zero.
        let mut v = pb_fixture(false); // H32
        set_reg(&mut v, 0x0B, 0x04); // full h, 2-cell v
        for col in 0..32 {
            put_cell(&mut v, 0xE000 + (32 + col) * 2, 0x0001); // plane row 1 red
        }
        put_cell(&mut v, 0x8002, 4); // hscroll & 15 != 0
        write_vsram(&mut v, 1, 8); // normal word (would show row 1 if used)
        write_vsram(&mut v, 38, 8);
        write_vsram(&mut v, 39, 8); // AND = 8 (would show row 1 if used)
        let bg = v.cram_rgb(0);
        assert_eq!(
            v.render_line(0)[0],
            bg,
            "H32 leftmost col is fixed 0 → row 0 (empty), not word 1 or the AND"
        );
    }

    #[test]
    fn display_disabled_is_backdrop_only() {
        let mut v = pb_fixture(false);
        put_cell(&mut v, 0xE004, 0x0001);
        assert_eq!(v.render_line(0)[16], v.cram_rgb(1), "enabled: red");
        set_reg(&mut v, 0x01, 0x00); // display off
        assert_eq!(v.render_line(0)[16], v.cram_rgb(0), "disabled: backdrop");
    }

    #[test]
    fn render_line_width_tracks_the_mode() {
        assert_eq!(pb_fixture(false).render_line(0).len(), 256, "H32");
        assert_eq!(pb_fixture(true).render_line(0).len(), 320, "H40");
    }
}
