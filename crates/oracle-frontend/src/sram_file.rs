//! Slice S2 — `.srm` battery-save persistence, the frontend-only file layer. The core owns the live SRAM
//! bytes and a dirty flag ([`System::sram`](oracle_core::system::System::sram) /
//! [`sram_dirty`](oracle_core::system::System::sram_dirty)); this module is the pure, unit-testable I/O half:
//! derive the `.srm` path next to the ROM, read an existing image on boot, and write it back **atomically**.
//! All functions are side-effect-isolated so `main.rs` stays the only place that owns wall-clock timing / the
//! run loop. See `docs/2026-07-23-sram-design-recon.md` (§C8/§C9, S2).
//!
//! # Atomic replacement ([`write_atomic`])
//!
//! The save file is the player's only copy of their progress, so it is never written in place. `std::fs::write`
//! truncates the destination and *then* streams the new bytes: a crash, a power loss, or a full disk anywhere
//! in that window leaves a truncated or half-old/half-new `.srm` where a complete save used to be, and a
//! concurrent reader can observe the same partial state. [`write_atomic`] instead writes a sibling temporary
//! file, `fsync`s it, and `rename`s it over the target — a same-directory rename is an atomic directory-entry
//! swap on POSIX, and a same-volume replace on Windows, so a reader sees either the whole old image or the
//! whole new one and never anything in between. The temporary **must** be a sibling: `rename` across
//! filesystems fails with `EXDEV`, which is exactly what a `/tmp` scratch file would hit whenever the ROM
//! lives on another mount.
//!
//! This is the frontend's shared file-writing primitive: [`crate::save_state`] writes its slot files through
//! it too, for the same reason.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Write the SRAM buffer to `path`, replacing any existing save **atomically** (see the module doc). Returns
/// the I/O result so the caller can log a failure without aborting the emulator; on failure the previous
/// `.srm` is still whole and no temporary file is left behind.
pub fn save_srm(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_atomic(path, bytes)
}

/// The scratch file [`write_atomic`] writes before renaming: a dot-prefixed sibling of `path` in the **same
/// directory**, so the rename stays within one filesystem (a cross-filesystem rename fails with `EXDEV`).
/// The name carries the process id and a per-process counter so two writers — or two tests sharing a
/// directory — never collide on it.
fn temp_path_for(path: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().unwrap_or_else(|| Path::new(""));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{name}.tmp{}-{seq}", std::process::id()))
}

/// Replace `path` with `bytes` atomically: write a sibling temporary, flush it all the way to the device, then
/// `rename` it over the destination. A reader therefore observes either the complete previous file or the
/// complete new one — never a truncated or half-written save (module doc).
///
/// Every failure path returns the underlying [`std::io::Error`] (never panics) and removes the temporary, so a
/// failed save leaves neither a damaged target nor a turd next to it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_then_rename(&temp_path_for(path), path, bytes)
}

/// The body of [`write_atomic`], with the temporary path injected. Split out so the "the write could not even
/// be started" failure path is testable (a `tmp` in a directory that does not exist), which is precisely the
/// case where a non-atomic writer would already have destroyed the destination.
///
/// `tmp` must be a sibling of `path` — see [`temp_path_for`]; the split is for tests, not for callers who want
/// a scratch directory elsewhere.
fn write_then_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    // `sync_all` before the rename is what makes the guarantee real: without it the rename can reach the disk
    // before the data does, so a power loss could publish a name pointing at an unwritten file.
    let written = (|| -> std::io::Result<()> {
        let mut f = File::create(tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(tmp); // best-effort cleanup; the original error is what the caller wants
        return Err(e);
    }
    if let Err(e) = std::fs::rename(tmp, path) {
        let _ = std::fs::remove_file(tmp);
        return Err(e);
    }
    sync_parent_dir(path);
    Ok(())
}

/// `fsync` the directory holding `path`, so the rename itself (a directory-entry change) survives a power
/// loss and not just the file contents. Best-effort and deliberately silent: opening a directory as a file is
/// a POSIX facility that Windows does not offer, and a save that is durable-but-not-yet-fsynced is still a
/// complete file — failing the write over it would be worse than the risk it covers.
fn sync_parent_dir(path: &Path) {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let _ = File::open(&dir).and_then(|d| d.sync_all());
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

    /// A scratch **directory** the atomic-write tests own outright, so "what files exist next to the target"
    /// is a meaningful assertion. The caller removes it.
    fn unique_temp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "oracle_srmdir_{tag}_{nanos}_{:?}",
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&p).expect("create the scratch directory");
        p
    }

    /// The temporary is a **sibling** of the target — the same-filesystem requirement that makes the final
    /// `rename` atomic (a `/tmp` scratch file would fail with `EXDEV` for a ROM on another mount), and it is
    /// distinct on every call so concurrent writers cannot collide.
    #[test]
    fn the_temp_file_is_a_sibling_of_the_target() {
        let target = Path::new("/games/saves/sonic3.srm");
        let a = temp_path_for(target);
        let b = temp_path_for(target);
        assert_eq!(
            a.parent(),
            target.parent(),
            "the temp must live in the target's own directory"
        );
        assert_ne!(a, b, "each call yields a distinct temp path");
        // A bare relative filename resolves to the same (current) directory, not to an absolute scratch dir.
        let rel = temp_path_for(Path::new("sonic3.srm"));
        assert_eq!(rel.parent(), Some(Path::new("")));
    }

    /// A completed save replaces the previous one exactly, including shrinking it, and leaves no temporary
    /// behind: after the writes the directory holds the target and nothing else.
    #[test]
    fn write_atomic_replaces_the_file_and_leaves_no_temporaries() {
        let dir = unique_temp_dir("replace");
        let target = dir.join("save.srm");

        save_srm(&target, &vec![0xAAu8; 4096]).expect("first write");
        assert_eq!(load_srm(&target).unwrap(), vec![0xAAu8; 4096]);
        // A shorter image must fully replace the longer one (no stale tail).
        save_srm(&target, &[1, 2, 3]).expect("second, shorter write");
        assert_eq!(load_srm(&target).unwrap(), vec![1, 2, 3]);

        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("save.srm")], "no turds");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The data-loss property.** When the new image cannot be written at all, the previous save must survive
    /// untouched — the exact case a truncate-then-write loses. The failure is injected by pointing the
    /// temporary at a directory that does not exist, so `File::create` fails before a single byte is
    /// committed anywhere.
    #[test]
    fn a_failed_write_leaves_the_previous_save_intact() {
        let dir = unique_temp_dir("failed");
        let target = dir.join("save.srm");
        let good = vec![0x5Au8; 2048];
        save_srm(&target, &good).expect("the good save");

        let doomed_tmp = dir.join("no_such_subdir").join(".save.srm.tmp");
        let err = write_then_rename(&doomed_tmp, &target, &[0xFFu8; 2048])
            .expect_err("writing through a missing directory must fail");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);

        assert_eq!(
            load_srm(&target).expect("the old save is still readable"),
            good,
            "a failed write must not touch the previous save"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The atomicity property, observed.** A reader hammering the file while it is repeatedly replaced must
    /// only ever see a *complete* image — the in-process stand-in for "a crash mid-write cannot corrupt the
    /// save". With a truncate-then-write save this fails almost immediately (zero-length and short reads);
    /// with the rename it is structurally impossible, because `fs::read` resolves the name to one inode and
    /// that inode is always whole.
    ///
    /// The writer keeps replacing the file until the reader has actually observed it `MIN_READS` times, rather
    /// than for a fixed number of iterations. That removes the load-dependence: on a saturated machine the
    /// reader thread may not be scheduled at all during a short fixed loop, which would make the test report
    /// "no partial reads" (or trip its own liveness assert) without having exercised anything.
    #[test]
    fn a_concurrent_reader_never_observes_a_partial_file() {
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        use std::sync::Arc;

        const LEN: usize = 8192;
        const MIN_WRITES: usize = 40; // enough replacements to overlap a reader that is running
        const MIN_READS: usize = 20; // …and enough observed reads to make "none were partial" mean something
        const MAX_WRITES: usize = 5000; // liveness bound, so a starved reader fails loudly instead of hanging

        let dir = unique_temp_dir("concurrent");
        let target = dir.join("save.srm");
        let (a, b) = (vec![0xAAu8; LEN], vec![0xBBu8; LEN]);
        save_srm(&target, &a).expect("seed the file");

        let stop = Arc::new(AtomicBool::new(false));
        let reads = Arc::new(AtomicUsize::new(0));
        let reader = {
            let (path, stop, reads) = (target.clone(), Arc::clone(&stop), Arc::clone(&reads));
            std::thread::spawn(move || {
                let mut partial = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    if let Ok(v) = std::fs::read(&path) {
                        reads.fetch_add(1, Ordering::Relaxed);
                        let whole = v.len() == LEN
                            && (v.iter().all(|&x| x == 0xAA) || v.iter().all(|&x| x == 0xBB));
                        if !whole {
                            partial += 1;
                        }
                    }
                }
                partial
            })
        };

        let mut writes = 0usize;
        while writes < MAX_WRITES {
            save_srm(&target, if writes.is_multiple_of(2) { &b } else { &a }).expect("replace");
            writes += 1;
            if writes >= MIN_WRITES && reads.load(Ordering::Relaxed) >= MIN_READS {
                break;
            }
        }
        stop.store(true, Ordering::Relaxed);
        let partial = reader.join().expect("the reader thread");
        let reads = reads.load(Ordering::Relaxed);

        assert!(
            reads >= MIN_READS,
            "the reader only managed {reads} reads over {writes} replacements — it never ran concurrently, \
             so this test proved nothing"
        );
        assert_eq!(
            partial, 0,
            "{partial} of {reads} reads saw a partial/mixed image — the replacement is not atomic"
        );
        let _ = std::fs::remove_dir_all(&dir);
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
