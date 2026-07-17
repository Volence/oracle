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
use crate::vdp::{DmaRecord, Vdp};

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

/// A decoded sprite attribute-table entry (design §4 `sprites_decoded`, recon R5 / RR8). Y / size / link
/// come from the **SAT cache**; X / tile / attributes from **VRAM at the current reg-5 base** — so
/// `cache_divergence` exposes the stale-cache state (the cached Y/size/link disagree with VRAM at the new
/// base after a reg-5 change without a rewrite — the Castlevania Bloodlines mixing).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpriteDecoded {
    /// SAT index 0..=79 (the slot, not the link-walk position).
    pub index: u8,
    /// Screen Y = `(Yfield & 0x3FF) − 128` (cached).
    pub y: i16,
    /// Screen X = `(Xfield & 0x1FF) − 128` (VRAM at the current base).
    pub x: i16,
    /// Width in cells, 1..=4 (cached size bits 3–2 + 1).
    pub width_cells: u8,
    /// Height in cells, 1..=4 (cached size bits 1–0 + 1).
    pub height_cells: u8,
    /// Link — the next sprite index (cached, bits 6–0).
    pub link: u8,
    /// Base tile index (VRAM attribute word, recon RR1/RR8).
    pub tile: u16,
    /// Palette line 0–3 (VRAM attribute word).
    pub palette: u8,
    /// Horizontal flip (VRAM attribute word).
    pub hflip: bool,
    /// Vertical flip (VRAM attribute word).
    pub vflip: bool,
    /// Priority bit (VRAM attribute word; decoded + reported, the ordering it drives is push 5).
    pub priority: bool,
    /// The cached Y/size/link disagree with VRAM at the current reg-5 base — the stale-cache state made
    /// visible (recon R5). False when the cache and VRAM are coherent.
    pub cache_divergence: bool,
}

/// Which layer produced a resolved screen pixel (recon RR7). The `Sprite` variant carries the winning SAT
/// index; the shadow/highlight operators are push 5.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Backdrop,
    PlaneB,
    PlaneA,
    Window,
    /// A sprite pixel; the SAT index of the winning sprite (push 4).
    Sprite(u8),
}

/// The per-pixel shadow/highlight state (recon R11). `Normal` is the plain intensity ramp; the two enabled
/// S/H modes shift the winning pixel's intensity (never its identity). S/H disabled ⇒ every pixel `Normal`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PixelState {
    /// Half-intensity (Min→½Max ramp).
    Shadow,
    /// Full intensity (Min→Max ramp) — identical to no shadow/highlight.
    #[default]
    Normal,
    /// Upper-half intensity (½Max→Max ramp).
    Highlight,
}

/// One resolved screen pixel + its provenance. Attribution **is** the render computation (design §1): the
/// pipeline produces this directly, so `render_line` and `render_line_report` cannot drift.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PixelResolution {
    /// Screen x (0-based).
    pub x: u16,
    /// The winning layer (recon RR9 priority resolution).
    pub layer: Layer,
    /// The winning CRAM index (0..=63): `palette * 16 + nibble`, or `reg $07 & 0x3F` for the backdrop.
    pub cram_index: u8,
    /// The winning cell's tile index (0 for the backdrop).
    pub tile: u16,
    /// The winning cell's palette line (0 for the backdrop).
    pub palette: u8,
    /// The winning cell's priority bit (recon RR9: it selects the winner within its tier).
    pub priority: bool,
    /// The resolved shadow/highlight state applied to this pixel's intensity (recon R11).
    pub state: PixelState,
}

/// Why a candidate layer did not display at a dot (design §4 losing-candidate reasons).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CandidateVerdict {
    /// This layer is the displayed winner.
    Won,
    /// Opaque here, but a higher-priority layer (RR9) won.
    LostToPriority,
    /// No opaque pixel at this dot (colour nibble 0) — it contributes nothing to the display.
    Transparent,
    /// An opaque sprite **operator** (palette 3, nibble 14/15): it outranks the winner but is not displayed —
    /// it shifted the winner's shadow/highlight state instead (recon R11.3).
    Operator,
}

/// One candidate layer at a dot, in RR9 rank order, with why it won or lost (design §4 losing-candidate list).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Candidate {
    /// The layer.
    pub layer: Layer,
    /// Did this layer have an opaque pixel at the dot.
    pub opaque: bool,
    /// The layer's priority bit.
    pub priority: bool,
    /// The CRAM index this layer would have shown.
    pub cram_index: u8,
    /// Why it won or lost.
    pub verdict: CandidateVerdict,
}

/// The full per-dot resolution (design §4 `pixel_attribution`): the winning layer + its CRAM/RGB + resolved
/// shadow/highlight state, the winning plane/window cell (if any), and the RR9-ordered candidate list with
/// verdicts. Derived from the same `resolve_line` `render_line` maps to RGB — attribution **is** the render.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PixelAttribution {
    /// Screen x.
    pub x: u16,
    /// Screen y (line).
    pub y: u16,
    /// The winning layer.
    pub winner: Layer,
    /// The winner's CRAM index (post-operator: the displayed pixel's index).
    pub cram_index: u8,
    /// The winner's CRAM index decoded at the resolved shadow/highlight state — equals `render_line(y)[x]`.
    pub rgb: (u8, u8, u8),
    /// The resolved shadow/highlight state.
    pub state: PixelState,
    /// The winning plane/window nametable cell (tile/palette/flips/priority); `None` for sprite/backdrop.
    pub cell: Option<Cell>,
    /// The candidate layers in RR9 rank order with verdicts (design §4).
    pub candidates: Vec<Candidate>,
}

/// The window's horizontal span on one line (design §4 "window span"): screen x `[start_x, end_x)`.
/// `full_line` marks a line the *vertical* window band covers entirely (recon RR4 / Sega manual §J).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WindowSpan {
    /// First window pixel (inclusive).
    pub start_x: u16,
    /// One past the last window pixel (exclusive).
    pub end_x: u16,
    /// The whole line is window (the vertical window band covers it).
    pub full_line: bool,
}

/// The effective vertical scroll of a plane on one line (design §4 "effective … v-scroll per plane",
/// post-mode-resolution): a single value in full mode, or one value per 16-px column in 2-cell mode
/// (including the R8 leftmost-column resolution).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VScroll {
    /// Full-screen vertical scroll (reg $0B bit 2 = 0).
    Full(u16),
    /// Per-16-px-column vertical scroll, left→right (reg $0B bit 2 = 1); 16 entries in H32 / 20 in H40.
    TwoCell(Vec<u16>),
}

/// The effective post-mode-resolution scroll of one plane on one line (design §4).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlaneScroll {
    /// Effective horizontal scroll (recon RR5).
    pub hscroll: u16,
    /// Effective vertical scroll (recon RR6/R8).
    pub vscroll: VScroll,
}

/// One walked sprite's evaluation outcome on a line (design §4 `render_line_report`, recon R10 / RR8) — the
/// "which sprites dropped on line N and why" differentiator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpriteOutcome {
    /// On-line, within the per-line sprite + pixel limits, output not masked — drawn.
    Rendered,
    /// Parsed but this line is outside the sprite's Y span (design "offscreen"); consumes no budget.
    OffLine,
    /// On-line but beyond the per-line sprite count (20 H40 / 16 H32) — the brief's "limit".
    DroppedLineLimit,
    /// On-line but the per-line pixel budget (320 H40 / 256 H32) was already exhausted — dot overflow.
    DroppedPixelBudget,
    /// On-line, in budget, but R10 x=0 masking suppressed its pixel output — the brief's "masking"
    /// (produced at render time when X is fetched, push-4 slice 3).
    Masked,
}

/// Why the sprite link-walk ended (recon RR8 — the brief's "link-cut": sprites past this are unreachable).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpriteWalkEnd {
    /// A link field was 0 (points back at sprite 0) — the normal end of the list.
    LinkZero,
    /// The hardware maximum of 80 (H40) / 64 (H32) parsed sprites was reached (or a link ran out of range).
    MaxCount,
}

/// One walked sprite's evaluation record (design §4), in link-walk order. Y / size / link come from the SAT
/// cache; X from VRAM at the current base (recon R5 / RR8).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpriteEval {
    /// SAT index of this sprite (the slot).
    pub index: u8,
    /// Screen Y = `(Yfield & 0x3FF) − 128` (cached).
    pub y: i16,
    /// Screen X = `(Xfield & 0x1FF) − 128` (VRAM at the current base).
    pub x: i16,
    /// Width in cells, 1..=4.
    pub width_cells: u8,
    /// Height in cells, 1..=4.
    pub height_cells: u8,
    /// This sprite's link field (next index).
    pub link: u8,
    /// Why this sprite did or did not draw on the line.
    pub outcome: SpriteOutcome,
}

/// The result of the sprite pipeline for one line — the walk, the composited sprite line buffer, and the
/// status flags. Shared by `render_line` (overlays the buffer), `render_line_report` (the evaluation list +
/// status), and `render_scanline` (commits the latches), so attribution is the render (design §1).
struct SpriteLine {
    /// Every walked sprite, in link-walk order, with its outcome (including R10 `Masked`).
    sprites: Vec<SpriteEval>,
    /// How the walk terminated (link-cut vs the parse cap).
    walk_end: SpriteWalkEnd,
    /// Any on-line sprite was dropped by the per-line sprite count or the pixel budget (status bit 6).
    overflow: bool,
    /// The per-line pixel (dot) budget was exhausted — the R10 masking carry into the next line.
    dot_overflow: bool,
    /// Two opaque sprite pixels overlapped on the line (status bit 5).
    collision: bool,
    /// The composited opaque sprite pixels, indexed by screen x (first-come-wins in link order).
    buffer: Vec<Option<SpritePixel>>,
}

/// One composited sprite pixel — the CRAM index + provenance for `Layer::Sprite`.
#[derive(Clone, Copy)]
struct SpritePixel {
    cram_index: u8,
    index: u8,
    palette: u8,
    priority: bool,
    tile: u16,
    /// The raw colour nibble (1..=15) — needed for the R11 colour-14-never-shadowed quirk and operator
    /// detection (palette 3 nibble 14 = highlight-op / 15 = shadow-op).
    nibble: u8,
}

impl SpritePixel {
    /// The shadow/highlight operator this pixel is, if any (recon R11.3): palette line 3, colour nibble 14 =
    /// highlight operator, nibble 15 = shadow operator. Operators are not displayed; they shift the underlying
    /// pixel's S/H state. `None` for every ordinary sprite pixel.
    fn operator(&self) -> Option<PixelState> {
        match (self.palette, self.nibble) {
            (3, 14) => Some(PixelState::Highlight),
            (3, 15) => Some(PixelState::Shadow),
            _ => None,
        }
    }
}

/// Combine an underlying S/H `base` state with an `op` operator (recon R11.3): levels Shadow=−1, Normal=0,
/// Highlight=+1; the operator adds ±1, clamped to `[−1, +1]`. Realizes normal+highlight = highlight,
/// normal+shadow = shadow, shadow+highlight = normal (undo), shadow+shadow = shadow (no double-shadow), and
/// the symmetric highlight+highlight = highlight, highlight+shadow = normal.
fn combine_operator(base: PixelState, op: PixelState) -> PixelState {
    let level = |s| match s {
        PixelState::Shadow => -1,
        PixelState::Normal => 0,
        PixelState::Highlight => 1,
    };
    match (level(base) + level(op)).clamp(-1, 1) {
        -1 => PixelState::Shadow,
        0 => PixelState::Normal,
        _ => PixelState::Highlight,
    }
}

/// A fully resolved line: the per-pixel composite plus the sprite-pipeline result. The single source
/// `render_line` / `render_line_report` / `render_scanline` all derive from (design §1: attribution is the
/// render).
struct ResolvedLine {
    pixels: Vec<PixelResolution>,
    sprite: SpriteLine,
}

/// The semantic line report (design §4 `render_line_report`): the latched inputs and per-pixel + per-sprite
/// resolution outcomes for one line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LineReport {
    /// The line number this report describes.
    pub line: u16,
    /// H40 (320 px) vs H32 (256 px).
    pub h40: bool,
    /// Display-enable (reg $01 bit 6): when false the active area is the backdrop only.
    pub display_enabled: bool,
    /// The backdrop CRAM index (reg $07 & 0x3F).
    pub backdrop: u8,
    /// Plane A's effective scroll.
    pub plane_a: PlaneScroll,
    /// Plane B's effective scroll.
    pub plane_b: PlaneScroll,
    /// The window's horizontal span on this line (if any).
    pub window: Option<WindowSpan>,
    /// The sprite evaluation list, in link-walk order (recon R10 / RR8) — each walked sprite + why it drew or
    /// dropped. Sprites unreachable past `sprite_walk_end` are link-cut (absent).
    pub sprites: Vec<SpriteEval>,
    /// Why the sprite link-walk ended (link-cut vs the parse cap).
    pub sprite_walk_end: SpriteWalkEnd,
    /// Sprite overflow for this line (status bit 6): a per-line sprite-count or pixel-budget drop occurred.
    pub sprite_overflow: bool,
    /// Sprite collision for this line (status bit 5): two opaque sprite pixels overlapped.
    pub sprite_collision: bool,
    /// Per-pixel resolution (length = the active width); the same computation `render_line` maps to RGB.
    pub pixels: Vec<PixelResolution>,
}

/// The per-frame introspection rollup (design §4 `frame_report`; recon R4). This push lands the **DMA
/// section** — the most recent transfer performed (source / dest / length / mode / target). The design's full
/// frame_report also rolls up dropped-sprites-per-line, overflow/collision lines, and HINT/VINT lines fired;
/// those are derivable from the per-line [`LineReport`]s and land with their own push (documented interim —
/// this push exists for DMA + FIFO).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FrameReport {
    /// The most recently completed DMA (recon R4), or `None` if none has run this session.
    pub dma: Option<DmaRecord>,
}

/// Per-line plane-A-slot inputs computed once by `resolve_line` (the window span + the R9 window-bug
/// predicate), passed to `a_slot_pixel` per dot.
#[derive(Clone, Copy)]
struct ASlotCtx {
    win: Option<WindowSpan>,
    r9: bool,
    boundary: usize,
    a_hscroll: u16,
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

/// Build a [`PixelResolution`] for screen `x` from a winning plane pixel `p` on layer `layer` (state `Normal`;
/// shadow/highlight is applied by `resolve_dot` in the R11 pass).
fn px_from(x: usize, layer: Layer, p: &PlanePixel) -> PixelResolution {
    PixelResolution {
        x: x as u16,
        layer,
        cram_index: p.cram_index(),
        tile: p.tile,
        palette: p.palette,
        priority: p.priority,
        state: PixelState::Normal,
    }
}

/// Build a backdrop [`PixelResolution`] for screen `x` (recon RR4/RR9 floor).
fn backdrop_px(x: usize, backdrop: u8) -> PixelResolution {
    PixelResolution {
        x: x as u16,
        layer: Layer::Backdrop,
        cram_index: backdrop,
        tile: 0,
        palette: 0,
        priority: false,
        state: PixelState::Normal,
    }
}

/// Build a sprite [`PixelResolution`] for screen `x` from a composited sprite pixel `sp` (state `Normal`).
fn sprite_px_res(x: usize, sp: &SpritePixel) -> PixelResolution {
    PixelResolution {
        x: x as u16,
        layer: Layer::Sprite(sp.index),
        cram_index: sp.cram_index,
        tile: sp.tile,
        palette: sp.palette,
        priority: sp.priority,
        state: PixelState::Normal,
    }
}

/// The shadow/highlight-aware intensity ramp (recon R11.5, integer — no floats). A 3-bit CRAM channel level
/// (`0..=7`) and a [`PixelState`] map through a shared `0..=14` quantization, `out = step × 255 / 14`:
/// `Shadow` uses steps `0..=7` (Min→½Max), `Normal` uses the even steps `0,2,…,14` (Min→Max — the plain
/// ramp), `Highlight` uses steps `7..=14` (½Max→Max). Exactly the pinned "normal Min→Max, shadow Min→½Max,
/// highlight ½Max→Max". Exact DAC calibration is the R11 deferred remainder.
fn intensity(level: u8, state: PixelState) -> u8 {
    let step = match state {
        PixelState::Shadow => level,        // 0..7   → Min..½Max
        PixelState::Normal => level * 2,    // 0..14  → Min..Max (the plain ramp)
        PixelState::Highlight => level + 7, // 7..14  → ½Max..Max
    };
    (step as u16 * 255 / 14) as u8
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

/// Per-line sprite limits `(max_sprites, max_pixels, parse_cap)` for the mode (recon R10 / RR8): H40 =
/// 20 / 320 / 80, H32 = 16 / 256 / 64.
fn sprite_limits(h40: bool) -> (usize, usize, usize) {
    if h40 {
        (20, 320, 80)
    } else {
        (16, 256, 64)
    }
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

    /// Read a big-endian word from VRAM at byte address `addr` (wrapped into the 64 KiB region).
    fn vram_word(&self, addr: usize) -> u16 {
        let a = addr & (VRAM_SIZE - 1);
        ((self.vram()[a] as u16) << 8) | self.vram()[(a + 1) & (VRAM_SIZE - 1)] as u16
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

    /// Decode all 80 SAT entries (design §4 `sprites_decoded`, recon R5 / RR8). Y/size/link come from the
    /// **SAT cache**; X/tile/attr from **VRAM at the current reg-5 base** — with a per-entry
    /// `cache_divergence` flag exposing the stale-cache state. Pure introspection — recomputed on demand.
    pub fn sprites_decoded(&self) -> Vec<SpriteDecoded> {
        let base = self.sat_base();
        let cache = self.sat_cache();
        (0..80)
            .map(|i| {
                // Cached half (Y + size/link), big-endian.
                let y_field = (((cache[i * 4] as u16) << 8) | cache[i * 4 + 1] as u16) & 0x03FF;
                let size = cache[i * 4 + 2];
                let link = cache[i * 4 + 3] & 0x7F;
                // Render-fetched half from VRAM at the current base.
                let slot = base + i * 8;
                let cell = decode_cell(self.vram_word(slot + 4));
                let x_field = self.vram_word(slot + 6) & 0x01FF;
                // Divergence: cached Y/size/link vs the VRAM bytes at the current base (recon R5 stale-cache).
                let v_y = self.vram_word(slot) & 0x03FF;
                let v_size = self.vram()[(slot + 2) & (VRAM_SIZE - 1)];
                let v_link = self.vram()[(slot + 3) & (VRAM_SIZE - 1)] & 0x7F;
                let cache_divergence = y_field != v_y || size != v_size || link != v_link;
                SpriteDecoded {
                    index: i as u8,
                    y: y_field as i16 - 128,
                    x: x_field as i16 - 128,
                    width_cells: (size >> 2 & 0x03) + 1,
                    height_cells: (size & 0x03) + 1,
                    link,
                    tile: cell.tile,
                    palette: cell.palette,
                    hflip: cell.hflip,
                    vflip: cell.vflip,
                    priority: cell.priority,
                    cache_divergence,
                }
            })
            .collect()
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

    /// The nametable cell + within-tile pixel covering screen (`x`, `line`) for `plane` (recon RR5/RR6 sign
    /// conventions: increasing hscroll ⇒ plane right, `plane_x = x − hscroll`; increasing vscroll ⇒ plane up,
    /// `plane_y = line + vscroll`; both wrap modulo the plane's power-of-two pixel size). Shared by
    /// `plane_pixel` (samples the pixel) and `winning_cell` (attribution needs the decoded cell incl. flips).
    fn plane_sample(&self, plane: Plane, line: u16, x: usize, h40: bool) -> (Cell, u8, u8) {
        let (base, w_cells, h_cells) = self.plane_geometry(plane);
        let plane_w = w_cells as usize * 8;
        let plane_h = h_cells as usize * 8;
        let hscroll = self.plane_hscroll(plane, line);
        let vscroll = self.plane_vscroll(plane, x, h40, hscroll);
        let plane_x = x.wrapping_sub(hscroll as usize) & (plane_w - 1);
        let plane_y = (line as usize + vscroll as usize) & (plane_h - 1);
        let cell = self.nametable_cell(base, w_cells, (plane_x / 8) as u16, (plane_y / 8) as u16);
        (cell, (plane_x & 7) as u8, (plane_y & 7) as u8)
    }

    /// Fetch the plane pixel covering screen (`x`, `line`) — the sampled cell pixel (RR1/RR2 with flips).
    fn plane_pixel(&self, plane: Plane, line: u16, x: usize, h40: bool) -> PlanePixel {
        let (cell, px, py) = self.plane_sample(plane, line, x, h40);
        self.cell_pixel(cell, px, py)
    }

    /// Decode one CRAM index (0..=63) to RGB at the given shadow/highlight `state` (recon R11.5). The 9-bit
    /// colour is three 3-bit channels (B<<9 | G<<5 | R<<1, big-endian); each is run through `intensity`.
    fn cram_rgb_state(&self, index: u8, state: PixelState) -> (u8, u8, u8) {
        let i = (index as usize & 0x3F) * 2;
        let word = ((self.cram()[i] as u16) << 8) | self.cram()[i + 1] as u16;
        (
            intensity(((word >> 1) & 0x07) as u8, state),
            intensity(((word >> 5) & 0x07) as u8, state),
            intensity(((word >> 9) & 0x07) as u8, state),
        )
    }

    /// Decode one CRAM index (0..=63) to RGB at `Normal` intensity — the same layout/ramp as
    /// `Vdp::cram_decoded` (guarded by `cram_rgb_matches_cram_decoded`). Test-only reference: the renderer
    /// uses `cram_rgb_state` so it never drops the shadow/highlight state.
    #[cfg(test)]
    fn cram_rgb(&self, index: u8) -> (u8, u8, u8) {
        self.cram_rgb_state(index, PixelState::Normal)
    }

    /// The window's horizontal span on `line` (recon RR4 / Sega manual §J union model): if the vertical
    /// window band covers the line (reg $12: DOWN=0 ⇒ `line < WVP*8`; DOWN=1 ⇒ `line ≥ WVP*8`) the whole
    /// line is window; otherwise the horizontal split (reg $11: RIGT=0 ⇒ cells `[0, WHP*2)`; RIGT=1 ⇒
    /// `[WHP*2, w)`). Returns `None` if the window does not appear on this line.
    fn window_span(&self, line: u16, width: usize) -> Option<WindowSpan> {
        let wvp = (self.regs()[0x12] & 0x1F) as usize * 8;
        let v_active = if self.regs()[0x12] & 0x80 != 0 {
            line as usize >= wvp
        } else {
            (line as usize) < wvp
        };
        if v_active {
            return Some(WindowSpan {
                start_x: 0,
                end_x: width as u16,
                full_line: true,
            });
        }
        let whp = (self.regs()[0x11] & 0x1F) as usize * 16;
        if self.regs()[0x11] & 0x80 != 0 {
            // Right window: [WHP*16, width).
            (whp < width).then_some(WindowSpan {
                start_x: whp as u16,
                end_x: width as u16,
                full_line: false,
            })
        } else {
            // Left window: [0, WHP*16).
            (whp > 0).then_some(WindowSpan {
                start_x: 0,
                end_x: whp as u16,
                full_line: false,
            })
        }
    }

    /// The window pixel at screen (`x`, `line`) — the window map does not scroll (`plane_x = x`,
    /// `plane_y = line`), base `(reg $03 & mask) << 10`, row stride 64/32 (recon RR3/RR4).
    fn window_pixel(&self, line: u16, x: usize) -> PlanePixel {
        let cell = self.nametable_cell(
            self.window_base(),
            self.window_stride(),
            (x / 8) as u16,
            line / 8,
        );
        self.cell_pixel(cell, (x & 7) as u8, (line & 7) as u8)
    }

    /// The R9 window-bug pixel (interim model, recon R9): the first 16 px of plane A right of a *left*
    /// window boundary reuse the window's last-column tile, sampled at plane A's fine-scroll offset. The
    /// exact sub-tile alignment is the recon R9 open remainder — confirm by the golden-frame differential
    /// (push 5); the pinned observable is that the *tile* is the window's last column, not plane A's.
    fn r9_reused_pixel(&self, line: u16, x: usize, boundary: usize, a_hscroll: u16) -> PlanePixel {
        let last_col = ((boundary / 8) as u16).saturating_sub(1);
        let cell =
            self.nametable_cell(self.window_base(), self.window_stride(), last_col, line / 8);
        let tpx = (((x - boundary) + (a_hscroll as usize & 7)) & 7) as u8;
        self.cell_pixel(cell, tpx, (line & 7) as u8)
    }

    /// Sample cell `cell` at within-tile pixel (`px`, `py`) applying its flips (recon RR1/RR2).
    fn cell_pixel(&self, cell: Cell, mut px: u8, mut py: u8) -> PlanePixel {
        if cell.hflip {
            px ^= 7;
        }
        if cell.vflip {
            py ^= 7;
        }
        PlanePixel {
            nibble: self.tile_nibble(cell.tile, px, py),
            palette: cell.palette,
            priority: cell.priority,
            tile: cell.tile,
        }
    }

    /// The plane-A-slot pixel + its layer at screen `x` on `line` (recon RR4/R9): the window pixel in the
    /// window span, else plane A — including the R9 window-bug reuse. Returned even when transparent (its
    /// priority bit feeds RR9 and the R11 shadow/highlight default state). `ctx` holds the per-line window
    /// inputs computed once by `resolve_line`.
    fn a_slot_pixel(&self, line: u16, x: usize, h40: bool, ctx: &ASlotCtx) -> (PlanePixel, Layer) {
        match ctx.win {
            Some(w) if x >= w.start_x as usize && x < w.end_x as usize => {
                (self.window_pixel(line, x), Layer::Window)
            }
            _ if ctx.r9 && x >= ctx.boundary && x < ctx.boundary + 16 => (
                self.r9_reused_pixel(line, x, ctx.boundary, ctx.a_hscroll),
                Layer::PlaneA,
            ),
            _ => (self.plane_pixel(Plane::A, line, x, h40), Layer::PlaneA),
        }
    }

    /// Select the RR9 priority winner at screen `x` from the flattened sprite pixel `s`, the plane-A-slot pixel
    /// `a` (on layer `a_layer`), and the plane-B pixel `b`, over the `backdrop` floor. Order (highest first):
    /// high-sprite > high-A > high-B > low-sprite > low-A > low-B > backdrop; only opaque pixels are
    /// candidates (transparent loses by transparency). State is `Normal` here (R11 is applied afterward).
    fn rr9_winner(
        &self,
        x: usize,
        backdrop: u8,
        s: Option<SpritePixel>,
        a: &PlanePixel,
        a_layer: Layer,
        b: &PlanePixel,
    ) -> PixelResolution {
        // High-priority tier.
        if let Some(sp) = s {
            if sp.priority {
                return sprite_px_res(x, &sp);
            }
        }
        if a.opaque() && a.priority {
            return px_from(x, a_layer, a);
        }
        if b.opaque() && b.priority {
            return px_from(x, Layer::PlaneB, b);
        }
        // Low-priority tier (sprite buffer pixels are always opaque).
        if let Some(sp) = s {
            return sprite_px_res(x, &sp);
        }
        if a.opaque() {
            return px_from(x, a_layer, a);
        }
        if b.opaque() {
            return px_from(x, Layer::PlaneB, b);
        }
        backdrop_px(x, backdrop)
    }

    /// The shadow/highlight state of one dot (recon R11), given the S/H enable, the RR9 `winner` layer, the
    /// plane-A-slot pixel `a`, the plane-B pixel `b`, and the flattened sprite pixel `s`. S/H disabled ⇒
    /// `Normal`. The **default** state is `Shadow` iff both the A-slot and B priority bits are 0 (transparent
    /// planes still contribute their tile's priority — the Bloodlines light-ray trick), else `Normal`; the
    /// backdrop and plane/window winners take the default. A **high-priority** sprite pixel is never shadowed;
    /// a **colour-14** sprite pixel (any palette) is never shadowed; a low-priority sprite takes the default.
    /// Sprite operators (palette 3, nibble 14/15) are handled in the operator pass, not here.
    fn sh_state(
        &self,
        sh: bool,
        winner: Layer,
        a: &PlanePixel,
        b: &PlanePixel,
        s: Option<SpritePixel>,
    ) -> PixelState {
        if !sh {
            return PixelState::Normal;
        }
        let default = if a.priority || b.priority {
            PixelState::Normal
        } else {
            PixelState::Shadow
        };
        match winner {
            Layer::Sprite(_) => match s {
                // High-priority or colour-14 sprite pixels are never shadowed (R11); a low-priority sprite
                // takes the default (shadowed only when both planes are low-priority).
                Some(sp) if sp.priority || sp.nibble == 14 => PixelState::Normal,
                _ => default,
            },
            _ => default,
        }
    }

    /// Resolve one dot fully (recon RR9 + R11): pick the RR9 winner, then apply shadow/highlight. A winning
    /// **sprite operator** (palette 3, nibble 14/15) is not displayed — the winner is recomputed *without* the
    /// sprite (planes + backdrop), and the operator shifts that underlying pixel's state one step (R11.3). An
    /// operator that loses RR9 (a high-priority plane over a low-priority operator) has no effect — it never
    /// becomes the winner. Ordinary winners take `sh_state`.
    #[allow(clippy::too_many_arguments)]
    fn resolve_dot(
        &self,
        sh: bool,
        x: usize,
        backdrop: u8,
        s: Option<SpritePixel>,
        a: &PlanePixel,
        a_layer: Layer,
        b: &PlanePixel,
    ) -> PixelResolution {
        let winner = self.rr9_winner(x, backdrop, s, a, a_layer, b);
        if sh {
            if let Layer::Sprite(_) = winner.layer {
                if let Some(op) = s.and_then(|sp| sp.operator()) {
                    // The operator wins the sprite slot: display the background beneath it, shifted.
                    let mut under = self.rr9_winner(x, backdrop, None, a, a_layer, b);
                    let base = self.sh_state(sh, under.layer, a, b, None);
                    under.state = combine_operator(base, op);
                    return under;
                }
            }
        }
        let mut px = winner;
        px.state = self.sh_state(sh, px.layer, a, b, s);
        px
    }

    /// Resolve one scanline — the single source `render_line` / `render_line_report` / `render_scanline` all
    /// derive from (design §1: attribution is the render). Each dot is resolved by RR9 priority ordering
    /// (high-sprite > high-A > high-B > low-sprite > low-A > low-B > backdrop) over the flattened sprite pixel,
    /// the plane-A-slot pixel (window/plane A with R9), and plane B. Shadow/highlight (R11) is a later pass.
    /// Returns the per-pixel composite + the sprite pipeline result (walk, status flags, buffer).
    fn resolve_line(&self, line: u16) -> ResolvedLine {
        let h40 = self.render_h40();
        let width = if h40 { 320 } else { 256 };
        let backdrop = self.backdrop_index();
        // Sprite evaluation is display-independent (a debugger asks "which sprites on line N" regardless of
        // display enable), so the walk always runs for the report; only compositing is gated on display.
        let sprite = self.sprite_line(line, h40, width);
        // Display disabled (reg $01 bit 6 clear, RR4): the active area is the backdrop only — no planes/sprites.
        if self.regs()[0x01] & 0x40 == 0 {
            let out = (0..width).map(|x| backdrop_px(x, backdrop)).collect();
            return ResolvedLine {
                pixels: out,
                sprite,
            };
        }
        // Per-line plane-A-slot inputs (the window span + the R9 window-bug predicate).
        let win = self.window_span(line, width);
        let a_hscroll = self.plane_hscroll(Plane::A, line);
        let boundary = win.map_or(0, |w| w.end_x as usize);
        let r9 = matches!(win, Some(w) if !w.full_line && w.start_x == 0) && a_hscroll & 0x0F != 0;
        let ctx = ASlotCtx {
            win,
            r9,
            boundary,
            a_hscroll,
        };
        // Shadow/highlight enable (reg $0C bit 3, recon R11).
        let sh = self.regs()[0x0C] & 0x08 != 0;
        let mut out: Vec<PixelResolution> = (0..width)
            .map(|x| {
                let b = self.plane_pixel(Plane::B, line, x, h40);
                let (a, a_layer) = self.a_slot_pixel(line, x, h40, &ctx);
                let s = sprite.buffer[x];
                self.resolve_dot(sh, x, backdrop, s, &a, a_layer, &b)
            })
            .collect();
        // Leftmost-column blank (reg $00 bit 5, RR4): force the leftmost 8 px to the backdrop (an output-stage
        // blank, after priority resolution).
        if self.regs()[0x00] & 0x20 != 0 {
            for (x, px) in out.iter_mut().enumerate().take(8.min(width)) {
                *px = backdrop_px(x, backdrop);
            }
        }
        ResolvedLine {
            pixels: out,
            sprite,
        }
    }

    /// Render one scanline to RGB (design §3): each pixel is `resolve_line`'s winning CRAM index decoded at
    /// the fixed ramp. Length = the active width (256 H32 / 320 H40). Pure function of latched state + line.
    pub fn render_line(&self, line: u16) -> Vec<(u8, u8, u8)> {
        self.resolve_line(line)
            .pixels
            .iter()
            .map(|p| self.cram_rgb_state(p.cram_index, p.state))
            .collect()
    }

    /// The effective post-mode-resolution scroll of `plane` on `line` (design §4 report field): full v-scroll,
    /// or one per-16-px-column value in 2-cell mode (including the R8 leftmost-column resolution).
    fn plane_scroll(&self, plane: Plane, line: u16, h40: bool, width: usize) -> PlaneScroll {
        let hscroll = self.plane_hscroll(plane, line);
        let vscroll = if self.regs()[0x0B] & 0x04 == 0 {
            VScroll::Full(self.plane_vscroll(plane, 0, h40, hscroll))
        } else {
            VScroll::TwoCell(
                (0..width / 16)
                    .map(|c| self.plane_vscroll(plane, c * 16, h40, hscroll))
                    .collect(),
            )
        };
        PlaneScroll { hscroll, vscroll }
    }

    /// The sprite pipeline for one line (recon R10 / RR8): the cache-only link-walk + per-line limits, then
    /// the render phase — fetch X + tile/attr from VRAM, apply R10 x=0 masking (seeded from the
    /// `sprite_dot_overflow_carry` field, read-only), draw each admitted sprite into the sprite line buffer
    /// (first-come-wins in link order), and detect collision. Reads **only the SAT cache** for Y/size/link
    /// (X/tile are VRAM at the current base). Pure — the carry is not written back here (that is
    /// [`Vdp::render_scanline`]). `width` is the active pixel width for the buffer.
    fn sprite_line(&self, line: u16, h40: bool, width: usize) -> SpriteLine {
        let (max_sprites, max_px, cap) = sprite_limits(h40);
        let cache = self.sat_cache();
        let base = self.sat_base();
        let mut sprites = Vec::new();
        let mut buffer: Vec<Option<SpritePixel>> = vec![None; width];
        let mut walk_end = SpriteWalkEnd::MaxCount; // exhausting the cap without a 0-link ends here
        let mut idx = 0usize; // the walk always starts at sprite 0
        let mut on_line_count = 0usize;
        let mut px_used = 0usize;
        let mut overflow = false;
        let mut dot_overflow = false;
        let mut collision = false;
        // R10 masking: `seen_nonzero` seeds from the previous line's dot-overflow carry (the first-on-line
        // mask); `masking_active` latches once an x=0 sprite masks, suppressing every later sprite this line.
        let mut seen_nonzero = self.sprite_dot_overflow_carry();
        let mut masking_active = false;
        for _ in 0..cap {
            if idx >= 80 {
                break; // an out-of-range link terminates the list (MaxCount)
            }
            let y_field = (((cache[idx * 4] as u16) << 8) | cache[idx * 4 + 1] as u16) & 0x03FF;
            let size = cache[idx * 4 + 2];
            let link = cache[idx * 4 + 3] & 0x7F;
            let w = (size >> 2 & 0x03) + 1;
            let h = (size & 0x03) + 1;
            let screen_y = y_field as i16 - 128;
            let on_line = (line as i16) >= screen_y && (line as i16) < screen_y + (h as i16) * 8;
            let x_field = self.vram_word(base + idx * 8 + 6) & 0x01FF;
            let outcome = if !on_line {
                SpriteOutcome::OffLine
            } else if on_line_count >= max_sprites {
                overflow = true;
                SpriteOutcome::DroppedLineLimit
            } else if px_used >= max_px {
                overflow = true;
                dot_overflow = true;
                SpriteOutcome::DroppedPixelBudget
            } else {
                // Admitted: it consumes a slot + its pixel budget even if masked (recon R10).
                on_line_count += 1;
                px_used += w as usize * 8;
                if masking_active {
                    SpriteOutcome::Masked
                } else if x_field == 0 && seen_nonzero {
                    // An x=0 sprite read after an x≠0 sprite masks this + all later sprites (recon R10).
                    masking_active = true;
                    SpriteOutcome::Masked
                } else {
                    if x_field != 0 {
                        seen_nonzero = true;
                    }
                    let attr = decode_cell(self.vram_word(base + idx * 8 + 4));
                    self.draw_sprite(
                        &mut buffer,
                        idx as u8,
                        &attr,
                        screen_y,
                        x_field,
                        w,
                        h,
                        line,
                        width,
                        &mut collision,
                    );
                    SpriteOutcome::Rendered
                }
            };
            sprites.push(SpriteEval {
                index: idx as u8,
                y: screen_y,
                x: x_field as i16 - 128,
                width_cells: w,
                height_cells: h,
                link,
                outcome,
            });
            if link == 0 {
                walk_end = SpriteWalkEnd::LinkZero;
                break;
            }
            idx = link as usize;
        }
        SpriteLine {
            sprites,
            walk_end,
            overflow,
            dot_overflow,
            collision,
            buffer,
        }
    }

    /// Draw one admitted sprite into the sprite line buffer (recon RR8): column-major multi-cell tile
    /// addressing with flips, first-come-wins (an already-set pixel is not overwritten), and collision on any
    /// opaque-over-opaque overlap. `x_field` is the raw 9-bit X (screen X = `x_field − 128`).
    #[allow(clippy::too_many_arguments)]
    fn draw_sprite(
        &self,
        buffer: &mut [Option<SpritePixel>],
        index: u8,
        attr: &Cell,
        screen_y: i16,
        x_field: u16,
        w: u8,
        h: u8,
        line: u16,
        width: usize,
        collision: &mut bool,
    ) {
        let screen_x0 = x_field as i32 - 128;
        let sy = (line as i32 - screen_y as i32) as usize; // 0..h*8 (on-line guaranteed)
        let wpx = w as usize * 8;
        let hpx = h as usize * 8;
        for sx in 0..wpx {
            let screen_x = screen_x0 + sx as i32;
            if screen_x < 0 || screen_x >= width as i32 {
                continue; // off the left/right edge
            }
            // Flips mirror the whole sprite; column-major tile = base + cell_col*height + cell_row (RR8).
            let src_sx = if attr.hflip { wpx - 1 - sx } else { sx };
            let src_sy = if attr.vflip { hpx - 1 - sy } else { sy };
            let tile = attr
                .tile
                .wrapping_add(((src_sx / 8) * h as usize + src_sy / 8) as u16);
            let nibble = self.tile_nibble(tile, (src_sx & 7) as u8, (src_sy & 7) as u8);
            if nibble == 0 {
                continue; // transparent sprite pixel
            }
            let sxi = screen_x as usize;
            if buffer[sxi].is_some() {
                *collision = true; // two opaque sprite pixels overlap — first-come-wins (recon RR8)
                continue;
            }
            buffer[sxi] = Some(SpritePixel {
                cram_index: attr.palette * 16 + nibble,
                index,
                palette: attr.palette,
                priority: attr.priority,
                tile,
                nibble,
            });
        }
    }

    /// The semantic line report for the plane stages (design §4 `render_line_report`): the latched scroll /
    /// window inputs + the per-pixel resolution + the sprite evaluation list. The `pixels` and the sprite
    /// section both come from the *same* `resolve_line` call `render_line` maps to RGB — attribution is the
    /// render, not a parallel path (design §1). Recomputed on demand, never stored. Each sprite's outcome
    /// (recon R10 / RR8) — including the render-phase `Masked` — and `sprite_collision` are as rendered.
    pub fn render_line_report(&self, line: u16) -> LineReport {
        let resolved = self.resolve_line(line);
        self.line_report_from(line, resolved)
    }

    /// The per-frame rollup (design §4 `frame_report`; recon R4) — DMA section: the most recent transfer
    /// performed (source / dest / length / mode / target). `None` until the first DMA runs.
    pub fn frame_report(&self) -> FrameReport {
        FrameReport {
            dma: self.last_dma(),
        }
    }

    /// The winning plane/window nametable cell at screen (`x`, `line`) for `layer` (design §4: attribution
    /// reports the decoded entry incl. flips). `None` for a sprite or backdrop winner. Uses the same
    /// coordinate helpers `resolve_line` does, so the returned cell's tile matches the winner's.
    fn winning_cell(
        &self,
        layer: Layer,
        line: u16,
        x: usize,
        h40: bool,
        ctx: &ASlotCtx,
    ) -> Option<Cell> {
        match layer {
            Layer::PlaneB => Some(self.plane_sample(Plane::B, line, x, h40).0),
            Layer::Window => Some(self.nametable_cell(
                self.window_base(),
                self.window_stride(),
                (x / 8) as u16,
                line / 8,
            )),
            Layer::PlaneA => {
                if ctx.r9 && x >= ctx.boundary && x < ctx.boundary + 16 {
                    // R9 reuse: the window's last-column cell (recon R9).
                    let last_col = ((ctx.boundary / 8) as u16).saturating_sub(1);
                    Some(self.nametable_cell(
                        self.window_base(),
                        self.window_stride(),
                        last_col,
                        line / 8,
                    ))
                } else {
                    Some(self.plane_sample(Plane::A, line, x, h40).0)
                }
            }
            Layer::Backdrop | Layer::Sprite(_) => None,
        }
    }

    /// Build the RR9-ordered candidate list for one dot (design §4), annotating each with a verdict relative to
    /// the displayed `winner_layer`: the winner is `Won`; an opaque layer ranked *below* the winner
    /// `LostToPriority`; a transparent layer `Transparent`; an opaque layer ranked *above* the winner but not
    /// displayed is a sprite `Operator` (it shifted the winner's state instead — recon R11.3).
    fn dot_candidates(
        &self,
        backdrop: u8,
        s: Option<SpritePixel>,
        a: &PlanePixel,
        a_layer: Layer,
        b: &PlanePixel,
        winner_layer: Layer,
    ) -> Vec<Candidate> {
        // (rank, layer, opaque, priority, cram_index) — RR9 rank: high-sprite 0, high-A 1, high-B 2,
        // low-sprite 3, low-A 4, low-B 5, backdrop 6.
        let mut list: Vec<(u8, Layer, bool, bool, u8)> = Vec::with_capacity(4);
        if let Some(sp) = s {
            let rank = if sp.priority { 0 } else { 3 };
            list.push((
                rank,
                Layer::Sprite(sp.index),
                true,
                sp.priority,
                sp.cram_index,
            ));
        }
        list.push((
            if a.priority { 1 } else { 4 },
            a_layer,
            a.opaque(),
            a.priority,
            a.cram_index(),
        ));
        list.push((
            if b.priority { 2 } else { 5 },
            Layer::PlaneB,
            b.opaque(),
            b.priority,
            b.cram_index(),
        ));
        list.push((6, Layer::Backdrop, true, false, backdrop));
        list.sort_by_key(|c| c.0);
        let winner_rank = list.iter().find(|c| c.1 == winner_layer).map_or(6, |c| c.0);
        list.into_iter()
            .map(|(rank, layer, opaque, priority, cram_index)| {
                let verdict = if layer == winner_layer {
                    CandidateVerdict::Won
                } else if opaque && rank < winner_rank {
                    CandidateVerdict::Operator
                } else if opaque {
                    CandidateVerdict::LostToPriority
                } else {
                    CandidateVerdict::Transparent
                };
                Candidate {
                    layer,
                    opaque,
                    priority,
                    cram_index,
                    verdict,
                }
            })
            .collect()
    }

    /// Why pixel (`x`, `y`) is the colour it is (design §4 `pixel_attribution`): the winning layer, its
    /// CRAM index → RGB at the resolved shadow/highlight state, the winning plane/window cell, and the
    /// RR9-ordered losing-candidate list. Derived from the same `resolve_line` `render_line` maps to RGB
    /// (attribution is the render, design §1), so `rgb == render_line(y)[x]`.
    pub fn pixel_attribution(&self, x: u16, y: u16) -> PixelAttribution {
        let h40 = self.render_h40();
        let width = if h40 { 320 } else { 256 };
        let xi = x as usize;
        let backdrop = self.backdrop_index();
        let resolved = self.resolve_line(y);
        let winner = resolved
            .pixels
            .get(xi)
            .copied()
            .unwrap_or_else(|| backdrop_px(xi, backdrop));
        // Per-line plane-A-slot inputs (same as resolve_line) for the candidate list + winning cell.
        let win = self.window_span(y, width);
        let a_hscroll = self.plane_hscroll(Plane::A, y);
        let boundary = win.map_or(0, |w| w.end_x as usize);
        let r9 = matches!(win, Some(w) if !w.full_line && w.start_x == 0) && a_hscroll & 0x0F != 0;
        let ctx = ASlotCtx {
            win,
            r9,
            boundary,
            a_hscroll,
        };
        let disp = self.regs()[0x01] & 0x40 != 0;
        let lcb = self.regs()[0x00] & 0x20 != 0 && xi < 8;
        let candidates = if !disp || lcb || xi >= width {
            // Blanked (display off / leftmost-column blank / out of range): only the backdrop is meaningful.
            vec![Candidate {
                layer: Layer::Backdrop,
                opaque: true,
                priority: false,
                cram_index: backdrop,
                verdict: CandidateVerdict::Won,
            }]
        } else {
            let b = self.plane_pixel(Plane::B, y, xi, h40);
            let (a, a_layer) = self.a_slot_pixel(y, xi, h40, &ctx);
            self.dot_candidates(
                backdrop,
                resolved.sprite.buffer[xi],
                &a,
                a_layer,
                &b,
                winner.layer,
            )
        };
        PixelAttribution {
            x,
            y,
            winner: winner.layer,
            cram_index: winner.cram_index,
            rgb: self.cram_rgb_state(winner.cram_index, winner.state),
            state: winner.state,
            cell: self.winning_cell(winner.layer, y, xi, h40, &ctx),
            candidates,
        }
    }

    /// Build the [`LineReport`] from an already-resolved line (shared by `render_line_report` and
    /// `render_scanline` so both derive from the same `resolve_line` — attribution is the render, design §1).
    fn line_report_from(&self, line: u16, resolved: ResolvedLine) -> LineReport {
        let h40 = self.render_h40();
        let width = if h40 { 320 } else { 256 };
        LineReport {
            line,
            h40,
            display_enabled: self.regs()[0x01] & 0x40 != 0,
            backdrop: self.backdrop_index(),
            plane_a: self.plane_scroll(Plane::A, line, h40, width),
            plane_b: self.plane_scroll(Plane::B, line, h40, width),
            window: self.window_span(line, width),
            sprites: resolved.sprite.sprites,
            sprite_walk_end: resolved.sprite.walk_end,
            sprite_overflow: resolved.sprite.overflow,
            sprite_collision: resolved.sprite.collision,
            pixels: resolved.pixels,
        }
    }

    /// Render one scanline **and commit** its sprite latches (recon R10) — the stateful per-line advance that
    /// makes the sprite-overflow / collision status bits and the masking carry "go real". Unlike the pure
    /// `render_line` / `render_line_report`, this takes `&mut self`: it seeds masking from the current
    /// `sprite_dot_overflow_carry`, then commits the new carry (this line's dot overflow), ORs the
    /// sprite-overflow / collision status latches (sticky until a status read clears them), and returns the
    /// same [`LineReport`]. This is the hook the eventual per-frame render loop / push-5 golden-frame
    /// differential drives; it is **not** wired into `System::run` this push (so the export golden is
    /// untouched — the test ROM drives no rendering).
    pub fn render_scanline(&mut self, line: u16) -> LineReport {
        let resolved = self.resolve_line(line);
        let (dot, over, coll) = (
            resolved.sprite.dot_overflow,
            resolved.sprite.overflow,
            resolved.sprite.collision,
        );
        let report = self.line_report_from(line, resolved);
        self.commit_scanline_sprites(dot, over, coll);
        report
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

    // --- RR7/RR4/R9: plane A + window + transparency compositing ------------------------------------------

    /// A plane-A + plane-B + window fixture: display on, A at `$C000`, B at `$E000`, window at `$A000`,
    /// 32×32, h-scroll table at `$8000`, full scroll, a solid red tile 1 + blue tile 2 (CRAM 1/2).
    fn pa_fixture(h40: bool) -> Vdp {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x01, 0x40); // display on
        if h40 {
            set_reg(&mut v, 0x0C, 0x81);
        }
        set_reg(&mut v, 0x02, 0x30); // plane A base 0xC000
        set_reg(&mut v, 0x04, 0x07); // plane B base 0xE000
        set_reg(&mut v, 0x03, 0x28); // window base 0xA000
        set_reg(&mut v, 0x10, 0x00); // 32x32
        set_reg(&mut v, 0x0D, 0x20); // h-scroll table base 0x8000
        set_reg(&mut v, 0x0B, 0x00); // full scroll
        fill_tile(&mut v, 1, 1);
        fill_tile(&mut v, 2, 2);
        write_cram(&mut v, 1, 0x000E); // red
        write_cram(&mut v, 2, 0x0E00); // blue
        v
    }

    #[test]
    fn plane_a_composites_over_b_by_transparency() {
        let mut v = pa_fixture(false);
        put_cell(&mut v, 0xE000, 0x0001); // B cell(0,0) red
        put_cell(&mut v, 0xC000, 0x0002); // A cell(0,0) blue → opaque A wins
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(2),
            "opaque plane A over plane B"
        );
        put_cell(&mut v, 0xE000 + 2, 0x0001); // B cell(1,0) red
                                              // A cell(1,0) stays tile 0 (transparent) → B shows through
        assert_eq!(
            v.render_line(0)[8],
            v.cram_rgb(1),
            "transparent plane A shows plane B"
        );
        assert_eq!(
            v.render_line(0)[16],
            v.cram_rgb(0),
            "both transparent → backdrop"
        );
    }

    #[test]
    fn high_priority_plane_b_wins_over_low_priority_plane_a() {
        // RR9 (push-5, replaces the push-3 opacity-only boundary): a HIGH-priority plane B now beats an opaque
        // LOW-priority plane A (high-B > low-A). The pre-push-5 test asserted the opposite (opacity wins); the
        // scope boundary moved when priority ordering went real.
        let mut v = pa_fixture(false);
        put_cell(&mut v, 0xE000, 0x8001); // B cell(0,0) red, PRIORITY set
        put_cell(&mut v, 0xC000, 0x0002); // A cell(0,0) blue, low priority
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "high-priority B wins over low-priority A (RR9: high-B > low-A)"
        );
        assert_eq!(
            v.render_line_report(0).pixels[0].layer,
            Layer::PlaneB,
            "the reported winner is plane B"
        );
        // Sanity: with B low-priority instead, low-A > low-B → plane A (blue) wins.
        put_cell(&mut v, 0xE000, 0x0001); // B now low priority
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(2),
            "both low → low-A > low-B, plane A (blue) wins"
        );
    }

    #[test]
    fn window_span_covers_the_configured_region() {
        let mut v = pa_fixture(false); // H32, width 256
        set_reg(&mut v, 0x11, 0x03); // left window, WHP=3 → [0,48)
        set_reg(&mut v, 0x12, 0x00);
        assert_eq!(
            v.window_span(0, 256),
            Some(WindowSpan {
                start_x: 0,
                end_x: 48,
                full_line: false
            })
        );
        set_reg(&mut v, 0x11, 0x83); // right window → [48,256)
        assert_eq!(
            v.window_span(0, 256),
            Some(WindowSpan {
                start_x: 48,
                end_x: 256,
                full_line: false
            })
        );
        set_reg(&mut v, 0x11, 0x00);
        set_reg(&mut v, 0x12, 0x04); // top vertical window, WVP=4 → lines 0..32 full
        assert_eq!(
            v.window_span(0, 256),
            Some(WindowSpan {
                start_x: 0,
                end_x: 256,
                full_line: true
            })
        );
        assert_eq!(
            v.window_span(32, 256),
            None,
            "below the top band → no window"
        );
        set_reg(&mut v, 0x12, 0x84); // bottom vertical window → lines 32.. full
        assert_eq!(v.window_span(0, 256), None);
        assert_eq!(
            v.window_span(32, 256),
            Some(WindowSpan {
                start_x: 0,
                end_x: 256,
                full_line: true
            })
        );
        set_reg(&mut v, 0x12, 0x00);
        assert_eq!(v.window_span(0, 256), None, "no window configured");
    }

    #[test]
    fn window_replaces_plane_a_and_does_not_scroll() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x11, 0x02); // left window WHP=2 → [0,32)
        put_cell(&mut v, 0xA000, 0x0001); // window cell(0,0) red
        put_cell(&mut v, 0xC000, 0x0002); // plane A cell(0,0) blue (should be replaced)
        put_cell(&mut v, 0xC000 + 4 * 2, 0x0002); // plane A cell(4,0) blue at x=32 (outside window)
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "window replaces plane A at x=0"
        );
        assert_eq!(
            v.render_line(0)[32],
            v.cram_rgb(2),
            "plane A shows outside the window"
        );
        put_cell(&mut v, 0x8000, 100); // plane A h-scroll = 100
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "the window does not scroll with plane A"
        );
    }

    #[test]
    fn r9_reuses_the_window_last_column_tile() {
        // R9 (interim): left window + plane-A hscroll & 15 != 0 → the first 16 px of plane A right of the
        // boundary show the window's last-column tile, not plane A's own.
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x11, 0x02); // left window [0,32) → boundary at x=32
        put_cell(&mut v, 0xA000 + 3 * 2, 0x0001); // window last column (col 3) = red
        put_cell(&mut v, 0xC000 + 4 * 2, 0x0002); // plane A cell(4,0) = blue (would show without the bug)
        put_cell(&mut v, 0x8000, 4); // plane A hscroll = 4 → & 15 != 0 → R9 active
        assert_eq!(
            v.render_line(0)[32],
            v.cram_rgb(1),
            "R9: the glitched column reuses the window's last-column tile (red)"
        );
        put_cell(&mut v, 0x8000, 0); // hscroll aligned → no R9
        assert_eq!(
            v.render_line(0)[32],
            v.cram_rgb(2),
            "no R9 when plane-A hscroll is 16-aligned → plane A's own tile (blue)"
        );
    }

    #[test]
    fn leftmost_column_blank_forces_backdrop() {
        let mut v = pa_fixture(false);
        put_cell(&mut v, 0xC000, 0x0002); // A cell(0,0) blue → x 0..7
        put_cell(&mut v, 0xC000 + 2, 0x0002); // A cell(1,0) blue → x 8..15
        assert_eq!(v.render_line(0)[0], v.cram_rgb(2), "no LCB: blue at x=0");
        set_reg(&mut v, 0x00, 0x20); // LCB (reg 0 bit 5)
        let l = v.render_line(0);
        let bg = v.cram_rgb(0);
        for (x, px) in l.iter().enumerate().take(8) {
            assert_eq!(*px, bg, "LCB blanks x={x} to backdrop");
        }
        assert_eq!(l[8], v.cram_rgb(2), "x=8 is unaffected by LCB");
    }

    // --- design §4: render_line_report -------------------------------------------------------------------

    #[test]
    fn report_pixels_reproduce_render_line() {
        // The §4 attribution invariant: mapping each reported winner's cram_index to RGB reproduces the pixel.
        let mut v = pa_fixture(false);
        put_cell(&mut v, 0xE000, 0x0001); // B
        put_cell(&mut v, 0xC000 + 2, 0x0002); // A
        put_cell(&mut v, 0x8000, 3); // some plane A hscroll
        put_cell(&mut v, 0x8002, 5); // some plane B hscroll
        let rgb = v.render_line(5);
        let rep = v.render_line_report(5);
        assert_eq!(rep.pixels.len(), rgb.len());
        for (x, px) in rep.pixels.iter().enumerate() {
            assert_eq!(
                v.cram_rgb(px.cram_index),
                rgb[x],
                "reported winner reproduces the pixel at x={x}"
            );
        }
    }

    #[test]
    fn report_carries_effective_scroll_and_window() {
        let mut v = pa_fixture(false);
        put_cell(&mut v, 0x8000, 10); // plane A hscroll
        put_cell(&mut v, 0x8002, 20); // plane B hscroll
        write_vsram(&mut v, 0, 5); // plane A full vscroll
        write_vsram(&mut v, 1, 7); // plane B full vscroll
        set_reg(&mut v, 0x11, 0x02); // left window [0,32)
        let r = v.render_line_report(0);
        assert_eq!(r.plane_a.hscroll, 10);
        assert_eq!(r.plane_b.hscroll, 20);
        assert_eq!(r.plane_a.vscroll, VScroll::Full(5));
        assert_eq!(r.plane_b.vscroll, VScroll::Full(7));
        assert_eq!(
            r.window,
            Some(WindowSpan {
                start_x: 0,
                end_x: 32,
                full_line: false
            })
        );
        assert!(r.display_enabled && !r.h40);
        assert_eq!(r.backdrop, 0);
    }

    #[test]
    fn report_two_cell_vscroll_has_per_column_values() {
        let mut v = pa_fixture(true); // H40 → 20 columns
        set_reg(&mut v, 0x0B, 0x04); // full h, 2-cell v
        write_vsram(&mut v, 2, 9); // plane A column 1 word (word idx 1*2 = 2)
        let r = v.render_line_report(0);
        match r.plane_a.vscroll {
            VScroll::TwoCell(cols) => {
                assert_eq!(cols.len(), 20, "H40 → 20 per-column values");
                assert_eq!(cols[0], 0, "column 0");
                assert_eq!(cols[1], 9, "column 1 from VSRAM word 2");
            }
            other => panic!("expected TwoCell, got {other:?}"),
        }
    }

    #[test]
    fn report_reflects_display_disabled() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x01, 0x00);
        let r = v.render_line_report(0);
        assert!(!r.display_enabled);
        assert!(
            r.pixels.iter().all(|p| p.layer == Layer::Backdrop),
            "every pixel is backdrop when display is off"
        );
    }

    // --- design §4 / RR8 / R5: sprites_decoded -----------------------------------------------------------

    /// Write one SAT entry's four words through the data port (so the SAT-cache write-through runs), with
    /// the base already set in reg 5 and autoinc = 2.
    fn write_sprite(v: &mut Vdp, index: usize, y: u16, sizelink: u16, attr: u16, x: u16) {
        let base = v.sat_base();
        setup_write(v, 0x01, (base + index * 8) as u16);
        v.data_write(y);
        v.data_write(sizelink);
        v.data_write(attr);
        v.data_write(x);
    }

    #[test]
    fn sprites_decoded_reads_cache_for_y_size_link_and_vram_for_x_tile() {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x0F, 2); // autoinc 2
        set_reg(&mut v, 0x05, 0x10); // SAT base 0x2000
                                     // Y=0x0100 (screen 128), size=0x05 (2×2), link=3, attr=0x8123 (pri, tile 0x123), X=0x00C8 (screen 72).
        write_sprite(&mut v, 0, 0x0100, 0x0503, 0x8123, 0x00C8);
        let s = v.sprites_decoded();
        assert_eq!(s.len(), 80, "all 80 entries decoded");
        let s0 = s[0];
        assert_eq!(s0.y, 128, "screen Y = Yfield - 128 (cached)");
        assert_eq!(s0.x, 72, "screen X = Xfield - 128 (VRAM)");
        assert_eq!(
            (s0.width_cells, s0.height_cells),
            (2, 2),
            "size 0x05 → 2×2 cells"
        );
        assert_eq!(s0.link, 3);
        assert_eq!(s0.tile, 0x123);
        assert!(s0.priority);
        assert!(
            !s0.cache_divergence,
            "cache and VRAM coherent → no divergence"
        );
    }

    #[test]
    fn sprites_decoded_flags_cache_divergence_after_a_reg5_change() {
        // Recon R5 (Bloodlines): move the SAT base without rewriting — the cache keeps the old Y/size/link
        // while VRAM at the new base reads different, so the entry flags cache_divergence.
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10); // base 0x2000
        write_sprite(&mut v, 0, 0x0100, 0x0503, 0x8123, 0x00C8);
        set_reg(&mut v, 0x05, 0x20); // move base to 0x4000 (VRAM there is all zero)
        let s0 = v.sprites_decoded()[0];
        assert_eq!(s0.y, 0x0100 - 128, "Y is still the stale cached value");
        assert!(
            s0.cache_divergence,
            "cached Y/size/link disagree with VRAM at the new base"
        );
    }

    // --- design §4 / R10 / RR8: sprite evaluation walk (render_line_report sprite section) ----------------

    /// Write a chain of `n` sprites (indices 0..n) all at `y_field` with `size`, each linking to the next
    /// (the last links to 0 = terminate). SAT base + autoinc must already be set.
    fn write_chain(v: &mut Vdp, n: usize, y_field: u16, size: u16) {
        for i in 0..n {
            let link = if i + 1 < n { (i + 1) as u16 } else { 0 };
            write_sprite(v, i, y_field, (size << 8) | link, 0x0001, 0x0080 + i as u16);
        }
    }

    #[test]
    fn evaluation_walks_the_link_list_and_ends_at_link_zero() {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10); // base 0x2000
        write_chain(&mut v, 3, 0x0080, 0x00); // sprites 0→1→2→(0 = end), all on line 0
        let r = v.render_line_report(0);
        assert_eq!(
            r.sprites.iter().map(|s| s.index).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "walked in link order from sprite 0"
        );
        assert_eq!(r.sprite_walk_end, SpriteWalkEnd::LinkZero);
        assert!(r
            .sprites
            .iter()
            .all(|s| s.outcome == SpriteOutcome::Rendered));
        assert!(!r.sprite_overflow);
    }

    #[test]
    fn evaluation_terminates_a_self_looping_link_at_the_parse_cap() {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // Sprite 0 links to 1; sprite 1 links to itself (link 1) → a cycle that link-0 can't express.
        // (size byte 0 = 1×1, so the size/link word is just the link value.)
        write_sprite(&mut v, 0, 0x0080, 0x0001, 0x0001, 0x0080);
        write_sprite(&mut v, 1, 0x0080, 0x0001, 0x0001, 0x0088);
        let r = v.render_line_report(0);
        assert_eq!(
            r.sprite_walk_end,
            SpriteWalkEnd::MaxCount,
            "a self-loop is bounded by the parse cap (never hangs)"
        );
        assert_eq!(r.sprites.len(), 64, "H32 parses at most 64 sprites");
    }

    #[test]
    fn evaluation_marks_off_line_sprites_by_the_y_span() {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // Sprite 0: screen y 0, 1×2 (covers lines 0..=15); sprite 1: screen y 16, 1×1 (link 0).
        write_sprite(&mut v, 0, 0x0080, (0x01 << 8) | 1, 0x0001, 0x0080);
        write_sprite(&mut v, 1, 0x0090, 0x0000, 0x0001, 0x0090);
        assert_eq!(v.render_line_report(0).sprites[0].height_cells, 2);
        assert_eq!(
            v.render_line_report(0).sprites[0].outcome,
            SpriteOutcome::Rendered
        );
        assert_eq!(
            v.render_line_report(0).sprites[1].outcome,
            SpriteOutcome::OffLine
        );
        assert_eq!(
            v.render_line_report(15).sprites[0].outcome,
            SpriteOutcome::Rendered,
            "the 2-cell-tall sprite still covers line 15"
        );
        assert_eq!(
            v.render_line_report(16).sprites[0].outcome,
            SpriteOutcome::OffLine,
            "sprite 0 ends at line 15"
        );
    }

    #[test]
    fn evaluation_drops_beyond_the_per_line_sprite_limit() {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        write_chain(&mut v, 18, 0x0080, 0x00); // 18 × 1×1 (8 px each = 144 px, no budget issue) on line 0
        let r = v.render_line_report(0);
        assert_eq!(r.sprites.len(), 18, "all 18 walked");
        let drawn = r
            .sprites
            .iter()
            .filter(|s| s.outcome == SpriteOutcome::Rendered)
            .count();
        assert_eq!(drawn, 16, "H32 draws only 16 per line");
        assert_eq!(r.sprites[16].outcome, SpriteOutcome::DroppedLineLimit);
        assert_eq!(r.sprites[17].outcome, SpriteOutcome::DroppedLineLimit);
        assert!(r.sprite_overflow, "the count limit sets overflow");
    }

    #[test]
    fn evaluation_drops_when_the_pixel_budget_is_exhausted() {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        write_chain(&mut v, 10, 0x0080, 0x0C); // 10 × 4×1 (32 px each) on line 0
        let r = v.render_line_report(0);
        let drawn = r
            .sprites
            .iter()
            .filter(|s| s.outcome == SpriteOutcome::Rendered)
            .count();
        assert_eq!(
            drawn, 8,
            "256 px / 32 px = 8 sprites fit (count 8 < 16, so it is a budget drop)"
        );
        assert_eq!(r.sprites[8].outcome, SpriteOutcome::DroppedPixelBudget);
        assert!(r.sprite_overflow, "the pixel budget sets overflow");
    }

    #[test]
    fn evaluation_uses_h40_limits() {
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x0C, 0x81); // H40
        set_reg(&mut v, 0x05, 0x10); // base 0x2000 (H40 bit-0 mask keeps it)
        write_chain(&mut v, 22, 0x0080, 0x00); // 22 × 1×1 on line 0
        let r = v.render_line_report(0);
        assert!(r.h40);
        let drawn = r
            .sprites
            .iter()
            .filter(|s| s.outcome == SpriteOutcome::Rendered)
            .count();
        assert_eq!(drawn, 20, "H40 draws 20 per line");
    }

    #[test]
    fn plane_only_line_has_an_empty_sprite_list() {
        // A cleared SAT (all entries link 0, y = -128) parses just sprite 0 (off-line) and stops.
        let v = pa_fixture(false);
        let r = v.render_line_report(0);
        assert_eq!(r.sprite_walk_end, SpriteWalkEnd::LinkZero);
        assert!(!r.sprite_overflow && !r.sprite_collision);
        assert!(r
            .sprites
            .iter()
            .all(|s| s.outcome == SpriteOutcome::OffLine));
    }

    // --- RR8 / R10 / RR7: sprite compositing + masking + collision (slice 3) -----------------------------

    /// The reported pixel at screen `x`, asserting it is a sprite pixel.
    fn sprite_px(r: &LineReport, x: usize) -> PixelResolution {
        let p = r.pixels[x];
        assert!(
            matches!(p.layer, Layer::Sprite(_)),
            "pixel {x} should be a sprite"
        );
        p
    }

    #[test]
    fn sprite_composites_over_planes_by_opacity() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10); // SAT base 0x2000
        put_cell(&mut v, 0xC000, 0x0002); // plane A cell(0,0) blue at x 0..7
                                          // Sprite 0: screen (0,0), 1×1, tile 1 (red), link 0.
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0001, 0x0080);
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "opaque sprite (red) overlays plane A (blue)"
        );
        assert_eq!(v.render_line_report(0).pixels[0].layer, Layer::Sprite(0));
    }

    #[test]
    fn transparent_sprite_pixel_shows_the_plane() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        put_cell(&mut v, 0xC000, 0x0002); // plane A blue at x 0..7
                                          // Sprite 0: tile 0 (all transparent), 1×1 at screen (0,0).
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0000, 0x0080);
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(2),
            "a transparent sprite pixel shows plane A beneath"
        );
    }

    #[test]
    fn sprite_multi_cell_tiles_are_column_major() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        for t in 4..=7 {
            fill_tile(&mut v, t, 1); // solid opaque, distinct tile indices
        }
        // 2×2 sprite, base tile 4, at screen (0,0). size byte 0x05 = 2×2 (high byte of the size/link word).
        write_sprite(&mut v, 0, 0x0080, 0x0500, 0x0004, 0x0080);
        // Column-major (RR8): cell (cx,cy) tile = 4 + cx*height(2) + cy.
        let r0 = v.render_line_report(0); // cy = 0
        assert_eq!(sprite_px(&r0, 0).tile, 4, "cell (0,0) → base + 0");
        assert_eq!(
            sprite_px(&r0, 8).tile,
            6,
            "cell (1,0) → base + 2 (down the column first)"
        );
        let r8 = v.render_line_report(8); // cy = 1
        assert_eq!(sprite_px(&r8, 0).tile, 5, "cell (0,1) → base + 1");
        assert_eq!(sprite_px(&r8, 8).tile, 7, "cell (1,1) → base + 3");
    }

    #[test]
    fn sprite_hflip_mirrors_the_cells() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        fill_tile(&mut v, 4, 1); // red
        fill_tile(&mut v, 5, 2); // blue
                                 // 2×1 sprite (size byte 0x04), base tile 4 → left cell tile 4 (red), right cell tile 5 (blue).
        write_sprite(&mut v, 0, 0x0080, 0x0400, 0x0004, 0x0080);
        let r = v.render_line(0);
        assert_eq!(r[0], v.cram_rgb(1), "no flip: left cell red");
        assert_eq!(r[8], v.cram_rgb(2), "no flip: right cell blue");
        // hflip (attr bit 11 = 0x0800): the cells swap.
        write_sprite(&mut v, 0, 0x0080, 0x0400, 0x0804, 0x0080);
        let r = v.render_line(0);
        assert_eq!(
            r[0],
            v.cram_rgb(2),
            "hflip: left now shows the right cell (blue)"
        );
        assert_eq!(
            r[8],
            v.cram_rgb(1),
            "hflip: right now shows the left cell (red)"
        );
    }

    #[test]
    fn x_zero_sprite_masks_all_later_sprites_after_a_nonzero_read() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // Sprite 0: x≠0 (screen 16), tile 1 red, link 1 — arms the masking latch.
        write_sprite(&mut v, 0, 0x0080, 0x0001, 0x0001, 128 + 16);
        // Sprite 1: x=0 (screen -128, off-left), link 2 — masks itself + all later sprites (R10).
        write_sprite(&mut v, 1, 0x0080, 0x0002, 0x0001, 0x0000);
        // Sprite 2: x≠0 (screen 100), tile 1 red, link 0 — would draw, but is masked.
        write_sprite(&mut v, 2, 0x0080, 0x0000, 0x0001, 128 + 100);
        let r = v.render_line_report(0);
        assert_eq!(r.sprites[0].outcome, SpriteOutcome::Rendered);
        assert_eq!(
            r.sprites[1].outcome,
            SpriteOutcome::Masked,
            "x=0 after x≠0 masks"
        );
        assert_eq!(
            r.sprites[2].outcome,
            SpriteOutcome::Masked,
            "and every later sprite"
        );
        assert_eq!(
            v.render_line(0)[16],
            v.cram_rgb(1),
            "sprite 0 drew before the mask"
        );
        assert_eq!(
            v.render_line(0)[100],
            v.cram_rgb(0),
            "masked sprite 2 output suppressed → backdrop"
        );
    }

    #[test]
    fn first_on_line_x_zero_does_not_mask_without_the_carry() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // Sprite 0: x=0 first-on-line (carry false at power-on) — does NOT mask; sprite 1: x≠0, draws.
        write_sprite(&mut v, 0, 0x0080, 0x0001, 0x0001, 0x0000);
        write_sprite(&mut v, 1, 0x0080, 0x0000, 0x0001, 128 + 50);
        let r = v.render_line_report(0);
        assert_eq!(
            r.sprites[0].outcome,
            SpriteOutcome::Rendered,
            "first-on-line x=0 does not mask when the carry is clear"
        );
        assert_eq!(r.sprites[1].outcome, SpriteOutcome::Rendered);
        assert_eq!(
            v.render_line(0)[50],
            v.cram_rgb(1),
            "sprite 1 draws at x=50"
        );
    }

    #[test]
    fn overlapping_sprites_set_collision_first_come_wins() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // Two opaque 1×1 sprites both at screen (0,0) → overlap.
        write_sprite(&mut v, 0, 0x0080, 0x0001, 0x0001, 0x0080); // link 1
        write_sprite(&mut v, 1, 0x0080, 0x0000, 0x0001, 0x0080); // link 0
        let r = v.render_line_report(0);
        assert!(
            r.sprite_collision,
            "two opaque sprite pixels overlapping set collision"
        );
        assert_eq!(
            r.pixels[0].layer,
            Layer::Sprite(0),
            "first-come-wins: sprite 0 (earlier in link order) owns the pixel"
        );
    }

    #[test]
    fn high_priority_plane_beats_low_priority_sprite() {
        // RR9 (push-5, replaces the push-4 opacity-only boundary): a HIGH-priority plane now beats a
        // LOW-priority sprite (high-A > low-sprite). The pre-push-5 test asserted the opposite (the sprite
        // overlaid by opacity); the scope boundary moved when priority ordering went real.
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        put_cell(&mut v, 0xC000, 0x8002); // plane A cell(0,0): HIGH priority, blue
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0001, 0x0080); // low-priority red sprite
        let r = v.render_line_report(0);
        assert_eq!(
            r.pixels[0].layer,
            Layer::PlaneA,
            "high-priority plane A beats the low-priority sprite (RR9: high-A > low-sprite)"
        );
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(2),
            "plane A blue wins by priority"
        );
        assert!(
            r.pixels[0].priority,
            "the winning plane's priority bit is set"
        );
        // Make the sprite high-priority: high-sprite > high-A → the sprite (red) wins.
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x8001, 0x0080); // attr bit 15 set → high priority
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "a high-priority sprite beats the high-priority plane (RR9: high-sprite > high-A)"
        );
        assert_eq!(v.render_line_report(0).pixels[0].layer, Layer::Sprite(0));
    }

    // --- RR9: full inter-layer priority order ------------------------------------------------------------

    #[test]
    fn rr9_high_a_beats_high_b() {
        // Both planes high-priority + opaque → high-A > high-B.
        let mut v = pa_fixture(false);
        put_cell(&mut v, 0xC000, 0x8001); // A high, red
        put_cell(&mut v, 0xE000, 0x8002); // B high, blue
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "high-A wins over high-B"
        );
        assert_eq!(v.render_line_report(0).pixels[0].layer, Layer::PlaneA);
    }

    #[test]
    fn rr9_low_sprite_beats_low_a_beats_low_b_beats_backdrop() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        put_cell(&mut v, 0xE000, 0x0002); // B low, blue at x 0..7
        put_cell(&mut v, 0xE000 + 2, 0x0002); // B low, blue at x 8..15
        put_cell(&mut v, 0xC000, 0x0001); // A low, red at x 0..7 (covers B)
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0003, 0x0080); // low sprite, tile 3 at screen 0
        fill_tile(&mut v, 3, 3); // tile 3 = colour 3
        write_cram(&mut v, 3, 0x00E0); // colour 3 = green
        let r = v.render_line(0);
        assert_eq!(r[0], v.cram_rgb(3), "x0: low-sprite > low-A > low-B");
        assert_eq!(r[8], v.cram_rgb(2), "x8: no sprite, no A → low-B (blue)");
        assert_eq!(r[16], v.cram_rgb(0), "x16: nothing opaque → backdrop");
    }

    #[test]
    fn rr9_transparent_higher_layer_is_skipped() {
        // A transparent plane-A pixel does not block plane B (loses by transparency), even though A would
        // outrank B at equal priority.
        let mut v = pa_fixture(false);
        put_cell(&mut v, 0xE000, 0x0001); // B opaque red
                                          // A cell(0,0) stays tile 0 → transparent
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "transparent A → plane B shows through"
        );
    }

    #[test]
    fn rr9_window_occupies_the_a_slot() {
        // The window sits in plane A's priority slot: a high-priority window pixel beats low-priority plane B;
        // a low-priority window pixel loses to high-priority plane B.
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x11, 0x02); // left window [0,32)
        put_cell(&mut v, 0xA000, 0x8001); // window cell(0,0): HIGH priority, red
        put_cell(&mut v, 0xE000, 0x0002); // plane B low, blue
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "high-priority window (A-slot) > low-B"
        );
        assert_eq!(v.render_line_report(0).pixels[0].layer, Layer::Window);
        put_cell(&mut v, 0xA000, 0x0001); // window now low priority
        put_cell(&mut v, 0xE000, 0x8002); // plane B now HIGH priority
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(2),
            "high-B > low-priority window (A-slot)"
        );
        assert_eq!(v.render_line_report(0).pixels[0].layer, Layer::PlaneB);
    }

    // --- R11: shadow/highlight (non-operator) ------------------------------------------------------------

    #[test]
    fn intensity_ramp_matches_the_pinned_table() {
        // R11.5: shared 0..14 quantization, out = step*255/14. Normal == the plain ramp.
        assert_eq!(intensity(0, PixelState::Normal), 0);
        assert_eq!(intensity(7, PixelState::Normal), 255);
        assert_eq!(intensity(0, PixelState::Shadow), 0);
        assert_eq!(intensity(7, PixelState::Shadow), 127, "shadow max = ½Max");
        assert_eq!(
            intensity(0, PixelState::Highlight),
            127,
            "highlight min = ½Max"
        );
        assert_eq!(
            intensity(7, PixelState::Highlight),
            255,
            "highlight max = Max"
        );
    }

    #[test]
    fn sh_disabled_is_always_normal() {
        // Reg $0C bit 3 = 0: both planes low-priority does NOT shadow — output is the plain ramp.
        let mut v = pa_fixture(false);
        put_cell(&mut v, 0xE000, 0x0001); // B low, red, opaque; A transparent → both planes low
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(1),
            "S/H disabled → normal red"
        );
        assert_eq!(v.render_line_report(0).pixels[0].state, PixelState::Normal);
    }

    #[test]
    fn sh_default_shadow_when_both_planes_low() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0C, 0x08); // S/H on (H32)
        put_cell(&mut v, 0xE000, 0x0001); // B low, red, opaque; A transparent → default shadow
        assert_eq!(
            v.render_line(0)[0],
            (127, 0, 0),
            "both planes low + S/H → the plane B pixel is shadowed"
        );
        assert_eq!(v.render_line_report(0).pixels[0].state, PixelState::Shadow);
        // A high-priority plane pixel makes the default normal.
        put_cell(&mut v, 0xE000, 0x8001); // B high, red
        assert_eq!(
            v.render_line(0)[0],
            (255, 0, 0),
            "high-priority plane → normal"
        );
    }

    #[test]
    fn sh_transparent_plane_priority_contributes_to_default() {
        // The Bloodlines light-ray trick: a transparent but HIGH-priority plane B still contributes its
        // priority bit, flipping the default to normal even though it draws nothing.
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0C, 0x08); // S/H on
        put_cell(&mut v, 0xC000, 0x0001); // A low, red, opaque → wins
        put_cell(&mut v, 0xE000, 0x8000); // B HIGH priority but tile 0 (transparent)
        assert_eq!(
            v.render_line(0)[0],
            (255, 0, 0),
            "transparent high-priority B → default normal → A not shadowed"
        );
        put_cell(&mut v, 0xE000, 0x0000); // B low + transparent → default shadow
        assert_eq!(
            v.render_line(0)[0],
            (127, 0, 0),
            "without B's priority the low-A pixel is shadowed"
        );
    }

    #[test]
    fn sh_backdrop_is_shadowed_when_both_planes_low() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0C, 0x08); // S/H on
        set_reg(&mut v, 0x07, 0x01); // backdrop = CRAM 1 (red)
                                     // planes all transparent (cleared VRAM), both low → default shadow.
        let r = v.render_line_report(0);
        assert_eq!(r.pixels[0].layer, Layer::Backdrop);
        assert_eq!(r.pixels[0].state, PixelState::Shadow);
        assert_eq!(
            v.render_line(0)[0],
            (127, 0, 0),
            "the backdrop is shadowed too"
        );
    }

    #[test]
    fn sh_high_priority_sprite_is_never_shadowed() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0C, 0x08); // S/H on
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // Both planes transparent (default shadow), a HIGH-priority red sprite → never shadowed.
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x8001, 0x0080);
        assert_eq!(v.render_line(0)[0], (255, 0, 0), "high sprite → normal");
        assert_eq!(v.render_line_report(0).pixels[0].state, PixelState::Normal);
    }

    #[test]
    fn sh_low_priority_sprite_takes_the_default() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0C, 0x08); // S/H on
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // Low-priority red sprite, both planes low → shadowed.
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0001, 0x0080);
        assert_eq!(
            v.render_line(0)[0],
            (127, 0, 0),
            "low sprite + both planes low → shadow"
        );
        // A high-priority (transparent) plane B flips the default to normal → the sprite is not shadowed.
        put_cell(&mut v, 0xE000, 0x8000);
        assert_eq!(
            v.render_line(0)[0],
            (255, 0, 0),
            "low sprite + a high plane present → normal"
        );
    }

    #[test]
    fn sh_colour_14_sprite_pixel_is_never_shadowed() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0C, 0x08); // S/H on
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        fill_tile(&mut v, 8, 14); // tile of solid nibble 14
        fill_tile(&mut v, 9, 13); // tile of solid nibble 13 (the contrast)
        write_cram(&mut v, 14, 0x000E); // colour 14 = red
        write_cram(&mut v, 13, 0x000E); // colour 13 = red
                                        // Low-priority sprite, both planes low (default shadow): nibble 14 is never shadowed.
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0008, 0x0080);
        assert_eq!(
            v.render_line(0)[0],
            (255, 0, 0),
            "colour-14 sprite pixel → never shadowed (normal)"
        );
        // Same setup with nibble 13 → shadowed (proving the quirk is specific to colour 14).
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0009, 0x0080);
        assert_eq!(
            v.render_line(0)[0],
            (127, 0, 0),
            "colour-13 sprite pixel → shadowed by the default"
        );
    }

    // --- R11.3: shadow/highlight operators ---------------------------------------------------------------

    #[test]
    fn combine_operator_table() {
        use PixelState::*;
        assert_eq!(combine_operator(Normal, Highlight), Highlight);
        assert_eq!(combine_operator(Normal, Shadow), Shadow);
        assert_eq!(combine_operator(Shadow, Highlight), Normal, "undo");
        assert_eq!(combine_operator(Shadow, Shadow), Shadow, "no double-shadow");
        assert_eq!(combine_operator(Highlight, Highlight), Highlight, "clamp");
        assert_eq!(combine_operator(Highlight, Shadow), Normal);
    }

    /// An S/H-on fixture with a palette-3 highlight-op tile (8, nibble 14) and shadow-op tile (9, nibble 15).
    fn op_fixture() -> Vdp {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0C, 0x08); // S/H on
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10); // SAT base 0x2000
        fill_tile(&mut v, 8, 14); // highlight operator (palette-3 colour 14)
        fill_tile(&mut v, 9, 15); // shadow operator (palette-3 colour 15)
        v
    }

    #[test]
    fn operator_highlight_over_normal_background() {
        let mut v = op_fixture();
        put_cell(&mut v, 0xC000, 0x0001); // plane A low, red (the underlying pixel)
        put_cell(&mut v, 0xE000, 0x8000); // plane B high + transparent → default normal
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x6008, 0x0080); // palette 3, tile 8 (highlight-op), low pri
        let r = v.render_line_report(0);
        assert_eq!(
            r.pixels[0].layer,
            Layer::PlaneA,
            "the operator is not displayed → the underlying plane A shows"
        );
        assert_eq!(r.pixels[0].state, PixelState::Highlight);
        assert_eq!(v.render_line(0)[0], (255, 127, 127), "highlighted red");
    }

    #[test]
    fn operator_highlight_over_shadow_undoes_to_normal() {
        let mut v = op_fixture();
        put_cell(&mut v, 0xC000, 0x0001); // plane A low, red; plane B low + transparent → default shadow
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x6008, 0x0080); // highlight-op
        assert_eq!(
            v.render_line(0)[0],
            (255, 0, 0),
            "shadow + highlight = normal (undo)"
        );
        assert_eq!(v.render_line_report(0).pixels[0].state, PixelState::Normal);
    }

    #[test]
    fn operator_shadow_over_normal_and_over_shadow() {
        let mut v = op_fixture();
        put_cell(&mut v, 0xC000, 0x0001); // plane A low, red
        put_cell(&mut v, 0xE000, 0x8000); // plane B high + transparent → default normal
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x6009, 0x0080); // shadow-op (tile 9)
        assert_eq!(v.render_line(0)[0], (127, 0, 0), "normal + shadow = shadow");
        // Now both planes low → default shadow; shadow-op must not double.
        put_cell(&mut v, 0xE000, 0x0000);
        assert_eq!(
            v.render_line(0)[0],
            (127, 0, 0),
            "shadow + shadow = shadow (no double-shadow)"
        );
    }

    #[test]
    fn low_operator_under_a_high_plane_has_no_effect() {
        let mut v = op_fixture();
        put_cell(&mut v, 0xC000, 0x8001); // plane A HIGH, red → beats the low operator (RR9)
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x6008, 0x0080); // low highlight-op
        let r = v.render_line_report(0);
        assert_eq!(r.pixels[0].layer, Layer::PlaneA);
        assert_eq!(r.pixels[0].state, PixelState::Normal, "unshifted");
        assert_eq!(
            v.render_line(0)[0],
            (255, 0, 0),
            "the operator lost RR9 → no effect"
        );
        // Drop plane A to low (B high + transparent → normal bg): the operator now wins and highlights.
        put_cell(&mut v, 0xC000, 0x0001);
        put_cell(&mut v, 0xE000, 0x8000);
        assert_eq!(
            v.render_line(0)[0],
            (255, 127, 127),
            "with a low plane the operator now fires"
        );
    }

    #[test]
    fn high_operator_fires_over_a_high_plane() {
        let mut v = op_fixture();
        put_cell(&mut v, 0xC000, 0x8001); // plane A HIGH, red → default normal
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0xE008, 0x0080); // HIGH highlight-op (attr bit 15 set)
        assert_eq!(
            v.render_line(0)[0],
            (255, 127, 127),
            "high-sprite > high-A → the operator fires (normal + highlight)"
        );
        assert_eq!(v.render_line_report(0).pixels[0].layer, Layer::PlaneA);
    }

    #[test]
    fn operator_pixels_are_ordinary_colours_when_sh_disabled() {
        let mut v = op_fixture();
        set_reg(&mut v, 0x0C, 0x00); // S/H OFF
        put_cell(&mut v, 0xC000, 0x0001); // plane A low, red
        write_cram(&mut v, 3 * 16 + 14, 0x0E00); // palette-3 colour 14 = blue
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x6008, 0x0080); // palette 3, nibble 14
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(3 * 16 + 14),
            "S/H off → the operator pixel is a normal palette-3 colour on top"
        );
        assert_eq!(v.render_line_report(0).pixels[0].layer, Layer::Sprite(0));
    }

    // --- design §4: pixel_attribution -------------------------------------------------------------------

    #[test]
    fn attribution_rgb_reproduces_render_line() {
        // The §4 invariant, extended to carry the S/H state: for every pixel, the attribution's RGB equals the
        // rendered pixel (attribution is the render, design §1).
        let mut v = op_fixture(); // S/H on, operator tiles present
        put_cell(&mut v, 0xC000, 0x0001); // plane A low red
        put_cell(&mut v, 0xE000, 0x8002); // plane B high blue
        write_sprite(&mut v, 0, 0x0080, 0x0001, 0x0001, 0x0088); // low sprite red at screen 8, link 1
        write_sprite(&mut v, 1, 0x0080, 0x0000, 0x6008, 0x00A0); // highlight-op at screen 32
        let rgb = v.render_line(0);
        for (x, want) in rgb.iter().enumerate() {
            let a = v.pixel_attribution(x as u16, 0);
            assert_eq!(
                a.rgb, *want,
                "attribution RGB reproduces render_line at x={x}"
            );
        }
    }

    #[test]
    fn attribution_candidate_order_and_verdicts() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        put_cell(&mut v, 0xC000, 0x8001); // plane A HIGH red, opaque → the winner
                                          // plane B stays tile 0 → transparent
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0001, 0x0080); // low sprite red, opaque, at screen 0
        let a = v.pixel_attribution(0, 0);
        assert_eq!(a.winner, Layer::PlaneA);
        let layers: Vec<_> = a.candidates.iter().map(|c| c.layer).collect();
        assert_eq!(
            layers,
            vec![
                Layer::PlaneA,    // high-A rank 1 (winner)
                Layer::Sprite(0), // low-sprite rank 3
                Layer::PlaneB,    // low-B rank 5
                Layer::Backdrop,  // rank 6
            ],
            "candidates in RR9 rank order"
        );
        let verdicts: Vec<_> = a.candidates.iter().map(|c| c.verdict).collect();
        assert_eq!(
            verdicts,
            vec![
                CandidateVerdict::Won,
                CandidateVerdict::LostToPriority, // opaque sprite, outranked by high-A
                CandidateVerdict::Transparent,    // plane B has no opaque pixel
                CandidateVerdict::LostToPriority, // backdrop
            ]
        );
    }

    #[test]
    fn attribution_attaches_the_winning_cell() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // Plane A winner: tile 1, hflip (0x0800), high priority (0x8000).
        put_cell(&mut v, 0xC000, 0x8801);
        let a = v.pixel_attribution(0, 0);
        let cell = a.cell.expect("a plane-A winner reports its cell");
        assert_eq!(cell.tile, 1);
        assert!(cell.hflip && cell.priority && !cell.vflip);
        // Sprite winner → no cell.
        put_cell(&mut v, 0xC000, 0x0000); // clear plane A
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x0001, 0x0080);
        assert!(
            v.pixel_attribution(0, 0).cell.is_none(),
            "a sprite winner has no plane cell"
        );
        // Backdrop winner → no cell.
        let empty = pa_fixture(false);
        assert!(empty.pixel_attribution(0, 0).cell.is_none());
        assert_eq!(empty.pixel_attribution(0, 0).winner, Layer::Backdrop);
    }

    #[test]
    fn attribution_reports_operator_shift_and_verdict() {
        let mut v = op_fixture();
        put_cell(&mut v, 0xC000, 0x0001); // plane A low red (underlying)
        put_cell(&mut v, 0xE000, 0x8000); // plane B high + transparent → default normal
        write_sprite(&mut v, 0, 0x0080, 0x0000, 0x6008, 0x0080); // highlight-op
        let a = v.pixel_attribution(0, 0);
        assert_eq!(
            a.winner,
            Layer::PlaneA,
            "the operator is not the displayed winner"
        );
        assert_eq!(
            a.state,
            PixelState::Highlight,
            "it shifted the underlying state"
        );
        assert_eq!(a.rgb, (255, 127, 127));
        let sprite_cand = a
            .candidates
            .iter()
            .find(|c| matches!(c.layer, Layer::Sprite(_)))
            .expect("the operator sprite is a candidate");
        assert_eq!(
            sprite_cand.verdict,
            CandidateVerdict::Operator,
            "the operator outranks the winner but shifted its state instead of displaying"
        );
    }

    // --- R10 slice 4: render_scanline commit path (status bits go real, the masking carry) ----------------

    #[test]
    fn render_scanline_commits_sprite_overflow() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        write_chain(&mut v, 18, 0x0080, 0x00); // 18 on-line 1×1 → per-line sprite-count overflow
        assert_eq!(
            v.status_word(0) & 0x40,
            0,
            "no overflow before render_scanline"
        );
        let r = v.render_scanline(0);
        assert!(r.sprite_overflow);
        assert_eq!(
            v.status_word(0) & 0x40,
            0x40,
            "render_scanline commits overflow to the status latch (bit 6)"
        );
    }

    #[test]
    fn render_scanline_commits_collision() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        write_sprite(&mut v, 0, 0x0080, 0x0001, 0x0001, 0x0080); // link 1, opaque tile 1 at screen 0
        write_sprite(&mut v, 1, 0x0080, 0x0000, 0x0001, 0x0080); // overlaps at screen 0
        assert_eq!(v.status_word(0) & 0x20, 0, "no collision before");
        v.render_scanline(0);
        assert_eq!(
            v.status_word(0) & 0x20,
            0x20,
            "render_scanline commits collision to the status latch (bit 5)"
        );
    }

    #[test]
    fn dot_overflow_carry_makes_the_next_line_first_x_zero_mask() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2);
        set_reg(&mut v, 0x05, 0x10);
        // 10 sprites, 4×2 cells (32 px wide, cover lines 0..15), chained. Sprite 0 is x=0 (first-on-line);
        // the rest are x≠0. On line 0 (carry clear) the x=0 does not mask and the line dot-overflows
        // (10 × 32 px > 256 px budget).
        for i in 0..10u16 {
            let link = if i + 1 < 10 { i + 1 } else { 0 };
            let x = if i == 0 { 0x0000 } else { 128 + 8 + i * 32 };
            write_sprite(&mut v, i as usize, 0x0080, (0x0D << 8) | link, 0x0001, x);
        }
        let r0 = v.render_line_report(0);
        assert_eq!(
            r0.sprites[0].outcome,
            SpriteOutcome::Rendered,
            "line 0: first-on-line x=0 does not mask (carry clear)"
        );
        let committed = v.render_scanline(0);
        assert!(
            committed.sprite_overflow,
            "line 0 dot-overflows the pixel budget"
        );
        // Line 1: the carry advanced by line 0 now seeds masking, so the first-on-line x=0 sprite masks.
        let r1 = v.render_line_report(1);
        assert_eq!(
            r1.sprites[0].outcome,
            SpriteOutcome::Masked,
            "the previous line's dot-overflow carry makes the first-on-line x=0 mask (R10)"
        );
    }
}
