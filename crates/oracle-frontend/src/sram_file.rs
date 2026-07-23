//! Slice S2 — `.srm` battery-save persistence, the frontend-only file layer. The core owns the live SRAM
//! bytes and a dirty flag ([`System::sram`](oracle_core::system::System::sram) /
//! [`sram_dirty`](oracle_core::system::System::sram_dirty)); this module is the pure, unit-testable I/O half:
//! derive the `.srm` path next to the ROM, read an existing image on boot, and write it back atomically-enough
//! with a plain `std::fs::write`. All three functions are side-effect-isolated so `main.rs` stays the only
//! place that owns wall-clock timing / the run loop. See `docs/2026-07-23-sram-design-recon.md` (§C8/§C9, S2).

use std::path::{Path, PathBuf};

/// The `.srm` battery-save file that sits next to the ROM: same directory + stem, `.srm` extension
/// (`.../foo.bin` → `.../foo.srm`). A ROM path with no extension simply gains `.srm`.
pub fn srm_path_for(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("srm")
}

/// Read the `.srm` image if it exists, else `None`. A missing file is the normal first-launch case — never a
/// panic. A genuine read error (permissions, a directory in the way) also degrades to `None`: the frontend
/// treats "couldn't load a save" as "boot with fresh SRAM" rather than refusing to run.
pub fn load_srm(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Write the SRAM buffer to `path`. A plain `std::fs::write` (truncate + write) is enough for this slice —
/// the buffer is small and the frontend debounces so writes are infrequent. Returns the I/O result so the
/// caller can log a failure without aborting the emulator.
pub fn save_srm(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn srm_path_replaces_the_rom_extension() {
        assert_eq!(
            srm_path_for(Path::new("/games/sonic3.bin")),
            PathBuf::from("/games/sonic3.srm")
        );
        // No extension → append `.srm`.
        assert_eq!(
            srm_path_for(Path::new("/games/rom")),
            PathBuf::from("/games/rom.srm")
        );
        // Multi-dot stem: only the final component is replaced.
        assert_eq!(
            srm_path_for(Path::new("phantasy.star.md")),
            PathBuf::from("phantasy.star.srm")
        );
    }

    /// A unique-enough temp path in the OS temp dir; the caller removes it.
    fn unique_temp(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "oracle_srm_{tag}_{nanos}_{:?}.srm",
            std::thread::current().id()
        ));
        p
    }

    #[test]
    fn save_then_load_round_trips_bytes() {
        let path = unique_temp("roundtrip");
        let data: Vec<u8> = (0..=255u8).cycle().take(0x2000).collect();
        save_srm(&path, &data).expect("write the temp .srm");
        let back = load_srm(&path).expect("the file we just wrote loads");
        assert_eq!(back, data, "bytes survive the round-trip unchanged");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_srm_on_a_missing_path_is_none() {
        let path = unique_temp("missing");
        assert!(!path.exists(), "precondition: the file does not exist");
        assert!(
            load_srm(&path).is_none(),
            "a missing .srm loads as None, no panic"
        );
    }

    /// End-to-end: a saved image fed back through [`System::load_sram`] lands in the live SRAM buffer, using a
    /// synthetic "RA" ROM the same way the core's S1 tests build one.
    #[test]
    fn loaded_image_reaches_the_core_sram_buffer() {
        use oracle_core::system::System;

        // Minimal Genesis "RA" header: magic at $1B0-1, parity bit3 in $1B2, [base,end] big-endian in $1B4-B.
        let mut rom = vec![0u8; 0x1000];
        rom[0x1B0] = b'R';
        rom[0x1B1] = b'A';
        rom[0x1B2] = 0xA0 | 0x08; // odd-byte lane
        rom[0x1B4..0x1B8].copy_from_slice(&0x20_0001u32.to_be_bytes());
        rom[0x1B8..0x1BC].copy_from_slice(&0x20_3FFFu32.to_be_bytes());

        let mut sys = System::new(0x5EED);
        sys.load_rom(rom);
        assert!(sys.sram_present(), "the RA cart maps SRAM");

        // Persist a distinctive image, then reload it through the file layer + the core accessor.
        let path = unique_temp("e2e");
        let saved = vec![0x5Au8; sys.sram().len()];
        save_srm(&path, &saved).expect("write");
        let loaded = load_srm(&path).expect("read");
        sys.load_sram(&loaded);
        assert_eq!(
            sys.sram(),
            saved.as_slice(),
            "the .srm image is now the live SRAM"
        );
        assert!(!sys.sram_dirty(), "loading a save is not a guest write");
        let _ = std::fs::remove_file(&path);
    }
}
