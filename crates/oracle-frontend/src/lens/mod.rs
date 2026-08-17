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
//!
//! This module holds the spine — ids, the toggle bitset, the config-file spelling, and the one
//! [`models`]/[`draw`] pair the run loop calls. Each lens is its own submodule, declared as it
//! arrives; a placeholder file would be dead weight the gate would rightly flag.

pub mod cpu;
pub mod video;
pub mod watch;

use crate::present::Rect;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use oracle_core::watchpoints::Watchpoints;

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

/// [`LensSet`] is a `u8`, so a ninth lens would shift past the end: `1u8 << 8` panics in debug but
/// **wraps in release**, silently aliasing bit 0 — a shipped build where toggling the ninth lens
/// also toggles the first. Widen `LensSet`'s field before adding one. Six are here and the audio
/// meters are gated rather than cancelled, so the seventh is already spoken for.
const _: () = assert!(
    LensId::ALL.len() <= 8,
    "LensSet is a u8: widen it before adding a ninth lens"
);

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
    /// Whether anything is on. Half of the run loop's draw guard: with everything off *and the
    /// machine running* it skips [`models`] entirely, so a lens that is not on costs nothing — not
    /// even the reads its model would make. The other half is `paused`, because the CPU chip shows
    /// itself when the machine stops (see [`models`]).
    pub fn any(self) -> bool {
        self.0 != 0
    }
}

/// Everything the enabled lenses may read this frame, borrowed once.
///
/// Grouped rather than passed positionally because the list grew — five fields at the first lens,
/// six at the last — and several of them are same-typed, which is the point at which a transposed
/// call site stops being a compile error. Collecting them early is what made the hover callout's
/// arrival one field rather than one more positional argument at every call site.
///
/// What is *not* here: the blit's forward map, and anything else in window pixels. A model built
/// from window coordinates would have to be rebuilt on every resize and could not be tested without
/// a window; the mapping happens in [`draw`], which is handed the picture's rect instead.
pub struct FrameCtx<'a> {
    pub sys: &'a System,
    pub wp: &'a Watchpoints,
    pub symbols: Option<&'a SymbolTable>,
    /// The run loop's own frame counter — the one the title bar and the status line already show.
    /// `System` has no `frame()`, and inventing a second count that could disagree with the
    /// window title would make the chip worse than useless while stepping.
    pub frame: u64,
    /// The run loop's `paused`, likewise authoritative for the UI: `System` has no `is_paused()`
    /// because pausing is a frontend idea, not a machine one.
    pub paused: bool,
    /// The **game dot under the mouse**, or `None` when the cursor is outside the picture, the
    /// palette is eating the mouse, or there is no cursor at all.
    ///
    /// Genuine frontend state with no home in core — where the pointer is is a question about the
    /// window — so it rides here rather than being read inside [`models`], which has no `Window`.
    /// It is already in *game* coordinates: `present::window_to_native` is the hit test (it answers
    /// `None` outside the picture, which is exactly the test wanted), and it belongs to the run
    /// loop because only there is the rect the frame was actually blitted into.
    ///
    /// **Not** the window pixel. A model that knew window pixels would have stopped being a model,
    /// which is the same reason the blit's forward map stays a [`draw`] parameter.
    pub hover: Option<(u16, u16)>,
}

/// Everything the enabled lenses need to draw this frame, extracted once. Absent = that lens is
/// off, so [`draw`] never has to know the set.
pub struct Models {
    pub ticker: Option<watch::Ticker>,
    pub cpu: Option<cpu::Chip>,
    /// The 64 live CRAM entries, already packed for the window buffer. An `[u32; 64]` by value
    /// because that is 256 bytes on the stack against a heap allocation every frame, and because
    /// `cram_decoded` hands back an owned array anyway — there is nothing to borrow.
    pub cram: Option<[u32; 64]>,
    /// The visible sprites' outlines, in **game pixels**. Deliberately not in window pixels: the
    /// blit's forward map is a pure function of the view rect and the source size, both of which
    /// [`draw`] is handed, so mapping there keeps this a model. A model that knew window pixels
    /// would have to be rebuilt on every window resize, and would be untestable without one.
    pub sprites: Option<Vec<video::SpriteBox>>,
    /// The callout for the dot under the cursor, text already assembled, anchored in **game
    /// pixels** for the same reason the outlines are. Absent when the lens is off or the cursor is
    /// not over the picture.
    pub hover: Option<video::Hover>,
}

/// Build the models for whatever is on. Called once per frame, immediately before drawing, and
/// skipped entirely when nothing is on and the machine is running.
pub fn models(set: LensSet, cx: &FrameCtx<'_>) -> Models {
    // **One decode per frame, shared by the two lenses that want it.** `sprites_decoded` walks all
    // 80 slots and allocates ~1.3 KB every call, so neither lens may call it a second time: the
    // outlines' `boxes` and the hover callout's `hover_text` both read *this* vec.
    //
    // The guard is the union rather than the outlines alone because the callout wants the raw slots
    // — `tile` and `palette`, which a `SpriteBox` does not carry — and it can be on with the
    // outlines off.
    //
    // It stays a **local**, not a `Models` field. Task 8 tried the field and measured the failure:
    // a bin-only crate has no production reader for it, so `clippy --all-targets -D warnings` fails
    // with `field decoded_sprites is never read`. That is still true here — both readers are in
    // this function, and a field nothing outside it reads would reintroduce exactly the error, and
    // with it the pressure for the `#[allow(dead_code)]` this slice forbids. Sharing the decode was
    // the point; carrying it in `Models` never was.
    let decoded = (set.is_on(LensId::Sprites) || set.is_on(LensId::Hover))
        .then(|| cx.sys.vdp().sprites_decoded());
    // The outlines follow the **link walk**, not the SAT: `render_line_report(_).sprites` is
    // "every walked sprite, in link-walk order", and everything past the walk is a slot the
    // hardware never looks at, holding whatever was last written there. Outlining those drew boxes
    // on empty picture (see [`video::boxes`]).
    //
    // **One line is enough, and line 0 will do.** Reachability is a property of the link *fields*,
    // which are frame-level state: every line walks the same chain from slot 0 and produces the
    // same set of `index`es. Only the per-sprite `outcome` is a per-line fact, and `boxes`
    // discards outcomes entirely — a sprite that is `OffLine` on line 0 still gets its box. (The
    // R10 dot-overflow carry can differ at line 0 too; it likewise only moves outcomes.)
    //
    // The cost is one line resolve, the same order as the hover callout's `pixel_attribution` and
    // paid only when the lens is on.
    let sprites = decoded
        .as_deref()
        .filter(|_| set.is_on(LensId::Sprites))
        .map(|decoded| {
            let vdp = cx.sys.vdp();
            video::boxes(
                &vdp.render_line_report(0).sprites,
                decoded,
                vdp.active_display(),
            )
        });
    // Hover **explains**; click **arms** (spec §5.2) — this reads and nothing else. Three things
    // must all hold: the lens is on, the run loop found a dot under the cursor, and the decode
    // above therefore happened. `pixel_attribution` is one extra scanline resolve — about 1/224 of
    // the render this loop already does every frame, and skipped outright by the `is_on` guard the
    // rest of the time.
    let hover = match (set.is_on(LensId::Hover), cx.hover, decoded.as_deref()) {
        (true, Some((x, y)), Some(decoded)) => Some(video::Hover {
            text: video::hover_text(&cx.sys.vdp().pixel_attribution(x, y), decoded),
            at: (x, y),
        }),
        _ => None,
    };
    Models {
        ticker: set
            .is_on(LensId::Watch)
            .then(|| watch::model(cx.wp, cx.symbols, watch::ROWS)),
        // Three ways in: either CPU lens, or a paused machine. The auto-show is the reason the run
        // loop guards on `any() || paused` — a pause with every lens off must still produce a
        // chip, because "where did it stop?" is the first question pausing asks.
        cpu: (set.is_on(LensId::Cpu) || set.is_on(LensId::CpuRegs) || cx.paused).then(|| {
            cpu::model(
                cx.sys.cpu_regs(),
                cx.symbols,
                cx.frame,
                cx.paused,
                set.is_on(LensId::CpuRegs),
            )
        }),
        // `cram_decoded` walks all 64 entries and returns an owned array, so it is a read the
        // `then` guard genuinely saves when the lens is off — the same reason every other model
        // here is built lazily.
        cram: set
            .is_on(LensId::Cram)
            .then(|| video::swatches(&cx.sys.vdp().cram_decoded())),
        sprites,
        hover,
    }
}

/// Draw every built model, in a fixed back-to-front order, so a later lens draws over an earlier
/// one.
///
/// **Draw order is not [`LensId::ALL`] order, and the difference is deliberate.** `ALL` is the
/// *registration* order: it fixes the palette rows, the bit each lens owns, and the order lens
/// names are written into the config file's `lenses` value — an order `all_lists_every_variant_`
/// `exactly_once` pins and which must stay stable, because rewriting it would churn a user's file
/// on every launch. Layering is a question about pixels, and the two answers are allowed to differ.
/// Reorder the arms below to change layering; never reorder `ALL` to do it.
///
/// The order is: **outlines, ticker, chip, strip, callout.** The ticker (bottom), the chip
/// (top-right) and the CRAM strip
/// (top-left, one text row down) each own a different corner, and on an ordinary picture none of
/// them meet. Two pairs can, and both resolve by draw order rather than by geometry:
/// - **Ticker vs chip.** Only on a picture short enough for the chip's panel to reach the bottom
///   strip, where the chip wins — the same overlap `watch.rs` already accepts against the toasts,
///   and for the same reason.
/// - **Strip vs chip.** These two share a band *vertically at every size*: the strip's rows sit
///   inside the compact chip's panel exactly (at `px = 1`, rows 16..32 against the chip's 4..32).
///   All that separates them is horizontal, so they collide once the picture is narrower than
///   `2 * margin + strip_w + chip_w` — 207 device pixels at `px = 1` with the register block
///   showing, which a 200-wide picture is already inside: measured, they overlap by 112 pixels
///   there. The **strip wins**, being drawn later. That is the right way round — the strip is 64
///   fixed cells whose whole meaning is positional, so clipping it would misreport CRAM, while the
///   chip loses a few glyphs off one end of two lines and stays readable.
///
/// Both are accepted rather than resolved: a picture that small has no arrangement where five
/// panels fit, and moving one lens under another by size would make the layout jump around while
/// a window is dragged.
///
/// **The sprite outlines go first, underneath everything.** They are drawn *on* the picture rather
/// than in a corner of it, so they own no corner and can cross any panel — and the reason they lose
/// every one of those crossings is that the two kinds of damage are not comparable. A panel's
/// content is opaque *on purpose*: the strip's swatches must be the CRAM entry exactly ("a swatch
/// blended over the picture would be a different colour from the entry it is reporting"), and the
/// chip's and ticker's glyphs are text. A hairline over any of them is **interference, not
/// occlusion**: a missing glyph is visibly missing, but a `$3F` with a stroke through it reads as
/// `$8F`, and a swatch with one blended row reports a colour CRAM never held. Silent wrongness in a
/// readout is the failure this whole slice keeps guarding against, and it beats the outline's own
/// claim on the pixels. The outline pays almost nothing for losing: the panels are only
/// `PANEL_ALPHA = 190`, so an outline underneath one is dimmed rather than erased, and its
/// *position* — the only thing an outline conveys — survives intact.
///
/// **The hover callout goes last, over everything.** It is the only lens the user is *pointing at*:
/// it exists for the half-second between putting the cursor somewhere and reading the answer, and
/// an answer half-hidden under a panel that was already on screen is no answer. Nothing pays much
/// for losing to it either — it is one line of text following the cursor, so whatever it covers is
/// uncovered again by moving the mouse, which is not true of any fixed panel covering another.
///
/// Anchored to `area` (the picture), never
/// the window: the letterbox stays black, and a tall window with a narrow picture must not make
/// the font wider than the panel (the `draw_narrow_panel_does_not_underflow` hazard class).
///
/// `native` is the size of the frame that was blitted into `area` — `(width, HEIGHT)` in the run
/// loop, the same pair the status line reports. It is a parameter rather than a [`Models`] field
/// because it is not a model: it describes the picture on the glass, not the machine, and it is
/// exactly what [`present::native_rect_to_window`](crate::present::native_rect_to_window) needs
/// alongside `area` to place a game-pixel rect.
pub fn draw(buf: &mut [u32], w: usize, h: usize, area: Rect, native: (usize, usize), m: &Models) {
    let px = crate::overlay::Overlay::font_scale(area.h.max(1));
    let mut c = crate::font::Canvas::new(buf, w, h);
    // Back to front. The outlines are first because every panel above them is opaque on purpose;
    // see the layering note above, and `the_panels_draw_over_the_sprite_outlines` for its witness.
    if let Some(bx) = &m.sprites {
        video::draw_sprites(&mut c, area, px, native, bx);
    }
    if let Some(t) = &m.ticker {
        watch::draw(&mut c, area, px, t);
    }
    if let Some(chip) = &m.cpu {
        cpu::draw(&mut c, area, px, chip);
    }
    if let Some(sw) = &m.cram {
        video::draw_cram(&mut c, area, px, sw);
    }
    // Last, so the thing the user is pointing at is never underneath something they are not.
    if let Some(hv) = &m.hover {
        video::draw_hover(&mut c, area, px, native, hv);
    }
}

/// Parse the config file's `lenses` value: a comma-separated list of [`LensId::key`] spellings.
///
/// Returns the recognised set plus the names this build does not know, **kept rather than
/// dropped** — the same forward-compatibility rule the config's unknown *keys* follow
/// (F-CONFIG-UNKNOWN-KEYS), applied one level down. S4 and S5 are the slices that add lenses, so
/// "an older build reads a newer build's file" is the next two slices, not a hypothetical: without
/// this, launching this build once would delete every lens it had not heard of.
///
/// The remnant is returned for [`config::Config`](crate::config::Config) to hold rather than
/// stored in [`LensSet`], which stays `Copy` and allocation-free for the run loop.
/// Empty items are skipped silently so `a,,b` and a trailing comma are both fine.
pub fn parse_set(value: &str) -> (LensSet, Vec<String>) {
    let mut set = LensSet::default();
    let mut unrecognised = Vec::new();
    for name in value.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        match LensId::ALL.iter().find(|id| id.key() == name) {
            Some(id) => set.set(*id, true),
            None => unrecognised.push(name.to_string()),
        }
    }
    (set, unrecognised)
}

/// The inverse: known lenses in [`LensId::ALL`] order so the file is stable across saves (an
/// unstable order would rewrite the file — and wake the debounce — on every launch), followed by
/// the names this build did not recognise in file order, written back verbatim.
pub fn format_set(set: LensSet, unrecognised: &[String]) -> String {
    LensId::ALL
        .iter()
        .filter(|id| set.is_on(**id))
        .map(|id| id.key().to_string())
        .chain(unrecognised.iter().cloned())
        .collect::<Vec<_>>()
        .join(",")
}

/// The `(top, bottom, left, right)` extremes of everything a draw fn changed, or `None` if it
/// painted nothing — every lens's pixel tests measure where the ink landed, and this was three
/// verbatim copies before the outlines wanted a fourth.
///
/// `bg` is a parameter rather than a module constant because each lens module owns its own `BG`
/// (all `0x0012_3456` today, all for the same reason: an alpha-blended black panel over a zero
/// buffer is a no-op and invisible to every assertion).
#[cfg(test)]
pub(crate) fn ink_bounds(buf: &[u32], w: usize, bg: u32) -> Option<(usize, usize, usize, usize)> {
    let mut b: Option<(usize, usize, usize, usize)> = None;
    for (i, p) in buf.iter().enumerate() {
        if *p == bg {
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

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::vdp::Vdp;

    #[test]
    fn set_round_trips_through_the_file_spelling() {
        let mut set = LensSet::default();
        set.set(LensId::Watch, true);
        set.set(LensId::Cram, true);
        let text = format_set(set, &[]);
        assert_eq!(text, "watch,cram", "stable order, ALL order");
        let (back, unrecognised) = parse_set(&text);
        assert_eq!(back, set);
        assert!(
            unrecognised.is_empty(),
            "own output was not understood: {unrecognised:?}"
        );
    }

    #[test]
    fn every_lens_round_trips_and_the_empty_set_is_empty() {
        for id in LensId::ALL {
            let mut set = LensSet::default();
            set.set(id, true);
            let (back, unrecognised) = parse_set(&format_set(set, &[]));
            assert_eq!(back, set, "{} did not round-trip", id.key());
            assert!(unrecognised.is_empty());
        }
        assert_eq!(format_set(LensSet::default(), &[]), "");
        assert_eq!(parse_set("").0, LensSet::default());
        assert!(
            parse_set("").1.is_empty(),
            "an empty value is not an unknown lens"
        );
    }

    /// The lens-level half of F-CONFIG-UNKNOWN-KEYS: a name this build does not know is handed
    /// back to the caller to store, not discarded, and the known names around it are unaffected.
    /// The padding sits on a **known** name on purpose — on the unknown one it proves nothing,
    /// since an untrimmed unknown name is unknown either way.
    #[test]
    fn an_unknown_lens_is_kept_and_leaves_the_rest_alone() {
        let (set, unrecognised) = parse_set("watch, cram , from_the_future");
        assert!(
            set.is_on(LensId::Watch) && set.is_on(LensId::Cram),
            "a padded known name still parses"
        );
        assert_eq!(unrecognised, vec!["from_the_future".to_string()]);
    }

    /// The whole point of keeping unknown names: they must come back out of `format_set`, after
    /// the known ones, so the next save writes them back instead of deleting them.
    #[test]
    fn an_unknown_lens_survives_a_format_parse_cycle() {
        let (set, unrecognised) = parse_set("heatmap,watch,audio_meters");
        let text = format_set(set, &unrecognised);
        assert_eq!(
            text, "watch,heatmap,audio_meters",
            "known first in ALL order, unknown after in file order"
        );
        let (set2, unrecognised2) = parse_set(&text);
        assert_eq!(set2, set, "a second cycle is a fixed point");
        assert_eq!(unrecognised2, unrecognised);
    }

    #[test]
    fn toggle_and_any_agree_with_is_on() {
        let mut set = LensSet::default();
        assert!(!set.any(), "the default set is empty");
        assert!(!set.is_on(LensId::Hover));
        set.toggle(LensId::Hover);
        assert!(set.is_on(LensId::Hover));
        assert!(set.any(), "one lens on is enough for any()");
        set.toggle(LensId::Hover);
        assert!(!set.is_on(LensId::Hover), "toggle is its own inverse");
        assert!(
            !set.any(),
            "and turning the last one off empties the set again"
        );
        // `set(id, false)` must actually clear — S4's view presets assign rather than toggle, and
        // a no-op clear-branch would leave a preset unable to turn anything off.
        set.set(LensId::Hover, true);
        set.set(LensId::Cram, true);
        set.set(LensId::Hover, false);
        assert!(!set.is_on(LensId::Hover), "set(id, false) must clear");
        assert!(set.is_on(LensId::Cram), "and must clear only that lens");
        assert!(set.any(), "one still on");
    }

    /// `any()` must answer for **every** lens, not just the low bits: it is the draw guard, and a
    /// lens it read as "nothing on" would be a toggle that toasts and then draws nothing.
    #[test]
    fn any_sees_every_lens_on_its_own() {
        for id in LensId::ALL {
            let mut set = LensSet::default();
            set.set(id, true);
            assert!(set.any(), "{} alone did not register as on", id.key());
        }
    }

    /// A frame's worth of borrows over a machine nobody has run — every model under test here is
    /// keyed off the lens set, not off what the machine happens to contain.
    fn ctx<'a>(sys: &'a System, wp: &'a Watchpoints, paused: bool) -> FrameCtx<'a> {
        FrameCtx {
            sys,
            wp,
            symbols: None,
            frame: 0,
            paused,
            hover: None,
        }
    }

    /// The same frame with the cursor parked on a game dot. Separate from [`ctx`] rather than an
    /// extra parameter on it, because "no cursor over the picture" is the state most of these tests
    /// are about and spelling it out at every call site would bury the two that are not.
    fn ctx_hovering<'a>(sys: &'a System, wp: &'a Watchpoints, at: (u16, u16)) -> FrameCtx<'a> {
        FrameCtx {
            hover: Some(at),
            ..ctx(sys, wp, false)
        }
    }

    /// The guard that keeps a lens that is off from costing anything: no model is built for it, so
    /// the reads its model would make never happen.
    #[test]
    fn models_are_built_only_for_lenses_that_are_on() {
        let sys = System::new(0x5EED);
        let wp = Watchpoints::new(8);
        let mut set = LensSet::default();
        assert!(
            models(set, &ctx(&sys, &wp, false)).ticker.is_none(),
            "a model was built for a lens that is off"
        );
        set.set(LensId::Watch, true);
        assert!(
            models(set, &ctx(&sys, &wp, false)).ticker.is_some(),
            "no model was built for a lens that is on"
        );
        // A different lens must not switch the ticker on — the models are keyed per id.
        let mut other = LensSet::default();
        other.set(LensId::Cram, true);
        assert!(
            models(other, &ctx(&sys, &wp, false)).ticker.is_none(),
            "the ticker model keyed off the wrong lens"
        );
    }

    /// The CPU chip is the one model with three ways in: either of its two lenses, **or** a paused
    /// machine with every lens off (spec §5.3 — it auto-shows while stopped, which is why the run
    /// loop's draw guard is `any() || paused` rather than `any()`).
    #[test]
    fn the_cpu_chip_is_built_for_either_of_its_lenses_and_while_paused() {
        let sys = System::new(0x5EED);
        let wp = Watchpoints::new(8);
        assert!(
            models(LensSet::default(), &ctx(&sys, &wp, false))
                .cpu
                .is_none(),
            "a chip was built for a running machine with every lens off"
        );
        for id in [LensId::Cpu, LensId::CpuRegs] {
            let mut set = LensSet::default();
            set.set(id, true);
            assert!(
                models(set, &ctx(&sys, &wp, false)).cpu.is_some(),
                "{} did not build the chip",
                id.key()
            );
        }
        let auto = models(LensSet::default(), &ctx(&sys, &wp, true))
            .cpu
            .expect("the chip auto-shows while paused");
        assert!(auto.paused, "and it knows it is the paused one");
        // An unrelated lens must not drag the chip on with it.
        let mut other = LensSet::default();
        other.set(LensId::Cram, true);
        assert!(
            models(other, &ctx(&sys, &wp, false)).cpu.is_none(),
            "the chip keyed off the wrong lens"
        );
    }

    /// Only `CpuRegs` expands it — and it expands it **whether or not the machine is paused**.
    ///
    /// That second half is not a formality: pausing is the moment the register block exists for, so
    /// a model that consulted `paused` when choosing the form would collapse the block to the
    /// compact chip exactly when it was wanted. Every combination of the two CPU lenses against
    /// both run states is covered, because with only the running rows tested that collapse passed.
    #[test]
    fn only_cpu_regs_expands_the_chip() {
        const COMPACT: usize = 3;
        const EXPANDED: usize = 11;
        let sys = System::new(0x5EED);
        let wp = Watchpoints::new(8);
        let lines = |ids: &[LensId], paused: bool| {
            let mut set = LensSet::default();
            for id in ids {
                set.set(*id, true);
            }
            models(set, &ctx(&sys, &wp, paused))
                .cpu
                .expect("a chip was expected")
                .lines
                .len()
        };
        for paused in [false, true] {
            assert_eq!(
                lines(&[LensId::Cpu], paused),
                COMPACT,
                "Cpu alone is the compact chip (paused={paused})"
            );
            assert_eq!(
                lines(&[LensId::CpuRegs], paused),
                EXPANDED,
                "CpuRegs is the full register block (paused={paused})"
            );
            assert_eq!(
                lines(&[LensId::Cpu, LensId::CpuRegs], paused),
                EXPANDED,
                "both on is still the register block (paused={paused})"
            );
        }
        assert_eq!(
            lines(&[], true),
            COMPACT,
            "the paused auto-show is the compact chip"
        );
    }

    /// The CRAM strip end to end: its model is built for its own lens and no other, and [`draw`]
    /// actually puts it on the glass.
    ///
    /// The parallel of `models_are_built_only_for_lenses_that_are_on` (the ticker) and
    /// `the_cpu_chip_is_built_for_either_of_its_lenses_and_while_paused` (the chip), for the strip.
    /// Whether the model then *reaches the glass* is
    /// [`every_lens_with_a_draw_arm_reaches_the_glass`]'s job, for every lens at once.
    #[test]
    fn the_cram_strip_model_is_built_for_its_own_lens_only() {
        let sys = System::new(0x5EED);
        let wp = Watchpoints::new(8);

        assert!(
            models(LensSet::default(), &ctx(&sys, &wp, false))
                .cram
                .is_none(),
            "a strip was built with every lens off"
        );
        let mut other = LensSet::default();
        other.set(LensId::Watch, true);
        assert!(
            models(other, &ctx(&sys, &wp, false)).cram.is_none(),
            "the strip model keyed off the wrong lens"
        );

        let mut set = LensSet::default();
        set.set(LensId::Cram, true);
        let m = models(set, &ctx(&sys, &wp, false));
        assert_eq!(
            m.cram
                .expect("the strip did not build for its own lens")
                .len(),
            64,
            "the strip carries all 64 CRAM entries"
        );
        assert!(
            m.ticker.is_none() && m.cpu.is_none(),
            "the strip's lens dragged another model on with it"
        );
    }

    /// Whether `id` has an arm in [`draw`] yet, declared **per variant** rather than counted.
    ///
    /// Exhaustive on purpose: a seventh lens cannot compile until its author says here whether it
    /// draws, which is the point — the bug this whole test exists for is a lens nobody remembered
    /// to wire into the dispatch. And it is a literal declaration, never derived from
    /// [`LensId::ALL`] or from `Models`' fields: an expectation computed from the same iteration
    /// the production code uses moves with the bug and cancels itself out (the trap
    /// `all_lists_every_variant_exactly_once` documents for `ALL.len()`).
    fn draws_yet(id: LensId) -> bool {
        match id {
            LensId::Watch
            | LensId::Cpu
            | LensId::CpuRegs
            | LensId::Sprites
            | LensId::Cram
            | LensId::Hover => true,
        }
    }

    /// A machine with `slots` written into the SAT from slot 0 up, as `(x_field, y_field, size)`
    /// straight off the hardware layout — the only fixture in this module that puts anything into
    /// the VDP at all, and the reason it exists is that a power-on machine has no sprites.
    ///
    /// It has to: a power-on SAT cache is all zeroes, so every one of the 80 slots decodes as
    /// `y == -128` — the parked idiom — and the outline lens correctly draws nothing. Run against
    /// that, [`every_lens_with_a_draw_arm_reaches_the_glass`] would assert a model was built and no
    /// ink landed, and read as a missing arm in [`draw`] when the arm was there and right.
    ///
    /// Written through the VDP's own ports rather than by poking VRAM, because the Y/size/link half
    /// of a sprite comes from the **SAT cache**, which only the write-through on a VRAM port write
    /// fills (`vram_mut` would leave the cache at zero and the sprite parked). The SAT sits at the
    /// power-on reg-5 base, VRAM `$0000`. Every slot not listed stays parked.
    ///
    /// **Mode 5 has to be turned on first.** A power-on VDP has reg 1 bit 2 (M5) clear, which is
    /// Mode 4, and there `write_register` discards every register above 10 — including reg $0F, the
    /// auto-increment. Without this line all four data writes land on address `$0000`, the size
    /// byte never reaches the cache, and the sprite decodes 1x1 at whatever `System::new`'s seeded
    /// VRAM happens to hold in the X word. That is not a hypothetical; it is what this fixture did
    /// on its first run.
    /// **The link byte in `size` is load-bearing since the outlines started following the walk.**
    /// The low byte of the size word is the sprite's `link` — the next slot the hardware visits —
    /// and the walk always starts at slot 0 and stops when a link is 0. So a fixture whose slot 0
    /// links to 0 is a **one-sprite list** no matter how many slots it wrote, and every later slot
    /// is unreachable. Each caller therefore spells its own chain out; `h40` picks the parse cap
    /// (80 vs 64) as well as the display width.
    fn sys_with_sprites_in(h40: bool, slots: &[(u16, u16, u16, u16)]) -> System {
        let mut sys = System::new(0x5EED);
        let v: &mut Vdp = sys.vdp_mut();
        v.control_write(0x8104, 0); // reg $01 = M5: leave Mode 4, where regs above 10 are discarded
                                    // reg $0C RS0+RS1: H40 is a 320-dot display parsing 80 slots, H32 256 dots and 64.
        v.control_write(if h40 { 0x8C81 } else { 0x8C00 }, 0);
        v.control_write(0x8F02, 0); // reg $0F: auto-increment 2 bytes per data write
        for (slot, (x_field, y_field, size, attr)) in slots.iter().enumerate() {
            let addr = (slot * 8) as u16;
            v.control_write(0x4000 | (addr & 0x3FFF), 0); // VRAM write ...
            v.control_write(0x0000, 0); // ... at the SAT base + slot * 8
            v.data_write(*y_field);
            v.data_write(*size); // size in the high byte, link in the low
            v.data_write(*attr); // tile / attributes; bit 15 is the priority flag
            v.data_write(*x_field);
        }
        sys
    }

    /// The H40 case, which is what almost every test here wants.
    fn sys_with_sprites(slots: &[(u16, u16, u16, u16)]) -> System {
        sys_with_sprites_in(true, slots)
    }

    /// `slots` written from slot 0 up and **chained**: each links to the next, the last links to 0
    /// to terminate the walk. That makes every slot listed reachable, which is what a fixture that
    /// means "these sprites are on screen" has to say now.
    fn sys_with_chained_sprites(h40: bool, slots: &[(u16, u16, u16, u16)]) -> System {
        let chained: Vec<(u16, u16, u16, u16)> = slots
            .iter()
            .enumerate()
            .map(|(i, (x, y, size, attr))| {
                let next = if i + 1 < slots.len() { i as u16 + 1 } else { 0 };
                (*x, *y, (size & 0xFF00) | next, *attr)
            })
            .collect();
        sys_with_sprites_in(h40, &chained)
    }

    /// Slot 0 only, 2x2 cells at game (16, 16). `$90` is 16 once the 128 bias comes off, and the
    /// link byte is 0 — a one-sprite list, which is exactly what this fixture claims to be.
    fn sys_with_a_visible_sprite() -> System {
        sys_with_sprites(&[(0x0090, 0x0090, 0x0500, 0x0000)])
    }

    /// The fixture is only worth anything if the sprite really came through, so pin it here rather
    /// than discovering a silently-parked sprite as a confusing failure two tests down.
    #[test]
    fn the_sprite_fixture_really_puts_one_sprite_on_screen() {
        let sys = sys_with_a_visible_sprite();
        let vdp = sys.vdp();
        let bx = video::boxes(
            &vdp.render_line_report(0).sprites,
            &vdp.sprites_decoded(),
            vdp.active_display(),
        );
        assert_eq!(bx.len(), 1, "the other 79 slots stay parked");
        assert_eq!(bx[0].index, 0);
        assert_eq!(
            bx[0].rect,
            Rect {
                x: 16,
                y: 16,
                w: 16,
                h: 16
            }
        );
    }

    /// Every lens set with the outlines on, for the two tests below.
    fn outlines_on() -> LensSet {
        let mut s = LensSet::default();
        s.set(LensId::Sprites, true);
        s
    }

    /// The slots a model outlined, in order.
    fn outlined(sys: &System) -> Vec<u8> {
        let wp = Watchpoints::new(8);
        models(outlines_on(), &ctx(sys, &wp, false))
            .sprites
            .expect("the outline lens is on")
            .iter()
            .map(|b| b.index)
            .collect()
    }

    /// **The ghost-box regression.** The SAT holds 80 entries; the hardware only displays the ones
    /// reachable by walking `link` from slot 0 until a link of 0. Every other slot keeps whatever
    /// bytes were last written there, so outlining the *table* drew amber boxes over empty picture
    /// — which is what the owner saw on the glass.
    ///
    /// The chain here is 0 -> 2 -> 7 -> 0, and the six slots it skips are loaded with perfectly
    /// plausible on-screen sprites. Only three boxes may come back, and they must be those three
    /// slots in walk order: a fix that took the first three table entries, or that sorted by index,
    /// would produce three boxes too.
    ///
    /// The `decoded` assertion is the vacuity guard. If the ghosts were parked — as every slot of
    /// an unrun machine is — there would be nothing to wrongly outline and this test would pass
    /// against the very bug it is named for.
    #[test]
    fn unreachable_sat_slots_are_not_outlined() {
        // (x_field, y_field, size|link, attr). `$0500` is 2x2 cells; the low byte is the link.
        let mut slots: Vec<(u16, u16, u16, u16)> = (0..10u16)
            .map(|i| (128 + 10 + i * 20, 128 + 10, 0x0500, 0x0000))
            .collect();
        slots[0].2 = 0x0500 | 2; // 0 -> 2
        slots[2].2 = 0x0500 | 7; // 2 -> 7
        slots[7].2 = 0x0500; //     7 -> 0, end of the list
        let sys = sys_with_sprites_in(true, &slots);

        // Every one of the ten is a real, on-screen sprite as far as the *table* is concerned.
        let table = sys.vdp().sprites_decoded();
        for (slot, entry) in table.iter().take(10).enumerate() {
            assert!(
                entry.y == 10 && entry.x >= 10,
                "slot {slot} is not on screen, so it could never have been a ghost box"
            );
        }

        assert_eq!(
            outlined(&sys),
            vec![0, 2, 7],
            "the outlines followed the SAT rather than the link walk — the six unreachable slots \
             between them are ghosts the hardware never draws"
        );
    }

    /// The parse cap, end to end — the replacement for `slots_past_parsed_max_are_not_outlined`,
    /// which used to live in `lens/video.rs` and could not survive `boxes` losing its `parsed_max`
    /// argument (see the receipt there).
    ///
    /// **The fixture is a link *cycle*: 0 -> 1 -> 2 -> 1 -> 2 ...**, and only three slots hold any
    /// data at all. Nothing but the cap can stop that walk, and the hardware genuinely
    /// re-evaluates the same slots as it goes — which is the whole reason the cap exists. So the
    /// walk reports 80 entries in H40 and 64 in H32 from three sprites, a count no source but the
    /// walk can produce: reading the SAT table instead yields 3.
    ///
    /// **The obvious fixture — 80 chained visible slots — is vacuous, measured.** In H32 the SAT
    /// cache write-through only mirrors 64 entries (`vdp.rs:707`), so slots 64-79 stay parked and
    /// get clipped anyway. The whole-table mutation survived that version of this test while
    /// producing exactly the 64 it asserted, for entirely the wrong reason.
    #[test]
    fn the_walk_stops_at_the_parse_cap() {
        // (x_field, y_field, size|link, attr). Size byte $00 = 1x1 cell; the low byte is the link.
        let slots = [
            (128 + 10, 128 + 10, 0x0001, 0x0000), // slot 0 -> 1
            (128 + 30, 128 + 10, 0x0002, 0x0000), // slot 1 -> 2
            (128 + 50, 128 + 10, 0x0001, 0x0000), // slot 2 -> 1, and round again
        ];

        let h40 = outlined(&sys_with_sprites_in(true, &slots));
        assert_eq!(
            &h40[..5],
            &[0, 1, 2, 1, 2],
            "the walk is not following the link cycle, so the count below means nothing"
        );
        assert_eq!(h40.len(), 80, "H40 parses 80 slots before giving up");

        let h32 = outlined(&sys_with_sprites_in(false, &slots));
        assert_eq!(h32.len(), 64, "H32 parses only 64");
    }

    /// The sprite outlines end to end: the model is built for its own lens and no other, and it
    /// carries the visible sprites. The parallel of `the_cram_strip_model_is_built_for_its_own_lens_only`.
    #[test]
    fn the_sprite_outline_model_is_built_for_its_own_lens_only() {
        let sys = sys_with_a_visible_sprite();
        let wp = Watchpoints::new(8);

        assert!(
            models(LensSet::default(), &ctx(&sys, &wp, false))
                .sprites
                .is_none(),
            "outlines were built with every lens off"
        );
        let mut other = LensSet::default();
        other.set(LensId::Cram, true);
        assert!(
            models(other, &ctx(&sys, &wp, false)).sprites.is_none(),
            "the outline model keyed off the wrong lens"
        );
        let mut set = LensSet::default();
        set.set(LensId::Sprites, true);
        let m = models(set, &ctx(&sys, &wp, false));
        assert_eq!(
            m.sprites
                .as_deref()
                .expect("the outlines did not build for their own lens")
                .len(),
            1,
            "the one visible sprite"
        );
        assert!(
            m.ticker.is_none() && m.cpu.is_none() && m.cram.is_none() && m.hover.is_none(),
            "the outline lens dragged another model on with it"
        );
    }

    /// The hover callout end to end: built for its own lens and no other, **and only when the run
    /// loop found a dot**. The parallel of the two tests above, with the extra input the other
    /// lenses do not have.
    ///
    /// The no-dot case is the half that would rot silently: a `models` that ignored `cx.hover` and
    /// resolved some default dot would draw a callout in the corner of a window the mouse is
    /// nowhere near, and every other test here would stay green.
    ///
    /// The last assertion is the shared-decode obligation, from the reader's side: hover is on and
    /// the outlines are **off**, so `sprites_decoded` had to run for hover alone. A decode still
    /// gated on `Sprites` would leave `decoded` as `None` and no callout would be built at all.
    #[test]
    fn the_hover_callout_model_is_built_for_its_own_lens_and_only_with_a_dot() {
        let sys = sys_with_a_visible_sprite();
        let wp = Watchpoints::new(8);
        let only_hover = {
            let mut s = LensSet::default();
            s.set(LensId::Hover, true);
            s
        };

        assert!(
            models(only_hover, &ctx(&sys, &wp, false)).hover.is_none(),
            "a callout was built with the cursor off the picture"
        );
        assert!(
            models(LensSet::default(), &ctx_hovering(&sys, &wp, (24, 24)))
                .hover
                .is_none(),
            "a callout was built with every lens off"
        );
        let mut other = LensSet::default();
        other.set(LensId::Sprites, true);
        assert!(
            models(other, &ctx_hovering(&sys, &wp, (24, 24)))
                .hover
                .is_none(),
            "the callout model keyed off the wrong lens"
        );

        let m = models(only_hover, &ctx_hovering(&sys, &wp, (24, 25)));
        let hv = m
            .hover
            .expect("the callout did not build for its own lens with a dot");
        assert_eq!(
            hv.at,
            (24, 25),
            "the callout describes the dot it was given"
        );
        assert!(
            !hv.text.is_empty(),
            "the callout says nothing about the dot"
        );
        assert!(
            m.ticker.is_none() && m.cpu.is_none() && m.cram.is_none() && m.sprites.is_none(),
            "the hover lens dragged another model on with it"
        );
    }

    /// **Every lens that builds a model must reach the glass**, driven end to end:
    /// `models()` → [`draw`] with that one lens on, asserting ink.
    ///
    /// Each lens's own module already pixel-tests its `draw` fn directly, and every one of those
    /// tests stays green with the lens's arm in [`draw`] deleted outright. The resulting bug is
    /// nasty precisely because it looks like it works: the lens toasts ON, persists to the config
    /// across relaunch, rebuilds its model every frame — and never paints. A user-visible dead
    /// feature with a fully green suite.
    ///
    /// Table-driven over [`LensId::ALL`] rather than one test per lens, so lens seven is covered by
    /// arithmetic rather than by somebody remembering. The lenses need no per-lens fixtures for
    /// this: an unrun machine and an unarmed watchpoint set are enough, because the question is
    /// "did any ink land", not "was it the right ink" (their own modules answer that).
    ///
    /// Exactly one lens is on per iteration, so the ink is unambiguously that lens's — a shared
    /// buffer with several on would let one lens cover for another's missing arm.
    ///
    /// `BG` is `0x0012_3456` and not `0` for the reason every lens draw test states: the panels are
    /// black alpha-blended, invisible over a zero buffer — and an unrun machine's CRAM is all
    /// zeroes, so its swatches are black too. Over zeroes this test would assert nothing at all.
    ///
    /// The context **hovers a dot**, for the same class of reason the sprite fixture exists: the
    /// hover callout builds no model at all without one, so with `hover: None` this table would
    /// find hover building nothing, painting nothing, and passing `has_model == painted` while
    /// telling us precisely nothing about its arm. Supplying a dot is the honest fix; excusing
    /// hover from the table would hand back the escape hatch the table exists to close.
    #[test]
    fn every_lens_with_a_draw_arm_reaches_the_glass() {
        const BG: u32 = 0x0012_3456;
        const ARMS_TODAY: usize = 6;
        let (w, h) = (320usize, 224usize);
        let area = Rect { x: 0, y: 0, w, h };
        // Not `System::new` on its own: an unrun machine has every sprite parked, so the outline
        // lens would build a model and correctly paint nothing, and this test would read that as a
        // missing arm. See `sys_with_a_visible_sprite`.
        let sys = sys_with_a_visible_sprite();
        let wp = Watchpoints::new(8);

        let mut witnessed = 0;
        for id in LensId::ALL {
            let mut set = LensSet::default();
            set.set(id, true);
            let m = models(set, &ctx_hovering(&sys, &wp, (24, 24)));
            let mut buf = vec![BG; w * h];
            draw(&mut buf, w, h, area, (w, h), &m);
            let painted = buf.iter().any(|p| *p != BG);

            // **Built implies drawn.** Asking only "did ink land" leaves one escape hatch wide
            // open: a lens whose model `models()` builds every frame, whose arm in `draw` nobody
            // wrote, and whose author dutifully declared `draws_yet => false`. That passes — it was
            // built and demonstrated on this very test — and it is exactly the dead-feature bug the
            // test exists to catch. Tying the two together closes it without retiring `draws_yet`,
            // which still forces an author to state intent.
            //
            // The destructure is the load-bearing part: a new `Models` field is a **compile error**
            // here, so Tasks 8 and 9 cannot add a model without being sent to this line. Counting
            // `Some`s through anything generic over the struct would quietly absorb the new field
            // and hand the escape hatch straight back.
            let Models {
                ticker,
                cpu,
                cram,
                sprites,
                hover,
            } = &m;
            let has_model = ticker.is_some()
                || cpu.is_some()
                || cram.is_some()
                || sprites.is_some()
                || hover.is_some();
            assert_eq!(
                has_model,
                painted,
                "{}: a model was built and no ink landed (a missing arm in `draw`), or ink landed \
                 with no model behind it",
                id.key()
            );

            if draws_yet(id) {
                assert!(
                    painted,
                    "{} builds a model every frame and never reaches the glass — its arm in \
                     `draw` is missing",
                    id.key()
                );
                witnessed += 1;
            } else {
                assert!(
                    !painted,
                    "{} drew without being declared in `draws_yet` — say so there, so the arm \
                     stays witnessed",
                    id.key()
                );
            }
        }
        // Without this, a `draws_yet` that answered `false` everywhere would skip every positive
        // assertion and pass having checked nothing. The count is written out, never taken from
        // `ALL` or from a filter over `draws_yet` itself.
        assert_eq!(
            witnessed, ARMS_TODAY,
            "expected {ARMS_TODAY} lenses to paint; a lens gained or lost an arm without this \
             test being told"
        );
    }

    /// **The layering witness.** Wherever a panel puts down an *opaque* pixel — a CRAM swatch, a
    /// glyph — the sprite outlines never change it: the outlines draw first and the panel covers
    /// them. Under the panels' translucent bodies the outlines survive, dimmed, which is the other
    /// half of the same decision.
    ///
    /// Without this the whole ordering is unpinned — swapping any two arms in [`draw`] leaves the
    /// suite green, because every other test here turns on one lens at a time. And the gap is worse
    /// than an ordinary untested-doc gap for the CRAM strip: its swatches are deliberately
    /// **opaque** so that a swatch is the palette entry exactly ("a swatch blended over the picture
    /// would be a different colour from the entry it is reporting"), so a 200-alpha hairline
    /// through one reports a colour CRAM never held. Same class for the chip and the ticker, whose
    /// glyphs are text: a stroke through `$3F` reads as `$8F`.
    ///
    /// **Opacity is measured, not assumed.** Rendering the panel alone over two different
    /// backgrounds identifies exactly the pixels whose value does not depend on what was underneath
    /// — that is what opaque *means*, and it needs no knowledge of any panel's geometry, alpha or
    /// glyph shapes, so it cannot go stale when one moves. The claim is then made over all of them
    /// at once rather than at a hand-picked probe, and no alpha blend is ever recomputed here:
    /// every assertion compares renders of the same buffer against each other.
    ///
    /// Three vacuity traps are closed explicitly. The outlines must **cross** each panel's opaque
    /// pixels, or "they agree there" is a statement about nothing. They must have **drawn** in the
    /// combined render, or a deleted arm in [`draw`] satisfies every equality. And at least one
    /// translucent panel pixel must **differ**, or a panel that had gone fully opaque would pass
    /// while quietly erasing the outlines the doc promises it only dims.
    #[test]
    fn the_panels_draw_over_the_sprite_outlines() {
        const BG: u32 = 0x0012_3456;
        const BG2: u32 = 0x0065_4321;
        let (w, h) = (320usize, 224usize);
        let area = Rect { x: 0, y: 0, w, h };
        // A 32-pixel grid of 4x4-cell sprites across the whole picture, so the outline bars cross
        // every panel wherever the panels happen to sit — 70 of the 80 slots, all inside H40's
        // 320x224 display. Hand-placing one sprite per panel would re-derive each panel's geometry
        // here and go stale the moment a panel moved.
        //
        // `$8000` sets each sprite's **priority** bit, so the strokes are `ACCENT` rather than
        // `INFO`. That is load-bearing, not decoration: the ticker's and chip's glyphs are `INFO`
        // too, and white blended over white at any alpha is white — so with ordinary sprites this
        // test passed with the outlines drawn *over* the ticker, blind to exactly the reordering it
        // exists to catch. Measured, not reasoned: moving the arm one line down survived.
        let slots: Vec<(u16, u16, u16, u16)> = (0..7u16)
            .flat_map(|row| {
                (0..10u16).map(move |col| (col * 32 + 128, row * 32 + 128, 0x0F00, 0x8000))
            })
            .collect();
        assert_eq!(slots.len(), 70, "inside the 80 the SAT holds");
        // Chained: the walk follows links, so an unchained grid would be a one-sprite list.
        let sys = sys_with_chained_sprites(true, &slots);
        let wp = Watchpoints::new(8);

        // A dot near the top-left, so the callout is on screen and the outline grid crosses it.
        let render = |set: LensSet, bg: u32| {
            let m = models(set, &ctx_hovering(&sys, &wp, (24, 24)));
            let mut buf = vec![bg; w * h];
            draw(&mut buf, w, h, area, (w, h), &m);
            buf
        };
        let only = |id: LensId| {
            let mut s = LensSet::default();
            s.set(id, true);
            s
        };
        let outlines = render(only(LensId::Sprites), BG);
        assert!(
            outlines.iter().any(|p| *p != BG),
            "the outline fixture painted nothing, so nothing below is measuring anything"
        );

        for id in [
            LensId::Watch,
            LensId::Cpu,
            LensId::CpuRegs,
            LensId::Cram,
            LensId::Hover,
        ] {
            let panel = render(only(id), BG);
            let panel_on_other = render(only(id), BG2);
            let mut with_sprites = only(id);
            with_sprites.set(LensId::Sprites, true);
            let both = render(with_sprites, BG);

            let mut opaque = 0usize;
            let mut crossings = 0usize;
            let mut dimmed = 0usize;
            for i in 0..w * h {
                // Opaque here means "the same whatever was underneath" — which is precisely what
                // the two backgrounds measure, and it also implies the panel drew at all (BG and
                // BG2 differ, so an untouched pixel can never agree).
                if panel[i] == panel_on_other[i] {
                    opaque += 1;
                    if outlines[i] != BG {
                        crossings += 1;
                    }
                    assert_eq!(
                        both[i], panel[i],
                        "{}: an outline changed an opaque panel pixel at ({}, {}) — the panel must \
                         win every crossing, or it reports something the machine never held",
                        id.key(),
                        i % w,
                        i / w
                    );
                } else if panel[i] != BG && both[i] != panel[i] {
                    // `panel[i] != BG` is what makes this the *translucent panel* branch rather
                    // than "everywhere else": a pixel the panel never touched is BG here and BG2
                    // there, so it fails the opacity test above and lands in this arm too. Without
                    // the guard `dimmed` just counts outline pixels out in the open, which the last
                    // assertion already covers — measured: with it missing, turning every panel
                    // opaque left this clause green.
                    dimmed += 1;
                }
            }
            assert!(opaque > 0, "{}: the panel drew nothing opaque", id.key());
            assert!(
                crossings > 0,
                "{}: the outlines never cross this panel's opaque pixels, so the equality above \
                 checked nothing",
                id.key()
            );
            assert!(
                dimmed > 0,
                "{}: no outline survives under the panel's translucent body — it is erasing them, \
                 not dimming them, and the position an outline exists to convey is lost",
                id.key()
            );
            assert!(
                (0..w * h).any(|i| panel[i] == BG && both[i] != BG),
                "{}: the outlines left no mark at all beside the panel — a deleted arm in `draw` \
                 would pass every assertion above",
                id.key()
            );
        }
    }

    /// **The other end of the layering order: the hover callout wins over every fixed panel.**
    ///
    /// The outlines lose every crossing because a hairline through a readout is silent wrongness
    /// (see above). The callout is the opposite case and gets the opposite answer: it is the one
    /// lens the user is actively pointing at, and half of it hidden under a strip that was already
    /// on screen is not an answer. Neither half of that order is pinned by anything else — with one
    /// lens on at a time every other test in this module is blind to it, and moving the callout's
    /// arm up two lines leaves the suite green.
    ///
    /// The CRAM strip is the panel to test it against, and not an arbitrary choice: it is the only
    /// one whose ink is deliberately **opaque** (a swatch must be the palette entry exactly), so it
    /// is the panel that would obliterate rather than dim, and it owns the top-left corner where a
    /// dot near the origin puts the callout.
    ///
    /// Opacity is measured rather than assumed, the same way as above: rendering the callout over
    /// two different backgrounds identifies the pixels whose value does not depend on what was
    /// underneath, with no knowledge of glyph shapes or alphas, so it cannot go stale.
    #[test]
    fn the_hover_callout_draws_over_the_other_panels() {
        const BG: u32 = 0x0012_3456;
        const BG2: u32 = 0x0065_4321;
        let (w, h) = (320usize, 224usize);
        let area = Rect { x: 0, y: 0, w, h };
        let sys = System::new(0x5EED);
        let wp = Watchpoints::new(8);
        // A 1:1 picture, so game dot (16,16) is window (16,16) and the callout — 4 px down and to
        // the right of it, one text row tall — lands across the strip's rows.
        let render = |set: LensSet, bg: u32| {
            let m = models(set, &ctx_hovering(&sys, &wp, (16, 16)));
            let mut buf = vec![bg; w * h];
            draw(&mut buf, w, h, area, (w, h), &m);
            buf
        };
        let only = |id: LensId| {
            let mut s = LensSet::default();
            s.set(id, true);
            s
        };

        let callout = render(only(LensId::Hover), BG);
        let callout_on_other = render(only(LensId::Hover), BG2);
        let strip = render(only(LensId::Cram), BG);
        let mut both_on = only(LensId::Hover);
        both_on.set(LensId::Cram, true);
        let both = render(both_on, BG);

        let mut opaque = 0usize;
        let mut crossings = 0usize;
        for i in 0..w * h {
            if callout[i] != callout_on_other[i] {
                continue; // translucent, or never touched: the panel body is allowed to blend
            }
            opaque += 1;
            if strip[i] != BG {
                crossings += 1;
            }
            assert_eq!(
                both[i],
                callout[i],
                "a panel covered an opaque callout pixel at ({}, {}) — the callout draws last",
                i % w,
                i / w
            );
        }
        assert!(opaque > 0, "the callout drew nothing opaque");
        assert!(
            crossings > 0,
            "the callout never overlaps the strip, so the equality above checked nothing"
        );
        // And the strip really did draw underneath, or a `draw` that had lost the CRAM arm
        // entirely would satisfy every equality above.
        assert!(
            (0..w * h).any(|i| callout[i] == BG && both[i] != BG),
            "the strip left no mark beside the callout"
        );
    }

    /// **The collision the owner actually saw.** At their 896x672 window the font scale is 3, and
    /// the expanded CPU chip — sized to a fifty-glyph PC symbol — clamped to the full width of the
    /// picture and drew straight over the CRAM strip in the opposite corner.
    ///
    /// This is the regression test for that, at exactly that geometry, driven end to end through
    /// [`models`] and [`draw`] so it covers the sizing, the scale drop and the anchoring together.
    /// The two panels own opposite corners, so the claim is simply that their ink never shares a
    /// column — an assertion that is false for a full-width panel however tall it is.
    ///
    /// The long symbol is placed at the machine's own PC so it genuinely resolves, and the model's
    /// first line is checked before anything is drawn: without that, a fixture whose symbol quietly
    /// failed to resolve would leave a short `$00FF00` PC line, no collision, and a test that
    /// passed against the bug it is named for.
    #[test]
    fn the_expanded_chip_clears_the_cram_strip_at_the_owners_geometry() {
        const BG: u32 = 0x0012_3456;
        let (w, h) = (896usize, 672usize);
        let area = Rect { x: 0, y: 0, w, h };
        let sys = System::new(0x5EED);
        let wp = Watchpoints::new(8);
        let pc = sys.cpu_regs().pc;
        let symbols = oracle_core::symbols::SymbolTable::parse(&format!(
            "  Symbol Table (* = unused):\n  --------------------------\n\n \
             GAMESTATE_OJZSCROLL.UPDATE.SKIP_ENTITY_SCAN_THEN_KEEP_GOING : {pc:X} C |\n\n    \
             1 symbols\n    0 unused symbols\n"
        ))
        .expect("the fixture listing parses");

        let render = |id: LensId| {
            let mut set = LensSet::default();
            set.set(id, true);
            let m = models(
                set,
                &FrameCtx {
                    sys: &sys,
                    wp: &wp,
                    symbols: Some(&symbols),
                    frame: 7,
                    paused: false,
                    hover: None,
                },
            );
            if id == LensId::CpuRegs {
                let line = &m.cpu.as_ref().expect("the chip is on").lines[0];
                // Long enough that sizing the panel to it would still reach across the picture
                // *after* the expanded block's scale drop — otherwise this test would only ever
                // fail when both halves of the fix were reverted at once, and would be blind to
                // either on its own. Measured: at 46 glyphs it was.
                assert!(
                    line.chars().count() >= 60,
                    "the fixture symbol did not resolve, or is too short to have collided: \
                     {line:?}"
                );
            }
            let mut buf = vec![BG; w * h];
            draw(&mut buf, w, h, area, (320, 224), &m);
            ink_bounds(&buf, w, BG).unwrap_or_else(|| panic!("{} painted nothing", id.key()))
        };

        let (_, _, strip_left, strip_right) = render(LensId::Cram);
        let (_, _, chip_left, chip_right) = render(LensId::CpuRegs);
        assert!(
            chip_left > strip_right,
            "the CPU chip (columns {chip_left}..={chip_right}) overlaps the CRAM strip (columns \
             {strip_left}..={strip_right})"
        );
        // And it is a right-hand panel that got smaller, not one that moved away by growing off
        // the left: it must still hug the right edge.
        assert!(
            chip_right > area.x + area.w - 16,
            "the chip stopped hugging the right edge (last ink at column {chip_right})"
        );
    }

    /// A variant can be added to `LensId` — forcing `key`/`title`/`label` edits — and still be
    /// left out of `ALL`, where it would get no bit-uniqueness check, no config spelling and no
    /// palette command. The match below is exhaustive, so a new variant fails to *compile* until
    /// it is handled here; the assertions then fail until it is in `ALL` too.
    #[test]
    fn all_lists_every_variant_exactly_once() {
        // `seen` is sized by VARIANTS, deliberately NOT by `ALL.len()`: sizing it by `ALL` makes
        // the test vacuous for the one bug it exists to catch, because a variant missing from
        // `ALL` also shrinks the thing you are checking against.
        const VARIANTS: usize = 6;
        fn slot(id: LensId) -> usize {
            match id {
                LensId::Watch => 0,
                LensId::Cpu => 1,
                LensId::CpuRegs => 2,
                LensId::Sprites => 3,
                LensId::Cram => 4,
                LensId::Hover => 5,
            }
        }
        let mut seen = [false; VARIANTS];
        for id in LensId::ALL {
            let s = slot(id);
            assert!(!seen[s], "{} appears in ALL twice", id.key());
            seen[s] = true;
        }
        assert!(
            seen.iter().all(|s| *s),
            "a LensId variant is missing from ALL"
        );
        assert_eq!(LensId::ALL.len(), VARIANTS, "ALL and VARIANTS disagree");
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
