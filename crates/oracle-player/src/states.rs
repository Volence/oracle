//! **The ten numbered save-state slots**, on disk beside the ROM — this window's half of
//! [`oracle_frontend::save_state`].
//!
//! The container is not written here and must not be: magic, the container version, the derived
//! machine-layout fingerprint, the ROM fingerprint and the payload checksum all live in
//! [`oracle_frontend::save_state`], which this crate links as of migration S0/S3. Both windows therefore
//! write **one** file format, and a state written from the minifb window loads in this one. A second
//! container here would have been the drift the lib target exists to prevent, and it would have been
//! invisible until somebody tried to load the other window's file.
//!
//! # ⚑ What a state load is, and why it does not go through the bus
//!
//! `emulator/restore` exists and is served, and it is **not** this. That method restores an in-memory
//! checkpoint — volatile, server-assigned id, gone when the process ends. These are files, named by the
//! same rule as the `.srm` (`…/foo.bin` slot 3 → `…/foo.state3`), and they survive a relaunch. They are
//! the feature the person at the window has, and there is no served method that means it.
//!
//! So a load replaces the window's own [`System`] directly, which has three consequences and all three are
//! handled rather than hoped about:
//!
//! * **The battery is flushed first.** A snapshot carries the cartridge SRAM backwards with it, so an
//!   in-game save a second old would be destroyed. This is one of the two gestures whose ordering this
//!   window *does* control (the other is quitting), so unlike a bus-driven swap it can obey
//!   `oracle-frontend`'s rule literally — [`Battery::flush`](crate::battery::Battery::flush), before
//!   anything is swapped.
//! * **The autosave is cancelled and the dirty flag cleared afterwards**, so the rolled-back buffer
//!   reaches disk only once the game actually saves again. Without it the debounce would fire moments
//!   later and overwrite the `.srm` that was just flushed with the *older* restored contents.
//! * **The timeline is resynchronised**, because the master clock just moved backwards:
//!   [`Machine::adopt_system`](crate::machine::Machine::adopt_system) does it in the same statement that
//!   takes the machine, so there is no order in which one can happen without the other.
//!
//! # ⚑ What a connected client is NOT told, stated rather than hidden
//!
//! A load replaces the machine without moving the engine's `rom_generation`, so a client attached to this
//! window over the socket gets no `emulator/romReloaded` and no `rom_changed` — it sees the clock jump in
//! the next stamp and nothing else. `oracle-frontend`'s F4 has exactly the same hole and always has.
//! Closing it would mean a new signal on a contract lane's surface, which is a change request and not a
//! slice. It is recorded here so the next person does not have to rediscover it at a debugger.

use std::path::Path;

use oracle_core::system::System;
use oracle_frontend::save_state::{self, SLOT_COUNT};

use crate::battery::Battery;
use crate::machine::Machine;

/// A line for a human, and whether it is a refusal — the [`crate::ui::Echo`] shape, for the same reason:
/// a surface colours on this field and never on the shape of the string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    pub refused: bool,
}

/// The slot state: which one the controls act on, what the cartridge's fingerprint is, and which slots
/// have a file.
pub struct States {
    /// `0..SLOT_COUNT`. The frontend's `state_slot`, same wrap, same default.
    slot: usize,
    /// **The identity of the cartridge in the machine**, re-derived whenever it is replaced. This is what
    /// makes a state written against a previous build refuse to load instead of quietly putting the
    /// previous build's ROM back — a snapshot carries the cartridge with it.
    rom_fp: u64,
    /// Which slots have a file, probed at open and after a cartridge swap, and updated by a save. A
    /// cache of the filesystem, so it is only ever *shown*; nothing is gated on it.
    on_disk: [bool; SLOT_COUNT],
    /// The path the slot files are keyed to. Re-derived with the cartridge.
    rom_path: String,
    /// The last thing a save or a load had to say. Standing, so it does not have to be caught as it goes
    /// past.
    last: Option<Note>,
}

impl States {
    pub fn open(rom_path: &str, sys: &System) -> Self {
        let mut s = States {
            slot: 0,
            rom_fp: 0,
            on_disk: [false; SLOT_COUNT],
            rom_path: rom_path.to_string(),
            last: None,
        };
        s.rekey(rom_path, sys);
        s
    }

    /// The launch line: how many slots, where they go, and which are occupied.
    pub fn announcement(&self) -> String {
        format!(
            "states: {SLOT_COUNT} slots, {} occupied — F2 saves, F4 loads, F6/F7 pick (slot {} now, e.g. {})",
            self.on_disk.iter().filter(|b| **b).count(),
            self.slot,
            save_state::state_path_for(Path::new(&self.rom_path), self.slot).display()
        )
    }

    /// **Re-key to the cartridge now in the machine.** Called whenever `rom_changed` fired, for the
    /// `.srm`'s reason one field over: a slot file still keyed to the outgoing image would let a state
    /// written for one game restore over another.
    pub fn after_replacement(&mut self, rom_path: &str, sys: &System) {
        self.rekey(rom_path, sys);
    }

    fn rekey(&mut self, rom_path: &str, sys: &System) {
        self.rom_path = rom_path.to_string();
        self.rom_fp = save_state::rom_fingerprint(sys.rom());
        for (slot, occupied) in self.on_disk.iter_mut().enumerate() {
            *occupied = save_state::state_path_for(Path::new(rom_path), slot).exists();
        }
    }

    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn occupied(&self, slot: usize) -> bool {
        self.on_disk.get(slot).copied().unwrap_or(false)
    }

    pub fn last(&self) -> Option<&Note> {
        self.last.as_ref()
    }

    /// Select a slot directly. Out-of-range is ignored rather than clamped: a caller that computed one is
    /// wrong, and clamping would hide it behind a save to slot 9.
    pub fn select(&mut self, slot: usize) {
        if slot < SLOT_COUNT {
            self.slot = slot;
        }
    }

    /// Step the selection, wrapping over `0..SLOT_COUNT`. `delta` is `-1` / `+1` from the two keys.
    pub fn step(&mut self, delta: isize) {
        let n = SLOT_COUNT as isize;
        self.slot = (self.slot as isize + delta).rem_euclid(n) as usize;
    }

    /// Write the machine to the selected slot.
    ///
    /// No battery interaction at all, and that is not an omission: a save **reads** the machine and
    /// changes nothing about it, so there is nothing the `.srm` could lose. The SRAM rides into the file
    /// with everything else, which is what makes the load's flush necessary and this one's absence
    /// correct.
    pub fn save(&mut self, machine: &Machine) -> &Note {
        let path = save_state::state_path_for(Path::new(&self.rom_path), self.slot);
        let note = match save_state::save(&path, machine.system(), self.rom_fp) {
            Ok(n) => {
                self.on_disk[self.slot] = true;
                Note {
                    text: format!(
                        "state: saved {n} bytes to slot {} ({})",
                        self.slot,
                        path.display()
                    ),
                    refused: false,
                }
            }
            Err(e) => Note {
                text: format!("state: save to slot {} failed: {e}", self.slot),
                refused: true,
            },
        };
        self.last = Some(note);
        self.last.as_ref().expect("just set")
    }

    /// **Restore the machine from the selected slot**, with everything that has to happen around it.
    ///
    /// The refusal comes first, and it is a *clean* one:
    /// [`save_state::load`](oracle_frontend::save_state::load) is a static constructor returning a whole
    /// `System` or an error, so a stale, corrupt or foreign-cartridge file leaves the running machine
    /// untouched — there is no window in which a half-restored machine is on screen. Nothing below the
    /// `?` runs unless a complete machine exists.
    ///
    /// Then, in this order and for the reasons in the module doc: flush the battery, take the machine
    /// (which resynchronises the timeline in the same statement), cancel the autosave, clear the restored
    /// dirty flag.
    pub fn load(&mut self, machine: &mut Machine, battery: &mut Battery, said: &mut Vec<String>) {
        let path = save_state::state_path_for(Path::new(&self.rom_path), self.slot);
        let loaded = match save_state::load(&path, self.rom_fp) {
            Ok(sys) => sys,
            Err(e) => {
                self.last = Some(Note {
                    text: format!("state: load of slot {} failed: {e}", self.slot),
                    refused: true,
                });
                return;
            }
        };
        // Before the swap, because after it the bytes are gone. A failed flush is reported and the load
        // still proceeds — the restored buffer is a *valid older* save and the person explicitly asked to
        // go back to it, which is `oracle-frontend`'s ruling on the same fork.
        battery.flush(machine.system(), "before the state load", said);
        machine.adopt_system(loaded);
        // …and the other half. (1) cancel the pending autosave, which would otherwise fire moments later
        // and overwrite the `.srm` just flushed with the *older* restored contents; (2) clear the
        // restored dirty flag, so the rolled-back SRAM reaches disk only once the game saves again.
        battery.cancel_autosave();
        machine.system_mut().clear_sram_dirty();
        self.last = Some(Note {
            text: format!("state: loaded slot {} from {}", self.slot, path.display()),
            refused: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::m68000::bus68k::Bus68k;
    use std::path::PathBuf;

    fn booted() -> Machine {
        Machine::new(oracle_core::testrom::build(), None)
    }

    /// A private directory with one cartridge in it, removed when the row ends.
    struct Cartridge {
        dir: PathBuf,
        rom: PathBuf,
    }

    impl Cartridge {
        fn new(tag: &str) -> Self {
            let stamp = format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let dir = std::env::temp_dir().join(format!("oracle-states-{tag}-{stamp}"));
            std::fs::create_dir_all(&dir).expect("make the fixture directory");
            let rom = dir.join("game.bin");
            std::fs::write(&rom, oracle_core::testrom::build()).expect("write the cartridge");
            Cartridge { dir, rom }
        }

        fn path(&self) -> String {
            self.rom.display().to_string()
        }
    }

    impl Drop for Cartridge {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// The slot selection wraps over `0..SLOT_COUNT` in both directions, derived from the container's own
    /// constant rather than from a literal 10 — a slot count that changed and a wrap that did not is a
    /// save that silently goes to the wrong file.
    #[test]
    fn the_slot_selection_wraps_over_the_constant_in_both_directions() {
        let m = booted();
        let mut s = States::open("/nonexistent/rom.bin", m.system());
        assert_eq!(s.slot(), 0);
        s.step(-1);
        assert_eq!(
            s.slot(),
            SLOT_COUNT - 1,
            "down from 0 wraps to the last slot"
        );
        s.step(1);
        assert_eq!(s.slot(), 0, "and back");
        s.select(SLOT_COUNT);
        assert_eq!(
            s.slot(),
            0,
            "an out-of-range selection is ignored, not clamped onto the last slot"
        );
    }

    /// ★ **A slot round-trips the whole machine, and the load flushes the battery first.**
    ///
    /// Two assertions in one row because they are one gesture and their ORDER is the thing that can be
    /// wrong: a snapshot carries the cartridge SRAM backwards with it, so a load that did not flush first
    /// would destroy an in-game save that happened to be a second old — which is `oracle-frontend`'s
    /// stated reason for the same line, and which is invisible unless the battery is *pending* at the
    /// moment of the load. It is made pending here on purpose.
    ///
    /// The machine assertion is on `System::state_hash`, and the two states are asserted **different**
    /// before the load: "the machine matches the snapshot" witnesses nothing if it never left it.
    ///
    /// Mutation proven red: delete `battery.flush(...)` from `States::load`.
    ///
    /// ⚠ **And one that is NOT, measured and recorded rather than claimed.** Deleting
    /// `self.resync_after_replacement()` from [`Machine::adopt_system`] leaves this row — and the whole
    /// suite — **green**. The `capture_lines() == 0` assertion below is therefore not a gate on it: this
    /// fixture drives `System::run_frames` directly, which never feeds `Machine::cap` at all, so the count
    /// was already zero. Making it non-zero needs the only thing that ends a run mid-frame — a breakpoint
    /// — and both fixture addresses tried (`$00020E`, the fixture ROM's hot inner loop, and `$000400`)
    /// halt before the first scanline of a frame completes, so the capture is still empty at the halt. The
    /// audio half is worse: its absence is *indefinite silence*, and observing it needs a real cpal output
    /// device in the suite.
    ///
    /// What stands in a gate's place is structural and is the reason [`Machine::adopt_system`] exists at
    /// all: the assignment and the resync are two statements of one method, and no caller can perform the
    /// first without the second. That is weaker than a red mutation and is written here as weaker.
    /// Tagged for a follow-up that can produce a mid-frame halt cheaply.
    #[test]
    fn a_slot_round_trips_the_machine_and_the_load_flushes_the_battery_first() {
        let cart = Cartridge::new("roundtrip");
        let mut machine = booted();
        let (mut battery, _) = Battery::open(&cart.path(), machine.system_mut());
        let mut states = States::open(&cart.path(), machine.system());

        machine.system_mut().run_frames(2);
        // ⚑ The anchor is the **master clock**, not `state_hash`. Measured, and worth recording: the
        // fixture ROM's outer loop is quiescent enough that `state_hash().combined` is byte-identical
        // after three more frames, so a row anchored on it fails its own anti-vacuity clause — which is
        // exactly what it is for. `mclk` rides the bincode snapshot, so a restore rewinds it, and it is
        // the coordinate every other repair in this window is keyed to.
        let anchor = machine.system().scheduler().now();
        let anchor_hash = machine.system().state_hash().combined;

        // The guest saves AFTER the snapshot, so the bytes on disk afterwards must be the newer ones and
        // not the snapshot's.
        {
            let sys = machine.system_mut();
            sys.mega_bus(&mut ()).write8(0xA1_30F1, 5, 0x01);
            sys.mega_bus(&mut ()).write8(0x20_0001, 5, 0x6D);
            assert!(
                sys.sram_used() && sys.sram_dirty(),
                "the guest write did not latch"
            );
        }
        let note = states.save(&machine).clone();
        assert!(!note.refused, "{}", note.text);
        assert!(states.occupied(0), "the slot is not marked occupied");
        let live = machine.system().sram().to_vec();

        machine.system_mut().run_frames(3);
        assert_ne!(
            machine.system().scheduler().now(),
            anchor,
            "anti-vacuity: the machine must have moved away from the snapshot, or the load below \
             cannot be seen to have brought it back"
        );

        let srm = oracle_frontend::sram_file::srm_path_for(&cart.rom);
        assert!(
            !srm.exists(),
            "the battery reached disk before the load, so the flush below proves nothing"
        );

        let mut said = Vec::new();
        states.load(&mut machine, &mut battery, &mut said);
        let last = states.last().expect("the load said nothing at all");
        assert!(!last.refused, "{}", last.text);
        assert_eq!(
            machine.system().scheduler().now(),
            anchor,
            "the restored machine's clock is not the one that was saved"
        );
        assert_eq!(
            machine.system().state_hash().combined,
            anchor_hash,
            "…and neither is its state"
        );
        assert_eq!(
            machine.capture_lines(),
            0,
            "the scanline capture survived a machine replacement — its lines belong to a timeline \
             that is gone and would be spliced onto the restored machine's first frame"
        );
        let on_disk = std::fs::read(&srm).unwrap_or_else(|e| {
            panic!(
                "the pending battery was never flushed ({e}) — a state load destroyed a real in-game \
                 save that happened to be a second old"
            )
        });
        assert_eq!(
            on_disk, live,
            "the flushed bytes are not the ones the guest wrote"
        );
        assert!(
            !machine.system().sram_dirty(),
            "the restored (older) SRAM is still marked dirty, so the next autosave will write it over \
             the newer image just flushed"
        );
    }

    /// ★ **An empty slot is refused cleanly, and the running machine is untouched.**
    ///
    /// `save_state::load` is a static constructor — a whole `System` or an error — so this is a property
    /// of the type rather than of care taken here. It is asserted anyway, on the machine, because it is
    /// the one thing a person betting a session on a mis-typed slot number needs to be true.
    #[test]
    fn an_empty_slot_is_refused_and_the_machine_keeps_running() {
        let cart = Cartridge::new("empty-slot");
        let mut machine = booted();
        let (mut battery, _) = Battery::open(&cart.path(), machine.system_mut());
        let mut states = States::open(&cart.path(), machine.system());
        machine.system_mut().run_frames(2);
        let before = machine.system().state_hash().combined;

        states.select(7);
        assert!(!states.occupied(7), "the fixture slot must be empty");
        let mut said = Vec::new();
        states.load(&mut machine, &mut battery, &mut said);
        let last = states.last().expect("a refused load said nothing");
        assert!(last.refused, "loading a slot with no file was not refused");
        assert!(
            last.text.contains('7'),
            "the refusal does not say which slot: {}",
            last.text
        );
        assert_eq!(
            machine.system().state_hash().combined,
            before,
            "a refused load moved the machine"
        );
    }

    /// ★ **A state written against a different cartridge is refused**, which is the whole reason
    /// `rom_fp` is re-derived whenever the machine is replaced.
    ///
    /// A snapshot carries the ROM bytes with it, so a load that did not check would silently swap the
    /// running game out from under a window whose `.srm` path, slot files and title still point at the
    /// other one. The fixture changes one byte, because the fingerprint is over the image and one byte is
    /// the smallest thing that must be enough.
    #[test]
    fn a_state_from_another_cartridge_is_refused_after_the_fingerprint_is_re_derived() {
        let cart = Cartridge::new("other-cart");
        let mut machine = booted();
        let (mut battery, _) = Battery::open(&cart.path(), machine.system_mut());
        let mut states = States::open(&cart.path(), machine.system());
        let note = states.save(&machine).clone();
        assert!(!note.refused, "{}", note.text);

        // A different cartridge, by one byte, and the window re-keys to it exactly as `drain` does on
        // `rom_changed`.
        let mut other = oracle_core::testrom::build();
        other[0x180] ^= 0xFF;
        machine.system_mut().load_rom(other);
        states.after_replacement(&cart.path(), machine.system());

        let before = machine.system().state_hash().combined;
        let mut said = Vec::new();
        states.load(&mut machine, &mut battery, &mut said);
        let last = states.last().expect("the load said nothing");
        assert!(
            last.refused,
            "a state belonging to another cartridge was loaded — the running game has been swapped \
             out from under a window still keyed to the other one: {}",
            last.text
        );
        assert_eq!(
            machine.system().state_hash().combined,
            before,
            "a refused load moved the machine"
        );
    }
}
