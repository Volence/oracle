# Player S1 — Command Registry + Bindings + Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the frontend's hand-maintained hotkey if-chain with a data-driven command registry, and add a modal, searchable command palette (backtick to open) over the running game.

**Architecture:** A metadata-only registry (`commands.rs`) is the single source of truth for every action: id (a `Cmd` enum), title, group, default key. The main loop dispatches through ONE `match cmd` (actions stay where the mutable state lives — idiomatic Rust; no closures over loop state). `palette.rs` is a pure state machine + renderer with zero I/O, tested without a window. Spec: `docs/superpowers/specs/2026-08-16-player-buildout-design.md` §3–§4.

**Tech Stack:** Rust, existing crates only (minifb 0.28, `font::Canvas`, `overlay`, `present`). Zero new dependencies. Zero `oracle-core` changes.

**Verified facts (do not re-derive):** `Key::Backquote` exists (minifb-0.28.0/src/key.rs:63). `window.get_keys_pressed(KeyRepeat) -> Vec<Key>` exists (lib.rs:767). `present::Rect` is `{x, y, w, h}: usize` (present.rs:65). Font API: `font::Canvas::new(buf, w, h)`, `.fill_rect(x, y, w, h, color, alpha)`, `.text(x, y, px, color, text) -> usize`, `font::ADVANCE=6`, `font::LINE_H=8`, `font::text_width`, `font::PANEL_ALPHA` (font.rs). `overlay::{INFO, ACCENT}`, `Overlay::font_scale(win_h)`.

**House rules that bind every task:** run tests with `cargo test -p oracle-frontend` — NEVER pipe through `tail` (hides failures and the exit code). Every evidence-bearing test gets a mutation check at writing time: break the implementation as the step says, SEE the test fail, revert, and record one line `mutation: <what you broke> -> <which test failed>` in that task's commit message body. Commit messages: plain `feat:`/`refactor:` style, no Co-Authored-By trailers.

**File structure for the slice:**

| File | Responsibility |
|---|---|
| `crates/oracle-frontend/src/commands.rs` (create) | `Cmd`, `Group`, `CommandInfo`, `registry()`, `subseq_match`, `key_name`, `key_char` — pure data + pure functions |
| `crates/oracle-frontend/src/palette.rs` (create) | Palette state machine (`handle`), row model (`rows`), picker, MRU, `draw` — pure over inputs |
| `crates/oracle-frontend/src/main.rs` (modify) | declare modules; bindings loop replaces the if-chain; palette wiring; Esc/Tab/quit changes; doc header |

---

### Task 1: `commands.rs` — Cmd, Group, CommandInfo, registry, invariants

**Files:**
- Create: `crates/oracle-frontend/src/commands.rs`
- Modify: `crates/oracle-frontend/src/main.rs` (add `mod commands;` next to the existing `mod overlay;` block)
- Test: in-file `#[cfg(test)]` (house style — every existing module tests in-file)

- [ ] **Step 1: Write the module with types and an EMPTY registry, plus the failing invariant tests**

Create `crates/oracle-frontend/src/commands.rs`:

```rust
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
    pub const ALL: [Group; 4] = [Group::Game, Group::SaveStates, Group::Watch, Group::Settings];
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
        CommandInfo { cmd, title, group, hotkey, repeat: false, hidden: false }
    }
}

/// The full table. Built at startup, immutable after.
pub fn registry() -> Vec<CommandInfo> {
    Vec::new() // Step 3 fills this in
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
                reg.iter().any(|c| c.cmd == Cmd::SlotSelect(n) && c.hidden && c.hotkey.is_some()),
                "missing hidden SlotSelect({n})"
            );
        }
    }
}
```

In `main.rs`, next to the existing module declarations (search for `mod overlay;`), add:

```rust
mod commands;
mod palette; // created in Task 4; comment this line out until then if you build in between
```

(Leave `mod palette;` commented until Task 4 so this task builds.)

- [ ] **Step 2: Run tests, verify they FAIL**

Run: `cargo test -p oracle-frontend commands::`
Expected: FAIL — `titles_unique` and `hotkeys_unique` assert non-empty, `groups_nonempty` finds no members.

- [ ] **Step 3: Fill in the registry**

Replace the `registry()` body:

```rust
pub fn registry() -> Vec<CommandInfo> {
    let mut reg = vec![
        CommandInfo::new(Cmd::Pause, "Pause / resume", Group::Game, Some(Key::Space)),
        CommandInfo::new(Cmd::Step, "Step one frame", Group::Game, Some(Key::Period)),
        // Tab honors Gens/Fusion muscle memory (spec §3); F1 remains as a hidden alias below.
        CommandInfo::new(Cmd::Reset, "Soft reset (SRAM kept)", Group::Game, Some(Key::Tab)),
        CommandInfo::new(Cmd::ReloadRom, "Reload ROM from disk + reset", Group::Game, Some(Key::F5)),
        CommandInfo::new(Cmd::Quit, "Quit", Group::Game, None),
        CommandInfo::new(Cmd::SaveState, "Save state to current slot", Group::SaveStates, Some(Key::F2)),
        CommandInfo::new(Cmd::LoadState, "Load state from current slot", Group::SaveStates, Some(Key::F4)),
        CommandInfo::new(Cmd::SlotPrev, "Previous save slot", Group::SaveStates, Some(Key::F6)),
        CommandInfo::new(Cmd::SlotNext, "Next save slot", Group::SaveStates, Some(Key::F7)),
        CommandInfo::new(Cmd::SlotPicker, "Select save slot...", Group::SaveStates, None),
        CommandInfo::new(Cmd::DumpHits, "Dump watch hits to terminal", Group::Watch, Some(Key::W)),
        CommandInfo::new(Cmd::ClearWatch, "Clear armed watches", Group::Watch, Some(Key::C)),
        CommandInfo::new(Cmd::ToggleStatusLine, "Toggle status line", Group::Settings, Some(Key::F3)),
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
        "Select slot 0", "Select slot 1", "Select slot 2", "Select slot 3", "Select slot 4",
        "Select slot 5", "Select slot 6", "Select slot 7", "Select slot 8", "Select slot 9",
    ];
    let slot_keys = [
        Key::Key0, Key::Key1, Key::Key2, Key::Key3, Key::Key4,
        Key::Key5, Key::Key6, Key::Key7, Key::Key8, Key::Key9,
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
        reg.push(CommandInfo::new(Cmd::MuteToggle, "Mute / unmute", Group::Settings, Some(Key::M)));
    }
    reg
}
```

- [ ] **Step 4: Run tests, verify they PASS (both feature variants)**

Run: `cargo test -p oracle-frontend commands::` then `cargo test -p oracle-frontend --no-default-features commands::`
Expected: PASS ×2. (No-default-features drops the audio rows; `groups_nonempty` still passes because `ToggleStatusLine` keeps Settings populated.)

- [ ] **Step 5: Mutation checks (record each line in the commit body)**

1. Duplicate a hotkey (give `Cmd::Pause` `Some(Key::W)`): expect `hotkeys_unique` FAIL. Revert.
2. Delete the `SlotSelect` push loop: expect `slot_selects_cover_all_slots` FAIL. Revert.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add crates/oracle-frontend/src/commands.rs crates/oracle-frontend/src/main.rs
git commit -m "feat(frontend): command registry — single source of truth for actions" \
  -m "mutation: duplicated Pause hotkey -> hotkeys_unique FAIL" \
  -m "mutation: removed SlotSelect loop -> slot_selects_cover_all_slots FAIL"
```

---

### Task 2: `commands.rs` — subsequence matcher

**Files:**
- Modify: `crates/oracle-frontend/src/commands.rs`

- [ ] **Step 1: Write the failing tests** (append inside `mod tests`)

```rust
    #[test]
    fn subseq_match_cases() {
        // (query, title, expected)
        let cases = [
            ("", "Pause / resume", true),          // empty matches everything
            ("wat", "Dump watch hits to terminal", true),
            ("WAT", "dump watch hits", true),      // case-insensitive both sides
            ("ssl", "Save state to current slot", true), // subsequence, not substring
            ("xyz", "Pause / resume", false),
            ("pausex", "Pause / resume", false),   // exhausted title before query
        ];
        for (q, t, want) in cases {
            assert_eq!(subseq_match(q, t), want, "query {q:?} vs {t:?}");
        }
    }
```

- [ ] **Step 2: Run to verify FAIL**

Run: `cargo test -p oracle-frontend commands::subseq`
Expected: FAIL — `subseq_match` not defined (compile error).

- [ ] **Step 3: Implement** (above the tests module)

```rust
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
```

- [ ] **Step 4: Run to verify PASS**

Run: `cargo test -p oracle-frontend commands::subseq` — Expected: PASS.

- [ ] **Step 5: Mutation check**

Change `.all(` to `.any(` : expect `subseq_match_cases` FAIL on the `"xyz"` case. Revert; record.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -u
git commit -m "feat(frontend): palette subsequence matcher" \
  -m "mutation: all->any in subseq_match -> subseq_match_cases FAIL"
```

---

### Task 3: `commands.rs` — key names (for the hotkey column) and key→char (for typing)

**Files:**
- Modify: `crates/oracle-frontend/src/commands.rs`

- [ ] **Step 1: Failing tests** (append inside `mod tests`)

```rust
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
```

- [ ] **Step 2: Run to verify FAIL** — `cargo test -p oracle-frontend commands::key` → compile error (functions missing).

- [ ] **Step 3: Implement**

```rust
/// Short display name for the palette's hotkey column. Only keys the registry actually uses
/// need names; anything else renders "?" (and the test above keeps the registry inside the
/// named set).
pub fn key_name(k: Key) -> &'static str {
    match k {
        Key::Space => "Space",
        Key::Period => ".",
        Key::Tab => "Tab",
        Key::Enter => "Enter",
        Key::Minus => "-",
        Key::Equal => "=",
        Key::F1 => "F1", Key::F2 => "F2", Key::F3 => "F3", Key::F4 => "F4",
        Key::F5 => "F5", Key::F6 => "F6", Key::F7 => "F7",
        Key::W => "W", Key::C => "C", Key::M => "M",
        Key::Key0 => "0", Key::Key1 => "1", Key::Key2 => "2", Key::Key3 => "3",
        Key::Key4 => "4", Key::Key5 => "5", Key::Key6 => "6", Key::Key7 => "7",
        Key::Key8 => "8", Key::Key9 => "9",
        _ => "?",
    }
}

/// The typable subset for palette filtering: a-z, 0-9, space. Anything else (F-keys, the
/// backtick that opened the palette, punctuation) is not text. Lowercase only — the matcher
/// is case-insensitive so shift adds nothing.
pub fn key_char(k: Key) -> Option<char> {
    use Key::*;
    let c = match k {
        A => 'a', B => 'b', C => 'c', D => 'd', E => 'e', F => 'f', G => 'g', H => 'h',
        I => 'i', J => 'j', K => 'k', L => 'l', M => 'm', N => 'n', O => 'o', P => 'p',
        Q => 'q', R => 'r', S => 's', T => 't', U => 'u', V => 'v', W => 'w', X => 'x',
        Y => 'y', Z => 'z',
        Key0 => '0', Key1 => '1', Key2 => '2', Key3 => '3', Key4 => '4',
        Key5 => '5', Key6 => '6', Key7 => '7', Key8 => '8', Key9 => '9',
        Space => ' ',
        _ => return None,
    };
    Some(c)
}
```

- [ ] **Step 4: Run to verify PASS** — `cargo test -p oracle-frontend commands::` → all PASS.

- [ ] **Step 5: Mutation check** — remove the `Key::Tab => "Tab"` arm: expect `key_names_for_all_registry_hotkeys` FAIL (Tab is Reset's hotkey). Revert; record.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -u
git commit -m "feat(frontend): key names and key->char for the palette" \
  -m "mutation: dropped Tab arm from key_name -> key_names_for_all_registry_hotkeys FAIL"
```

---

### Task 4: `palette.rs` — state machine (open/filter/navigate/run + MRU)

**Files:**
- Create: `crates/oracle-frontend/src/palette.rs`
- Modify: `crates/oracle-frontend/src/main.rs` (uncomment/add `mod palette;`)

- [ ] **Step 1: Write the module skeleton with failing tests**

Create `crates/oracle-frontend/src/palette.rs`:

```rust
//! The command palette — the modal control surface (spec §3–§4). Pure state machine: input is
//! [`PaletteKey`], output is [`PaletteAction`]; no window, no I/O, fully testable headless.
//! The main loop feeds it keys while open and swallows game input; the game keeps RUNNING
//! behind it (dev-first: the watch ticker stays live while you type).

use crate::commands::{self, Cmd, CommandInfo, Group};

/// Keys the palette understands, already translated from minifb by the caller
/// (`commands::key_char` for the typable set).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaletteKey {
    Char(char),
    Backspace,
    Up,
    Down,
    Enter,
    Esc,
}

/// What the main loop should do after a key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaletteAction {
    None,
    /// Run this command (palette has closed itself).
    Run(Cmd),
}

/// One visible row: a group header or an index into the registry slice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Header(&'static str),
    Item(usize),
}

/// A secondary pick list ("Select save slot..."), opened by the main loop with concrete items.
pub struct Picker {
    pub title: String,
    /// (label, command to run when chosen)
    pub items: Vec<(String, Cmd)>,
    pub sel: usize,
}

pub struct Palette {
    open: bool,
    query: String,
    /// Selection as an index into the CURRENT `rows()` output, always on an `Item` row.
    sel: usize,
    /// Most-recently-used commands, newest first, capped at MRU_CAP, visible-only.
    recents: Vec<Cmd>,
    picker: Option<Picker>,
}

pub const MRU_CAP: usize = 3;

impl Palette {
    pub fn new() -> Self {
        Palette { open: false, query: String::new(), sel: 0, recents: Vec::new(), picker: None }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.picker = None;
        self.sel = 0;
    }
    pub fn close(&mut self) {
        self.open = false;
        self.picker = None;
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn picker(&self) -> Option<&Picker> {
        self.picker.as_ref()
    }
    /// Open a secondary pick list (the main loop builds the items — occupancy etc. lives there).
    pub fn open_picker(&mut self, title: String, items: Vec<(String, Cmd)>) {
        self.picker = Some(Picker { title, items, sel: 0 });
        self.open = true;
    }

    /// The rows the palette shows for its current query. Empty query = grouped full list with
    /// an optional RECENT section on top (spec §4: the empty palette IS the menu). Non-empty
    /// query = flat filtered list, no headers. Hidden registry rows never appear.
    pub fn rows(&self, reg: &[CommandInfo]) -> Vec<Row> {
        Vec::new() // Step 3
    }

    /// Feed one key. Returns what the caller should do. Selection is clamped to Item rows;
    /// Enter on an Item runs it (recording MRU) and closes; Esc closes (picker first).
    pub fn handle(&mut self, key: PaletteKey, reg: &[CommandInfo]) -> PaletteAction {
        PaletteAction::None // Step 3
    }

    pub fn sel(&self) -> usize {
        self.sel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::registry;

    fn open_palette() -> (Palette, Vec<CommandInfo>) {
        let mut p = Palette::new();
        p.open();
        (p, registry())
    }

    /// Empty query: every group header present in order, every visible command present,
    /// no hidden command present.
    #[test]
    fn empty_query_lists_everything_grouped() {
        let (p, reg) = open_palette();
        let rows = p.rows(&reg);
        let headers: Vec<&str> = rows
            .iter()
            .filter_map(|r| if let Row::Header(h) = r { Some(*h) } else { None })
            .collect();
        for g in Group::ALL {
            assert!(headers.contains(&g.title()), "missing header {}", g.title());
        }
        let visible = reg.iter().filter(|c| !c.hidden).count();
        let items = rows.iter().filter(|r| matches!(r, Row::Item(_))).count();
        assert_eq!(items, visible, "every visible command listed exactly once");
        for r in &rows {
            if let Row::Item(i) = r {
                assert!(!reg[*i].hidden, "hidden command leaked into the list");
            }
        }
    }

    /// Typing filters; headers disappear; the filtered set is exactly the matching titles.
    #[test]
    fn typing_filters() {
        let (mut p, reg) = open_palette();
        for c in "watch".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        let rows = p.rows(&reg);
        assert!(rows.iter().all(|r| matches!(r, Row::Item(_))), "no headers while filtering");
        assert!(!rows.is_empty());
        for r in &rows {
            if let Row::Item(i) = r {
                assert!(
                    commands::subseq_match("watch", reg[*i].title),
                    "non-matching row {}",
                    reg[*i].title
                );
            }
        }
    }

    /// Enter runs the selected command, closes the palette, and records it in MRU; reopening
    /// shows it under RECENT.
    #[test]
    fn enter_runs_and_records_mru() {
        let (mut p, reg) = open_palette();
        // Filter down to exactly one row to make the selection deterministic.
        for c in "dump".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        let rows = p.rows(&reg);
        assert_eq!(rows.len(), 1, "'dump' should match exactly the dump-hits command");
        let act = p.handle(PaletteKey::Enter, &reg);
        assert_eq!(act, PaletteAction::Run(Cmd::DumpHits));
        assert!(!p.is_open(), "palette closes after running");
        p.open();
        let rows = p.rows(&reg);
        assert_eq!(rows[0], Row::Header("RECENT"));
        match rows[1] {
            Row::Item(i) => assert_eq!(reg[i].cmd, Cmd::DumpHits),
            _ => panic!("first recent row is not an item"),
        }
    }

    /// Up/Down move the selection over Item rows only (headers are skipped) and clamp at the
    /// ends; backspace un-filters; Esc closes without running.
    #[test]
    fn navigation_and_esc() {
        let (mut p, reg) = open_palette();
        assert_eq!(p.handle(PaletteKey::Down, &reg), PaletteAction::None);
        let rows = p.rows(&reg);
        assert!(matches!(rows[p.sel()], Row::Item(_)), "selection sits on an item");
        p.handle(PaletteKey::Up, &reg);
        p.handle(PaletteKey::Up, &reg); // clamp at top, no panic
        for c in "zzzz".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        assert!(p.rows(&reg).is_empty(), "nothing matches zzzz");
        assert_eq!(p.handle(PaletteKey::Enter, &reg), PaletteAction::None, "enter on empty is a no-op");
        for _ in 0..4 {
            p.handle(PaletteKey::Backspace, &reg);
        }
        assert!(!p.rows(&reg).is_empty(), "backspace restored the list");
        assert_eq!(p.handle(PaletteKey::Esc, &reg), PaletteAction::None);
        assert!(!p.is_open());
    }

    /// The picker: arrows move, Enter yields the picked command, Esc falls back to the main
    /// list (not a full close).
    #[test]
    fn picker_flow() {
        let (mut p, reg) = open_palette();
        p.open_picker(
            "SELECT SLOT".into(),
            vec![("slot 0".into(), Cmd::SlotSelect(0)), ("slot 1".into(), Cmd::SlotSelect(1))],
        );
        p.handle(PaletteKey::Down, &reg);
        let act = p.handle(PaletteKey::Enter, &reg);
        assert_eq!(act, PaletteAction::Run(Cmd::SlotSelect(1)));
        assert!(!p.is_open());
        // Esc inside a picker returns to the list, palette stays open.
        p.open_picker("SELECT SLOT".into(), vec![("slot 0".into(), Cmd::SlotSelect(0))]);
        p.handle(PaletteKey::Esc, &reg);
        assert!(p.is_open(), "esc closes the picker, not the palette");
        assert!(p.picker().is_none());
    }
}
```

Add `mod palette;` to `main.rs` (uncomment the line from Task 1).

- [ ] **Step 2: Run to verify FAIL**

Run: `cargo test -p oracle-frontend palette::`
Expected: FAIL — `rows` returns empty (all five tests), `handle` is a no-op.

- [ ] **Step 3: Implement `rows` and `handle`**

Replace the two stub bodies:

```rust
    pub fn rows(&self, reg: &[CommandInfo]) -> Vec<Row> {
        let mut out = Vec::new();
        if self.query.is_empty() {
            // RECENT section first (visible commands only, newest first).
            let recent_idx: Vec<usize> = self
                .recents
                .iter()
                .filter_map(|cmd| reg.iter().position(|c| c.cmd == *cmd && !c.hidden))
                .collect();
            if !recent_idx.is_empty() {
                out.push(Row::Header("RECENT"));
                out.extend(recent_idx.into_iter().map(Row::Item));
            }
            for g in Group::ALL {
                let members: Vec<usize> = reg
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.group == g && !c.hidden)
                    .map(|(i, _)| i)
                    .collect();
                if !members.is_empty() {
                    out.push(Row::Header(g.title()));
                    out.extend(members.into_iter().map(Row::Item));
                }
            }
        } else {
            out.extend(
                reg.iter()
                    .enumerate()
                    .filter(|(_, c)| !c.hidden && commands::subseq_match(&self.query, c.title))
                    .map(|(i, _)| Row::Item(i)),
            );
        }
        out
    }

    pub fn handle(&mut self, key: PaletteKey, reg: &[CommandInfo]) -> PaletteAction {
        // Picker mode intercepts everything.
        if let Some(pk) = self.picker.as_mut() {
            match key {
                PaletteKey::Up => pk.sel = pk.sel.saturating_sub(1),
                PaletteKey::Down => pk.sel = (pk.sel + 1).min(pk.items.len().saturating_sub(1)),
                PaletteKey::Enter => {
                    if let Some((_, cmd)) = pk.items.get(pk.sel) {
                        let cmd = *cmd;
                        self.close();
                        return PaletteAction::Run(cmd);
                    }
                }
                PaletteKey::Esc => self.picker = None, // back to the list, palette stays open
                PaletteKey::Char(_) | PaletteKey::Backspace => {}
            }
            return PaletteAction::None;
        }
        match key {
            PaletteKey::Char(c) => {
                self.query.push(c);
                self.sel = 0;
            }
            PaletteKey::Backspace => {
                self.query.pop();
                self.sel = 0;
            }
            PaletteKey::Up => self.move_sel(reg, -1),
            PaletteKey::Down => self.move_sel(reg, 1),
            PaletteKey::Esc => self.close(),
            PaletteKey::Enter => {
                let rows = self.rows(reg);
                if let Some(Row::Item(i)) = rows.get(self.sel) {
                    let cmd = reg[*i].cmd;
                    self.record_recent(cmd);
                    self.close();
                    return PaletteAction::Run(cmd);
                }
            }
        }
        // Keep the selection on an Item row (the list may have changed under it).
        let rows = self.rows(reg);
        if !rows.is_empty() {
            self.sel = self.sel.min(rows.len() - 1);
            if matches!(rows[self.sel], Row::Header(_)) {
                self.move_sel(reg, 1);
            }
        } else {
            self.sel = 0;
        }
        PaletteAction::None
    }

    /// Move the selection to the next/previous `Item` row, skipping headers, clamped.
    fn move_sel(&mut self, reg: &[CommandInfo], dir: isize) {
        let rows = self.rows(reg);
        if rows.is_empty() {
            self.sel = 0;
            return;
        }
        let mut i = self.sel as isize;
        loop {
            i += dir;
            if i < 0 || i as usize >= rows.len() {
                // Clamp: stay where we were if there is no further Item in this direction.
                if !matches!(rows.get(self.sel), Some(Row::Item(_))) {
                    // Initial position may sit on a header (fresh open): find the first Item.
                    if let Some(first) = rows.iter().position(|r| matches!(r, Row::Item(_))) {
                        self.sel = first;
                    }
                }
                return;
            }
            if matches!(rows[i as usize], Row::Item(_)) {
                self.sel = i as usize;
                return;
            }
        }
    }

    fn record_recent(&mut self, cmd: Cmd) {
        self.recents.retain(|c| *c != cmd);
        self.recents.insert(0, cmd);
        self.recents.truncate(MRU_CAP);
    }
```

Note: `open()` leaves `sel = 0`, which is a header row on the empty query; the first
`Up`/`Down`/re-clamp lands it on an Item. The `navigation_and_esc` test's first `Down` covers
exactly this.

- [ ] **Step 4: Run to verify PASS**

Run: `cargo test -p oracle-frontend palette::` — Expected: 5 PASS.
Also: `cargo test -p oracle-frontend --no-default-features palette::` — PASS (registry shrinks; nothing here assumed audio rows).

- [ ] **Step 5: Mutation checks**

1. In `rows`, drop the `!c.hidden` filter on the query branch: expect `typing_filters` or `empty_query_lists_everything_grouped` FAIL (hidden leak). Revert.
2. In `record_recent`, comment out `truncate(MRU_CAP)` AND `retain`: expect `enter_runs_and_records_mru` still passes → this shows the cap is NOT evidence-bearing in that test; add the missing assertion instead (run 4 distinct commands, reopen, assert at most `MRU_CAP` recent items) if not already failing. The point of this step: prove the MRU cap has a failing test. Record what you did.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -u crates/oracle-frontend
git commit -m "feat(frontend): palette state machine — filter, navigate, MRU, picker" \
  -m "mutation: removed hidden filter -> empty_query_lists_everything_grouped FAIL" \
  -m "mutation: <record the MRU-cap outcome from step 5.2>"
```

---

### Task 5: `palette.rs` — renderer

**Files:**
- Modify: `crates/oracle-frontend/src/palette.rs`

- [ ] **Step 1: Failing tests** (append inside `mod tests`)

```rust
    /// Rendering smoke: the palette paints its panel into the buffer (some pixels change) and
    /// stays inside the given area. Pixel-exactness is not asserted — layout is free to evolve;
    /// what must hold is "drew something, only inside the picture rect".
    #[test]
    fn draw_paints_inside_area_only() {
        let (mut p, reg) = open_palette();
        p.handle(PaletteKey::Down, &reg);
        let (w, h) = (320usize, 224usize);
        let mut buf = vec![0u32; w * h];
        let area = crate::present::Rect { x: 40, y: 20, w: 240, h: 180 };
        p.draw(&mut buf, w, h, area, &reg);
        let painted = buf.iter().filter(|px| **px != 0).count();
        assert!(painted > 0, "draw painted nothing");
        for (i, px) in buf.iter().enumerate() {
            if *px != 0 {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
    }

    /// Closed palette draws nothing.
    #[test]
    fn draw_noop_when_closed() {
        let p = Palette::new();
        let reg = registry();
        let mut buf = vec![0u32; 320 * 224];
        p.draw(&mut buf, 320, 224, crate::present::Rect { x: 0, y: 0, w: 320, h: 224 }, &reg);
        assert!(buf.iter().all(|px| *px == 0));
    }
```

- [ ] **Step 2: Run to verify FAIL** — `cargo test -p oracle-frontend palette::draw` → compile error (`draw` missing).

- [ ] **Step 3: Implement `draw`**

Add to `impl Palette` (imports at top of file: `use crate::font::{self, Canvas};` `use crate::overlay::{self, ACCENT, INFO};` `use crate::present::Rect;`):

```rust
    /// Paint the palette into the presentation buffer, inside the picture rect only (the same
    /// rule the overlay obeys — never the retained native framebuffer, spec §10). Scale follows
    /// the overlay's: `Overlay::font_scale`.
    pub fn draw(&self, buf: &mut [u32], w: usize, h: usize, area: Rect, reg: &[CommandInfo]) {
        if !self.open || area.w == 0 || area.h == 0 {
            return;
        }
        let px = overlay::Overlay::font_scale(h);
        let line_h = font::LINE_H * px;
        let margin = 4 * px;
        // Panel: inset from the picture rect, top-anchored, tall enough for the query line
        // plus what fits.
        let panel_x = area.x + area.w / 10;
        let panel_w = area.w - 2 * (area.w / 10);
        let panel_y = area.y + area.h / 12;
        let panel_h = (area.h - 2 * (area.h / 12)).min(area.h);
        let mut canvas = Canvas::new(buf, w, h);
        canvas.fill_rect(panel_x as i32, panel_y as i32, panel_w, panel_h, 0x000A1418, font::PANEL_ALPHA);

        let text_x = (panel_x + margin) as i32;
        let mut y = (panel_y + margin) as i32;

        if let Some(pk) = &self.picker {
            canvas.text(text_x, y, px, ACCENT, &pk.title);
            y += (line_h + margin / 2) as i32;
            for (i, (label, _)) in pk.items.iter().enumerate() {
                if (y as usize + line_h) > panel_y + panel_h {
                    break;
                }
                if i == pk.sel {
                    canvas.fill_rect(text_x - 2, y - 1, panel_w - 2 * margin, line_h, 0x00123A46, 255);
                }
                canvas.text(text_x, y, px, INFO, label);
                y += line_h as i32;
            }
            return;
        }

        // Query line: "> query_" (static underscore cursor; append-only editing needs no more).
        let q = format!("> {}_", self.query);
        canvas.text(text_x, y, px, ACCENT, &q);
        y += (line_h + margin / 2) as i32;

        for (ri, row) in self.rows(reg).iter().enumerate() {
            if (y as usize + line_h) > panel_y + panel_h {
                break; // capped rows; scrolling arrives with a taller list than fits (none yet)
            }
            match row {
                Row::Header(hdr) => {
                    canvas.text(text_x, y, px, ACCENT, hdr);
                }
                Row::Item(i) => {
                    let c = &reg[*i];
                    if ri == self.sel {
                        canvas.fill_rect(text_x - 2, y - 1, panel_w - 2 * margin, line_h, 0x00123A46, 255);
                    }
                    canvas.text(text_x + (2 * font::ADVANCE * px) as i32, y, px, INFO, c.title);
                    if let Some(k) = c.hotkey {
                        let name = commands::key_name(k);
                        let kw = font::text_width(name) * px;
                        let kx = (panel_x + panel_w - margin).saturating_sub(kw) as i32;
                        canvas.text(kx, y, px, 0x007AA0BB, name);
                    }
                }
            }
            y += line_h as i32;
        }
    }
```

(If `font::Canvas`'s `fill_rect`/`text` clip at buffer edges but not at an arbitrary rect — check `font.rs:154` — the top-anchored inset layout above never exceeds `area` anyway; the test asserts it.)

- [ ] **Step 4: Run to verify PASS** — `cargo test -p oracle-frontend palette::` → all PASS.

- [ ] **Step 5: Mutation check** — set `panel_x = 0` (ignoring `area.x`): expect `draw_paints_inside_area_only` FAIL (paints left of the area). Revert; record.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -u crates/oracle-frontend
git commit -m "feat(frontend): palette renderer on the overlay's font machinery" \
  -m "mutation: panel_x=0 -> draw_paints_inside_area_only FAIL"
```

---

### Task 6: `main.rs` — bindings dispatch replaces the if-chain; palette wired; Esc/Tab/quit semantics

This is the integration task: behavior-preserving refactor of the hotkey chain into ONE
`match cmd`, plus the palette. Existing handler BODIES move verbatim — do not rewrite their
logic; every body carries load-bearing comments and ordering (SRAM flush before reset, etc.).

**Files:**
- Modify: `crates/oracle-frontend/src/main.rs` (hotkey chain at ~907–1237; docs at 26–45)

- [ ] **Step 1: Add setup state before the `while` loop** (near `let mut state_slot` at main.rs:897)

```rust
    // The command registry + palette (spec §4). The registry is the single source of truth for
    // actions; dispatch happens in ONE `match cmd` below so the actions keep borrowing the
    // loop's state directly.
    let reg = commands::registry();
    let mut palette = palette::Palette::new();
    let mut running = true;
    ov.push("PRESS ` FOR COMMANDS", overlay::INFO); // discoverability layer 1 (spec §4)
```

- [ ] **Step 2: Change the loop condition** (main.rs:907)

From: `while window.is_open() && !window.is_key_down(Key::Escape) {`
To: `while window.is_open() && running {`
(Esc no longer quits — spec §3. Quit = close button or the Quit command.)

- [ ] **Step 3: Replace the edge-triggered key blocks with input routing + one dispatch match**

Delete the `if window.is_key_pressed(...)` blocks between the `let (win_w, win_h) = ...`/`let view = ...` lines and the `// Inputs are sampled live every frame` comment (main.rs:913–1237) — EXCEPT the mouse-click block (main.rs:924–967), which stays as-is (mouse is not a command). In their place:

```rust
        // --- Input routing (spec §3): palette open -> it eats every key; else bindings. ---
        let mut pending: Vec<commands::Cmd> = Vec::new();
        let mut step = false;
        if palette.is_open() {
            for k in window.get_keys_pressed(KeyRepeat::Yes) {
                let pk = match k {
                    Key::Backspace => Some(palette::PaletteKey::Backspace),
                    Key::Up => Some(palette::PaletteKey::Up),
                    Key::Down => Some(palette::PaletteKey::Down),
                    Key::Enter => Some(palette::PaletteKey::Enter),
                    Key::Escape => Some(palette::PaletteKey::Esc),
                    _ => commands::key_char(k).map(palette::PaletteKey::Char),
                };
                if let Some(pk) = pk {
                    if let palette::PaletteAction::Run(cmd) = palette.handle(pk, &reg) {
                        pending.push(cmd);
                    }
                }
            }
        } else {
            let ctrl = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
            if window.is_key_pressed(Key::Backquote, KeyRepeat::No)
                || (ctrl && window.is_key_pressed(Key::P, KeyRepeat::No))
            {
                palette.open();
            } else {
                for c in reg.iter().filter(|c| c.hotkey.is_some()) {
                    let rep = if c.repeat { KeyRepeat::Yes } else { KeyRepeat::No };
                    if window.is_key_pressed(c.hotkey.unwrap(), rep) {
                        pending.push(c.cmd);
                    }
                }
            }
        }

        // --- Dispatch: the one match. Bodies moved verbatim from the old if-chain. ---
        for cmd in pending {
            match cmd {
                commands::Cmd::Pause => {
                    // (old Space body, main.rs:915-917)
                    paused = !paused;
                    println!("{}", if paused { "paused" } else { "resumed" });
                }
                commands::Cmd::Step => {
                    // DWIM (spec §4): stepping while unpaused pauses AND steps.
                    paused = true;
                    step = true;
                }
                commands::Cmd::ToggleStatusLine => ov.status_line = !ov.status_line,
                commands::Cmd::DumpHits => {
                    // move the W body here verbatim (old main.rs:970-983)
                }
                commands::Cmd::ClearWatch => {
                    // move the C body here verbatim (old main.rs:984-991)
                }
                commands::Cmd::SlotPrev | commands::Cmd::SlotNext | commands::Cmd::SlotSelect(_) => {
                    // Slot handling, folded from the three old blocks (main.rs:995-1027).
                    match cmd {
                        commands::Cmd::SlotPrev => state_slot = next_slot(state_slot, -1),
                        commands::Cmd::SlotNext => state_slot = next_slot(state_slot, 1),
                        commands::Cmd::SlotSelect(n) => state_slot = n,
                        _ => unreachable!(),
                    }
                    // then the old `if slot_changed { ... }` body verbatim (probe_slots, flash,
                    // notify — main.rs:1010-1027), unconditionally: reaching here IS the change.
                }
                commands::Cmd::SlotPicker => {
                    // Items carry occupancy, exactly what the slot toast says today.
                    let items = (0..save_state::SLOT_COUNT)
                        .map(|n| {
                            let occ = if slots_on_disk[n] { "occupied" } else { "empty" };
                            (format!("slot {n} ({occ})"), commands::Cmd::SlotSelect(n))
                        })
                        .collect();
                    palette.open_picker("SELECT SLOT".into(), items);
                }
                commands::Cmd::SaveState => {
                    // move the F2 body here verbatim (old main.rs:1031-1048)
                }
                commands::Cmd::LoadState => {
                    // move the F4 body here verbatim (old main.rs:1050-1110)
                }
                commands::Cmd::Reset => {
                    // move the F1 body here verbatim (old main.rs:1113-1132)
                }
                commands::Cmd::ReloadRom => {
                    // move the F5 body here verbatim (old main.rs:1133-1210)
                }
                commands::Cmd::Quit => running = false,
                #[cfg(feature = "audio")]
                commands::Cmd::VolumeUp | commands::Cmd::VolumeDown | commands::Cmd::MuteToggle => {
                    // Fold the old volume block (main.rs:1214-1237): mutate per the variant,
                    // then the shared `changed` notify body verbatim.
                    match cmd {
                        commands::Cmd::VolumeUp => volume = (volume + 1).min(audio::VOLUME_STEPS),
                        commands::Cmd::VolumeDown => volume = volume.saturating_sub(1),
                        commands::Cmd::MuteToggle => muted = !muted,
                        _ => unreachable!(),
                    }
                    let line = if muted {
                        format!("volume: {volume}/{}  [MUTED]", audio::VOLUME_STEPS)
                    } else {
                        format!("volume: {volume}/{}", audio::VOLUME_STEPS)
                    };
                    notify(&mut ov, INFO, line);
                }
                #[cfg(not(feature = "audio"))]
                commands::Cmd::VolumeUp | commands::Cmd::VolumeDown | commands::Cmd::MuteToggle => {}
            }
        }
```

Where a comment says "move the X body here verbatim (old main.rs:A-B)", cut EXACTLY those
lines from the deleted region and paste them unchanged (they compile in place — same scope,
same locals). The old `let step = window.is_key_pressed(Key::Period, ...)` line is deleted;
`step` is now the `let mut step` above and its consumer (`if paused && step`-style logic near
main.rs:1277) is unchanged — verify the consumer still reads a plain `bool` named `step`.

- [ ] **Step 4: Swallow game input while the palette is open** (the `let mut player = ...` line, main.rs:1245)

From: `let mut player = [poll_pad(&window), Pad::default()];`
To:

```rust
        // Palette open = keys are text, not gameplay (spec §3). Gamepads always reach the game.
        let mut player = [
            if palette.is_open() { Pad::default() } else { poll_pad(&window) },
            Pad::default(),
        ];
```

- [ ] **Step 5: Draw the palette** — find the overlay draw call in the present path (`ov.draw(...)` near main.rs:1477-1481, drawn into the presentation buffer after `scale_into`). IMMEDIATELY BEFORE `ov.draw(...)`, add:

```rust
        palette.draw(&mut winbuf, win_w, win_h, view, &reg);
```

(`winbuf` = whatever the presentation buffer local is called at that site — the same `&mut` slice `ov.draw` receives; toasts stay on top of the palette by draw order.)

- [ ] **Step 6: Update the module doc table** (main.rs:26-45): change the Esc row to `| Esc | close the palette / picker (quit = window close button or the Quit command) |`, add rows `` | ` (backtick) or Ctrl+P | open the command palette | `` and `| Tab | soft reset (F1 alias kept) |`, and add one sentence under Controls: "Every action lives in the command registry (`commands.rs`); the palette lists them grouped, with hotkeys shown — the list is the cheat-sheet."

- [ ] **Step 7: Build + full frontend test suite, both variants**

Run: `cargo test -p oracle-frontend` then `cargo test -p oracle-frontend --no-default-features`
Expected: ALL PASS (pre-existing tests prove the refactor preserved `next_slot`, arg parsing, overlay behavior). Fix any borrow issues by keeping the moved bodies inside the single match (they may not be split into helper fns — the borrows are the reason the match lives in the loop).

- [ ] **Step 8: Manual smoke (the one non-automatable step)**

Run: `cargo run --release -p oracle-frontend -- <any .bin you use, e.g. ../aeon/s4.bin>`
Verify, in order: startup toast shows; backtick opens the grouped list; typing filters; Enter on "Pause / resume" pauses (PAUSED banner); backtick → "select save slot" → Enter opens picker, arrows + Enter selects (toast confirms); Space/W/C/F2/F4/F6/F7/0-9/F3/F5/-/=/M all still work; Tab AND F1 reset; Esc closes palette, does NOT quit; arrows/A/S/D do nothing to the game while palette is open; Quit command exits.

- [ ] **Step 9: Commit**

```bash
cargo fmt && git add -u crates/oracle-frontend
git commit -m "feat(frontend): command palette + data-driven dispatch replace the hotkey chain" \
  -m "Esc closes UI instead of quitting; Tab = reset alias; game input swallowed while palette open"
```

---

### Task 7: Final gates

- [ ] **Step 1: Workspace gates** (single tree, never two at once)

Run, each to completion, no piping:
```bash
cargo fmt --check
cargo clippy -p oracle-frontend --all-targets -- -D warnings
cargo clippy -p oracle-frontend --all-targets --no-default-features -- -D warnings
cargo test --workspace
```
Expected: fmt clean, clippy 0 warnings both variants, all tests pass, and `git status` shows `crates/oracle-core/` untouched (the zero-currency gate is structural — nothing in this plan touches core).

- [ ] **Step 2: Confirm zero core diff explicitly**

Run: `git diff --stat origin/m68000-microop-framework -- crates/oracle-core/`
Expected: empty output. If not empty, STOP — something moved that this slice must not move.

- [ ] **Step 3: Commit any gate fixups**

```bash
git add -u && git commit -m "chore(frontend): S1 gate fixups (fmt/clippy)"
```
(Skip if nothing changed.)

---

## Self-review (done at plan-writing time)

- **Spec coverage (§3–§4 = S1's scope):** backtick+Ctrl+P open (Task 6.3), Esc semantics (6.2/6.3), Tab reset (Task 1 registry), palette-eats-keys + game-runs-behind + input swallow (6.3/6.4), registry as single source (1), grouped empty-state + filter + hotkey column (4, 5), MRU (4), picker + no free text (4, 6.3), DWIM step (6.3), absent-not-greyed audio commands (1, 6.3), discoverability hint (6.1), rendering on existing font/scale rules (5). Deferred to later slices per spec: lenses (S3), views (S4), config/rebinding (S2/S5), status-line hint *line* (S2 owns status-line content changes; S1 uses a startup toast).
- **Placeholders:** the only "move verbatim" references carry exact source line ranges — deliberate (rewriting 60-line battle-tested bodies in a plan invites transcription bugs; the instruction is mechanical).
- **Type consistency:** `Cmd`/`CommandInfo`/`registry()`/`subseq_match`/`key_name`/`key_char` (Task 1–3) are exactly what Tasks 4–6 import; `PaletteKey`/`PaletteAction`/`Row`/`draw(buf,w,h,area,reg)` (Task 4–5) match Task 6's call sites; `Rect{x,y,w,h}` verified against present.rs:65.
