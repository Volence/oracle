//! The two refusals that stand between this runner and a confidently wrong answer.
//!
//! # 1. The listing must describe the ROM being run
//!
//! Mirrors `oracle-frontend`'s `symbol_file` policy table exactly — including the rule that an
//! `Indeterminate` verdict on a listing that is **not** [intact](SymbolTable::is_intact) is a refusal, not
//! an acceptance. That rule closes a real fail-open hole: any listing that would be a `Mismatch` becomes
//! `Indeterminate` the moment its `EndOfRom` row goes missing, and truncation cuts from the end, where
//! `EndOfRom` sits.
//!
//! The one difference is the consequence. The frontend degrades to raw addresses; here symbols are
//! load-bearing (every address is resolved by name, because *every documented address in the plans is
//! stale* — see the crate docs), so a refusal is fatal.
//!
//! This is the third copy of that table in the tree (`oracle-frontend/src/symbol_file.rs:50`,
//! `oracle-aether/src/engine.rs:970`, here). That is accepted for exactly one slice: extracting it is a
//! mechanical, no-behaviour-change edit across three crates, and bundling it would blur the review of the
//! runner itself. It is the immediately-following slice, not a follow-up ticket.
//!
//! # 2. The ROM must be a DEBUG build
//!
//! `aeon/engine/system/replay.emp:174-186` gates the checkpoint compare on `if DEBUG == 1`; the release
//! shape *steps over the payload without comparing*. A release ROM therefore runs the stream to completion
//! and sets `Replay_Done = $FF` **having verified nothing** — a false green, which is worse than no gate at
//! all.
//!
//! The shape binding above is suggestive (a `Match` means "not obviously wrong", never "proven right"), so
//! it is backed by a positive assertion: the literal bytes `REPLAY DESYNC` must appear in the image.
//! Verified firsthand against the real artifacts — `s4.bin` contains neither trap string, `s4.debug.bin`
//! contains one occurrence of each.

use oracle_core::symbols::{RomBinding, SymbolTable, TableSource};

/// The trap message the DEBUG-only compare path raises. Its presence in the image is the positive
/// assertion that the compare path was assembled at all.
pub const DESYNC_TRAP_STRING: &[u8] = b"REPLAY DESYNC";

/// What to do with a parsed listing, given the ROM about to be run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LstVerdict {
    /// The listing binds to this image. `note` is the one-line account for the log.
    Accept { note: String },
    /// The listing carries no build fingerprint but is internally whole — a legitimate hand-written or
    /// non-Aeon listing. Accepted, loudly.
    AcceptUnverified { note: String },
    /// Fatal. `reason` names the fault.
    Refuse { reason: String },
}

/// Apply the accept/refuse policy table to `table` against `rom`.
pub fn judge_listing(table: &SymbolTable, rom: &[u8]) -> LstVerdict {
    match table.validate_against_rom(rom) {
        RomBinding::Mismatch(fault) => LstVerdict::Refuse {
            reason: format!(
                "the listing does not describe this ROM ({fault:?}). Rebuild, or point --lst at the \
                 matching listing"
            ),
        },
        // "No fingerprint" plus "damaged file" most likely means the fingerprint symbol fell off a
        // truncated end — a mismatch wearing a disguise.
        RomBinding::Indeterminate(why) if !table.is_intact() => LstVerdict::Refuse {
            reason: format!(
                "no build fingerprint ({why:?}) AND the listing is not intact ({}); it may be a \
                 truncated listing for a different ROM",
                integrity_note(table)
            ),
        },
        RomBinding::Indeterminate(why) => LstVerdict::AcceptUnverified {
            note: format!("carries no build fingerprint ({why:?}) — loaded unverified"),
        },
        RomBinding::Match {
            appendix_offset,
            appendix_len,
        } => LstVerdict::Accept {
            note: format!(
                "{} symbols, bound to this ROM (deb2 appendix at ${appendix_offset:06X}, \
                 {appendix_len} bytes)",
                table.len()
            ),
        },
    }
}

/// A short human account of *how* a listing failed [`SymbolTable::is_intact`], for the message text.
pub fn integrity_note(table: &SymbolTable) -> String {
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

/// Whether the DEBUG-only checkpoint compare path is present in this image.
pub fn has_desync_trap(rom: &[u8]) -> bool {
    contains(rom, DESYNC_TRAP_STRING)
}

/// Refuse a release ROM, which would run the stream to completion comparing nothing and report a green.
pub fn require_debug_rom(rom: &[u8]) -> Result<(), String> {
    if has_desync_trap(rom) {
        Ok(())
    } else {
        Err(format!(
            "this ROM does not contain the bytes `{}`, so its checkpoint compare was assembled out \
             (`replay.emp:174-186` gates it on DEBUG == 1). A release ROM replays the stream to \
             completion having verified NOTHING and reports a false green — refusing. Build the DEBUG \
             shape (e.g. s4.debug.bin)",
            String::from_utf8_lossy(DESYNC_TRAP_STRING)
        ))
    }
}

/// Plain substring search over bytes (no dependency for a 3-line scan).
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ROM image carrying a `deb2` appendix at `end`, big enough to clear the size floor.
    fn rom_with_appendix(end: usize) -> Vec<u8> {
        let mut rom = vec![0u8; end + 0x4000];
        rom[end] = 0xDE;
        rom[end + 1] = 0xB2;
        rom
    }

    fn listing_for(end: u32) -> String {
        format!(
            "  Symbol Table (* = unused):\n\n Main : 300 C |\n EndOfRom : {end:X} C |\n\n   2 symbols\n"
        )
    }

    #[test]
    fn a_listing_that_binds_is_accepted() {
        let t = SymbolTable::parse(&listing_for(0x8000)).unwrap();
        assert!(matches!(
            judge_listing(&t, &rom_with_appendix(0x8000)),
            LstVerdict::Accept { .. }
        ));
    }

    #[test]
    fn a_listing_from_a_different_build_is_refused() {
        let t = SymbolTable::parse(&listing_for(0x9000)).unwrap();
        assert!(matches!(
            judge_listing(&t, &rom_with_appendix(0x8000)),
            LstVerdict::Refuse { .. }
        ));
    }

    /// The fail-open path, closed: unverifiable **and** visibly damaged is still a refusal.
    #[test]
    fn an_unverifiable_and_damaged_listing_is_refused() {
        let t =
            SymbolTable::parse("  Symbol Table (* = unused):\n\n Main : 300 C |\n\n   9 symbols\n")
                .unwrap();
        assert!(!t.is_intact());
        assert!(matches!(
            judge_listing(&t, &rom_with_appendix(0x8000)),
            LstVerdict::Refuse { .. }
        ));
    }

    /// …but unverifiable and whole is a legitimate hand-written listing, and refusing would be a false
    /// negative.
    #[test]
    fn an_unverifiable_but_intact_listing_is_accepted_loudly() {
        let t =
            SymbolTable::parse("  Symbol Table (* = unused):\n\n Main : 300 C |\n\n   1 symbols\n")
                .unwrap();
        assert!(matches!(
            judge_listing(&t, &rom_with_appendix(0x8000)),
            LstVerdict::AcceptUnverified { .. }
        ));
    }

    #[test]
    fn a_rom_without_the_trap_string_is_refused_as_a_release_build() {
        assert!(require_debug_rom(&[0u8; 0x100]).is_err());
        assert!(!has_desync_trap(&[0u8; 0x100]));
    }

    #[test]
    fn a_rom_carrying_the_trap_string_is_accepted() {
        let mut rom = vec![0u8; 0x100];
        rom[0x40..0x40 + DESYNC_TRAP_STRING.len()].copy_from_slice(DESYNC_TRAP_STRING);
        assert!(has_desync_trap(&rom));
        assert!(require_debug_rom(&rom).is_ok());
    }

    /// The needle straddling nothing: a short image cannot contain it, and the scan must not panic.
    #[test]
    fn the_scan_survives_an_image_shorter_than_the_needle() {
        assert!(!has_desync_trap(b"RE"));
        assert!(!contains(&[], b"x"));
    }
}
