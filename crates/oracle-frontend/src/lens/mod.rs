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
/// Grouped rather than passed positionally because the list only grows — the remaining lenses add
/// the VDP, the blit's forward map and the mouse position — and several of them are same-typed
/// references, which is the point at which a transposed call site stops being a compile error.
/// Collected here while it is five fields rather than at the last lens, when it would be eight.
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
}

/// Build the models for whatever is on. Called once per frame, immediately before drawing, and
/// skipped entirely when nothing is on and the machine is running.
pub fn models(set: LensSet, cx: &FrameCtx<'_>) -> Models {
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
        // `sprites_decoded` decodes all 80 slots and allocates ~1.3 KB every call, so it is called
        // **once**, here, behind the lens guard — never per sprite and never again in the draw path.
        sprites: set.is_on(LensId::Sprites).then(|| {
            let vdp = cx.sys.vdp();
            video::boxes(
                &vdp.sprites_decoded(),
                vdp.parsed_sprite_max(),
                vdp.active_display(),
            )
        }),
    }
}

/// Draw every built model, in a fixed back-to-front order — [`LensId::ALL`] order, so a later lens
/// draws over an earlier one. The ticker (bottom), the chip (top-right) and the CRAM strip
/// (top-left, one text row down) each own a different corner, and on an ordinary picture none of
/// them meet. Two pairs can, and both resolve by draw order rather than by geometry:
///
/// - **Ticker vs chip.** Only on a picture short enough for the chip's panel to reach the bottom
///   strip, where the chip wins — the same overlap `watch.rs` already accepts against the toasts,
///   and for the same reason.
/// - **Strip vs chip.** These two share a band *vertically at every size*: the strip's rows sit
///   inside the compact chip's panel exactly (at `px = 1`, rows 16..32 against the chip's 4..32).
///   All that separates them is horizontal, so they collide once the picture is narrower than
///   `2 * margin + strip_w + chip_w` — 207 device pixels at `px = 1` with the register block
///   showing, which a 200-wide picture is already inside: measured, they overlap by 112 pixels
///   there. The **strip wins**, being later in [`LensId::ALL`]. That is the right way round — the
///   strip is 64 fixed cells whose whole meaning is positional, so clipping it would misreport
///   CRAM, while the chip loses a few glyphs off one end of two lines and stays readable.
///
/// Both are accepted rather than resolved: a picture that small has no arrangement where five
/// panels fit, and moving one lens under another by size would make the layout jump around while
/// a window is dragged.
///
/// The sprite outlines are the exception to all of that: they are drawn *on* the picture rather
/// than in a corner of it, so they own no corner and can cross any panel. [`LensId::ALL`] puts them
/// after the ticker and the chip and before the strip, so they draw **over** those two and **under**
/// the strip — and that is the same principle the strip-vs-chip call above uses, not an accident of
/// ordering. An outline means nothing except *where it is*: move it and it is wrong, so it is the
/// one thing here that must never be displaced by something that can afford to lose a few glyphs.
/// The strip still wins over the outlines for the same reason it wins over the chip — 64 fixed
/// cells whose meaning is equally positional, and which a hairline through them would misreport.
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
    if let Some(t) = &m.ticker {
        watch::draw(&mut c, area, px, t);
    }
    if let Some(chip) = &m.cpu {
        cpu::draw(&mut c, area, px, chip);
    }
    if let Some(bx) = &m.sprites {
        video::draw_sprites(&mut c, area, px, native, bx);
    }
    if let Some(sw) = &m.cram {
        video::draw_cram(&mut c, area, px, sw);
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

#[cfg(test)]
mod tests {
    use super::*;

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
            LensId::Watch | LensId::Cpu | LensId::CpuRegs | LensId::Sprites | LensId::Cram => true,
            // No arm in `draw` yet — the hover callout is still to come. When it lands, flipping
            // this to `true` is what the test below demands of it.
            LensId::Hover => false,
        }
    }

    /// A machine with exactly one visible sprite, and the only fixture in this module that puts
    /// anything into the VDP at all.
    ///
    /// It has to: a power-on SAT cache is all zeroes, so every one of the 80 slots decodes as
    /// `y == -128` — the parked idiom — and the outline lens correctly draws nothing. Run against
    /// that, [`every_lens_with_a_draw_arm_reaches_the_glass`] would assert a model was built and no
    /// ink landed, and read as a missing arm in [`draw`] when the arm was there and right.
    ///
    /// Written through the VDP's own ports rather than by poking VRAM, because the Y/size/link half
    /// of a sprite comes from the **SAT cache**, which only the write-through on a VRAM port write
    /// fills (`vram_mut` would leave the cache at zero and the sprite parked). Slot 0 at the
    /// power-on reg-5 base (VRAM `$0000`), 2x2 cells, at game (16, 16) — the fields are biased by
    /// 128, so `$90` is 16.
    ///
    /// **Mode 5 has to be turned on first.** A power-on VDP has reg 1 bit 2 (M5) clear, which is
    /// Mode 4, and there `write_register` discards every register above 10 — including reg $0F, the
    /// auto-increment. Without this line all four data writes land on address `$0000`, the size
    /// byte never reaches the cache, and the sprite decodes 1x1 at whatever `System::new`'s seeded
    /// VRAM happens to hold in the X word. That is not a hypothetical; it is what this fixture did
    /// on its first run.
    fn sys_with_a_visible_sprite() -> System {
        let mut sys = System::new(0x5EED);
        let v = sys.vdp_mut();
        v.control_write(0x8104, 0); // reg $01 = M5: leave Mode 4, where regs above 10 are discarded
        v.control_write(0x8F02, 0); // reg $0F: auto-increment 2 bytes per data write
        v.control_write(0x4000, 0); // VRAM write ...
        v.control_write(0x0000, 0); // ... at address $0000, the SAT base
        v.data_write(0x0090); // Y field 144 -> screen y = 16
        v.data_write(0x0500); // size = 2x2 cells (bits 3-2 width-1, 1-0 height-1); link 0
        v.data_write(0x0000); // tile / attributes
        v.data_write(0x0090); // X field 144 -> screen x = 16
        sys
    }

    /// The fixture is only worth anything if the sprite really came through, so pin it here rather
    /// than discovering a silently-parked sprite as a confusing failure two tests down.
    #[test]
    fn the_sprite_fixture_really_puts_one_sprite_on_screen() {
        let sys = sys_with_a_visible_sprite();
        let vdp = sys.vdp();
        let bx = video::boxes(
            &vdp.sprites_decoded(),
            vdp.parsed_sprite_max(),
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
            m.ticker.is_none() && m.cpu.is_none() && m.cram.is_none(),
            "the outline lens dragged another model on with it"
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
    #[test]
    fn every_lens_with_a_draw_arm_reaches_the_glass() {
        const BG: u32 = 0x0012_3456;
        const ARMS_TODAY: usize = 5;
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
            let m = models(set, &ctx(&sys, &wp, false));
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
            } = &m;
            let has_model =
                ticker.is_some() || cpu.is_some() || cram.is_some() || sprites.is_some();
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
