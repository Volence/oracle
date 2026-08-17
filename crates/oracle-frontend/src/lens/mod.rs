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

pub mod watch;

use crate::present::Rect;
use oracle_core::symbols::SymbolTable;
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
    /// Whether anything is on. The run loop's draw guard: with everything off it skips [`models`]
    /// entirely, so a lens that is not on costs nothing — not even the reads its model would make.
    pub fn any(self) -> bool {
        self.0 != 0
    }
}

/// Everything the enabled lenses need to draw this frame, extracted once. Absent = that lens is
/// off, so [`draw`] never has to know the set.
pub struct Models {
    pub ticker: Option<watch::Ticker>,
}

/// Build the models for whatever is on. Called once per frame, immediately before drawing, and
/// skipped entirely when nothing is on.
pub fn models(set: LensSet, wp: &Watchpoints, symbols: Option<&SymbolTable>) -> Models {
    Models {
        ticker: set
            .is_on(LensId::Watch)
            .then(|| watch::model(wp, symbols, watch::ROWS)),
    }
}

/// Draw every built model, in a fixed back-to-front order. Anchored to `area` (the picture), never
/// the window: the letterbox stays black, and a tall window with a narrow picture must not make
/// the font wider than the panel (the `draw_narrow_panel_does_not_underflow` hazard class).
pub fn draw(buf: &mut [u32], w: usize, h: usize, area: Rect, m: &Models) {
    let px = crate::overlay::Overlay::font_scale(area.h.max(1));
    let mut c = crate::font::Canvas::new(buf, w, h);
    if let Some(t) = &m.ticker {
        watch::draw(&mut c, area, px, t);
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

    /// The guard that keeps a lens that is off from costing anything: no model is built for it, so
    /// the reads its model would make never happen.
    #[test]
    fn models_are_built_only_for_lenses_that_are_on() {
        let wp = Watchpoints::new(8);
        let mut set = LensSet::default();
        assert!(
            models(set, &wp, None).ticker.is_none(),
            "a model was built for a lens that is off"
        );
        set.set(LensId::Watch, true);
        assert!(
            models(set, &wp, None).ticker.is_some(),
            "no model was built for a lens that is on"
        );
        // A different lens must not switch the ticker on — the models are keyed per id.
        let mut other = LensSet::default();
        other.set(LensId::Cram, true);
        assert!(
            models(other, &wp, None).ticker.is_none(),
            "the ticker model keyed off the wrong lens"
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
