//! The docked layout: `egui_dock` with the game screen as a tab alongside the debug panels.
//!
//! Parcel 1 delivered the shell — a `DockState` the user can drag, tab, split and close, with the emulator
//! picture living inside it as one tab among others — plus [`Tab::Pacing`], which shows that parcel's own
//! subject (governor rebases, ring occupancy, starvations, drops) live, so a wobble is visible while it is
//! happening rather than only in a report afterwards.
//!
//! **Parcel 2a makes [`Tab::Registers`] real.** It was a placeholder shaped like a register panel, and it
//! carried a live defect: nineteen values where `emulator/registers` serves twenty-one keys, the two
//! missing ones being the active `A7` and `SP` — the two a human debugging a 68000 actually wants. See
//! `docs/2026-09-03-debug-panels-design.md` §9.3.
//!
//! The panel and the bus method are held together by construction rather than by intention: the rows come
//! from [`register_rows`], a pure function the egui body merely loops over, and `mod bus_parity` below
//! compares those rows against what `Engine::registers` answers **through `Host::call`** — the in-process
//! read of the same method registry that contract D15 says an in-process GUI is. If the two ever drift,
//! that test is what says so.
//!
//! Memory, Objects and the symbol-table port are parcels 2b/2c; the transport bar (step / run / pause /
//! reset) needs the player's pause flag mirrored onto the bus and is parcel 3.

use crate::machine::Machine;
use crate::pacing::Governor;

/// A docked tab.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tab {
    /// The emulator picture. The one tab that carries the uploaded texture.
    Screen,
    /// Live pacing state — this parcel's subject, visible while it happens.
    Pacing,
    /// The 68000 register file and the cheap half of `emulator/status`, in one tab. Nine key/values in a
    /// tab of their own beside a tab holding the same pc/sp/sr is two panels waiting to disagree.
    Registers,
}

/// Everything the tab bodies read. Held apart from the `DockState` so both can be borrowed at once.
pub struct Panels<'a> {
    pub tex: Option<&'a egui::TextureHandle>,
    pub machine: &'a Machine,
    pub governor: &'a Governor,
    pub status: &'a str,
    /// The `--rom` argument, verbatim — see [`StatusStrip::rom_path`] for why it is the player's own
    /// string and not the bus's absolutised `romPath`.
    pub rom_path: &'a str,
}

impl egui_dock::TabViewer for Panels<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(match tab {
            Tab::Screen => "screen",
            Tab::Pacing => "pacing",
            Tab::Registers => "registers",
        })
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            Tab::Screen => "Screen",
            Tab::Pacing => "Pacing",
            Tab::Registers => "Registers",
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Screen => self.screen(ui),
            Tab::Pacing => self.pacing(ui),
            Tab::Registers => self.registers(ui),
        }
    }
}

impl Panels<'_> {
    fn screen(&self, ui: &mut egui::Ui) {
        let Some(tex) = self.tex else {
            ui.centered_and_justified(|ui| ui.label("no frame yet"));
            return;
        };
        // Aspect-fit, the job `crates/oracle-frontend/src/present.rs::dest_rect` does today. Nearest
        // sampling, because a Genesis pixel is a Genesis pixel.
        let avail = ui.available_size();
        let src = tex.size_vec2();
        let scale = (avail.x / src.x).min(avail.y / src.y).max(0.01);
        ui.centered_and_justified(|ui| {
            ui.add(egui::Image::new(tex).fit_to_exact_size(src * scale));
        });
    }

    fn pacing(&self, ui: &mut egui::Ui) {
        ui.monospace(format!("frames emulated   {}", self.machine.frames()));
        ui.monospace(format!("pictures drawn    {}", self.machine.pictures()));
        ui.separator();
        ui.monospace(format!(
            "governor rebases  {}   <- stalls of a whole frame or more",
            self.governor.rebases()
        ));
        ui.monospace(format!("early wakes       {}", self.governor.early_wakes()));
        ui.monospace(format!(
            "worst late        {:.2} ms",
            self.governor.worst_late().as_secs_f64() * 1000.0
        ));
        ui.separator();
        match self.machine.device() {
            Some(d) => {
                use ringbuf::traits::Observer;
                use std::sync::atomic::Ordering;
                let c = d.counters();
                let occ = d.prod().occupied_len();
                ui.monospace(format!(
                    "device            {} Hz / {} ch",
                    d.rate(),
                    d.channels()
                ));
                ui.monospace(format!(
                    "ring              {occ} / {} samples ({:.1} ms)",
                    d.ring_capacity(),
                    occ as f64 * 500.0 / d.rate() as f64
                ));
                ui.monospace(format!(
                    "starved (steady)  {}",
                    c.starved_steady.load(Ordering::Relaxed)
                ));
                ui.monospace(format!("producer drops    {}", d.dropped()));
            }
            None => {
                ui.monospace("device            NONE — pacing is unmeasured, not fine");
            }
        }
        ui.separator();
        ui.monospace(self.status);
    }

    fn registers(&self, ui: &mut egui::Ui) {
        for (label, value) in StatusStrip::of(self.machine, self.rom_path).rows() {
            ui.monospace(format!("{label:<18}{value}"));
        }
        ui.separator();
        egui::Grid::new("regs").num_columns(2).show(ui, |ui| {
            for row in register_rows(self.machine.cpu_regs()) {
                ui.monospace(row.label);
                ui.monospace(row.hex());
                ui.end_row();
            }
        });
        ui.separator();
        // Said out loud, because a panel that silently shows one number twice is a new wrong answer.
        ui.small(
            "A7 and SP are one register: the stack pointer the CPU is using right now — SSP in \
             supervisor mode, USP in user. USP and SSP below it are the two storage slots, both shown \
             whichever mode the machine is in.",
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// The Registers derivation — one function, two consumers (design §4.4 R1)
// ---------------------------------------------------------------------------------------------------

/// One row of the Registers panel: the label a human reads, the value, and **the `emulator/registers`
/// keys this row accounts for**.
///
/// `keys` is the row's own claim about what it shows, and it exists so the parity test can check the
/// panel against the *handler's* reply rather than against a hand-written list of names. A list of names
/// keeps passing after someone adds `a8`; a claim checked against the reply does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegRow {
    pub label: &'static str,
    pub keys: &'static [&'static str],
    pub value: u32,
    /// Hex digits to print: 8 for a 32-bit register, 4 for SR.
    pub digits: usize,
}

impl RegRow {
    pub fn hex(&self) -> String {
        format!("{:0width$X}", self.value, width = self.digits)
    }
}

/// The rows the Registers panel shows, derived from the 68000 file.
///
/// Pure, and separated from the drawing for one reason: it is the half a parity test can call. The egui
/// body above is a `for` loop over this and holds no derivation of its own.
pub fn register_rows(r: &oracle_core::m68000::Registers) -> Vec<RegRow> {
    let mut rows = Vec::with_capacity(20);
    const D: [&str; 8] = ["D0", "D1", "D2", "D3", "D4", "D5", "D6", "D7"];
    const DK: [[&str; 1]; 8] = [
        ["d0"],
        ["d1"],
        ["d2"],
        ["d3"],
        ["d4"],
        ["d5"],
        ["d6"],
        ["d7"],
    ];
    for i in 0..8 {
        rows.push(RegRow {
            label: D[i],
            keys: &DK[i],
            value: r.d[i],
            digits: 8,
        });
    }
    const A: [&str; 7] = ["A0", "A1", "A2", "A3", "A4", "A5", "A6"];
    const AK: [[&str; 1]; 7] = [["a0"], ["a1"], ["a2"], ["a3"], ["a4"], ["a5"], ["a6"]];
    for i in 0..7 {
        rows.push(RegRow {
            label: A[i],
            keys: &AK[i],
            value: r.a[i],
            digits: 8,
        });
    }
    // **One row, two keys.** `emulator/registers` serves twenty-one keys carrying twenty distinct values:
    // `a7` is `Registers::addr_reg(7)`, `sp` is `Registers::a7()`, and `addr_reg(7)` *is* `a7()`. Two rows
    // showing the same number under two unrelated names would be a new believable wrong answer — a reader
    // would take them for two registers that happen to agree — so the label names both and the row claims
    // both keys. Parcel 1's panel omitted this register entirely on the grounds that "A7 lives in usp/ssp
    // on this core": true about the storage, and `addr_reg(7)` is exactly the accessor that resolves it.
    rows.push(RegRow {
        label: "A7 = SP",
        keys: &["a7", "sp"],
        value: r.addr_reg(7),
        digits: 8,
    });
    rows.push(RegRow {
        label: "USP",
        keys: &["usp"],
        value: r.usp,
        digits: 8,
    });
    rows.push(RegRow {
        label: "SSP",
        keys: &["ssp"],
        value: r.ssp,
        digits: 8,
    });
    rows.push(RegRow {
        label: "PC",
        keys: &["pc"],
        value: r.pc,
        digits: 8,
    });
    rows.push(RegRow {
        label: "SR",
        keys: &["sr"],
        value: u32::from(r.sr),
        digits: 4,
    });
    rows
}

// ---------------------------------------------------------------------------------------------------
// The status strip — the cheap half of `emulator/status`
// ---------------------------------------------------------------------------------------------------

/// The header strip above the register grid: what cartridge is loaded and where the machine is in time.
///
/// Deliberately **not** the whole of `emulator/status`. `symbolAtPc` / `symbolCount` need a symbol table
/// and this player has none — no `--symbols`, no auto-discovery — so the strip says *"none loaded"* in
/// words rather than showing a `0` that a reader would take for "this ROM has no symbols".
pub struct StatusStrip {
    /// The path as the human gave it on `--rom`. **Not** the bus's `romPath`, which `Engine::set_rom_path`
    /// absolutises through a private helper; the two agree only when the argument was already absolute,
    /// so this field is the player's own and is not claimed to be the served string.
    pub rom_path: String,
    /// Bytes of cartridge **as the machine holds them** (`System::rom().len()`) — the identical derivation
    /// `emulator/status`'s `romBytes` uses, so the two cannot drift.
    pub rom_bytes: usize,
    /// The **emulated** frame index, `mclk / MCLK_PER_FRAME` — the identical derivation behind the bus's
    /// `frameToken`.
    pub frame: u64,
    /// Frames the player's own loop has run. This is a *different number* from [`frame`](Self::frame) and
    /// is labelled as one: `engine.rs`'s `status` comment names the UI-counter-versus-emulated-index
    /// confusion (`F-WINDOW-BUS-FRAME-OFFBYONE`) as something that has already cost this suite three
    /// hand-rolled realignments. Showing both, named apart, is the cheap way not to repeat it.
    pub frames_run: u64,
}

impl StatusStrip {
    /// Derived from the machine, by the same expressions `Engine::status` uses. One derivation, two
    /// consumers.
    pub fn of(machine: &Machine, rom_path: &str) -> Self {
        let sys = machine.system();
        Self {
            rom_path: rom_path.to_string(),
            rom_bytes: sys.rom().len(),
            frame: sys.scheduler().now() / oracle_core::system::MCLK_PER_FRAME,
            frames_run: machine.frames(),
        }
    }

    /// The strip as label/value pairs, in display order.
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        vec![
            ("rom", self.rom_path.clone()),
            ("rom bytes", format!("{}", self.rom_bytes)),
            ("frame (emulated)", format!("{}", self.frame)),
            ("frames run (player)", format!("{}", self.frames_run)),
            // Never a `0` and never blank: this player has no symbol table at all, and a count of zero
            // reads as "this ROM has no symbols" rather than "nothing was loaded".
            ("symbols", "none loaded".into()),
        ]
    }
}

/// The starting layout: the screen on the left, Pacing over Registers on the right.
///
/// `egui_dock::DockState` derives `Serialize`/`Deserialize` under the crate's `serde` feature, so
/// remembering a user's layout across runs is one feature flag and a `Serialize` bound on [`Tab`]. Still
/// deliberately **not** done: `DockState<Tab>` serializes the `Tab` values themselves, and a plain
/// externally-tagged enum errors on an unknown variant — so a layout saved before the enum stops moving
/// does not lose one tab, it fails to deserialize and the user loses the whole layout. It turns on at the
/// end of the parcel that removes the last placeholder tab (design §6), which parcel 2a is not.
pub fn initial_dock() -> egui_dock::DockState<Tab> {
    let mut dock = egui_dock::DockState::new(vec![Tab::Screen]);
    let surface = dock.main_surface_mut();
    let [_, right] = surface.split_right(egui_dock::NodeIndex::root(), 0.68, vec![Tab::Pacing]);
    surface.split_below(right, 0.45, vec![Tab::Registers]);
    dock
}

// ---------------------------------------------------------------------------------------------------
// The parity invariant — design §4.4 R3
// ---------------------------------------------------------------------------------------------------

/// **This panel and `emulator/registers` must never disagree**, and the guard lives here rather than in
/// `oracle-aether/tests/` for a structural reason: `oracle-player` is the crate that can see both sides.
///
/// It reaches the handler through `oracle_aether::host::Host::call` — the synchronous, in-process read of
/// the same method registry that contract D15 says an in-process GUI *is* ("a consumer of the same
/// registry, not a second server"). Not a socket, not `pump`, and not `Engine::dispatch` reached around
/// the `Host`: routing it through `call` is what gives that entry point a real consumer inside this
/// parcel instead of shipping a method nobody invokes.
///
/// **The expected key set is enumerated from the handler's own reply, never written down here.** A
/// hand-written list of twenty-one names goes on passing forever after someone adds `a8`; a set derived
/// from the reply goes red the moment the served surface grows a key the panel does not show. That is the
/// whole point of [`RegRow::keys`].
///
/// `oracle-aether` is `#![cfg(unix)]`, so this module is too.
#[cfg(all(test, unix))]
mod bus_parity {
    use super::*;
    use oracle_aether::host::{Host, HostConfig};
    use oracle_core::system::System;
    use serde_json::{json, Map, Value};

    fn booted() -> System {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        sys
    }

    /// `"0x0000B000"` → `0xB000`. The bus spells values as hex strings (D9 category 1); the panel carries
    /// them as numbers, so the comparison has to cross that boundary explicitly rather than by matching
    /// two strings and calling it agreement.
    fn hex_of(field: &str, v: &Value) -> u32 {
        let s = v
            .as_str()
            .unwrap_or_else(|| panic!("`{field}` should be a hex string (D9), got {v}"));
        u32::from_str_radix(s.trim_start_matches("0x"), 16)
            .unwrap_or_else(|e| panic!("`{field}` = {s:?}: {e}"))
    }

    /// `emulator/registers`, answered in-process through `Host::call`.
    fn served(h: &mut Host, sys: &mut System) -> Map<String, Value> {
        let (result, stamp) = h.call(sys, "emulator/registers", &json!({}));
        // The stamp is checked, not ignored: a `call` that failed to swap the machine in would answer
        // `mclk 0` off the placeholder, and every value below would then be a placeholder's zero agreeing
        // with a panel reading the real machine — a green run proving nothing.
        assert_eq!(
            stamp["mclk"],
            json!(sys.scheduler().now()),
            "the call answered for the placeholder machine, not this one"
        );
        match result.expect("emulator/registers answers") {
            Value::Object(m) => m,
            other => panic!("emulator/registers must answer an object, got {other}"),
        }
    }

    /// The panel's rows checked against one served reply: every key the handler serves is claimed by
    /// exactly one row and carries that row's value, and no row claims a key the handler does not serve.
    fn assert_parity(sys: &mut System, h: &mut Host, what: &str) {
        let reply = served(h, sys);
        let rows = register_rows(sys.cpu_regs());

        // Every key the *handler* served — enumerated from the reply, never listed here.
        for (key, value) in &reply {
            let claimants: Vec<&RegRow> = rows
                .iter()
                .filter(|r| r.keys.contains(&key.as_str()))
                .collect();
            assert_eq!(
                claimants.len(),
                1,
                "{what}: `emulator/registers` serves `{key}` and {} panel rows show it. The panel is \
                 MISSING a register the tool answers (or shows it twice); rows = {:?}",
                claimants.len(),
                rows.iter().map(|r| r.label).collect::<Vec<_>>()
            );
            assert_eq!(
                claimants[0].value,
                hex_of(key, value),
                "{what}: `{key}` — the panel's `{}` row shows 0x{} and the bus says {value}: the two \
                 have DRIFTED",
                claimants[0].label,
                claimants[0].hex()
            );
        }

        // …and nothing the other way: a row claiming a key the handler does not serve is a panel
        // inventing a register, which reads exactly as convincingly as a real one.
        for row in &rows {
            for key in row.keys {
                assert!(
                    reply.contains_key(*key),
                    "{what}: the `{}` row claims `{key}`, which `emulator/registers` does not serve",
                    row.label
                );
            }
        }
    }

    /// The parity check over the machine as it actually runs — boot, and three points into the fixture
    /// ROM, so the registers under comparison are real values rather than a reset's zeros.
    #[test]
    fn the_panel_shows_every_register_the_bus_serves_and_the_same_value() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();

        assert_parity(&mut sys, &mut h, "at reset");
        for frames in [1u64, 7, 30] {
            sys.run_frames(frames);
            assert_parity(&mut sys, &mut h, &format!("after {frames} more frames"));
        }

        // State the coverage this fixture does NOT have rather than leaving it to look covered. `System`
        // exposes no mutable register accessor, so a user-mode machine cannot be built here; the A7
        // selection rule is pinned by `a7_row_follows_the_supervisor_bit` below, over a hand-built
        // `Registers`, and both paths go through the same `Registers::addr_reg(7)` the handler calls.
        assert!(
            sys.cpu_regs().supervisor(),
            "the fixture ROM left supervisor mode — the note above is now stale and the user-mode A7 \
             case may be reachable from here after all"
        );
    }

    /// **`sp` and `a7` are one register on this core**, and the panel says so with one row rather than
    /// printing the same number twice under two unrelated names.
    ///
    /// The handler serves 21 keys carrying 20 distinct values: `a7` is `Registers::addr_reg(7)` and `sp`
    /// is `Registers::a7()`, and `addr_reg(7)` *is* `a7()`. A panel that showed them as two rows would be
    /// a new believable wrong answer — a reader would take them for two registers that happen to agree.
    #[test]
    fn the_shared_a7_sp_row_carries_both_keys_and_says_so() {
        let sys = booted();
        let rows = register_rows(sys.cpu_regs());
        let shared: Vec<&RegRow> = rows.iter().filter(|r| r.keys.len() > 1).collect();
        assert_eq!(shared.len(), 1, "exactly one row carries more than one key");
        let mut keys = shared[0].keys.to_vec();
        keys.sort_unstable();
        assert_eq!(keys, ["a7", "sp"]);
        assert!(
            shared[0].label.contains("A7") && shared[0].label.contains("SP"),
            "the label must name both, or a reader cannot tell it is one register: {:?}",
            shared[0].label
        );
        assert_eq!(shared[0].value, sys.cpu_regs().a7());
    }

    /// The A7 row follows the supervisor bit — SSP in supervisor mode, USP in user mode — which is the
    /// half the running fixture above cannot reach.
    #[test]
    fn a7_row_follows_the_supervisor_bit() {
        let mut r = booted().cpu_regs().clone();
        r.ssp = 0x00FF_1000;
        r.usp = 0x00FF_2000;
        let a7 = |rows: &[RegRow]| {
            rows.iter()
                .find(|x| x.keys.contains(&"a7"))
                .expect("an a7 row")
                .value
        };

        r.sr |= 0x2000; // S set — supervisor
        let sup = register_rows(&r);
        assert_eq!(a7(&sup), 0x00FF_1000, "supervisor A7 is SSP");

        r.sr &= !0x2000; // S clear — user
        let usr = register_rows(&r);
        assert_eq!(a7(&usr), 0x00FF_2000, "user A7 is USP");

        // And the USP/SSP rows keep showing the storage regardless of mode, so all three are visible.
        for rows in [&sup, &usr] {
            let pick = |k: &str| {
                rows.iter()
                    .find(|x| x.keys.contains(&k))
                    .expect("a row")
                    .value
            };
            assert_eq!(pick("usp"), 0x00FF_2000);
            assert_eq!(pick("ssp"), 0x00FF_1000);
        }
    }

    /// The status strip's two derivable fields against `emulator/status`, through the same `Host::call`.
    ///
    /// `romPath` is deliberately **not** compared: `Engine::set_rom_path` absolutises through a private
    /// helper, so the served string and the `--rom` argument agree only when the argument was already
    /// absolute. The strip shows the player's own argument and says so; claiming parity on it would be
    /// claiming a normalisation the panel does not perform.
    #[test]
    fn the_status_strip_agrees_with_emulator_status_on_what_it_can_derive() {
        let mut h = Host::new(HostConfig::default());
        let mut sys = booted();
        sys.run_frames(5);

        let (result, _) = h.call(&mut sys, "emulator/status", &json!({}));
        let reply = result.expect("emulator/status answers");

        assert_eq!(
            reply["romBytes"].as_u64().expect("romBytes is a count"),
            sys.rom().len() as u64,
            "the strip's `rom bytes` and the bus's `romBytes` are one expression"
        );
        assert_eq!(
            reply["frameToken"].as_u64().expect("frameToken is a count"),
            sys.scheduler().now() / oracle_core::system::MCLK_PER_FRAME,
            "the strip's `frame (emulated)` and the bus's `frameToken` are one expression"
        );
        // The fixture must actually have moved, or both sides agreeing on 0 proves nothing.
        assert!(reply["frameToken"].as_u64().unwrap() > 0, "the fixture ran");

        // And the honest half: no symbol table exists in this player, so the strip must say so in words.
        assert_eq!(
            reply["symbolCount"],
            json!(0),
            "the bus counts zero symbols, which is exactly the `0` the strip must not show a human"
        );
        let strip_symbols = StatusStrip {
            rom_path: "/tmp/x.bin".into(),
            rom_bytes: 0,
            frame: 0,
            frames_run: 0,
        }
        .rows()
        .into_iter()
        .find(|(k, _)| *k == "symbols")
        .expect("the strip has a symbols row")
        .1;
        assert_eq!(strip_symbols, "none loaded");
    }
}
