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
//! This module is currently the spine only — ids, the toggle bitset, and the config-file
//! spelling. The lenses themselves (`watch`, `cpu`, `video`) arrive in the following tasks, each
//! declaring its own submodule; a placeholder file here would be dead weight the gate would
//! rightly flag.

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
}

/// Parse the config file's `lenses` value: a comma-separated list of [`LensId::key`] spellings.
/// Unknown names warn and are dropped rather than failing the line — a newer build's lens must
/// not cost an older build its whole setting (the same forward-compatibility rule the config's
/// unknown *keys* follow). Empty items are skipped silently so `a,,b` and a trailing comma are
/// both fine.
pub fn parse_set(value: &str) -> (LensSet, Vec<String>) {
    let mut set = LensSet::default();
    let mut warnings = Vec::new();
    for name in value.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        match LensId::ALL.iter().find(|id| id.key() == name) {
            Some(id) => set.set(*id, true),
            None => warnings.push(format!("config: ignored lens `{name}` (unknown lens)")),
        }
    }
    (set, warnings)
}

/// The inverse, in [`LensId::ALL`] order so the file is stable across saves (an unstable order
/// would rewrite the file — and wake the debounce — on every launch).
pub fn format_set(set: LensSet) -> String {
    LensId::ALL
        .iter()
        .filter(|id| set.is_on(**id))
        .map(|id| id.key())
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
        let text = format_set(set);
        assert_eq!(text, "watch,cram", "stable order, ALL order");
        let (back, warnings) = parse_set(&text);
        assert_eq!(back, set);
        assert!(warnings.is_empty(), "own output warned: {warnings:?}");
    }

    #[test]
    fn every_lens_round_trips_and_the_empty_set_is_empty() {
        for id in LensId::ALL {
            let mut set = LensSet::default();
            set.set(id, true);
            let (back, warnings) = parse_set(&format_set(set));
            assert_eq!(back, set, "{} did not round-trip", id.key());
            assert!(warnings.is_empty());
        }
        assert_eq!(format_set(LensSet::default()), "");
        assert_eq!(parse_set("").0, LensSet::default());
        assert!(
            parse_set("").1.is_empty(),
            "an empty value is not a warning"
        );
    }

    #[test]
    fn an_unknown_lens_warns_and_leaves_the_rest_alone() {
        let (set, warnings) = parse_set("watch, from_the_future ,cram");
        assert!(set.is_on(LensId::Watch) && set.is_on(LensId::Cram));
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("from_the_future"),
            "the warning names it"
        );
    }

    /// `any()` (the run loop's draw guard) lands with its first caller in the lens-draw task, so
    /// this pins only what exists here: toggle is its own inverse, and agrees with `is_on`.
    #[test]
    fn toggle_agrees_with_is_on() {
        let mut set = LensSet::default();
        assert!(!set.is_on(LensId::Hover));
        set.toggle(LensId::Hover);
        assert!(set.is_on(LensId::Hover));
        set.toggle(LensId::Hover);
        assert!(!set.is_on(LensId::Hover), "toggle is its own inverse");
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
