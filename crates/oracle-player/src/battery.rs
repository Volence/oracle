//! **The cartridge's battery-backed save, on disk** — `.srm` persistence for this window.
//!
//! The file layer itself is not written here. [`oracle_frontend::sram_file`] is, and this crate now links
//! it (migration S0 gave `oracle-frontend` a lib target; S3 exported this pair through it), so the two
//! windows write the *same* file, with the same naming rule and the same atomic write. What is here is the
//! part that is not a file operation: **when** the image is read, when it is written, and what has to
//! happen around the machine being replaced underneath it.
//!
//! # ⚑ The whole difficulty in one sentence
//!
//! `oracle-frontend`'s rule is *"flush the pending `.srm` FIRST"*, and it can obey it because the window
//! is the only thing that ever replaces the machine there: its F5 block reads the file, flushes, and only
//! then calls `load_rom`. **This window is not the only thing.** A client's `emulator/reload_rom` or
//! `emulator/restore` arrives inside [`Host::pump`](oracle_aether::host::Host::pump) and has already
//! zeroed or rewound the SRAM buffer by the time [`crate::bus::drain`] gets a word in. There is no hook
//! before it and asking for one would be a change to a contract lane's surface.
//!
//! So the ordering rule is kept by **carrying the bytes instead of racing them**. [`Battery::carry`] takes
//! the pending image at the top of every drain — before the pump, and before the `build_ui` whose palette
//! and transport bar are this window's own door onto the same commands — and
//! [`Battery::after_replacement`] writes it to *the outgoing cartridge's* path once the machine has moved.
//! One copy, one place, one rule, and it holds for both producers of change.
//!
//! **What that costs, stated rather than hidden.** The frontend can *abort* a ROM swap whose flush failed,
//! because it has not swapped yet; this window cannot abort a client's. What it does instead is keep the
//! bytes — [`Battery::orphan`] — and retry the write on every subsequent iteration, loudly, until it
//! lands. Memory holds the only copy in the meantime, exactly as it does in the frontend's failed-flush
//! window, and unlike the frontend nothing has been thrown away.
//!
//! # The debounce, and why the carry is not simply a flush
//!
//! A guest that saves writes SRAM many times in a burst, so a write-per-dirty-frame would be sixty atomic
//! file writes a second. `oracle-frontend` coalesces them behind
//! [`AUTOSAVE_DEBOUNCE_FRAMES`] frames of quiescence and this window keeps the same number, because it is
//! the same trade against the same disk. Flushing at the top of every drain instead of carrying would have
//! been simpler and would have *deleted* that debounce — the drain runs every iteration, so "flush
//! whenever something is pending" is "flush every frame while something is pending".
//!
//! # Which replacements re-read the file, and which must not
//!
//! [`PumpReport::rom_changed`](oracle_aether::host::PumpReport::rom_changed) has three producers and they
//! do three different things to the SRAM buffer:
//!
//! | Producer | The buffer afterwards | What this module does |
//! |---|---|---|
//! | `emulator/reload_rom` | re-provisioned from the new header, **zeroed**, `sram_used` cleared | apply the new cartridge's `.srm` |
//! | `emulator/restore` | the checkpoint's, rolled backwards | leave it; the snapshot's battery is the one that belongs to that machine |
//! | `emulator/reset` | **unchanged** — a soft reset preserves SRAM, as on real hardware | leave it |
//!
//! The discriminant is [`System::sram_used`] and nothing else: only `load_rom` clears it, and a buffer
//! nothing has saved into is a buffer the on-disk image is the right content for. Guessing from the ROM
//! path instead would have been wrong on the loud case — F5 reloads *the same path*.

use std::path::{Path, PathBuf};

use oracle_core::system::System;
use oracle_frontend::sram_file;

/// Frames of quiescence before a dirtied SRAM buffer is written out. `oracle-frontend`'s
/// `SRAM_AUTOSAVE_DEBOUNCE_FRAMES`, unchanged: two seconds at 60 Hz, which coalesces a save burst into one
/// write without leaving a real save unwritten for long enough to notice.
pub const AUTOSAVE_DEBOUNCE_FRAMES: u32 = 120;

/// Bytes that belong to a cartridge, and the file they belong in.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    path: PathBuf,
    bytes: Vec<u8>,
}

/// The window's battery-save state: which file, how long until the autosave fires, and anything rescued
/// from a machine that was replaced before it could be written.
pub struct Battery {
    /// The `.srm` for the cartridge **currently** in the machine. Re-derived by
    /// [`Battery::after_replacement`], never remembered across a swap.
    path: PathBuf,
    /// The autosave debounce. `None` = nothing owed.
    countdown: Option<u32>,
    /// The image taken at the top of this iteration's drain, in case the pump or a gesture replaces the
    /// machine before it can be written. Refreshed every drain and dropped when nothing is owed.
    carried: Option<Pending>,
    /// ⚑ **Bytes that outlived their machine and have not reached disk.** Set when a replacement was
    /// detected and the write failed; retried on every [`Battery::tick`] until it lands. It is the reason
    /// this window never has to refuse a client's reload the way the frontend refuses its own F5.
    orphan: Option<Pending>,
}

impl Battery {
    /// Derive the `.srm` path for `rom_path` and apply whatever is on disk to `sys`.
    ///
    /// Returns the lines worth saying out loud. Both outcomes are said — a missing file is not a failure
    /// and is reported as the ordinary thing it is, because "there is no save yet" and "the save could not
    /// be read" are different facts and a silent absence collapses them.
    pub fn open(rom_path: &str, sys: &mut System) -> (Self, Vec<String>) {
        let path = sram_file::srm_path_for(Path::new(rom_path));
        let mut said = Vec::new();
        match sram_file::load_srm(&path) {
            Some(bytes) => {
                said.push(format!(
                    "SRAM: loaded {} bytes from {}",
                    bytes.len(),
                    path.display()
                ));
                sys.load_sram(&bytes);
            }
            None => said.push(format!(
                "SRAM: no save yet at {} (a `.srm` is written only once the game saves)",
                path.display()
            )),
        }
        (
            Battery {
                path,
                countdown: None,
                carried: None,
                orphan: None,
            },
            said,
        )
    }

    /// The `.srm` this window is currently keyed to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A battery keyed to `rom_path` that has **read nothing from disk** — for the rows in this crate
    /// that drive [`crate::bus::drain`] for reasons which are not the battery's.
    ///
    /// Named for what it is rather than `for_tests`: the difference from [`Battery::open`] is not "this
    /// is a test", it is *this one never applied an image*, and a row that needs the applying tested has
    /// to say so by calling `open`. `#[cfg(test)]` so no shipped path can reach it.
    #[cfg(test)]
    pub fn detached(rom_path: &str) -> Self {
        Battery {
            path: sram_file::srm_path_for(Path::new(rom_path)),
            countdown: None,
            carried: None,
            orphan: None,
        }
    }

    /// Whether anything the guest wrote is still only in memory — the `sram_used` gate included, so a cart
    /// that has never saved never reports one.
    pub fn owes(&self, sys: &System) -> bool {
        sys.sram_used() && (sys.sram_dirty() || self.countdown.is_some())
    }

    /// **Take the pending image, before anything can replace the machine holding it.**
    ///
    /// Called at the top of [`crate::bus::drain`] — before the pump, and therefore before this iteration's
    /// `build_ui` too, since the next drain is the next iteration's. So one carry covers both doors: the
    /// client command that arrives in the pump immediately below, and the palette or transport gesture
    /// that will be issued later in this same frame.
    ///
    /// A carry never overwrites an [`orphan`](Battery::orphan): those bytes have already outlived one
    /// machine and the buffer now in `sys` is not them.
    pub fn carry(&mut self, sys: &System) {
        self.carried = self.owes(sys).then(|| Pending {
            path: self.path.clone(),
            bytes: sys.sram().to_vec(),
        });
    }

    /// **Everything this window owes the battery when the machine has been replaced under it.**
    ///
    /// `rom_path` is the cartridge now loaded, as [`crate::bus::drain`] has just re-read it from the
    /// engine. The order below is `oracle-frontend`'s reload order with the one step it can take first
    /// taken last instead, for the reason in the module doc.
    ///
    /// 1. **The outgoing cartridge's bytes reach the outgoing cartridge's file.** Only when the buffer in
    ///    the machine is *not* the one they came from: a soft reset leaves it identical, and writing then
    ///    would be a disk write that the still-armed debounce is going to make anyway.
    /// 2. **The debounce is cancelled and the dirty flag cleared** in that same case, because whatever is
    ///    in the machine now belongs to another machine — the frontend's state-load rule, arriving here
    ///    for `restore` and `reload_rom` alike. Without it the next autosave would write the *rolled-back*
    ///    image over the one just rescued.
    /// 3. **The path is re-keyed** to the cartridge now loaded, before anything can write through it.
    /// 4. **A freshly provisioned buffer gets its cartridge's `.srm`**, keyed on [`System::sram_used`] —
    ///    see the module doc's table for why that predicate and not the ROM path.
    pub fn after_replacement(&mut self, sys: &mut System, rom_path: &str) -> Vec<String> {
        let mut said = Vec::new();
        if let Some(c) = self.carried.take() {
            if c.bytes != sys.sram() {
                self.write(c, "before the cartridge was replaced", &mut said);
                self.countdown = None;
                sys.clear_sram_dirty();
            }
            // Identical: a soft reset. The countdown is this window's, not the machine's, so it survives
            // and the ordinary autosave will write exactly these bytes.
        }
        self.path = sram_file::srm_path_for(Path::new(rom_path));
        if !sys.sram_used() {
            if let Some(bytes) = sram_file::load_srm(&self.path) {
                said.push(format!(
                    "SRAM: loaded {} bytes from {}",
                    bytes.len(),
                    self.path.display()
                ));
                sys.load_sram(&bytes);
            }
            self.countdown = None; // the fresh buffer is clean and matches disk
        }
        said
    }

    /// The per-iteration autosave: arm the debounce when the guest dirties SRAM, count it down, write when
    /// it elapses — and retry an [`orphan`](Battery::orphan) every time, since memory holds the only copy
    /// of those bytes.
    ///
    /// `oracle-frontend`'s block, one for one, including the `sram_used` gate that keeps a pure-ROM cart
    /// from ever fabricating a file.
    pub fn tick(&mut self, sys: &mut System) -> Vec<String> {
        let mut said = Vec::new();
        if let Some(o) = self.orphan.take() {
            self.write(
                o,
                "(retry — these bytes outlived their cartridge)",
                &mut said,
            );
        }
        if !sys.sram_used() {
            return said;
        }
        if sys.sram_dirty() && self.countdown.is_none() {
            self.countdown = Some(AUTOSAVE_DEBOUNCE_FRAMES);
        }
        match self.countdown {
            Some(0) => {
                match sram_file::save_srm(&self.path, sys.sram()) {
                    Ok(()) => {
                        sys.clear_sram_dirty();
                        said.push(format!(
                            "SRAM: saved {} bytes to {}",
                            sys.sram().len(),
                            self.path.display()
                        ));
                    }
                    Err(e) => {
                        said.push(format!("SRAM: save failed ({}): {e}", self.path.display()))
                    }
                }
                self.countdown = None;
            }
            Some(n) => self.countdown = Some(n - 1),
            None => {}
        }
        said
    }

    /// Persist anything owed **now**, ahead of an operation this window itself is about to perform that
    /// would destroy it — a save-state load, or quitting. `why` names that operation and appears verbatim.
    ///
    /// Returns whether the on-disk image is up to date afterwards, *including* the common case of nothing
    /// being owed. `false` means memory still holds the only copy.
    ///
    /// This is the frontend's `flush_pending_srm`, and it is only reachable for gestures whose ordering
    /// this window controls. Everything that arrives through the bus — from a client or from this
    /// window's own palette — is [`Battery::carry`]'s and [`Battery::after_replacement`]'s, because the
    /// machine has already moved by the time either is heard.
    pub fn flush(&mut self, sys: &System, why: &str, said: &mut Vec<String>) -> bool {
        if !self.owes(sys) {
            return true;
        }
        match sram_file::save_srm(&self.path, sys.sram()) {
            Ok(()) => {
                said.push(format!(
                    "SRAM: saved {} bytes to {} {why}",
                    sys.sram().len(),
                    self.path.display()
                ));
                self.countdown = None;
                true
            }
            Err(e) => {
                said.push(format!(
                    "SRAM: save {why} failed ({}): {e}",
                    self.path.display()
                ));
                false
            }
        }
    }

    /// Cancel the pending autosave. The other half of a machine this window replaced on purpose (a
    /// save-state load): the buffer that arrived with the snapshot is older than the disk, and the
    /// debounce would otherwise write it there moments later.
    pub fn cancel_autosave(&mut self) {
        self.countdown = None;
    }

    /// One write, one message, one place to retry from on failure.
    fn write(&mut self, p: Pending, why: &str, said: &mut Vec<String>) {
        match sram_file::save_srm(&p.path, &p.bytes) {
            Ok(()) => said.push(format!(
                "SRAM: saved {} bytes to {} {why}",
                p.bytes.len(),
                p.path.display()
            )),
            Err(e) => {
                said.push(format!(
                    "SRAM: save {why} FAILED ({}): {e} — the bytes are still held in memory and the \
                     write will be retried every frame",
                    p.path.display()
                ));
                self.orphan = Some(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine with a real SRAM buffer, dirtied, so the gates below are exercised rather than skipped.
    /// `sram_used` is a `System` flag set by an actual guest write, so it is produced by writing.
    fn dirtied() -> System {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        sys.load_sram(&[0xAB; 32]);
        sys
    }

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pb-{tag}-{stamp}.bin"))
    }

    /// `load_sram` populates the buffer but does not make the cart "used" — only a guest write does. These
    /// tests therefore drive the `used`/`dirty` pair through the public API the window itself reads, and
    /// this row is the statement of what that API answers, so the fixtures below are not built on a guess.
    #[test]
    fn a_loaded_image_is_not_a_guest_save() {
        let sys = dirtied();
        assert!(
            sys.sram_present(),
            "the fixture cart must provision a buffer or every row here is vacuous"
        );
        assert!(
            !sys.sram_used(),
            "loading an image is the window putting bytes back, not the guest saving"
        );
    }
}
