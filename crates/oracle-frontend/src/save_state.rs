//! User-facing **save states** — the frontend-only container + file layer around the core's
//! [`System::snapshot`]/[`System::restore`] pair.
//!
//! The core already serializes the whole machine (bincode, every field of `System` including the cartridge
//! ROM and SRAM). What it does *not* give us is any way to tell a **stale or corrupt** file from a good one:
//! a bincode blob carries no magic, no version, and no checksum, and — measured, not assumed (see
//! "What `restore` actually does", below) — it will happily decode a byte-flipped payload into a silently
//! wrong machine. This module wraps the blob in a small header that makes those cases hard errors.
//!
//! # File format (all little-endian)
//!
//! | Offset | Size | Field |
//! |--------|------|-------|
//! | 0      | 4    | magic `ONSS` ("oracle-next save state") |
//! | 4      | 2    | [`FORMAT_VERSION`] — this container's own version |
//! | 6      | 8    | [`layout_fingerprint`] — identifies the *bincode layout* of `System` |
//! | 14     | 8    | [`rom_fingerprint`] — identifies the cartridge the state belongs to |
//! | 22     | 8    | payload length in bytes |
//! | 30     | 8    | FNV-1a of the payload |
//! | 38     | …    | the `System::snapshot()` bincode payload |
//!
//! The interesting field is the **layout fingerprint**. A bincode snapshot is layout-coupled: add, remove,
//! reorder, or retype a `System` field and yesterday's file decodes into garbage (or, worse, decodes
//! "successfully"). Rather than rely on a human remembering to bump a constant, the fingerprint is *derived*:
//! it is the FNV-1a of a canonical `System::new(PROBE_SEED).snapshot()`, computed at runtime. Any change to
//! the encoded shape of `System` — and any change to the deterministic power-on fill, which is just as
//! invalidating — moves those bytes and therefore rejects every older file. It is deliberately conservative
//! (it can reject files that would in fact have loaded); a refused load costs a save state, a silently wrong
//! load costs trust. It is not a *proof* of layout equality: bincode varint-encodes integers, so e.g. a
//! `u16 → u32` field whose probe value is 0 encodes identically. The payload checksum and the exact-length
//! check cover the rest.
//!
//! # What `restore` actually does on malformed input (measured, bincode 2.0.1, `config::standard()`)
//!
//! [`System::restore`] is a **static constructor**: it returns `Result<System, DecodeError>`, a brand-new
//! machine or nothing. It therefore *cannot* half-apply — the caller swaps whole `System` values, so a failed
//! load structurally leaves the running machine untouched. Probing it with truncated, byte-flipped, and
//! adversarial inputs found:
//!
//! * truncation at any offset → `Err(UnexpectedEnd)`; **no panic**;
//! * random junk, including a maximal varint length prefix → `Err(InvalidIntegerType)`; **no panic, no
//!   runaway allocation** (bincode bounds container reads by the remaining input);
//! * **trailing garbage after a complete payload is silently accepted** — so a *shrunk* `System` would decode
//!   "fine" and leave bytes on the floor. Hence this module's explicit length field: the payload must be
//!   exactly as long as the header claims, and the file exactly header + payload;
//! * **a flipped byte inside a data region (RAM/VRAM/…) decodes OK, silently** — bincode has no integrity
//!   check at all. Hence the payload checksum.
//!
//! # Key map (mirrored in `main.rs`'s controls table and its startup printout)
//!
//! | Key   | Action |
//! |-------|--------|
//! | F2    | save state to the current slot |
//! | F4    | load state from the current slot |
//! | F6/F7 | previous / next slot (wraps over 0..=9) |
//! | 0–9   | select that slot directly |
//!
//! # Files
//!
//! Named by the same rule as the `.srm` battery save (see [`crate::sram_file`]): next to the ROM, same stem,
//! extension swapped — `…/foo.bin` slot 3 → `…/foo.state3`.

use oracle_core::system::System;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Container magic: "Oracle-Next Save State".
const MAGIC: [u8; 4] = *b"ONSS";

/// Version of *this container* (the header, not the payload). Bump when the header layout changes; the
/// payload's own layout churn is caught automatically by [`layout_fingerprint`].
pub const FORMAT_VERSION: u16 = 1;

/// Bytes of header preceding the bincode payload: magic(4) + version(2) + layout(8) + rom(8) + len(8) +
/// checksum(8).
pub const HEADER_LEN: usize = 4 + 2 + 8 + 8 + 8 + 8;

/// Number of numbered save-state slots (`0..SLOT_COUNT`), one per number key.
pub const SLOT_COUNT: usize = 10;

/// Seed of the canonical probe machine whose snapshot defines [`layout_fingerprint`]. Any fixed value works;
/// changing it merely invalidates existing save states.
const PROBE_SEED: u64 = 0x5A5E_57A7_E0B0_0B1E;

/// FNV-1a 64-bit over `bytes` — the same construction the core uses for `export_state_hash`, reimplemented
/// here so this frontend container never reaches into currency code.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// Identity of the cartridge a save state belongs to: FNV-1a of the ROM image, mixed with its length so two
/// ROMs cannot collide on a truncation. Loading a state saved from a *different* ROM would otherwise
/// "succeed" — the snapshot carries the ROM bytes with it, so it would silently swap the running game out
/// from under the frontend (whose `.srm` path, window title, etc. still point at the original).
pub fn rom_fingerprint(rom: &[u8]) -> u64 {
    fnv1a(rom) ^ (rom.len() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Fingerprint of the bincode layout of `System`, derived from a canonical power-on snapshot (see the module
/// doc). Computed once per process and cached.
pub fn layout_fingerprint() -> u64 {
    static FP: OnceLock<u64> = OnceLock::new();
    *FP.get_or_init(|| fnv1a(&System::new(PROBE_SEED).snapshot()))
}

/// The save-state file for `rom_path` and `slot`: same directory and stem as the ROM, extension `state{slot}`
/// (`…/foo.bin` slot 3 → `…/foo.state3`). Exactly the `.srm` rule in [`crate::sram_file::srm_path_for`], so
/// users find save states where they already find battery saves.
pub fn state_path_for(rom_path: &Path, slot: usize) -> PathBuf {
    rom_path.with_extension(format!("state{slot}"))
}

/// Why a save-state file was refused. Every variant is a **clean** refusal: the caller keeps running the
/// machine it already had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// Fewer bytes than the fixed header.
    TooShort { len: usize },
    /// The first four bytes are not `ONSS` — not one of our files.
    BadMagic,
    /// Written by a different container version.
    Version { found: u16, expected: u16 },
    /// Written against a different `System` layout — a stale state from an older build.
    Layout { found: u64, expected: u64 },
    /// Written while running a different ROM.
    Rom { found: u64, expected: u64 },
    /// The file is not exactly header + the payload length it declares (truncated or padded).
    Length { declared: u64, actual: usize },
    /// The payload does not match its recorded checksum — corruption.
    Checksum { found: u64, expected: u64 },
    /// bincode refused the payload (the layout guard did not catch this one).
    Decode(String),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::TooShort { len } => {
                write!(f, "not a save state: only {len} bytes (header is {HEADER_LEN})")
            }
            StateError::BadMagic => write!(f, "not a save state: bad magic"),
            StateError::Version { found, expected } => write!(
                f,
                "save-state format v{found}, this build writes v{expected} — state discarded"
            ),
            StateError::Layout { found, expected } => write!(
                f,
                "stale save state: written by a build with a different machine layout \
                 (fingerprint {found:#018x}, this build {expected:#018x}) — state discarded"
            ),
            StateError::Rom { found, expected } => write!(
                f,
                "save state belongs to a different ROM (fingerprint {found:#018x}, \
                 loaded ROM {expected:#018x}) — state discarded"
            ),
            StateError::Length { declared, actual } => write!(
                f,
                "corrupt save state: declares a {declared}-byte payload but the file is {actual} bytes"
            ),
            StateError::Checksum { found, expected } => write!(
                f,
                "corrupt save state: payload checksum {found:#018x}, expected {expected:#018x}"
            ),
            StateError::Decode(e) => write!(f, "corrupt save state: {e}"),
        }
    }
}

impl std::error::Error for StateError {}

/// Serialize `sys` into a versioned, checksummed save-state image for the cartridge identified by `rom_fp`.
pub fn encode(sys: &System, rom_fp: u64) -> Vec<u8> {
    let payload = sys.snapshot();
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&layout_fingerprint().to_le_bytes());
    out.extend_from_slice(&rom_fp.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&fnv1a(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

/// Validate and decode a save-state image for the cartridge identified by `rom_fp`, returning a **new**
/// machine. Every failure path returns `Err` and constructs nothing: the caller's running `System` is only
/// replaced on `Ok`, so a bad file can never leave a half-restored machine behind.
pub fn decode(bytes: &[u8], rom_fp: u64) -> Result<System, StateError> {
    if bytes.len() < HEADER_LEN {
        return Err(StateError::TooShort { len: bytes.len() });
    }
    if bytes[0..4] != MAGIC {
        return Err(StateError::BadMagic);
    }
    let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u64_at = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().expect("8 bytes"));

    let version = u16_at(4);
    if version != FORMAT_VERSION {
        return Err(StateError::Version {
            found: version,
            expected: FORMAT_VERSION,
        });
    }
    let layout = u64_at(6);
    let expected_layout = layout_fingerprint();
    if layout != expected_layout {
        return Err(StateError::Layout {
            found: layout,
            expected: expected_layout,
        });
    }
    let rom = u64_at(14);
    if rom != rom_fp {
        return Err(StateError::Rom {
            found: rom,
            expected: rom_fp,
        });
    }
    // Exact-length: bincode silently tolerates trailing bytes, so the container — not the decoder — is what
    // rejects a padded or truncated file.
    let declared = u64_at(22);
    if declared != (bytes.len() - HEADER_LEN) as u64 {
        return Err(StateError::Length {
            declared,
            actual: bytes.len(),
        });
    }
    let payload = &bytes[HEADER_LEN..];
    let checksum = u64_at(30);
    let actual_checksum = fnv1a(payload);
    if actual_checksum != checksum {
        return Err(StateError::Checksum {
            found: actual_checksum,
            expected: checksum,
        });
    }
    System::restore(payload).map_err(|e| StateError::Decode(e.to_string()))
}

/// A save-state load failure: either the file could not be read, or its contents were refused.
#[derive(Debug)]
pub enum LoadError {
    /// The file could not be read (missing slot, permissions, …).
    Io(std::io::Error),
    /// The file was read but refused — see [`StateError`].
    State(StateError),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "{e}"),
            LoadError::State(e) => write!(f, "{e}"),
        }
    }
}

/// Write `sys` to `path` as a save state. Returns the byte count written, or the I/O error (a failed save is
/// reported, never fatal — exactly like the `.srm` writer).
pub fn save(path: &Path, sys: &System, rom_fp: u64) -> std::io::Result<usize> {
    let image = encode(sys, rom_fp);
    std::fs::write(path, &image)?;
    Ok(image.len())
}

/// Read and validate the save state at `path`, returning a new machine. On any error the caller simply keeps
/// running the machine it has.
pub fn load(path: &Path, rom_fp: u64) -> Result<System, LoadError> {
    let bytes = std::fs::read(path).map_err(LoadError::Io)?;
    decode(&bytes, rom_fp).map_err(LoadError::State)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vendored test ROM (`vendor/` is gitignored — a fresh worktree needs the symlink or these tests
    /// cannot run, so a missing file is a hard failure with a pointer, never a silent skip).
    fn vendored(name: &str) -> Vec<u8> {
        let path = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../vendor/TestRoms/"
        ))
        .join(name);
        std::fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "vendored test ROM {} is missing ({e}) — run tools/fetch-testroms.sh (or symlink vendor/ \
                 into this worktree); these tests must not skip silently",
                path.display()
            )
        })
    }

    fn booted(name: &str) -> (System, u64) {
        let rom = vendored(name);
        let fp = rom_fingerprint(&rom);
        let mut sys = System::new(0x5EED);
        sys.load_rom(rom);
        sys.reset();
        (sys, fp)
    }

    /// A unique-enough temp path; the caller removes it. Mirrors `sram_file`'s helper.
    fn unique_temp(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "oracle_state_{tag}_{nanos}_{:?}.bin",
            std::thread::current().id()
        ));
        p
    }

    #[test]
    fn state_path_matches_the_srm_naming_rule() {
        assert_eq!(
            state_path_for(Path::new("/games/sonic3.bin"), 0),
            PathBuf::from("/games/sonic3.state0")
        );
        assert_eq!(
            state_path_for(Path::new("/games/rom"), 9),
            PathBuf::from("/games/rom.state9")
        );
        // Multi-dot stem: only the final component is replaced, exactly like `srm_path_for`.
        assert_eq!(
            state_path_for(Path::new("phantasy.star.md"), 4),
            PathBuf::from("phantasy.star.state4")
        );
        // Every slot maps to a distinct file.
        let all: std::collections::BTreeSet<_> = (0..SLOT_COUNT)
            .map(|s| state_path_for(Path::new("/g/r.bin"), s))
            .collect();
        assert_eq!(all.len(), SLOT_COUNT);
    }

    /// **The round-trip proof.** Run a ROM, save, run further, restore, run the *same* further frames again,
    /// and require the machine to land on the identical state — in both currencies (`state_hash`, the
    /// Oracle-compatible VDP hash, and `export_state_hash`, the determinism gate's). Both are read-only uses.
    ///
    /// Non-vacuity is asserted inline: the post-advance hashes must *differ* from the hashes at the save
    /// point, otherwise "the timelines match" would be trivially true of a machine that never changes.
    #[test]
    fn save_then_load_reproduces_the_original_timeline() {
        const WARMUP: u64 = 12;
        const ADVANCE: u64 = 17;

        let (mut sys, fp) = booted("m68k_memory_test.bin");
        sys.run_frames(WARMUP);

        let image = encode(&sys, fp);
        let at_save = (sys.state_hash(), sys.export_state_hash());

        // Timeline A: keep going from the save point.
        sys.run_frames(ADVANCE);
        let timeline_a = (sys.state_hash(), sys.export_state_hash());
        assert_ne!(
            at_save, timeline_a,
            "non-vacuity: the machine must actually evolve over {ADVANCE} frames, else this test proves \
             nothing"
        );

        // Timeline B: restore the save and replay the same frames.
        let mut restored = decode(&image, fp).expect("the image we just wrote must load");
        assert_eq!(
            (restored.state_hash(), restored.export_state_hash()),
            at_save,
            "a freshly restored machine equals the machine at the save point"
        );
        restored.run_frames(ADVANCE);
        assert_eq!(
            (restored.state_hash(), restored.export_state_hash()),
            timeline_a,
            "replaying {ADVANCE} frames from a restored state reproduces the original timeline exactly"
        );

        // Full structural equality, not just the two hashes (`System: PartialEq`).
        assert!(
            restored == sys,
            "the whole machine — not only the hashed regions — matches after the round trip"
        );
    }

    /// The on-disk path: [`save`] then [`load`] behave like [`encode`]/[`decode`].
    #[test]
    fn file_round_trip() {
        let (mut sys, fp) = booted("m68k_memory_test.bin");
        sys.run_frames(3);
        let path = unique_temp("file");
        let n = save(&path, &sys, fp).expect("write the state file");
        assert_eq!(n, std::fs::metadata(&path).unwrap().len() as usize);
        let back = load(&path, fp).expect("read it back");
        assert!(back == sys, "the file round-trips the whole machine");
        let _ = std::fs::remove_file(&path);
    }

    /// A missing slot is a plain `Io` error, not a panic.
    #[test]
    fn loading_a_missing_slot_is_a_clean_io_error() {
        let path = unique_temp("missing");
        assert!(!path.exists());
        match load(&path, 0) {
            Err(LoadError::Io(_)) => {}
            other => panic!("expected an Io error, got {other:?}"),
        }
    }

    /// **Corrupt file.** A single flipped payload byte — which bincode itself decodes *silently and wrongly*
    /// (measured; see the module doc) — must be refused by the checksum, and the caller's running machine
    /// must be untouched.
    #[test]
    fn a_flipped_payload_byte_is_refused_and_the_running_machine_survives() {
        let (mut sys, fp) = booted("m68k_memory_test.bin");
        sys.run_frames(4);
        let image = encode(&sys, fp);
        let before = (sys.state_hash(), sys.export_state_hash());

        // Deep inside a data region — the exact case bincode accepts silently.
        let mut corrupt = image.clone();
        let pos = HEADER_LEN + (image.len() - HEADER_LEN) / 2;
        corrupt[pos] ^= 0xFF;

        match decode(&corrupt, fp) {
            Err(StateError::Checksum { .. }) => {}
            other => panic!("a flipped payload byte must be a Checksum error, got {other:?}"),
        }
        // The frontend swaps whole `System` values, so a refusal leaves the live machine exactly as it was.
        assert_eq!(
            (sys.state_hash(), sys.export_state_hash()),
            before,
            "a refused load must not touch the running machine"
        );
        // And the intact image still loads — the corruption, not the test setup, caused the failure.
        assert!(decode(&image, fp).is_ok());
    }

    /// **Wrong version.** A file written by a different container version is refused with a clear message.
    #[test]
    fn a_wrong_container_version_is_refused() {
        let (sys, fp) = booted("m68k_memory_test.bin");
        let mut image = encode(&sys, fp);
        image[4..6].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        match decode(&image, fp) {
            Err(StateError::Version { found, expected }) => {
                assert_eq!((found, expected), (FORMAT_VERSION + 1, FORMAT_VERSION));
            }
            other => panic!("expected a Version error, got {other:?}"),
        }
    }

    /// **Stale layout.** The derived fingerprint stands in for "an older build wrote this"; a file whose
    /// fingerprint does not match the running build is refused before bincode ever sees the payload.
    #[test]
    fn a_stale_layout_fingerprint_is_refused() {
        let (sys, fp) = booted("m68k_memory_test.bin");
        let mut image = encode(&sys, fp);
        let stale = layout_fingerprint() ^ 1;
        image[6..14].copy_from_slice(&stale.to_le_bytes());
        match decode(&image, fp) {
            Err(StateError::Layout { found, expected }) => {
                assert_eq!((found, expected), (stale, layout_fingerprint()));
            }
            other => panic!("expected a Layout error, got {other:?}"),
        }
    }

    /// The layout fingerprint is stable within a build and is genuinely derived from the encoded `System`
    /// (it is the hash of a real snapshot, not a constant).
    #[test]
    fn layout_fingerprint_is_stable_and_derived() {
        assert_eq!(layout_fingerprint(), layout_fingerprint());
        assert_eq!(
            layout_fingerprint(),
            fnv1a(&System::new(PROBE_SEED).snapshot())
        );
    }

    /// **Wrong ROM.** The snapshot carries the cartridge with it, so without this guard loading another
    /// game's state would silently swap the running game.
    #[test]
    fn a_state_from_another_rom_is_refused() {
        let (sys, fp) = booted("m68k_memory_test.bin");
        let image = encode(&sys, fp);
        let other = rom_fingerprint(&vendored("vcounter.bin"));
        assert_ne!(other, fp, "the two vendored ROMs fingerprint differently");
        match decode(&image, other) {
            Err(StateError::Rom { found, expected }) => {
                assert_eq!((found, expected), (fp, other));
            }
            other_err => panic!("expected a Rom error, got {other_err:?}"),
        }
    }

    /// Truncation, padding, short files and foreign files are all clean refusals — never a panic.
    #[test]
    fn malformed_files_are_clean_refusals() {
        let (sys, fp) = booted("m68k_memory_test.bin");
        let image = encode(&sys, fp);

        assert!(matches!(
            decode(&[], fp),
            Err(StateError::TooShort { len: 0 })
        ));
        assert!(matches!(
            decode(&image[..HEADER_LEN - 1], fp),
            Err(StateError::TooShort { .. })
        ));
        assert!(matches!(
            decode(&vec![0xAAu8; 4096], fp),
            Err(StateError::BadMagic)
        ));

        // Truncated payload: the declared length no longer matches the file.
        assert!(matches!(
            decode(&image[..image.len() - 16], fp),
            Err(StateError::Length { .. })
        ));
        // Padded payload: bincode alone would accept this (trailing bytes are ignored — measured).
        let mut padded = image.clone();
        padded.extend_from_slice(&[0u8; 16]);
        assert!(matches!(
            decode(&padded, fp),
            Err(StateError::Length { .. })
        ));

        // A payload that passes length + checksum but is not a valid `System` still fails cleanly.
        let junk = vec![0xFFu8; 512];
        let mut forged = Vec::new();
        forged.extend_from_slice(&MAGIC);
        forged.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        forged.extend_from_slice(&layout_fingerprint().to_le_bytes());
        forged.extend_from_slice(&fp.to_le_bytes());
        forged.extend_from_slice(&(junk.len() as u64).to_le_bytes());
        forged.extend_from_slice(&fnv1a(&junk).to_le_bytes());
        forged.extend_from_slice(&junk);
        assert!(matches!(decode(&forged, fp), Err(StateError::Decode(_))));
    }

    /// **Empirical answer to "does a snapshot include SRAM?"** — build a cart whose reset vector runs three
    /// instructions: enable `$A130F1` SRAM access, store a byte into the SRAM window, then spin. After one
    /// frame the SRAM buffer holds the byte and both bookkeeping flags are set; a save-state round trip must
    /// carry *all three* across — which is exactly why a state load rolls the battery backwards and why the
    /// frontend cancels its pending `.srm` autosave (see `main.rs`).
    #[test]
    fn snapshot_carries_sram_bytes_and_its_dirty_bookkeeping() {
        // Hand-assembled 68000: reset SSP/PC, then
        //   move.b #$01,$00A130F1   ; SRAM access on
        //   move.b #$5A,$00200001   ; store into the fallback SRAM window (odd byte lane)
        //   bra.s  *                ; spin
        let mut rom = vec![0u8; 0x400];
        rom[0x00..0x04].copy_from_slice(&0x00FF_0000u32.to_be_bytes()); // initial SSP
        rom[0x04..0x08].copy_from_slice(&0x0000_0200u32.to_be_bytes()); // initial PC
        let code: [u8; 18] = [
            0x13, 0xFC, 0x00, 0x01, 0x00, 0xA1, 0x30, 0xF1, // move.b #$01,$00A130F1
            0x13, 0xFC, 0x00, 0x5A, 0x00, 0x20, 0x00, 0x01, // move.b #$5A,$00200001
            0x60, 0xFE, // bra.s *
        ];
        rom[0x200..0x200 + code.len()].copy_from_slice(&code);

        let fp = rom_fingerprint(&rom);
        let mut sys = System::new(0x5EED);
        sys.load_rom(rom);
        sys.reset();
        assert!(
            !sys.sram_dirty() && !sys.sram_used(),
            "clean before running"
        );
        sys.run_frames(1);

        assert_eq!(
            sys.sram()[0],
            0x5A,
            "the guest store reached the SRAM buffer"
        );
        assert!(sys.sram_dirty(), "a guest SRAM write sets the dirty flag");
        assert!(sys.sram_used(), "…and latches the `this cart saves` flag");

        let restored = decode(&encode(&sys, fp), fp).expect("round trip");
        assert_eq!(restored.sram(), sys.sram(), "SRAM bytes ride the snapshot");
        assert!(restored.sram_dirty(), "so does `sram_dirty`");
        assert!(restored.sram_used(), "so does `sram_used`");
    }
}
