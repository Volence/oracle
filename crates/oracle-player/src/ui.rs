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
//! **Parcel 2b adds [`Tab::Memory`]**, the symbol table, and the `Host` the running player owns. The
//! Memory panel's model lives in [`crate::memory`] — reads through the *same* functions the five read
//! handlers call, every gesture through `Host::call`, and the paused-write asymmetry reflected rather
//! than smoothed. The status strip below stops saying `symbols  none loaded` and carries `symbolCount`
//! and `symbolAtPc` for real, checked against `emulator/status` by the test module at the bottom.
//!
//! **Parcel 2c adds [`Tab::Objects`]** — the live object pool, the player slots and one addressed slot,
//! in one tab (design §2.1: `player_state` is a section and `object_slot` is a row expansion, because a
//! separate tab for either would be the same table under a different filter). Its model lives in
//! [`crate::objects`], which calls `oracle_aether::decoders` — *the module the three handlers themselves
//! use* — so panel and reply run one decoder over one set of bytes. That is R1 at its purest and also its
//! sharpest edge: a parity pair cannot see a defect in what it shares, which is why that module's test
//! carries a clause comparing the decode against values the test wrote rather than against the bus.
//!
//! **Parcel 3 makes the run-loop change** — `Observe` wrappers plus a per-frame `pump` — and adds the
//! [`Transport`] bar that rides it. It is a **control, not a [`Tab`]**: things you *do* live on the bar,
//! things you *look at* live in the dock, and a `Tab` variant would also owe
//! [`crate::layout::LAYOUT_VERSION`] a bump and discard every stored layout. The three tabs that read the
//! instruments this parcel started feeding — Breakpoints, Watchpoints, Profiler — are the next parcel's;
//! [`Bus::read_instruments`](crate::bus::Bus::read_instruments) is what they will draw from.
//!
//! **`PANELS-NAV` adds the way in.** All eight tabs shipped and **six of them were unreachable**:
//! `egui_dock` draws only each leaf's active tab, [`initial_dock`] stacks three and three, and the window
//! had no menu, no tab list and no other affordance. [`crate::nav`] is the repair — a `panels` menu in the
//! top bar, a control rather than a ninth [`Tab`] for the reason [`Transport`] is one, deriving its rows
//! from [`Tab::ALL`] so the next panel somebody adds cannot be left out of it. Design §5.9.

use crate::bus::Bus;
use crate::machine::Machine;
use crate::memory::{self, MemoryPanel};
use crate::objects::{self, Objects, ObjectsPanel};
use crate::pacing::Governor;
use crate::screen;
use crate::screen_pick;
use crate::stopping::{self, Live};
use oracle_core::io::Pad;
use oracle_core::symbols::SymbolTable;
use serde_json::{json, Value};

/// A docked tab.
///
/// **`Serialize`/`Deserialize` are load-bearing and their spelling is part of the saved file.** A
/// `DockState<Tab>` stores these values inline as serde's external tagging of unit variants — the literal
/// text `Objects` ends up in the layout file — so renaming, removing or reordering a variant invalidates
/// every layout already on disk. That is handled by discarding, not migrating: **bump
/// [`crate::layout::LAYOUT_VERSION`] in the same change that touches this enum.** See `layout.rs`'s header
/// for what happens if you do not (the user still gets a working default; the discard is just less
/// deliberate).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum Tab {
    /// The emulator picture. The one tab that carries the uploaded texture.
    Screen,
    /// Live pacing state — this parcel's subject, visible while it happens.
    Pacing,
    /// The 68000 register file and the cheap half of `emulator/status`, in one tab. Nine key/values in a
    /// tab of their own beside a tab holding the same pc/sp/sr is two panels waiting to disagree.
    Registers,
    /// One hex view over five address spaces, with a selector rather than five tabs — five tabs would be
    /// five scroll positions to keep in your head (design §2.1).
    Memory,
    /// The live object pool, the player slots as a section, and one addressed slot as a row expansion.
    /// Three served rows, one tab, for the same reason Memory is one tab.
    Objects,
    /// The armed breakpoint set with hit counts, an add box and a per-row toggle. **Reads
    /// [`Bus::read_breakpoints`], not `read_instruments`** — see that method for why breakpoints are not
    /// one of the two instruments.
    Breakpoints,
    /// The armed watches, the retained hit log, and the three counters that make a negative finding
    /// readable (`seen` / `matched` / `dropped`).
    Watchpoints,
    /// The cycle accountant: armed state, the sample's divisor, and the hottest routines.
    Profiler,
}

impl Tab {
    /// **Every variant, in the order the docs and the layout vocabulary list them.**
    ///
    /// Its completeness is guarded twice. `layout_version_is_the_last_row_of_the_tab_vocabulary` in
    /// [`crate::layout`] refuses a `Tab` change that forgets [`crate::layout::LAYOUT_VERSION`] — but it
    /// measures "today's vocabulary" *through this array*, so a variant added to the enum and left out of
    /// here is invisible to it. That hole is closed by
    /// `every_tab_the_player_ships_is_reachable_from_the_nav` in [`crate::nav`], which asks **serde's
    /// derive** what variants exist and compares. It has to: [`crate::nav::entries`] maps over this
    /// array, so a variant missing from it is a panel with a body and no way to open it.
    pub const ALL: [Tab; 8] = [
        Tab::Screen,
        Tab::Pacing,
        Tab::Registers,
        Tab::Memory,
        Tab::Objects,
        Tab::Breakpoints,
        Tab::Watchpoints,
        Tab::Profiler,
    ];

    /// **The name on this tab's bar** — and, because [`crate::nav`] calls this same function, the name in
    /// the panel menu.
    ///
    /// A method rather than a `match` inside [`egui_dock::TabViewer::title`] for exactly the reason
    /// [`Transport::toggle`] is a method: a second surface reports these strings, and a label spelled
    /// twice is a nav and a tab bar naming one panel two ways — a defect no test of either alone can see.
    /// `TabViewer::title` below is now this call and nothing else.
    pub const fn title(self) -> &'static str {
        match self {
            Tab::Screen => "Screen",
            Tab::Pacing => "Pacing",
            Tab::Registers => "Registers",
            Tab::Memory => "Memory",
            Tab::Objects => "Objects",
            Tab::Breakpoints => "Breakpoints",
            Tab::Watchpoints => "Watchpoints",
            Tab::Profiler => "Profiler",
        }
    }
}

/// Everything the tab bodies touch. Held apart from the `DockState` so both can be borrowed at once.
///
/// **`machine` and `bus` are `&mut` from this parcel on**, because `Host::call` swaps the caller's
/// `System` into the engine for the duration of a dispatch and hands it straight back. A panel still
/// cannot *advance* the machine — nothing here reaches `run_frames` — but it can no longer take a shared
/// borrow, and pretending otherwise would mean copying the machine to ask it a question.
pub struct Panels<'a> {
    pub tex: Option<&'a egui::TextureHandle>,
    pub machine: &'a mut Machine,
    pub bus: &'a mut Bus,
    pub mem: &'a mut MemoryPanel,
    /// The Objects tab's own state: which row is expanded. The pool itself is re-derived every repaint
    /// and never cached — `emulator/load_symbols` can move the layout mid-session, so a cached one is
    /// stale by construction.
    pub objects: &'a mut ObjectsPanel,
    /// The three stopping tabs' own state: what is typed into their add boxes and the last answer each
    /// got. **Nothing about what is armed lives here** — that is the `Host`'s, read afresh every repaint
    /// through [`Bus::read_breakpoints`] and [`Bus::read_instruments`] (R2).
    pub stopping: &'a mut stopping::Panel,
    /// The Screen tab's own state: the standing readout of the last click, the handles of the watches
    /// **this panel** armed, and spawn mode. See [`crate::screen_pick`].
    pub screen: &'a mut screen_pick::Panel,
    /// ⚑ **The display mask [`tex`](Panels::tex) was drawn under**, or `None` before the first upload.
    ///
    /// The mask of *the picture on the glass* — read off the uploaded texture, deliberately not off the bus.
    /// The bus's mask is what the machine has been told; this is what a person is looking at, and the two
    /// separate for as long as it takes a change to reach the next upload. `crate::screen_pick` refuses a
    /// click on exactly that gap rather than describing a picture that is not there.
    pub screen_mask: Option<oracle_core::render::LayerMask>,
    pub governor: &'a Governor,
    pub status: &'a str,
    /// The `--rom` argument as the human typed it. The strip absolutises it through the bus's own
    /// [`oracle_aether::engine::absolutise`] before showing it — see [`StatusStrip::rom_path`].
    pub rom_path: &'a str,
    /// The listing actually loaded, or `None`. The same table the bus resolves against — one
    /// `SymbolTable`, handed to `Host::set_machine_info` and borrowed here, never two.
    pub symbols: Option<&'a SymbolTable>,
}

impl egui_dock::TabViewer for Panels<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(match tab {
            Tab::Screen => "screen",
            Tab::Pacing => "pacing",
            Tab::Registers => "registers",
            Tab::Memory => "memory",
            Tab::Objects => "objects",
            Tab::Breakpoints => "breakpoints",
            Tab::Watchpoints => "watchpoints",
            Tab::Profiler => "profiler",
        })
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        // One expression, shared with the panel menu. See [`Tab::title`].
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Screen => {
                // Controls and the standing readout first, then the picture with whatever is left. The
                // order is the layout: `screen` allocates all remaining space.
                self.screen_controls(ui);
                ui.separator();
                self.screen(ui);
            }
            Tab::Pacing => self.pacing(ui),
            Tab::Registers => self.registers(ui),
            Tab::Memory => self.memory(ui),
            Tab::Objects => self.objects(ui),
            Tab::Breakpoints => self.breakpoints(ui),
            Tab::Watchpoints => self.watchpoints(ui),
            Tab::Profiler => self.profiler(ui),
        }
    }
}

impl Panels<'_> {
    /// The picture, and **the pointer over it**.
    ///
    /// ⚑ **The `Response` is kept.** It used to be thrown away — `ui.add(Image…)`'s return value was
    /// discarded, which is exactly why `docs/OVERSEER.md`'s `F-SPAWN-PICKER-PANEL-SURFACE` recorded that
    /// this tab could not receive a click. It now carries the two things the inverse needs and that minifb
    /// never told `oracle-frontend`: **where the image actually landed** (`Response::rect`) and **where the
    /// pointer was when it went down** (`Response::interact_pointer_pos`), both in the same space.
    ///
    /// The rect is allocated explicitly rather than through `centered_and_justified`, because the whole
    /// gesture rests on the rect being *the picture's* and nothing else's: a justified layout is free to
    /// hand a widget more room than it asked for, and a click inverted against a rect one pixel wider than
    /// the picture is an offset nothing on screen would explain.
    ///
    /// Everything about what a click *means* is `crate::screen_pick`'s, including the standing statements
    /// this draws. This function decides where the picture goes and what the pointer did, and nothing else.
    fn screen(&mut self, ui: &mut egui::Ui) {
        let Some(tex) = self.tex else {
            ui.centered_and_justified(|ui| ui.label("no frame yet"));
            return;
        };
        let src = tex.size_vec2();
        let ppp = ui.pixels_per_point();
        // One reading of the available space, used for both the fit and the allocation. Two calls would
        // be two readings of a thing that can change, and the picture would then be fitted to one box and
        // centred in another.
        let avail = ui.available_size();
        let size = screen_pick::fit(
            avail,
            src.x as usize,
            src.y as usize,
            ppp,
            self.screen.aspect,
        );
        if size.x <= 0.0 || size.y <= 0.0 {
            return;
        }
        let (outer, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let image_rect = egui::Rect::from_center_size(outer.center(), size);
        // Nearest sampling, because a Genesis pixel is a Genesis pixel.
        egui::Image::new(tex)
            .texture_options(egui::TextureOptions::NEAREST)
            .paint_at(ui, image_rect);
        let hit = ui.interact(
            image_rect,
            ui.id().with("screen-picture"),
            egui::Sense::click(),
        );

        if let (true, Some(pos)) = (hit.clicked(), hit.interact_pointer_pos()) {
            if let Some(dot) =
                screen_pick::dot_at(image_rect, pos, ppp, src.x as usize, src.y as usize)
            {
                // The mask **this texture** was drawn under travels with the click, so the panel can
                // refuse rather than describe if the machine's mask has moved since. See `screen_pick`.
                self.screen
                    .click(self.machine, self.bus, self.screen_mask, dot);
            }
        }
    }

    /// The Screen tab's controls and its standing readout, drawn **above** the picture in their own strip.
    ///
    /// Separate from [`Panels::screen`] because the picture takes all the room there is: a control drawn
    /// inside that allocation would be over the game, and one drawn after it would have no room at all.
    /// Above rather than below for the reason the halting alarm is on the top bar rather than in a tab —
    /// a standing statement that can be scrolled or cropped out of view is not standing.
    fn screen_controls(&mut self, ui: &mut egui::Ui) {
        // ⚑ **The standing mask statement (S2a), first and unconditionally.** A mask changes what the
        // picture *is*, so it says so for as long as it is on, in prose, where a person is looking — not
        // in a tooltip, not once, and not only in the wire caveat. `docs/OVERSEER.md`'s GUI-LAYERS entry
        // banks the reasoning: *the author will forget, and then read a masked picture as the real one.*
        // The sentence itself is `screen_pick::mask_statement`, derived from `LayerMask::hidden()`.
        //
        // **Read off the GLASS, not off the bus**, and that is the whole point of `screen_mask`: this line
        // describes the picture below it. `Bus::layers()` is what the machine has been told, and the two
        // separate for as long as it takes a change to reach the next upload.
        let bus_mask = self.bus.layers();
        if let Some(s) = self.screen_mask.and_then(screen_pick::mask_statement) {
            ui.colored_label(ui.visuals().warn_fg_color, s);
        }
        // ⚑ **Loud on unmeasurable.** When the glass and the machine are not the same mask, this panel
        // cannot honestly describe what is on screen, so it says that rather than describing it — the
        // same rule that makes a click refuse in that window, from a function beside that one so the
        // alarm and the refusal cannot disagree about a frame. Ordinarily impossible to see: the drain
        // masks the picture before this frame is composed, so the two agree.
        if let Some(s) = screen_pick::glass_alarm(self.screen_mask, bus_mask) {
            ui.colored_label(ui.visuals().error_fg_color, s);
        }
        // The spawn badge is a correctness requirement rather than decoration: a mode that changes what a
        // left-click *does* must say so for as long as it is on, and it must name the archetype.
        if let Some(badge) = self.screen.badge() {
            ui.colored_label(ui.visuals().warn_fg_color, &badge);
        }
        // ⚑ **The four layer toggles** — one per `LayerMask::targets()` entry, generated from the core's
        // own vocabulary rather than typed here, so this window cannot offer a layer the bus lacks or
        // spell one differently. This is `F-PLAYER-PALETTE-NO-ACTIONS`'s four `ToggleLayer` rows, and they
        // close here rather than being re-typed: they are the only frontend actions that were already a
        // served method (`emulator/set_layer_enabled`), which is what the toggle calls.
        //
        // The checkbox shows **the bus's** mask, not the glass's, because it is a control: it must report
        // the state it writes. The line above is what reports the glass.
        ui.horizontal(|ui| {
            ui.weak("layers:");
            for (name, layer) in oracle_core::render::LayerMask::targets() {
                let mut shown = bus_mask.shows(layer);
                if ui.checkbox(&mut shown, name).changed() {
                    self.screen.set_layer(self.machine, self.bus, name, shown);
                }
            }
        });
        ui.horizontal(|ui| {
            // Spawn mode is a **control**, not a tab: things you *do* are controls. The armed/disarmed
            // split is one button for one question, the same rule the transport bar states.
            if self.screen.is_armed() {
                if ui.button("spawn: off").clicked() {
                    self.screen.disarm_spawn();
                }
                if ui.button("next archetype").clicked() {
                    self.screen.cycle_spawn();
                }
            } else if ui.button("spawn mode…").clicked() {
                self.screen.arm_spawn(self.machine, self.bus);
            }
            ui.separator();
            // ⚑ The aspect selector. `Aspect::name()` is the frontend's own short name, so the two windows
            // cannot spell a mode differently, and the set is written out rather than derived because
            // `Aspect` is a three-variant enum with no `ALL` — an added variant is a compile error at the
            // match in `present.rs`, not a silently missing button here.
            for a in [
                oracle_frontend::present::Aspect::Tv,
                oracle_frontend::present::Aspect::Square,
                oracle_frontend::present::Aspect::Integer,
            ] {
                if ui
                    .selectable_label(self.screen.aspect == a, a.name())
                    .clicked()
                {
                    self.screen.aspect = a;
                }
            }
            ui.separator();
            ui.weak(format!("{} armed by this panel", self.screen.armed_count()));
        });
        if let Some(r) = self.screen.readout() {
            // Coloured on the **field**, never on the shape of the text. See `screen_pick::Readout`.
            let colour = if r.refused {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(colour, &r.text);
        }
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
        // `Some(...)`, never `None`: this panel has a bus, so the strip is asked rather than told nothing.
        // The `None` arm exists for a caller that genuinely has no bus, and there is none on this path.
        let held = Some(self.bus.held_pads());
        // The same derivation the transport bar draws from, built here rather than passed down: both are
        // pure reads of the `Host`'s own set and instruments, so there is one answer and two consumers of
        // it, never a copy handed between panels.
        let halting = {
            let (watch, _, _) = self.bus.read_instruments();
            Some(crate::stopping::Halting::of(
                self.bus.read_breakpoints(),
                self.bus.last_break(),
                watch,
                self.bus.is_paused(),
                self.machine.system().scheduler().now() / oracle_core::system::MCLK_PER_FRAME,
                self.symbols,
            ))
        };
        for (label, value) in StatusStrip::of(
            self.machine,
            self.rom_path,
            self.symbols,
            held,
            Some(self.bus.aether_status()),
            halting,
        )
        .rows()
        {
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

    /// **The Memory panel.** One hex view, a space selector, an address box that takes a symbol, a write
    /// cell that states its own gate, and a hash button. See [`crate::memory`] for which half of this
    /// goes through `Host::call` and which reads direct, and why.
    fn memory(&mut self, ui: &mut egui::Ui) {
        // --- the space selector ---
        ui.horizontal_wrapped(|ui| {
            ui.label("space");
            for space in memory::Space::ALL {
                if ui
                    .selectable_label(self.mem.space == space, space.label())
                    .clicked()
                {
                    self.mem.space = space;
                    // Notes belong to the gesture that produced them, and a gesture is scoped to the
                    // space it was made in. Carrying "REFUSED …" across a selector click would put a
                    // sentence about VRAM under a bus view.
                    self.mem.addr_note = None;
                    self.mem.write_note = None;
                    self.mem.hash_note = None;
                }
            }
        });
        ui.small(format!(
            "reads reproduce {} · writes go through {}",
            self.mem.space.read_method(),
            self.mem
                .space
                .write_method()
                .unwrap_or("(no write row on this space)")
        ));
        ui.separator();

        // --- the address box, which IS `emulator/lookup_symbol` (design §2.2) ---
        ui.horizontal(|ui| {
            ui.label("address");
            let entry = ui.add(
                egui::TextEdit::singleline(&mut self.mem.addr_text)
                    .desired_width(220.0)
                    .hint_text("0xFFFF0000 or a symbol name"),
            );
            let go = ui.button("go").clicked()
                || (entry.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                let space = self.mem.space;
                let text = self.mem.addr_text.clone();
                let (bus, sys) = (&mut *self.bus, self.machine.system_mut());
                self.mem.addr_note = Some(match memory::resolve_address(bus, sys, space, &text) {
                    memory::Resolved::Hex(a) => {
                        self.mem.base = a;
                        memory::Line::plain(format!(
                            "{} — a hex literal, taken as typed",
                            oracle_aether::hex::addr(a)
                        ))
                    }
                    memory::Resolved::Symbol { addr, reply } => {
                        self.mem.base = addr;
                        memory::Line::plain(format!("ok — {reply}"))
                    }
                    memory::Resolved::Refused(e) => {
                        memory::answer_line(&crate::bus::Answer::Err(e))
                    }
                    memory::Resolved::Rejected(why) => memory::Line::from_panel(why),
                });
            }
            if ui.button("◀ page").clicked() {
                self.mem.base = self
                    .mem
                    .base
                    .wrapping_sub((memory::ROWS * memory::PER_ROW) as u32);
                self.mem.addr_text = oracle_aether::hex::addr(self.mem.base);
            }
            if ui.button("page ▶").clicked() {
                self.mem.base = self
                    .mem
                    .base
                    .wrapping_add((memory::ROWS * memory::PER_ROW) as u32);
                self.mem.addr_text = oracle_aether::hex::addr(self.mem.base);
            }
        });
        if let Some(note) = &self.mem.addr_note {
            note_label(ui, note);
        }
        ui.separator();

        // --- the hex view: a DIRECT read, through the handlers' own functions ---
        let v = memory::view(
            self.mem.space,
            self.machine.system(),
            self.mem.base,
            memory::ROWS,
            memory::PER_ROW,
        );
        match &v.error {
            // Never an empty grid: a blank hex view and a refused read look identical on a screen, and
            // only one of them means "there is nothing here".
            Some(e) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("REFUSED {}: {}", e.code, e.message),
                );
            }
            None => {
                if let Some(r) = v.region {
                    ui.small(format!("region  {r}"));
                }
                if let Some(n) = v.truncated_to {
                    ui.small(format!(
                        "showing {n} bytes from {} — the space ends before a full page",
                        oracle_aether::hex::addr(v.base)
                    ));
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for row in &v.rows {
                        ui.monospace(format!(
                            "{}  {:<47}  {}",
                            oracle_aether::hex::addr(row.addr),
                            row.hex(),
                            row.ascii()
                        ));
                    }
                });
            }
        }
        ui.separator();

        // --- the write cell, gated by the handler's own answer ---
        let gate = {
            let (bus, sys) = (&mut *self.bus, self.machine.system_mut());
            self.mem.gates_for(bus, sys).clone()
        };
        ui.horizontal(|ui| {
            ui.label("write at address");
            // `add_enabled` is what makes the cell inert; the sentence beneath is what makes it
            // *explicable*. Neither alone is acceptable — a greyed box with no words is a control a
            // human cannot tell from a broken one.
            ui.add_enabled(
                gate.is_open(),
                egui::TextEdit::singleline(&mut self.mem.write_text)
                    .desired_width(220.0)
                    .hint_text("hex bytes, e.g. 4E71"),
            );
            if ui
                .add_enabled(gate.is_open(), egui::Button::new("poke"))
                .clicked()
            {
                let (space, base) = (self.mem.space, self.mem.base);
                let payload = self.mem.write_text.clone();
                self.mem.write_note = Some(match memory::write_params(space, base, &payload) {
                    Err(why) => memory::Line::from_panel(why),
                    Ok(params) => {
                        let method = space.write_method().unwrap_or("");
                        let (bus, sys) = (&mut *self.bus, self.machine.system_mut());
                        // Stamped, because `write_vram` is the one write that lands in a *running*
                        // machine (see the asymmetry below): "ok" alone leaves a human unable to say
                        // which frame absorbed the poke, and the next frame may already have redrawn
                        // over it. D11 puts `{frame, mclk, running}` on every reply for exactly this.
                        let (answer, stamp) = bus.call_stamped(sys, method, &params);
                        let line = memory::answer_line(&answer);
                        memory::Line {
                            refused: line.refused,
                            text: format!(
                                "{}   [frame {} · mclk {} · running {}]",
                                line.text,
                                stamp
                                    .get("frame")
                                    .map_or_else(|| "?".into(), |v| v.to_string()),
                                stamp
                                    .get("mclk")
                                    .map_or_else(|| "?".into(), |v| v.to_string()),
                                stamp
                                    .get("running")
                                    .map_or_else(|| "?".into(), |v| v.to_string()),
                            ),
                        }
                    }
                });
                // A write can change the gate's own answer only via the run state, which a write cannot
                // touch — so nothing is invalidated here. Said out loud because the reflex is to
                // re-probe, and re-probing after every poke would be a dispatch per keystroke.
            }
        });
        ui.small(gate.why());
        if let Some(note) = &self.mem.write_note {
            note_label(ui, note);
        }

        // ⚑ **All five gates at once, always visible.** The asymmetry this shows is real and it is the
        // server's: three writes are paused-only and `write_vram` is not, so right now a human can poke
        // VRAM mid-frame and is refused the identical gesture on work RAM. The panel does not gate VRAM
        // for consistency's sake — a panel that refuses what the tool allows misdescribes the server
        // just as surely as one that allows what the tool refuses — and it does not hide the
        // inconsistency behind the selector either, because an asymmetry you can only find by clicking
        // through five spaces is an asymmetry nobody finds.
        ui.collapsing("what every space accepts right now", |ui| {
            for space in memory::Space::ALL {
                let g = self.mem.gate_of(space);
                ui.monospace(format!(
                    "{:<22} {}  {}",
                    space.label(),
                    if g.is_open() { "WRITE" } else { "  —  " },
                    g.why()
                ));
            }
            ui.small(
                "Not a defect in this panel. §6's run-control rule names write_memory, write_cram and \
                 z80_write and does not name write_vram, and the server serves the gate it was given \
                 (relaxing a refusal later is additive; introducing one is not). The argument for \
                 naming that row is filed upstream, not settled here.",
            );
        });
        ui.separator();

        // --- memory_hash: a read you invoke, for a range you chose ---
        let hash_gate = memory::hash_gate(self.mem.space);
        ui.horizontal(|ui| {
            ui.label("hash range: len");
            ui.add_enabled(
                hash_gate.is_ok(),
                egui::TextEdit::singleline(&mut self.mem.hash_len_text).desired_width(80.0),
            );
            if ui
                .add_enabled(hash_gate.is_ok(), egui::Button::new("memory_hash"))
                .clicked()
            {
                let base = self.mem.base;
                let parsed = self.mem.hash_len_text.trim().parse::<u64>();
                self.mem.hash_note = Some(match parsed {
                    Err(e) => {
                        memory::Line::from_panel(format!("len {:?}: {e}", self.mem.hash_len_text))
                    }
                    Ok(len) => {
                        let (bus, sys) = (&mut *self.bus, self.machine.system_mut());
                        memory::answer_line(&memory::hash(bus, sys, base, len))
                    }
                });
            }
        });
        if let Err(why) = &hash_gate {
            ui.small(why.as_str());
        }
        if let Some(note) = &self.mem.hash_note {
            note_label(ui, note);
        }
    }

    /// **The Objects tab.** One tab, three served rows: the pool table (`object_list`), the player
    /// section (`player_state`) and the row expansion (`object_slot`). Every one of them is a DIRECT
    /// read through `oracle_aether::decoders` — the module the handlers use — because these repaint at
    /// 60 Hz and none of the three is paused-gated, so the table is live while the game plays.
    ///
    /// ⚑ **The refusal is the first thing this function handles and it is a whole-tab state**, not a
    /// banner over an empty grid. `decoders::derive(None)` refuses, and an empty table in its place would
    /// assert that this game has no objects.
    fn objects(&mut self, ui: &mut egui::Ui) {
        let view = Objects::of(self.symbols, self.machine.system());
        let pool = match &view {
            Objects::Refused(e) => {
                ui.colored_label(ui.visuals().error_fg_color, objects::refusal_text(e));
                return;
            }
            Objects::Pool(p) => p,
        };

        // The layout, as facts rather than as its own JSON. Every value on these lines is a key of the
        // `layout` object the three ⚙ rows serve — the same answer, spelled for a human. Composed in
        // `objects` rather than here so the strings are reachable from a test.
        for line in pool.layout_lines() {
            ui.monospace(line);
        }

        ui.small(
            "Every address here is read out of the loaded listing — the base from Object_RAM/Player_1, \
             the stride from Player_2 − Player_1, the count from Object_RAM_End, and the ring buffer \
             below from Ring_Count − Ring_Buffer. Nothing is hardcoded, because an object-table address \
             is a fact about one build.",
        );
        ui.separator();

        // --- the player section: the same decoder, the same layout, its own refusal ---
        match &pool.players {
            Err(e) => {
                ui.strong("players — emulator/player_state");
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "REFUSED {} {}\n\nThe pool table below is unaffected: this refusal is about \
                         which slots are PLAYERS, not about where the table is.",
                        e.code, e.message
                    ),
                );
            }
            Ok(pv) => {
                // The section says how much of the table it covers. Two rows out of sixty-six is a fact
                // a reader needs in order not to take this section for the pool.
                ui.strong(format!(
                    "players — emulator/player_state — {} of {} slots",
                    pv.players.len(),
                    pv.layout.slot_count()
                ));
                for p in &pv.players {
                    let role = p.cell("role");
                    if p.active {
                        ui.monospace(format!("{role:<12} {}", p.summary()));
                    } else {
                        // `active: false` is the answer to "is player 2 present", so it is a row that
                        // says so — not a row omitted and not a row of zeroes.
                        ui.monospace(format!(
                            "{role:<12} {:>3}  {:<10}  not present (the slot is empty)",
                            p.slot,
                            p.cell("addr")
                        ));
                    }
                }
            }
        }
        ui.separator();

        // --- the pool table ---
        ui.strong(format!(
            "object pool — emulator/object_list — {} active of {} slots",
            pool.total, pool.slot_count
        ));
        // --- rings, immediately under the object count, because that is where the question is asked ---
        //
        // The two numbers sit together on purpose: "5 active of 66 slots" invites "so where are the
        // rings", and the answer is that they are not in that 66 at all.
        match &pool.rings {
            Ok(r) => {
                ui.monospace(r.summary());
                ui.small(objects::RINGS_WHY);
                // The ceiling is in `summary()` when the listing publishes the entry size (§11.36).
                // When it does not, the gap is STATED: an absent ceiling with no sentence beside it is
                // an invitation to divide the span above by a remembered entry size.
                if r.ceiling.is_none() {
                    ui.small(objects::CEILING_UNKNOWN);
                }
            }
            // A ring line this listing cannot supply is one missing line, and it says which symbol did
            // not answer — never a `0`, which would read as "no rings are loaded".
            Err(e) => {
                ui.monospace("rings   —");
                ui.small(format!("rings unavailable — {} {}", e.code, e.message));
            }
        }

        if pool.objects.is_empty() {
            // A stated fact, and a different one from the refusal above: the layout derived, the table
            // was read, and nothing is live in it right now.
            ui.monospace(
                "0 active objects — the layout derived and every slot's code word is the empty-slot \
                 sentinel. This is not a missing listing.",
            );
        }
        ui.monospace(format!(
            "{:>3}  {:<10}  {:<8}  {:>7} {:>7}  {}",
            "sl", "addr", "code", "x", "y", "name"
        ));
        egui::ScrollArea::vertical()
            .id_salt("object-pool")
            .max_height(240.0)
            .show(ui, |ui| {
                for r in &pool.objects {
                    let open = self.objects.selected == Some(r.slot);
                    if ui
                        .selectable_label(open, egui::RichText::new(r.summary()).monospace())
                        .clicked()
                    {
                        // A second click closes it: the expansion is one row's detail, and a row that
                        // cannot be un-expanded is a mode with no way out.
                        self.objects.selected = if open { None } else { Some(r.slot) };
                    }
                }
            });

        // --- the row expansion: one addressed slot, every field the layout declares ---
        let Some(slot) = self.objects.selected else {
            return;
        };
        ui.separator();
        match objects::object_slot(self.symbols, self.machine.system(), slot) {
            Err(e) => {
                ui.strong(format!("slot {slot} — emulator/object_slot"));
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("REFUSED {} {}", e.code, e.message),
                );
            }
            Ok(v) => {
                // The address is spelled by the bus's own `hex::addr`, not by a second `{:X}` here: this
                // is the string `addr` carries on the wire, and two spellings of one address is how a
                // reader ends up comparing the panel to a tool and seeing a difference that is not one.
                ui.strong(format!(
                    "slot {slot} of {} at {} — emulator/object_slot",
                    v.layout.slot_count(),
                    oracle_aether::hex::addr(v.row.addr)
                ));
                egui::ScrollArea::vertical()
                    .id_salt("object-slot")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (k, val) in &v.row.item {
                            if k == "fields" {
                                continue;
                            }
                            ui.monospace(format!("{k:<12} {}", render(val)));
                        }
                        if let Some(f) = v.row.item.get("fields").and_then(|f| f.as_object()) {
                            ui.separator();
                            for (k, val) in f {
                                ui.monospace(format!("  {k:<20} {}", render(val)));
                            }
                        } else {
                            // Only reachable on an inactive slot, where the decoded keys are OMITTED
                            // rather than zeroed — bytes the game never wrote are not data.
                            ui.monospace(
                                "no fields — this slot is empty, and an empty slot's record is bytes \
                                 the game never wrote, so they are omitted rather than shown as zeroes",
                            );
                        }
                    });
            }
        }
    }

    // -----------------------------------------------------------------------------------------------
    // ⚑ The three stopping tabs
    //
    // Every body below obeys design §4.4 in the same two moves: the TABLE is a direct read of the
    // instrument the loop itself feeds, and every GESTURE is a `Host::call` whose answer is rendered by
    // `memory::answer_line` and coloured by `Line::refused`. No panel here composes a refusal, branches on
    // message prose, or keeps a second copy of what is armed.
    // -----------------------------------------------------------------------------------------------

    /// Make one gesture and keep the server's answer. **The whole of a panel's write path.**
    ///
    /// The `Answer` goes straight to [`memory::answer_line`], which is the sentence the tool would have
    /// given a socket client, with `refused` carried beside it as the flag the colour is chosen from.
    fn issue(&mut self, method: &str, params: &Value) -> memory::Line {
        let (bus, sys) = (&mut *self.bus, self.machine.system_mut());
        memory::answer_line(&bus.call(sys, method, params))
    }

    /// The headline every stopping tab opens with: **is what follows live, or left over?**
    ///
    /// Drawn before any table, in the tab's own colour language: a retained view is not an error, so it is
    /// not coloured like one, but it is emphasised, because a reader who misses it is reading a frozen
    /// table as a moving one.
    fn live_head(ui: &mut egui::Ui, live: Live, armed: &str, retained: &str) {
        let text = live.sentence(armed, retained);
        match live {
            Live::Yes => ui.strong(text),
            Live::Retained => ui.colored_label(ui.visuals().warn_fg_color, text),
            Live::Never => ui.colored_label(ui.visuals().weak_text_color(), text),
        };
        ui.separator();
    }

    /// **The Breakpoints tab.** The armed set, an add box, a per-row toggle, a per-row ✕ and a clear-all.
    ///
    /// The table is [`Bus::read_breakpoints`] — the `Host`'s own list, the one
    /// `emulator/breakpoint_list` pages — read fresh every repaint. The four gestures are
    /// `breakpoint_add`, `breakpoint_set_enabled` and `breakpoint_clear` (twice), each through
    /// `Host::call`, so the cap refusal, the unknown-handle refusal and the `all`-plus-handle refusal all
    /// arrive in the handler's own words.
    fn breakpoints(&mut self, ui: &mut egui::Ui) {
        let view = stopping::breakpoints(self.bus.read_breakpoints(), self.symbols);

        Self::live_head(
            ui,
            view.live,
            &format!(
                "{} of {} breakpoint{} armed — the machine will halt at {}",
                view.armed,
                view.rows.len(),
                if view.rows.len() == 1 { "" } else { "s" },
                if view.armed == 1 { "it" } else { "them" }
            ),
            &if view.rows.is_empty() {
                "No breakpoint has been armed, so nothing here will stop the machine.".to_owned()
            } else {
                format!(
                    "{} breakpoint{} held and every one of them disabled, carrying {} hit{} between them \
                     from when they were armed.",
                    view.rows.len(),
                    if view.rows.len() == 1 { "" } else { "s" },
                    view.retained_hits,
                    if view.retained_hits == 1 { "" } else { "s" },
                )
            },
        );

        // ⚑ **What the set has already DONE to this machine** — the half [`Live`] cannot express.
        // `live_head` above says whether these rows can stop the machine; this says whether one of them
        // is stopping it right now, at what address, and how many times it has. Same derivation as the
        // transport bar and the status strip, so the tab a human opens to fix the problem and the bar
        // that told them about it cannot word it differently.
        let halting = {
            let (watch, _, _) = self.bus.read_instruments();
            stopping::Halting::of(
                self.bus.read_breakpoints(),
                self.bus.last_break(),
                watch,
                self.bus.is_paused(),
                self.machine.system().scheduler().now() / oracle_core::system::MCLK_PER_FRAME,
                self.symbols,
            )
        };
        if let Some(head) = halting.headline() {
            let colour = if halting.halted_here() {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().warn_fg_color
            };
            ui.colored_label(colour, head);
            if let Some(advice) = halting.advice() {
                ui.small(advice);
            }
            ui.separator();
        }

        // --- add ---
        let mut gesture: Option<(&'static str, Value)> = None;
        ui.horizontal(|ui| {
            ui.label("at");
            ui.add(
                egui::TextEdit::singleline(&mut self.stopping.bp_target)
                    .desired_width(120.0)
                    .hint_text("0x400 or a symbol"),
            );
            ui.label("label");
            ui.add(
                egui::TextEdit::singleline(&mut self.stopping.bp_label).desired_width(90.0),
            );
            if ui
                .button("arm")
                .on_hover_text(
                    "emulator/breakpoint_add. A name goes to the server as `symbol` and the server \
                     resolves it — the reply carries the address it landed on. A second add at an \
                     occupied address is a SECOND breakpoint, never a duplicate error.",
                )
                .clicked()
            {
                match stopping::breakpoint_add_params(
                    &self.stopping.bp_target,
                    &self.stopping.bp_label,
                ) {
                    Ok(p) => gesture = Some((stopping::BREAKPOINT_ADD, p)),
                    Err(why) => self.stopping.bp_note = Some(memory::Line::from_panel(why)),
                }
            }
            if !view.rows.is_empty() {
                ui.separator();
                if ui
                    .button("clear all")
                    .on_hover_text(
                        "emulator/breakpoint_clear {all:true} — EVERY breakpoint on this server, \
                         including ones another client armed. It is a separate spelling from a handle \
                         precisely because it is not the same gesture.",
                    )
                    .clicked()
                {
                    gesture =
                        Some((stopping::BREAKPOINT_CLEAR, stopping::breakpoint_clear_all_params()));
                }
            }
        });

        // --- the table ---
        if view.live.has_rows() {
            ui.separator();
            ui.monospace(format!(
                "{:<5} {:<10} {:<8} {:>9}",
                "id", "addr", "state", "hits"
            ));
            egui::ScrollArea::vertical()
                .id_salt("breakpoint-rows")
                .max_height(220.0)
                .show(ui, |ui| {
                    for r in &view.rows {
                        ui.horizontal(|ui| {
                            // The checkbox is `breakpoint_set_enabled`, the ONE writer of this field on
                            // this bus. Its value is read from the row, never from a local mirror, so a
                            // refused toggle simply leaves the box where the server left it.
                            let mut on = r.enabled;
                            if ui
                                .checkbox(&mut on, "")
                                .on_hover_text(
                                    "emulator/breakpoint_set_enabled. `hits` is carried ACROSS the \
                                     toggle — this surface never resets a count; a fresh one means \
                                     clear and re-add.",
                                )
                                .changed()
                            {
                                gesture = Some((
                                    stopping::BREAKPOINT_SET_ENABLED,
                                    stopping::breakpoint_enable_params(&r.handle, on),
                                ));
                            }
                            if ui.small_button("✕").clicked() {
                                gesture = Some((
                                    stopping::BREAKPOINT_CLEAR,
                                    stopping::breakpoint_clear_params(&r.handle),
                                ));
                            }
                            let text = egui::RichText::new(r.summary()).monospace();
                            // A disabled row is dimmed, from `enabled` — the same field the word in the
                            // row says. Two encodings of one fact, but the fact is the one a reader is
                            // most likely to skim past, and neither is derived from the other's string.
                            if r.enabled {
                                ui.label(text);
                            } else {
                                ui.label(text.weak());
                            }
                        });
                    }
                });
        }

        if let Some((method, params)) = gesture {
            self.stopping.bp_note = Some(self.issue(method, &params));
        }
        if let Some(note) = &self.stopping.bp_note {
            ui.separator();
            note_label(ui, note);
        }
    }

    /// **The Watchpoints tab.** The armed watches, the retained hit log, and the aggregates.
    ///
    /// ⚑ **The hit log outliving its watch is the ordinary state here, not a corner.**
    /// `emulator/watchpoint_clear` keeps a watch's hits deliberately, so one click turns a live trace into
    /// a historical one with the rows unchanged. [`Live`] is the only thing on screen that can say which
    /// it is.
    ///
    /// `seen` and `matched` are shown together and always: **`seen > 0, matched == 0` is a genuine
    /// negative finding** and is indistinguishable from a silently-dropped watch unless both numbers are
    /// in front of the reader. That is the instrument's own stated hazard, not a rule invented here.
    fn watchpoints(&mut self, ui: &mut egui::Ui) {
        let view = {
            let (w, _, _) = self.bus.read_instruments();
            stopping::watches(w)
        };

        Self::live_head(
            ui,
            view.live,
            &format!(
                "{} watch{} armed",
                view.watches.len(),
                if view.watches.len() == 1 { "" } else { "es" }
            ),
            &if view.hits.is_empty() && view.seen == 0 {
                "No watch has been armed, so nothing has been recorded.".to_owned()
            } else {
                format!(
                    "No watch is armed; {} recorded hit{} remain, kept on purpose when the watch was \
                     cleared so one client cannot erase another's evidence.",
                    view.hits.len(),
                    if view.hits.len() == 1 { "" } else { "s" },
                )
            },
        );

        // --- add ---
        let mut gesture: Option<(&'static str, Value)> = None;
        let st = &mut *self.stopping;
        ui.horizontal_wrapped(|ui| {
            ui.label("at");
            ui.add(
                egui::TextEdit::singleline(&mut st.w_target)
                    .desired_width(110.0)
                    .hint_text("0xFF0000 / symbol"),
            );
            ui.label("len");
            ui.add(
                egui::TextEdit::singleline(&mut st.w_len)
                    .desired_width(60.0)
                    .hint_text("bytes"),
            )
            .on_hover_text(
                "A DECIMAL byte count. It goes on the wire as a JSON number: the handler reads it with \
                 as_u64() and refuses a string outright, so \"0x10\" here is this panel's own refusal \
                 rather than a -32602 for a shape the panel chose.",
            );
            egui::ComboBox::from_id_salt("watch-space")
                .selected_text(stopping::WATCH_SPACES[st.w_space])
                .show_ui(ui, |ui| {
                    for (i, s) in stopping::WATCH_SPACES.iter().enumerate() {
                        ui.selectable_value(&mut st.w_space, i, *s);
                    }
                });
            ui.checkbox(&mut st.w_read, "read");
            ui.checkbox(&mut st.w_write, "write")
                .on_hover_text(
                    "The op is these two BOOLEANS; `emulator/watchpoint_add` has no `op` param at all \
                     (`op` is a key of its reply, saying what the pair became). Both unticked is refused \
                     by the handler — a watch that can never match — and this panel lets it say so.",
                );
            ui.label("stopAfter");
            ui.add(
                egui::TextEdit::singleline(&mut st.w_stop_after)
                    .desired_width(55.0)
                    .hint_text("∞"),
            )
            .on_hover_text("Halt the run after this many matches. Empty means never halt.");
            ui.label("label");
            ui.add(egui::TextEdit::singleline(&mut st.w_label).desired_width(80.0));
            if ui.button("arm").clicked() {
                match stopping::watch_add_params(
                    &st.w_target,
                    &st.w_len,
                    stopping::WATCH_SPACES[st.w_space],
                    st.w_read,
                    st.w_write,
                    &st.w_stop_after,
                    &st.w_label,
                ) {
                    Ok(p) => gesture = Some((stopping::WATCHPOINT_ADD, p)),
                    Err(why) => st.w_note = Some(memory::Line::from_panel(why)),
                }
            }
            if !view.watches.is_empty() && ui.button("clear all").clicked() {
                gesture = Some((stopping::WATCHPOINT_CLEAR, stopping::watch_clear_all_params()));
            }
        });

        ui.separator();
        ui.monospace(format!(
            "seen {}   matched {}   dropped {}",
            view.seen, view.matched, view.dropped
        ));
        ui.small(
            "`seen` counts every access the instrument looked at. seen > 0 with matched == 0 is a real \
             negative finding — the range was watched and nothing touched it — and it is only \
             distinguishable from a watch that never armed because both numbers are here.",
        );
        for c in &view.caveats {
            ui.colored_label(ui.visuals().warn_fg_color, c);
        }

        // --- the armed watches ---
        if !view.watches.is_empty() {
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("watch-rows")
                .max_height(160.0)
                .show(ui, |ui| {
                    for row in &view.watches {
                        let w = &row.report;
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("✕")
                                .on_hover_text(
                                    "emulator/watchpoint_clear. The watch goes; its recorded HITS stay, \
                                     deliberately — a destructive clear would let one client erase \
                                     another's evidence. The headline above changes to STOPPED.",
                                )
                                .clicked()
                            {
                                gesture = Some((
                                    stopping::WATCHPOINT_CLEAR,
                                    stopping::watch_clear_params(&row.handle),
                                ));
                            }
                            ui.monospace(format!(
                                "{:<4} {:?} {}..={}  {:?}  matched {}{}{}",
                                row.handle,
                                w.space,
                                oracle_aether::hex::addr(*w.range.start()),
                                oracle_aether::hex::addr(*w.range.end()),
                                w.op,
                                w.matched,
                                match w.stop_after {
                                    Some(n) => format!("  stopAfter {n}"),
                                    None => String::new(),
                                },
                                if w.label.is_empty() {
                                    String::new()
                                } else {
                                    format!("  ({})", w.label)
                                }
                            ));
                        });
                    }
                });
        }

        // --- the hit log ---
        if !view.hits.is_empty() {
            ui.separator();
            ui.strong(format!(
                "hit log — {} retained{}",
                view.hits.len(),
                if view.dropped > 0 {
                    format!(", {} dropped (a gap in `seq` marks them)", view.dropped)
                } else {
                    String::new()
                }
            ));
            // ⚑ **`show_rows`, not `show` — the log is virtualised, and it has to be.** The ring holds
            // `EngineConfig::watch_ring_cap` = 4096 hits and a 64 KB write watch fills it in well under a
            // second; a plain `show` formats and lays out **every** retained hit on every repaint to fill
            // a 220 px viewport that displays about ten of them. Measured at 15.220 ms of `ui-build` —
            // 91 % of a frame budget — in design §5.7.1. `show_rows` draws only the visible slice.
            //
            // It is sound here for one reason and would not be sound without it: **every row is exactly
            // one `ui.monospace` line**, so the rows are uniform and the height below describes them. A
            // `row_height` that disagrees with what is drawn misaligns the scrollbar silently, which is a
            // wrong answer traded for speed. The height is asked of the style rather than typed, and is
            // passed **sans spacing** — `show_rows` adds `item_spacing.y` itself (egui 0.36.1
            // `scroll_area.rs:991`), so adding it here would double-count it and skew the scrollbar.
            //
            // `stick_to_bottom` survives: `show_rows` calls `ui.set_height` for the whole virtual list, so
            // the `content_size` the stick-to-end arithmetic uses (`scroll_area.rs:1284`) is the full
            // height and not the drawn slice's.
            //
            // ⚠ **The one thing this DID change, so it is not re-found as a bug.** A vertical `ScrollArea`
            // has `auto_shrink.x = true` by default (`scroll_area.rs:397`, applied at `:1186`), so its
            // width follows its content — and its content is now the visible rows rather than all 4096.
            // Row widths vary by a few characters (`{:#X}` on the value, `{:?}` on op and size), so this
            // box's scrollbar can sit a few characters further left or right as the log is scrolled, where
            // before it was pinned by the widest row in the whole ring. The rows themselves are identical.
            // `auto_shrink([false, true])` would pin the scrollbar to the panel's right edge instead —
            // a LARGER departure from what shipped, which is why it was not taken. Design §5.7.2.
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            egui::ScrollArea::vertical()
                .id_salt("watch-hits")
                .max_height(220.0)
                .stick_to_bottom(true)
                .show_rows(ui, row_height, view.hits.len(), |ui, rows| {
                    for h in &view.hits[rows] {
                        ui.monospace(format!(
                            "#{:<7} f{:<6} {} {:?} {:?} {:#X} pc {}",
                            h.seq,
                            h.frame,
                            oracle_aether::hex::addr(h.addr),
                            h.op,
                            h.size,
                            h.value,
                            oracle_aether::hex::addr(h.pc),
                        ));
                    }
                });
        }

        if let Some((method, params)) = gesture {
            self.stopping.w_note = Some(self.issue(method, &params));
        }
        if let Some(note) = &self.stopping.w_note {
            ui.separator();
            note_label(ui, note);
        }
    }

    /// **The Profiler tab.** Armed state, the sample's divisor, and the hottest routines.
    ///
    /// ⚑ **This is the tab the armed-vs-retained trap was written about.** `emulator/set_profiler
    /// {enabled:false}` disarms and *keeps* the sample (§11.16), so a grid of hot routines from four
    /// minutes ago is byte-identical to one from now. [`Live`] is drawn first, in words, and
    /// [`Live::Never`] refuses to draw the grid at all — the Objects tab's rule: an empty table in place
    /// of "never measured" asserts that this ROM has no hot code.
    ///
    /// **The figures are UNDIVIDED, and the divisor is beside them.** §11.16 puts the division in the
    /// server, in `emulator/get_profiler_frames`, and a panel that divided here would be a second
    /// implementation of it — one that would have to reproduce `perFrameExact` and every one of its
    /// `*Total` partners to mean the same thing. Showing the totals and the frame count is the honest
    /// read: it is what the server divides, and it is one derivation rather than two.
    fn profiler(&mut self, ui: &mut egui::Ui) {
        let view = {
            let (_, p, armed) = self.bus.read_instruments();
            stopping::profiler(p, armed, self.symbols)
        };

        Self::live_head(
            ui,
            view.live,
            &format!(
                "the accountant is armed. {} frame{} in the sample so far, {} routine{}, {} frame{} open \
                 on the shadow stack",
                view.frames,
                if view.frames == 1 { "" } else { "s" },
                view.routine_count,
                if view.routine_count == 1 { "" } else { "s" },
                view.open_frames,
                if view.open_frames == 1 { "" } else { "s" },
            ),
            &if matches!(view.live, Live::Never) {
                "The profiler has never been armed in this session, so there is no sample to show. This \
                 is not `no hot code` — it is `nothing was measured`. Arm it below."
                    .to_owned()
            } else {
                format!(
                    "The sample of {} frame{} and {} routine{} below was retained when the accountant was \
                     disarmed (§11.16: arming resets, disarming retains, reading never clears).",
                    view.frames,
                    if view.frames == 1 { "" } else { "s" },
                    view.routine_count,
                    if view.routine_count == 1 { "" } else { "s" },
                )
            },
        );

        // --- arm / disarm ---
        let mut gesture: Option<Value> = None;
        let st = &mut *self.stopping;
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.prof_per_frame, "perFrame");
            ui.checkbox(&mut st.prof_callers, "callers");
            if ui
                .button(if view.armed { "disarm" } else { "arm" })
                .on_hover_text(
                    "emulator/set_profiler. ⚑ ARMING RESETS THE SAMPLE — every arming flag resets \
                     together (§11.18), so ticking `callers` on a running measurement and re-arming \
                     starts a FRESH sample under the lenses this click names, and the one you were \
                     watching is gone. Disarming keeps it.",
                )
                .clicked()
            {
                gesture = Some(stopping::set_profiler_params(
                    !view.armed,
                    st.prof_per_frame,
                    st.prof_callers,
                ));
            }
            ui.weak(format!(
                "lenses on the retained sample: perFrame {}   callers {}",
                view.per_frame_armed, view.callers_armed
            ));
        });

        // --- the rows ---
        if view.live.has_rows() {
            ui.separator();
            ui.monospace(format!(
                "frames in sample (the divisor `emulator/get_profiler_frames` uses)   {}",
                view.frames
            ));
            ui.small(
                "Every figure below is the UNDIVIDED sample total. The per-frame view is the server's \
                 (`emulator/get_profiler_frames`), which divides these by the count above and reports \
                 `perFrameExact` beside them; this panel shows what it divides rather than dividing a \
                 second time.",
            );
            ui.separator();
            ui.strong(format!(
                "hottest routines — top {} of {}",
                view.top.len(),
                view.routine_count
            ));
            ui.monospace(format!(
                "{:<10} {:>13} {:>13} {:>11} {:>9}  name",
                "addr", "cycles", "self", "stall", "calls"
            ));
            egui::ScrollArea::vertical()
                .id_salt("profiler-rows")
                .max_height(280.0)
                .show(ui, |ui| {
                    for r in &view.top {
                        let name = match &r.symbol {
                            Some((n, 0)) => format!("  {n}"),
                            Some((n, d)) => format!("  {n}+0x{d:X}"),
                            None => String::new(),
                        };
                        ui.monospace(format!(
                            "{:<10} {:>13} {:>13} {:>11} {:>9}{name}",
                            r.addr_text,
                            r.counts.cycles,
                            r.counts.self_cycles,
                            r.counts.stall_cycles,
                            r.counts.calls
                        ));
                    }
                });
            if view.routine_count > view.top.len() {
                ui.small(format!(
                    "{} further routine{} in the sample are not drawn. The full list is \
                     `emulator/get_profiler_frames`, whose `top` refuses a request above its cap rather \
                     than clamping — so a client can always tell a full list from a clipped one.",
                    view.routine_count - view.top.len(),
                    if view.routine_count - view.top.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
            }
        }

        if let Some(params) = gesture {
            self.stopping.prof_note = Some(self.issue(stopping::SET_PROFILER, &params));
        }
        if let Some(note) = &self.stopping.prof_note {
            ui.separator();
            note_label(ui, note);
        }
    }
}

/// A JSON scalar as the panel prints it: a string without its quotes, anything else as itself.
fn render(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// One gesture's answer, coloured by whether it was a refusal.
///
/// The colour is taken from [`memory::Line::refused`], which the [`crate::bus::Answer`] carried — never
/// from the shape of the rendered text. A refusal that reads like a success is the one rendering mistake
/// a debug surface cannot afford, and deciding by looking for a `"REFUSED"` prefix would be a second
/// encoding of a fact already in hand.
fn note_label(ui: &mut egui::Ui, note: &memory::Line) {
    if note.refused {
        ui.colored_label(ui.visuals().error_fg_color, &note.text);
    } else {
        ui.monospace(&note.text);
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
/// **Parcel 2b closes both of parcel 2a's honest gaps.** The player now has a symbol table (`--symbols`,
/// plus the `.lst`-beside-the-ROM discovery `oracle-frontend` already does), so `symbolCount` and
/// `symbolAtPc` are real rather than a "none loaded" placeholder — and the same table is handed to
/// `Host::set_machine_info`, so the engine resolves names against the identical listing. Two tables would
/// be the exact drift D7 exists to prevent, and the panel would have been the one that looked right.
pub struct StatusStrip {
    /// The ROM path **absolutised through the bus's own [`oracle_aether::engine::absolutise`]**, which is
    /// the function `Engine::set_rom_path` calls.
    ///
    /// Parcel 2a could not do this and said so: the helper was a private free function, so R1's
    /// one-derivation-two-consumers was defeated by a visibility modifier rather than by a decision, and
    /// the row was labelled `rom` to avoid claiming a normalisation it did not perform. §11.30 (CR-I) had
    /// already ruled that reporting an absolute path is a property of *every* reply field carrying a
    /// filesystem path, so this parcel published the helper and the row is now `romPath` — the same
    /// string, from the same four lines, including the pass-through case for a label that is not a path.
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
    /// Symbols in the loaded listing, or `None` when no listing loaded at all.
    ///
    /// **An `Option`, not a `usize`.** `emulator/status` serves `symbolCount: 0` for both "no table" and
    /// "an empty table", which is fine on a wire a client branches on `symbolsPath` for, and is exactly
    /// the ambiguity that must not reach a human: a `0` reads as *this ROM has no symbols*, not as
    /// *nothing was loaded*. The two are rendered as different sentences below.
    pub symbol_count: Option<usize>,
    /// The nearest preceding symbol for the PC and its displacement, through
    /// [`oracle_aether::engine::symbol_at`] — the same function `emulator/status` resolves `symbolAtPc`
    /// and `symbolDisp` with.
    pub symbol_at_pc: Option<(String, u32)>,
    /// **The buttons a client is holding through `emulator/hold`, both ports** (design §9.4).
    ///
    /// `Some([Pad::default(); 2])` and `None` are different facts and are rendered as different sentences:
    /// the first is *the bus was asked and nothing is held*, which is the ordinary case and draws nothing
    /// at all; the second is *nothing asked the bus*, which draws a loud row. Collapsing them would make
    /// the one state this field exists to reveal indistinguishable from the state where the field is not
    /// wired up — the same "unmeasurable rendered as a measurement" mistake `symbol_count` is an `Option`
    /// to avoid, one field up.
    pub held: Option<[Pad; 2]>,
    /// **Whether anything outside this process can attach to this window** (`PLAYER-SERVE`), as
    /// [`crate::bus::Bus::serve_outcome`] reports it. `None` means the strip was built with no bus to ask,
    /// exactly as for [`held`](Self::held) and for the same reason.
    pub aether: Option<crate::bus::AetherStatus>,
    /// **What is armed to halt this window, and whether it just did** (`ARMED-STATE-VISIBLE`), as
    /// [`crate::stopping::Halting`] derives it. `None` means the strip was built with no bus to ask,
    /// exactly as for [`held`](Self::held) and [`aether`](Self::aether), and it draws the same loud row.
    ///
    /// The strip carries the *long* form; the transport bar carries the short one. Both are
    /// [`crate::stopping::Halting::headline`] and [`advice`](crate::stopping::Halting::advice) — one
    /// derivation, two consumers, nothing for the two to word differently.
    pub halting: Option<crate::stopping::Halting>,
}

/// The label on the held-pads row. A constant because two tests derive their expectations from it rather
/// than retyping it, and a row whose label a test pins by hand is a row that can be renamed to nothing.
pub const HELD_LABEL: &str = "held by a client";

/// The label on the Aether row, a constant for [`HELD_LABEL`]'s reason.
pub const AETHER_LABEL: &str = "aether";

/// The label on the halting row. [`crate::stopping::HALTING_LABEL`] re-exported by use rather than
/// re-spelled, because the derivation module owns the vocabulary.
pub use crate::stopping::HALTING_LABEL;

impl StatusStrip {
    /// Derived from the machine, by the same expressions `Engine::status` uses. One derivation, two
    /// consumers.
    /// `held` is [`crate::bus::Bus::held_pads`], or `None` when the caller has no bus to ask — see
    /// [`held`](Self::held) for why that is a different row and not an absent one.
    pub fn of(
        machine: &Machine,
        rom_path: &str,
        symbols: Option<&SymbolTable>,
        held: Option<[Pad; 2]>,
        aether: Option<crate::bus::AetherStatus>,
        halting: Option<crate::stopping::Halting>,
    ) -> Self {
        let sys = machine.system();
        Self {
            halting,
            rom_path: oracle_aether::engine::absolutise(rom_path),
            rom_bytes: sys.rom().len(),
            frame: sys.scheduler().now() / oracle_core::system::MCLK_PER_FRAME,
            frames_run: machine.frames(),
            symbol_count: symbols.map(|t| t.len()),
            symbol_at_pc: symbols
                .and_then(|t| oracle_aether::engine::symbol_at(t, sys.cpu_regs().pc)),
            held,
            aether,
        }
    }

    /// **The Aether row — always present, unlike [`held_row`](Self::held_row)**, and the difference is the
    /// decision.
    ///
    /// `held_row` draws nothing when nothing is held, because a permanent `held by a client   (none)` is a
    /// line every reader learns to skip. This row is the opposite case: the state it most needs to report
    /// is the *quiet* one — a window nothing can attach to — and that is precisely the state an
    /// only-when-interesting row would render as blank space. **An absence is not a statement**; the
    /// defect `PLAYER-SERVE` inherits from `oracle-frontend` (a launch that said nothing about a bus that
    /// was off, and an owner who went twice to a window that could not be attached to) is exactly what a
    /// row that hid itself would reproduce inside the window.
    ///
    /// The sentence is [`crate::bus::ServeOutcome::sentence`]'s — the same string the launch line prints —
    /// so the terminal and the window cannot describe this window's bus differently.
    pub fn aether_row(&self) -> (&'static str, String) {
        (
            AETHER_LABEL,
            match &self.aether {
                None => "NOT MEASURED — this strip was built with no bus to ask".into(),
                Some(o) => o.sentence(),
            },
        )
    }

    /// **The held-pads row, or `None` when there is nothing to say** (design §9.4).
    ///
    /// Three outcomes, and the middle one is the reason this is not a `bool`:
    ///
    /// * `None` for the field → a loud row. Nothing asked the bus, and an empty set is not the answer to a
    ///   question nobody put.
    /// * asked, nothing held → **no row at all**. §9.4 specifies the field is shown only when non-empty,
    ///   and a permanent `held by a client   (none)` is a line every reader learns to skip, which is how a
    ///   row that matters one day in a hundred stops being read on that day.
    /// * asked, something held → the row, naming its own subject in words and naming the remedy.
    ///
    /// **The button names are [`oracle_aether::engine::held_names`]'s**, which is the function
    /// `emulator/hold`'s reply `held` array is built from. A panel that spelled `left`/`start` for itself
    /// would be a second vocabulary for one fact.
    ///
    /// The wording is deliberately a sentence rather than a bare `left`. §9.4's whole complaint is that a
    /// human watching their character walk left forever has nothing that tells them *why*, and a lens that
    /// is perfectly correct and explains nothing is a lens that gets asked "what are the purple boxes".
    /// `emulator/release_all` is named because §9.4 says the remedy "is one call, but you have to know to
    /// make it" — so the field is where you learn it.
    pub fn held_row(&self) -> Option<(&'static str, String)> {
        let Some(pads) = self.held else {
            return Some((
                HELD_LABEL,
                "NOT MEASURED — this strip was built with no bus to ask".into(),
            ));
        };
        let by_port: Vec<String> = pads
            .iter()
            .enumerate()
            .filter_map(|(port, pad)| {
                let names = oracle_aether::engine::held_names(pad);
                (!names.is_empty()).then(|| format!("port {port}: {}", names.join(", ")))
            })
            .collect();
        if by_port.is_empty() {
            return None;
        }
        Some((
            HELD_LABEL,
            format!(
                "{} — a bus client is holding these, not you; emulator/release_all clears them",
                by_port.join(" · ")
            ),
        ))
    }

    /// **The halting row, or `None` when there is nothing to say** (`ARMED-STATE-VISIBLE`).
    ///
    /// Three outcomes, and they are [`held_row`](Self::held_row)'s three, one instrument over:
    ///
    /// * `None` for the field → a loud row. Nothing asked the bus, and "nothing is armed" is not the
    ///   answer to a question nobody put.
    /// * asked, nothing armed, nothing stopped, nothing ever halted → **no row at all**. A permanent
    ///   `armed to halt   (nothing)` is a line every reader learns to skip, which is how the one day in a
    ///   hundred it says something else gets skipped too.
    /// * anything else → the row: what is armed, whether the machine is stopped and whether a breakpoint
    ///   is why, how many times it has halted, and **the way out, naming the calls**.
    ///
    /// **It is the same two sentences the transport bar draws.** The bar shows the headline and hides the
    /// advice behind a hover; here both are on the glass, because a strip has the width and a bar does
    /// not. Neither surface writes a word of its own.
    pub fn halt_row(&self) -> Option<(&'static str, String)> {
        let Some(h) = &self.halting else {
            return Some((
                HALTING_LABEL,
                "NOT MEASURED — this strip was built with no bus to ask".into(),
            ));
        };
        let head = h.headline()?;
        match h.advice() {
            Some(a) => Some((HALTING_LABEL, format!("{head} — {a}"))),
            None => Some((HALTING_LABEL, head)),
        }
    }

    /// The strip as label/value pairs, in display order.
    ///
    /// **Nothing here is ever blank and nothing unmeasurable is ever a `0`.** Each of the three absences
    /// below — no listing, a listing that names nothing at this PC, and a symbol landing exactly on the
    /// PC — is a different fact, and each gets its own sentence.
    ///
    /// # ⚑ [`held_row`](Self::held_row) is FIRST, and the position is the decision
    ///
    /// Every other row here answers *what is loaded and where is the machine* — questions a reader came
    /// with. The held row answers *is this window still doing what my hands tell it*, which is a question
    /// a reader has not thought to ask and is the reason they are staring at the strip in the first place.
    /// A row that only ever appears when something is wrong belongs where an alarm belongs, above the
    /// steady state, not appended after six rows a reader has already learned to skim past.
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        // ⚑ The halting row is FIRST, ahead of even the held row, and the order is the ranking of alarms.
        // A held pad makes the game do something you did not ask for; a halted machine makes it do
        // NOTHING, which is the state a reader cannot diagnose at all from the picture. It goes at the
        // top for `held_row`'s reason, more so.
        let mut rows: Vec<(&'static str, String)> =
            self.halt_row().into_iter().chain(self.held_row()).collect();
        rows.extend([
            ("romPath", self.rom_path.clone()),
            ("rom bytes", format!("{}", self.rom_bytes)),
            ("frame (emulated)", format!("{}", self.frame)),
            ("frames run (player)", format!("{}", self.frames_run)),
            (
                "symbols",
                match self.symbol_count {
                    None => "none loaded (no --symbols, and no .lst beside the ROM)".into(),
                    Some(n) => format!("{n} loaded"),
                },
            ),
            (
                "symbol at pc",
                match (&self.symbol_at_pc, self.symbol_count) {
                    (Some((name, 0)), _) => name.clone(),
                    (Some((name, disp)), _) => format!("{name}+${disp:X}"),
                    (None, None) => "— no listing loaded".into(),
                    // A table that resolves nothing at this address is a real answer and a different one:
                    // the listing is there and the PC is before its first symbol (or past its end).
                    (None, Some(_)) => "— the listing names no symbol at or before pc".into(),
                },
            ),
        ]);
        // **Last, and the position is a decision too.** Every row above answers *what is loaded and where
        // is the machine* — facts about the emulated system. This one answers *can anything outside this
        // process reach this window*, which is a fact about the process, so it sits after the machine
        // rather than among it. It is not in the alarm slot `held_row` occupies because it is not an
        // alarm: it is always true of something, and it is always shown.
        rows.push(self.aether_row());
        rows
    }
}

/// The starting layout: the screen on the left, Pacing over Registers on the right, and Memory tabbed
/// beside Registers — the two panels a debugger reads together, in one pane, so a human is not choosing
/// between "where is the PC" and "what is at that address".
///
/// **This is now the *fallback*, not the layout.** Since the layout-persistence parcel the window opens on
/// whatever the user last arranged, and reaches this function only on a first run or when a stored layout
/// is refused — a version mismatch, or bytes that will not decode. [`crate::layout`] holds that decision
/// and the reasoning; the short version is that a layout is discarded wholesale rather than migrated,
/// because a `DockState<Tab>` carries the [`Tab`] variant *names* and an unknown one costs the whole tree.
///
/// The cost of turning it on was the two feature flags design §9.2 predicted (`eframe/persistence` and
/// `egui_dock/serde`) rather than the one this comment used to claim, plus `ron` entering the lock file.
pub fn initial_dock() -> egui_dock::DockState<Tab> {
    let mut dock = egui_dock::DockState::new(vec![Tab::Screen]);
    let surface = dock.main_surface_mut();
    let [_, right] = surface.split_right(egui_dock::NodeIndex::root(), 0.68, vec![Tab::Pacing]);
    let [inspect, _] =
        surface.split_below(right, 0.45, vec![Tab::Registers, Tab::Memory, Tab::Objects]);
    // **The three stopping tabs get a pane of their own rather than a sixth, seventh and eighth title in
    // the pane above.** They are one subject — *what will halt this machine, and what has it seen?* — and
    // a human arming a breakpoint is usually about to watch the profiler or the hit log react to it. Six
    // titles in one narrow pane would also make the Registers pane's tab bar the widest thing in the
    // window, which is the layout answering a question nobody asked.
    surface.split_below(
        inspect,
        0.5,
        vec![Tab::Breakpoints, Tab::Watchpoints, Tab::Profiler],
    );
    dock
}

/// **Every tab in a leaf of its own** — the arrangement the panel-cost measurement runs under, and not a
/// layout for a human.
///
/// `egui_dock` draws only the *active* tab of a leaf, so a bench run against [`initial_dock`] executes one
/// panel body out of the three that share a pane and reports it as the cost of adding three. That is a
/// measurement of the arrangement rather than of the panels. This function puts all eight in their own
/// leaves, so every body runs on every frame: the worst case a user could arrange, and the only
/// arrangement in which measuring N panels measures N panels.
///
/// Reachable only through `--dock every-tab`, which the window mode also honours — a flag whose effect a
/// human cannot see for themselves is a flag whose effect is asserted rather than shown.
pub fn every_tab_dock() -> egui_dock::DockState<Tab> {
    let mut rest = Tab::ALL.iter().copied();
    let first = rest.next().expect("Tab::ALL is never empty");
    let mut dock = egui_dock::DockState::new(vec![first]);
    let surface = dock.main_surface_mut();
    let mut at = egui_dock::NodeIndex::root();
    // Alternating right/below, so eight leaves stay roughly square rather than becoming eight slivers in
    // one direction — a leaf too thin to lay out is a leaf whose body egui may skip.
    for (i, tab) in rest.enumerate() {
        let [_, next] = if i % 2 == 0 {
            surface.split_right(at, 0.5, vec![tab])
        } else {
            surface.split_below(at, 0.5, vec![tab])
        };
        at = next;
    }
    dock
}

// ---------------------------------------------------------------------------------------------------
// ⚑ The transport bar — a CONTROL, not a tab
// ---------------------------------------------------------------------------------------------------

/// The three gestures the bar makes.
///
/// **Named as methods, not as verbs**, because the method name is the whole of what the bar knows. It does
/// not model "pausing"; it asks a registry entry a question and shows the reply. Constants rather than
/// literals at the call sites so the test that checks them against the engine's `METHODS` registry is
/// checking *these* strings and not a second copy of them.
pub const PAUSE: &str = "emulator/pause";
pub const RESUME: &str = "emulator/resume";
pub const STEP: &str = "emulator/step";

/// **The bar's own labels, as constants**, because [`crate::screen`] reports the bar over
/// `emulator/screen_text` and a label written twice is a window and a tool describing one button
/// differently. Nothing here is a *copy* of what the bar draws — these are the strings the bar draws.
pub const PAUSE_LABEL: &str = "⏸ pause";
pub const RESUME_LABEL: &str = "▶ resume";
pub const STEP_LABEL: &str = "⏭ step";

/// The name in the top bar's left corner, and the window manager's title for this window
/// (`ViewportBuilder::with_title` in `main.rs`). One string for both, so the title `screen_text` reports
/// and the label a human reads cannot become two different names for one program.
pub const APP_NAME: &str = "oracle-player";

/// One answer the bus gave a transport gesture, kept for display until the next one replaces it.
///
/// The `text` is the **server's own words**, assembled from `code` and `message` and nothing else — no
/// wording of ours anywhere in it. That is the rule `crate::bus`'s [`Answer`](crate::bus::Answer) doc
/// states: *"a refusal a panel writes for itself is a sentence about a server, not the server's."*
pub struct Echo {
    /// The method that was called. Shown so a human can tell which button produced the line.
    pub method: &'static str,
    /// `"<code> <message>"` for a refusal, or the compact reply for a success. Verbatim either way.
    pub text: String,
    /// `error.data.reason` — the machine-readable discriminant, shown *as* a discriminant. `None` on
    /// success, and also on a refusal that carried no reason, which is a distinction worth seeing.
    pub reason: Option<String>,
    /// Whether this was a refusal. **This is what the bar colours on**, never the shape of `text`: a
    /// refusal that reads like a success is the one rendering mistake a debug surface cannot afford.
    pub refused: bool,
}

impl Echo {
    /// **The line the bar puts on the glass for this answer**, and the only place it is spelled.
    ///
    /// [`Transport::bar`] passes this to `ui.colored_label` and hands the same string to
    /// [`crate::screen`], so a client reading `emulator/screen_text` gets the characters a human is
    /// looking at rather than a second rendering of the same `Echo`. The colour is not in it — colour is
    /// not text, and `refused` is the field that carries it.
    pub fn line(&self) -> String {
        let reason = match &self.reason {
            Some(r) => format!(" [{r}]"),
            None => String::new(),
        };
        format!("{}: {}{}", self.method, self.text, reason)
    }
}

/// The transport bar's state between repaints: the last answer, and nothing else.
///
/// It deliberately holds **no pause flag**. The play/pause button reads `Bus::is_paused()` every frame,
/// which is the bus's own truthful reading (it consults `pending_free_run`, which a `call` does not
/// apply). A cached copy here would be a second belief about the run state — the drift R2 exists to
/// prevent, in the one place a human would read it.
#[derive(Default)]
pub struct Transport {
    pub last: Option<Echo>,
}

impl Transport {
    /// Draw the bar and issue whatever the human clicked.
    ///
    /// **Every gesture goes through `Host::call`.** The alternative — reaching past the bus to flip a flag
    /// the player also owns — is what makes a debug surface and its tool disagree, and it is specifically
    /// what puts a *second* pause state in this process (R2). Going through the registry also means the
    /// bar inherits every refusal the tool already knows how to give, including ones nobody here
    /// anticipated: `emulator/step` against a free-running machine is refused `-32005 machineRunning` by
    /// `require_stopped`, and that sentence is the server's, arrives here whole, and is shown whole.
    /// **Returns the [`screen::Run`]s it just drew**, for `emulator/screen_text` (§11.29). Handing them
    /// back is deliberately not the same as offering a helper both this and [`crate::screen`] could call:
    /// there is no second expression to drift, and the snapshot cannot exist before the bar has drawn it,
    /// which is the ordering `Host::set_screen_text` demands of its caller.
    pub fn bar(
        &mut self,
        ui: &mut egui::Ui,
        machine: &mut Machine,
        bus: &mut Bus,
        symbols: Option<&SymbolTable>,
    ) -> Vec<screen::Run> {
        // The bus's reading, every frame, never a field of ours.
        let paused = bus.is_paused();
        let mut drew = Vec::new();

        // ⚑ Pause and resume are ONE button, because they are one question ("is it running?") and two
        // buttons would let a human ask for the state it is already in — whose honest answer from the
        // tool is a success that changes nothing, which reads as a broken button.
        let (label, method) = Self::toggle(paused);
        if ui.button(label).clicked() {
            self.issue(machine, bus, method);
        }
        drew.push(screen::Run::label(label));

        // Step is offered unconditionally, and while running it is REFUSED rather than hidden. A hidden
        // button teaches nothing; the refusal names the state and the remedy in the tool's own words, and
        // it is the same sentence a socket client gets for the same mistake.
        if ui.button(STEP_LABEL).clicked() {
            self.issue(machine, bus, STEP);
        }
        drew.push(screen::Run::label(STEP_LABEL));

        // --- ⚑ ARMED-STATE-VISIBLE: the halting alarm and the way out. ---
        //
        // **It is on the TOP BAR and not in a tab, and that is the whole repair.** The status strip says
        // the same thing at more length, but the strip lives inside the Registers tab, and `egui_dock`
        // draws only each leaf's active tab — so a reader staring at a frozen Screen tab cannot see it.
        // The incident was a human looking at a window that had halted and finding nothing that said so;
        // an alarm behind another tab title reproduces it exactly.
        //
        // The button beside it is a **control, not a tab** (the standing ruling: things you look at are
        // tabs, things you *do* are controls), and it goes through `Host::call` like every other gesture
        // on this bar, one call per handle, so each refusal is the handler's own.
        let halting = {
            let (watch, _, _) = bus.read_instruments();
            stopping::Halting::of(
                bus.read_breakpoints(),
                bus.last_break(),
                watch,
                paused,
                machine.system().scheduler().now() / oracle_core::system::MCLK_PER_FRAME,
                symbols,
            )
        };
        let mut release: Option<Vec<(&'static str, Value)>> = None;
        if let Some(head) = halting.headline() {
            ui.separator();
            // Coloured on the *derivation*, never on the shape of the string: an alarm that reads its own
            // text back to decide how loud to be is one refactor away from being quiet.
            let colour = if halting.halted_here() {
                ui.visuals().error_fg_color
            } else if halting.can_halt() {
                ui.visuals().warn_fg_color
            } else {
                ui.visuals().weak_text_color()
            };
            let advice = halting.advice().unwrap_or_default();
            ui.colored_label(colour, &head).on_hover_text(&advice);
            drew.push(screen::Run::after_sep(head));
            if halting.can_halt() {
                let (label, _) = halting.release_label();
                if ui.button(label).on_hover_text(&advice).clicked() {
                    release = Some(stopping::release_gestures(&halting));
                }
                drew.push(screen::Run::label(label));
            }
        }
        if let Some(gestures) = release {
            self.issue_all(machine, bus, &gestures);
        }

        // **What is RECORDING**, read from the instruments the loop itself feeds — one count, not a list,
        // because the lists are the three stopping tabs. It belongs on the transport bar rather than in a
        // tab for the same reason the buttons do: a human reaching for "step" needs to know what is
        // riding the run. Read through [`Bus::read_instruments`], which is the same borrow
        // `emulator/watchpoint_hits` answers from — there is one instrument, so the bar and a client
        // cannot disagree about how many watches exist.
        //
        // ⚑ **It used to be captioned "what is armed to stop this machine", and it was not.** Neither of
        // these two can stop the running game in this window: `Engine::run_sinks` lends both wrapped in
        // `Observe`, precisely so a watch's level-triggered `stopAfter` cannot end every 1-frame run the
        // player makes. The one thing that *can* halt the game is a breakpoint, which this line never
        // mentioned — so on the night the window froze, the bar's "armed" summary was describing the two
        // instruments that were innocent. The halting alarm above is the other half; this is now only
        // ever about recording, and it says so.
        let (watch, _, profiler_armed) = bus.read_instruments();
        let watches = watch.watch_count();
        if let Some(recording) = Self::recording(watches, profiler_armed) {
            ui.separator();
            ui.weak(&recording);
            drew.push(screen::Run::after_sep(recording));
        }

        if let Some(e) = &self.last {
            ui.separator();
            let colour = if e.refused {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().weak_text_color()
            };
            let line = e.line();
            ui.colored_label(colour, &line).on_hover_text(
                "the bus's own reply, verbatim. The bracketed word is `error.data.reason`, the \
                 discriminant clients branch on — never the message text.",
            );
            drew.push(screen::Run::after_sep(line));
        }
        drew
    }

    /// **The play/pause button's label and the method it issues**, for a given run state.
    ///
    /// A function rather than an expression inside [`bar`](Self::bar) because [`crate::screen`] reports
    /// the label over the wire, and the pair must move together: a label that said `pause` while the
    /// button issued `emulator/resume` is the one defect a readback of the bar could not distinguish from
    /// a correct window.
    pub fn toggle(paused: bool) -> (&'static str, &'static str) {
        if paused {
            (RESUME_LABEL, RESUME)
        } else {
            (PAUSE_LABEL, PAUSE)
        }
    }

    /// **What is RECORDING on this machine**, or `None` when the bar draws nothing there.
    ///
    /// `None` rather than an empty string, for [`StatusStrip::held_row`]'s reason one screen up: a row
    /// that is *absent* and a row that is *blank* are different facts on the glass, and `screen_text`
    /// must not make them one artifact.
    ///
    /// ⚑ **Renamed from `armed`, and the rename is the correction.** Neither a watch nor the profiler can
    /// halt the running game in this window — `Engine::run_sinks` lends both wrapped in `Observe` on
    /// purpose — so a line captioned *what is armed to stop this machine* that listed exactly these two
    /// was naming the innocent parties. What can stop it is a breakpoint, and that is
    /// [`crate::stopping::Halting`]. The word `recording` is now in the string itself, so the bar and its
    /// `screen_text` readback both say which question this answers.
    pub fn recording(watches: usize, profiler_armed: bool) -> Option<String> {
        (watches > 0 || profiler_armed).then(|| {
            format!(
                "recording: {watches} watch{} · profiler {}",
                if watches == 1 { "" } else { "es" },
                if profiler_armed { "on" } else { "off" }
            )
        })
    }

    /// **A whole gesture that is more than one call** — the release sequence, and nothing else today.
    ///
    /// Each pair is dispatched through `Host::call` on its own and judged by its own handler. The loop
    /// **stops at the first refusal** and keeps that answer, in the server's words and shape, exactly as
    /// [`issue`](Self::issue) does for a single call: a sequence that pressed on past a refusal would be
    /// this panel deciding that a partial result counts as success, which is the one judgement it is not
    /// allowed to make.
    ///
    /// There is no summary sentence of ours. The bar's own halting line is re-derived on the very next
    /// repaint from the set the calls just changed, so *how far it got* is visible as state rather than
    /// asserted as prose.
    fn issue_all(
        &mut self,
        machine: &mut Machine,
        bus: &mut Bus,
        gestures: &[(&'static str, Value)],
    ) {
        for (method, params) in gestures {
            let answer = bus.call(machine.system_mut(), method, params);
            let refused = answer.is_err();
            self.last = Some(Echo {
                method,
                refused,
                reason: answer.reason().map(str::to_string),
                text: match &answer {
                    crate::bus::Answer::Ok(v) => format!("ok {v}"),
                    crate::bus::Answer::Err(e) => format!("{} {}", e.code, e.message),
                },
            });
            if refused {
                return;
            }
        }
    }

    /// Make one call and keep its answer.
    ///
    /// A method the registry does not carry would come back `-32601` and be shown like any other refusal;
    /// the explicit check exists so a *typo in this file* is caught by the test below rather than by a
    /// human clicking a button that can only ever fail.
    fn issue(&mut self, machine: &mut Machine, bus: &mut Bus, method: &'static str) {
        let answer = bus.call(machine.system_mut(), method, &json!({}));
        self.last = Some(Echo {
            method,
            refused: answer.is_err(),
            reason: answer.reason().map(str::to_string),
            text: match &answer {
                // The reply bodies here are small (`emulator/step` carries the new pc); shown compactly
                // rather than summarised, so nothing of the server's answer is dropped on the way.
                crate::bus::Answer::Ok(v) => format!("ok {v}"),
                crate::bus::Answer::Err(e) => format!("{} {}", e.code, e.message),
            },
        });
    }
}

#[cfg(all(test, unix))]
mod transport_tests {
    use super::*;
    use crate::machine::Machine;

    fn rig() -> (Machine, Bus) {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let bus = Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo::default(),
            false,
            None,
        );
        (machine, bus)
    }

    /// ★ **The label and the method move together**, which is the pair `emulator/screen_text` makes
    /// readable and therefore the pair that can now be *wrong in public*.
    ///
    /// A bar whose button said `⏸ pause` while it issued `emulator/resume` is a window lying to a human
    /// and, since `PLAYER-SCREEN-TEXT`, to every client reading the glass. [`Transport::toggle`] is the one
    /// place the pairing exists, so this is the one place it can be checked.
    ///
    /// **⚑ The third assertion.** The two rows above are agreement, not correctness: a `toggle` that
    /// ignored its argument and returned one constant satisfies "the label matches the method" perfectly.
    /// So the two run states are asserted to produce **different** answers — against the raw input, not
    /// against each other.
    #[test]
    fn the_transport_label_and_the_method_it_issues_are_one_decision() {
        assert_eq!(Transport::toggle(false), (PAUSE_LABEL, PAUSE));
        assert_eq!(Transport::toggle(true), (RESUME_LABEL, RESUME));
        assert_ne!(
            Transport::toggle(false),
            Transport::toggle(true),
            "the agreement above is two copies of the same untouched value: a `toggle` that ignored the \
             run state would satisfy both rows and offer `pause` to a stopped machine forever"
        );
        assert!(
            !PAUSE_LABEL.contains(RESUME_LABEL) && !RESUME_LABEL.contains(PAUSE_LABEL),
            "the two labels must stay distinguishable by substring — the wire test in `main.rs` reads \
             them out of one composed line and asserts one is present and the other is not"
        );
    }

    /// **The armed summary is ABSENT when nothing is armed, never blank.**
    ///
    /// `None` and `Some(String::new())` reach the glass as the same nothing, and reach
    /// `emulator/screen_text` as the same nothing too — but only one of them is honest, and the difference
    /// is the one [`StatusStrip::held_row`] already turns on. The plural and the profiler word are checked
    /// because both are *derived* from the counts rather than fixed, and a summary that said
    /// `1 watches` is the kind of wrong that survives review forever.
    #[test]
    fn the_recording_summary_is_absent_when_nothing_records_and_never_claims_to_stop_the_machine() {
        assert_eq!(
            Transport::recording(0, false),
            None,
            "a bar with nothing recording draws no summary at all"
        );
        assert_eq!(
            Transport::recording(1, false).as_deref(),
            Some("recording: 1 watch · profiler off")
        );
        assert_eq!(
            Transport::recording(2, true).as_deref(),
            Some("recording: 2 watches · profiler on")
        );
        // The profiler alone is enough to draw the row — the condition is an `or`, and a `>0` on watches
        // alone would hide the one fact a human arming a profiler most wants confirmed.
        assert_eq!(
            Transport::recording(0, true).as_deref(),
            Some("recording: 0 watches · profiler on")
        );
        // ⚑ **The caption is the correction, so it is pinned.** This line used to be captioned
        // *what is armed to stop this machine*, and neither of the two things it counts can stop the
        // running game in this window — `Engine::run_sinks` lends both wrapped in `Observe`. A summary
        // that says or implies "stop" here sends a reader hunting the wrong instrument on the night the
        // window freezes, which is what happened.
        for s in [
            Transport::recording(3, true).unwrap(),
            Transport::recording(0, true).unwrap(),
        ] {
            assert!(s.starts_with("recording:"), "{s}");
            assert!(
                !s.contains("stop") && !s.contains("halt") && !s.contains("armed"),
                "the recording summary must not claim to be about stopping the machine: {s}"
            );
        }
    }

    /// **The echo line is spelled once**, and it carries the reason as a *discriminant* rather than prose.
    ///
    /// The third assertion is the `assert_ne!` against the reason-less line: without it, an
    /// [`Echo::line`] that dropped `reason` entirely would pass every "contains" check above it.
    #[test]
    fn the_echo_line_carries_the_servers_own_words_and_its_reason_in_brackets() {
        let plain = Echo {
            method: PAUSE,
            text: String::from("ok {}"),
            reason: None,
            refused: false,
        };
        assert_eq!(plain.line(), "emulator/pause: ok {}");

        let refused = Echo {
            method: STEP,
            text: String::from("-32005 the machine is running"),
            reason: Some(String::from("machineRunning")),
            refused: true,
        };
        assert_eq!(
            refused.line(),
            "emulator/step: -32005 the machine is running [machineRunning]"
        );
        assert_ne!(
            refused.line(),
            format!("{}: {}", refused.method, refused.text),
            "the agreement above is two copies of the same untouched value: the reason vanished from \
             the line a human and a client both read"
        );
    }

    /// **Every button names a method the registry actually carries.**
    ///
    /// A typo here produces a button whose only possible outcome is `-32601`, which the bar would render
    /// perfectly correctly and which a human would read as "the emulator is broken". Checked against
    /// `METHODS` — the same slice `emulator/initialize` builds its advertised list from — rather than
    /// against a second list here, so there is nothing for the two to drift apart from.
    ///
    /// **The alternative green path, ruled out:** `is_served` returning `true` unconditionally would pass
    /// the loop above and prove nothing, so a name that must NOT be served is checked in the same test.
    #[test]
    fn every_transport_button_names_a_served_method() {
        for m in [PAUSE, RESUME, STEP] {
            assert!(
                memory::is_served(m),
                "the transport bar offers {m}, which the engine's METHODS registry does not carry — that \
                 button can only ever produce -32601"
            );
        }
        assert!(
            !memory::is_served("emulator/pause_but_spelled_wrong"),
            "`is_served` answered true for a method that cannot exist, so the loop above witnesses \
             nothing"
        );
    }

    /// ★ **The refusal is the server's, and the bar branches on `reason`, not on prose.**
    ///
    /// `emulator/step` against a free-running player is refused by `require_stopped` with
    /// `-32005 machineRunning`. This drives the bar's own `issue` — the code path a click takes — and
    /// checks that the discriminant survives to the [`Echo`] intact.
    ///
    /// **Two alternative green paths, both ruled out here:**
    ///
    /// 1. *The bar composes its own refusal and it happens to say the same thing.* Ruled out by asserting
    ///    the echoed text contains the handler's own numeric code, which nothing in `ui.rs` writes.
    /// 2. *`reason` is `Some` for everything, so matching it proves nothing.* Ruled out by the second half:
    ///    the very next gesture succeeds, and its echo must carry `reason == None` and `refused == false`.
    #[test]
    fn step_is_refused_by_the_tool_while_the_player_runs_and_taken_once_it_is_paused() {
        let (mut machine, mut bus) = rig();
        let mut t = Transport::default();

        // The arrangement stated as a fact rather than assumed: an un-paused player IS a free-running bus.
        assert!(
            !bus.is_paused(),
            "the fixture must begin free-running or the refusal below is not the one being tested"
        );

        t.issue(&mut machine, &mut bus, STEP);
        let e = t.last.as_ref().expect("a gesture leaves an echo");
        assert!(
            e.refused,
            "a step against a running machine must be refused"
        );
        assert_eq!(
            e.reason.as_deref(),
            Some("machineRunning"),
            "the bar must carry the tool's own discriminant: {}",
            e.text
        );
        assert!(
            e.text.contains("-32005"),
            "the echoed text must be the server's, and the code is the part no panel writes: {}",
            e.text
        );

        // …and the same button, once the machine is stopped, is taken.
        t.issue(&mut machine, &mut bus, PAUSE);
        assert!(
            bus.is_paused(),
            "`emulator/pause` through Host::call must move the bus's own reading"
        );
        let pc_before = machine.system().cpu_regs().pc;
        t.issue(&mut machine, &mut bus, STEP);
        let e = t.last.as_ref().expect("a gesture leaves an echo");
        assert!(
            !e.refused,
            "a step against a stopped machine must be taken: {}",
            e.text
        );
        assert_eq!(
            e.reason, None,
            "a success carries no reason — if it did, matching on `reason` would be meaningless"
        );
        // The third assertion: an `ok` that moved nothing would satisfy everything above.
        assert_ne!(
            machine.system().cpu_regs().pc,
            pc_before,
            "the step reported success without advancing the machine, so `Host::call` answered for the \
             engine's placeholder rather than for this machine"
        );
    }

    /// **The bar holds no pause flag of its own** (R2), so a resume issued through the tool is visible to
    /// the bar on the very next read.
    ///
    /// The alternative green path — a `Transport` that cached the state and happened to be right — is
    /// ruled out structurally: `Transport` has one field and it is the echo. Asserted anyway on the value
    /// the button label is chosen from, because that is the thing a human sees.
    #[test]
    fn the_bars_label_follows_the_bus_and_not_a_cached_flag() {
        let (mut machine, mut bus) = rig();
        let mut t = Transport::default();
        assert!(!bus.is_paused());
        t.issue(&mut machine, &mut bus, PAUSE);
        assert!(bus.is_paused(), "pause must land");
        t.issue(&mut machine, &mut bus, RESUME);
        assert!(
            !bus.is_paused(),
            "resume must land too — a one-way transport is worse than none"
        );
    }
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

    /// A bus for a machine with **nothing armed** — the state every one of these parity fixtures is in.
    ///
    /// Parcel 3 put the bus into `Machine::step`, so the two tests below now run their frames *through the
    /// seam*. That is not incidental: each of them already compares `machine.system().state_hash()`
    /// against a plain `sys.run_frames()` of the same count, and that comparison is now also the proof
    /// that **an unarmed seam does not perturb the machine** — three `None` sinks, a bare `Fanout`, and a
    /// byte-identical timeline. Had the wrappers changed a single cycle, both tests would go red here.
    fn idle_bus(machine: &mut Machine) -> Bus {
        Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo::default(),
            false,
            None,
        )
    }

    /// **A real, quiet [`crate::stopping::Halting`]** for a fixture with a bus and nothing armed.
    ///
    /// `None` for that field is the *loud* "no bus to ask" row, not the quiet one — so a fixture that
    /// passed `None` because it did not care would be asserting against a strip carrying an alarm. Every
    /// strip built in this module has a bus, so every one of them asks it.
    fn quiet_halting(machine: &Machine, bus: &Bus) -> Option<crate::stopping::Halting> {
        let (watch, _, _) = bus.read_instruments();
        Some(crate::stopping::Halting::of(
            bus.read_breakpoints(),
            bus.last_break(),
            watch,
            bus.is_paused(),
            machine.system().scheduler().now() / oracle_core::system::MCLK_PER_FRAME,
            None,
        ))
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

    /// The status strip against `emulator/status`, through the same `Host::call`.
    ///
    /// **It calls [`StatusStrip::of`], not the expressions inside it.** Comparing hand-written copies of
    /// those expressions to the bus would check that *this test* agrees with the bus and leave the panel's
    /// own derivation untested — the shape of a control that measures something other than the thing it
    /// names. So the panel side is a real [`Machine`] and the bus side is a `System` **proved to be the
    /// same machine by its state hash** before anything is compared.
    ///
    /// **`romPath` is compared now, and parcel 2a's reason for not comparing it is gone.** 2a said the
    /// absolutiser was "a private helper", so the strip could only show the `--rom` argument verbatim and
    /// claiming parity would have been claiming a normalisation the panel did not perform. Parcel 2b
    /// published [`oracle_aether::engine::absolutise`] (§11.30 / CR-I: reporting an absolute path is a
    /// property of *every* reply field carrying one), and the fixture below makes the check
    /// non-vacuous — the path it passes in is one `canonicalize` visibly changes, so a strip that
    /// skipped the call could not accidentally agree.
    #[test]
    fn the_status_strip_agrees_with_emulator_status_on_what_it_can_derive() {
        const FRAMES: u64 = 5;
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let mut idle = idle_bus(&mut machine);
        for _ in 0..FRAMES {
            machine.step([oracle_core::io::Pad::default(); 2], &mut idle);
        }
        let mut sys = booted();
        sys.set_pad(0, oracle_core::io::Pad::default());
        sys.set_pad(1, oracle_core::io::Pad::default());
        sys.run_frames(FRAMES);

        // Without this the two sides could be two different machines agreeing by luck, and every
        // assertion below would be measuring nothing.
        assert_eq!(
            machine.system().state_hash().combined,
            sys.state_hash().combined,
            "the panel's machine and the bus's machine must BE the same machine"
        );

        // A real file, named by a path `canonicalize` must rewrite — an existing directory traversed and
        // backed out of. A path that is already canonical would let a strip that never absolutised at
        // all pass this test, which is the vacuous-control shape.
        let dir = std::env::temp_dir().join(format!("oracle-player-rom-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let real = dir.join("testrom.bin");
        std::fs::write(&real, oracle_core::testrom::build()).unwrap();
        let winding = format!("{}/sub/../testrom.bin", dir.display());
        assert_ne!(
            winding,
            real.display().to_string(),
            "the fixture path must not already be canonical, or this proves nothing"
        );

        // `Some(idle.held_pads())` and not `None`: a real bus, really asked, with nothing held — so the
        // held row is absent because the fact is "nothing is held" and not because nobody looked.
        let strip = StatusStrip::of(
            &machine,
            &winding,
            None,
            Some(idle.held_pads()),
            Some(idle.aether_status()),
            None,
        );
        let mut h = Host::new(HostConfig::default());
        h.set_machine_info(oracle_aether::host::MachineInfo {
            rom_path: Some(winding.clone()),
            symbols: None,
            symbols_path: None,
        });
        let (result, _) = h.call(&mut sys, "emulator/status", &json!({}));
        let reply = result.expect("emulator/status answers");

        assert_eq!(
            strip.rom_bytes as u64,
            reply["romBytes"].as_u64().expect("romBytes is a count"),
            "the strip's `rom bytes` and the bus's `romBytes` have DRIFTED"
        );
        assert_eq!(
            strip.frame,
            reply["frameToken"].as_u64().expect("frameToken is a count"),
            "the strip's `frame (emulated)` and the bus's `frameToken` have DRIFTED"
        );
        // Both must have moved, or two zeros agreeing would read as a pass.
        assert!(strip.frame > 0 && strip.rom_bytes > 0, "the fixture ran");
        assert_eq!(strip.frames_run, FRAMES, "the player's own count");

        // ⚑ The residual parcel 2a booked, closed and checked in both directions.
        assert_eq!(
            strip.rom_path,
            reply["romPath"].as_str().expect("romPath is a string"),
            "the strip's `romPath` and the bus's `romPath` have DRIFTED"
        );
        assert_ne!(
            strip.rom_path, winding,
            "the strip showed the argument unchanged, so it did not absolutise and the agreement above \
             is two copies of the same untouched string rather than one shared normalisation"
        );

        // The no-listing half, which is still the honest answer when there is no listing.
        assert_eq!(
            reply["symbolCount"],
            json!(0),
            "the bus counts zero symbols, which is exactly the `0` the strip must not show a human"
        );
        assert_eq!(strip.symbol_count, None);
        let rows = strip.rows();
        assert_eq!(
            row(&rows, "symbols"),
            "none loaded (no --symbols, and no .lst beside the ROM)"
        );
        assert_eq!(row(&rows, "symbol at pc"), "— no listing loaded");
        // Nothing in the strip may be rendered as a bare `0` or a blank — an unmeasurable shown as a
        // number is the wrong answer this row exists to avoid.
        for (label, value) in &rows {
            assert!(!value.is_empty(), "`{label}` renders blank");
            assert_ne!(value, "0", "`{label}` renders an unmeasurable as a bare 0");
        }
    }

    fn row<'a>(rows: &'a [(&'static str, String)], key: &str) -> &'a str {
        &rows
            .iter()
            .find(|(k, _)| *k == key)
            .unwrap_or_else(|| panic!("the strip has a `{key}` row"))
            .1
    }

    /// **The symbol half, with a listing actually loaded** — the two fields parcel 2a could only render
    /// as "none loaded", now checked against the bus that resolves them.
    ///
    /// The listing is **built from the machine's own PC** rather than from a hardcoded address, so the
    /// test cannot rot when the fixture ROM's boot path moves: a symbol is planted a known displacement
    /// below wherever the PC actually is, and both `symbolAtPc` and `symbolDisp` are then predictions the
    /// bus has to reproduce. A listing with a fixed address would keep passing while resolving to
    /// nothing, which is the failure D7 records.
    #[test]
    fn the_status_strip_and_emulator_status_resolve_the_same_symbol_at_pc() {
        const DISP: u32 = 4;
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let mut idle = idle_bus(&mut machine);
        for _ in 0..5 {
            machine.step([oracle_core::io::Pad::default(); 2], &mut idle);
        }
        let mut sys = booted();
        sys.set_pad(0, oracle_core::io::Pad::default());
        sys.set_pad(1, oracle_core::io::Pad::default());
        sys.run_frames(5);
        assert_eq!(
            machine.system().state_hash().combined,
            sys.state_hash().combined,
            "the panel's machine and the bus's machine must BE the same machine"
        );

        let pc = machine.system().cpu_regs().pc;
        assert!(
            pc > DISP,
            "the fixture's PC must leave room for a symbol below it, got {pc:#X}"
        );
        let listing = format!(
            "  Symbol Table (* = unused):\n\n Boot : {:X} C |\n\n   1 symbols\n",
            pc - DISP
        );
        let table = oracle_core::symbols::SymbolTable::parse(&listing).expect("a parsable listing");

        let strip = StatusStrip::of(
            &machine,
            "testrom",
            Some(&table),
            Some(idle.held_pads()),
            Some(idle.aether_status()),
            None,
        );
        let mut h = Host::new(HostConfig::default());
        // The SAME table on both sides — one parse, two consumers. Two parses of one file would agree
        // here and would still be the arrangement D7 exists to forbid.
        h.set_machine_info(oracle_aether::host::MachineInfo {
            rom_path: Some("testrom".into()),
            symbols: Some(table.clone()),
            symbols_path: Some("testrom.lst".into()),
        });
        let (result, _) = h.call(&mut sys, "emulator/status", &json!({}));
        let reply = result.expect("emulator/status answers");

        assert_eq!(
            strip.symbol_count.map(|n| n as u64),
            reply["symbolCount"].as_u64(),
            "the strip's symbol count and the bus's `symbolCount` have DRIFTED"
        );
        let (name, disp) = strip
            .symbol_at_pc
            .clone()
            .expect("the planted symbol must resolve at the PC");
        assert_eq!(
            name,
            reply["symbolAtPc"].as_str().expect("symbolAtPc is served"),
            "the strip's symbol and the bus's `symbolAtPc` have DRIFTED"
        );
        assert_eq!(
            u64::from(disp),
            reply["symbolDisp"].as_u64().expect("symbolDisp is served"),
            "the strip's displacement and the bus's `symbolDisp` have DRIFTED"
        );
        // The prediction, not just the agreement: two sides both resolving to nothing would agree too.
        assert_eq!(name, "Boot");
        assert_eq!(disp, DISP);
        assert_eq!(
            row(&strip.rows(), "symbol at pc"),
            format!("Boot+${DISP:X}")
        );
        assert_eq!(row(&strip.rows(), "symbols"), "1 loaded");
    }

    // -------------------------------------------------------------------------------------------
    // HELD-PADS-PLAYER, half 2 — design §9.4
    // -------------------------------------------------------------------------------------------

    /// The rows the strip carries with nothing held, in order. **Derived from the strip itself** below
    /// rather than trusted: a hand-written list that had silently gone stale would make the displacement
    /// check below assert against fiction.
    ///
    /// `PLAYER-SERVE` appended [`AETHER_LABEL`] and this list grew with it — six to seven. The row is
    /// **always** present (see [`StatusStrip::aether_row`] for why it does not hide itself the way the
    /// held row does), so it belongs in the base and not in the held delta.
    const BASE_LABELS: [&str; 7] = [
        "romPath",
        "rom bytes",
        "frame (emulated)",
        "frames run (player)",
        "symbols",
        "symbol at pc",
        AETHER_LABEL,
    ];

    fn labels(rows: &[(&'static str, String)]) -> Vec<&'static str> {
        rows.iter().map(|(k, _)| *k).collect()
    }

    fn hold(bus: &mut Bus, machine: &mut Machine, params: Value) -> Value {
        match bus.call(machine.system_mut(), "emulator/hold", &params) {
            crate::bus::Answer::Ok(v) => v,
            crate::bus::Answer::Err(e) => panic!("emulator/hold refused: {} {}", e.code, e.message),
        }
    }

    /// ★ **The held row leads the strip, and displaces nothing** (§9.4).
    ///
    /// ⚑ **This asserts the WHOLE row list on every one of the three states**, not the new row alone. A
    /// test that only looked at what it added would be structurally unable to see what it pushed out, and
    /// an added row on a strip is by definition a displacement of everything below it.
    ///
    /// ⚑ **On the ordering claim's anti-vacuity clause, and where this departs from the brief.** The
    /// brief asked for "a width that drops a late field while keeping an early one". **This surface does
    /// not truncate** — [`StatusStrip::rows`] is a `Vec` and [`Panels::registers`] draws one
    /// `ui.monospace` line per row, so there is no width at which a late row is dropped and no such clause
    /// is constructible here. The vacuity it guards against is real all the same: "the new row is first"
    /// is worth nothing on a strip with one row. So the equivalent is asserted directly — the strip has
    /// six other rows, the row that *was* first is now second, and the six below the new one are
    /// byte-identical to the six the quiet strip renders.
    ///
    /// **The alternative green paths ruled out:**
    ///
    /// 1. *The row is always present and the "only when non-empty" rule is unimplemented.* Ruled out by
    ///    the quiet strip, whose labels must equal `BASE_LABELS` exactly.
    /// 2. *The row is never present, and the loud case is measuring nothing.* Ruled out by the held strip
    ///    having exactly one row more, whose label is [`HELD_LABEL`].
    /// 3. *`None` renders as "nothing held"* — the unmeasurable drawn as a measurement. Ruled out by the
    ///    third strip, which must produce a row that differs from both other states.
    #[test]
    fn the_held_row_leads_the_strip_and_displaces_nothing() {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let mut bus = idle_bus(&mut machine);
        // Advance before anything is measured, and never between the three strips below — the last
        // clause of this test forbids a bare `0` on any row, and `frame (emulated)` on an unrun machine
        // is a *measured* zero that the clause cannot tell from an unmeasurable one. Stepping between the
        // strips would instead move `frame`/`frames run` and break the displacement equality on purpose.
        for _ in 0..5 {
            machine.step([Pad::default(); 2], &mut bus);
        }

        // --- (1) asked, nothing held: no row, and the strip is exactly what it was before this parcel ---
        let quiet = StatusStrip::of(
            &machine,
            "testrom",
            None,
            Some(bus.held_pads()),
            Some(bus.aether_status()),
            quiet_halting(&machine, &bus),
        );
        let quiet_rows = quiet.rows();
        assert_eq!(
            quiet.held_row(),
            None,
            "nothing is held, so §9.4's `shown only when non-empty` means no row at all"
        );
        assert_eq!(
            labels(&quiet_rows),
            BASE_LABELS,
            "the quiet strip is no longer the strip that shipped — something was added, removed or \
             reordered on a surface where every row displaces the ones below it"
        );

        // --- (2) held: one row more, first, and nothing below it moved or changed ---
        hold(
            &mut bus,
            &mut machine,
            json!({"port": 0, "buttons": ["left", "start"]}),
        );
        let loud_rows = StatusStrip::of(
            &machine,
            "testrom",
            None,
            Some(bus.held_pads()),
            Some(bus.aether_status()),
            quiet_halting(&machine, &bus),
        )
        .rows();
        assert_eq!(
            loud_rows.len(),
            quiet_rows.len() + 1,
            "a held set must add exactly one row, not zero and not two"
        );
        assert_eq!(loud_rows[0].0, HELD_LABEL, "and it must be the FIRST row");
        // The anti-vacuity clause, adapted: "first" is a claim about an ordering, so there must be
        // something it is ahead of, and the row that used to be first must have visibly moved.
        assert_eq!(
            loud_rows[1].0, BASE_LABELS[0],
            "the row that was first is not second, so the held row did not displace it — either the \
             strip has no other rows or they were reordered"
        );
        assert!(loud_rows.len() > 1, "a one-row strip makes `first` vacuous");
        // ⚑ The whole surface, not the new field: everything below the addition must be untouched.
        assert_eq!(
            &loud_rows[1..],
            &quiet_rows[..],
            "the held row DISPLACED content — the six rows below it are no longer the six the strip \
             renders without it"
        );
        assert!(
            loud_rows[0].1.contains("left, start"),
            "the row must name the buttons, got {:?}",
            loud_rows[0].1
        );

        // --- (3) NOT measured: loud, and distinguishable from both states above ---
        // Only the HELD field is unmeasured here. The aether and halting fields are still asked,
        // deliberately: this test's subject is the held row displacing nothing, and blinding a second
        // field as well would add a row ABOVE it and make the displacement equality fail for an
        // unrelated reason.
        let blind_rows = StatusStrip::of(
            &machine,
            "testrom",
            None,
            None,
            Some(bus.aether_status()),
            quiet_halting(&machine, &bus),
        )
        .rows();
        assert_eq!(
            blind_rows[0].0, HELD_LABEL,
            "a strip with no bus to ask drew no held row at all — `unavailable` was rendered as `nothing \
             is held`, which is the one collapse this field must never make"
        );
        assert_eq!(
            &blind_rows[1..],
            &quiet_rows[..],
            "the unmeasured strip displaced content too"
        );
        assert_ne!(
            labels(&blind_rows),
            labels(&quiet_rows),
            "`no bus to ask` rendered as `nothing held` — an unmeasurable drawn as a measurement, which \
             is the one state this field must never be confused with"
        );
        assert_ne!(
            blind_rows[0].1, loud_rows[0].1,
            "`no bus to ask` renders the same sentence as a real held set"
        );

        // The strip-wide invariant, re-run over all three states: nothing blank, nothing a bare `0`.
        for rows in [&quiet_rows, &loud_rows, &blind_rows] {
            for (label, value) in rows.iter() {
                assert!(!value.is_empty(), "`{label}` renders blank");
                assert_ne!(value, "0", "`{label}` renders an unmeasurable as a bare 0");
            }
        }
    }

    // -------------------------------------------------------------------------------------------
    // ARMED-STATE-VISIBLE — the strip says what can halt this window, and it is the bar's sentence
    // -------------------------------------------------------------------------------------------

    /// ★ **The halting row leads the strip, is absent when there is nothing to say, and is loud when
    /// nobody asked.**
    ///
    /// The incident was a halted window with nothing on it that said so. The transport bar carries the
    /// primary alarm — it cannot be hidden behind a tab — and this row is the long form beside the
    /// registers a reader will be staring at while they work out what happened. Both are
    /// [`crate::stopping::Halting`]'s own two sentences, so there is nothing for them to word differently.
    ///
    /// **The alternative green paths ruled out:**
    ///
    /// 1. *The row is always drawn, so "it appears when armed" is vacuous.* Ruled out by the quiet strip,
    ///    whose labels must equal `BASE_LABELS` exactly.
    /// 2. *The row is constant.* Ruled out by requiring the armed sentence to differ from the quiet one
    ///    **and** by the `assert_ne!` against `Halting::headline` alone — the raw derivation — which
    ///    catches a `halt_row` that dropped the way out and kept the alarm.
    /// 3. *`None` (no bus to ask) renders as "nothing is armed"* — the unmeasurable drawn as a
    ///    measurement. Ruled out explicitly, exactly as the held and aether rows rule it out.
    #[test]
    fn the_halting_row_leads_the_strip_and_carries_the_same_two_sentences_the_bar_does() {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let mut bus = idle_bus(&mut machine);

        // --- (1) asked, nothing armed: no row at all. ---
        let quiet = StatusStrip::of(
            &machine,
            "testrom",
            None,
            Some(bus.held_pads()),
            Some(bus.aether_status()),
            quiet_halting(&machine, &bus),
        );
        assert_eq!(
            quiet.halt_row(),
            None,
            "nothing is armed and nothing has halted, so a permanent all-clear row would be one more \
             line a reader learns to skip"
        );
        assert_eq!(
            labels(&quiet.rows()),
            BASE_LABELS,
            "the quiet strip must be the strip that shipped"
        );

        // --- (3) NOT asked: a loud row, never silence and never "nothing is armed". ---
        let blind = StatusStrip::of(
            &machine,
            "testrom",
            None,
            Some(bus.held_pads()),
            Some(bus.aether_status()),
            None,
        );
        let (label, unmeasured) = blind
            .halt_row()
            .expect("no bus to ask is a row, not an absence");
        assert_eq!(label, HALTING_LABEL);
        assert!(
            unmeasured.contains("NOT MEASURED"),
            "an unmeasurable must say which: {unmeasured}"
        );

        // --- (2) armed: the row, first, and it is the derivation's own two sentences. ---
        let a = bus.call(
            machine.system_mut(),
            crate::stopping::BREAKPOINT_ADD,
            &crate::stopping::breakpoint_add_params("0x20E", "").expect("a hex target"),
        );
        assert!(!a.is_err(), "arming must land");
        let h = quiet_halting(&machine, &bus).expect("there is a bus");
        let head = h.headline().expect("armed says so");
        let advice = h.advice().expect("…and offers the way out");
        let armed = StatusStrip::of(
            &machine,
            "testrom",
            None,
            Some(bus.held_pads()),
            Some(bus.aether_status()),
            quiet_halting(&machine, &bus),
        );
        let rows = armed.rows();
        assert_eq!(
            rows[0].0, HALTING_LABEL,
            "a halted or armed window is the loudest thing the strip has to say, so it goes first: {:?}",
            labels(&rows)
        );
        assert_eq!(rows.len(), BASE_LABELS.len() + 1, "exactly one row more");
        assert_eq!(
            &labels(&rows)[1..],
            BASE_LABELS,
            "the halting row must displace the rest, not replace any of them"
        );
        assert!(
            rows[0].1.contains(&head) && rows[0].1.contains(&advice),
            "the strip must carry the SAME sentences the bar draws, not a second wording: {}",
            rows[0].1
        );
        // 2: not a constant, and not the headline alone.
        assert_ne!(
            rows[0].1, unmeasured,
            "an armed window rendered identically to one with no bus to ask"
        );
        assert_ne!(
            rows[0].1, head,
            "the agreement above is two copies of one untouched value: the row is the headline with the \
             way out DROPPED, which is the half the incident actually needed"
        );
    }

    // -------------------------------------------------------------------------------------------
    // PLAYER-SERVE — the window says whether anything can attach to it
    // -------------------------------------------------------------------------------------------

    /// ★ **The Aether row is ALWAYS drawn, and the quiet state is the one it exists for.**
    ///
    /// The defect this parcel inherits from `oracle-frontend` is a launch that said *nothing* when the bus
    /// was off — an absence is not a statement, and the measured cost was the owner going twice to a
    /// window nothing could attach to. A row that hid itself when there was no socket would reproduce
    /// that inside the window, so the row is unconditional and this pins it.
    ///
    /// **The alternative green paths ruled out:**
    ///
    /// 1. *The row is present but constant* — it would pass any single-state check. Ruled out by
    ///    rendering four distinct states and requiring four distinct sentences.
    /// 2. *`None` (no bus to ask) renders as "not serving"* — the unmeasurable drawn as a measurement,
    ///    the same collapse the held row must never make. Ruled out by the explicit `assert_ne`.
    /// 3. *The row exists but the launch line says something else* — two surfaces describing one window
    ///    differently, which is the whole reason the sentence is a shared function. Ruled out by
    ///    comparing the row against `Bus::announcement()` itself.
    #[test]
    fn the_aether_row_is_always_drawn_and_agrees_with_the_launch_line() {
        use crate::bus::{AetherStatus, ServeOutcome};
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let bus = idle_bus(&mut machine);

        let quiet = StatusStrip::of(
            &machine,
            "testrom",
            None,
            None,
            Some(bus.aether_status()),
            None,
        );
        let (label, off) = quiet.aether_row();
        assert_eq!(label, AETHER_LABEL);
        assert!(
            quiet.rows().iter().any(|(k, v)| (*k, v) == (label, &off)),
            "the row must actually be IN the strip, not merely derivable from it"
        );
        assert!(
            off.contains("not serving") && off.contains("--aether"),
            "the quiet state must say so and name the remedy: {off}"
        );

        // 3: the terminal and the window are the same sentence, not two descriptions of one window.
        assert_eq!(
            bus.announcement(),
            format!("{}: {off}", AETHER_LABEL),
            "the launch line and the row have drifted apart"
        );

        // 1 and 2: four states, four sentences.
        let unmeasured = StatusStrip::of(&machine, "testrom", None, None, None, None)
            .aether_row()
            .1;
        let up = StatusStrip::of(
            &machine,
            "testrom",
            None,
            None,
            Some(AetherStatus {
                outcome: ServeOutcome::Serving("/tmp/probe/s".into()),
                attached: false,
            }),
            None,
        )
        .aether_row()
        .1;
        let busy = StatusStrip::of(
            &machine,
            "testrom",
            None,
            None,
            Some(AetherStatus {
                outcome: ServeOutcome::Serving("/tmp/probe/s".into()),
                attached: true,
            }),
            None,
        )
        .aether_row()
        .1;
        assert_ne!(
            off, unmeasured,
            "`no bus to ask` rendered identically to `the bus is off` — an unmeasurable drawn as a \
             measurement"
        );
        assert!(
            unmeasured.contains("NOT MEASURED"),
            "and it must say which: {unmeasured}"
        );
        assert!(
            up.contains("/tmp/probe/s"),
            "a serving window must name the path a client is supposed to dial: {up}"
        );
        assert_ne!(
            up, busy,
            "`serving, nobody there` and `serving, someone attached` are different facts about this \
             second, and the row that explains a character walking on its own is the second one"
        );
        for a in [&off, &unmeasured, &up, &busy] {
            assert!(!a.trim().is_empty(), "no state renders as blank space");
        }
    }

    /// ★ **The row names the same buttons `emulator/hold` reports** — one vocabulary, not two (§9.4, R1).
    ///
    /// ⚑ **A parity pair is structurally blind to a defect in the derivation it shares.** The strip and
    /// the reply are both [`oracle_aether::engine::held_names`] since this parcel, so they agree *by
    /// construction* and the agreement below can witness agreement and never correctness — break
    /// `held_names` and both sides move together. The two clauses at the end are the third assertion that
    /// pair owes: the derivation is a **function of its argument** (a different held set renders a
    /// different sentence) and it is **not the constant** either degenerate implementation would give
    /// (all eight names, or none).
    ///
    /// The panel side is the **rendered row**, parsed back out, not a second call to `held_names` — a
    /// comparison between two calls of one function is an identity, not a test of the panel.
    #[test]
    fn the_held_row_names_the_same_buttons_emulator_hold_reports() {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let mut bus = idle_bus(&mut machine);

        let reply = hold(
            &mut bus,
            &mut machine,
            json!({"port": 0, "buttons": ["left", "start"]}),
        );
        let wire: Vec<&str> = reply["held"]
            .as_array()
            .expect("`held` is an array")
            .iter()
            .map(|v| v.as_str().expect("a button name"))
            .collect();
        assert!(
            !wire.is_empty(),
            "the wire reported nothing held, so the parity below would be two \
             empty lists agreeing"
        );

        let (label, value) = StatusStrip::of(
            &machine,
            "testrom",
            None,
            Some(bus.held_pads()),
            Some(bus.aether_status()),
            None,
        )
        .held_row()
        .expect("something is held, so there is a row");
        assert_eq!(label, HELD_LABEL);
        let shown = names_in(&value, 0);
        assert_eq!(
            shown,
            wire.join(", "),
            "the status strip and `emulator/hold`'s own `held` array name different buttons"
        );

        // --- the third assertion the pair owes ---
        const ALL: [&str; 8] = ["up", "down", "left", "right", "a", "b", "c", "start"];
        assert_ne!(
            shown,
            ALL.join(", "),
            "both sides named every button there is, so the agreement above is a derivation that ignores \
             its argument"
        );
        assert_ne!(
            shown, "",
            "both sides named nothing, so the agreement above is two copies of one empty list"
        );
        assert_eq!(
            shown, "left, start",
            "the names must be the two the request asked for and no others"
        );

        // …and a DIFFERENT held set must render a DIFFERENT sentence. A row that is a constant would
        // satisfy every assertion above and still tell a human nothing about their own machine.
        hold(
            &mut bus,
            &mut machine,
            json!({"port": 0, "buttons": ["left", "start"], "down": false}),
        );
        hold(&mut bus, &mut machine, json!({"port": 1, "buttons": ["c"]}));
        let other = StatusStrip::of(
            &machine,
            "testrom",
            None,
            Some(bus.held_pads()),
            Some(bus.aether_status()),
            None,
        )
        .held_row()
        .expect("port 1 holds something, so there is still a row")
        .1;
        assert_ne!(
            other, value,
            "two different held sets rendered the identical sentence — the row is a constant and the \
             agreement above is two copies of one untouched string"
        );
        assert_eq!(
            names_in(&other, 1),
            "c",
            "and it is port 1's set that is named now"
        );
        assert!(
            !other.contains("port 0"),
            "port 0 was released and must have left the row entirely, got {other:?}"
        );
    }

    /// Pull the button list back out of a rendered held row, for `port`. Parses the row the human reads
    /// rather than re-deriving it, which is the only way the *panel* is what is under test.
    fn names_in(value: &str, port: usize) -> String {
        let after = value
            .split_once(&format!("port {port}: "))
            .unwrap_or_else(|| panic!("the row names no port {port}: {value:?}"))
            .1;
        // The row is `port N: a, b [· port M: …] — <remedy>`; stop at whichever separator comes first.
        let end = [" · ", " — "]
            .iter()
            .filter_map(|s| after.find(s))
            .min()
            .unwrap_or(after.len());
        after[..end].to_string()
    }
}
