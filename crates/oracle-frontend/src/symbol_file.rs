//! The frontend's file layer for [`oracle_core::symbols`] — find the `<rom>.lst` beside the ROM, read it,
//! and decide whether it may be trusted against the image actually loaded.
//!
//! The split mirrors [`crate::sram_file`]: the core owns the pure, deterministic half (parsing, lookup,
//! and the ROM-binding check, all `&str`/`&[u8]` in) and this module owns the filesystem and the reporting.
//! `oracle-core`'s charter is no-I/O, so a path is never handed to it.
//!
//! # Policy
//!
//! Symbols are **opt-in by presence** and never load-bearing: the emulator runs identically without them.
//! Four outcomes, three of which end with the machine running unsymbolised:
//!
//! | on disk | verdict | what happens |
//! |---|---|---|
//! | absent | — | one informational line, exactly like a first-launch `.srm`; not an error |
//! | unparseable | — | warning, run without symbols |
//! | parsed, [`RomBinding::Mismatch`] | **refused** | warning naming the fault, run without symbols |
//! | parsed, [`RomBinding::Match`] / [`RomBinding::Indeterminate`] | accepted | symbols annotate output |
//!
//! The refusal is the point. A listing from a different build shape is not *degraded* information — of the
//! symbols `s4.lst` and `s4.debug.lst` share, 92.6% name a different address, so every annotation would be
//! confidently wrong. The suite contract's decision D7 records exactly this failure: a session in which
//! every symbol shifted by `+$24` and a "verified" hardcoded literal rotted while it was being used.
//!
//! `Indeterminate` is accepted rather than refused because it means "this listing carries no `EndOfRom`, so
//! there is nothing to check" — a hand-written or non-Aeon listing, where refusing would be a false negative.

use oracle_core::symbols::{RomBinding, SymbolTable};
use std::path::{Path, PathBuf};

/// The `.lst` listing that sits next to the ROM: same directory + stem, `.lst` extension
/// (`.../s4.bin` → `.../s4.lst`). This is exactly where `sigil build --emit-lst` writes it, so a normal
/// Aeon build needs no configuration at all.
pub fn lst_path_for(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("lst")
}

/// Load the symbol table for `rom_path`, validate it against the `rom` bytes about to be run, and return it
/// only if it may be trusted. Prints a one-line account of whichever outcome occurred.
///
/// `rom` must be the image actually loaded — that is what the binding check probes.
pub fn load_symbols(rom_path: &Path, rom: &[u8]) -> Option<SymbolTable> {
    let path = lst_path_for(rom_path);
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!(
            "symbols: none at {} (build with `sigil build --emit-lst` to name addresses)",
            path.display()
        );
        return None;
    };

    let table = match SymbolTable::parse(&text) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "symbols: {} is not a usable listing ({e}) — running with raw addresses",
                path.display()
            );
            return None;
        }
    };

    match table.validate_against_rom(rom) {
        RomBinding::Mismatch(fault) => {
            // Refuse rather than tolerate: see the module doc.
            eprintln!(
                "symbols: REFUSED {} — it does not describe this ROM ({fault:?}). Rebuild, or point at \
                 the matching listing; running with raw addresses.",
                path.display()
            );
            return None;
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

    // A count that disagrees with the file's own footer means a truncated or half-written listing. Worth
    // saying out loud, but the symbols that *did* parse are still correct, so it is not a refusal.
    if table.matches_declared_count() == Some(false) {
        eprintln!(
            "symbols: warning — parsed {} but {} declares {:?}; the file may be truncated",
            table.len(),
            path.display(),
            table.declared_count()
        );
    }
    Some(table)
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

    /// A scratch directory unique to this test binary run.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oracle-sym-{}-{tag}", std::process::id()));
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
    fn a_missing_listing_is_normal_and_yields_no_table() {
        let rom = scratch("missing").join("nothing-here.bin");
        assert!(load_symbols(&rom, &[0u8; 0x200]).is_none());
    }

    /// The load-bearing path, end to end through the filesystem: a listing whose `EndOfRom` does not find
    /// the appendix in the image being run is **refused**, so the caller gets no table at all rather than
    /// one that names the wrong addresses.
    #[test]
    fn a_listing_from_a_different_build_is_refused() {
        let dir = scratch("refuse");
        let rom_path = dir.join("game.bin");
        std::fs::write(&rom_path, [0u8; 4]).unwrap();
        // The image has its appendix at $8000; the listing insists on $9000.
        std::fs::write(dir.join("game.lst"), listing_for(0x9000)).unwrap();
        assert!(
            load_symbols(&rom_path, &rom_with_appendix(0x8000)).is_none(),
            "a mismatched listing must be refused, not loaded"
        );
    }

    /// …and the matching listing for the same image loads and resolves.
    #[test]
    fn a_matching_listing_loads_and_resolves() {
        let dir = scratch("accept");
        let rom_path = dir.join("game.bin");
        std::fs::write(&rom_path, [0u8; 4]).unwrap();
        std::fs::write(dir.join("game.lst"), listing_for(0x8000)).unwrap();
        let t = load_symbols(&rom_path, &rom_with_appendix(0x8000)).expect("must load");
        assert_eq!(t.address_of("Main"), Some(0x300));
        assert_eq!(t.resolve(0x304).unwrap().to_string(), "Main+$4");
    }
}
