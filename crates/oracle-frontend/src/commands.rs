//! The command registry — the single source of truth for every frontend action (spec §4).
//!
//! Metadata only: each entry names a [`Cmd`], its palette title, its group, and its default key.
//! The main loop owns the actions in one `match cmd` (state lives there); the palette renders
//! this table; the binding loop reads keys from it. Adding a command here yields the hotkey,
//! the palette entry, and searchability — there is no second list to update.

use minifb::Key;
use oracle_core::render::LayerMask;
use std::borrow::Cow;

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
    /// Turn one lens on or off (spec §5). Payload-carrying like `SlotSelect`, so one arm and one
    /// registration loop cover every lens.
    ToggleLens(crate::lens::LensId),
    /// Hide or show one **display layer** — the same mask `emulator/set_layer_enabled` moves, carried as
    /// the core's own [`Layer`](oracle_core::render::Layer) rather than a frontend enum of the same shape.
    /// The payload is the mask *target*, so `Layer::Sprite`'s slot is the `Layer::ALL` representative and
    /// means "the sprite layer", never one sprite.
    ToggleLayer(oracle_core::render::Layer),
    // Audio-only, and absent — not merely unbound — from a no-audio build: with nothing to attenuate
    // the command genuinely *cannot* exist (spec §4), which is also what keeps the main loop's
    // dispatch exhaustive without a dead catch-all arm.
    #[cfg(feature = "audio")]
    VolumeUp,
    #[cfg(feature = "audio")]
    VolumeDown,
    #[cfg(feature = "audio")]
    MuteToggle,
}

/// Palette group headers, in display order (spec §4: group by subsystem).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Game,
    SaveStates,
    Watch,
    Lenses,
    /// The display layer mask — the same one `emulator/set_layer_enabled` moves. Its own group rather than
    /// a corner of `Lenses`: a lens draws *over* the picture and a layer toggle changes *what the picture
    /// is*, and a user hunting for "why is the background gone" should not have to know they are the same
    /// kind of thing, because they are not.
    Layers,
    Settings,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::Game,
        Group::SaveStates,
        Group::Watch,
        Group::Lenses,
        Group::Layers,
        Group::Settings,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Group::Game => "GAME",
            Group::SaveStates => "SAVE STATES",
            Group::Watch => "WATCH",
            Group::Lenses => "LENSES",
            Group::Layers => "DISPLAY LAYERS",
            Group::Settings => "SETTINGS",
        }
    }
}

/// The palette title for one display-layer toggle, built from the layer's own mask name.
///
/// One function so the registry row and any other reader cannot disagree, and it takes the **name** rather
/// than the `Layer` so a caller cannot reach it for the backdrop — which has no mask name and no toggle.
pub fn layer_toggle_title(mask_key: &str) -> String {
    format!("Hide / show {mask_key}")
}

/// One registry row. `hidden` rows bind a key but do not appear in the palette list (the ten
/// number keys would drown the list; the visible "Select slot…" picker covers them).
pub struct CommandInfo {
    pub cmd: Cmd,
    /// The palette row's text. A [`Cow`] rather than a `&'static str` because the display-layer rows
    /// **build** their titles out of the core's own mask vocabulary ([`LayerMask::targets`]) instead of
    /// transcribing four names the wire also spells — which is the whole reason that derivation moved into
    /// `oracle-core`. Every other row is still a borrowed literal and allocates nothing.
    pub title: Cow<'static, str>,
    pub group: Group,
    pub hotkey: Option<Key>,
    /// `true` = fire on key repeat while held (volume ramp); everything else is edge-only.
    pub repeat: bool,
    pub hidden: bool,
}

impl CommandInfo {
    fn new(
        cmd: Cmd,
        title: impl Into<Cow<'static, str>>,
        group: Group,
        hotkey: Option<Key>,
    ) -> Self {
        CommandInfo {
            cmd,
            title: title.into(),
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
        title: "Soft reset (F1 alias)".into(),
        group: Group::Game,
        hotkey: Some(Key::F1),
        repeat: false,
        hidden: true,
    });
    // Number keys 0-9 -> direct slot select, hidden. This table is the only place that mapping lives —
    // the main loop's old `SLOT_KEYS` array is gone, so a slot key and its command cannot drift apart.
    // Both arrays are typed by `SLOT_COUNT` on purpose (the guarantee the deleted const carried): adding a
    // slot without adding its key and title is a *compile* error here, never a runtime index panic on
    // `slots_on_disk[n]` in the main loop.
    const SLOT_TITLES: [&str; crate::save_state::SLOT_COUNT] = [
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
    let slot_keys: [Key; crate::save_state::SLOT_COUNT] = [
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
            title: SLOT_TITLES[n].into(),
            group: Group::SaveStates,
            hotkey: Some(*key),
            repeat: false,
            hidden: true,
        });
    }
    // One toggle per lens, generated from `LensId::ALL` for the reason the slot loop is generated:
    // a lens that exists without a command, or a command naming a lens that no longer exists, is a
    // compile error rather than a row nobody notices is missing. Palette-only — every obvious key
    // is already taken and `hotkeys_unique` would catch a collision; S5 owns binding.
    for id in crate::lens::LensId::ALL {
        reg.push(CommandInfo::new(
            Cmd::ToggleLens(id),
            id.title(),
            Group::Lenses,
            None,
        ));
    }
    // One toggle per display layer, generated from the **core's** `LayerMask::targets()` — the same
    // derivation that produces `emulator/get_layer_states`'s key set and `emulator/set_layer_enabled`'s
    // accepted values. Nothing here transcribes a layer name, so the palette cannot offer a layer the bus
    // does not have, spell one differently, or miss one that is added. The backdrop is absent for free:
    // it has no mask key, so `targets()` never yields it.
    //
    // Palette-only, exactly like the lens toggles and for the same reason: every obvious key is taken, and
    // a mask is a thing you set deliberately and then leave set. The badge, not a key under a finger, is
    // what tells you it is on.
    for (name, layer) in LayerMask::targets() {
        reg.push(CommandInfo::new(
            Cmd::ToggleLayer(layer),
            layer_toggle_title(name),
            Group::Layers,
            None,
        ));
    }
    // Audio-only commands: absent from a no-audio build entirely (spec §4 "a command is absent
    // only when it cannot exist").
    #[cfg(feature = "audio")]
    {
        reg.push(CommandInfo {
            cmd: Cmd::VolumeUp,
            title: "Volume up".into(),
            group: Group::Settings,
            hotkey: Some(Key::Equal),
            repeat: true,
            hidden: false,
        });
        reg.push(CommandInfo {
            cmd: Cmd::VolumeDown,
            title: "Volume down".into(),
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

/// Short display name for the palette's hotkey column. Only keys the registry actually uses
/// need names; anything else renders "?" (and the test below keeps the registry inside the
/// named set).
pub fn key_name(k: Key) -> &'static str {
    match k {
        Key::Space => "Space",
        Key::Period => ".",
        Key::Tab => "Tab",
        Key::Enter => "Enter",
        Key::Minus => "-",
        Key::Equal => "=",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::W => "W",
        Key::C => "C",
        Key::M => "M",
        Key::Key0 => "0",
        Key::Key1 => "1",
        Key::Key2 => "2",
        Key::Key3 => "3",
        Key::Key4 => "4",
        Key::Key5 => "5",
        Key::Key6 => "6",
        Key::Key7 => "7",
        Key::Key8 => "8",
        Key::Key9 => "9",
        _ => "?",
    }
}

/// The typable subset for palette filtering: a-z, 0-9, space. Anything else (F-keys, the
/// backtick that opened the palette, punctuation) is not text. Lowercase only — the matcher
/// is case-insensitive so shift adds nothing.
pub fn key_char(k: Key) -> Option<char> {
    use Key::*;
    let c = match k {
        A => 'a',
        B => 'b',
        C => 'c',
        D => 'd',
        E => 'e',
        F => 'f',
        G => 'g',
        H => 'h',
        I => 'i',
        J => 'j',
        K => 'k',
        L => 'l',
        M => 'm',
        N => 'n',
        O => 'o',
        P => 'p',
        Q => 'q',
        R => 'r',
        S => 's',
        T => 't',
        U => 'u',
        V => 'v',
        W => 'w',
        X => 'x',
        Y => 'y',
        Z => 'z',
        Key0 => '0',
        Key1 => '1',
        Key2 => '2',
        Key3 => '3',
        Key4 => '4',
        Key5 => '5',
        Key6 => '6',
        Key7 => '7',
        Key8 => '8',
        Key9 => '9',
        Space => ' ',
        _ => return None,
    };
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::render::Layer;

    /// Every visible title is unique — two commands with one name would be indistinguishable
    /// in the palette.
    #[test]
    fn titles_unique() {
        let reg = registry();
        let mut titles: Vec<&str> = reg
            .iter()
            .filter(|c| !c.hidden)
            .map(|c| c.title.as_ref())
            .collect();
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

    /// Every lens must reach the palette, or a toggle exists with no way to reach it.
    #[test]
    fn every_lens_registers_a_visible_command() {
        let reg = registry();
        for id in crate::lens::LensId::ALL {
            let row = reg
                .iter()
                .find(|c| c.cmd == Cmd::ToggleLens(id))
                .unwrap_or_else(|| panic!("no command for lens {}", id.key()));
            assert!(!row.hidden, "{} is unreachable from the palette", id.key());
            assert_eq!(row.group, Group::Lenses);
            assert_eq!(
                row.title,
                id.title(),
                "the row and the lens must not drift apart"
            );
        }
    }

    /// Lens toggles are palette-only this slice (S5 owns rebinding); a default hotkey added here
    /// without thought would collide silently with the game keys.
    #[test]
    fn lens_toggles_bind_no_keys_yet() {
        for c in registry() {
            if matches!(c.cmd, Cmd::ToggleLens(_)) {
                assert_eq!(c.hotkey, None, "{} bound a key", c.title);
            }
        }
    }

    /// **Every mask target the bus serves is reachable from the palette, and nothing else is.**
    ///
    /// The expectation is [`LayerMask::targets`] — the same derivation that produces
    /// `emulator/get_layer_states`'s key set and `emulator/set_layer_enabled`'s accepted values, which
    /// `oracle-aether/tests/layers.rs` pins against the vendored contract fragment. So this row is not a
    /// transcription of four names; it is "the window offers exactly what the wire accepts", and a layer
    /// added to the contract that the palette did not grow a row for fails **here**.
    ///
    /// Both directions, because only one of them is the interesting failure: a missing row is a feature
    /// nobody can reach, and an *extra* row is a toggle that would call `set_layer` with something the
    /// mask refuses — the backdrop being the live candidate, since it is a `Layer` and is not a target.
    #[test]
    fn every_mask_target_gets_a_visible_toggle_and_nothing_else_does() {
        let reg = registry();
        let targets = LayerMask::targets();
        assert!(
            !targets.is_empty(),
            "COULD NOT MEASURE: the core reports no mask targets, so this row proves nothing"
        );
        for (name, layer) in &targets {
            let row = reg
                .iter()
                .find(|c| c.cmd == Cmd::ToggleLayer(*layer))
                .unwrap_or_else(|| panic!("no palette row for the `{name}` layer"));
            assert!(!row.hidden, "`{name}` is unreachable from the palette");
            assert_eq!(row.group, Group::Layers);
            assert_eq!(
                row.title,
                layer_toggle_title(name),
                "the row and the layer's own name must not drift apart"
            );
            assert!(
                row.title.contains(name),
                "the palette must spell the layer the way the wire does, so a user can type it: {}",
                row.title
            );
        }
        // Nothing outside the target set, and in particular not the backdrop.
        let registered: Vec<Layer> = reg
            .iter()
            .filter_map(|c| match c.cmd {
                Cmd::ToggleLayer(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(
            registered.len(),
            targets.len(),
            "the palette offers {} layer toggles for {} mask targets",
            registered.len(),
            targets.len()
        );
        for l in &registered {
            assert!(
                l.mask_key().is_some(),
                "{l:?} has no mask name, so `LayerMask::set` would refuse it — it must not be offered"
            );
        }
    }

    /// Layer toggles are palette-only, for the reason the lens toggles are: every obvious key is taken,
    /// and `hotkeys_unique` would only catch the collision after someone shipped it.
    #[test]
    fn layer_toggles_bind_no_keys() {
        for c in registry() {
            if matches!(c.cmd, Cmd::ToggleLayer(_)) {
                assert_eq!(c.hotkey, None, "{} bound a key", c.title);
            }
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

    #[test]
    fn key_names_for_all_registry_hotkeys() {
        // Every default hotkey must render something readable in the palette's right column.
        for c in registry() {
            if let Some(k) = c.hotkey {
                assert!(!key_name(k).is_empty(), "no name for {:?}", k);
                assert_ne!(key_name(k), "?", "unnamed key {:?}", k);
            }
        }
    }

    #[test]
    fn key_char_covers_typing() {
        assert_eq!(key_char(Key::A), Some('a'));
        assert_eq!(key_char(Key::Z), Some('z'));
        assert_eq!(key_char(Key::Key0), Some('0'));
        assert_eq!(key_char(Key::Key9), Some('9'));
        assert_eq!(key_char(Key::Space), Some(' '));
        assert_eq!(key_char(Key::F5), None); // function keys never type
        assert_eq!(key_char(Key::Backquote), None); // the open key must not self-insert
    }
}
