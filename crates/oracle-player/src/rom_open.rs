//! **Opening a ROM from inside this window** — a browsable listing, a pasted path, and a dropped file, all
//! funnelling into one swap sequence.
//!
//! # What was true before, and why it is the whole point
//!
//! This window took its cartridge from `--rom` and from nowhere else. `F5` re-reads *the same* path
//! ([`crate::input::MachineKey::ReloadRom`]), and the only route to a *different* game was the command
//! palette: pause, type `emulator/reload_rom`, type `{"path": "/…/s4.bin"}`. So a client on the bus could
//! always change the owner's cartridge and the owner could not — the same asymmetry
//! `docs/2026-08-28-rom-open.md` §1 recorded for the minifb window, still true here after that parcel fixed
//! it *there*.
//!
//! # ⚑ A CONTROL, not a [`crate::ui::Tab`]
//!
//! Per the owner's amended rule in [`crate::palette`]'s header: *a gesture you hit and forget is a control;
//! a standing surface you read is a tab.* Opening a ROM is the first case — invoked, used once, dismissed —
//! so it is an `egui::Window` reached from a button and a chord, it adds no `Tab` variant (which would owe
//! [`crate::layout::LAYOUT_VERSION`] a bump and discard the owner's stored layout), and **nothing about it
//! is persisted across launches.**
//!
//! # ⚑ The model is egui-free, and that is the parcel's central structural requirement
//!
//! Everything decidable is a pure function or a method on plain state: [`folder_of`], [`RomOpen::rows`],
//! [`RomOpen::activate`], [`decide_drop`], [`reload_params`]. [`RomOpen::show`] draws them and decides
//! nothing — it collects a keystroke or a click into a flag and resolves it *after* the closure has closed.
//!
//! That is not tidiness. `docs/2026-08-28-rom-open.md` §5 is the frontend's own confession that its swap
//! block *"is not covered at all"*, precisely because the deciding happens inside a run loop nothing can
//! call. Here the deciding is callable, and every row of the coverage list below is a call.
//!
//! # ⚑ ONE sequence, three routes
//!
//! The browser's Enter, a pasted path and a dropped file all end in [`RomOpen::open`], which is:
//!
//! 1. [`crate::screen_pick::paused_for`] — **called, never re-implemented.** `emulator/reload_rom` refuses
//!    a running machine `-32005 machineRunning` (`Engine::reload_rom`'s first line), so the pause is not
//!    politeness, it is the difference between a gesture that works and one that always refuses.
//! 2. `emulator/reload_rom` with **`{"path": …}` and nothing else**. The registry declares that param set
//!    closed (`METHODS`' row for the method, `params: &["path"]`) and `Engine::dispatch` refuses any other
//!    top-level key `-32602` *before the handler runs*, so there is no second key to add.
//! 3. The run state put back by `paused_for` — which is the owner-approved answer to the one open question
//!    this parcel had: **opening a ROM implies playing it.** The alternative was refusing a running machine
//!    and making the human pause first, which is a refusal for a state the window could have fixed itself.
//!    The palette keeps the plain unpaused path, so `-32005` stays reachable and tested there.
//! 4. On a swap that **landed**, the listing is re-taken on [`folder_of`] of the bus's *new* `rom_path` —
//!    not on the folder that happened to be on screen. Every row's `[loaded]` marker is a claim about
//!    which cartridge is running, so a listing left on the old folder marks nothing as loaded while the
//!    image that is running is absent from it. A **refused** swap re-lists nothing: the cartridge did not
//!    move, so the listing is still right.
//!
//! Three routes and one sequence, for `docs/2026-08-28-rom-open.md` §2.1's stated reason: two copies of one
//! delicate sequence drift into two half-correct ones. Step 4 is the one that was got wrong by arranging it
//! per-route instead — see [`RomOpen::open`].
//!
//! **Nothing here touches the `.srm` or the save slots**, and that is not an omission.
//! [`crate::battery::Battery::carry`] takes the pending battery image at the top of *every*
//! [`crate::bus::drain`] — before the pump and before the `build_ui` this control is drawn in — and
//! `after_replacement` writes it against the outgoing cartridge once the machine has moved. A cartridge
//! replacement arriving from this window is the same event as one arriving from a socket client, and that
//! module already answers it for both.
//!
//! # ⚑ A refusal arrives as the server's sentence
//!
//! [`crate::ui::Echo`] carries `code` and `message` **verbatim** and colours on `refused`, never on the
//! shape of the string; [`remedy`] is keyed on `error.data.reason` and never on the message text. Straight
//! from [`crate::palette`], which is the pattern rather than a coincidence.
//!
//! # ⚑ Drag-and-drop, and what only a live window can confirm
//!
//! `docs/2026-08-28-rom-open.md` §4 measured drops as *not a callback* — minifb 0.28 models no file drop at
//! all, so the frontend would have needed hand-rolled XDND **and** a `wl_data_device` on minifb's own
//! display. **That blocker was minifb-specific and does not hold here.** Read from the crate source rather
//! than assumed: `egui-winit-0.36.1/src/lib.rs:474` maps `WindowEvent::DroppedFile` into
//! `egui_input.dropped_files`, which is `Vec<DroppedFileHandle>` on `egui::RawInput`
//! (`egui-0.36.1/src/data/input/raw_input.rs:81`), where `DroppedFileHandle = Arc<dyn DroppedFile>` and
//! `DroppedFile::path(&self) -> &Path` (`src/data/input/dropped_file.rs`). So the toolkit hands us the path
//! for free and [`dropped_paths`] is the whole of the reading.
//!
//! **What that does NOT establish**: whether the owner's Wayland compositor actually delivers the drop to
//! winit on his desktop. That is a runtime fact about a running window and is tagged for a foreground pass
//! in `docs/2026-09-08-player-rom-open.md`, not claimed here.
//!
//! ## The drop rules, stated because they are decisions
//!
//! [`decide_drop`] is a pure function over the paths, and it takes four positions:
//!
//! * **One ROM image → open it.** The whole point.
//! * **One directory → browse it.** A folder is an obvious "show me what is in here", and refusing it would
//!   make the gesture answer *no* to something with one right answer.
//! * **One file that is not a ROM image → REFUSED, out loud, and `reload_rom` is never called.**
//!   `Engine::reload_rom` does `std::fs::read` and `load_rom` with **no header check**, so a dropped
//!   `.txt` would be loaded as a cartridge and run as garbage. A believable wrong answer, which is the
//!   class this repo's rulings are written against — so the extension gate is [`rom_browser::is_rom`], the
//!   same predicate the listing filters on, and the refusal names the extensions that would have worked.
//! * **More than one file → REFUSED, out loud.** A Genesis holds one cartridge. Picking the first of five
//!   dropped files is the window choosing a game on the person's behalf, silently, and being wrong four
//!   times out of five.

use std::path::{Path, PathBuf};

use egui::{Key, KeyboardShortcut, Modifiers};
use oracle_frontend::rom_browser::{self, Entry, EntryKind};
use serde_json::{json, Value};

use crate::bus::{Answer, Bus};
use crate::machine::Machine;
use crate::screen;
use crate::spawn_picker::{Deed, RunState};
use crate::ui::Echo;

/// The transport bar's label for the control that opens this, and this window's own title.
///
/// A `const` for [`crate::palette::PALETTE_LABEL`]'s reason: [`crate::screen`] reports the bar over
/// `emulator/screen_text`, and a label spelled twice is a window and a client naming one control two ways.
pub const OPEN_LABEL: &str = "📂 open ROM";

/// **The keystroke that opens it.** `Ctrl+O` — the open-a-file convention, and unbound elsewhere in this
/// window: [`crate::palette::SHORTCUT`] is the only other chord (`Ctrl+P`), and [`crate::input`]'s pad and
/// machine keys are plain keys read only while nothing wants the keyboard.
pub const SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::O);

/// How [`SHORTCUT`] is written for a human, once, so the button's hover text and any prose about it cannot
/// disagree with the binding above.
pub const SHORTCUT_LABEL: &str = "Ctrl+O";

/// What the window paused the machine *for*, for [`RunState::sentence_of`]. A cartridge swap is not a
/// placement and not a preview, and *"paused the machine to place the object"* would send a reader looking
/// for an object nobody placed.
pub const OPENING: Deed = Deed {
    doing: "open the ROM",
    occasion: "when you asked for it",
};

/// **The folder to list**, given the path of the image the bus is running.
///
/// Derived from the bus's own answer ([`Bus::rom_path`]) rather than from the launch argument, for that
/// accessor's stated reason: `emulator/reload_rom` moves it, and a listing keyed to `argv` would describe
/// the folder of a cartridge that is no longer in the machine.
///
/// A bare filename has no parent, and neither does `None`; both browse the working directory rather than
/// refusing to open, which is `oracle-frontend`'s `Cmd::RomPicker` rule unchanged.
pub fn folder_of(rom_path: Option<&str>) -> PathBuf {
    rom_path
        .map(Path::new)
        .and_then(|p| p.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// **The exact params [`RomOpen::open`] sends.**
///
/// A named function so the one thing that must reach the bus unmodified — the path, character for character
/// — is assertable with no bus, no window and no machine in the way. Nothing here canonicalises: a path the
/// person pasted, a path winit handed us and a path this module joined are all sent as they are, so a
/// refusal quotes back the spelling that was actually tried (`Engine::reload_rom` absolutises only *after*
/// the read succeeds, precisely so its refusal shows the caller's own string).
pub fn reload_params(path: &Path) -> Value {
    json!({"path": path.to_string_lossy()})
}

/// **What the text box names**, when it names something that is on disk.
///
/// This is decision `d-19`'s chosen option — accept a pasted path — which
/// `docs/2026-08-28-rom-open.md` §4 filed as the cheap middle ground and which never shipped in the minifb
/// window. It costs one `is_dir`/`is_file` pair per repaint and it is what most people reach for
/// drag-and-drop to accomplish.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Typed {
    /// The box holds a filter, an empty string, or a path that does not exist. No row is synthesised.
    Nothing,
    /// An existing directory: Enter descends into it.
    Dir(PathBuf),
    /// An existing ROM image: Enter loads it.
    Rom(PathBuf),
    /// An existing file that is **not** a ROM image. Said out loud rather than ignored, and no row is
    /// offered — see the module header on why a `.txt` must not reach `reload_rom`.
    NotAnImage(PathBuf),
}

/// **One row on screen**: the shared model's entry, plus where the row came from.
///
/// The listing's rows and the pasted-path row are one type on purpose. [`RomOpen::activate`] resolves
/// through **this** list and nothing else, so there is one selection model and one Enter, and the typed row
/// cannot acquire a second set of rules by being drawn somewhere else.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    /// Label, kind and path — [`rom_browser::Entry`], so the marker rule below is the shared one.
    pub entry: Entry,
    /// True for the row synthesised from the text box, false for a row of the scanned listing. Drawn
    /// differently (it is a path, not a name) and reported as such; it changes nothing about Enter.
    pub typed: bool,
}

impl Row {
    /// [`rom_browser::LOADED_MARKER`] when this row is the image in the machine, else `None`. The shared
    /// predicate, by path — a same-named ROM in another folder is a different cartridge.
    pub fn marker(&self, current: Option<&Path>) -> Option<&'static str> {
        rom_browser::picker_marker(&self.entry, current)
    }

    /// The string this row is painted as: the label, then [`Row::marker`] when there is one.
    ///
    /// ⚑ **The marker is painted here and never filtered on** — [`RomOpen::rows`] filters on
    /// `entry.label` alone. `F-PICKER-FILTER-MARKER` in the minifb window was exactly this: with the marker
    /// baked into the label, typing `lod` kept the loaded ROM on a list that had hidden everything else.
    /// Asserted against [`rom_browser::picker_label`], the one written-down spelling, so the two windows
    /// cannot drift on what a marked row looks like.
    pub fn display(&self, current: Option<&Path>) -> String {
        format!("{}{}", self.entry.label, self.marker(current).unwrap_or(""))
    }
}

/// **What Enter on the selected row means.** Returned as a value rather than done inline so the deciding is
/// callable with no window — the module header's central requirement.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    /// Nothing is selected, or the filter hides every row. Enter does **nothing** rather than running a row
    /// the person just filtered away.
    None,
    /// Descend into (or ascend to) this folder and list it.
    Descend(PathBuf),
    /// Swap the cartridge for this image.
    Open(PathBuf),
}

/// **What a set of dropped paths means** — [`decide_drop`]'s answer. See the module header for the four
/// rules and why each one is a decision.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Dropped {
    /// Nothing was dropped this frame. The overwhelmingly common case.
    Nothing,
    /// One ROM image: swap the cartridge for it.
    Open(PathBuf),
    /// One directory: put the browser up on it.
    Browse(PathBuf),
    /// Refused, in these words, and **nothing was sent**.
    Refused(String),
}

/// **The drop rule, as a pure function.** No window, no context, no state.
///
/// The extension gate is [`rom_browser::is_rom`] — the same predicate the listing filters on, so a file
/// this refuses is a file the browser would not have offered either.
pub fn decide_drop(paths: &[PathBuf]) -> Dropped {
    match paths {
        [] => Dropped::Nothing,
        [one] if one.is_dir() => Dropped::Browse(one.clone()),
        [one] if rom_browser::is_rom(one) => Dropped::Open(one.clone()),
        [one] => Dropped::Refused(format!(
            "{} is not a Genesis image, so nothing was loaded. This window opens {}. It does not sniff \
             headers, so a file it cannot name is a file it will not run.",
            one.display(),
            exts()
        )),
        many => Dropped::Refused(format!(
            "{} files were dropped at once and a Genesis holds one cartridge, so nothing was loaded. Drop \
             one image, or use the list below.",
            many.len()
        )),
    }
}

/// The offered extensions, spelled from [`rom_browser::ROM_EXTS`] rather than transcribed — a refusal that
/// named a set the filter does not use would send a person looking for a file the window will not list.
fn exts() -> String {
    rom_browser::ROM_EXTS
        .iter()
        .map(|e| format!(".{e}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// **The paths dropped on this window since the last repaint.**
///
/// The whole of the toolkit reading, kept to one line so the deciding is [`decide_drop`]'s and can be
/// tested without a context. `DroppedFile::path` is an absolute path on native platforms per its own doc.
pub fn dropped_paths(ctx: &egui::Context) -> Vec<PathBuf> {
    ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    })
}

/// **What this window can do about a refusal**, keyed on `error.data.reason`, or `None`.
///
/// [`crate::palette::remedy`]'s rule, and deliberately a *different list*: this control pauses the machine
/// itself, so `machineRunning` is a refusal it cannot receive and inventing a remedy for it would point a
/// person at a control that was already used on their behalf. The one entry is the one refusal a person can
/// act on here — a path that could not be read — and it is reached through `-32602`, which carries no
/// `reason`, so it is keyed on `None` **only alongside a refusal**; see [`RomOpen::line`].
pub fn remedy(reason: Option<&str>) -> Option<String> {
    match reason {
        // The machine was paused by this control before the call, so a `machineRunning` here means the
        // pause was accepted and did not land — which is a fault in this window, not something the person
        // can fix, and saying "press pause" would be worse than saying nothing.
        Some("machineRunning") => Some(format!(
            "this control pauses the machine itself, so this refusal should not be reachable from here: \
             the pause was accepted and did not take effect. `{}` from the palette will show the same \
             refusal.",
            crate::ui::RELOAD_ROM
        )),
        _ => None,
    }
}

/// The ROM-open control's state between repaints.
///
/// Holds **no copy of the loaded cartridge's path**: the `[loaded]` marker is derived from
/// [`Bus::rom_path`] every repaint, because a remembered one would keep marking the previous game after any
/// swap — including one a socket client made.
#[derive(Default)]
pub struct RomOpen {
    /// Whether the window is up. Not persisted: see the module header.
    pub open: bool,
    /// The folder [`RomOpen::entries`] lists. `None` until a scan has succeeded.
    pub dir: Option<PathBuf>,
    /// The retained listing. **Kept across a failed scan** — an empty pick list and a folder that could not
    /// be read are the same picture on screen, and only one of them means "no games here"
    /// (`docs/2026-08-28-rom-open.md` §5(c)).
    pub entries: Vec<Entry>,
    /// The one box: a filter over the listing, and a path when it names one.
    pub text: String,
    /// The selected row, indexing [`RomOpen::rows`] — **the visible rows**, never [`RomOpen::entries`].
    pub sel: usize,
    /// Whether a listing has been attempted since the control was opened. Not `dir.is_some()`: a scan that
    /// fails leaves `dir` as it was, and retrying it on every repaint would re-`read_dir` a locked folder
    /// sixty times a second and re-say the failure just as often.
    pub attempted: bool,
    /// **This window's own last sentence** — a refused drop, a typed path that is not an image, a folder
    /// that could not be read, or a pause that was refused. Never the server's; that is [`RomOpen::last`].
    pub said: Option<String>,
    /// The bus's own last answer, verbatim. `None` before the first swap.
    pub last: Option<Echo>,
    /// What the last swap did to the run state. `None` until one happens.
    pub run: Option<RunState>,
}

impl RomOpen {
    /// Consume [`SHORTCUT`] if it was pressed this frame, and toggle.
    ///
    /// `consume_shortcut` rather than a raw key read, for [`crate::palette::Palette::handle_shortcut`]'s
    /// two reasons: the chord must not also reach whatever has focus, and [`crate::input::poll_machine_keys`]
    /// must not read a digit typed into this window's box as a save-slot change.
    pub fn handle_shortcut(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT)) {
            self.open = !self.open;
            if self.open {
                // A fresh open lists the running cartridge's folder again rather than wherever the last
                // session of this control wandered to.
                self.attempted = false;
            }
        }
    }

    /// **List `dir`.** On success the listing, the folder and the selection are replaced together — they are
    /// one fact and were never written separately. On failure the previous listing **stays up** and the
    /// reason is said; with no previous listing there is only the message.
    pub fn rescan(&mut self, dir: &Path) {
        self.attempted = true;
        match rom_browser::scan(dir) {
            Ok(entries) => {
                self.dir = Some(dir.to_path_buf());
                self.entries = entries;
                self.sel = 0;
                self.said = None;
            }
            // The reason before the path, `oracle-frontend`'s `cannot_read_toast` rule: a sentence cut for
            // width must lose the path and keep the reason, because the reason is the answer.
            Err(e) => self.said = Some(format!("cannot read {}: {e}", dir.display())),
        }
    }

    /// List the running cartridge's own folder, once, on the first repaint after opening.
    ///
    /// Split out of [`RomOpen::show`] so "which folder does it open on" is a testable decision rather than
    /// a line inside a draw closure.
    pub fn ensure_listing(&mut self, rom_path: Option<&str>) {
        if self.attempted {
            return;
        }
        let dir = folder_of(rom_path);
        self.rescan(&dir);
    }

    /// [`Typed`] for whatever is in the box right now.
    pub fn typed(&self) -> Typed {
        let t = self.text.trim();
        if t.is_empty() {
            return Typed::Nothing;
        }
        let p = PathBuf::from(t);
        if p.is_dir() {
            Typed::Dir(p)
        } else if !p.is_file() {
            Typed::Nothing
        } else if rom_browser::is_rom(&p) {
            Typed::Rom(p)
        } else {
            Typed::NotAnImage(p)
        }
    }

    /// **The rows on screen, in order.** The pasted-path row first when there is one, then the listing
    /// filtered by the box.
    ///
    /// ⚑ **The filter reads `entry.label` and nothing else** — not [`Row::display`], which carries the
    /// `[loaded]` marker. See [`Row::display`] for the defect that rule exists to prevent.
    ///
    /// Case-insensitive substring rather than the minifb picker's subsequence match: this box is also a
    /// path box, and a subsequence match over a folder of images turns a half-typed path into a list of
    /// coincidences.
    pub fn rows(&self) -> Vec<Row> {
        let mut out = Vec::new();
        // The pasted path is offered first so a paste followed by Enter opens it, which is the entire
        // gesture that route exists for.
        match self.typed() {
            Typed::Rom(path) => out.push(Row {
                entry: Entry {
                    label: path.to_string_lossy().into_owned(),
                    kind: EntryKind::Rom,
                    path,
                },
                typed: true,
            }),
            Typed::Dir(path) => out.push(Row {
                entry: Entry {
                    label: format!("{}/", path.to_string_lossy()),
                    kind: EntryKind::Dir,
                    path,
                },
                typed: true,
            }),
            Typed::Nothing | Typed::NotAnImage(_) => {}
        }
        let needle = self.text.trim().to_ascii_lowercase();
        for e in &self.entries {
            if needle.is_empty() || e.label.to_ascii_lowercase().contains(&needle) {
                out.push(Row {
                    entry: e.clone(),
                    typed: false,
                });
            }
        }
        out
    }

    /// ★ **What Enter means — resolved through [`RomOpen::rows`], never through [`RomOpen::entries`].**
    ///
    /// This is the load-bearing function of the whole parcel, and its defect is **silent**: with a filter
    /// applied, `sel = 0` means *the first row on screen*, and an implementation that indexed the
    /// unfiltered listing would run row 0 of *that* — a different game, loaded with no complaint, with the
    /// right row still highlighted. Nothing on the glass would contradict it.
    /// `docs/2026-08-28-rom-open.md` §3.1 calls the same mutation that parcel's most important one.
    pub fn activate(&self) -> Action {
        let rows = self.rows();
        match rows.get(self.sel) {
            None => Action::None,
            Some(row) if row.entry.kind == EntryKind::Rom => Action::Open(row.entry.path.clone()),
            Some(row) => Action::Descend(row.entry.path.clone()),
        }
    }

    /// Move the selection by `d` rows, clamped to what is on screen.
    pub fn step(&mut self, d: isize) {
        let n = self.rows().len();
        if n == 0 {
            self.sel = 0;
            return;
        }
        let last = n - 1;
        self.sel = (self.sel.min(last) as isize + d).clamp(0, last as isize) as usize;
    }

    /// Do whatever [`RomOpen::activate`] decided.
    pub fn run_action(&mut self, machine: &mut Machine, bus: &mut Bus, action: Action) {
        match action {
            Action::None => {}
            Action::Descend(dir) => self.rescan(&dir),
            Action::Open(path) => self.open(machine, bus, &path),
        }
    }

    /// ★ **The one shared sequence.** Pause, `emulator/reload_rom {path}`, run state restored.
    ///
    /// Every route ends here — a row's Enter, a pasted path, a dropped file — for the module header's
    /// reason. [`crate::screen_pick::paused_for`] is *called*: it is this crate's one implementation of
    /// "the machine really was paused while the body ran, and it is where it was afterwards", and a second
    /// capture-and-restore would be a second answer to what already-paused means.
    ///
    /// `Err` from `paused_for` is the **pause** being refused, in the server's own words: the call was
    /// never made and the run state was never touched, so there is nothing to restore and nothing to say
    /// about it. That is a different fact from a refused reload and is rendered as this window's own
    /// sentence rather than as the bus's.
    pub fn open(&mut self, machine: &mut Machine, bus: &mut Bus, path: &Path) {
        let params = reload_params(path);
        self.said = None;
        match crate::screen_pick::paused_for(machine, bus, |machine, bus| {
            bus.call(machine.system_mut(), crate::ui::RELOAD_ROM, &params)
        }) {
            Err(why) => {
                self.last = None;
                self.run = None;
                self.said = Some(format!(
                    "the window could not pause the machine to {}, so nothing was loaded. {why}",
                    OPENING.doing
                ));
            }
            Ok((answer, run)) => {
                self.run = Some(run);
                self.last = Some(Echo {
                    method: crate::ui::RELOAD_ROM,
                    refused: answer.is_err(),
                    reason: answer.reason().map(str::to_string),
                    text: match &answer {
                        // The reply whole: it carries the absolutised path the engine actually loaded,
                        // `symbolsDropped` (D7) and `hitsDropped` (§11.39), and every one of the three is
                        // something a person swapping a cartridge wants to see.
                        Answer::Ok(v) => format!("ok {v}"),
                        Answer::Err(e) => format!("{} {}", e.code, e.message),
                    },
                });
                // ⚑ **On a swap that LANDED, re-list the folder the NEW cartridge came from** — read back
                // off the bus, never `self.dir`.
                //
                // The listing is stale the instant the cartridge moves: every row's `[loaded]` marker is
                // about a game that is no longer in the machine. Re-listing `self.dir` was WRONG and the
                // defect had a route — a file dropped from a folder other than the one being browsed left
                // the window showing the old folder, whose rows carry no marker at all while the image
                // that *is* running is not on the list. So the picture marked nothing as loaded and the
                // loaded thing was absent, which is the believable-wrong-answer shape rather than a
                // missing one. (Found by the controller's verification pass; the two comments here used
                // to contradict each other, and the `attempted = false` in
                // [`RomOpen::act_on_drop`] that was meant to fix it was overwritten by this `rescan`.)
                //
                // `folder_of(bus.rom_path())` unifies all three routes rather than special-casing the
                // drop: for a row's Enter the new cartridge is *in* the folder being browsed, so the
                // answer is the same folder and nothing visibly changes; for a drop or a pasted path from
                // elsewhere it repoints, and the marker lands on the image just loaded. It is also the
                // rule this feature already states everywhere else — *the folder the running image came
                // from* ([`folder_of`], `ensure_listing`, `oracle-frontend`'s `Cmd::RomPicker`).
                //
                // A REFUSED swap deliberately re-lists nothing: the cartridge did not move, so the
                // listing and its marker are still correct, and `attempted` is left as it was — which
                // means a refusal arriving before any listing was taken still gets one on the next
                // repaint, from the unchanged path.
                //
                // A rescan that then fails leaves `said` carrying the folder error beside a `last` that
                // reports the successful load. Both are true and both are shown; the swap happened and
                // the new folder cannot be listed.
                if !self.last.as_ref().is_some_and(|e| e.refused) {
                    let dir = folder_of(bus.rom_path());
                    self.rescan(&dir);
                }
            }
        }
    }

    /// **Act on whatever was dropped on the window this frame.** The reading is [`dropped_paths`], the
    /// deciding is [`decide_drop`], and this is only the doing.
    pub fn handle_drops(&mut self, ctx: &egui::Context, machine: &mut Machine, bus: &mut Bus) {
        self.act_on_drop(machine, bus, decide_drop(&dropped_paths(ctx)));
    }

    /// The doing half of [`RomOpen::handle_drops`], taking the decision as a value so it is reachable
    /// without a context.
    ///
    /// A drop opens the window as well as acting: a refusal nobody can see is a silent failure, and a
    /// successful swap wants the listing put up on the new cartridge's folder.
    pub fn act_on_drop(&mut self, machine: &mut Machine, bus: &mut Bus, what: Dropped) {
        match what {
            Dropped::Nothing => {}
            Dropped::Open(path) => {
                self.open = true;
                // Nothing about the listing is arranged here. [`RomOpen::open`] re-lists the folder the
                // new cartridge came from on every swap that lands, so a drop from elsewhere repoints by
                // the shared sequence rather than by this arm. An `attempted = false` stood here, meaning
                // to do exactly that, and was silently overwritten by that `rescan` — the defect this
                // arm no longer tries to fix on its own.
                self.open(machine, bus, &path);
            }
            Dropped::Browse(dir) => {
                self.open = true;
                self.rescan(&dir);
            }
            Dropped::Refused(why) => {
                self.open = true;
                self.last = None;
                self.run = None;
                self.said = Some(why);
            }
        }
    }

    /// The bus's last answer as one line, with [`remedy`] appended when there is one. `None` before the
    /// first swap.
    pub fn line(&self) -> Option<String> {
        let e = self.last.as_ref()?;
        Some(match remedy(e.reason.as_deref()).filter(|_| e.refused) {
            Some(r) => format!("{}. {r}", e.line()),
            None => e.line(),
        })
    }

    /// The headline: what folder is listed, and how many of its rows are on screen. **Derived, both
    /// halves** — a count that was stored would be the stale thing.
    pub fn headline(&self) -> String {
        let dir = match &self.dir {
            Some(d) => d.display().to_string(),
            None => "no folder listed".to_string(),
        };
        format!(
            "{} - {} of {} rows shown. Enter opens the highlighted row; {} loads a pasted path; drop a \
             file on this window.",
            dir,
            self.rows().len(),
            self.entries.len(),
            SHORTCUT_LABEL,
        )
    }

    /// **Draw the control**, and return the [`screen::Run`]s it put on the glass for
    /// `emulator/screen_text` (§11.29).
    ///
    /// Handing back what was drawn rather than offering a helper both this and [`crate::screen`] could
    /// call, for [`crate::palette::Palette::show`]'s reason: a modal covering this window whose text no
    /// client could read is a hole in the readback.
    ///
    /// **The rows inside the scroll area are deliberately NOT reported**, and the selected row is. That is
    /// [`crate::screen`]'s own rule rather than a saving: what a scroll offset reveals is computed inside
    /// egui's paint, so listing every row would report text nobody can see. The selected row is drawn
    /// *outside* the scroll area precisely so it can be both seen and read back.
    ///
    /// ⚑ **Nothing is decided in here.** A keystroke or a click sets a flag; [`RomOpen::activate`] and
    /// [`RomOpen::run_action`] resolve it after the closure has closed. See the module header.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        machine: &mut Machine,
        bus: &mut Bus,
    ) -> Vec<screen::Run> {
        let mut drew = Vec::new();
        if !self.open {
            return drew;
        }
        self.ensure_listing(bus.rom_path());
        // Read every repaint, never remembered — see the struct doc.
        let current: Option<PathBuf> = bus.rom_path().map(PathBuf::from);
        let mut open = self.open;
        let mut enter = false;
        let mut clicked: Option<usize> = None;
        let mut refresh = false;
        let mut up = false;
        let mut walk: isize = 0;
        egui::Window::new(OPEN_LABEL)
            .open(&mut open)
            .default_width(620.0)
            .show(ctx, |ui| {
                let head = self.headline();
                ui.weak(&head);
                drew.push(screen::Run::label(head));

                ui.horizontal(|ui| {
                    ui.label("path");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.text)
                            .hint_text("filter this folder, or paste a full path to an image")
                            .desired_width(420.0),
                    );
                    if r.changed() {
                        // Filtering puts the selection on the first match, so Enter after typing means
                        // "the row I can see", which is the property `activate` exists to keep.
                        self.sel = 0;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        enter = true;
                    }
                    // ⚑ **Up/Down walk the rows, and ONLY while the box has focus.** Read through the
                    // response rather than consumed globally, because the arrows are also the D-pad: a
                    // `consume_shortcut` here would swallow them from the game whenever this window
                    // happened to be up. While the box has focus `Context::egui_wants_keyboard_input` is
                    // true, so `crate::input::decide` is already handing the machine an empty pad — the
                    // keystroke cannot be both at once.
                    if r.has_focus() {
                        ui.input(|i| {
                            if i.key_pressed(Key::ArrowDown) {
                                walk += 1;
                            }
                            if i.key_pressed(Key::ArrowUp) {
                                walk -= 1;
                            }
                        });
                    }
                    if ui.button("↑ up").clicked() {
                        up = true;
                    }
                    if ui.button("⟳ refresh").clicked() {
                        refresh = true;
                    }
                });

                // The one refusal the box itself can raise, said where it was typed.
                if let Typed::NotAnImage(p) = self.typed() {
                    let msg = format!(
                        "{} is not a Genesis image, so it is not offered. This window opens {}.",
                        p.display(),
                        exts()
                    );
                    ui.colored_label(ui.visuals().error_fg_color, &msg);
                    drew.push(screen::Run::label(msg));
                }

                let rows = self.rows();
                // Drawn OUTSIDE the scroll area so it is always both visible and readable back.
                let sel_line = match rows.get(self.sel) {
                    Some(r) => format!("Enter: {}", r.display(current.as_deref())),
                    None => "Enter: nothing, because no row matches what is typed".to_string(),
                };
                ui.separator();
                ui.strong(&sel_line);
                drew.push(screen::Run::after_sep(sel_line));

                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for (n, row) in rows.iter().enumerate() {
                            let label = row.display(current.as_deref());
                            let text = if row.typed {
                                egui::RichText::new(&label).italics()
                            } else {
                                egui::RichText::new(&label)
                            };
                            if ui.selectable_label(n == self.sel, text).clicked() {
                                clicked = Some(n);
                            }
                        }
                    });

                if let Some(msg) = &self.said {
                    ui.separator();
                    // Coloured on being this window's own refusal, never on the shape of the string.
                    ui.colored_label(ui.visuals().error_fg_color, msg)
                        .on_hover_text(
                            "this window's own sentence: the cartridge was not changed and, where a call \
                             was never made, nothing reached the bus",
                        );
                    drew.push(screen::Run::after_sep(msg.clone()));
                }
                if let (Some(e), Some(line)) = (&self.last, self.line()) {
                    ui.separator();
                    let colour = if e.refused {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.colored_label(colour, &line).on_hover_text(
                        "the bus's own reply, verbatim. The bracketed word is `error.data.reason`, the \
                         discriminant clients branch on, never the message text.",
                    );
                    drew.push(screen::Run::after_sep(line));
                }
                if let Some(run) = &self.run {
                    let s = run.sentence_of(OPENING);
                    if run.alarming() {
                        ui.colored_label(ui.visuals().warn_fg_color, &s);
                    } else {
                        ui.weak(&s);
                    }
                    drew.push(screen::Run::label(s));
                }
            });
        self.open = open;

        // --- Everything decided, out here, where a test can reach it. ---
        if refresh {
            self.attempted = false;
            self.ensure_listing(bus.rom_path());
        }
        if up {
            if let Some(parent) = self.dir.as_deref().and_then(Path::parent) {
                let parent = parent.to_path_buf();
                self.rescan(&parent);
            }
        }
        if walk != 0 {
            self.step(walk);
        }
        if let Some(n) = clicked {
            self.sel = n;
            enter = true;
        }
        if enter {
            let action = self.activate();
            self.run_action(machine, bus, action);
        }
        drew
    }
}

// ---------------------------------------------------------------------------------------------------

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use oracle_aether::engine::METHODS;

    /// A scratch folder that removes itself, so these tests leave nothing behind and run in parallel.
    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Tmp {
            let dir = std::env::temp_dir().join(format!(
                "oracle-rom-open-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Tmp(dir)
        }
        /// A file with real test-ROM bytes in it, so `emulator/reload_rom` can actually load it.
        fn rom(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            std::fs::write(&p, oracle_core::testrom::build()).unwrap();
            p
        }
        fn junk(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            std::fs::write(&p, b"not a cartridge").unwrap();
            p
        }
        fn dir(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            std::fs::create_dir_all(&p).unwrap();
            p
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A **running** machine whose bus knows the cartridge at `rom`, which is what the marker and the
    /// initial folder are derived from. Running on purpose: [`RomOpen::open`] must pause it and put it back,
    /// and a fixture that started paused would make the restore vacuous.
    fn rig(rom: &Path) -> (Machine, Bus) {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let bus = Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo {
                rom_path: Some(rom.to_string_lossy().into_owned()),
                symbols: None,
                symbols_path: None,
            },
            false,
            None,
        );
        (machine, bus)
    }

    fn labels(rows: &[Row]) -> Vec<String> {
        rows.iter().map(|r| r.entry.label.clone()).collect()
    }

    /// A control listing `dir`, with nothing typed.
    fn listing(dir: &Path) -> RomOpen {
        let mut r = RomOpen::default();
        r.rescan(dir);
        assert!(
            r.dir.is_some() && !r.entries.is_empty(),
            "COULD NOT MEASURE: the fixture folder did not list: {:?}",
            r.said
        );
        r
    }

    /// ★★ **Enter resolves through the VISIBLE rows, not by indexing the listing.**
    ///
    /// The parcel's load-bearing property, and the one whose defect is silent: the highlighted row is
    /// correct, Enter is pressed, and a *different* cartridge loads with nothing on screen contradicting
    /// it.
    ///
    /// The fixture is built so the two answers **cannot coincide**: the filter's first match is the last
    /// ROM in the unfiltered listing, and the assertion is asserted against the row the filter selected
    /// *and* asserted different from row 0 of `entries`. An implementation indexing `entries` would return
    /// `../`, i.e. a `Descend`, so the two arms are not even the same variant.
    #[test]
    fn enter_opens_the_row_the_filter_left_on_screen() {
        let t = Tmp::new("filtered-enter");
        t.rom("alpha.bin");
        t.rom("beta.bin");
        let target = t.rom("zulu.bin");
        let mut r = listing(&t.0);

        // The control: unfiltered, row 0 is the parent, so `entries[0]` is NOT the answer below.
        assert_eq!(
            labels(&r.rows()),
            vec!["../", "alpha.bin", "beta.bin", "zulu.bin"],
            "COULD NOT MEASURE: the unfiltered listing is not the shape this test reasons about"
        );
        assert_eq!(
            r.entries[0].kind,
            EntryKind::Parent,
            "COULD NOT MEASURE: row 0 of the listing must be a row a defect would visibly pick"
        );

        r.text = "zulu".into();
        r.sel = 0;
        let rows = r.rows();
        assert_eq!(
            labels(&rows),
            vec!["zulu.bin"],
            "the filter must leave exactly the one row this test presses Enter on"
        );
        assert_eq!(
            r.activate(),
            Action::Open(target.clone()),
            "Enter ran a row that is not the one on screen — an implementation indexing the unfiltered \
             listing would have answered Descend(../) here"
        );

        // And a filter that matches nothing does NOTHING, rather than running the row it just hid.
        r.text = "no-such-game".into();
        assert!(r.rows().is_empty(), "COULD NOT MEASURE: the filter matched");
        assert_eq!(
            r.activate(),
            Action::None,
            "Enter with nothing on screen must not run a row the person filtered away"
        );
    }

    /// ★ **Descend, ascend, and the `[loaded]` marker survives the round trip.**
    ///
    /// Two properties in one walk because they are one gesture: `../` must point at the parent (a `../` row
    /// pointing at its own folder is a no-op that looks like it worked), and coming back must re-mark the
    /// running image — the marker is derived from the bus every time and never remembered, so a round trip
    /// is exactly what would catch a remembered one.
    #[test]
    fn descending_and_coming_back_keeps_the_loaded_marker_on_the_running_image() {
        let t = Tmp::new("round-trip");
        let loaded = t.rom("s4.bin");
        let sub = t.dir("acts");
        let current = Some(loaded.clone());

        let mut r = listing(&t.0);
        let marked = |r: &RomOpen| -> Vec<String> {
            r.rows()
                .iter()
                .filter(|row| row.marker(current.as_deref()).is_some())
                .map(|row| row.display(current.as_deref()))
                .collect()
        };
        assert_eq!(
            marked(&r),
            vec!["s4.bin   [loaded]"],
            "the running image must be marked, once, before we go anywhere"
        );
        // The one written-down spelling, so the two windows cannot drift on what a marked row looks like.
        let row = r
            .rows()
            .into_iter()
            .find(|row| row.entry.label == "s4.bin")
            .unwrap();
        assert_eq!(
            row.display(current.as_deref()),
            rom_browser::picker_label(&row.entry, current.as_deref()),
            "`Row::display` and `rom_browser::picker_label` disagree on the painted row"
        );

        // Descend into the subfolder: the marker goes, because the running image is not in here.
        r.text = "acts".into();
        r.sel = 0;
        let Action::Descend(into) = r.activate() else {
            panic!("a directory row must descend, not open: {:?}", r.activate());
        };
        assert_eq!(into, sub);
        r.text.clear();
        r.rescan(&into);
        assert_eq!(r.dir.as_deref(), Some(sub.as_path()));
        assert!(
            marked(&r).is_empty(),
            "a folder that does not hold the running image must mark nothing: {:?}",
            marked(&r)
        );
        // ⚑ **A SAME-NAMED image in another folder is a different cartridge**, and this leg is here
        // because the lib's own by-path guard cannot see a re-implementation on this side: a
        // `Row::marker` that compared file names instead of delegating to `rom_browser::picker_marker`
        // would keep `rom_browser`'s test green and tell the person the game they are looking at is
        // already running when it is not. Two folders in this workspace hold an `s4.bin`.
        std::fs::write(sub.join("s4.bin"), oracle_core::testrom::build()).unwrap();
        r.rescan(&sub);
        assert!(
            r.rows().iter().any(|row| row.entry.label == "s4.bin"),
            "COULD NOT MEASURE: the decoy is not in the listing"
        );
        assert!(
            marked(&r).is_empty(),
            "a same-named ROM in a different folder was marked as the running cartridge: {:?}",
            marked(&r)
        );
        std::fs::remove_file(sub.join("s4.bin")).unwrap();
        r.rescan(&sub);

        // Up through `../`, which must point at the PARENT.
        let up = r.rows().into_iter().next().expect("`../` is offered");
        assert_eq!(up.entry.kind, EntryKind::Parent);
        assert_eq!(
            up.entry.path, t.0,
            "`../` must point at the containing folder, not at the folder itself"
        );
        r.sel = 0;
        let Action::Descend(back) = r.activate() else {
            panic!("the parent row must navigate");
        };
        r.rescan(&back);
        assert_eq!(
            marked(&r),
            vec!["s4.bin   [loaded]"],
            "the `[loaded]` marker did not survive the round trip out and back"
        );
    }

    /// ★ **An unreadable folder keeps the previous listing up and says why.**
    ///
    /// `docs/2026-08-28-rom-open.md` §5(c). An empty pick list and a folder that could not be read are the
    /// same picture on screen, and only one of them means "no games here". The rows are compared **whole**
    /// against the first listing, not counted — a re-scan of some other folder with the same number of
    /// entries would count the same.
    ///
    /// The message is derived from the same `io::Error` a caller would get, so the expectation is the OS's
    /// text and not this test's.
    #[test]
    fn an_unreadable_folder_keeps_the_previous_listing_and_says_why() {
        let t = Tmp::new("unreadable");
        t.rom("s4.bin");
        let mut r = listing(&t.0);
        let before = r.entries.clone();
        let dir_before = r.dir.clone();
        assert!(r.said.is_none(), "a clean scan says nothing");

        let missing = t.0.join("gone");
        let expected = {
            let e = rom_browser::scan(&missing).expect_err("the folder does not exist");
            format!("cannot read {}: {e}", missing.display())
        };
        r.rescan(&missing);

        assert_eq!(
            r.entries, before,
            "a failed scan must not rebuild the listing"
        );
        assert_eq!(
            r.dir, dir_before,
            "the control still describes the readable folder"
        );
        assert_eq!(
            r.said.as_deref(),
            Some(expected.as_str()),
            "the failure must be said, in the OS's own words"
        );
        // The reason comes before the path: a sentence cut for width must keep the half that answers.
        let said = r.said.clone().unwrap();
        assert!(said.starts_with("cannot read"), "unexpected shape: {said}");

        // With no previous listing there is only the message — nothing pretends to be a folder.
        let mut fresh = RomOpen::default();
        fresh.rescan(&missing);
        assert!(fresh.entries.is_empty() && fresh.dir.is_none());
        assert_eq!(fresh.said.as_deref(), Some(expected.as_str()));
    }

    /// ★ **A pasted path is offered and opens on Enter** — decision `d-19`'s option, delivered.
    ///
    /// Four legs, because the interesting ones are the refusals: an image outside the listed folder is
    /// offered as the first row (so paste-then-Enter works with the selection where it starts), a
    /// directory descends, a file that is not a Genesis image is **not offered** and is named as the
    /// reason, and a path that does not exist is simply a filter.
    #[test]
    fn a_pasted_path_is_offered_and_a_pasted_non_image_is_refused() {
        let here = Tmp::new("paste-here");
        here.rom("s4.bin");
        let there = Tmp::new("paste-there");
        let elsewhere = there.rom("sonic2.md");
        let junk = there.junk("readme.txt");
        let mut r = listing(&here.0);

        // An image in a folder this control is not listing.
        r.text = elsewhere.to_string_lossy().into_owned();
        assert_eq!(r.typed(), Typed::Rom(elsewhere.clone()));
        let rows = r.rows();
        assert!(
            rows.first().is_some_and(|row| row.typed),
            "the pasted path must be the FIRST row, or paste-then-Enter opens something else: {:?}",
            labels(&rows)
        );
        r.sel = 0;
        assert_eq!(r.activate(), Action::Open(elsewhere.clone()));

        // A directory navigates rather than loading.
        r.text = there.0.to_string_lossy().into_owned();
        assert_eq!(r.typed(), Typed::Dir(there.0.clone()));
        r.sel = 0;
        assert_eq!(r.activate(), Action::Descend(there.0.clone()));

        // A real file that is not a Genesis image: not offered, and the reason names the extensions.
        r.text = junk.to_string_lossy().into_owned();
        assert_eq!(r.typed(), Typed::NotAnImage(junk.clone()));
        assert!(
            r.rows().is_empty(),
            "a non-image must not be offered as a row: {:?}",
            labels(&r.rows())
        );
        assert_eq!(
            r.activate(),
            Action::None,
            "and Enter on it must do nothing at all"
        );

        // A path that does not exist is just a filter — and it filters.
        r.text = "s4".into();
        assert_eq!(r.typed(), Typed::Nothing);
        assert_eq!(labels(&r.rows()), vec!["s4.bin"]);
    }

    /// ★★ **`open()` ran its body on a machine that was genuinely paused, and put the prior run state
    /// back.**
    ///
    /// The first half is what makes the gesture work at all: `Engine::reload_rom` refuses `-32005
    /// machineRunning`, so a window that dispatched without pausing would refuse every open, and one whose
    /// pause did not land would look identical from outside.
    ///
    /// **The alternative green paths, each closed by a named assertion:**
    /// 1. *The fixture was paused all along*, making the restore vacuous — asserted running first.
    /// 2. *The call never happened* — the reply is asserted non-refused **and** `Bus::rom_path()` is
    ///    asserted to have moved to the new image, which is the machine's own account rather than the
    ///    window's.
    /// 3. *The pause is inferred rather than observed* — the reply itself is the witness: a `-32005` would
    ///    mean the body saw a running machine.
    #[test]
    fn open_runs_on_a_paused_machine_and_restores_the_prior_run_state() {
        let t = Tmp::new("paused-open");
        let first = t.rom("s4.bin");
        let second = t.rom("sonic2.bin");
        let (mut machine, mut bus) = rig(&first);
        assert!(
            !bus.is_paused(),
            "the control: this fixture must start RUNNING or the restore witnesses nothing"
        );

        let mut r = RomOpen::default();
        r.open(&mut machine, &mut bus, &second);

        let echo = r.last.as_ref().expect("the bus must have answered");
        assert!(
            !echo.refused,
            "the reload was refused, so nothing below is about a machine that moved: {}",
            echo.line()
        );
        assert!(
            r.said.is_none(),
            "the window raised its own refusal too: {:?}",
            r.said
        );
        // The machine's own account of which cartridge it is running — not the window's, and not the
        // reply's. The engine absolutises on success, so the expectation is derived through the same
        // function rather than transcribed.
        assert_eq!(
            bus.rom_path(),
            Some(oracle_aether::engine::absolutise(&second.to_string_lossy()).as_str()),
            "the bus is not running the image that was opened"
        );
        assert_eq!(
            r.run,
            Some(RunState::Restored { frames: None }),
            "a machine that was running when the ROM was opened must be running afterwards"
        );
        assert!(
            !bus.is_paused(),
            "this window paused the machine and left it paused: nobody asked it to stop and nothing here \
             would restart it"
        );
        assert!(
            !r.run.as_ref().unwrap().alarming(),
            "a clean restore is the quiet case"
        );

        // And a machine somebody else paused is left exactly as it was found.
        let (mut machine, mut bus) = rig(&first);
        assert!(!bus
            .call(machine.system_mut(), crate::ui::PAUSE, &json!({}))
            .is_err());
        assert!(bus.is_paused(), "the control: it must be paused now");
        let mut r = RomOpen::default();
        r.open(&mut machine, &mut bus, &second);
        assert_eq!(r.run, Some(RunState::AlreadyPaused));
        assert!(
            bus.is_paused(),
            "the window RESUMED a machine somebody else stopped"
        );
    }

    /// ★ **The path reaches `Bus::call` verbatim, under the one declared key.**
    ///
    /// Nothing canonicalises: the spelling that was pasted, dropped or joined is the spelling that is sent,
    /// so `Engine::reload_rom`'s refusal quotes back what was actually tried. The awkward spelling below
    /// would not survive a `canonicalize` and is chosen for that reason.
    ///
    /// The key set is **derived from the registry**, not typed: `Engine::dispatch` refuses any other
    /// top-level key `-32602` before the handler runs, so an extra key here would be a call that never
    /// reaches the machine.
    #[test]
    fn the_path_is_sent_verbatim_under_the_registry_s_only_declared_key() {
        // ⚑ **A path that EXISTS, spelled the long way round.** The first draft of this test used a
        // made-up path with `..` in it, and a `reload_params` mutated to `std::fs::canonicalize` stayed
        // GREEN under it: `canonicalize` fails on a path that is not there, so the fallback returned the
        // input and the mutation was inert. Applied-and-still-green is a test defect, not a pass. The
        // fixture is now a real file reached through a real detour, so a canonicalise collapses it and the
        // assertion fires.
        let t = Tmp::new("verbatim");
        let real = t.rom("s4.bin");
        let sub = t.dir("acts");
        let detour = sub.join("..").join("s4.bin");
        assert!(
            detour.exists() && detour != real,
            "COULD NOT MEASURE: the detour must exist AND differ from the direct spelling, or a \
             canonicalise would be undetectable here: {detour:?} vs {real:?}"
        );
        assert_eq!(
            std::fs::canonicalize(&detour).unwrap(),
            real,
            "COULD NOT MEASURE: the two spellings must name the same file"
        );

        let params = reload_params(&detour);
        assert_eq!(
            params["path"],
            detour.to_string_lossy().into_owned(),
            "the path must reach the bus character for character: nothing here canonicalises, so \
             `Engine::reload_rom`'s refusal quotes back the spelling that was actually tried"
        );
        // And an awkward spelling that never touches the filesystem survives too.
        let literal = "./games/../games/My ROMs/s4 (final).bin";
        assert_eq!(reload_params(Path::new(literal))["path"], literal);

        let spec = METHODS
            .iter()
            .find(|m| m.name == crate::ui::RELOAD_ROM)
            .unwrap_or_else(|| panic!("`{}` is not served by this build", crate::ui::RELOAD_ROM));
        assert_eq!(
            spec.params,
            &["path"],
            "the registry's declared key set for this method has changed; `reload_params` must follow it"
        );
        let sent: Vec<&String> = params
            .as_object()
            .expect("params is an object")
            .keys()
            .collect();
        assert_eq!(
            sent,
            spec.params.iter().collect::<Vec<_>>(),
            "the params sent must be exactly the registry's closed key set"
        );
    }

    /// ★ **A bus refusal is echoed with its code and message intact, and branched on `reason`.**
    ///
    /// The expectation is obtained from an independent direct `Bus::call`, so it is the server's sentence
    /// rather than a transcription, and it is asserted non-empty (or `contains` passes forever).
    ///
    /// **And the machine did not move.** A refusal that swapped the cartridge anyway is a far worse defect
    /// than a wrong sentence, and the reply alone cannot see it.
    #[test]
    fn a_bus_refusal_is_echoed_verbatim_and_the_cartridge_did_not_change() {
        let t = Tmp::new("refusal");
        let first = t.rom("s4.bin");
        let missing = t.0.join("no-such-game.bin");
        let (mut machine, mut bus) = rig(&first);
        let before = bus.rom_path().map(str::to_string);

        // The server's own words, obtained independently of this control. Paused first, because an
        // unpaused machine would answer `-32005` and this test is about the path refusal.
        assert!(!bus
            .call(machine.system_mut(), crate::ui::PAUSE, &json!({}))
            .is_err());
        let theirs = match bus.call(
            machine.system_mut(),
            crate::ui::RELOAD_ROM,
            &reload_params(&missing),
        ) {
            Answer::Err(e) => e,
            Answer::Ok(v) => panic!("a path that does not exist must be refused; it replied {v}"),
        };
        assert!(
            !theirs.message.is_empty(),
            "an empty message makes the `contains` below pass forever"
        );

        let (mut machine, mut bus) = rig(&first);
        let mut r = RomOpen::default();
        r.open(&mut machine, &mut bus, &missing);

        let echo = r.last.as_ref().expect("the bus must have answered");
        assert!(echo.refused, "this must be coloured as a refusal");
        assert!(
            echo.text.contains(&theirs.message),
            "the server's message must arrive whole. It said `{}`; the control showed `{}`",
            theirs.message,
            echo.text
        );
        assert!(
            echo.text.contains(&theirs.code.to_string()),
            "the code must be shown beside it; the control showed `{}`",
            echo.text
        );
        // Branched on the discriminant and never on the prose: this refusal carries no `reason`, so there
        // is no remedy, and a `remedy` keyed on message text would have invented one.
        let their_reason = theirs
            .data
            .as_ref()
            .and_then(|d| d.get("reason"))
            .and_then(|v| v.as_str());
        assert_eq!(echo.reason.as_deref(), their_reason);
        assert_eq!(
            remedy(echo.reason.as_deref()),
            None,
            "a refusal with no discriminant must get no invented remedy"
        );
        assert!(
            remedy(Some("machineRunning")).is_some(),
            "the one discriminant this control speaks to must still be keyed"
        );
        assert_eq!(
            bus.rom_path().map(str::to_string),
            before,
            "the open was refused and the cartridge changed anyway"
        );
    }

    /// ★★ **A dropped non-ROM is refused and `emulator/reload_rom` is NEVER called.**
    ///
    /// `Engine::reload_rom` reads the bytes and calls `load_rom` with **no header check**, so a silently
    /// loaded `.txt` runs as garbage — a believable wrong answer rather than a missing one.
    ///
    /// *"Never called"* is asserted two ways, because neither alone is enough: `RomOpen::last` staying
    /// `None` says no reply was recorded, and `Bus::rom_path()` being unchanged says the machine did not
    /// move. A refusal that dispatched anyway and then discarded the reply would satisfy only the first.
    #[test]
    fn a_dropped_non_rom_is_refused_and_reload_rom_is_never_called() {
        let t = Tmp::new("dropped");
        let cart = t.rom("s4.bin");
        let junk = t.junk("notes.txt");
        let other = t.rom("sonic2.bin");
        let sub = t.dir("acts");
        let (mut machine, mut bus) = rig(&cart);
        let before = bus.rom_path().map(str::to_string);

        // The decision, with no window anywhere.
        let refusal = decide_drop(std::slice::from_ref(&junk));
        let why = match &refusal {
            Dropped::Refused(w) => w.clone(),
            other => panic!("a `.txt` must be refused, not {other:?}"),
        };
        assert!(
            why.contains("notes.txt") && rom_browser::ROM_EXTS.iter().all(|e| why.contains(e)),
            "the refusal must name the file and the extensions that would have worked: {why}"
        );

        let mut r = RomOpen::default();
        r.act_on_drop(&mut machine, &mut bus, refusal);
        assert_eq!(r.said.as_deref(), Some(why.as_str()), "and it is said");
        assert!(
            r.open,
            "a refusal nobody can see is a silent failure: the window must come up"
        );
        assert!(
            r.last.is_none(),
            "a reply was recorded, so the call was made — `reload_rom` must never see a non-image"
        );
        assert_eq!(
            bus.rom_path().map(str::to_string),
            before,
            "the machine moved, so `reload_rom` ran on a file this window refused"
        );

        // Many files at once: refused, for a stated reason, and again nothing is sent.
        let many = decide_drop(&[other.clone(), cart.clone()]);
        let Dropped::Refused(w) = &many else {
            panic!("two files at once must be refused, not {many:?}")
        };
        assert!(w.contains('2'), "the refusal must say how many: {w}");
        r.act_on_drop(&mut machine, &mut bus, many);
        assert!(r.last.is_none());
        assert_eq!(bus.rom_path().map(str::to_string), before);

        // Nothing dropped is nothing done — the case that runs on every other frame of the program's life.
        assert_eq!(decide_drop(&[]), Dropped::Nothing);

        // A folder browses; an image opens. Without these the rows above are satisfied by a
        // `decide_drop` that refuses everything.
        assert_eq!(
            decide_drop(std::slice::from_ref(&sub)),
            Dropped::Browse(sub.clone())
        );
        assert_eq!(
            decide_drop(std::slice::from_ref(&other)),
            Dropped::Open(other.clone())
        );
        r.act_on_drop(&mut machine, &mut bus, Dropped::Open(other.clone()));
        let echo = r.last.as_ref().expect("a dropped image must be loaded");
        assert!(
            !echo.refused,
            "the dropped image was refused: {}",
            echo.line()
        );
        assert_eq!(
            bus.rom_path(),
            Some(oracle_aether::engine::absolutise(&other.to_string_lossy()).as_str()),
            "a dropped image must reach the machine"
        );
    }

    /// ★ **The rows a client can read back are the rows that were drawn**, and the modal is not a hole in
    /// `emulator/screen_text`.
    ///
    /// Driven through the real `show`, on a real `egui::Context`, with no window and no GPU — which is also
    /// the only way to establish that the deciding survives being outside the draw closure.
    ///
    /// The headline's two counts are asserted **derived** (rows shown / rows listed), the selected row is
    /// asserted present *outside* the scroll area, and a closed control is asserted to report **nothing** —
    /// without that leg, a `show` that returned its runs unconditionally would pass.
    #[test]
    fn the_control_reports_what_it_drew_and_reports_nothing_while_closed() {
        let t = Tmp::new("readback");
        let loaded = t.rom("s4.bin");
        t.rom("sonic2.bin");
        let (mut machine, mut bus) = rig(&loaded);

        let ctx = egui::Context::default();
        let mut r = RomOpen::default();

        // Closed: nothing drawn, nothing reported, and no folder read.
        let mut runs = Vec::new();
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            runs = r.show(ui.ctx(), &mut machine, &mut bus);
        });
        std::mem::forget(out);
        assert!(
            runs.is_empty() && !r.attempted,
            "a closed control must draw nothing and read no folder: {runs:?}"
        );

        // Open, listing the running cartridge's own folder because that is where the bus says it is.
        r.open = true;
        let mut runs = Vec::new();
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            runs = r.show(ui.ctx(), &mut machine, &mut bus);
        });
        std::mem::forget(out);
        assert_eq!(
            r.dir.as_deref(),
            Some(t.0.as_path()),
            "the control must open on the folder the BUS says the cartridge came from"
        );

        let text: Vec<&str> = runs.iter().map(|x| x.text.as_str()).collect();
        assert!(
            !text.is_empty(),
            "COULD NOT MEASURE: an open control reported no runs at all"
        );
        // The headline, both counts derived from the state that was drawn.
        let head = r.headline();
        assert!(
            head.contains(&format!("{} of {} rows", r.rows().len(), r.entries.len()))
                && head.contains(&t.0.display().to_string()),
            "the headline must name the folder and derive both counts: {head}"
        );
        assert!(
            text.contains(&head.as_str()),
            "the headline was drawn and not reported: {text:?}"
        );
        // The selected row is drawn outside the scroll area precisely so it CAN be read back.
        let sel = format!(
            "Enter: {}",
            r.rows()[r.sel].display(bus.rom_path().map(Path::new))
        );
        assert!(
            text.contains(&sel.as_str()),
            "the row Enter would run was not reported: wanted {sel:?} in {text:?}"
        );
        assert!(
            sel.contains("[loaded]") || r.rows()[r.sel].entry.kind != EntryKind::Rom,
            "COULD NOT MEASURE: the marker never appeared, so this leg says nothing about it"
        );

        // And this window's own sentences reach the glass too — a refusal a client cannot read is a hole.
        r.said = Some("a sentence this control raised".to_string());
        let mut runs = Vec::new();
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            runs = r.show(ui.ctx(), &mut machine, &mut bus);
        });
        std::mem::forget(out);
        assert!(
            runs.iter()
                .any(|x| x.text == "a sentence this control raised"),
            "the window's own refusal was drawn and not reported: {runs:?}"
        );
    }

    /// ★ **The folder is the running cartridge's, and the odd shapes have answers.**
    ///
    /// `folder_of` is what decides which folder a person sees when they hit the chord, and all three of its
    /// awkward inputs — a bare filename, a path with no parent, and no cartridge at all — arrive in real
    /// use (`--rom s4.bin` from the folder it is in, most obviously).
    #[test]
    fn the_folder_listed_is_the_running_cartridge_s_own() {
        assert_eq!(
            folder_of(Some("/home/x/games/s4.bin")),
            PathBuf::from("/home/x/games")
        );
        // A bare filename has no parent: the working directory, rather than a refusal to open.
        assert_eq!(folder_of(Some("s4.bin")), PathBuf::from("."));
        assert_eq!(folder_of(None), PathBuf::from("."));
        assert_eq!(folder_of(Some("/s4.bin")), PathBuf::from("/"));
    }

    /// ★ **The chord and the label are what this module says they are.**
    ///
    /// Both are `pub const`s so prose cannot drift from what is drawn, and the shortcut must not collide
    /// with the only other chord in this window — asserted against [`crate::palette::SHORTCUT`] itself
    /// rather than against a transcription of `Ctrl+P`.
    #[test]
    fn the_shortcut_is_spelled_once_and_collides_with_nothing() {
        assert_ne!(
            SHORTCUT,
            crate::palette::SHORTCUT,
            "this control and the palette answer the same chord"
        );
        assert_eq!(SHORTCUT.modifiers, Modifiers::CTRL);
        assert_eq!(SHORTCUT.logical_key, Key::O);
        assert_eq!(
            SHORTCUT_LABEL, "Ctrl+O",
            "the human spelling must match the binding above"
        );
        // The label is what the transport bar draws and what `screen_text` reports, so it must be the one
        // in the headline's own sentence about the chord.
        assert!(RomOpen::default().headline().contains(SHORTCUT_LABEL));
        assert!(!OPEN_LABEL.is_empty());
    }

    /// ★ **The selection walks the rows on screen and cannot leave them.**
    ///
    /// A `step` that ran past the end would put `activate` on `None` while a row was still highlighted,
    /// which is the same silent shape as indexing the wrong list.
    #[test]
    fn the_selection_cannot_walk_off_the_visible_rows() {
        let t = Tmp::new("step");
        t.rom("alpha.bin");
        t.rom("beta.bin");
        let mut r = listing(&t.0);
        let n = r.rows().len();
        assert!(n >= 3, "COULD NOT MEASURE: too few rows to walk: {n}");

        r.step(-1);
        assert_eq!(r.sel, 0, "up from the top row stays on the top row");
        r.step(1000);
        assert_eq!(r.sel, n - 1, "down past the end stops on the last row");
        assert!(
            !matches!(r.activate(), Action::None),
            "the selection must still name a row after walking to the end"
        );

        // A filter that shrinks the list under a selection near the end must not leave it dangling.
        r.text = "alpha".into();
        r.step(0);
        assert_eq!(r.sel, r.rows().len() - 1);
        assert!(!matches!(r.activate(), Action::None));
    }

    /// ★★ **A swap that lands re-lists the folder the NEW cartridge came from, and the `[loaded]` marker
    /// follows the cartridge.**
    ///
    /// The defect this test exists for, found by the controller's verification pass and reproduced here:
    /// `open` re-listed `self.dir`, so a file dropped from a folder **other** than the one being browsed
    /// left the window showing the old folder — whose rows carry **no** marker at all, while the image
    /// that *is* running is not on the list. The picture then marks nothing as loaded and the loaded thing
    /// is absent, which is the believable-wrong-answer shape rather than a missing one.
    ///
    /// **The two folders are distinct by construction**, and that is what the old test could not do: the
    /// existing drop test drops an image from the folder it is already listing, where `self.dir` and
    /// `folder_of(rom_path)` coincide and the bug is invisible.
    ///
    /// Driven through **both** entry points, because the fix belongs to the shared sequence and not to the
    /// drop arm: `act_on_drop(Dropped::Open(..))` is the reported route, and a direct `open(..)` is the
    /// row/pasted-path route. A fix that lived in `act_on_drop` would satisfy the first leg and fail the
    /// second.
    #[test]
    fn a_swap_from_another_folder_repoints_the_listing_and_the_marker_follows_the_cartridge() {
        let browsed = Tmp::new("repoint-browsed");
        let here = browsed.rom("here.bin");
        let elsewhere = Tmp::new("repoint-elsewhere");
        let there = elsewhere.rom("there.bin");
        let third = Tmp::new("repoint-third");
        let other = third.rom("other.bin");
        assert!(
            browsed.0 != elsewhere.0 && browsed.0 != third.0,
            "COULD NOT MEASURE: the folders must differ or `self.dir` and `folder_of` coincide and the \
             defect is invisible"
        );

        let (mut machine, mut bus) = rig(&here);
        let mut r = listing(&browsed.0);
        // The control: before the swap the listing is the browsed folder and `here.bin` is marked.
        let marked = |r: &RomOpen, bus: &Bus| -> Vec<String> {
            let cur = bus.rom_path().map(PathBuf::from);
            r.rows()
                .iter()
                .filter(|row| row.marker(cur.as_deref()).is_some())
                .map(|row| row.display(cur.as_deref()))
                .collect()
        };
        assert_eq!(r.dir.as_deref(), Some(browsed.0.as_path()));
        assert_eq!(
            marked(&r, &bus),
            vec!["here.bin   [loaded]"],
            "COULD NOT MEASURE: the running image must be marked before the swap"
        );

        // (1) The reported route: a file dropped from a folder this control is not listing.
        r.act_on_drop(&mut machine, &mut bus, Dropped::Open(there.clone()));
        let echo = r.last.as_ref().expect("the bus must have answered");
        assert!(
            !echo.refused,
            "the swap was refused, so nothing below is about a cartridge that moved: {}",
            echo.line()
        );
        assert_eq!(
            r.dir.as_deref(),
            Some(elsewhere.0.as_path()),
            "the listing must repoint to the folder the NEW cartridge came from; it still describes {:?}",
            r.dir
        );
        assert_eq!(
            marked(&r, &bus),
            vec!["there.bin   [loaded]"],
            "the `[loaded]` marker must land on the image that was just loaded"
        );
        // And the thing that is running is ON the list, which is the half a person would notice.
        assert!(
            r.rows().iter().any(|row| row.entry.path == there),
            "the running cartridge is not among the rows: {:?}",
            labels(&r.rows())
        );
        assert!(
            !r.rows().iter().any(|row| row.entry.path == here),
            "the previous cartridge is still listed, so the folder did not change: {:?}",
            labels(&r.rows())
        );

        // (2) The row / pasted-path route, through the shared sequence directly. Put the listing back on
        // the browsed folder first, so this leg starts from the same mismatch as the one above.
        r.rescan(&browsed.0);
        assert_eq!(r.dir.as_deref(), Some(browsed.0.as_path()));
        r.open(&mut machine, &mut bus, &other);
        assert!(
            !r.last.as_ref().expect("answered").refused,
            "the second swap was refused"
        );
        assert_eq!(
            r.dir.as_deref(),
            Some(third.0.as_path()),
            "a swap through `open` must repoint too — the fix belongs to the shared sequence, not to the \
             drop arm"
        );
        assert_eq!(marked(&r, &bus), vec!["other.bin   [loaded]"]);
    }

    /// ★ **A REFUSED swap re-lists nothing, and a refusal before any listing still gets one.**
    ///
    /// The other half of the rule above, and it is a decision rather than a leftover: the cartridge did
    /// **not** move, so the listing and its `[loaded]` marker are still correct, and re-taking them would
    /// be a folder read nobody asked for on the one path where nothing changed.
    ///
    /// Two states, because `attempted` makes them different and only one of them was ever reasoned about:
    ///
    /// 1. **A listing is already up.** It is left exactly as it was, marker included.
    /// 2. **No listing has ever been taken** — a drop onto a control nobody has opened. `attempted` stays
    ///    false, so the next repaint derives the listing from the **unchanged** `rom_path` and marks the
    ///    cartridge that is still running. Asserted through the real `show`, since `ensure_listing` is
    ///    what closes it.
    #[test]
    fn a_refused_swap_leaves_the_listing_alone_and_still_describes_the_running_cartridge() {
        let t = Tmp::new("refused-listing");
        let running = t.rom("s4.bin");
        let missing = t.0.join("no-such-game.bin");
        let (mut machine, mut bus) = rig(&running);

        // (1) A listing is up, and the refusal must not disturb it.
        let mut r = listing(&t.0);
        let before = r.entries.clone();
        let dir_before = r.dir.clone();
        r.open(&mut machine, &mut bus, &missing);
        assert!(
            r.last.as_ref().expect("answered").refused,
            "COULD NOT MEASURE: this path must be refused, or the test is about a successful swap"
        );
        assert_eq!(r.entries, before, "a refused swap must not re-list");
        assert_eq!(r.dir, dir_before);
        let cur = bus.rom_path().map(PathBuf::from);
        assert_eq!(
            r.rows()
                .iter()
                .filter(|row| row.marker(cur.as_deref()).is_some())
                .map(|row| row.display(cur.as_deref()))
                .collect::<Vec<_>>(),
            vec!["s4.bin   [loaded]"],
            "the cartridge did not move, so the marker must still be on it"
        );

        // (2) A refusal that arrives before any listing was taken: the next repaint still gets one, from
        // the path that did not change.
        let (mut machine, mut bus) = rig(&running);
        let mut fresh = RomOpen::default();
        fresh.act_on_drop(&mut machine, &mut bus, Dropped::Open(missing.clone()));
        assert!(fresh.last.as_ref().expect("answered").refused);
        assert!(
            !fresh.attempted && fresh.dir.is_none(),
            "a refused swap must leave the listing un-attempted, or the repaint below cannot take one"
        );

        let ctx = egui::Context::default();
        let mut runs = Vec::new();
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            runs = fresh.show(ui.ctx(), &mut machine, &mut bus);
        });
        std::mem::forget(out);
        assert_eq!(
            fresh.dir.as_deref(),
            Some(t.0.as_path()),
            "the repaint must list the folder of the cartridge that is STILL running"
        );
        let cur = bus.rom_path().map(PathBuf::from);
        assert!(
            fresh
                .rows()
                .iter()
                .any(|row| row.marker(cur.as_deref()).is_some()),
            "and mark it: {:?}",
            labels(&fresh.rows())
        );
        assert!(
            runs.iter().any(|x| x.text.contains("no-such-game.bin")),
            "the refusal must still be on the glass and readable back: {runs:?}"
        );
    }
}
