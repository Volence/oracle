//! The docked layout: `egui_dock` with the game screen as a tab alongside placeholder panels.
//!
//! **These are not the debug panels.** Parcel 1 delivers the shell — a `DockState` the user can drag,
//! tab, split and close, with the emulator picture living inside it as one tab among others — and two
//! placeholders shaped like the panels that will replace them. The real Registers / Memory / Breakpoints /
//! Profiler panels are later parcels, and every one of them is already served by the Aether method table
//! (see `docs/2026-09-02-toolkit-spike.md` §5.1).
//!
//! The one panel that is *not* a placeholder is [`Tab::Pacing`]. It shows this parcel's own subject —
//! governor rebases, ring occupancy, starvations, drops — live, on screen, so a wobble like the spike's is
//! visible while it is happening rather than only in a report afterwards.

use crate::machine::Machine;
use crate::pacing::Governor;

/// A docked tab.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tab {
    /// The emulator picture. The one tab that carries the uploaded texture.
    Screen,
    /// Live pacing state — this parcel's subject, visible while it happens.
    Pacing,
    /// A placeholder shaped like a real register panel: 20 monospace rows rebuilt every frame. It is here
    /// to cost what a real panel costs, so the measurement is not taken against an empty layout.
    Registers,
}

/// Everything the tab bodies read. Held apart from the `DockState` so both can be borrowed at once.
pub struct Panels<'a> {
    pub tex: Option<&'a egui::TextureHandle>,
    pub machine: &'a Machine,
    pub governor: &'a Governor,
    pub status: &'a str,
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
            Tab::Registers => "Registers (placeholder)",
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
        let r = self.machine.cpu_regs();
        egui::Grid::new("regs").num_columns(2).show(ui, |ui| {
            for i in 0..8 {
                ui.monospace(format!("D{i}"));
                ui.monospace(format!("{:08X}", r.d[i]));
                ui.end_row();
            }
            // A0–A6; A7 lives in usp/ssp on this core.
            for i in 0..7 {
                ui.monospace(format!("A{i}"));
                ui.monospace(format!("{:08X}", r.a[i]));
                ui.end_row();
            }
            for (name, v) in [("USP", r.usp), ("SSP", r.ssp), ("PC", r.pc)] {
                ui.monospace(name);
                ui.monospace(format!("{v:08X}"));
                ui.end_row();
            }
            ui.monospace("SR");
            ui.monospace(format!("{:04X}", r.sr));
            ui.end_row();
        });
    }
}

/// The starting layout: the screen on the left, Pacing over Registers on the right.
///
/// `egui_dock::DockState` derives `Serialize`/`Deserialize` under the crate's `serde` feature, so
/// remembering a user's layout across runs is one feature flag and a `Serialize` bound on [`Tab`]. That is
/// deliberately **not** done in parcel 1 — persisting a layout before the real panels exist would persist a
/// layout of placeholders and then have to migrate it. It is a parcel-2 line item, not an unknown.
pub fn initial_dock() -> egui_dock::DockState<Tab> {
    let mut dock = egui_dock::DockState::new(vec![Tab::Screen]);
    let surface = dock.main_surface_mut();
    let [_, right] = surface.split_right(egui_dock::NodeIndex::root(), 0.68, vec![Tab::Pacing]);
    surface.split_below(right, 0.45, vec![Tab::Registers]);
    dock
}
