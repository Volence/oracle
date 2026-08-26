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

use crate::state_hash::{CRAM_SIZE, VRAM_SIZE};
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

impl Layer {
    /// Every variant, once. The `Sprite` representative carries slot 0 — nothing that iterates this cares
    /// which slot, because the whole *layer* is what a mask or a name applies to.
    ///
    /// Exists so callers that need "the set of layers" **derive** it instead of transcribing a list; the
    /// exhaustive matches in [`LayerMask::shows`] and [`LayerMask::set`] make a new variant a compile
    /// error there, and `layer_all_lists_every_variant` makes a new variant missing *here* a test failure.
    pub const ALL: [Layer; 5] = [
        Layer::Backdrop,
        Layer::PlaneB,
        Layer::PlaneA,
        Layer::Window,
        Layer::Sprite(0),
    ];

    /// This layer's name as a mask target, or `None` for [`Layer::Backdrop`] — which is the floor the
    /// fall-through ends at, not a layer that can be switched off ([`LayerMask::set`] refuses it).
    ///
    /// The match is exhaustive on purpose, exactly as [`LayerMask::shows`]'s is: a new variant cannot
    /// compile until it declares whether a mask reaches it *and* what to call it. See
    /// [`LayerMask::targets`] for why the vocabulary lives in the core rather than beside the bus handler
    /// that first needed it.
    pub const fn mask_key(self) -> Option<&'static str> {
        match self {
            Layer::Backdrop => None,
            Layer::PlaneB => Some("planeB"),
            Layer::PlaneA => Some("planeA"),
            Layer::Window => Some("window"),
            Layer::Sprite(_) => Some("sprites"),
        }
    }
}

/// **A display mask: which layers are allowed to win a pixel.** Not machine state — see the module note
/// below, and [`Vdp::resolve_line_masked`] for where it is applied.
///
/// # It suppresses CANDIDACY, it does not blank output
///
/// A masked layer is removed from the RR9 priority contest *before* a winner is chosen, so whatever is
/// behind it shows through and the fall-through ends at the backdrop. Blanking the winner after the fact
/// would paint backdrop over dots plane B was visible at — a wrong answer that looks right on a screenshot
/// of a simple scene, which is why the mask lives inside [`Vdp::rr9_winner`] and nowhere downstream of it.
///
/// # It does not perturb the machine
///
/// This type is a **parameter**, never a field: no `Vdp` and no `System` holds one, so it is in no
/// snapshot, no `state_hash`, and nothing a reset or a restore can carry or drop. The only stateful render
/// — [`Vdp::render_scanline`], which commits the sprite-overflow / collision latches and the R10 masking
/// carry the ROM itself polls — takes no mask and has no masked twin. Sprite *evaluation* runs identically
/// under every mask (`resolve_line_masked` calls `sprite_line` before consulting the mask at all), so
/// masking `sprites` hides them from the picture and changes nothing the game can observe.
///
/// # The backdrop is not a mask target
///
/// It is the floor the fall-through ends at, not a layer that can be switched off; [`LayerMask::set`]
/// refuses it, and [`LayerMask::shows`] answers `true` for it always.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LayerMask {
    pub plane_a: bool,
    pub plane_b: bool,
    pub window: bool,
    pub sprites: bool,
}

impl Default for LayerMask {
    fn default() -> Self {
        Self::ALL
    }
}

impl LayerMask {
    /// Every layer drawn — the state that makes every render path byte-identical to an unmasked build.
    pub const ALL: Self = LayerMask {
        plane_a: true,
        plane_b: true,
        window: true,
        sprites: true,
    };

    /// Is `layer` allowed to win a pixel? The backdrop always is.
    ///
    /// The match is exhaustive on purpose: a new [`Layer`] variant cannot compile until it says whether a
    /// mask reaches it.
    pub const fn shows(self, layer: Layer) -> bool {
        match layer {
            Layer::Backdrop => true,
            Layer::PlaneB => self.plane_b,
            Layer::PlaneA => self.plane_a,
            Layer::Window => self.window,
            Layer::Sprite(_) => self.sprites,
        }
    }

    /// Set one layer's mask bit. Returns whether `layer` is a mask target at all — `false` for
    /// [`Layer::Backdrop`], which leaves the mask untouched rather than pretending to have applied.
    pub fn set(&mut self, layer: Layer, enabled: bool) -> bool {
        let slot = match layer {
            Layer::Backdrop => return false,
            Layer::PlaneB => &mut self.plane_b,
            Layer::PlaneA => &mut self.plane_a,
            Layer::Window => &mut self.window,
            Layer::Sprite(_) => &mut self.sprites,
        };
        *slot = enabled;
        true
    }

    /// Is every layer drawn? The predicate every caller uses to decide whether a masked render is needed
    /// at all, so the unmasked path stays exactly the code that ran before this type existed.
    pub const fn is_all(self) -> bool {
        self.plane_a && self.plane_b && self.window && self.sprites
    }

    /// **The mask vocabulary, derived.** Every [`Layer`] that is a mask target, paired with its name, in
    /// [`Layer::ALL`] order.
    ///
    /// This is the single source for every place the four names appear, on **both** sides of the process:
    /// the bus's `emulator/get_layer_states` key set, `emulator/set_layer_enabled`'s accepted values, its
    /// refusal message, the screenshot/scanlines caveat — and, since the player window grew its own
    /// toggles, the palette entries and the on-screen badge that says a mask is on. It lives here rather
    /// than in `oracle-aether` for exactly that reason: the panel and the wire naming a layer differently
    /// is the same drift class as the panel and the wire *resolving* a dot differently, and one of those
    /// was already fixed by moving the derivation down here (see [`sprite_tile_at`]).
    ///
    /// `oracle-aether`'s `tests/layers.rs::the_mask_vocabulary_is_the_contract_fragments_own` proves what
    /// this produces equals the vendored contract fragment's enum, in both directions — so the frontend's
    /// vocabulary is contract-pinned by construction rather than by transcription.
    pub fn targets() -> Vec<(&'static str, Layer)> {
        Layer::ALL
            .into_iter()
            .filter_map(|l| l.mask_key().map(|n| (n, l)))
            .collect()
    }

    /// The names of the layers this mask **hides**, in [`Layer::ALL`] order. Empty when nothing is hidden.
    ///
    /// One derivation for two audiences: the bus's caveat (*"you are looking at a masked picture"*) and the
    /// player's standing on-screen badge. Neither can name a layer the mask does not actually hide, and
    /// neither can miss one, because both read this.
    pub fn hidden(self) -> Vec<&'static str> {
        Self::targets()
            .into_iter()
            .filter(|(_, l)| !self.shows(*l))
            .map(|(name, _)| name)
            .collect()
    }
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

/// One row held back for deferred emission: the [`LineReport`] `render_scanline` already built, the CRAM as
/// it stood at that row's own line start, and the sub-line write journal for the row.
///
/// Retained by [`ScanlineScaffold`]; see its docs for why none of this is machine state.
#[derive(Clone)]
pub(crate) struct RetainedRow {
    /// The resolved row, exactly as `render_scanline` returned it — never re-resolved (re-resolving after
    /// the sprite latch commit would reseed the R10 masking carry and could change the sprites, see
    /// [`Vdp::report_rgb`]).
    pub(crate) report: LineReport,
    /// CRAM as of this row's line start — the image [`report_rgb_with_cram`] decodes the row against, and
    /// the base the journal's landings are replayed on top of. Inline and fixed-size: CRAM *is* a
    /// [`CRAM_SIZE`]-byte image, so the array shape makes a wrong-length snapshot unrepresentable and costs
    /// no allocation on the per-active-line stash path.
    pub(crate) cram: [u8; CRAM_SIZE],
    /// The sub-line CRAM journal for this row: every CRAM write that landed inside the row's own line, in
    /// write order, each already resolved to the pixel it takes effect at.
    ///
    /// Empty ⇒ the row decodes whole against `cram`, which is byte-for-byte what `report_rgb` produced at
    /// line start — the identity slice 3 exists to guarantee and slice 4 must not disturb.
    pub(crate) journal: Vec<CramLanding>,
}

/// One CRAM write that landed inside a row's own scanline (`F-SCANLINE-SUBLINE` slice 4).
///
/// `x` is resolved at journal time by [`crate::vdp::subline_x`] from the write's own master clock and the
/// **resolved row's** H40 flag (decision B-2 — a mid-line mode switch must not place the landing on a grid
/// the row was never drawn on). Pixels `0..x` keep the pre-write colour; `x..width` show the new one. A
/// landing in the line's blanking tail resolves to `x == width`: it colours no pixel of this row, but it is
/// still journalled, because the working-CRAM guard at emit time accounts for **every** write of the line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct CramLanding {
    /// First active-display pixel that shows the new colour (`0..=width`).
    pub(crate) x: usize,
    /// CRAM byte address of the written word — even, `0..CRAM_SIZE`.
    pub(crate) addr: usize,
    /// The 9-bit-masked colour word, as the VDP stored it.
    pub(crate) word: u16,
}

/// The deferred-emission scaffolding for the opt-in per-scanline capture (`F-SCANLINE-SUBLINE`,
/// **decision D-1**): at most one [`RetainedRow`], held from the instant its line was resolved until the
/// next line's `Scanline` event emits it.
///
/// # This is render scaffolding, not machine state — and the type enforces it
///
/// [`crate::system::System`] derives `PartialEq`/`Eq` **and** `bincode::Encode`/`Decode`, and the tree's
/// whole-machine neutrality claims (`tests/scanline_capture.rs`'s `frame_boundary_is_state_neutral`, the
/// determinism gate, every `assert_eq!(plain, tapped)`) compare the machine *with* an attached capture
/// against one without. A plain field would make an armed run that ends mid-frame leave residue and quietly
/// falsify all of them. So, exactly as `vdp.rs` already rules for render output (*"the rendering output
/// stays derived, not state — nothing render-related serializes"*):
///
/// - [`PartialEq`] is **constant true**, so two machines are never unequal because of a retained row;
/// - [`bincode::Encode`] writes **zero bytes** and [`bincode::Decode`] reads none, so the checkpoint byte
///   format is unchanged and a snapshot taken before this type existed still restores;
/// - `System::reset` drops it (it rebuilds the whole struct from `System::new`), matching
///   `Engine::invalidate_screen`'s rule that reset/reload/restore drop the retained frame.
///
/// It must nonetheless live in `System` and **persist across runs** (decision D-2): a `run_until` that ends
/// mid-frame would otherwise drop the pending row, leaving the frame one row short with no signal. The
/// converse of D-2 is a real property of the public API: a caller who interleaves an *unarmed* run between
/// two armed ones loses exactly the one row the first armed run was still holding, because the unarmed run
/// drops it. Nothing in this tree does that (every run-driver that carries a capture carries it on every
/// run), but a caller alternating `run_frames` with `run_frames_with_sink(…, capture)` mid-frame would see
/// a one-row gap.
///
/// # The blindness is **one-directional** — this is the part that is easy to misread
///
/// "A retained row cannot make two machines unequal" does **not** mean two equal machines produce the same
/// rows. It means exactly the reverse implication and no more:
///
/// - Two machines that compare `==`, hash the same under `state_hash`, and serialize to identical snapshot
///   bytes may still emit **different row streams** from the same future run — one may be holding a row the
///   other is not, and that row is emitted at the next `Scanline` event.
/// - Therefore `snapshot`/`restore` is **not** row-stream-neutral: a restore drops at most one pending row,
///   so a run resumed from a checkpoint taken mid-frame emits one fewer row than the run it was cut from.
///   Accepted by design — the alternative is putting render output in the checkpoint, which is the thing
///   `vdp.rs` rules out — and bounded at one row, which no whole-frame consumer can observe (`run_frames`
///   ends on a boundary, where nothing is pending).
#[derive(Clone, Default)]
pub(crate) struct ScanlineScaffold {
    pending: Option<RetainedRow>,
}

impl ScanlineScaffold {
    /// Hold `report` back for emission at the next line's event, alongside the CRAM image live right now
    /// (this row's line start) and an empty journal.
    ///
    /// Panics if `cram` is not a whole [`CRAM_SIZE`] image — the only caller passes `Vdp::cram()`, which is
    /// that by construction.
    pub(crate) fn stash(&mut self, report: LineReport, cram: &[u8]) {
        self.pending = Some(RetainedRow {
            report,
            cram: cram.try_into().expect("CRAM is a whole CRAM_SIZE image"),
            journal: Vec::new(),
        });
    }

    /// Take the retained row, if any.
    pub(crate) fn take(&mut self) -> Option<RetainedRow> {
        self.pending.take()
    }

    /// Journal one CRAM write against the row currently retained, at the pixel `d_mclk` places it
    /// (`F-SCANLINE-SUBLINE` slice 4). A no-op when no row is retained — CRAM writes on non-active lines
    /// simply carry into the next line-start snapshot, which is exactly today's behaviour.
    ///
    /// **Coalescing by pixel is mandatory, not an optimisation.** `direct_color_dma` pushes 44,352 CRAM
    /// words inside a single instruction, and under decision C-6 they all share one master clock and so one
    /// `x`. Without this, that is a ~700 KB per-line journal and 44 k zero-length decode spans. Landings at
    /// the same pixel are one segment, and within a segment a later write to the same CRAM address simply
    /// **replaces** the earlier one — the pixel can only ever show the last colour written at it — so the
    /// burst above collapses to a single entry.
    ///
    /// The scan walks backwards only over the current pixel's group: `x` is non-decreasing (master clocks
    /// are, within a line), so equal-`x` entries are contiguous at the tail.
    ///
    /// Takes the write's **absolute** master clock, not a pre-reduced in-line offset, so this can check the
    /// one thing neither the pixel-order assert nor the emit-time working-CRAM guard can see: that the write
    /// belongs to the row it is being journalled against. A write stamped on a *different* line but reduced
    /// mod `MCLK_PER_LINE` lands at a plausible-looking `x` in the wrong row — the working-CRAM guard still
    /// passes (every write is accounted for, just filed under the wrong row) and the picture is silently
    /// wrong. Doing the reduction here is what makes that class visible.
    pub(crate) fn journal_cram(&mut self, mclk: u64, addr: usize, word: u16) {
        let Some(row) = self.pending.as_mut() else {
            return;
        };
        debug_assert_eq!(
            (mclk / crate::vdp::MCLK_PER_LINE) % crate::vdp::LINES_PER_FRAME,
            u64::from(row.report.line),
            "a CRAM write stamped on one line was journalled against another row's decode — reducing the \
             clock mod the line would have hidden it at a plausible x"
        );
        let d_mclk = mclk % crate::vdp::MCLK_PER_LINE;
        let x = crate::vdp::subline_x(d_mclk, row.report.h40);
        debug_assert!(
            row.journal.last().is_none_or(|l| l.x <= x),
            "landings arrive in pixel order — the coalescing tail-scan and the segmented decode both rely \
             on it, and a backwards x means a write was stamped outside the row's own line"
        );
        for l in row.journal.iter_mut().rev() {
            if l.x != x {
                break;
            }
            if l.addr == addr {
                l.word = word;
                return;
            }
        }
        row.journal.push(CramLanding { x, addr, word });
    }

    /// Drop the retained row.
    ///
    /// Called at the start of a run whose sink does **not** want rows, so such a run drops whatever a
    /// previous run left retained. Note what that does *not* say: two different scanline-wanting sinks run
    /// back to back **do** hand the row across, because the second run is armed and D-2 says an armed run
    /// inherits. That is reachable in this tree — the aether engine runs its own `Fanout` capture while the
    /// player's loop runs `cap` — and is accepted: the row is a faithful render of the machine both sinks
    /// are watching, and its line number is carried with it.
    pub(crate) fn clear(&mut self) {
        self.pending = None;
    }

    /// The line number of the retained row, if one is held — the non-vacuity handle for the neutrality
    /// tests (a "retained state is invisible" claim is worth nothing if nothing was retained). Test-only on
    /// purpose: no *machine* state may be derived from whether a row is pending (the emitter's own
    /// "is there one to flush?" test is sink-facing and changes nothing the machine can see).
    #[cfg(test)]
    pub(crate) fn pending_line(&self) -> Option<u16> {
        self.pending.as_ref().map(|r| r.report.line)
    }
}

/// Constant-true: a retained row is not machine state, so it can never make two machines unequal (D-1).
impl PartialEq for ScanlineScaffold {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for ScanlineScaffold {}

/// Encodes as **zero bytes** (D-1) — the checkpoint byte format is unchanged by this field's existence.
impl bincode::Encode for ScanlineScaffold {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        _encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        Ok(())
    }
}

/// Decodes from **zero bytes** (D-1) — a snapshot taken before this field existed still restores, and the
/// restored machine starts with no retained row (the same state `reset` leaves).
impl<Ctx> bincode::Decode<Ctx> for ScanlineScaffold {
    fn decode<D: bincode::de::Decoder<Context = Ctx>>(
        _decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self::default())
    }
}

bincode::impl_borrow_decode!(ScanlineScaffold);

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

/// Decode one CRAM index (0..=63) to RGB at `state`, against an **explicit CRAM byte image** rather than a
/// live [`Vdp`]. The 9-bit colour is three 3-bit channels (`B<<9 | G<<5 | R<<1`, big-endian); each is run
/// through [`intensity`].
///
/// This is the single CRAM decode in the tree: `Vdp::cram_rgb_state` passes its own live CRAM, and
/// [`report_rgb_with_cram`] passes a retained line-start snapshot (`F-SCANLINE-SUBLINE` deferred emission).
/// One arithmetic, so the live path and the snapshot path cannot drift — the same reason
/// [`Vdp::render_line`] and [`Vdp::report_rgb`] share `pixels_rgb`.
fn cram_rgb_state_from(cram: &[u8], index: u8, state: PixelState) -> (u8, u8, u8) {
    let i = (index as usize & 0x3F) * 2;
    let word = ((cram[i] as u16) << 8) | cram[i + 1] as u16;
    (
        intensity(((word >> 1) & 0x07) as u8, state),
        intensity(((word >> 5) & 0x07) as u8, state),
        intensity(((word >> 9) & 0x07) as u8, state),
    )
}

/// Decode an already-built [`LineReport`]'s pixels to RGB against `cram` — [`Vdp::report_rgb`]'s exact map
/// (`cram_rgb_state_from` per winning index/state), but reading a caller-supplied CRAM image instead of the
/// VDP's live one.
///
/// This is what lets the row be *emitted* one line after it was *resolved* without the emitted bytes moving:
/// the resolve stage is index-domain and never reads CRAM, so decoding the retained pixels against the CRAM
/// that was live at the row's own line start reproduces `report_rgb`'s answer byte for byte
/// (`docs/2026-08-19-subline-recon.md` §0, §A(ii)).
pub(crate) fn report_rgb_with_cram(cram: &[u8], report: &LineReport) -> Vec<(u8, u8, u8)> {
    // A short-but-nonempty CRAM would decode low indices silently and only panic on a high one, so the
    // whole-image contract is asserted rather than left to the index bounds. `RetainedRow.cram` is a
    // `[u8; CRAM_SIZE]` and so cannot violate it; this covers the `&[u8]` seam itself.
    debug_assert_eq!(
        cram.len(),
        CRAM_SIZE,
        "the decode reads a whole CRAM image, not a fragment"
    );
    report
        .pixels
        .iter()
        .map(|p| cram_rgb_state_from(cram, p.cram_index, p.state))
        .collect()
}

/// Decode a retained row to RGB, **segmented at each of its CRAM landings** (`F-SCANLINE-SUBLINE` slice 4)
/// — the emitter proper. Returns the finished row *and* the CRAM image the walk ended on, which the caller
/// checks against live CRAM: that equality is the guard that every write of the line reached the journal.
///
/// The segments partition `0..width`, so **each pixel is decoded exactly once** — this is not a re-render
/// and not a re-resolve (re-resolving would reseed the R10 masking carry, see [`Vdp::report_rgb`]). The
/// resolve stage is index-domain and never reads CRAM (recon §0), so splitting the *decode* is the whole of
/// the mechanism.
///
/// With an empty journal this is exactly [`report_rgb_with_cram`] against the line-start snapshot — the
/// same bytes a line-atomic emitter produced, by construction rather than by measurement.
pub(crate) fn row_rgb(row: &RetainedRow) -> (Vec<(u8, u8, u8)>, [u8; CRAM_SIZE]) {
    if row.journal.is_empty() {
        return (report_rgb_with_cram(&row.cram, &row.report), row.cram);
    }
    let px = &row.report.pixels;
    let mut cram = row.cram;
    let mut out = Vec::with_capacity(px.len());
    let mut k = 0usize;
    while k < row.journal.len() {
        // Decode the span that still shows the CRAM in force, up to this landing's pixel...
        let x = row.journal[k].x.min(px.len());
        while out.len() < x {
            let p = &px[out.len()];
            out.push(cram_rgb_state_from(&cram, p.cram_index, p.state));
        }
        // ...then apply every landing at that same pixel (one segment, decision C-6 / the coalescing rule).
        let at = row.journal[k].x;
        while let Some(l) = row.journal.get(k).filter(|l| l.x == at) {
            cram[l.addr] = (l.word >> 8) as u8;
            cram[l.addr | 1] = (l.word & 0xFF) as u8;
            k += 1;
        }
    }
    while out.len() < px.len() {
        let p = &px[out.len()];
        out.push(cram_rgb_state_from(&cram, p.cram_index, p.state));
    }
    (out, cram)
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

/// Which VRAM pattern sprite `s` draws screen dot (`x`, `y`) from, or `None` if the dot is outside the
/// sprite's box.
///
/// This is the same addressing [`Vdp::draw_sprite`] uses, hoisted out so the *one* derivation serves every
/// consumer: flips mirror the whole sprite (not each cell), and a multi-cell sprite's patterns run
/// **column-major** — down a column before moving right — so the offset from the base tile is
/// `(col * height_cells) + row` (recon RR8). The addition wraps, because a base tile near the top of VRAM
/// with a large sprite genuinely does wrap there.
///
/// `None` is the honest answer for a dot the sprite's box no longer contains — which, asked *after* the
/// frame was drawn, means the SAT moved since. Callers report the absence rather than inventing a tile.
pub fn sprite_tile_at(s: &SpriteDecoded, x: u16, y: u16) -> Option<u16> {
    let wpx = usize::from(s.width_cells) * 8;
    let hpx = usize::from(s.height_cells) * 8;
    let sx = usize::try_from(i32::from(x) - i32::from(s.x)).ok()?;
    let sy = usize::try_from(i32::from(y) - i32::from(s.y)).ok()?;
    if sx >= wpx || sy >= hpx {
        return None;
    }
    let src_sx = if s.hflip { wpx - 1 - sx } else { sx };
    let src_sy = if s.vflip { hpx - 1 - sy } else { sy };
    let offset = (src_sx / 8) * usize::from(s.height_cells) + src_sy / 8;
    Some(s.tile.wrapping_add(offset as u16))
}

impl Vdp {
    /// H40 (40-cell / 320 px) mode: reg $0C bits RS0 (bit 0) + RS1 (bit 7) both set (recon RR3, matching the
    /// timing FSM's `h40`). Recomputed from `regs()` so the renderer never reaches into private VDP state.
    fn render_h40(&self) -> bool {
        self.regs()[0x0C] & 0x81 == 0x81
    }

    /// The active display geometry `(width, height)` in pixels, as the renderer is currently configured:
    /// **320 × 224** in H40, **256 × 224** in H32.
    ///
    /// Exported so a caller that has to *bound* a coordinate — a bus method refusing a dot outside the
    /// display — gets the same answer the renderer resolves against, instead of re-deriving `render_h40`
    /// on its own. Width is the length [`Vdp::render_line`] returns; the two cannot drift.
    ///
    /// Height is 224 unconditionally, which is a statement about this core rather than about the chip:
    /// the whole machine is NTSC V28 (`vdp::LINES_PER_FRAME`, the line-224 VBlank anchor, the scheduler's
    /// active-line chain), so reporting 240 off reg $01's M2 bit would name a geometry nothing here
    /// renders. When V30 lands, it lands here.
    pub fn active_display(&self) -> (u16, u16) {
        (if self.render_h40() { 320 } else { 256 }, 224)
    }

    /// How many SAT slots the hardware actually parses in the current mode: **80** in H40, **64** in H32
    /// (recon R10 / RR8, via [`sprite_limits`]).
    ///
    /// Exported for the same reason [`Vdp::active_display`] is, one field over: [`Vdp::sprites_decoded`]
    /// decodes all 80 slots unconditionally, so a caller reporting how many of them are *real* must get
    /// the number from the same place the sprite walk gets it, instead of re-deriving `render_h40` and a
    /// `64 or 80` of its own. The bus method `emulator/sprites` reports this as `parsedMax`, and the
    /// contract (§11.10) forbids it computing the value itself for exactly this reason.
    pub fn parsed_sprite_max(&self) -> u8 {
        sprite_limits(self.render_h40()).2 as u8
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

    /// Decode one CRAM index (0..=63) to RGB at the given shadow/highlight `state` (recon R11.5), against
    /// this VDP's **live** CRAM. Thin wrapper over [`cram_rgb_state_from`] so the live decode and the
    /// snapshot decode are the same arithmetic and cannot drift.
    fn cram_rgb_state(&self, index: u8, state: PixelState) -> (u8, u8, u8) {
        cram_rgb_state_from(self.cram(), index, state)
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
    ///
    /// **`mask` removes a layer from the contest, it does not blank the result.** A masked layer is skipped
    /// exactly where a transparent one is, so the next candidate down wins and the fall-through still ends
    /// at the backdrop — see [`LayerMask`] for why post-hoc blanking would be the wrong answer here.
    #[allow(clippy::too_many_arguments)]
    fn rr9_winner(
        &self,
        x: usize,
        backdrop: u8,
        s: Option<SpritePixel>,
        a: &PlanePixel,
        a_layer: Layer,
        b: &PlanePixel,
        mask: LayerMask,
    ) -> PixelResolution {
        let sprites = mask.sprites;
        let a_shown = mask.shows(a_layer);
        let b_shown = mask.shows(Layer::PlaneB);
        // High-priority tier.
        if let Some(sp) = s {
            if sp.priority && sprites {
                return sprite_px_res(x, &sp);
            }
        }
        if a.opaque() && a.priority && a_shown {
            return px_from(x, a_layer, a);
        }
        if b.opaque() && b.priority && b_shown {
            return px_from(x, Layer::PlaneB, b);
        }
        // Low-priority tier (sprite buffer pixels are always opaque).
        if let Some(sp) = s {
            if sprites {
                return sprite_px_res(x, &sp);
            }
        }
        if a.opaque() && a_shown {
            return px_from(x, a_layer, a);
        }
        if b.opaque() && b_shown {
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
    ///
    /// **The mask reaches the winner and stops there.** `sh_state` is handed the *unmasked* `a`, `b` and `s`
    /// deliberately: R11's default is derived from the planes' priority bits, and re-deriving it from a
    /// masked-away plane would change the intensity of the layers still on screen. Masking plane A must
    /// reveal plane B in the colour plane B already had, not in a darker one — the mask decides what is
    /// drawn, never how a surviving pixel looks. A masked sprite takes its operator with it: `winner.layer`
    /// is then never `Sprite`, so the shift below is not applied, which is the same answer as the sprite
    /// not being there.
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
        mask: LayerMask,
    ) -> PixelResolution {
        let winner = self.rr9_winner(x, backdrop, s, a, a_layer, b, mask);
        if sh {
            if let Layer::Sprite(_) = winner.layer {
                if let Some(op) = s.and_then(|sp| sp.operator()) {
                    // The operator wins the sprite slot: display the background beneath it, shifted.
                    let mut under = self.rr9_winner(x, backdrop, None, a, a_layer, b, mask);
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
        self.resolve_line_masked(line, LayerMask::ALL)
    }

    /// [`resolve_line`](Self::resolve_line) with a display [`LayerMask`]. `LayerMask::ALL` is that function
    /// exactly — the mask reaches only `resolve_dot`'s candidate tests, so an all-on mask leaves every
    /// comparison it makes true and the code path identical.
    ///
    /// **Sprite evaluation happens above the mask and is never gated by it.** `sprite_line` runs first and
    /// its result — the walk, the per-sprite outcomes, `overflow`, `collision`, `dot_overflow` — is returned
    /// whole no matter what the mask says, because those are the bits the ROM polls through the VDP status
    /// register. Masking `sprites` may only change the picture; a mask that changed the sprite pipeline
    /// would make the machine behave differently under the instrument watching it.
    fn resolve_line_masked(&self, line: u16, mask: LayerMask) -> ResolvedLine {
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
                self.resolve_dot(sh, x, backdrop, s, &a, a_layer, &b, mask)
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
        self.pixels_rgb(&self.resolve_line(line).pixels)
    }

    /// [`render_line`](Self::render_line) with a display [`LayerMask`] — the masked picture, composited
    /// rather than blanked. `LayerMask::ALL` is `render_line` exactly.
    pub fn render_line_masked(&self, line: u16, mask: LayerMask) -> Vec<(u8, u8, u8)> {
        self.pixels_rgb(&self.resolve_line_masked(line, mask).pixels)
    }

    /// The one CRAM decode map from resolved pixels to RGB (winning index at the resolved shadow/highlight
    /// state) — shared by [`Vdp::render_line`] and [`Vdp::report_rgb`] so the two cannot drift.
    fn pixels_rgb(&self, pixels: &[PixelResolution]) -> Vec<(u8, u8, u8)> {
        pixels
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

    /// [`render_line_report`](Self::render_line_report) with a display [`LayerMask`].
    ///
    /// The split inside the returned report is the point: `pixels` is the *masked* composite, while
    /// `sprites`, `sprite_walk_end`, `sprite_overflow` and `sprite_collision` are the unmasked pipeline's,
    /// because a display mask must not move the bits the game reads. `mask_never_moves_the_sprite_pipeline`
    /// pins exactly that.
    pub fn render_line_report_masked(&self, line: u16, mask: LayerMask) -> LineReport {
        let resolved = self.resolve_line_masked(line, mask);
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
    ///
    /// **A masked layer is not in the list at all.** The list's contract is *"every layer that could have
    /// shown at this dot"*, and a masked layer could not: it never entered the contest `winner_layer` came
    /// out of. Omitting it is also the only answer the closed `verdict` vocabulary admits — `won` is false,
    /// `lostToPriority` names a reason that did not happen, `transparent` misreports opaque art, and
    /// `operator` means a sprite operator. So the mask suppresses the candidate rather than inventing a
    /// verdict for it, which keeps this list and [`Vdp::rr9_winner`]'s fall-through the same set of layers.
    #[allow(clippy::too_many_arguments)]
    fn dot_candidates(
        &self,
        backdrop: u8,
        s: Option<SpritePixel>,
        a: &PlanePixel,
        a_layer: Layer,
        b: &PlanePixel,
        winner_layer: Layer,
        mask: LayerMask,
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
        list.retain(|c| mask.shows(c.1));
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
        self.pixel_attribution_masked(x, y, LayerMask::ALL)
    }

    /// [`pixel_attribution`](Self::pixel_attribution) under a display [`LayerMask`].
    ///
    /// It reports **what was drawn, never what would have won**: `winner` is the post-mask winner, `rgb`
    /// still equals `render_line_masked(y)[x]` under the same mask, and a masked layer is absent from
    /// `candidates` entirely (see [`Vdp::dot_candidates`]). Answering with the layer a mask suppressed
    /// would make this method disagree with the picture every other surface shows — the one failure a
    /// pixel-attribution surface exists to rule out.
    pub fn pixel_attribution_masked(&self, x: u16, y: u16, mask: LayerMask) -> PixelAttribution {
        let h40 = self.render_h40();
        let width = if h40 { 320 } else { 256 };
        let xi = x as usize;
        let backdrop = self.backdrop_index();
        let resolved = self.resolve_line_masked(y, mask);
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
                mask,
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

    /// Decode an already-built [`LineReport`]'s pixels to RGB — the exact map [`Vdp::render_line`] applies
    /// (`cram_rgb_state` per winning index/state), but from the report a [`Vdp::render_scanline`] call
    /// already produced, so a caller holding that report (the `Scanline` event's opt-in capture sink) does
    /// not re-resolve the line. Re-resolving after `render_scanline` would be wrong as well as wasteful: the
    /// committed dot-overflow carry would reseed the R10 masking and could change the sprites.
    ///
    /// Deliberately expressed as [`report_rgb_with_cram`] against this VDP's live CRAM rather than as a
    /// second call site of `pixels_rgb`: the deferred emitter (`F-SCANLINE-SUBLINE`) calls the same function
    /// with a retained line-start snapshot, so "decoding later against the snapshot equals decoding now
    /// against live CRAM" is one function applied to two arguments, not two functions that agree today.
    ///
    /// **Do not call this on a *retained* report.** Since the emitter defers a row by one line, "live CRAM"
    /// at flush time is a line too late, and this method would decode the row against it — measurably: that
    /// substitution is exactly the mutation that moves the `scanline_goldens` scorecard. It has no
    /// production caller left; it exists as the live-decode companion for callers holding a report they
    /// resolved *now* (tests, `render_line_report` users).
    pub fn report_rgb(&self, report: &LineReport) -> Vec<(u8, u8, u8)> {
        report_rgb_with_cram(self.cram(), report)
    }

    /// Render one scanline **and commit** its sprite latches (recon R10) — the stateful per-line advance that
    /// makes the sprite-overflow / collision status bits and the masking carry "go real". Unlike the pure
    /// `render_line` / `render_line_report`, this takes `&mut self`: it seeds masking from the current
    /// `sprite_dot_overflow_carry`, then commits the new carry (this line's dot overflow), ORs the
    /// sprite-overflow / collision status latches (sticky until a status read clears them), and returns the
    /// same [`LineReport`]. This is the hook the eventual per-frame render loop / push-5 golden-frame
    /// differential drives; it is **not** wired into `System::run` this push (so the export golden is
    /// untouched — the test ROM drives no rendering).
    ///
    /// **It takes no [`LayerMask`], and it deliberately has no masked twin.** This is the one render that
    /// writes to the chip, so keeping the mask out of its signature is what makes "a display mask cannot
    /// perturb emulation" a property of the type system rather than a promise in a comment: there is no
    /// argument to thread, so no caller can reach the sprite-latch commit through a mask.
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

    /// A powered-on VDP **in Mode 5**. Every fixture below programs Mode-5-only state (H40, the window,
    /// plane bases, autoincrement), so it must declare M5 (reg 1 bit 2); a bare `power_on` leaves reg 1 at
    /// `$00`, which is Mode 4, where registers above 10 are not writable (see `Vdp::write_register`).
    fn fresh() -> Vdp {
        let mut v = Vdp::power_on(&mut SplitMix64::new(1));
        v.control_write(0x8104, 0); // reg 1 = $04 → M5 set (mode 5)
        v
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
        set_reg(&mut v, 0x01, 0x44); // display enable (reg 1 bit 6) + M5 (bit 2)
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

    /// The deferred emitter's whole neutrality argument, at the decode seam (`F-SCANLINE-SUBLINE` slice 3):
    /// decoding a row against a **retained CRAM snapshot** yields the same bytes as decoding it against live
    /// CRAM at the instant the snapshot was taken — and, crucially, a *different* answer once CRAM has moved
    /// on. The second half is what makes the snapshot load-bearing rather than decoration: an emitter that
    /// lazily read live CRAM one line later would pass the first assertion and fail the second.
    #[test]
    fn a_retained_cram_snapshot_decodes_a_row_exactly_as_live_cram_did() {
        let mut v = pb_fixture(false);
        for i in 0..4 {
            v.vram_mut()[2 * 32 + i] = 0x12;
        }
        write_cram(&mut v, 1, 0x000E); // red
        write_cram(&mut v, 2, 0x0E00); // blue
        put_cell(&mut v, 0xE000, 0x0002);
        let report = v.render_line_report(0);
        let live = v.report_rgb(&report);
        let snapshot = v.cram().to_vec();
        assert_eq!(
            report_rgb_with_cram(&snapshot, &report),
            live,
            "the snapshot decode IS the live decode at the instant it was taken"
        );

        // CRAM moves on, as it would between the row's line start and the next line's event.
        write_cram(&mut v, 2, 0x00E0); // green
        assert_ne!(
            v.report_rgb(&report),
            live,
            "non-vacuity: live CRAM really did change the row's colours"
        );
        assert_eq!(
            report_rgb_with_cram(&snapshot, &report),
            live,
            "and the snapshot decode is unmoved — this is why the one-line emission lag is invisible"
        );
    }

    /// `build_cram_midframe`'s scene, at core level: every plane pixel transparent and the backdrop pointed
    /// at CRAM entry 1, so **every pixel of every row samples index 1** and the colour *is* the picture.
    /// That is what makes a split row unmistakable — and it is the same trap-free shape the fixture ROM uses.
    fn backdrop_fixture() -> Vdp {
        let mut v = pb_fixture(false); // H32: 256 px at 10 mclk/px
        set_reg(&mut v, 0x07, 0x01); // backdrop = CRAM entry 1
        v
    }

    /// Build a retained row from `v`'s line 0 plus a journal, the way the run loop does: resolve the row,
    /// snapshot CRAM, then feed landings through the coalescing push.
    fn retained(v: &Vdp, landings: &[(u64, usize, u16)]) -> RetainedRow {
        let mut sc = ScanlineScaffold::default();
        sc.stash(v.render_line_report(0), v.cram());
        for &(d_mclk, addr, word) in landings {
            sc.journal_cram(d_mclk, addr, word); // line 0, so the absolute clock IS the in-line offset
        }
        sc.take().expect("a row was stashed")
    }

    /// **The behaviour slice, at core level** (`F-SCANLINE-SUBLINE` slice 4): a CRAM write inside the row's
    /// own line splits it — colour A up to the landing pixel, colour B from it on, with **exactly one**
    /// transition. This is the poison a line-atomic emitter cannot pass in either direction: it renders the
    /// row wholly A (today's behaviour) or, if it lazily read live CRAM, wholly B.
    #[test]
    fn a_cram_landing_inside_the_line_splits_the_row_at_its_pixel() {
        let mut v = backdrop_fixture();
        write_cram(&mut v, 1, 0x000E); // colour A = red
        let a = v.cram_rgb(1);

        // A write 1000 mclk into the line lands at pixel 100 (1000 / 10), inside the active window.
        const D_MCLK: u64 = 1000;
        const B_WORD: u16 = 0x0E00; // colour B = blue
        let row = retained(&v, &[(D_MCLK, 2, B_WORD)]);
        let width = row.report.pixels.len();
        let (rgb, _) = row_rgb(&row);

        let b = rgb[width - 1];
        assert_eq!(
            rgb[0], a,
            "the row opens on the colour that was live at its line start"
        );
        assert_ne!(
            b, a,
            "and closes on the colour the mid-line write installed"
        );
        let x = crate::vdp::subline_x(D_MCLK, false);
        assert_eq!(x, 100, "the mapping places this write at pixel 100 of 256");
        assert!(
            rgb[..x].iter().all(|&p| p == a) && rgb[x..].iter().all(|&p| p == b),
            "uniform A prefix, uniform B suffix"
        );
        assert_eq!(
            rgb.windows(2).filter(|w| w[0] != w[1]).count(),
            1,
            "EXACTLY one transition — a smeared or double-applied journal shows up here"
        );
        // And the sink still gets one whole row: segments are internal.
        assert_eq!(rgb.len(), width, "one complete row, not a list of spans");
    }

    /// The zero-mid-line-write case, re-proven under the segmented emitter (CR-25 adoption clause 2 / the
    /// slice-3 poison, re-run). Landings outside the active window must leave the row **bit-identical** to
    /// the line-atomic answer: `d = 0` is the whole row (the write precedes any pixel being consumed) and
    /// `d >= MCLK_PER_ACTIVE` is the blanking tail, which colours nothing here and first shows in row N+1.
    #[test]
    fn a_row_with_no_landing_inside_the_active_window_is_bit_identical_to_the_line_atomic_answer() {
        let mut v = backdrop_fixture();
        write_cram(&mut v, 1, 0x000E);
        let atomic = report_rgb_with_cram(v.cram(), &v.render_line_report(0));

        let empty = retained(&v, &[]);
        assert_eq!(
            row_rgb(&empty).0,
            atomic,
            "no journal: the slice-3 identity, untouched"
        );

        for d in [crate::vdp::MCLK_PER_ACTIVE, crate::vdp::MCLK_PER_LINE - 1] {
            let row = retained(&v, &[(d, 2, 0x0E00)]);
            let (rgb, working) = row_rgb(&row);
            assert_eq!(
                rgb, atomic,
                "a landing at d={d} is in blanking — row N is untouched"
            );
            assert_ne!(
                &working[..],
                v.cram(),
                "…but it IS applied to the working copy, or the emit-time guard would not see it"
            );
        }
        // `d = 0` is the other boundary: x = 0, so the whole row takes the new colour.
        let whole = retained(&v, &[(0, 2, 0x0E00)]);
        let (rgb, _) = row_rgb(&whole);
        assert!(
            rgb.iter().all(|&p| p == rgb[0]) && rgb[0] != atomic[0],
            "d = 0 recolours the entire row"
        );
    }

    /// **Coalescing by pixel is mandatory** (decision C-6 + the design's own "must include, or it is
    /// wrong"). `direct_color_dma` pushes 44,352 CRAM words inside one instruction, so they share one master
    /// clock and one landing pixel. Journal them naively and that is a ~700 KB per-line `Vec` and 44 k
    /// zero-length decode spans.
    #[test]
    fn a_forty_thousand_write_burst_at_one_clock_collapses_to_one_segment() {
        let v = backdrop_fixture();
        const BURST: usize = 44_352;
        let landings: Vec<(u64, usize, u16)> =
            (0..BURST).map(|i| (1000, 2, (i % 0x0EEE) as u16)).collect();
        let row = retained(&v, &landings);
        assert_eq!(
            row.journal.len(),
            1,
            "one pixel, one CRAM address ⇒ ONE journal entry, however many words the burst carried"
        );
        // The surviving entry is the LAST word written — the pixel shows the last colour written at it.
        let last = ((BURST - 1) % 0x0EEE) as u16 & 0x0EEE;
        assert_eq!(row.journal[0].word & 0x0EEE, last & 0x0EEE);

        // Distinct addresses at one pixel stay distinct (they are different colours, not overwrites), and
        // distinct pixels stay distinct segments.
        let mixed = retained(
            &v,
            &[(1000, 2, 1), (1000, 4, 2), (1000, 2, 3), (2000, 2, 4)],
        );
        assert_eq!(
            mixed.journal.len(),
            3,
            "two addresses at pixel 100 (the second write to entry 1 replacing the first) + one at pixel 200"
        );
        assert_eq!(
            mixed.journal[0].word, 3,
            "the later write to the same address won"
        );
    }

    /// **The emit-time guard's predicate, tested both ways.** `System::flush_pending_row` asserts that the
    /// CRAM `row_rgb` ends its walk on equals the machine's live CRAM; this pins the property that assert
    /// rests on — the fold over the journal reproduces the line's net CRAM effect, and **fails to** the
    /// moment a landing is missing.
    ///
    /// The `debug_assert_eq!` itself is not reachable from a test without mutating the run loop (there is no
    /// production path that skips a landing, which is the point), so it is covered by a recorded mutation
    /// instead; what is covered *here* is that the predicate it checks can actually be false.
    #[test]
    fn the_emit_time_guard_predicate_holds_only_when_every_landing_is_journalled() {
        let mut v = backdrop_fixture();
        write_cram(&mut v, 1, 0x000E);
        let report = v.render_line_report(0);
        let snapshot = *v.cram().first_chunk::<CRAM_SIZE>().expect("CRAM image");

        // Two CRAM writes during the line, as the machine would perform them.
        let writes = [(1000u64, 2usize, 0x0E00u16), (2000, 4, 0x00E0)];
        for &(_, addr, word) in &writes {
            write_cram(&mut v, addr / 2, word);
        }
        let live = v.cram();

        let build = |journal: &[(u64, usize, u16)]| {
            let mut sc = ScanlineScaffold::default();
            sc.stash(report.clone(), &snapshot);
            for &(d, addr, word) in journal {
                sc.journal_cram(d, addr, word);
            }
            row_rgb(&sc.take().expect("stashed")).1
        };

        assert_eq!(
            &build(&writes)[..],
            live,
            "every landing journalled ⇒ the walk ends on the machine's own CRAM — the guard's happy case"
        );
        assert_ne!(
            &build(&writes[..1])[..],
            live,
            "NON-VACUITY: drop one landing and the predicate is false. Without this the guard could be \
             asserting something no bug can break."
        );
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
        set_reg(&mut v, 0x01, 0x04); // display off (M5 kept)
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
        set_reg(&mut v, 0x01, 0x44); // display on + M5 (bit 2)
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
        set_reg(&mut v, 0x01, 0x04); // display off (M5 kept)
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
    fn report_rgb_is_render_line_decode_of_the_written_cram_color() {
        // The scanline-capture decode (report_rgb over render_scanline's report) is byte-identical to
        // render_line's map — same winning index, same shadow/highlight state — and the pixels really carry
        // the CRAM color that was written. Zeroed VRAM = transparent planes + empty SAT, so the whole line is
        // the backdrop: reg $07 index 42, programmed to level-7 R/G/B through the real data port.
        let mut v = fresh();
        v.vram_mut().fill(0);
        set_reg(&mut v, 0x07, 42);
        write_cram(&mut v, 42, 0x0EEE);
        let line = 100;
        // Captured BEFORE render_scanline: the carry commit could reseed R10 masking for a later re-resolve,
        // so the reference decode must come from the same pre-commit state the report was resolved in.
        let expected = v.render_line(line);
        let report = v.render_scanline(line);
        assert_eq!(
            v.report_rgb(&report),
            expected,
            "report_rgb is exactly render_line's decode of the same resolution"
        );
        let max = intensity(7, PixelState::Normal);
        assert_eq!(expected.len(), 256, "H32 default width");
        assert!(
            expected.iter().all(|&px| px == (max, max, max)),
            "every backdrop pixel decodes to the written CRAM color"
        );
    }

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
    // --- LayerMask: the display mask behind Aether's emulator/set_layer_enabled ----------------------

    /// **The name set is derived, not transcribed.** `Layer::ALL` is what every caller that needs "the set
    /// of layers" iterates — `oracle-aether` builds the wire enum by filtering it — so a variant missing
    /// here would silently shrink that enum. The match below is exhaustive, so a NEW variant cannot compile
    /// until it is named here; `seen` catches the other direction, a variant with an arm that never appears
    /// in the array.
    #[test]
    fn layer_all_lists_every_variant() {
        let mut seen = [false; 5];
        for l in Layer::ALL {
            let idx = match l {
                Layer::Backdrop => 0,
                Layer::PlaneB => 1,
                Layer::PlaneA => 2,
                Layer::Window => 3,
                Layer::Sprite(_) => 4,
            };
            assert!(!seen[idx], "Layer::ALL lists {l:?} twice");
            seen[idx] = true;
        }
        assert!(
            seen.iter().all(|&b| b),
            "Layer::ALL is missing a variant — seen = {seen:?}"
        );
    }

    /// The backdrop is the floor the fall-through ends at, never a mask target (the contract fragment's own
    /// `$comment` says so). `set` refuses it and *says* it refused; `shows` answers `true` for it whatever
    /// the mask holds, so "mask everything" still leaves a picture.
    #[test]
    fn the_backdrop_is_not_a_mask_target() {
        let mut m = LayerMask::ALL;
        assert!(
            !m.set(Layer::Backdrop, false),
            "set(Backdrop) must report that the backdrop is not a mask target"
        );
        assert_eq!(m, LayerMask::ALL, "a refused set must not change the mask");
        let none = LayerMask {
            plane_a: false,
            plane_b: false,
            window: false,
            sprites: false,
        };
        assert!(
            none.shows(Layer::Backdrop),
            "the backdrop shows under an all-off mask"
        );
        // …and every other layer IS a target — derived from Layer::ALL rather than listed here.
        for l in Layer::ALL {
            let target = !matches!(l, Layer::Backdrop);
            let mut m = LayerMask::ALL;
            assert_eq!(m.set(l, false), target, "set({l:?}) target-ness");
            assert_eq!(
                m.shows(l),
                !target,
                "after set({l:?}, false), shows({l:?}) must follow"
            );
        }
    }

    /// A fixture with three opaque layers stacked at screen (0,0) — sprites over plane A over plane B —
    /// plus enough overlapping sprites that the pipeline's status bits are BOTH set, so a "the mask did not
    /// move them" comparison has something it could have moved.
    ///
    /// Colours: plane B red (CRAM 1), plane A blue (CRAM 2), sprites green (CRAM 3), backdrop CRAM 0. All
    /// three are low priority, so RR9 order is low-sprite > low-A > low-B > backdrop and the fall-through
    /// visits every one of them in turn as masks are added.
    fn stack_fixture() -> Vdp {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0F, 2); // autoincrement 2 — write_sprite streams four words
        set_reg(&mut v, 0x05, 0x10); // SAT base 0x2000
        fill_tile(&mut v, 3, 3);
        write_cram(&mut v, 3, 0x00E0); // entry 3 = green
        put_cell(&mut v, 0xE000, 0x0001); // plane B cell(0,0) red, low priority
        put_cell(&mut v, 0xC000, 0x0002); // plane A cell(0,0) blue, low priority
                                          // 18 opaque 1x1 sprites all at screen (0,0): they overlap (collision) and there are more of them
                                          // than H32's 16-per-line count limit (overflow).
        for i in 0..18u16 {
            let link = if i == 17 { 0 } else { i + 1 };
            write_sprite(&mut v, i as usize, 0x0080, link, 0x0003, 0x0080);
        }
        v
    }

    /// [`stack_fixture`] plus a place on line 0 where **each** of the four maskable layers is the visible
    /// winner: plane A alone at x 64-79, plane B alone at x 96-111, and a right window from x 128 with
    /// opaque cells at x 128-143 (the stack at x 0-31 keeps the sprite).
    ///
    /// It exists because of a measured near-miss. The "an all-on mask is the unmasked render" control was
    /// green under a mutation that deliberately hid plane B, for a reason that had nothing to do with the
    /// rule: **the fixture never drew plane B anywhere visible**, so hiding it could not move the picture.
    /// A control whose comparison cannot move is not a control, and
    /// [`assert_layer_visibility_is_measurable`] is that discovery turned into an assertion.
    fn all_layers_visible_fixture() -> Vdp {
        let mut v = stack_fixture();
        set_reg(&mut v, 0x11, 0x88); // right window from x = 8 * 16 = 128
        put_cell(&mut v, 0xC000 + 8 * 2, 0x0002); // plane A alone, cells 8-9  (x 64-79)
        put_cell(&mut v, 0xC000 + 9 * 2, 0x0002);
        put_cell(&mut v, 0xE000 + 12 * 2, 0x0001); // plane B alone, cells 12-13 (x 96-111)
        put_cell(&mut v, 0xE000 + 13 * 2, 0x0001);
        put_cell(&mut v, 0xA000 + 16 * 2, 0x0003); // window, cells 16-17 (x 128-143)
        put_cell(&mut v, 0xA000 + 17 * 2, 0x0003);
        v
    }

    /// Every maskable layer must be the visible winner somewhere on line 0 — i.e. hiding any one of them,
    /// alone, changes the line. The precondition every sweep over `Layer::ALL` needs before its equality or
    /// its invariant means anything.
    fn assert_layer_visibility_is_measurable(v: &Vdp) {
        let base = v.render_line(0);
        for l in Layer::ALL {
            if matches!(l, Layer::Backdrop) {
                continue;
            }
            assert_ne!(
                v.render_line_masked(0, mask_off(l)),
                base,
                "fixture precondition: hiding {l:?} must change line 0 — a comparison that cannot \
                 move is not evidence"
            );
        }
    }

    /// `LayerMask::ALL` with `layer` switched off, asserting `layer` really is a target.
    fn mask_off(layer: Layer) -> LayerMask {
        let mut m = LayerMask::ALL;
        assert!(m.set(layer, false), "{layer:?} is a mask target");
        m
    }

    /// Do two layers name the same *layer*? `Sprite(a)` and `Sprite(b)` do — a mask applies to the whole
    /// sprite layer, never to one slot.
    fn same_layer(a: Layer, b: Layer) -> bool {
        matches!(
            (a, b),
            (Layer::Backdrop, Layer::Backdrop)
                | (Layer::PlaneA, Layer::PlaneA)
                | (Layer::PlaneB, Layer::PlaneB)
                | (Layer::Window, Layer::Window)
                | (Layer::Sprite(_), Layer::Sprite(_))
        )
    }

    /// **The believable-wrong-answer control.** A mask implemented as a post-hoc blank would paint the
    /// backdrop wherever the masked layer had won — including every dot where plane B was sitting right
    /// behind it. So this asserts the revealed colour is plane B's, and asserts first that the fixture gives
    /// the backdrop a *different* colour, because otherwise a blank and a fall-through are indistinguishable.
    #[test]
    fn a_masked_layer_falls_through_to_the_next_candidate_not_to_the_backdrop() {
        let v = stack_fixture();
        assert_eq!(
            v.render_line(0)[0],
            v.cram_rgb(3),
            "control: unmasked, the sprite (green) wins"
        );
        assert_ne!(
            v.cram_rgb(0),
            v.cram_rgb(1),
            "the fixture must give the backdrop and plane B different colours, or the assertions \
             below cannot tell a fall-through from a blank"
        );

        let no_sprites = mask_off(Layer::Sprite(0));
        assert_eq!(
            v.render_line_masked(0, no_sprites)[0],
            v.cram_rgb(2),
            "sprites masked → plane A (blue), NOT the backdrop"
        );
        assert_eq!(
            v.render_line_report_masked(0, no_sprites).pixels[0].layer,
            Layer::PlaneA,
            "…and the reported winner is plane A"
        );

        let mut no_sprites_no_a = no_sprites;
        no_sprites_no_a.set(Layer::PlaneA, false);
        assert_eq!(
            v.render_line_masked(0, no_sprites_no_a)[0],
            v.cram_rgb(1),
            "sprites + plane A masked → plane B (red), NOT the backdrop"
        );

        let mut none = no_sprites_no_a;
        none.set(Layer::PlaneB, false);
        assert_eq!(
            v.render_line_masked(0, none)[0],
            v.cram_rgb(0),
            "every maskable layer off → the backdrop, which is where the fall-through ends"
        );
        assert_eq!(
            v.render_line_report_masked(0, none).pixels[0].layer,
            Layer::Backdrop
        );
    }

    /// ⚑ **The mask is a display mask: it must not move anything the ROM can read.**
    ///
    /// Sprite overflow and sprite collision are VDP status bits games poll, and the per-sprite outcomes
    /// carry the R10 line-limit / pixel-budget decisions that drive the dot-overflow carry. All of them come
    /// out of `sprite_line`, which `resolve_line_masked` runs *before* it consults the mask at all.
    ///
    /// The fixture sets BOTH bits and drops sprites, and that is asserted first: comparing two `false`s
    /// across four masks would stay green with the guard removed, which is the alternative green path this
    /// control rules out.
    #[test]
    fn a_mask_never_moves_the_sprite_pipeline() {
        let v = stack_fixture();
        let base = v.render_line_report(0);
        assert!(
            base.sprite_overflow,
            "fixture precondition: 18 sprites on one H32 line must set overflow, or this test \
             compares false to false"
        );
        assert!(
            base.sprite_collision,
            "fixture precondition: 18 overlapping sprites must set collision, or this test \
             compares false to false"
        );
        assert!(
            base.sprites
                .iter()
                .any(|s| s.outcome != SpriteOutcome::Rendered),
            "fixture precondition: some sprite must be DROPPED, or the outcome comparison is vacuous"
        );

        for l in Layer::ALL {
            if matches!(l, Layer::Backdrop) {
                continue;
            }
            let m = mask_off(l);
            let r = v.render_line_report_masked(0, m);
            assert_eq!(
                r.sprite_overflow, base.sprite_overflow,
                "masking {l:?} moved sprite_overflow — a display mask changed a status bit the ROM reads"
            );
            assert_eq!(
                r.sprite_collision, base.sprite_collision,
                "masking {l:?} moved sprite_collision — a display mask changed a status bit the ROM reads"
            );
            assert_eq!(
                r.sprite_walk_end, base.sprite_walk_end,
                "masking {l:?} moved the sprite walk"
            );
            let (got, want): (Vec<_>, Vec<_>) = (
                r.sprites.iter().map(|s| (s.index, s.outcome)).collect(),
                base.sprites.iter().map(|s| (s.index, s.outcome)).collect(),
            );
            assert_eq!(
                got, want,
                "masking {l:?} moved the per-sprite outcomes (the R10 budget decisions)"
            );
        }
    }

    /// The other half of the same guarantee: the stateful render has no masked twin, so the latches it
    /// commits cannot be reached through a mask. Driving every mask through the pure renders first and then
    /// committing must land the VDP in exactly the state committing straight away does — including the R10
    /// dot-overflow carry, which is checked on the *next* line because that is the only place it shows.
    #[test]
    fn masked_renders_leave_the_committed_sprite_latches_untouched() {
        let mut plain = stack_fixture();
        let committed = plain.render_scanline(0);
        assert!(
            committed.sprite_overflow,
            "fixture precondition: the committed line must set overflow"
        );

        let mut poked = stack_fixture();
        for l in Layer::ALL {
            let mut m = LayerMask::ALL;
            m.set(l, false);
            let _ = poked.render_line_masked(0, m);
            let _ = poked.render_line_report_masked(0, m);
            let _ = poked.pixel_attribution_masked(0, 0, m);
        }
        let after = poked.render_scanline(0);
        assert_eq!(
            after.sprite_overflow, committed.sprite_overflow,
            "a masked render before the commit changed the committed overflow latch"
        );
        assert_eq!(
            after.sprite_collision, committed.sprite_collision,
            "a masked render before the commit changed the committed collision latch"
        );
        let (got, want): (Vec<_>, Vec<_>) = (
            poked
                .render_line_report(1)
                .sprites
                .iter()
                .map(|s| (s.index, s.outcome))
                .collect(),
            plain
                .render_line_report(1)
                .sprites
                .iter()
                .map(|s| (s.index, s.outcome))
                .collect(),
        );
        assert_eq!(
            got, want,
            "line 1's R10 masking differs, so the dot-overflow carry the commit seeded differs — \
             the mask reached chip state"
        );
    }

    /// **The mask decides what is drawn, never how a surviving pixel looks.**
    ///
    /// R11's shadow/highlight default is `Shadow` iff both the A-slot and plane-B priority bits are clear —
    /// a property of the tiles, not of what won. Re-deriving it from the post-mask pixels would darken plane
    /// B the moment a high-priority plane A above it was masked away, so masking one layer would change the
    /// colour of another. The `Shadow` control first proves this fixture's S/H is actually live.
    #[test]
    fn a_mask_does_not_change_the_shadow_highlight_of_what_remains() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x0C, 0x08); // shadow/highlight enable (reg $0C bit 3), H32
        put_cell(&mut v, 0xE000, 0x0001); // plane B cell(0,0) red, LOW priority
        put_cell(&mut v, 0xC000, 0x0002); // plane A cell(0,0) blue, LOW priority

        assert_eq!(
            v.render_line_report(0).pixels[0].state,
            PixelState::Shadow,
            "control: two low-priority planes must shadow, or this fixture's S/H is not enabled"
        );

        put_cell(&mut v, 0xC000, 0x8002); // plane A now HIGH priority → the default becomes Normal
        let unmasked = v.render_line_report(0).pixels[0];
        assert_eq!(unmasked.layer, Layer::PlaneA);
        assert_eq!(
            unmasked.state,
            PixelState::Normal,
            "control: a high-priority A-slot clears the shadow default"
        );

        let m = mask_off(Layer::PlaneA);
        let masked = v.render_line_report_masked(0, m).pixels[0];
        assert_eq!(
            masked.layer,
            Layer::PlaneB,
            "plane A masked → plane B shows"
        );
        assert_eq!(
            masked.state,
            PixelState::Normal,
            "plane B must keep the intensity it already had — masking plane A must not shadow it"
        );
        assert_eq!(
            v.render_line_masked(0, m)[0],
            v.cram_rgb(1),
            "…and the revealed colour is plane B's full-intensity red"
        );
    }

    /// The window and plane A share one rendering slot: inside the window span the hardware fetches the
    /// window's cell and plane A is never sampled there. So masking `window` falls through to plane B, the
    /// next RR9 candidate — it does NOT substitute plane A, which would mean *synthesising* a picture the
    /// hardware cannot produce rather than removing one from it.
    ///
    /// Outside the span the A slot is plane A, so `planeA` governs there and `window` does nothing.
    #[test]
    fn masking_the_window_falls_through_to_plane_b_not_to_plane_a() {
        let mut v = pa_fixture(false);
        set_reg(&mut v, 0x11, 0x02); // left window WHP=2 → [0,32)
        fill_tile(&mut v, 3, 3);
        write_cram(&mut v, 3, 0x00E0); // green
        put_cell(&mut v, 0xA000, 0x0003); // window cell(0,0) green
        put_cell(&mut v, 0xC000, 0x0002); // plane A cell(0,0) blue — in the window's slot
        put_cell(&mut v, 0xE000, 0x0001); // plane B cell(0,0) red

        assert_eq!(
            v.render_line_report(0).pixels[0].layer,
            Layer::Window,
            "control: the window owns x=0"
        );
        let no_window = mask_off(Layer::Window);
        assert_eq!(
            v.render_line_report_masked(0, no_window).pixels[0].layer,
            Layer::PlaneB,
            "window masked → plane B, the next RR9 candidate"
        );
        assert_eq!(
            v.render_line_masked(0, no_window)[0],
            v.cram_rgb(1),
            "…red, not plane A's blue: the mask removes a layer, it does not put another in its place"
        );
        assert_eq!(
            v.render_line_report_masked(0, mask_off(Layer::PlaneA))
                .pixels[0]
                .layer,
            Layer::Window,
            "plane A masked does not remove the window from its own slot"
        );
    }

    /// A mask can only ever REMOVE: at every dot the masked winner ranks no *higher* in RR9 order than the
    /// unmasked winner did, and the masked layer is never the one drawn. One invariant, swept over a whole
    /// line, rules out the class of bug where a mask reveals something the unmasked frame never contained.
    #[test]
    fn a_mask_is_strictly_subtractive_at_every_dot() {
        fn rank(l: Layer) -> u8 {
            match l {
                Layer::Sprite(_) => 0,
                Layer::Window | Layer::PlaneA => 1,
                Layer::PlaneB => 2,
                Layer::Backdrop => 3,
            }
        }
        let v = all_layers_visible_fixture();
        let base = v.render_line_report(0);
        assert_layer_visibility_is_measurable(&v);
        for l in Layer::ALL {
            if matches!(l, Layer::Backdrop) {
                continue;
            }
            let r = v.render_line_report_masked(0, mask_off(l));
            for (x, (m, b)) in r.pixels.iter().zip(base.pixels.iter()).enumerate() {
                assert!(
                    rank(m.layer) >= rank(b.layer),
                    "masking {l:?} promoted dot {x} from {:?} to {:?} — a mask must never add a layer",
                    b.layer,
                    m.layer
                );
                assert!(
                    !same_layer(m.layer, l),
                    "masking {l:?} still drew it at dot {x}"
                );
            }
        }
    }

    /// `pixel_attribution` must report what was DRAWN, not what would have won — and a masked layer is
    /// absent from the candidate list rather than carrying a verdict the closed vocabulary has no word for.
    #[test]
    fn attribution_under_a_mask_reports_what_was_drawn() {
        let v = stack_fixture();
        let full = v.pixel_attribution(0, 0);
        assert!(
            matches!(full.winner, Layer::Sprite(_)),
            "control: unmasked, a sprite wins (0,0)"
        );
        assert_eq!(
            full.candidates.len(),
            4,
            "control: sprite + A slot + B + backdrop"
        );

        let m = mask_off(Layer::Sprite(0));
        let masked = v.pixel_attribution_masked(0, 0, m);
        assert_eq!(
            masked.winner,
            Layer::PlaneA,
            "the winner is the layer that was drawn, not the one the mask suppressed"
        );
        assert!(
            !masked
                .candidates
                .iter()
                .any(|c| matches!(c.layer, Layer::Sprite(_))),
            "a masked layer is not a candidate — it must not appear in the list at all: {:?}",
            masked.candidates
        );
        assert_eq!(masked.candidates.len(), 3, "A slot + B + backdrop");
        assert_eq!(
            masked.candidates[0].verdict,
            CandidateVerdict::Won,
            "the head of the list is the drawn layer"
        );
        assert!(
            masked
                .candidates
                .iter()
                .all(|c| c.verdict != CandidateVerdict::Operator),
            "no surviving candidate may be labelled a sprite operator because a sprite was masked: {:?}",
            masked.candidates
        );
        assert_eq!(
            masked.rgb,
            v.render_line_masked(0, m)[0],
            "attribution's rgb must equal the masked render's pixel"
        );
    }

    /// **The currency control.** `LayerMask::ALL` must leave every render byte-identical to the code that
    /// ran before the mask existed — otherwise every golden in this repo moved for a feature that is off.
    ///
    /// The equality is preceded by [`assert_layer_visibility_is_measurable`] because the equality alone can
    /// be green for the wrong reason: a fixture that never draws a layer cannot notice one being dropped,
    /// and a planted "always hide plane B" defect sailed through this test until the fixture was fixed.
    #[test]
    fn an_all_on_mask_is_the_unmasked_render_exactly() {
        assert_layer_visibility_is_measurable(&all_layers_visible_fixture());
        for (mut v, sh_reg) in [
            (all_layers_visible_fixture(), 0x08u8),
            (pa_fixture(true), 0x89),
            (pb_fixture(false), 0x08),
        ] {
            set_reg(&mut v, 0x0C, sh_reg); // exercise the S/H path too
            for line in [0u16, 1, 7, 8, 63] {
                assert_eq!(
                    v.render_line_masked(line, LayerMask::ALL),
                    v.render_line(line),
                    "line {line}: an all-on mask changed the picture"
                );
                assert_eq!(
                    v.render_line_report_masked(line, LayerMask::ALL).pixels,
                    v.render_line_report(line).pixels,
                    "line {line}: an all-on mask changed the resolved pixels"
                );
            }
            assert_eq!(
                v.pixel_attribution_masked(0, 0, LayerMask::ALL),
                v.pixel_attribution(0, 0),
                "an all-on mask changed the attribution"
            );
        }
    }
}
