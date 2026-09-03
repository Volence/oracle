//! The player's file layer for [`oracle_core::symbols`] — find the listing for the ROM, read it, and
//! decide whether it may be trusted against the image actually loaded.
//!
//! **Ported from `crates/oracle-frontend/src/symbol_file.rs`, which is not edited** (the standing
//! instruction for the whole parcel-2 arc). The policy below is that module's, unchanged; what is new is
//! [`Source`] — the player takes a `--symbols PATH`, which the minifb player does not, and the two
//! behaviours must be *told apart* rather than merged. See "Why an explicit path refuses louder" below.
//!
//! The split mirrors the frontend's: the core owns the pure, deterministic half (parsing, lookup, and the
//! ROM-binding check, all `&str`/`&[u8]` in) and this module owns the filesystem and the reporting.
//! `oracle-core`'s charter is no-I/O, so a path is never handed to it.
//!
//! # Policy
//!
//! Symbols are **opt-in by presence** and never load-bearing: the emulator runs identically without them.
//!
//! | on disk | what happens |
//! |---|---|
//! | absent | one informational line, exactly like a first-launch `.srm`; not an error |
//! | unparseable | warning, run without symbols |
//! | [`RomBinding::Mismatch`] | **refused** — warning naming the fault, run without symbols |
//! | [`RomBinding::Indeterminate`] **and** not [intact](SymbolTable::is_intact) | **refused** |
//! | [`RomBinding::Indeterminate`] and intact | accepted, with a warning that it is unverified |
//! | [`RomBinding::Match`] | accepted (with a warning if the file is not intact) |
//!
//! The refusal is the point. A listing from a different build shape is not *degraded* information — of the
//! symbols `s4.lst` and `s4.debug.lst` share, 92.6% name a different address, so every annotation would be
//! confidently wrong. The suite contract's decision D7 records exactly this failure: a session in which
//! every symbol shifted by `+$24` and a "verified" hardcoded literal rotted while it was being used.
//!
//! # Why `Indeterminate` alone is not enough to accept
//!
//! `Indeterminate` means "this listing declares no `EndOfRom`, so there is nothing to probe" — the honest
//! verdict for a hand-written or non-Aeon listing, where refusing would be a false negative. But the two
//! verdicts are not independent: **any listing that would be a `Mismatch` becomes `Indeterminate` if its
//! `EndOfRom` row goes missing**, and a truncated file loses rows from the end, where `EndOfRom` sits.
//! Verified in the frontend's own tests: deleting that one row from the real `s4.debug.lst` turns a correct
//! refusal against `s4.bin` into `Indeterminate` — and then 1,775 of 1,811 shared symbols name the wrong
//! address. So the fail-open path is closed by requiring an `Indeterminate` listing to at least be
//! *internally* whole.
//!
//! # Why an explicit path refuses louder than a discovered one
//!
//! [`Source::Discovered`] is a guess the player made — *"there might be an `s4.lst` beside `s4.bin`"* — and
//! a guess that finds nothing is not a fault; the frontend prints one line and runs on. [`Source::Named`]
//! is the user's own instruction, and a `--symbols` that silently resolves to no table would leave a human
//! looking at a Memory panel with a working address box, wondering why every name is refused. **A named
//! path that does not exist is therefore fatal**, exactly as an unreadable `--rom` is: the difference is
//! not the filesystem's, it is whether anybody asked.
//!
//! Note what is *not* different: a named listing that is present but **mismatched is still refused, not
//! forced**. `--symbols` says which file to read, never that the file may be trusted; overriding the D7
//! binding check on request would put the one guard this module exists for behind a flag.

use oracle_core::symbols::{RomBinding, SymbolTable, TableSource};
use std::path::{Path, PathBuf};

/// Where the listing path came from, which is the only thing that separates the two policies above.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// `<rom>.lst`, guessed by [`lst_path_for`]. Absence is normal.
    Discovered,
    /// `--symbols PATH`. Absence is the user's mistake and is reported as one.
    Named,
}

/// What a load produced: the table, the path it was read from, and whether the caller should stop.
pub struct Loaded {
    pub table: Option<SymbolTable>,
    /// The path a table was actually read from. `None` whenever [`table`](Self::table) is `None` — the
    /// bus must never be handed a `symbolsPath` naming a file it is not resolving against (D7), which is
    /// what `oracle-frontend`'s `bus_machine_info` spells out as `symbols.is_some().then(...)`.
    pub path: Option<PathBuf>,
    /// A named path that did not exist. The caller exits; this module does not call `std::process::exit`
    /// so that it stays testable.
    pub fatal: Option<String>,
}

/// The `.lst` listing that sits next to the ROM: same directory + stem, `.lst` extension
/// (`.../s4.bin` → `.../s4.lst`). This is exactly where `sigil build --emit-lst` writes it, so a normal
/// Aeon build needs no configuration at all.
pub fn lst_path_for(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("lst")
}

/// Resolve the listing for this launch: `--symbols` if given, else the `.lst` beside the ROM.
///
/// One function rather than two call sites, so the *rule* — an explicit path wins, discovery is the
/// fallback — is a thing a test can read rather than a thing `main` happens to do.
pub fn resolve(rom_path: &str, symbols_arg: Option<&str>) -> (PathBuf, Source) {
    match symbols_arg {
        Some(p) => (PathBuf::from(p), Source::Named),
        None => (lst_path_for(Path::new(rom_path)), Source::Discovered),
    }
}

/// Load the symbol table at `path`, validate it against the `rom` bytes about to be run, and return it
/// only if it may be trusted. Prints a one-line account of whichever outcome occurred.
///
/// `rom` must be the image actually loaded — that is what the binding check probes.
pub fn load(path: &Path, source: Source, rom: &[u8]) -> Loaded {
    let none = |fatal: Option<String>| Loaded {
        table: None,
        path: None,
        fatal,
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return match source {
            Source::Discovered => {
                println!(
                    "symbols: none at {} (build with `sigil build --emit-lst` to name addresses)",
                    path.display()
                );
                none(None)
            }
            // Asked for by name and not there. See the module doc: the difference is not the
            // filesystem's, it is whether anybody asked.
            Source::Named => none(Some(format!(
                "cannot read the listing {} named by --symbols",
                path.display()
            ))),
        };
    };

    let table = match SymbolTable::parse(&text) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "symbols: {} is not a usable listing ({e}) — running with raw addresses",
                path.display()
            );
            return none(None);
        }
    };

    match table.validate_against_rom(rom) {
        RomBinding::Mismatch(fault) => {
            // Refuse rather than tolerate: see the module doc. `--symbols` does NOT override this.
            eprintln!(
                "symbols: REFUSED {} — it does not describe this ROM ({fault:?}). Rebuild, or point at \
                 the matching listing; running with raw addresses.",
                path.display()
            );
            return none(None);
        }
        RomBinding::Indeterminate(why) if !table.is_intact() => {
            // "No fingerprint" plus "damaged file" most likely means the fingerprint symbol fell off a
            // truncated end — so this may be a mismatch wearing a disguise. Refuse; see the module doc.
            eprintln!(
                "symbols: REFUSED {} — no build fingerprint ({why:?}) AND the file is not intact \
                 ({}); it may be a truncated listing for a different ROM. Running with raw addresses.",
                path.display(),
                integrity_note(&table)
            );
            return none(None);
        }
        RomBinding::Indeterminate(why) => {
            eprintln!(
                "symbols: {} carries no build fingerprint ({why:?}) — loading it unverified",
                path.display()
            );
        }
        RomBinding::Match {
            appendix_offset,
            appendix_len,
        } => {
            println!(
                "symbols: {} loaded from {} (matches this ROM — deb2 appendix at ${appendix_offset:06X}, \
                 {appendix_len} bytes)",
                table.len(),
                path.display()
            );
        }
    }

    // A listing that bound to the ROM but is not whole still resolves *correctly* — its symbols are real,
    // there are just fewer of them, so a PC lands on a coarser name rather than a wrong one. Say so out
    // loud (the coarser name looks perfectly healthy) but do not refuse.
    if !table.is_intact() {
        eprintln!(
            "symbols: warning — {} does not look intact ({}); addresses will resolve to coarser names",
            path.display(),
            integrity_note(&table)
        );
    }
    Loaded {
        table: Some(table),
        path: Some(path.to_path_buf()),
        fatal: None,
    }
}

/// A short human account of *how* a listing failed [`SymbolTable::is_intact`], for the warning text.
fn integrity_note(table: &SymbolTable) -> String {
    let mut why = Vec::new();
    if table.source() != TableSource::SymbolTable {
        why.push("no `Symbol Table` section — fell back to the body lines".to_string());
    }
    match table.matches_declared_count() {
        None => why.push("no `N symbols` footer".to_string()),
        Some(false) => why.push(format!(
            "parsed {} but the footer declares {:?}",
            table.len(),
            table.declared_count()
        )),
        Some(true) => {}
    }
    if table.skipped_lines() > 0 {
        why.push(format!("{} unrecognised rows", table.skipped_lines()));
    }
    why.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lst_path_sits_beside_the_rom() {
        assert_eq!(
            lst_path_for(Path::new("/games/s4.bin")),
            PathBuf::from("/games/s4.lst")
        );
        // A ROM path with no extension simply gains `.lst`.
        assert_eq!(
            lst_path_for(Path::new("/games/s4")),
            PathBuf::from("/games/s4.lst")
        );
        // Only the final extension is replaced, so `s4.debug.bin` pairs with `s4.debug.lst`.
        assert_eq!(
            lst_path_for(Path::new("/games/s4.debug.bin")),
            PathBuf::from("/games/s4.debug.lst")
        );
    }

    /// The resolution rule itself: `--symbols` wins outright, discovery is the fallback, and the
    /// *source* travels with the path so the two absences stay tellable apart downstream.
    #[test]
    fn an_explicit_symbols_argument_wins_and_is_marked_as_asked_for() {
        assert_eq!(
            resolve("/games/s4.bin", None),
            (PathBuf::from("/games/s4.lst"), Source::Discovered)
        );
        assert_eq!(
            resolve("/games/s4.bin", Some("/elsewhere/other.lst")),
            (PathBuf::from("/elsewhere/other.lst"), Source::Named)
        );
    }

    /// A scratch directory unique to this test binary run.
    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oracle-player-sym-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal ROM image carrying a `deb2` appendix at `end`, big enough to clear the size floor.
    fn rom_with_appendix(end: usize) -> Vec<u8> {
        let mut rom = vec![0u8; end + 0x4000];
        rom[end] = 0xDE;
        rom[end + 1] = 0xB2;
        rom
    }

    /// A minimal listing declaring `EndOfRom` at `end` plus one ordinary symbol.
    fn listing_for(end: u32) -> String {
        format!("  Symbol Table (* = unused):\n\n Main : 300 C |\n EndOfRom : {end:X} C |\n\n   2 symbols\n")
    }

    #[test]
    fn a_missing_discovered_listing_is_normal_and_yields_no_table() {
        let p = scratch("missing").join("nothing-here.lst");
        let out = load(&p, Source::Discovered, &[0u8; 0x200]);
        assert!(out.table.is_none());
        assert!(out.path.is_none());
        assert!(out.fatal.is_none(), "a guess that missed is not a fault");
    }

    /// …but the same absence at a path the *user named* is fatal. Told apart by [`Source`] alone, which
    /// is the whole reason that enum exists — the filesystem cannot tell the two cases apart.
    #[test]
    fn a_missing_named_listing_is_fatal() {
        let p = scratch("missing-named").join("nothing-here.lst");
        let out = load(&p, Source::Named, &[0u8; 0x200]);
        assert!(out.table.is_none());
        let fatal = out.fatal.expect("--symbols naming nothing must be fatal");
        assert!(
            fatal.contains("--symbols") && fatal.contains("nothing-here.lst"),
            "the message must name the flag and the path: {fatal:?}"
        );
    }

    /// The load-bearing path, end to end through the filesystem: a listing whose `EndOfRom` does not find
    /// the appendix in the image being run is **refused**, so the caller gets no table at all rather than
    /// one that names the wrong addresses.
    #[test]
    fn a_listing_from_a_different_build_is_refused() {
        let dir = scratch("refuse");
        let p = dir.join("game.lst");
        // The image has its appendix at $8000; the listing insists on $9000.
        std::fs::write(&p, listing_for(0x9000)).unwrap();
        assert!(
            load(&p, Source::Discovered, &rom_with_appendix(0x8000))
                .table
                .is_none(),
            "a mismatched listing must be refused, not loaded"
        );
    }

    /// **And `--symbols` does not override the binding check.** A named path says which file to read; it
    /// does not say the file may be trusted. Asserted separately from the discovered case because it is
    /// the one place where "the user asked for it" would be a plausible reason to relax D7, and relaxing
    /// it is exactly what must not happen.
    #[test]
    fn a_named_listing_from_a_different_build_is_still_refused() {
        let dir = scratch("refuse-named");
        let p = dir.join("game.lst");
        std::fs::write(&p, listing_for(0x9000)).unwrap();
        let out = load(&p, Source::Named, &rom_with_appendix(0x8000));
        assert!(out.table.is_none(), "--symbols must not force a mismatch");
        assert!(
            out.fatal.is_none(),
            "a present-but-refused listing runs on with raw addresses, exactly as a discovered one does"
        );
    }

    /// The fail-open path, closed: a listing that would be refused as a `Mismatch` loses that verdict the
    /// moment its `EndOfRom` row goes missing (which is what truncation does — it cuts from the end). The
    /// combination "unverifiable AND visibly damaged" must still be refused.
    #[test]
    fn an_unverifiable_and_damaged_listing_is_refused() {
        let dir = scratch("indet-damaged");
        let p = dir.join("game.lst");
        // No `EndOfRom` (so: Indeterminate) and a footer that disagrees (so: not intact).
        std::fs::write(
            &p,
            "  Symbol Table (* = unused):\n\n Main : 300 C |\n\n   9 symbols\n",
        )
        .unwrap();
        assert!(
            load(&p, Source::Discovered, &rom_with_appendix(0x8000))
                .table
                .is_none(),
            "an unverifiable listing that is also damaged must be refused"
        );
    }

    /// …but an unverifiable listing that is otherwise whole is a legitimate hand-written or non-Aeon file,
    /// and refusing it would be a false negative. It loads, with a warning.
    #[test]
    fn an_unverifiable_but_intact_listing_still_loads() {
        let dir = scratch("indet-ok");
        let p = dir.join("game.lst");
        std::fs::write(
            &p,
            "  Symbol Table (* = unused):\n\n Main : 300 C |\n\n   1 symbols\n",
        )
        .unwrap();
        let out = load(&p, Source::Discovered, &rom_with_appendix(0x8000));
        assert_eq!(
            out.table.expect("must load").address_of("Main"),
            Some(0x300)
        );
        assert_eq!(out.path.as_deref(), Some(p.as_path()));
    }

    /// …and the matching listing for the same image loads and resolves.
    #[test]
    fn a_matching_listing_loads_and_resolves() {
        let dir = scratch("accept");
        let p = dir.join("game.lst");
        std::fs::write(&p, listing_for(0x8000)).unwrap();
        let t = load(&p, Source::Discovered, &rom_with_appendix(0x8000))
            .table
            .expect("must load");
        assert_eq!(t.address_of("Main"), Some(0x300));
        assert_eq!(t.resolve(0x304).unwrap().to_string(), "Main+$4");
    }
}
