//! The in-window ROM browser's model — what the "Open ROM…" picker shows, and nothing about how it is
//! drawn. Pure apart from one `read_dir`, so the ordering, the filtering and the labels are unit-testable
//! headless; the palette renders the result through the same [`Picker`](crate::palette::Picker) the save-slot
//! list already uses.
//!
//! **Why this exists at all.** The player took its ROM from `argv` and nowhere else: F5 re-read *the same*
//! path, and the only way to open a different game was to quit and relaunch from a terminal. A client on the
//! bus could always swap the cartridge (`emulator/reload_rom` takes a `path`); the person sitting in front of
//! the window could not.
//!
//! **Deliberately not a native file dialog.** `rfd` and friends would pull a GTK/portal dependency tree into
//! a frontend whose dependency list is kept deliberately small, would need a portal present at runtime, and —
//! the reason that decided it — a native dialog is untestable by construction, while everything below is
//! covered headless. The palette already owns typed filtering, a selection model and a pick list, so the
//! cheaper thing is also the better-behaved one.

use std::path::{Path, PathBuf};

/// What one row of the picker is. The three cases behave differently on Enter — two navigate, one loads a
/// cartridge — so the picker must never have to infer it back from the label.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntryKind {
    /// The containing directory. Offered only when one exists, and always first.
    Parent,
    /// A subdirectory: descend into it.
    Dir,
    /// A ROM image: load it.
    Rom,
}

/// One row: what to show, what it is, and where it points.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub label: String,
    pub kind: EntryKind,
    pub path: PathBuf,
}

/// The image extensions offered, lowercase. Genesis/Mega Drive only — this player emulates one machine, and
/// listing a `.sfc` we would then fail to run is worse than not listing it.
pub const ROM_EXTS: [&str; 4] = ["bin", "md", "gen", "smd"];

/// Whether `path` names an image this player will offer. Extension-only and case-insensitive: the header is
/// not consulted, because a directory listing that stats and reads every file would stall on a large folder,
/// and the load path validates the bytes anyway.
pub fn is_rom(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| ROM_EXTS.contains(&e.as_str()))
}

/// The rows for `dir`: the parent (if any) first, then subdirectories, then ROM images, each group sorted
/// case-insensitively by name.
///
/// Only the `read_dir` itself can fail — a single unreadable entry is **skipped**, never fatal, because one
/// bad symlink in a folder must not cost the user the whole listing. Dotfiles are skipped: they are noise in
/// a game folder, and a hidden ROM stays reachable through `--` on the command line.
pub fn scan(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut dirs: Vec<Entry> = Vec::new();
    let mut roms: Vec<Entry> = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        // `file_type` follows no symlink; `is_dir` on the path does. A symlinked game folder should read as a
        // folder, so ask the path.
        if path.is_dir() {
            dirs.push(Entry {
                label: format!("{name}/"),
                kind: EntryKind::Dir,
                path,
            });
        } else if is_rom(&path) {
            roms.push(Entry {
                label: name.to_string(),
                kind: EntryKind::Rom,
                path,
            });
        }
    }

    let by_name = |a: &Entry, b: &Entry| a.label.to_lowercase().cmp(&b.label.to_lowercase());
    dirs.sort_by(by_name);
    roms.sort_by(by_name);

    let mut out = Vec::with_capacity(dirs.len() + roms.len() + 1);
    if let Some(parent) = dir.parent() {
        out.push(Entry {
            label: "../".to_string(),
            kind: EntryKind::Parent,
            path: parent.to_path_buf(),
        });
    }
    out.append(&mut dirs);
    out.append(&mut roms);
    Ok(out)
}

/// The string the picker shows for `entry`, marking the image that is loaded right now.
///
/// The marker is the whole reason this is a function rather than `entry.label`: the picker lists the folder
/// the running game came from, so the running game is almost always on screen, and a list where it looks like
/// every other row invites re-loading it — which resets the machine and drops the frame. Comparison is by
/// path, so a same-named ROM in another folder is correctly *not* marked.
pub fn picker_label(entry: &Entry, current: Option<&Path>) -> String {
    match current {
        Some(cur) if entry.kind == EntryKind::Rom && cur == entry.path => {
            format!("{}   [loaded]", entry.label)
        }
        _ => entry.label.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that removes itself, so the tests below leave nothing behind and can run in
    /// parallel with each other.
    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Tmp {
            let dir = std::env::temp_dir().join(format!(
                "oracle-rom-browser-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Tmp(dir)
        }
        fn file(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            std::fs::write(&p, [0u8; 4]).unwrap();
            p
        }
        fn dir(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            std::fs::create_dir_all(&p).unwrap();
            p
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rom_extensions_are_case_insensitive_and_bounded() {
        assert!(is_rom(Path::new("s4.bin")));
        assert!(is_rom(Path::new("Sonic.MD")));
        assert!(is_rom(Path::new("game.Gen")));
        assert!(is_rom(Path::new("game.smd")));
        // Not this machine's images, and not offered.
        assert!(!is_rom(Path::new("game.sfc")));
        assert!(!is_rom(Path::new("game.nes")));
        // Files that sit next to every ROM in this workspace and must never be offered as one.
        assert!(!is_rom(Path::new("s4.lst")));
        assert!(!is_rom(Path::new("s4.srm")));
        assert!(!is_rom(Path::new("s4.state0")));
        // No extension at all.
        assert!(!is_rom(Path::new("README")));
    }

    #[test]
    fn scan_lists_parent_then_dirs_then_roms_each_sorted() {
        let t = Tmp::new("order");
        // Created out of order on purpose: the assertion below is about `scan`'s sort, and a fixture that
        // happens to be created in the answer's order would pass with the sort deleted.
        t.file("zelda.md");
        t.file("alpha.bin");
        let _ = t.dir("zed");
        let _ = t.dir("Acts");
        t.file("Beta.GEN");
        // Present in the directory and not a ROM — the filter's subject.
        t.file("alpha.lst");
        t.file(".hidden.bin");

        let got = scan(&t.0).unwrap();
        let labels: Vec<&str> = got.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["../", "Acts/", "zed/", "alpha.bin", "Beta.GEN", "zelda.md"],
            "parent first, then dirs, then roms; each group case-insensitively sorted"
        );
        let kinds: Vec<EntryKind> = got.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                EntryKind::Parent,
                EntryKind::Dir,
                EntryKind::Dir,
                EntryKind::Rom,
                EntryKind::Rom,
                EntryKind::Rom
            ]
        );
        // The parent row points at the parent, not at the directory itself — the one thing that would make
        // `../` a no-op that looks like it worked.
        assert_eq!(got[0].path, t.0.parent().unwrap());
    }

    #[test]
    fn scan_reports_the_directory_error_rather_than_an_empty_listing() {
        // An absent directory must NOT come back as "no ROMs here": an empty list and a failed read are the
        // same picture on screen, and only one of them is the user's folder being empty.
        let missing = std::env::temp_dir().join("oracle-rom-browser-does-not-exist-9f3a1c");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(scan(&missing).is_err());
    }

    #[test]
    fn picker_label_marks_only_the_loaded_image_by_path() {
        let t = Tmp::new("label");
        let loaded = t.file("s4.bin");
        let other = t.file("s4other.bin");
        let entries = scan(&t.0).unwrap();
        let rom = |name: &str| {
            entries
                .iter()
                .find(|e| e.label == name)
                .unwrap_or_else(|| panic!("{name} missing from the listing"))
        };

        assert_eq!(
            picker_label(rom("s4.bin"), Some(&loaded)),
            "s4.bin   [loaded]"
        );
        assert_eq!(
            picker_label(rom("s4other.bin"), Some(&loaded)),
            "s4other.bin"
        );
        // A same-named ROM in a different folder is a different cartridge and must not be marked.
        let elsewhere = PathBuf::from("/somewhere/else/s4.bin");
        assert_eq!(picker_label(rom("s4.bin"), Some(&elsewhere)), "s4.bin");
        // Directory rows never carry the marker, whatever the current path is.
        let dirs: Vec<String> = entries
            .iter()
            .filter(|e| e.kind != EntryKind::Rom)
            .map(|e| picker_label(e, Some(&loaded)))
            .collect();
        assert!(
            dirs.iter().all(|l| !l.contains("[loaded]")),
            "a navigation row was marked as the loaded cartridge: {dirs:?}"
        );
        let _ = other;
    }
}
