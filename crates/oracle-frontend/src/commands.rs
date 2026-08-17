//! The command registry — the single source of truth for every frontend action (spec §4).
//!
//! Metadata only: each entry names a [`Cmd`], its palette title, its group, and its default key.
//! The main loop owns the actions in one `match cmd` (state lives there); the palette renders
//! this table; the binding loop reads keys from it. Adding a command here yields the hotkey,
//! the palette entry, and searchability — there is no second list to update.

use minifb::Key;

/// Every action the frontend can perform. `Copy` on purpose: dispatch passes these by value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    Pause,
    Step,
    Reset,
    ReloadRom,
    Quit,
    SaveState,
    LoadState,
    SlotPrev,
    SlotNext,
    /// Open the slot picker in the palette ("Select slot…").
    SlotPicker,
    /// Direct slot select (the number keys; hidden from the palette list).
    SlotSelect(usize),
    DumpHits,
    ClearWatch,
    ToggleStatusLine,
    VolumeUp,
    VolumeDown,
    MuteToggle,
}

/// Palette group headers, in display order (spec §4: group by subsystem).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Game,
    SaveStates,
    Watch,
    Settings,
}

impl Group {
    pub const ALL: [Group; 4] = [
        Group::Game,
        Group::SaveStates,
        Group::Watch,
        Group::Settings,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Group::Game => "GAME",
            Group::SaveStates => "SAVE STATES",
            Group::Watch => "WATCH",
            Group::Settings => "SETTINGS",
        }
    }
}

/// One registry row. `hidden` rows bind a key but do not appear in the palette list (the ten
/// number keys would drown the list; the visible "Select slot…" picker covers them).
pub struct CommandInfo {
    pub cmd: Cmd,
    pub title: &'static str,
    pub group: Group,
    pub hotkey: Option<Key>,
    /// `true` = fire on key repeat while held (volume ramp); everything else is edge-only.
    pub repeat: bool,
    pub hidden: bool,
}

impl CommandInfo {
    const fn new(cmd: Cmd, title: &'static str, group: Group, hotkey: Option<Key>) -> Self {
        CommandInfo {
            cmd,
            title,
            group,
            hotkey,
            repeat: false,
            hidden: false,
        }
    }
}

/// The full table. Built at startup, immutable after.
pub fn registry() -> Vec<CommandInfo> {
    let mut reg = vec![
        CommandInfo::new(Cmd::Pause, "Pause / resume", Group::Game, Some(Key::Space)),
        CommandInfo::new(Cmd::Step, "Step one frame", Group::Game, Some(Key::Period)),
        // Tab honors Gens/Fusion muscle memory (spec §3); F1 remains as a hidden alias below.
        CommandInfo::new(
            Cmd::Reset,
            "Soft reset (SRAM kept)",
            Group::Game,
            Some(Key::Tab),
        ),
        CommandInfo::new(
            Cmd::ReloadRom,
            "Reload ROM from disk + reset",
            Group::Game,
            Some(Key::F5),
        ),
        CommandInfo::new(Cmd::Quit, "Quit", Group::Game, None),
        CommandInfo::new(
            Cmd::SaveState,
            "Save state to current slot",
            Group::SaveStates,
            Some(Key::F2),
        ),
        CommandInfo::new(
            Cmd::LoadState,
            "Load state from current slot",
            Group::SaveStates,
            Some(Key::F4),
        ),
        CommandInfo::new(
            Cmd::SlotPrev,
            "Previous save slot",
            Group::SaveStates,
            Some(Key::F6),
        ),
        CommandInfo::new(
            Cmd::SlotNext,
            "Next save slot",
            Group::SaveStates,
            Some(Key::F7),
        ),
        CommandInfo::new(
            Cmd::SlotPicker,
            "Select save slot...",
            Group::SaveStates,
            None,
        ),
        CommandInfo::new(
            Cmd::DumpHits,
            "Dump watch hits to terminal",
            Group::Watch,
            Some(Key::W),
        ),
        CommandInfo::new(
            Cmd::ClearWatch,
            "Clear armed watches",
            Group::Watch,
            Some(Key::C),
        ),
        CommandInfo::new(
            Cmd::ToggleStatusLine,
            "Toggle status line",
            Group::Settings,
            Some(Key::F3),
        ),
    ];
    // F1 = reset alias (hidden: one visible "Soft reset" row is enough).
    reg.push(CommandInfo {
        cmd: Cmd::Reset,
        title: "Soft reset (F1 alias)",
        group: Group::Game,
        hotkey: Some(Key::F1),
        repeat: false,
        hidden: true,
    });
    // Number keys 0-9 -> direct slot select, hidden (SLOT_KEYS order, main.rs:358).
    const SLOT_TITLES: [&str; 10] = [
        "Select slot 0",
        "Select slot 1",
        "Select slot 2",
        "Select slot 3",
        "Select slot 4",
        "Select slot 5",
        "Select slot 6",
        "Select slot 7",
        "Select slot 8",
        "Select slot 9",
    ];
    let slot_keys = [
        Key::Key0,
        Key::Key1,
        Key::Key2,
        Key::Key3,
        Key::Key4,
        Key::Key5,
        Key::Key6,
        Key::Key7,
        Key::Key8,
        Key::Key9,
    ];
    for (n, key) in slot_keys.iter().enumerate() {
        reg.push(CommandInfo {
            cmd: Cmd::SlotSelect(n),
            title: SLOT_TITLES[n],
            group: Group::SaveStates,
            hotkey: Some(*key),
            repeat: false,
            hidden: true,
        });
    }
    // Audio-only commands: absent from a no-audio build entirely (spec §4 "a command is absent
    // only when it cannot exist").
    #[cfg(feature = "audio")]
    {
        reg.push(CommandInfo {
            cmd: Cmd::VolumeUp,
            title: "Volume up",
            group: Group::Settings,
            hotkey: Some(Key::Equal),
            repeat: true,
            hidden: false,
        });
        reg.push(CommandInfo {
            cmd: Cmd::VolumeDown,
            title: "Volume down",
            group: Group::Settings,
            hotkey: Some(Key::Minus),
            repeat: true,
            hidden: false,
        });
        reg.push(CommandInfo::new(
            Cmd::MuteToggle,
            "Mute / unmute",
            Group::Settings,
            Some(Key::M),
        ));
    }
    reg
}

/// Case-insensitive subsequence match: every char of `query`, in order, appears in `title`
/// ("ssl" hits "Save state to current slot"). Hand-rolled on purpose — no fuzzy-rank crate
/// (spec §4). ASCII-lowercase is enough: titles are ASCII by construction.
pub fn subseq_match(query: &str, title: &str) -> bool {
    let mut t = title.chars().map(|c| c.to_ascii_lowercase());
    query
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .all(|q| t.any(|c| c == q))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every visible title is unique — two commands with one name would be indistinguishable
    /// in the palette.
    #[test]
    fn titles_unique() {
        let reg = registry();
        let mut titles: Vec<&str> = reg.iter().filter(|c| !c.hidden).map(|c| c.title).collect();
        assert!(!titles.is_empty(), "registry must not be empty");
        titles.sort_unstable();
        let before = titles.len();
        titles.dedup();
        assert_eq!(before, titles.len(), "duplicate visible titles");
    }

    /// A physical key maps to at most one command — a duplicated default binding would make
    /// dispatch order-dependent.
    #[test]
    fn hotkeys_unique() {
        let reg = registry();
        let mut keys: Vec<Key> = reg.iter().filter_map(|c| c.hotkey).collect();
        assert!(!keys.is_empty());
        keys.sort_unstable_by_key(|k| *k as u32);
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "one key bound to two commands");
    }

    /// Every group in `Group::ALL` has at least one visible member (an empty header would
    /// render as a dangling label).
    #[test]
    fn groups_nonempty() {
        let reg = registry();
        for g in Group::ALL {
            assert!(
                reg.iter().any(|c| c.group == g && !c.hidden),
                "group {:?} has no visible commands",
                g
            );
        }
    }

    /// The number keys 0-9 each bind SlotSelect(n) with n matching the key, hidden.
    #[test]
    fn slot_selects_cover_all_slots() {
        let reg = registry();
        for n in 0..crate::save_state::SLOT_COUNT {
            assert!(
                reg.iter()
                    .any(|c| c.cmd == Cmd::SlotSelect(n) && c.hidden && c.hotkey.is_some()),
                "missing hidden SlotSelect({n})"
            );
        }
    }

    #[test]
    fn subseq_match_cases() {
        // (query, title, expected)
        let cases = [
            ("", "Pause / resume", true), // empty matches everything
            ("wat", "Dump watch hits to terminal", true),
            ("WAT", "dump watch hits", true), // case-insensitive both sides
            ("ssl", "Save state to current slot", true), // subsequence, not substring
            ("xyz", "Pause / resume", false),
            ("pausex", "Pause / resume", false), // exhausted title before query
        ];
        for (q, t, want) in cases {
            assert_eq!(subseq_match(q, t), want, "query {q:?} vs {t:?}");
        }
    }
}
