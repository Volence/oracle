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
//! **`PANELS-NAV` adds the way in.** Every tab shipped and most were unreachable: `egui_dock` draws only
//! each leaf's active tab, [`initial_dock`] stacks them into four leaves, and the window had no menu, no
//! tab list and no other affordance. Today that is **seven of eleven** behind another title; this line
//! said "six of eight" long after the enum had grown past both figures — lens finding H19, and
//! [`crate::nav`]'s header says why the leaf count rather than the figure is the durable statement.
//! [`crate::nav`] is the repair — a `panels` menu in the
//! top bar, a control rather than a twelfth [`Tab`] for the reason [`Transport`] is one, deriving its rows
//! from [`Tab::ALL`] so the next panel somebody adds cannot be left out of it. Design §5.9.

use crate::bus::Bus;
use crate::machine::Machine;
use crate::memory::{self, MemoryPanel};
use crate::objects::{self, Objects, ObjectsPanel};
use crate::pacing::{self, Governor};
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
    /// **Plane A, plane B and the window drawn whole from the nametable, with no scroll applied** — the
    /// view that answers *what does that floor actually look like* without a scroll offset in the way.
    /// Second, beside `Screen`, because it is the other tab you look at a picture in.
    Planes,
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
    /// **The spawn picker**: arm the mode, and choose the archetype a click on the picture places.
    ///
    /// ⚑ **This one is an exception to *things you DO are controls*, and the exception is the owner's.**
    /// It shipped in the Screen tab's control strip on that rule and he overturned it after using it:
    /// *"the placement works well it seems! it just takes up a lot of space haha. Maybe it should be its
    /// own debug tool in the right panel instead of part of screens?"* The rule stands for one-shot
    /// gestures; a picker is a **standing list you read while looking somewhere else**, and a strip is
    /// the one place a list of that size cannot go, because every row it grows is a row of the game view.
    /// See `crate::palette`'s header, amended in the same change.
    ///
    /// The *badge* stays in the Screen strip. A mode that changes what a left-click does must say so
    /// beside the thing being clicked, and that is not what moved here.
    Spawn,
    /// **The live effects switchboard** (`LIVE-EFFECTS`): pick a scene, a raster program or a band table
    /// by name and write the cells the engine's own installer writes, so the running game switches to it.
    ///
    /// A tab rather than a control, on the Spawn tab's own precedent: *things you DO are controls* holds
    /// for one-shot gestures, and this is a **standing list you read while looking at the picture**, plus
    /// a readback of what the machine actually holds. The one thing that is not a list — the standing
    /// statement that this panel is overriding the running game — is drawn here **and** in the Screen
    /// tab's control strip (`Panels::screen_controls`), because this tab shares a dock leaf with `Pacing`
    /// and `Spawn` and a statement that can be behind a tab bar is not standing. Both draw the identical
    /// `crate::effects::Panel::statement`, so they cannot describe one override two ways.
    Effects,
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
    pub const ALL: [Tab; 11] = [
        Tab::Screen,
        Tab::Planes,
        Tab::Pacing,
        Tab::Registers,
        Tab::Memory,
        Tab::Objects,
        Tab::Spawn,
        Tab::Effects,
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
            Tab::Planes => "Planes",
            Tab::Pacing => "Pacing",
            Tab::Registers => "Registers",
            Tab::Memory => "Memory",
            Tab::Objects => "Objects",
            Tab::Spawn => "Spawn",
            Tab::Effects => "Effects",
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
    /// The Planes tab's own state: which plane, the two toggles, and the texture it last rasterised with
    /// the fingerprint that texture was drawn from. `&mut` because the whole render-on-change decision
    /// lives in there, and a panel that re-derived it every repaint would be the thing it exists to avoid.
    pub planes: &'a mut crate::planes::Panel,
    /// The Effects tab's own state: which channel, what is typed in its two boxes, the last search, the
    /// last gesture's answer, and **what this panel has overridden and not put back**. `&mut` because
    /// every gesture on it writes engine RAM. Nothing in it is persisted; see [`crate::effects::Panel`].
    pub effects: &'a mut crate::effects::Panel,
    /// **The ten save-state slots** (S3). `&mut` because the Screen tab's slot controls are *controls*:
    /// things you do are not tabs, and a slot readout you could not act on would send the operator back
    /// to a function key they have to already know about.
    pub states: &'a mut crate::states::States,
    /// **The cartridge's `.srm`** (S3), needed by exactly one gesture on this tab — a state load, whose
    /// first act is flushing it. It is not a display: nothing here reads it, and it is here rather than
    /// inside [`crate::states::States`] because the battery outlives a slot and belongs to the loop.
    pub battery: &'a mut crate::battery::Battery,
    /// ⚑ **The display mask [`tex`](Panels::tex) was drawn under**, or `None` before the first upload.
    ///
    /// The mask of *the picture on the glass* — read off the uploaded texture, deliberately not off the bus.
    /// The bus's mask is what the machine has been told; this is what a person is looking at, and the two
    /// separate for as long as it takes a change to reach the next upload. `crate::screen_pick` refuses a
    /// click on exactly that gap rather than describing a picture that is not there.
    pub screen_mask: Option<oracle_core::render::LayerMask>,
    pub governor: &'a Governor,
    /// **The pacing figures `emulator/pacing` is serving right now** — the one value `Loop::iterate`
    /// derived this frame and published to the bus, borrowed rather than recomputed. §11.42's serve and
    /// this tab are two readers of it; see [`oracle_aether::engine::PacingFacts`].
    pub pacing: &'a oracle_aether::engine::PacingFacts,
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
            Tab::Planes => "planes",
            Tab::Pacing => "pacing",
            Tab::Registers => "registers",
            Tab::Memory => "memory",
            Tab::Objects => "objects",
            Tab::Spawn => "spawn",
            Tab::Effects => "effects",
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
                // Controls first, then the picture with whatever is left — the order is the layout, and
                // `screen` allocates all of the remainder. ⚑ The strip is drawn through
                // [`screen_strip`], which bounds it, because "all of the remainder" was reachably
                // NOTHING: see that function for why the tab's own scroll area cannot fix that.
                screen_strip(ui, |ui| self.screen_controls(ui));
                self.screen(ui);
            }
            Tab::Planes => self.planes(ui),
            Tab::Pacing => self.pacing(ui),
            Tab::Registers => self.registers(ui),
            Tab::Memory => self.memory(ui),
            Tab::Objects => self.objects(ui),
            Tab::Spawn => self.spawn(ui),
            Tab::Effects => self.effects(ui),
            Tab::Breakpoints => self.breakpoints(ui),
            Tab::Watchpoints => self.watchpoints(ui),
            Tab::Profiler => self.profiler(ui),
        }
    }
}

/// What the Screen tab says when the pane cannot hold a picture at all.
///
/// Advice about *size* rather than about width: the reachable cause is the control strip above eating the
/// pane's **height**, not a narrow pane. See [`screen_room`].
const NO_ROOM_FOR_SCREEN: &str = "no room to draw the picture here; make this pane larger";

/// **The picture's size, or the sentence to say instead.** There is no third outcome, and that is the
/// point of the `Result`: a caller that handles this at all cannot handle it silently.
///
/// ⚑ **The defect this exists to make unrepresentable** (`F-SCREEN-PICTURE-SILENT`). [`Panels::screen`]
/// used to read `screen_pick::fit` straight and `return` on a zero, drawing nothing and saying nothing.
/// An empty pane is indistinguishable from a broken one — the rule `machine.rs` states as *no picture
/// rather than a black rectangle presented as one* cuts both ways, and a refusal that renders as blank is
/// the same lie with the sign flipped.
///
/// **The condition, exactly.** [`screen_pick::fit`] returns `Vec2::ZERO` only when a whole device pixel
/// does not fit: `(avail * ppp).floor()` is zero on either axis, or `ppp`/`avail` is not finite. It never
/// returns zero merely for a *narrow* pane, because `present::dest_rect` floors its scale at `max(1)` and
/// hands back one whole aspect unit however little room there is. So this is a **collapse**, not a
/// squeeze, and the way it is reached here is the height: `Tab::Screen` draws
/// [`Panels::screen_controls`] above this in the same vertical stack with no bound of its own, so a short
/// pane and a long strip leave `available_size().y` at zero. That is the same mechanism as the
/// `plane_split` bug next door — a sibling in the stack takes the whole allocation — on the other axis.
fn screen_room(
    avail: egui::Vec2,
    src_w: usize,
    src_h: usize,
    ppp: f32,
    aspect: oracle_frontend::present::Aspect,
) -> Result<egui::Vec2, &'static str> {
    let size = screen_pick::fit(avail, src_w, src_h, ppp, aspect);
    if size.x <= 0.0 || size.y <= 0.0 {
        return Err(NO_ROOM_FOR_SCREEN);
    }
    Ok(size)
}

/// The Screen tab **with no picture in it**, and why, in the one shape both such branches use.
///
/// The readout comes back as a laid-out card here rather than as the overlay [`Panels::screen`] paints on
/// the glass, for the reason that branch already gave: the overlay exists so a click's answer cannot move
/// the picture, and with no picture there is nothing to move and nothing to paint it over. Losing the
/// standing answer to the last click along with the picture would be the same silence one level down.
fn no_picture(ui: &mut egui::Ui, why: &str, readout: Option<&screen_pick::Readout>) {
    ui.label(why);
    if let Some(r) = readout {
        readout_card(ui, r);
    }
}

/// **The most of the Screen pane the control strip may ever take**, as a share of the pane's height.
///
/// ⚑ **Not a tuned number, and the reason it is one half rather than a fraction someone liked the look
/// of.** The rule it expresses is *the strip may not take more of the pane than it leaves for the
/// picture*, and one half is the only value that states which of the two siblings is the larger. Every
/// other share is a preference with a knob on it.
///
/// The consequence is the property [`screen_strip_cap`]'s gate asserts and is what makes this safe to
/// apply unconditionally: the bound **bites only where the picture was already the minority**. A strip
/// whose natural height is under half the pane is not touched at all, so nothing about the tab changes at
/// any ordinary size; a strip over half the pane is exactly the case in which today's build hands the
/// picture less than half — and, past `avail`, hands it nothing.
const SCREEN_STRIP_MAX_SHARE: f32 = 0.5;

/// The strip's height budget in a pane of `available_height` points. See [`SCREEN_STRIP_MAX_SHARE`].
///
/// ⚑ **Unmeasurable is unbounded, never zero.** A pane whose height is not a finite positive number is
/// not a short pane, it is a broken one, and a cap of `0` there would erase the strip — the mask
/// statement, the effects statement and the spawn badge with it — with nothing on screen to say why. So
/// the bound stands down and the pane is left to [`screen_room`], which refuses a non-finite `avail`
/// outright and paints [`NO_ROOM_FOR_SCREEN`]. Loud on unmeasurable, in the one direction that says so.
fn screen_strip_cap(available_height: f32) -> f32 {
    if !available_height.is_finite() || available_height <= 0.0 {
        return f32::INFINITY;
    }
    available_height * SCREEN_STRIP_MAX_SHARE
}

/// What [`screen_strip`] did, for the gate. The tab ignores it.
///
/// The fields are read only by `screen_strip_tests`, and that is the point rather than an oversight: the
/// alternative is a gate that re-derives the budget and the occupancy from egui's own internals, which
/// is a second implementation of the thing under test. `screen_strip` reports what it did; nothing has to
/// guess.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))]
struct StripRoom {
    /// The budget [`screen_strip_cap`] set from the pane's height, before anything was drawn.
    cap: f32,
    /// The height the strip actually occupied, which is what the picture does not get.
    ///
    /// ⚑ Measured off **the caller's cursor**, not off `ScrollAreaOutput::inner_rect`. That field is read
    /// before the area shrinks to its content (`egui-0.36.1/src/containers/scroll_area.rs:1038` takes it
    /// from `prepared`, and the shrink happens in `end` at `:1184`), so it reports the *budget* on a
    /// strip that fits — a first draft of this read it and had a two-row strip occupying half an
    /// 800-point pane.
    shown: f32,
    /// The height the strip's content wanted. Greater than `shown` means it is scrolling.
    wanted: f32,
}

/// **The Screen tab's vertical split**: the control strip under a height bound, then the rule, then
/// whatever is left for the picture — which the caller draws.
///
/// ⚑ **The defect** (`F-SCREEN-TAB-STRIP-UNBOUNDED`). `Tab::Screen` used to call
/// [`Panels::screen_controls`] straight, and the strip is a column of up to nine rows, six of them
/// conditional (`docs/2026-09-09-palette-shape-and-the-strip.md` §2.1). Nothing bounded it, and
/// [`Panels::screen`] takes `ui.available_size()` — so the strip took its natural height and **the
/// picture absorbed the whole deficit**, down to zero and past it. The owner's symptom was a Screen tab
/// with no picture in it.
///
/// # Why this is not "make the tab scroll", which is what the row asked for
///
/// **Because the tab already scrolls, and it does not help.** `egui_dock` wraps every tab body in
/// `ScrollArea::new(tab_viewer.scroll_bars(tab))` (`egui_dock-0.21.1/src/widgets/dock_area/show/
/// leaf.rs:1390`), `TabViewer::scroll_bars` defaults to `[true, true]`, and [`Panels`] does not override
/// it. That scroll area has been there the whole time. It cannot reach this defect for a reason that is
/// structural rather than incidental: **a scroll area scrolls content that overflows, and this content
/// never overflows.** The picture is elastic — it asks for what is left — so the strip's excess is
/// swallowed rather than pushed past the viewport, `content_size` stays inside `inner_size`, and there is
/// nothing to scroll. Scrolling a pane whose picture is zero points tall shows you a zero-point picture
/// at every offset.
///
/// So the cure is not a scroll area over the whole tab. It is **a floor under the picture**, and the only
/// way to give the picture a floor is to give the strip a ceiling — [`screen_strip_cap`].
///
/// # Why the strip and not the tab gets the scrollbar
///
/// Putting the whole tab in a scroll area (or, equivalently, letting the picture push past the viewport
/// so `egui_dock`'s own one engages) trades a picture that is too short for a picture that is *not in the
/// pane*: the strip is drawn first, so at scroll offset zero — where the tab opens — you would see the
/// controls and no game at all, and the thing the tab exists for would be below the fold. Bounding the
/// strip keeps the picture on screen at every size and puts the scrollbar on the column of text, which is
/// the half a person scrolls through by nature.
///
/// **Nothing here changes what the strip looks like.** The owner has a parked look call on this exact
/// strip (`docs/2026-09-09-palette-shape-and-the-strip.md` §2, *"the box above screen kind of looks bad
/// too imo"*) and it is his: no row is added, removed, reordered or restyled. `max_height` is a cap and
/// not a reservation, so a strip shorter than the cap is laid out exactly as it is today, and a scroll
/// area with nothing to scroll draws no bar at all. The only pane in which anything looks different is
/// the one where the picture is currently gone.
///
/// ⚠ **What does change there, stated rather than discovered later.** `crate::theme` sets
/// `scroll.floating = false` with a 6-point bar (CHROME_SPEC), so on the frames the strip *is* scrolling
/// the bar takes 6 points of width from the strip's rows. That is the theme's own bar at the theme's own
/// width, in the regime where the alternative is content cropped with no way to reach it — but it is the
/// one visible consequence of this parcel and it wants the owner's eye, not this seat's.
///
/// `auto_shrink` is off **across** and on **down**, the pairing this file already argues for at the
/// subtype list: off across so the scrollbar sits at the pane's edge rather than inside the text, and so
/// the strip's prose wraps against the same width it wraps against today; on down so a two-line strip
/// does not hold half a pane of empty box open under it.
///
/// `min_scrolled_height(0.0)` because egui otherwise floors a scrollable area at 64 points
/// (`egui-0.36.1/src/containers/scroll_area.rs:399`, applied at `:776`), which would break the bound in
/// exactly the panes it exists for — a 90-point pane would hand the strip 64 of it and the picture 26.
fn screen_strip(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) -> StripRoom {
    let before = ui.available_height();
    let cap = screen_strip_cap(before);
    let out = egui::ScrollArea::vertical()
        // P7: an explicit, stable salt. The Screen tab's strip owns its own scroll position.
        .id_salt("screen_controls")
        .max_height(cap)
        .min_scrolled_height(0.0)
        .auto_shrink([false, true])
        .show(ui, add);
    // What the strip cost the pane, off the cursor rather than off the scroll area's own report: see
    // [`StripRoom::shown`]. The `item_spacing` comes back out because egui advances the cursor past the
    // widget *and* the gap before the next one, and that gap is the picture's business, not the strip's.
    let shown = before - ui.available_height() - ui.spacing().item_spacing.y;
    // The rule between the strip and the picture, exactly where it was: outside the scroll area, so it
    // marks the boundary of the strip's box rather than scrolling away with the strip's last row.
    ui.separator();
    StripRoom {
        cap,
        shown,
        wanted: out.content_size.y,
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
            // ⚑ **One of two branches where the readout is laid out rather than overlaid**, and both are
            // stated rather than silent. See [`no_picture`], which is the shape they share: the other is
            // the no-room branch below, which used to be a bare `return`.
            no_picture(ui, "no frame yet", self.screen.readout());
            return;
        };
        // ⚑ **Retire a picture whose art has been replaced, immediately before the ghost can draw it.**
        // Checked here as well as in the Spawn tab, because either surface can be the visible one and a
        // guard at one of two use sites is a guard that is missing at the other. See
        // [`screen_pick::Panel::expire_preview`] for why this retires rather than retakes.
        self.screen.expire_preview(self.machine.system().vdp());
        let src = tex.size_vec2();
        let ppp = ui.pixels_per_point();
        // One reading of the available space, used for both the fit and the allocation. Two calls would
        // be two readings of a thing that can change, and the picture would then be fitted to one box and
        // centred in another.
        let avail = ui.available_size();
        // ⚑ **Never a silent return** (`F-SCREEN-PICTURE-SILENT`). The room is asked for through
        // [`screen_room`], which answers with a size or with the sentence to say instead; there is no
        // arm of it that draws nothing. What used to be here was `if size.x <= 0.0 { return; }`, so a
        // pane whose control strip had taken all the height showed controls and then nothing, with no
        // statement anywhere that a picture was being withheld.
        let size = match screen_room(
            avail,
            src.x as usize,
            src.y as usize,
            ppp,
            self.screen.aspect,
        ) {
            Ok(size) => size,
            Err(why) => {
                no_picture(ui, why, self.screen.readout());
                return;
            }
        };
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

        // ⚑ **The ghost**, under the pointer, for as long as a click would place something.
        //
        // The owner's ask, and the scope in his own words: *"we don't have to actually draw it in the
        // game, like where my mouse is when placing it have roughly the size and shape of sprite as a
        // preview from the shell of oracle."* So it is drawn by this window over the picture, and nothing
        // about it reaches the machine.
        //
        // It is drawn **only while the mode is armed and the pointer is on the picture**, which is the
        // same condition as "a click here would place one", so the ghost is never a claim about a gesture
        // that is not available.
        if self.screen.is_armed() {
            // ⚑ **The armed statement, ON THE PICTURE, and unconditional inside this branch.**
            //
            // The owner's finding, 2026-09-09: *"With spawn, if I want to click into the window to move
            // the character around (even if it shows nothing) it'll spawn something there."* The ghost
            // below was the only thing this window drew inside the picture while armed, and it is
            // conditional twice over — a pointer that is over the picture *and* a preview that is
            // drawable. Both of the states he described fall outside it, so the picture that ate his
            // click looked exactly like an unarmed one. This does not: it is drawn from
            // [`screen_pick::Panel::is_armed`] and nothing else, which is the same predicate the click
            // itself reads.
            //
            // The frame is what carries it at a glance and the chip is what makes it say *what* and *how
            // to get out*; neither is decoration, and the wording is
            // [`screen_pick::Panel::armed_notice`]'s, derived from the badge so the strip and the glass
            // cannot name two different modes.
            if let Some(notice) = self.screen.armed_notice() {
                armed_frame(ui, image_rect, &notice);
            }
            if let (Some(p), Some(pos)) = (
                self.screen
                    .preview()
                    .and_then(crate::preview::Outcome::drawable),
                hit.hover_pos(),
            ) {
                ghost(ui, p, image_rect, pos, size.x / src.x);
            }
        }

        // ⚑ **The last click's answer, OVER the picture and not above it.**
        //
        // The owner's second finding, 2026-09-09: *"whenever I place something this text box comes up and
        // shifts the window for the game which doesn't feel great or look good."* It was a card in
        // [`Panels::screen_controls`], which is drawn **above** the picture in the same vertical stack, so
        // [`Panels::screen`] got `available_size()` minus the card and the picture jumped down by the
        // height of a sentence the moment a click produced one — and jumped back the next time the
        // sentence was shorter.
        //
        // Deleting it was never an option: it is the standing answer to the click, and the whole reason
        // this panel is honest about a refusal. So it moves onto the glass, where it costs no layout at
        // all: [`overlay_block`] paints through `Painter` against a rect that was already allocated, so
        // there is no arrangement in which it can move anything.
        //
        // **Bottom left**, opposite the armed notice at the top: the two are simultaneously present on
        // every placement, and a corner each is what keeps them from stacking into the single tall block
        // that is what he was complaining about in the first place.
        if let Some(r) = self.screen.readout() {
            readout_overlay(ui, image_rect, r);
        }

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
    /// a standing statement a person has to go looking for is not standing.
    ///
    /// ⚑ **Amended by `F-SCREEN-TAB-STRIP-UNBOUNDED`, and the old sentence was wrong when it was
    /// written.** It read *"a standing statement that can be scrolled or cropped out of view is not
    /// standing"*, and offered the strip's unbounded height as the thing that guaranteed it. It never
    /// did: an unbounded strip in a pane too short for it was **cropped** at the pane's edge with no
    /// scrollbar and no way to reach the rest, so the lines below the fold were not merely scrollable,
    /// they were unreachable. [`screen_strip`] bounds the strip and gives it a scrollbar, which trades
    /// that for lines that are off-screen **and reachable**. What actually holds the guarantee is the
    /// order: these statements are drawn first, so they are what the top of the strip shows, and the pane
    /// opens on them.
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
        // ⚑ **The effects switchboard's standing statement, HERE as well as on its own tab** (`LIVE-EFFECTS`).
        //
        // The badge above earned this position with the argument that a mode changing what a left-click
        // does must say so beside the thing being clicked. An effects override is the same claim about a
        // wider subject: it changes what **every frame** looks like, and the frame it produces is a
        // perfectly ordinary-looking picture of a configuration the act does not have. A person who has
        // forgotten reads it as the game's own, which is the layer mask's defect with a bigger blast
        // radius.
        //
        // **A tab cannot carry it alone**, and that is the whole reason for the duplication: `Tab::Effects`
        // shares a dock leaf with `Pacing` and `Spawn`, so the moment a person looks at either of those the
        // statement is behind a tab bar. A standing statement that can be hidden is not standing. The
        // picture, meanwhile, is the thing being lied about, so this strip is where the claim has to be.
        //
        // **One derivation, two readers** (`crate::effects::statement`, through `Panel::statement`): the
        // strip and the tab draw the identical `String` and cannot describe one override two ways. Nothing
        // is re-composed here.
        if let Some(s) = self.effects.statement() {
            ui.colored_label(ui.visuals().warn_fg_color, s);
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
        // ⚑ **What this window did to the machine's run state**, standing, in the strip rather than in a
        // toast. Required by `docs/2026-09-05-spawn-autopause-design.md`'s ruling and not optional: a
        // click now pauses the machine for a frame or two and puts it back, and a resume the person did
        // not perform must never be a mystery. It is coloured from `RunState::alarming()` and never from
        // the shape of the sentence, which is P5's rule generalised off refusals.
        //
        // ⚑ **The sentence comes from the panel, deed and all.** Two gestures pause and restore now, the
        // click that places and the selection that takes a picture, and `RunState::sentence()` would name
        // a placement after either. The panel knows which one it was; this strip does not, and must not
        // pick.
        if let Some((sentence, alarming)) = self.screen.run_line() {
            let colour = if alarming {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().weak_text_color()
            };
            ui.colored_label(colour, sentence)
                .on_hover_text(crate::spawn_picker::RunState::HELD_INPUT_HOVER);
        }
        ui.horizontal(|ui| {
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
        // ⚑ **The picker used to be drawn here and is now [`Tab::Spawn`]**, on the owner's own reversal
        // of the rule that put it here: *"the placement works well it seems! it just takes up a lot of
        // space haha. Maybe it should be its own debug tool in the right panel instead of part of
        // screens?"* Every row the list grew was a row taken off the game view, because this strip sits
        // above the picture and the picture gets what is left.
        //
        // **What deliberately did not move: the badge and the run-state line above.** Both are standing
        // statements about *this picture* — that a click places instead of picks, and that the window
        // paused and resumed the machine — and a statement that can be behind another tab is not
        // standing. The picker is a list you read; those two are facts you must not be able to miss.
        //
        // ⚑ **The save-state slots** (S3), as controls beside the keys rather than instead of them.
        //
        // `oracle-frontend` offers these on `F2`/`F4`/`F6`/`F7`/`0`-`9` and nowhere else, which is fine
        // for hands that already know them and is invisible to anyone else. The keys are bound here too
        // (`crate::input::machine_keys`, the incumbent's bindings unchanged) — this row is what makes the
        // capability *findable*, and it names the keys so the row teaches them rather than replacing
        // them.
        //
        // The occupancy dots are read from [`crate::states::States`]'s probe of the filesystem, not from
        // a count this window keeps: a slot file written by the other window, or deleted outside both,
        // must show as it is.
        ui.horizontal(|ui| {
            ui.weak("state:");
            if ui.button("◀").on_hover_text("F6: previous slot").clicked() {
                self.states.step(-1);
            }
            let slot = self.states.slot();
            ui.monospace(format!(
                "slot {slot} {}",
                if self.states.occupied(slot) {
                    "(occupied)"
                } else {
                    "(empty)"
                }
            ));
            if ui.button("▶").on_hover_text("F7: next slot").clicked() {
                self.states.step(1);
            }
            if ui
                .button("save")
                .on_hover_text("F2: write this machine to the selected slot")
                .clicked()
            {
                self.states.save(self.machine);
            }
            // Offered whether or not the slot has a file, and **refused by the loader** when it has none
            // — the transport bar's rule for `step`: a hidden control teaches nothing, and the refusal
            // names the file. `save_state::load` is a static constructor, so a refusal cannot leave a
            // half-restored machine on the glass.
            if ui
                .button("load")
                .on_hover_text("F4: restore this machine from the selected slot")
                .clicked()
            {
                let mut said = Vec::new();
                self.states
                    .load(self.machine, self.battery, self.bus, &mut said);
                for line in said {
                    crate::device::loud(&line);
                }
            }
        });
        if let Some(n) = self.states.last() {
            // Coloured on the field, never on the shape of the text — `Echo`'s rule, and `Note` carries
            // the same flag for the same reason.
            let colour = if n.refused {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().weak_text_color()
            };
            ui.colored_label(colour, &n.text);
        }
    }

    /// **The Spawn tab** — arm the mode, then choose the archetype a click on the picture places.
    ///
    /// # Why this is a tab, when the rule says a thing you DO is a control
    ///
    /// Because the owner used it and said so. It shipped inside [`Panels::screen_controls`] on
    /// `crate::palette`'s rule and his verdict was *"the placement works well it seems! it just takes up
    /// a lot of space haha. Maybe it should be its own debug tool in the right panel instead of part of
    /// screens?"* The strip is drawn **above** the picture and [`Panels::screen`] allocates whatever is
    /// left, so a list of archetypes and the game view were competing for the same pixels and the list
    /// was winning. The rule is amended in `palette.rs`'s header rather than quietly broken here.
    ///
    /// # What did NOT move, and why the split is where it is
    ///
    /// The **badge** (a click places instead of picks) and the **run-state line** (this window paused
    /// your machine and put it back) stay in the Screen strip. Both are standing statements about the
    /// picture, and a standing statement that can be behind another tab in a dock leaf is not standing.
    /// A picker is the opposite: a list you go and read, which is exactly what a tab is for.
    ///
    /// **Arming and placing are untouched.** The button calls the same
    /// [`screen_pick::Panel::arm_spawn`]/[`screen_pick::Panel::disarm_spawn`], a row calls the same
    /// [`screen_pick::Panel::select_archetype`], and the click that places is still
    /// [`Panels::screen`]'s, on the picture, through the same [`screen_pick::Panel::click`]. This
    /// function moved a layout, not a gesture.
    ///
    /// # The shape
    ///
    /// `docs/2026-09-05-debug-window-audit.md` §1 offers three. A list of symbol names is **rows of
    /// like-shaped data**, so it is a **column table** with the selection carried by fill — [`select_list`],
    /// unchanged, which is that furniture's single-column form. The count is a **labelled fact** on one
    /// small line and not a big-number readout, because nobody opens a picker to read a count. The two
    /// facts that are not counts are sentences, per P6.
    fn spawn(&mut self, ui: &mut egui::Ui) {
        let weak = ui.visuals().weak_text_color();
        self.rings(ui);
        ui.separator();
        ui.horizontal(|ui| {
            // The armed/disarmed split is one button for one question, the same rule the transport bar
            // states. It lives here rather than in the strip so the tab is whole: a picker you can read
            // but not arm would send the reader back to the panel this one was lifted out of.
            //
            // ⚑ `object_armed`, not `is_armed`: a window armed for RINGS is armed, and asking the wider
            // question here would offer "spawn mode off" for a mode that is already off and hide the
            // archetype list behind one that has nothing to do with it.
            if self.screen.object_armed() {
                if ui
                    .button("spawn: off")
                    // Names the key as well as the button, because the key is the one that is reachable
                    // from the picture — which is where a person is when they want out. The word for it
                    // is `screen_pick::DISARM_KEY_LABEL`, so this and the notice on the glass cannot
                    // name two different keys.
                    .on_hover_text(format!(
                        "a click on the picture goes back to arming a watch. {} does the same from \
                         anywhere in this window.",
                        screen_pick::DISARM_KEY_LABEL
                    ))
                    .clicked()
                {
                    self.screen.disarm_spawn();
                }
            } else if ui
                .button("spawn mode…")
                .on_hover_text(
                    "reads this build's archetypes and arms the first, so a click on the Screen tab's \
                     picture places it",
                )
                .clicked()
            {
                self.screen.arm_spawn(self.machine, self.bus);
            }
            // ⚑ The badge, echoed here **as the armed archetype's name** rather than as a second copy of
            // the Screen tab's standing warning: this tab is where the selection is made, and a picker
            // that did not show what is currently armed would be a list with no answer in it.
            match self.screen.badge() {
                Some(b) => {
                    ui.colored_label(ui.visuals().warn_fg_color, b);
                }
                None => {
                    ui.label(
                        egui::RichText::new("off: a click on the picture arms a watch")
                            .text_style(egui::TextStyle::Small)
                            .color(weak),
                    );
                }
            }
        });
        if !self.screen.object_armed() {
            // P6 again: the reason there is no list is a sentence, never an empty tab.
            ui.label(
                egui::RichText::new(
                    "Spawn mode is off, so this build's archetypes have not been read yet. Arm it \
                     above and the list appears here; the click that places one is on the Screen \
                     tab's picture, as it always was.",
                )
                .text_style(egui::TextStyle::Small)
                .color(weak),
            );
            return;
        }
        ui.separator();
        // ⚑ **Retire a picture whose art has been replaced, before anything can draw it.**
        //
        // Cheap: a fingerprint over the colour table and the handful of tiles the picture was drawn from.
        // Retired rather than retaken, because retaking here would put a checkpoint round trip in a draw
        // pass and would run one on every frame of an act load, while the art is still arriving. The
        // button below is how a person takes it again, and until they do, nothing is drawn: a real picture
        // of the wrong tiles is the failure the whole cache key exists to prevent.
        self.screen.expire_preview(self.machine.system().vdp());
        if let Some(out) = self.screen.preview() {
            let out = out.clone();
            preview_card(ui, &out);
        }
        if ui
            .button("take the picture again")
            .on_hover_text(
                "puts one of this object into the machine, reads the sprites it draws, and puts the                  machine back where it was",
            )
            .clicked()
        {
            self.screen.take_preview(self.machine, self.bus);
        }
        ui.separator();
        self.subtypes(ui);
        ui.separator();
        let listing = self.screen.listing();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("filter")
                    .text_style(egui::TextStyle::Small)
                    .color(weak),
            );
            ui.add(
                egui::TextEdit::singleline(self.screen.filter_mut())
                    .desired_width(140.0)
                    // ⚑ `F-PICKER-HINT-READS-AS-QUERY`: a bare `ring` in an empty box was read by a
                    // careful reader as a typed query. `type to filter` cannot be, because it is an
                    // instruction rather than a value.
                    .hint_text("type to filter"),
            )
            .on_hover_text(
                "narrows the list below. It never changes what a click places: the badge above always \
                 names the armed archetype, filtered into view or not.",
            );
            ui.label(
                egui::RichText::new(&listing.count)
                    .text_style(egui::TextStyle::Small)
                    .color(weak),
            );
        });
        // A partial measurement, said out loud. A list drawn from the first 20 of 137 archetypes with
        // nothing saying so is this window deciding, on the reader's behalf, that the other 117 do not
        // exist.
        if let Some(t) = &listing.truncation {
            ui.label(
                egui::RichText::new(t)
                    .text_style(egui::TextStyle::Small)
                    .color(crate::theme::WARNING),
            );
        }
        match &listing.absence {
            // P6: an absent result is a stated line, never an empty box.
            Some(a) => {
                ui.label(
                    egui::RichText::new(a)
                        .text_style(egui::TextStyle::Small)
                        .color(weak),
                );
            }
            None => {
                if let Some(name) = select_list(ui, "archetype", &listing.rows, "spawn_picker") {
                    self.screen.select_archetype(self.machine, self.bus, &name);
                }
            }
        }
    }

    /// **The Effects tab** (`LIVE-EFFECTS`): pick a scene, a raster program or a band table by name and
    /// write the cells the engine's own installer writes.
    ///
    /// # The order is the argument
    ///
    /// The **standing statement goes first**, above everything, on the layer mask's own rule: a surface
    /// that changes what every subsequent frame looks like must say so where the eye lands, not where the
    /// controls happen to end. Then the readback of what the machine actually holds, then the list, then
    /// what the last gesture did. The write-set and the disabled nudge control sit at the bottom because
    /// they are read once and then known.
    ///
    /// # ⚑ Every gesture that writes is pause, write, resume, and the machine says so
    ///
    /// [`crate::effects::Panel::gesture`] is the one path, and it calls
    /// [`crate::screen_pick::paused_for`] rather than re-implementing it. Nothing on this tab writes to a
    /// running machine: `emulator/write_memory` refuses one, and a paused write cannot land mid-frame,
    /// which is what dissolves aeon's torn-frame caveat for this panel.
    ///
    /// This function lays out and decides nothing; every string it draws is
    /// [`crate::effects`]'s. See that module's header for the RAM surface and its citations.
    fn effects(&mut self, ui: &mut egui::Ui) {
        let weak = ui.visuals().weak_text_color();
        let channel = self.effects.channel();

        // ⚑ THE STANDING STATEMENT. Drawn on every frame a selection made here is in effect, never as a
        // toast: a toast expires and the override does not. Above the controls, because a person reading
        // a picture they have forgotten they changed is the failure this line exists to prevent.
        if let Some(s) = self.effects.statement() {
            ui.label(
                egui::RichText::new(s)
                    .text_style(egui::TextStyle::Small)
                    .strong()
                    .color(crate::theme::WARNING),
            );
            ui.separator();
        }

        // Which channel. Three, so three buttons rather than a dropdown: a menu hides two of the three
        // answers behind a click, and this is the choice the whole tab is about.
        ui.horizontal(|ui| {
            for c in crate::effects::CHANNELS {
                let on = c.key == channel.key;
                if ui
                    .selectable_label(on, c.title)
                    .on_hover_text(c.subject)
                    .clicked()
                {
                    self.effects.look_at(c.key);
                }
            }
        });
        // ⚑ **Re-read after the row, because a click in it just changed the answer.** `channel` above was
        // taken before the buttons were drawn, and everything below this line describes a channel: the
        // subject sentence, the off control, the readback, the prefix box, the write-set. Keeping the
        // stale binding would draw one frame of the OUTGOING channel's headings over the INCOMING
        // channel's state, which is a label that does not match what it labels. One frame is enough:
        // that is the whole failure class this panel is written against, and there is no reason to
        // produce a small instance of it inside the panel that exists to prevent the large one.
        let channel = self.effects.channel();
        ui.label(
            egui::RichText::new(channel.subject)
                .text_style(egui::TextStyle::Small)
                .color(weak),
        );

        // What the run state was left as. Standing, in this panel's own words, and coloured on
        // `run_alarming` rather than on the shape of the sentence (P5).
        if let Some(line) = self.effects.run_line() {
            let colour = if self.effects.run_alarming() {
                crate::theme::WARNING
            } else {
                weak
            };
            ui.label(
                egui::RichText::new(line)
                    .text_style(egui::TextStyle::Small)
                    .color(colour),
            )
            .on_hover_text(crate::spawn_picker::RunState::HELD_INPUT_HOVER);
        }
        ui.separator();

        // ⚑ WHAT THE MACHINE ACTUALLY HOLDS. The antidote to the exact hour aeon lost: a readback, never
        // an echo of the last click. On the raster channel it goes on naming the old program until a
        // frame runs, which is true and is the point.
        ui.horizontal(|ui| {
            if ui
                .button("read it back")
                .on_hover_text(
                    "reads the cell the engine looks at and names what it points to. A pure read: the \
                     machine is not paused for it.",
                )
                .clicked()
            {
                self.effects.refresh_live(self.machine, self.bus);
            }
            // ⚑ **A channel with no off state draws the control DISABLED with the reason**, rather than
            // an enabled button that refuses on every click. Same call the nudge control makes and for
            // the same argument: the person who looks for an off switch and finds nothing concludes it
            // is missing, and the one who clicks a button that always refuses concludes it is broken.
            // A greyed control with a sentence is the only one of the three that is true.
            //
            // `Off::At` is still a live button rather than a disabled one, because whether its target is
            // in this build's listing is a question only the bus can answer, and the answer is the
            // refusal the gesture prints.
            match channel.off {
                crate::effects::Off::No(why) => {
                    ui.add_enabled(
                        false,
                        egui::Button::new(format!("turn {} off", channel.title)),
                    )
                    .on_disabled_hover_text(why);
                }
                crate::effects::Off::At { .. } => {
                    if ui
                        .button(format!("turn {} off", channel.title))
                        .on_hover_text("pauses the machine, writes the off target, and resumes it")
                        .clicked()
                    {
                        self.effects.off(self.machine, self.bus);
                    }
                }
            }
        });
        match self.effects.live_line() {
            Some(Ok(line)) => {
                ui.label(
                    egui::RichText::new(line)
                        .monospace()
                        .text_style(egui::TextStyle::Small)
                        .color(ui.visuals().strong_text_color()),
                );
            }
            // P4: a source that could not be read renders the refusal, never a blank.
            Some(Err(why)) => {
                ui.label(
                    egui::RichText::new(why)
                        .text_style(egui::TextStyle::Small)
                        .color(ui.visuals().error_fg_color),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new(
                        "nothing read back yet. This panel does not read on its own, because a stale \
                         line beside a fresh selection is the picture it exists to prevent.",
                    )
                    .text_style(egui::TextStyle::Small)
                    .color(weak),
                );
            }
        }
        ui.separator();

        // The two boxes. The PREFIX is what the bounded search is given; the FILTER only narrows what
        // came back. They are different questions and a person who conflates them gets a short list with
        // nothing saying so, which is why the count and the truncation line say which is which.
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("prefix")
                    .text_style(egui::TextStyle::Small)
                    .color(weak),
            );
            ui.add(
                egui::TextEdit::singleline(self.effects.prefix_mut())
                    .desired_width(160.0)
                    .hint_text("symbol prefix"),
            )
            .on_hover_text(
                "what the bus's bounded symbol search is asked for. The default is where this build \
                 publishes them today, not a claim that they can only be there.",
            );
            if ui.button("search").clicked() {
                self.effects.search(self.machine, self.bus);
            }
        });

        match self.effects.listing() {
            None => {
                ui.label(
                    egui::RichText::new(
                        "no search has been run for this channel yet. Press search to read the names \
                         this build publishes under the prefix above.",
                    )
                    .text_style(egui::TextStyle::Small)
                    .color(weak),
                );
            }
            Some(listing) => {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("filter")
                            .text_style(egui::TextStyle::Small)
                            .color(weak),
                    );
                    ui.add(
                        egui::TextEdit::singleline(self.effects.filter_mut())
                            .desired_width(140.0)
                            .hint_text("type to filter"),
                    )
                    .on_hover_text(
                        "narrows the list below. It never changes what is in effect: the line at the \
                         top always names that.",
                    );
                    ui.label(
                        egui::RichText::new(&listing.count)
                            .text_style(egui::TextStyle::Small)
                            .color(weak),
                    );
                });
                // ⚑ A partial measurement, said out loud. A list drawn from the first 256 of 400 names
                // with nothing saying so is this window deciding, on the reader's behalf, that the rest
                // do not exist.
                if let Some(t) = &listing.truncation {
                    ui.label(
                        egui::RichText::new(t)
                            .text_style(egui::TextStyle::Small)
                            .color(crate::theme::WARNING),
                    );
                }
                match &listing.absence {
                    // P6: an absent result is a stated line, never an empty box.
                    Some(a) => {
                        ui.label(
                            egui::RichText::new(a)
                                .text_style(egui::TextStyle::Small)
                                .color(weak),
                        );
                    }
                    None => {
                        if let Some(name) =
                            effects_list(ui, channel.title, &listing.rows, "effects_picker")
                        {
                            self.effects.select(self.machine, self.bus, &name);
                        }
                    }
                }
            }
        }

        // What the last gesture did, cell by cell. The cells are drawn rather than summarised because
        // the card promised one write and the engine wanted more, and a reader watching four go past is
        // owed the reason for each without leaving the window.
        if let Some(last) = self.effects.last() {
            ui.separator();
            let colour = if last.refused {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().strong_text_color()
            };
            ui.label(
                egui::RichText::new(&last.head)
                    .text_style(egui::TextStyle::Small)
                    .color(colour),
            );
            for cell in &last.cells {
                ui.label(
                    egui::RichText::new(cell)
                        .text_style(egui::TextStyle::Small)
                        .color(weak),
                );
            }
        }

        // The band readback, on the one channel that has a documented record layout to read.
        if channel.key == crate::effects::BANDS.key {
            ui.separator();
            if ui
                .button("read the bands")
                .on_hover_text("decodes the table this channel points at, one line per band")
                .clicked()
            {
                self.effects.refresh_bands(self.machine, self.bus);
            }
            match self.effects.band_read() {
                Some(Ok(b)) => {
                    if let Some(c) = &b.caveat {
                        ui.label(
                            egui::RichText::new(c)
                                .text_style(egui::TextStyle::Small)
                                .color(crate::theme::WARNING),
                        );
                    }
                    match &b.absence {
                        Some(a) => {
                            ui.label(
                                egui::RichText::new(a)
                                    .text_style(egui::TextStyle::Small)
                                    .color(weak),
                            );
                        }
                        None => {
                            for (i, band) in b.bands.iter().enumerate() {
                                ui.label(
                                    egui::RichText::new(format!("band {i}: {}", band.line()))
                                        .monospace()
                                        .text_style(egui::TextStyle::Small),
                                );
                            }
                        }
                    }
                }
                Some(Err(why)) => {
                    ui.label(
                        egui::RichText::new(why)
                            .text_style(egui::TextStyle::Small)
                            .color(ui.visuals().error_fg_color),
                    );
                }
                None => {}
            }
        }

        ui.separator();
        // ⚑ THE DISABLED NUDGE CONTROL. It is drawn OFF with the reason rather than omitted, because the
        // owner asked for nudges by name and an absent control reads as "we forgot". See
        // `crate::effects::NUDGE_BLOCKED` for the argument and for why it will be two numbers rather
        // than four when the engine hook lands.
        ui.add_enabled_ui(false, |ui| {
            ui.horizontal(|ui| {
                ui.label("nudge");
                ui.add(egui::DragValue::new(&mut 0i32).prefix("driver "));
                ui.add(egui::DragValue::new(&mut 0i32).prefix("rate shift "));
            });
        });
        ui.label(
            egui::RichText::new(crate::effects::NUDGE_BLOCKED)
                .text_style(egui::TextStyle::Small)
                .color(weak),
        );

        // The write-set, last, under a heading a person can skip. Read once and then known, but never
        // hidden: a panel that wrote four cells while showing one would be the thing this module's
        // header is about.
        ui.separator();
        ui.collapsing(format!("what a {} selection writes", channel.title), |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} cells, which is {}. Written together on a paused machine, so the next frame \
                     sees all of them or none.",
                    channel.writes.len(),
                    channel.installer
                ))
                .text_style(egui::TextStyle::Small)
                .color(weak),
            );
            for cell in channel.writes {
                ui.label(
                    egui::RichText::new(format!("{}: {}", cell.symbol, cell.why))
                        .text_style(egui::TextStyle::Small)
                        .color(weak),
                );
            }
        });
    }

    /// **The ring section of the Spawn tab**: a toggle, what a click does, and the one rule a person
    /// must not be left to discover by watching it happen.
    ///
    /// # ⚑ Why this is a section and not a row in the list below
    ///
    /// The archetype list draws what the loaded listing publishes under `ObjDef_`, and **there is no
    /// `ObjDef_Ring` in any build of this engine**. A ring is not an object here: it takes no pool slot,
    /// has no definition record, and never reaches the mailbox `emulator/object_spawn` writes. Putting an
    /// invented row in that list would be this window asserting a name the game does not have.
    ///
    /// # The statement is a condition of the feature
    ///
    /// A placed ring is swept out of the buffer once the camera moves away from it and nothing brings it
    /// back. Drawn for as long as the mode is armed, on the badge's own rule: a toast expires, and the
    /// fact that everything you place here is temporary does not. Coloured as a warning rather than as
    /// weak text, because it is the thing that will otherwise be read as a fault in the window.
    ///
    /// It sits at the **top** of the tab because it changes what a click does, and a person who armed it
    /// and then scrolled to the archetype list would otherwise be reading a list of things a click no
    /// longer places.
    fn rings(&mut self, ui: &mut egui::Ui) {
        let l = self.screen.ring_listing();
        ui.horizontal(|ui| {
            if l.armed {
                if ui
                    .button("rings: off")
                    .on_hover_text("a click on the picture goes back to arming a watch")
                    .clicked()
                {
                    self.screen.disarm_rings();
                }
            } else if ui
                .button("place rings…")
                .on_hover_text(
                    "a click on the Screen tab's picture puts a ring there. Rings are not objects in \
                     this engine and are not in the list below.",
                )
                .clicked()
            {
                self.screen.arm_rings();
            }
            ui.label(
                egui::RichText::new(&l.armed_line)
                    .text_style(egui::TextStyle::Small)
                    .color(if l.armed {
                        ui.visuals().strong_text_color()
                    } else {
                        ui.visuals().weak_text_color()
                    }),
            );
        });
        if let Some(t) = &l.temporary {
            ui.label(
                egui::RichText::new(t)
                    .text_style(egui::TextStyle::Small)
                    .color(crate::theme::WARNING),
            );
        }
    }

    /// **The subtype section of the Spawn tab**: which form of the selected archetype a click places.
    ///
    /// The owner's ask, in his words: *"remember I have to be able to choose subtypes for spawn"*. Springs
    /// come in strengths and directions and he wants to pick one before placing it.
    ///
    /// It sits **above** the archetype list rather than below it because the archetype list is the pane's
    /// long content and takes whatever height is left; a short, bounded list under an unbounded one is a
    /// list nobody scrolls to. Four things that are not rows are drawn before them, each a different
    /// finding: the standing statement of what is armed, a namespace clash, a cut-short search, and the
    /// stated absence that stands in for rows when there are none (P6).
    fn subtypes(&mut self, ui: &mut egui::Ui) {
        let weak = ui.visuals().weak_text_color();
        let listing = match self.screen.subtype_listing() {
            Ok(l) => l,
            // P4: a source that could not be read renders the refusal, never zero rows.
            Err(why) => {
                ui.label(
                    egui::RichText::new(why)
                        .text_style(egui::TextStyle::Small)
                        .color(ui.visuals().error_fg_color),
                );
                return;
            }
        };
        // ⚑ The standing statement of what a click carries, in words rather than as a highlighted row.
        // A reader who has scrolled this list, or the archetype list under it, must never be left
        // inferring the armed form from a fill colour: this is the badge's own rule one level down.
        ui.label(
            egui::RichText::new(&listing.armed)
                .text_style(egui::TextStyle::Small)
                .color(ui.visuals().strong_text_color()),
        );
        // Two namespaces that overlap. Loud, because the rows below may not all belong to this object and
        // nothing else on the glass would say so.
        if let Some(c) = &listing.collision {
            ui.label(
                egui::RichText::new(c)
                    .text_style(egui::TextStyle::Small)
                    .color(crate::theme::WARNING),
            );
        }
        // ⚑ **The one absence with no symptom.** A cut-short subtype list does not look cut short: it
        // looks like an object with fewer subtypes, and he would pick from three when there are twelve
        // with nothing wrong on screen. So it is said, in the words the model composed for it.
        if let Some(t) = &listing.truncation {
            ui.label(
                egui::RichText::new(t)
                    .text_style(egui::TextStyle::Small)
                    .color(crate::theme::WARNING),
            );
        }
        match &listing.absence {
            Some(a) => {
                ui.label(
                    egui::RichText::new(a)
                        .text_style(egui::TextStyle::Small)
                        .color(weak),
                );
            }
            None => {
                if let Some(name) = subtype_list(ui, &listing, "spawn_subtypes") {
                    self.screen
                        .select_subtype(self.machine, self.bus, name.as_str());
                }
            }
        }
    }

    /// **The Planes tab.** Controls, then the facts and the scroll reading beside the picture.
    ///
    /// Everything about *what the picture is* belongs to [`crate::planes`], including the decision not to
    /// rasterise. This function chooses where things go and nothing else, on [`Panels::screen`]'s split.
    ///
    /// The scroll reading sits **beside the picture rather than under it**, because the whole reason the
    /// panel is not just an image is that the image can be a confident wrong answer: the sentence saying a
    /// single register read could not establish the scroll has to be in the same glance as the thing it is
    /// about.
    fn planes(&mut self, ui: &mut egui::Ui) {
        let ink = crate::planes::Ink::of(crate::theme::DEFAULT_FAMILY);
        let is_window = self.planes.plane == oracle_core::render::Plane::Window;
        ui.horizontal_wrapped(|ui| {
            for (p, label) in crate::planes::CHOICES {
                if ui
                    .selectable_label(self.planes.plane == p, label)
                    .on_hover_text("draw this plane whole, from its nametable")
                    .clicked()
                {
                    // Through `select`, never by assignment: it is what drops a standing cell reading
                    // about the plane you just left. See `planes::Panel::select`.
                    self.planes.select(p);
                }
            }
            ui.separator();
            ui.checkbox(&mut self.planes.outline, "viewport")
                .on_hover_text("outline the part of this plane the screen is showing");
            // Offered and **disabled** rather than hidden, on the transport bar's rule: a control that
            // vanishes teaches nothing, and the hover says why this one is off.
            ui.add_enabled(
                !is_window,
                egui::Checkbox::new(&mut self.planes.apply_scroll, "apply live scroll"),
            )
            .on_hover_text(if is_window {
                "the window plane does not scroll, so there is nothing to apply"
            } else {
                "draw the region the scroll cuts out of this plane instead of the plane itself"
            });
        });
        ui.separator();

        // ⚑ Disjoint field borrows: the panel's own state mutably, the machine shared. The VDP read is
        // in-process and read-only (`screen_pick` reads it the same way for a click).
        let inp = self
            .planes
            .refresh(ui.ctx(), self.machine.system().vdp(), ink);
        let note = crate::planes::scroll_note(&inp);
        let (rasters, repaints) = self.planes.work();
        let (pw, ph) = inp.pixels();
        let (rw, rh) = inp.raster_size();
        let facts = vec![
            objects::Fact {
                label: "nametable at".into(),
                value: format!("${:04X}", inp.base),
                mono: true,
            },
            objects::Fact {
                label: "map".into(),
                value: format!("{} by {} cells", inp.cols, inp.rows),
                mono: false,
            },
            objects::Fact {
                label: "plane".into(),
                value: format!("{pw} by {ph} pixels"),
                mono: false,
            },
            objects::Fact {
                label: "drawn".into(),
                value: format!("{rw} by {rh} pixels"),
                mono: false,
            },
            objects::Fact {
                label: "scroll table at".into(),
                value: if is_window {
                    "not used by this plane".into()
                } else {
                    format!("${:04X}", inp.htable)
                },
                mono: !is_window,
            },
            objects::Fact {
                label: "rasterised".into(),
                value: format!("{rasters} times in {repaints} repaints"),
                mono: false,
            },
        ];

        // Cloned rather than borrowed because `side` is a closure and `plane_picture` below wants the
        // panel mutably. Four `String`s on the repaints where a reading is standing, against the six
        // `format!`s the facts above already cost every repaint: the same order of allocation this body
        // has always done, and none of it is the raster the fingerprint exists to skip.
        let reading = self.planes.reading().cloned();
        let side = |ui: &mut egui::Ui| {
            plane_side_column(ui, reading.as_ref(), &facts, &note, inp.scrolled)
        };
        plane_split(ui, side, |ui| self.plane_picture(ui, &inp));
    }

    /// The plane texture, fitted to whatever room is left, pixel grid preserved.
    ///
    /// [`Aspect::Square`] rather than the Screen tab's TV default, and that is not a taste call: this
    /// picture is a **map**, and stretching it to a 4:3 raster would put a cell's width and its height in
    /// different units on a view whose whole job is counting cells.
    ///
    /// `inp` is the gather this frame's texture was rasterised from, so a click is answered from the same
    /// facts the picture is drawn from rather than from a second read taken at click time.
    fn plane_picture(&mut self, ui: &mut egui::Ui, inp: &crate::planes::Inputs) {
        let Some(tex) = self.planes.texture() else {
            ui.centered_and_justified(|ui| ui.label("nothing rasterised yet"));
            return;
        };
        let src = tex.size_vec2();
        let avail = ui.available_size();
        let ppp = ui.pixels_per_point();
        let size = screen_pick::fit(
            avail,
            src.x as usize,
            src.y as usize,
            ppp,
            oracle_frontend::present::Aspect::Square,
        );
        // ⚑ **Never a silent return.** There is a texture; there is simply nowhere to put it. A panel that
        // draws nothing and says nothing is indistinguishable from a broken one, and that is exactly how
        // the `allocate_ui` squeeze fixed in `plane_split` shipped: planes A and B were blank, with no
        // message anywhere saying why. It is a layout condition rather than an error, so it is said in the
        // same quiet voice as the "nothing rasterised yet" branch above.
        if size.x <= 0.0 || size.y <= 0.0 {
            ui.centered_and_justified(|ui| ui.label(NO_ROOM_FOR_PICTURE));
            return;
        }
        let hit = egui::ScrollArea::both()
            .id_salt("planes_picture")
            .show(ui, |ui| {
                // The texture already carries `TextureOptions::NEAREST` from `Panel::refresh`, and a
                // second place saying so is a second spelling of one fact.
                ui.add(egui::Image::new((tex.id(), size)).sense(egui::Sense::click()))
                    .on_hover_text("click a cell to read its nametable entry")
            })
            .inner;

        // ⚑ The inverse is `screen_pick::dot_at`, unchanged and shared: it inverts **the rectangle egui
        // actually laid the image out in** (`Response::rect`) rather than re-deriving the fit, which is
        // what let it survive a change of fit on the Screen tab with no edit at all. It also takes `ppp`
        // explicitly, and that is the trap this seat cannot see: the owner's display is not at 1.0, so a
        // forgotten points-to-pixels conversion produces a picking offset that every 1.0 harness calls
        // correct.
        //
        // A picture inside a `ScrollArea` is where a second derivation would go wrong first, since the
        // rect moves under the scroll offset while any recomputed fit would not.
        if let (true, Some(pos)) = (hit.clicked(), hit.interact_pointer_pos()) {
            if let Some((rx, ry)) =
                screen_pick::dot_at(hit.rect, pos, ppp, src.x as usize, src.y as usize)
            {
                self.planes.click(inp, rx as usize, ry as usize);
            }
        }
    }

    /// **The audit page's exemplar.** See `docs/2026-09-05-debug-window-audit.md`.
    ///
    /// The facts are projected by [`pacing::Readout::of`], which holds no egui type and is therefore
    /// testable without a window. This function is only the drawing, and the split is the point: the
    /// window cannot be opened from an agent seat, so a panel whose correctness lives in its draw calls is
    /// a panel nothing can check.
    ///
    /// What it replaced was thirteen `ui.monospace(format!(..))` lines with their label columns spelled as
    /// literal spaces. See [`pacing::Readout`] for the three rules that broke and why P2's stated grep
    /// could not see the worst of them.
    fn pacing(&self, ui: &mut egui::Ui) {
        let device = self.machine.device().map(|d| {
            use ringbuf::traits::Observer;
            use std::sync::atomic::Ordering;
            pacing::DeviceFacts {
                rate_hz: d.rate(),
                channels: d.channels(),
                occupied: d.prod().occupied_len(),
                capacity: d.ring_capacity(),
                starved_steady: d.counters().starved_steady.load(Ordering::Relaxed),
                dropped: d.dropped(),
            }
        });
        // ⚑ `self.pacing` is the value `Loop::iterate` computed once this frame and handed to BOTH this
        // panel and `Host::set_pacing`. Nothing here recomputes a rate — see `pacing::Readout::of`.
        let r = pacing::Readout::of(
            self.machine.frames(),
            self.pacing,
            self.governor,
            device,
            self.status,
        );

        egui::ScrollArea::vertical()
            .id_salt("pacing")
            .show(ui, |ui| {
                // The three numbers the tab is opened to read, side by side and large. Emphasis is size
                // and colour, never weight: egui has no bold axis.
                //
                // ⚑ Drawn TWICE, on purpose and not for long. See the temporary block at
                // [`headline_comparison`]: the audit parked "bare number or bordered tile" as a look
                // call, and a look call is settled by looking. Both arms read the same `r.headline`,
                // which is the live projection, so neither is a mock.
                headline_comparison(ui, &r.headline);
                ui.add_space(SECTION_GAP);

                section(ui, "governor", None, "the loop's own rate limiter");
                card(ui, |ui| health_grid(ui, "pacing-governor", &r.governor));
                ui.add_space(SECTION_GAP);

                section(
                    ui,
                    "frame time",
                    None,
                    "wall clock per presented frame, as a distribution",
                );
                card(ui, |ui| health_grid(ui, "pacing-frame-time", &r.frame_time));
                ui.add_space(SECTION_GAP);

                section(ui, "audio", None, "the clock everything else follows");
                card(ui, |ui| match &r.audio {
                    // P4/P6: the absent case is a whole-section statement, not a table of zeroes. It is
                    // warn-coloured from the arm the projection chose, never from reading the sentence.
                    pacing::Audio::Absent { why } => {
                        ui.colored_label(ui.visuals().warn_fg_color, *why);
                    }
                    pacing::Audio::Open(a) => {
                        // The landed shape, deliberately not doubled: the comparison above is the
                        // question, and asking it twice on one panel would make the tab about the
                        // question rather than about pacing.
                        stat_row(ui, &a.stats, StatShape::Bare);
                        ui.add_space(SECTION_GAP);
                        health_grid(ui, "pacing-audio", &a.facts);
                        ui.add_space(SECTION_GAP);
                        meter(ui, &a.meter);
                    }
                });
                ui.add_space(SECTION_GAP);

                // The line the window publishes for `emulator/screen_text`, said to be that rather than
                // shown as a fourth opinion about numbers already above it. The Registers tab sets the
                // precedent: a panel that silently shows one number twice is a new wrong answer.
                ui.label(
                    egui::RichText::new(&r.status)
                        .text_style(egui::TextStyle::Small)
                        .color(ui.visuals().weak_text_color()),
                )
                .on_hover_text(
                    "The one-line summary this window publishes for `emulator/screen_text`, shown \
                     verbatim. Its frame and rebase counts are the same two numbers as above, not a \
                     second measurement of them.",
                );
            });
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
            "A7 and SP are one register: the stack pointer the CPU is using right now, SSP in \
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
                    // The hint is the panel's own default, not a second string beside it: this hint
                    // used to advertise `0xFFFF0000`, which was the value the box was pre-filled with
                    // AND the value the read path refused. One derivation now, so a hint cannot go on
                    // recommending an address the panel no longer opens on.
                    .hint_text(format!(
                        "{} or a symbol name",
                        memory::default_base_text()
                    )),
            );
            let go = ui.button("go").clicked()
                || (entry.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                let space = self.mem.space;
                let text = self.mem.addr_text.clone();
                let (bus, sys) = (&mut *self.bus, self.machine.system_mut());
                self.mem.addr_note = Some(match memory::resolve_address(bus, sys, space, &text) {
                    memory::Resolved::Hex {
                        addr,
                        listing: None,
                    } => {
                        self.mem.base = addr;
                        memory::Line::plain(format!(
                            "{}: a hex literal, taken as typed",
                            oracle_aether::hex::addr(addr)
                        ))
                    }
                    // ⚑ **Masked, and SAID.** The typed value carried bits above the 24 the 68000
                    // drives, so it was a listing spelling of a real bus address. The panel goes there
                    // and names both numbers rather than refusing the one a person read off the
                    // listing — and rather than moving silently, which is the thing the schema's
                    // `rawAddr` note is actually protecting against. This is the sentence
                    // `lookup_symbol` never wrote for the identical masking it has always done.
                    memory::Resolved::Hex {
                        addr,
                        listing: Some(raw),
                    } => {
                        self.mem.base = addr;
                        memory::Line::plain(format!(
                            "{}: the 68000 drives 24 address lines, so the listing spelling {} names \
                             this same location and this is where the page below is read from",
                            oracle_aether::hex::addr(addr),
                            oracle_aether::hex::addr(raw)
                        ))
                    }
                    memory::Resolved::Symbol { addr, reply } => {
                        self.mem.base = addr;
                        memory::Line::plain(format!("ok: {reply}"))
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
                        "showing {n} bytes from {}: the space ends before a full page",
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
                    if g.is_open() { "WRITE" } else { "  no " },
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

        // --- the derived layout, as a card of labelled facts ---
        //
        // Every value here is a key of the `layout` object the three served rows carry: the same answer,
        // laid out for a person instead of run together on one line. Composed in `objects::layout_facts`
        // rather than here so the strings are reachable from a test.
        card(ui, |ui| {
            fact_grid(ui, "objects-layout", &pool.layout_facts());
        });
        // ⚑ **Folded, not deleted, and this is the only prose on the tab that is.** The owner's fourth
        // finding is about reading the tables — *"look how difficult objects is to read"*, *"especially
        // if it's not fully open"* — and on a short pane four lines of derivation sat between the card
        // and the first table he came here for.
        //
        // It is a derivation a reader needs **once**, when they first doubt the addresses, and never
        // again in a session; that is exactly the shape a collapsing header is for, and it is why every
        // other line on this tab stays where it is. A refusal, a warning or a count is a fact about THIS
        // frame and none of them may be behind a click.
        egui::CollapsingHeader::new(
            egui::RichText::new("where these addresses come from")
                .text_style(egui::TextStyle::Small)
                .color(ui.visuals().weak_text_color()),
        )
        .id_salt("objects-layout-why")
        .show(ui, |ui| {
            ui.small(
                "Every address here is read out of the loaded listing: the base from \
                 Object_RAM/Player_1, the stride from Player_2 minus Player_1, the count from \
                 Object_RAM_End, and the ring buffer below from Ring_Count minus Ring_Buffer. \
                 Nothing is hardcoded, because an object-table address is a fact about one build.",
            );
        });

        // --- the player section: the same decoder, the same layout, its own refusal ---
        ui.add_space(SECTION_GAP);
        match &pool.players {
            Err(e) => {
                section(ui, "players", None, "emulator/player_state");
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
                section(
                    ui,
                    "players",
                    Some(format!(
                        "{} of {} slots",
                        pv.players.len(),
                        pv.layout.slot_count()
                    )),
                    "emulator/player_state",
                );
                slot_table(ui, &objects::PLAYER_COLS, &pv.players, None, "player-row");
            }
        }

        // --- the pool table ---
        ui.add_space(SECTION_GAP);
        section(
            ui,
            "object pool",
            Some(format!(
                "{} active of {} slots",
                pool.total, pool.slot_count
            )),
            "emulator/object_list",
        );

        // --- rings, immediately under the object count, because that is where the question is asked ---
        //
        // The two numbers sit together on purpose: "5 active of 66 slots" invites "so where are the
        // rings", and the answer is that they are not in that 66 at all.
        card(ui, |ui| match &pool.rings {
            Ok(r) => {
                fact_grid(ui, "objects-rings", &r.facts());
                ui.small(objects::RINGS_WHY);
                // The capacity is a fact of its own when the listing publishes the entry size (§11.36).
                // When it does not, the gap is STATED: an absent capacity with no sentence beside it is
                // an invitation to divide the span above by a remembered entry size.
                if r.ceiling.is_none() {
                    ui.colored_label(ui.visuals().warn_fg_color, objects::CEILING_UNKNOWN);
                }
            }
            // A ring line this listing cannot supply is one missing card, and it says which symbol did
            // not answer, never a `0`, which would read as "no rings are loaded".
            Err(e) => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("rings unavailable. {} {}", e.code, e.message),
                );
                ui.small(objects::RINGS_WHY);
            }
        });

        if pool.objects.is_empty() {
            // A stated fact, and a different one from the refusal above: the layout derived, the table
            // was read, and nothing is live in it right now.
            ui.label(
                "0 active objects. The layout derived and every slot's code word is the empty-slot \
                 sentinel; this is not a missing listing.",
            );
        }
        let clicked = egui::ScrollArea::vertical()
            .id_salt("object-pool")
            .max_height(240.0)
            .show(ui, |ui| {
                slot_table(
                    ui,
                    &objects::POOL_COLS,
                    &pool.objects,
                    Some(self.objects.selected),
                    "pool-row",
                )
            })
            .inner;
        if let Some(slot) = clicked {
            // A second click closes it: the expansion is one row's detail, and a row that cannot be
            // un-expanded is a mode with no way out.
            self.objects.selected = if self.objects.selected == Some(slot) {
                None
            } else {
                Some(slot)
            };
        }

        // --- the row expansion: one addressed slot, every field the layout declares ---
        let Some(slot) = self.objects.selected else {
            return;
        };
        ui.add_space(SECTION_GAP);
        match objects::object_slot(self.symbols, self.machine.system(), slot) {
            Err(e) => {
                section(ui, &format!("slot {slot}"), None, "emulator/object_slot");
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("REFUSED {} {}", e.code, e.message),
                );
            }
            Ok(v) => {
                // The address is spelled by the bus's own `hex::addr`, not by a second `{:X}` here: this
                // is the string `addr` carries on the wire, and two spellings of one address is how a
                // reader ends up comparing the panel to a tool and seeing a difference that is not one.
                section(
                    ui,
                    &format!("slot {slot}"),
                    Some(format!(
                        "of {} at {}",
                        v.layout.slot_count(),
                        oracle_aether::hex::addr(v.row.addr)
                    )),
                    "emulator/object_slot",
                );
                egui::ScrollArea::vertical()
                    .id_salt("object-slot")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        // The envelope's own keys, then the game's declared fields under their own
                        // heading. Two grids rather than one, because the second set is a fact about the
                        // loaded build and the first is a fact about the contract.
                        let envelope: Vec<objects::Fact> = v
                            .row
                            .item
                            .iter()
                            .filter(|(k, _)| k.as_str() != "fields")
                            .map(|(k, val)| objects::Fact {
                                label: k.clone(),
                                value: render(val),
                                mono: true,
                            })
                            .collect();
                        card(ui, |ui| fact_grid(ui, "object-slot-envelope", &envelope));
                        match v.row.item.get("fields").and_then(|f| f.as_object()) {
                            Some(f) => {
                                ui.add_space(SECTION_GAP);
                                ui.label(
                                    egui::RichText::new("declared fields")
                                        .color(ui.visuals().weak_text_color()),
                                );
                                let fields: Vec<objects::Fact> = f
                                    .iter()
                                    .map(|(k, val)| objects::Fact {
                                        label: k.clone(),
                                        value: render(val),
                                        mono: true,
                                    })
                                    .collect();
                                card(ui, |ui| fact_grid(ui, "object-slot-fields", &fields));
                            }
                            // Only reachable on an inactive slot, where the decoded keys are OMITTED
                            // rather than zeroed: bytes the game never wrote are not data.
                            None => {
                                ui.small(
                                    "no fields. This slot is empty, and an empty slot's record is \
                                     bytes the game never wrote, so they are omitted rather than \
                                     shown as zeroes.",
                                );
                            }
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

    /// **The Breakpoints tab.** The armed set, an add box, a per-row toggle, a per-row `remove` and a
    /// clear-all.
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
                "{} of {} breakpoint{} armed, so the machine will halt at {}",
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
                     resolves it: the reply carries the address it landed on. A second add at an \
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
                        "emulator/breakpoint_clear {all:true}: EVERY breakpoint on this server, \
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
            // The armed-for-removal handle is read out here and written back after the closure, so the
            // scroll area borrows neither `self` nor a field of it. `confirm` is the handle the human has
            // already pressed `remove` on; `next_confirm` is `Some(new value)` only when this frame moved
            // it, which keeps "nothing happened" distinguishable from "cleared".
            let confirm = self.stopping.confirm_remove.clone();
            let mut next_confirm: Option<Option<String>> = None;
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
                                     toggle: this surface never resets a count; a fresh one means \
                                     clear and re-add.",
                                )
                                .changed()
                            {
                                gesture = Some((
                                    stopping::BREAKPOINT_SET_ENABLED,
                                    stopping::breakpoint_enable_params(&r.handle, on),
                                ));
                            }
                            // ⚑ **Two presses, and a word.** This is the control that destroyed a
                            // breakpoint under a UX seat who thought it was the tick-box: it drew as a
                            // hollow box (see [`REMOVE_LABEL`]) 36 px from a real one, and it took effect
                            // on the first click with no tooltip, no confirmation and no undo. Both halves
                            // are fixed here, because a legible label on a control that still fires
                            // instantly only narrows the accident.
                            if stopping::confirming(confirm.as_deref(), &r.handle) {
                                if ui
                                    .small_button(
                                        egui::RichText::new(REMOVE_CONFIRM_LABEL)
                                            .color(ui.visuals().error_fg_color),
                                    )
                                    .on_hover_text(
                                        "emulator/breakpoint_clear on this handle alone. It does not \
                                         come back: re-arming the same address makes a NEW breakpoint, \
                                         with a new handle and `hits` at zero. To keep the row and stop \
                                         it halting the machine, untick it instead.",
                                    )
                                    .clicked()
                                {
                                    gesture = Some((
                                        stopping::BREAKPOINT_CLEAR,
                                        stopping::breakpoint_clear_params(&r.handle),
                                    ));
                                    next_confirm = Some(None);
                                }
                                if ui.small_button(REMOVE_CANCEL_LABEL).clicked() {
                                    next_confirm = Some(None);
                                }
                            } else if ui
                                .small_button(REMOVE_LABEL)
                                .on_hover_text(
                                    "Clear this breakpoint. Asks once more before it does, because it \
                                     cannot be undone. The tick-box to the left is the one that turns a \
                                     breakpoint OFF and keeps it.",
                                )
                                .clicked()
                            {
                                next_confirm = Some(Some(r.handle.clone()));
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
            if let Some(v) = next_confirm {
                self.stopping.confirm_remove = v;
            }
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
                     by the handler (a watch that can never match), and this panel lets it say so.",
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
             negative finding (the range was watched and nothing touched it), and it is only \
             distinguishable from a watch that never armed because both numbers are here.",
        );
        for c in &view.caveats {
            ui.colored_label(ui.visuals().warn_fg_color, c);
        }

        // --- the armed watches ---
        if !view.watches.is_empty() {
            ui.separator();
            // Read out and written back after the closure, exactly as in `breakpoints` above. The two tabs
            // share the field and cannot collide: the handles are the server's own `b…`/`w…` spellings.
            let confirm = self.stopping.confirm_remove.clone();
            let mut next_confirm: Option<Option<String>> = None;
            egui::ScrollArea::vertical()
                .id_salt("watch-rows")
                .max_height(160.0)
                .show(ui, |ui| {
                    for row in &view.watches {
                        let w = &row.report;
                        ui.horizontal(|ui| {
                            // ⚑ **The same word, the same two presses, as Breakpoints.** This control was
                            // the *first* thing in the row and there is no tick-box here at all, so the
                            // hollow box it used to draw read as this tab's on/off switch while it was in
                            // fact this tab's delete. The remedy for that is a word, not a font.
                            if stopping::confirming(confirm.as_deref(), &row.handle) {
                                if ui
                                    .small_button(
                                        egui::RichText::new(REMOVE_CONFIRM_LABEL)
                                            .color(ui.visuals().error_fg_color),
                                    )
                                    .on_hover_text(
                                        "emulator/watchpoint_clear. The watch goes and does not come \
                                         back; its recorded HITS stay, deliberately, because a \
                                         destructive clear would let one client erase another's \
                                         evidence. The headline above changes to STOPPED.",
                                    )
                                    .clicked()
                                {
                                    gesture = Some((
                                        stopping::WATCHPOINT_CLEAR,
                                        stopping::watch_clear_params(&row.handle),
                                    ));
                                    next_confirm = Some(None);
                                }
                                if ui.small_button(REMOVE_CANCEL_LABEL).clicked() {
                                    next_confirm = Some(None);
                                }
                            } else if ui
                                .small_button(REMOVE_LABEL)
                                .on_hover_text(
                                    "Clear this watch. Asks once more before it does, because it cannot \
                                     be undone. This tab has no off switch: a watch is armed or it is \
                                     gone, which is why the word here is the same one Breakpoints uses \
                                     for its destructive control and not for its tick-box.",
                                )
                                .clicked()
                            {
                                next_confirm = Some(Some(row.handle.clone()));
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
            if let Some(v) = next_confirm {
                self.stopping.confirm_remove = v;
            }
        }

        // --- the hit log ---
        if !view.hits.is_empty() {
            ui.separator();
            ui.strong(format!(
                "hit log: {} retained{}",
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
                 is not `no hot code`. It is `nothing was measured`. Arm it below."
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
                    "emulator/set_profiler. ⚑ ARMING RESETS THE SAMPLE: every arming flag resets \
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
                "hottest routines: top {} of {}",
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
                     than clamping, so a client can always tell a full list from a clipped one.",
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

// -------------------------------------------------------------------------------------------------------
// ⚑ Panel furniture: cards, labelled facts, section heads, and a real table
//
// The style page's P2 says rows of like-shaped data go in a table with fixed columns, and names the check:
// **no width-padded format specifier** doing a table's job inside a `ui.monospace(format!(..))`. These four
// helpers are what makes that possible without one.
//
// # Why the table is drawn by hand
//
// Neither ready-made option fits, and the choice was made rather than defaulted into:
//
// * `egui::Grid` sizes columns beautifully and is used below for every *labelled-fact* block. But it hands
//   back no row rectangle, so a whole row cannot be made clickable, striped or selected as one band, and
//   the pool table's central gesture is clicking a row.
// * `egui_extras::TableBuilder` does exactly that, and it is **not a dependency of this workspace** and
//   not in the vendored registry either, so adding it means a network fetch and a new crate for six
//   columns. Measuring the widths here with `Painter::layout_no_wrap` over the rows that are actually on
//   screen is about twenty lines, is exact for whatever face the theme installs, and adds nothing.
//
// The measurement matters more than it looks. Counting characters and multiplying by a remembered advance
// would be a fact about one font file, and this crate does not ship the font file: the style page's §2
// item 7 leaves the faces as egui's stock pair until the owner rules on vendoring Inter and JetBrains
// Mono, so the advance is going to change under this code.
// -------------------------------------------------------------------------------------------------------

/// The space between a panel's sections. `space.3` (6px).
const SECTION_GAP: f32 = 6.0;

/// A card's inner padding. `space.4` (8px).
const CARD_PAD: i8 = 8;

/// The gap between two table columns. `space.5` (12px) plus a hair, so two columns of right-aligned digits
/// never touch.
const COL_GUTTER: f32 = 14.0;

/// The narrowest the last (name) column is allowed to get on a cramped panel. Below this a name stops
/// being a name and the hover is the only place it exists.
const NAME_COL_FLOOR: f32 = 120.0;

/// The gap between the two columns of a selectable list, and the margin its band carries past the
/// glyphs.
///
/// ⚑ Owed to the owner's 2026-09-09 reading of a subtype row as `$20Down_Red`: the value and the name
/// were two labels in a `horizontal` whose `item_spacing.x` was **zero**, so a hex value and a symbol
/// name were drawn touching. Smaller than [`COL_GUTTER`] because these are two columns rather than six
/// and the list is narrow, but never zero, which is the whole point.
const LIST_GUTTER: f32 = 10.0;

/// The narrowest a [`StatShape::Tile`]'s **content** may be, so a row of tiles is a row of like-sized
/// boxes rather than three boxes shrink-wrapped to three different labels.
///
/// Chosen from the widest headline label the Pacing tab actually has (`frames emulated`, at the 10px
/// `Small` face) rather than from taste, so the floor is the thing that is already setting the width.
const TILE_MIN_W: f32 = 78.0;

/// A bordered block on the `raised` surface, one step above the panel it sits on.
///
/// The surface ladder is how depth is carried here: CHROME_SPEC forbids drop shadows on controls, and
/// `theme.rs` sets `window_shadow`/`popup_shadow` to `Shadow::NONE` accordingly.
fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(CARD_PAD))
        .show(ui, add)
        .inner
}

/// What the Planes tab says when it has a picture and nowhere to put it. See [`Panels::plane_picture`].
const NO_ROOM_FOR_PICTURE: &str = "no room to draw the plane here; widen the pane";

/// The width the Planes tab's side column is given when it sits beside the picture.
const PLANE_SIDE_W: f32 = 260.0;

/// Below this, a pane cannot carry a column of facts and a plane side by side, so the facts go under
/// the picture instead of squeezing it.
const PLANE_SIDE_BY_SIDE_MIN: f32 = 560.0;

/// **The Planes tab's two-column split**, extracted from [`Panels::planes`] so a headless test can drive
/// the real arrangement without a `Machine`.
///
/// ⚑ The side column is allocated **with its own top-down layout**, not with `allocate_ui`. `allocate_ui`
/// inherits the caller's layout, and the caller here is `horizontal_top`; a child that inherits a
/// horizontal layout lays its loose labels out left-to-right, and — because `Ui::wrap_mode` only returns
/// `Wrap` for a vertical layout or a wrapping horizontal one — it also **does not wrap text**. The side
/// column's `unestablished` caveat is a paragraph, so under `allocate_ui` it ran off to the right, ate the
/// row, and left the picture no width at all. That was a shipped defect: planes A and B showed no picture,
/// while `window` (whose note is two short lines and no caveat) looked fine.
fn plane_split(
    ui: &mut egui::Ui,
    side: impl FnOnce(&mut egui::Ui),
    picture: impl FnOnce(&mut egui::Ui),
) {
    if ui.available_width() < PLANE_SIDE_BY_SIDE_MIN {
        picture(ui);
        ui.add_space(SECTION_GAP);
        side(ui);
        return;
    }
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(PLANE_SIDE_W, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            side,
        );
        ui.add_space(SECTION_GAP);
        picture(ui);
    });
}

/// The Planes tab's facts-and-reading column: the last click's answer, then the standing facts and the
/// scroll note. Free rather than a closure inside [`Panels::planes`] so the test that measures what it
/// leaves for the picture draws **this** column and not a stand-in for it.
fn plane_side_column(
    ui: &mut egui::Ui,
    reading: Option<&crate::planes::CellReading>,
    facts: &[objects::Fact],
    note: &crate::planes::ScrollNote,
    scrolled: bool,
) {
    // **The last click's answer first**, above the standing facts, in its own card. The facts are
    // context that does not move; this is what the person just asked for, and a standing readout
    // drawn as loose text under a column of facts reads as one more fact's label. Same card
    // treatment as the Screen tab's pick readout, for the same reason it has one.
    if let Some(r) = reading {
        card(ui, |ui| {
            // The four parts at four weights, handed over by `planes::CellReading` already
            // separated. Nothing here recovers structure by looking at the text and nothing here
            // decides a colour by looking at it.
            ui.label(egui::RichText::new(&r.head).color(ui.visuals().strong_text_color()));
            ui.label(
                egui::RichText::new(&r.screen)
                    .text_style(egui::TextStyle::Small)
                    .color(ui.visuals().weak_text_color()),
            );
            // The addressing recedes and goes monospace: it is what a reader checks the sentence
            // against, in the spelling they would compare with a tool's reply.
            ui.label(
                egui::RichText::new(&r.detail)
                    .monospace()
                    .color(ui.visuals().weak_text_color()),
            );
            // The loud lines. Coloured on the field, never on the shape of the string, and never
            // omitted when present.
            for said in &r.unestablished {
                ui.add_space(SECTION_GAP);
                ui.label(
                    egui::RichText::new(said)
                        .text_style(egui::TextStyle::Small)
                        .color(crate::theme::WARNING),
                );
            }
        });
        ui.add_space(SECTION_GAP);
    }
    card(ui, |ui| {
        fact_grid(ui, "planes_facts", facts);
        ui.add_space(SECTION_GAP);
        let weak = ui.visuals().weak_text_color();
        for line in [&note.horizontal, &note.vertical] {
            ui.label(
                egui::RichText::new(line)
                    .text_style(egui::TextStyle::Small)
                    .color(weak),
            );
        }
        // ⚑ The loud line. Never omitted when set, and coloured on the field rather than on the
        // shape of the string (style page P5).
        if let Some(said) = &note.unestablished {
            ui.add_space(SECTION_GAP);
            ui.label(
                egui::RichText::new(said)
                    .text_style(egui::TextStyle::Small)
                    .color(crate::theme::WARNING),
            );
        }
        if scrolled {
            ui.add_space(SECTION_GAP);
            ui.label(
                egui::RichText::new(
                    "The scroll is applied, so this is the region the screen shows of this \
                     plane alone, with no other plane and no sprites over it.",
                )
                .text_style(egui::TextStyle::Small)
                .color(weak),
            );
        }
    });
}

/// Labelled facts in two aligned columns: the label small and recessed, the value emphasised.
///
/// Emphasis is **colour and size, never weight** -- egui selects fonts by family and has no bold axis, so
/// `RichText::strong()` only swaps in `Visuals::strong_text_color()`, which `theme.rs` sets to the
/// family's `text.hi`. That is the whole of what "typographically strong" can mean here, and saying it out
/// loud is cheaper than someone discovering it by trying to embolden a header.
fn fact_grid(ui: &mut egui::Ui, id: &str, facts: &[objects::Fact]) {
    let (weak, strong) = (
        ui.visuals().weak_text_color(),
        ui.visuals().strong_text_color(),
    );
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([COL_GUTTER, 3.0])
        .show(ui, |ui| {
            for f in facts {
                ui.label(
                    egui::RichText::new(&f.label)
                        .text_style(egui::TextStyle::Small)
                        .color(weak),
                );
                let v = egui::RichText::new(&f.value).color(strong);
                ui.label(if f.mono { v.monospace() } else { v });
                ui.end_row();
            }
        });
}

/// The colour a [`pacing::Health`] takes. **One function**, so the meaning of "watch" cannot be one
/// colour on a stat and a different one on the fact beside it.
///
/// `Unmeasured` is `weak_text_color` rather than a semantic colour on purpose: nothing measured it, so it
/// is neither good news nor bad news and must not be dressed as either.
fn health_colour(ui: &egui::Ui, h: pacing::Health) -> egui::Color32 {
    match h {
        pacing::Health::Good => ui.visuals().strong_text_color(),
        pacing::Health::Watch => crate::theme::WARNING,
        pacing::Health::Unmeasured => ui.visuals().weak_text_color(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════════════
// ⚑ TEMPORARY: THE HEADLINE-SHAPE CHOICE, ON SCREEN BECAUSE IT CANNOT BE SETTLED ANYWHERE ELSE
//
// `docs/2026-09-05-debug-window-audit.md` §6 parked look call 4 asks: *should a headline number be bare
// text at the 20px section face, or a small bordered tile?* Both satisfy the owner's brief, neither can
// be settled from source or from an argument, and this window cannot be opened from an agent seat. So
// the answer is a screen he can point at: **the same three live numbers, drawn both ways, one above the
// other, in the real panel among its real neighbours** rather than in a scratch tab where the reading
// would prove nothing.
//
// ⚑ **HOW THE LOSER IS DELETED — one arm, not a rewrite.** In every case the choice collapses to
// `stat_row`'s only shape, and the whole of the temporary surface is this block plus its call site:
//
//   * If **bare** wins: delete [`StatShape`] and [`headline_comparison`], delete `stat`'s `Tile` arm and
//     the `TILE_MIN_W` const, drop the `shape` parameter from `stat`/`stat_row` and their two call
//     sites, and restore `Panels::pacing`'s one line to `card(ui, |ui| stat_row(ui, &r.headline))`.
//   * If **tile** wins: the same deletions with the arms swapped, `stat`'s body becoming the tile frame
//     around [`stat_body`], and `Panels::pacing`'s line becoming a bare `stat_row(ui, &r.headline)` —
//     the card goes, because tiles do their own containment and a card around them is two borders doing
//     one job.
//
// Nothing else in the crate is touched by either outcome: [`stat_body`] is the content in both, so the
// number, its unit, its label, its colour and its hover are byte-identical between the two treatments
// and the only variable a reader is judging is the containment.
// ═══════════════════════════════════════════════════════════════════════════════════════════════════════

/// Which of the two candidate shapes a [`stat`] is drawn in. **Temporary**; see the block above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatShape {
    /// **Treatment A, and what the tab ships today.** Bare text on whatever surface it lands on: no
    /// fill of its own, no border of its own, nothing but the number, its unit and its label.
    Bare,
    /// **Treatment B.** The same content inside a small bordered box on the `raised` step.
    Tile,
}

/// The two treatments, one above the other, each labelled with which it is.
///
/// **`stats` is the live headline projection, passed to both arms.** That is the load-bearing property
/// and it is what [`stat_shape_tests`] guards: a comparison in which one treatment showed placeholder
/// numbers would look exactly like a working one and be worth nothing.
fn headline_comparison(ui: &mut egui::Ui, stats: &[pacing::Stat]) {
    ui.label(
        egui::RichText::new(HEADLINE_CHOICE)
            .text_style(egui::TextStyle::Small)
            .color(ui.visuals().warn_fg_color),
    )
    .on_hover_text(
        "Both shapes are drawn from the same three live counters, so the only thing being judged is \
         the box around them. Whichever one you keep becomes the shape every other panel's headline \
         numbers are built in, and the other is deleted.",
    );
    ui.add_space(SECTION_GAP);

    treatment_caption(
        ui,
        TREATMENT_A,
        "bare numbers sharing one card, which is what this tab shows today",
    );
    card(ui, |ui| stat_row(ui, stats, StatShape::Bare));
    ui.add_space(SECTION_GAP * 2.0);

    treatment_caption(
        ui,
        TREATMENT_B,
        "one bordered tile per number, and no shared card, because the tiles are the containment",
    );
    stat_row(ui, stats, StatShape::Tile);
}

/// Which treatment the block below the line is, said in two weights on one line the way [`section`] does.
fn treatment_caption(ui: &mut egui::Ui, name: &str, note: &str) {
    let (weak, strong) = (
        ui.visuals().weak_text_color(),
        ui.visuals().strong_text_color(),
    );
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            egui::RichText::new(name)
                .text_style(egui::TextStyle::Small)
                .color(strong),
        );
        ui.label(
            egui::RichText::new(note)
                .text_style(egui::TextStyle::Small)
                .color(weak),
        );
    });
}

/// The standing line above the two treatments. **Temporary**, and it says so, because a block a reader
/// cannot tell is a question is a block he will read as a design.
const HEADLINE_CHOICE: &str =
    "One choice, and this block is temporary: the same three numbers are drawn \
                               twice below. Say which reads better and the other one goes away.";

/// The two treatment names, as one constant each so the gate can look for the strings the panel actually
/// draws rather than for a second copy of them.
const TREATMENT_A: &str = "treatment A";
const TREATMENT_B: &str = "treatment B";

/// **The big-number readout: the "pops" in "clean and readable and pops".**
///
/// One measured number at the `section` face (20px), its unit small beside it, and its name small and
/// recessed beneath. That vertical order is the whole trick: a reader scanning a row of these reads the
/// numbers first and only drops to a label for the one that surprised them, which is the opposite of a
/// `label   value` line where the label is read first every time.
///
/// Emphasis is size and colour and nothing else. egui selects fonts by family and has no weight axis, so
/// there is no bold to reach for here even if one were wanted.
///
/// ⚑ **Two shapes are drawn side by side right now, and one of them is going away.** See [`StatShape`]
/// and [`headline_comparison`]. This function is the content of both; the shape only decides what is
/// drawn around it.
fn stat(ui: &mut egui::Ui, s: &pacing::Stat, shape: StatShape) {
    match shape {
        StatShape::Bare => stat_body(ui, s),
        StatShape::Tile => {
            egui::Frame::new()
                // The same surface step and the same border the `card` above it takes, at the scale of
                // one number instead of three. Nothing new is invented: `faint_bg_color` is the family's
                // `raised` and the stroke is `theme.rs`'s 1px `border`.
                .fill(ui.visuals().faint_bg_color)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::same(CARD_PAD))
                .show(ui, |ui| {
                    // Without a floor the three tiles are three different widths, which is the one thing
                    // a tile row must not be: the boxes would then read as a ragged list rather than as
                    // a set of readings.
                    ui.set_min_width(TILE_MIN_W);
                    stat_body(ui, s);
                });
        }
    }
}

/// The number, its unit and its label: the part that is **identical in both treatments**, so the choice
/// below is about containment and nothing else.
fn stat_body(ui: &mut egui::Ui, s: &pacing::Stat) {
    let big = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Name(crate::theme::SECTION.into()))
        .cloned()
        .unwrap_or_else(|| egui::FontId::proportional(20.0));
    let colour = health_colour(ui, s.health);
    let weak = ui.visuals().weak_text_color();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            // No item spacing between the number and its unit: "1.25 ms" is one reading, and the default
            // 4px gutter would make the unit look like a separate column.
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(egui::RichText::new(&s.value).font(big).color(colour));
            if let Some(u) = s.unit {
                ui.label(
                    egui::RichText::new(u)
                        .text_style(egui::TextStyle::Small)
                        .color(weak),
                );
            }
        });
        ui.label(
            egui::RichText::new(s.label)
                .text_style(egui::TextStyle::Small)
                .color(weak),
        );
    })
    .response
    .on_hover_text(s.hover);
}

/// A row of [`stat`]s across the top of a section, evenly gutted.
///
/// The gutter is the shape's, not a second free choice: bare numbers are held apart by whitespace and
/// nothing else, so they get the wide one; tiles are held apart by their own borders, so a gutter that
/// wide would push the row off a narrow dock panel for no gain.
fn stat_row(ui: &mut egui::Ui, stats: &[pacing::Stat], shape: StatShape) {
    let gutter = match shape {
        StatShape::Bare => COL_GUTTER * 2.0,
        StatShape::Tile => COL_GUTTER,
    };
    ui.horizontal_top(|ui| {
        for (i, s) in stats.iter().enumerate() {
            if i > 0 {
                ui.add_space(gutter);
            }
            stat(ui, s, shape);
        }
    });
}

/// [`fact_grid`], with each value coloured by the health the projection decided for it.
///
/// ⚑ **Two grids, and why.** This is not a copy of [`fact_grid`] that drifted: the two take different row
/// types on purpose. `fact_grid` draws [`objects::Fact`], whose values are all one weight because a
/// decoded object field has no health to carry; this draws [`pacing::Fact`], which does. Merging them
/// means giving `objects::Fact` a health it would always fill in as `Good`, which is a field that exists
/// to be ignored. **The merge is worth doing the moment a second panel needs a coloured fact row**, and at
/// that point the shared row type is this one and `objects::Fact` is the one that converts.
fn health_grid(ui: &mut egui::Ui, id: &str, facts: &[pacing::Fact]) {
    let weak = ui.visuals().weak_text_color();
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([COL_GUTTER, 3.0])
        .show(ui, |ui| {
            for f in facts {
                ui.label(
                    egui::RichText::new(f.label)
                        .text_style(egui::TextStyle::Small)
                        .color(weak),
                );
                let v = egui::RichText::new(&f.value).color(health_colour(ui, f.health));
                ui.label(if f.mono { v.monospace() } else { v });
                ui.end_row();
            }
        });
}

/// A horizontal bar with a threshold tick and a legend beneath it.
///
/// **The one place on this tab where the data has shape.** Ring occupancy is a fraction of a capacity with
/// a mark on it, and a fraction is a bar: this is the owner's *"strong visuals"* at the scale the fact
/// actually has, rather than a graph invented for a table.
///
/// The legend is drawn unconditionally and is not optional in [`pacing::Meter`] either. The failure this
/// repo has already paid for was a lens that was entirely correct and communicated nothing, received as
/// *"what are the purple boxes"*. Visual weight without a legend is a wall of monospace arriving from the
/// other side.
fn meter(ui: &mut egui::Ui, m: &pacing::Meter) {
    let h = 10.0;
    // Never wider than a comfortable reading length, never narrower than a bar a fraction is legible on.
    let w = ui.available_width().clamp(80.0, 320.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter();
    let radius = egui::CornerRadius::same(2);
    // The trough is the recessed field fill, which is the surface a value sits *in* rather than *on*.
    p.rect_filled(rect, radius, ui.visuals().extreme_bg_color);
    if m.fill > 0.0 {
        let mut filled = rect;
        filled.set_width(rect.width() * m.fill);
        p.rect_filled(filled, radius, crate::theme::ACCENT);
    }
    if let Some(mark) = m.mark {
        let x = rect.left() + rect.width() * mark;
        p.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, ui.visuals().strong_text_color()),
        );
    }
    p.rect_stroke(
        rect,
        radius,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    response.on_hover_text(&m.legend);
    ui.label(
        egui::RichText::new(&m.legend)
            .text_style(egui::TextStyle::Small)
            .color(ui.visuals().weak_text_color()),
    );
}

// -------------------------------------------------------------------------------------------------------
// ⚑ The object preview: one picture, drawn in two places
// -------------------------------------------------------------------------------------------------------

/// **The preview's picture as a texture**, uploaded once per picture rather than once per frame.
///
/// Cached in egui's own per-context store, keyed on the fingerprint the picture was taken under
/// ([`crate::preview::Preview::art_print`]) together with the archetype. Two different objects whose art
/// happened to fingerprint the same would otherwise share a texture, which is the one collision a hash on
/// its own permits.
///
/// It lives here rather than on [`crate::screen_pick::Panel`] because a `TextureHandle` is an egui type and
/// that panel's projection is deliberately free of them, and here rather than on [`Panels`] because
/// `Panels` is rebuilt every frame and a handle rebuilt with it would upload the same pixels forever.
///
/// `None` is the silhouette case, which has no pixels to upload and is drawn as rectangles instead.
fn preview_texture(
    ctx: &egui::Context,
    p: &crate::preview::Preview,
) -> Option<egui::TextureHandle> {
    let crate::preview::Art::Captured(shot) = &p.art else {
        return None;
    };
    let id = egui::Id::new("object-preview-texture");
    let key = (p.key.archetype.clone(), p.key.subtype, p.art_print);
    if let Some((had, tex)) =
        ctx.data(|d| d.get_temp::<((String, Option<u8>, u64), egui::TextureHandle)>(id))
    {
        if had == key {
            return Some(tex);
        }
    }
    // Transparent where the sprite is transparent, so the ghost is the object and not a box around it.
    let img = egui::ColorImage {
        size: [shot.w, shot.h],
        source_size: egui::vec2(shot.w as f32, shot.h as f32),
        pixels: shot
            .px
            .iter()
            .map(|s| match s {
                Some((r, g, b)) => egui::Color32::from_rgb(*r, *g, *b),
                None => egui::Color32::TRANSPARENT,
            })
            .collect(),
    };
    let tex = ctx.load_texture("object-preview", img, egui::TextureOptions::NEAREST);
    ctx.data_mut(|d| d.insert_temp(id, (key, tex.clone())));
    Some(tex)
}

/// Padding inside an overlay chip drawn on the picture, and the inset of the chip from the picture's own
/// edge. One constant, so the two chips this file draws over the game cannot sit at two different insets.
const OVERLAY_PAD: f32 = 6.0;

// -------------------------------------------------------------------------------------------------------
// ⚑ The height bound on anything painted over the picture, and why it is here rather than in the strings
// -------------------------------------------------------------------------------------------------------

/// The most of the picture's height a single overlay block may cover.
///
/// # The owner's complaint is about a class, so the fix has to be about the class
///
/// 2026-09-10: *"make sure to audit the verbosity of all things going in the black box please, there's a
/// lot of times I accidentally click and then like 1/4th of my screen is hidden behind it."* Every
/// producer that feeds [`readout_overlay`] was free to be as long as it liked, and one was: the ring
/// placement line carried a whole standing statement it did not need. Shortening that string fixes that
/// string. It does nothing about the next producer somebody writes, which is what *"a lot of times"* is
/// about.
///
/// So the bound lives **here**, at the one function that paints over the picture, and it holds for callers
/// that do not exist yet. Two blocks can be on screen at once (the armed notice at the top,
/// [`readout_overlay`] at the bottom), which is why this is well under the quarter he named rather than
/// equal to it.
const OVERLAY_MAX_FRACTION: f32 = 0.20;

/// ...and never more text rows than this, however tall the picture gets.
///
/// The fraction alone is not a bound a person can feel: on a maximised window a fifth of the picture is
/// still eight or nine rows, which is the wall he was looking at. This is what actually binds at the sizes
/// he plays at, and the fraction is what takes over when the picture is small.
const OVERLAY_MAX_ROWS: usize = 5;

/// **The rows one overlay block may spend**, as a pure function of the geometry, so it is testable with no
/// display, no context and no window. See rule 2 of this lane: he is using the window right now.
///
/// ⚑ **`max(lines)` is the deliberate exception to the fraction.** On a picture too short for the bound to
/// afford one row per line, the block goes over rather than dropping a line, because a block that painted
/// a head and silently swallowed the refusal underneath it is the one failure this must not have. Above
/// `overlay_min_bounded_height` that case cannot arise and the fraction holds.
fn overlay_row_budget(picture_h: f32, row_h: f32, lines: usize) -> usize {
    let gaps = OVERLAY_PAD * 2.0 + OVERLAY_PAD * 0.5 * lines.saturating_sub(1) as f32;
    let usable = picture_h * OVERLAY_MAX_FRACTION - gaps;
    let by_height = (usable / row_h.max(1.0)).floor().max(0.0) as usize;
    by_height.min(OVERLAY_MAX_ROWS).max(lines)
}

/// The picture height at and above which `overlay_row_budget` is genuinely bounded by the fraction,
/// which is the precondition the guarantee is stated under.
///
/// Test-only: it exists so the guarantee is asserted at its own stated precondition rather than at a
/// height somebody picked because it passed.
#[cfg(test)]
fn overlay_min_bounded_height(row_h: f32, lines: usize) -> f32 {
    let gaps = OVERLAY_PAD * 2.0 + OVERLAY_PAD * 0.5 * lines.saturating_sub(1) as f32;
    (row_h * lines as f32 + gaps) / OVERLAY_MAX_FRACTION
}

/// **Spend a row budget across the lines**, giving each at least one row and handing the surplus out from
/// the top down.
///
/// `want` is what each line would take unbounded. The result is what each line gets.
///
/// # Why every line is guaranteed a row before any line gets a second
///
/// The lines are `head`, `detail`, `outcome`, in that order, and the last of the three is where a refusal
/// reason and *"the machine was put back"* live. A budget spent purely greedily from the top would let a
/// verbose head eat the whole block and leave the outcome unpainted, which converts a verbosity problem
/// into a swallowed-refusal problem. That trade is not available: this file's own rule is that a refusal
/// is the one outcome the feature is not allowed to have go missing.
///
/// The surplus then goes top down, because the head is the answer to the click and the addressing beneath
/// it is what a reader checks the answer against.
fn overlay_spend_rows(want: &[usize], budget: usize) -> Vec<usize> {
    let mut got = vec![1usize; want.len()];
    let mut left = budget.saturating_sub(want.len());
    for (i, w) in want.iter().enumerate() {
        let extra = w.saturating_sub(1).min(left);
        got[i] += extra;
        left -= extra;
    }
    got
}

/// **Paint a block of text over the picture, at one of its corners, on its own panel.**
///
/// The one thing this exists to get right is that it takes **no layout space**: everything is painted
/// through [`egui::Painter`] against a rect that was already allocated for the image, so nothing it draws
/// can move the picture. That is the whole of the owner's second finding — *"whenever I place something
/// this text box comes up and shifts the window for the game"* — and it is a property of the mechanism
/// here rather than of any caller's care.
///
/// `lines` are drawn top to bottom in the order given, each with its own face and colour, wrapped to the
/// picture's width less the padding on both sides. Returns the rect it covered so a caller can stack a
/// second block clear of the first.
///
/// ⚑ **The block's height is bounded here, not by its callers.** See [`OVERLAY_MAX_FRACTION`]: a caller
/// may hand this any string at all and the result still cannot cover more than a fifth of the picture or
/// [`OVERLAY_MAX_ROWS`] rows, whichever is smaller. Overflow is **elided with a visible character**, never
/// dropped, so a reader can tell that a sentence was cut rather than believing they read all of it.
///
/// **A scroll region was the obvious alternative and it is barred**: scrolling needs allocated layout, and
/// this block exists precisely because the readout used to take layout and shove the picture down the
/// screen. Trading the owner's 2026-09-10 finding for his 2026-09-09 one is not a fix.
fn overlay_block(
    ui: &egui::Ui,
    picture: egui::Rect,
    lines: &[(String, egui::FontId, egui::Color32)],
    top: bool,
) -> egui::Rect {
    if lines.is_empty() {
        return egui::Rect::NOTHING;
    }
    let wrap = (picture.width() - OVERLAY_PAD * 4.0).max(40.0);
    // Laid out once unbounded to learn what each line *wants*, then again against the rows it *gets*.
    // Two layouts of one job rather than a guess: the budget is spent on measured demand, so a one-row
    // head does not hold rows the addressing under it needs. egui caches galleys, so the second pass of
    // an unchanged line is a lookup.
    let want: Vec<usize> = lines
        .iter()
        .map(|(t, face, colour)| {
            ui.painter()
                .layout(t.clone(), face.clone(), *colour, wrap)
                .rows
                .len()
        })
        .collect();
    let row_h = ui.ctx().fonts_mut(|f| f.row_height(&lines[0].1));
    let budget = overlay_row_budget(picture.height(), row_h, lines.len());
    let rows = overlay_spend_rows(&want, budget);
    let galleys: Vec<_> = lines
        .iter()
        .zip(&rows)
        .map(|((t, face, colour), max_rows)| {
            let mut job = egui::text::LayoutJob::simple(t.clone(), face.clone(), *colour, wrap);
            job.wrap.max_rows = *max_rows;
            // The default is already an ellipsis; named here because *that a cut is visible* is the
            // property this bound rests on, and a silent truncation would make the block lie.
            job.wrap.overflow_character = Some('…');
            (ui.painter().layout_job(job), *colour)
        })
        .collect();
    let w = galleys
        .iter()
        .map(|(g, _)| g.size().x)
        .fold(0.0_f32, f32::max);
    let h: f32 = galleys.iter().map(|(g, _)| g.size().y).sum::<f32>()
        + OVERLAY_PAD * 0.5 * (galleys.len().saturating_sub(1)) as f32;
    let size = egui::vec2(w + OVERLAY_PAD * 2.0, h + OVERLAY_PAD * 2.0);
    let min = if top {
        picture.min + egui::vec2(OVERLAY_PAD, OVERLAY_PAD)
    } else {
        egui::pos2(
            picture.min.x + OVERLAY_PAD,
            picture.max.y - OVERLAY_PAD - size.y,
        )
    };
    let panel = egui::Rect::from_min_size(min, size);
    // Clipped to the picture for [`ghost`]'s reason: this is a statement *about the picture*, and one
    // painted over the letterbox bars would be a statement about somewhere the machine is not showing.
    let painter = ui.painter().with_clip_rect(picture);
    // Opaque enough to read a proportional face against arbitrary game art, and never fully opaque: a
    // person must be able to see that there is a picture under it.
    painter.rect_filled(
        panel,
        egui::CornerRadius::same(3),
        egui::Color32::from_black_alpha(215),
    );
    let mut y = panel.min.y + OVERLAY_PAD;
    for (g, colour) in &galleys {
        let at = egui::pos2(panel.min.x + OVERLAY_PAD, y);
        painter.galley(at, g.clone(), *colour);
        y += g.size().y + OVERLAY_PAD * 0.5;
    }
    panel
}

/// **The standing armed statement, drawn inside the picture**: an accent border round the whole of it and
/// a chip naming the mode and the way out.
///
/// See the call site for the finding. The border is not redundant with the chip: the chip is at one
/// corner and a person aiming at the far side of the picture is not looking at it, whereas an edge is in
/// peripheral vision wherever the pointer is.
fn armed_frame(ui: &egui::Ui, picture: egui::Rect, notice: &str) {
    let painter = ui.painter().with_clip_rect(picture);
    painter.rect_stroke(
        picture,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(2.0, ui.visuals().warn_fg_color),
        egui::StrokeKind::Inside,
    );
    let face = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Small)
        .cloned()
        .unwrap_or_else(|| egui::FontId::proportional(10.0));
    overlay_block(
        ui,
        picture,
        &[(notice.to_owned(), face, ui.visuals().warn_fg_color)],
        true,
    );
}

/// **The click's standing answer, drawn over the picture at its bottom left.**
///
/// The three parts are [`screen_pick::Readout`]'s own fields, handed over by whoever composed them:
/// `pick::Pick` already carries the sentence and the addressing separately, and the outcome is the
/// panel's own count. Nothing here recovers structure by looking at the text, which is the same rule that
/// keeps the colour on `refused` rather than on a `"REFUSED"` prefix.
///
/// Coloured on the **field**, never on the shape of the text. See `screen_pick::Readout`.
fn readout_overlay(ui: &egui::Ui, picture: egui::Rect, r: &screen_pick::Readout) -> egui::Rect {
    let style = |s: egui::TextStyle, fallback: f32| {
        ui.style()
            .text_styles
            .get(&s)
            .cloned()
            .unwrap_or_else(|| egui::FontId::proportional(fallback))
    };
    let body = style(egui::TextStyle::Body, 13.0);
    let mono = style(egui::TextStyle::Monospace, 12.0);
    let small = style(egui::TextStyle::Small, 10.0);
    // Against a picture rather than against the panel, so the head is the theme's own emphasis colour
    // and not `strong_text_color`, which is tuned for a panel ground and can vanish on light game art.
    let head_colour = if r.refused {
        ui.visuals().error_fg_color
    } else {
        egui::Color32::WHITE
    };
    let mut lines = vec![(r.head.clone(), body, head_colour)];
    if let Some(d) = &r.detail {
        // The addressing recedes and goes monospace: it is what a reader checks the sentence against, in
        // the spelling they would compare with a tool's reply.
        lines.push((d.clone(), mono, egui::Color32::from_white_alpha(190)));
    }
    if let Some(o) = &r.outcome {
        lines.push((
            o.clone(),
            small,
            if r.refused {
                ui.visuals().error_fg_color
            } else {
                crate::theme::SUCCESS
            },
        ));
    }
    overlay_block(ui, picture, &lines, false)
}

/// The same readout **as a laid-out card**, for the one state that has no picture to draw it over.
///
/// A second rendering of one fact, which this file otherwise refuses — and it is here because the
/// alternative is worse: a panel that showed nothing when there is no frame yet would swallow exactly the
/// refusals a person hits before the first frame. It reads the identical fields in the identical order.
fn readout_card(ui: &mut egui::Ui, r: &screen_pick::Readout) {
    let refused = r.refused;
    let head = r.head.clone();
    let detail = r.detail.clone();
    let outcome = r.outcome.clone();
    card(ui, |ui| {
        let head_colour = if refused {
            ui.visuals().error_fg_color
        } else {
            ui.visuals().strong_text_color()
        };
        ui.label(egui::RichText::new(head).color(head_colour));
        if let Some(d) = detail {
            ui.label(
                egui::RichText::new(d)
                    .monospace()
                    .color(ui.visuals().weak_text_color()),
            );
        }
        if let Some(o) = outcome {
            ui.label(
                egui::RichText::new(o)
                    .text_style(egui::TextStyle::Small)
                    .color(if refused {
                        ui.visuals().error_fg_color
                    } else {
                        crate::theme::SUCCESS
                    }),
            );
        }
    });
}

/// **Draw the preview under the pointer, as a ghost.**
///
/// ⚑ **It must not look like a placed object**, and that is a correctness requirement rather than a taste:
/// a faithful sprite sitting under the cursor reads as something that is already there, and the person
/// then clicks to place a second one. So it is drawn at partial alpha inside an accent outline, with a
/// cross at the object's own origin. None of the three is decoration: the alpha says it is not real, the
/// outline says where the footprint ends when the art is dark, and the cross says which point of the
/// picture the click actually puts at the pointer.
///
/// `dot_scale` is points per screen dot, taken from the picture's own drawn size, so the ghost is exactly
/// the size the object will be in the picture below it at every aspect mode and every display scale.
fn ghost(
    ui: &egui::Ui,
    p: &crate::preview::Preview,
    picture: egui::Rect,
    at: egui::Pos2,
    dot_scale: f32,
) {
    if dot_scale <= 0.0 {
        return;
    }
    let size = egui::vec2(p.w as f32 * dot_scale, p.h as f32 * dot_scale);
    // The anchor is the object's own position inside its own picture, so this subtraction is what puts the
    // object where the pointer is rather than putting its top left corner there.
    let origin = at - egui::vec2(p.anchor.0 as f32 * dot_scale, p.anchor.1 as f32 * dot_scale);
    let rect = egui::Rect::from_min_size(origin, size);
    // ⚑ **Clipped to the PICTURE, not to the pane.** A ghost is a preview of a placement in the game
    // world, and the game world ends where the picture does. Clipping to the pane instead would draw the
    // object over the letterbox bars, which is a claim about somewhere the machine is not showing; and an
    // object near the edge whose ghost is cut off at the edge is telling the truth, which is that part of
    // it lands outside what is on screen. `ui.clip_rect()` here is the pane's and is what this used to
    // pass, which made the call a no-op.
    let painter = ui.painter().with_clip_rect(picture);

    match preview_texture(ui.ctx(), p) {
        Some(tex) => {
            painter.image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                // Not opaque, and never configurable: the whole point is that it cannot be mistaken for a
                // sprite the machine drew.
                egui::Color32::from_white_alpha(150),
            );
        }
        None => {
            // The silhouette: the rectangles are measured even though the colours are missing, so the
            // shape and the size are still honest.
            for c in &p.cells {
                let cell = egui::Rect::from_min_size(
                    origin + egui::vec2(c.x as f32 * dot_scale, c.y as f32 * dot_scale),
                    egui::vec2(c.w as f32 * dot_scale, c.h as f32 * dot_scale),
                );
                painter.rect_filled(
                    cell,
                    egui::CornerRadius::ZERO,
                    crate::theme::ACCENT.gamma_multiply(0.18),
                );
            }
        }
    }
    painter.rect_stroke(
        rect,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(1.0, crate::theme::ACCENT),
        egui::StrokeKind::Outside,
    );
    // The origin cross, which is the answer to "where exactly does it land".
    let arm = 3.0;
    let stroke = egui::Stroke::new(1.0, crate::theme::ACCENT);
    painter.line_segment(
        [at - egui::vec2(arm, 0.0), at + egui::vec2(arm, 0.0)],
        stroke,
    );
    painter.line_segment(
        [at - egui::vec2(0.0, arm), at + egui::vec2(0.0, arm)],
        stroke,
    );
}

/// **The preview, in the picker**, at a legible scale beside its own measurement note.
///
/// The scale is a whole number of screen dots per point, because a preview of a 16 dot sprite drawn at
/// 2.7x is a blurred claim about pixel art. It is the largest whole scale that fits the box, and never
/// smaller than 1: an object bigger than the box is drawn at 1 and clipped rather than shrunk into
/// something nobody can identify.
fn preview_card(ui: &mut egui::Ui, out: &crate::preview::Outcome) {
    let weak = ui.visuals().weak_text_color();
    card(ui, |ui| {
        if let Some(p) = out.drawable() {
            let box_dots = 48.0_f32;
            let scale = (box_dots / p.w.max(p.h).max(1) as f32).floor().max(1.0);
            let size = egui::vec2(p.w as f32 * scale, p.h as f32 * scale);
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            // Clipped to what was allocated, so an object larger than the box is cut rather than
            // spilling over the note under it.
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            match preview_texture(ui.ctx(), p) {
                Some(tex) => {
                    painter.image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                None => {
                    // The silhouette, outlined rather than filled: this is the shape with the colours
                    // honestly missing, and a filled block would read as a picture of a black object.
                    for c in &p.cells {
                        let cell = egui::Rect::from_min_size(
                            rect.min + egui::vec2(c.x as f32 * scale, c.y as f32 * scale),
                            egui::vec2(c.w as f32 * scale, c.h as f32 * scale),
                        );
                        painter.rect_stroke(
                            cell,
                            egui::CornerRadius::ZERO,
                            egui::Stroke::new(1.0, crate::theme::WARNING),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
            ui.label(
                egui::RichText::new(p.size_line())
                    .text_style(egui::TextStyle::Small)
                    .color(weak),
            );
        }
        // ⚑ **The sentence is drawn in every arm, including the one with a picture in it**, and that is
        // the difference between the panel claiming a fact and the panel showing a measurement. A person
        // looking at a wrong picture usually knows in a way no code here can, and only if the panel has
        // told them where the picture came from.
        ui.label(
            egui::RichText::new(out.sentence())
                .text_style(egui::TextStyle::Small)
                .color(weak),
        );
    });
}

/// A section head: the title, what it covers, and the served row it is a direct read of.
///
/// This replaces three headings of the form `players — emulator/player_state — 2 of 66 slots`, which used
/// two em dashes to do a colon's work and a parenthesis's work in one line. Em and en dashes are barred
/// from a tool's user-facing text by the owner's 2026-09-05 ruling (CHROME_SPEC, "Text in the tools"), and
/// the three facts are not the same kind of thing anyway: one is a name, one is a scope, one is a wire
/// method. So they get three different weights on one line instead of one string with punctuation in it.
fn section(ui: &mut egui::Ui, title: &str, scope: Option<String>, method: &str) {
    let (weak, strong) = (
        ui.visuals().weak_text_color(),
        ui.visuals().strong_text_color(),
    );
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new(title).color(strong));
        if let Some(s) = scope {
            ui.label(egui::RichText::new(s).color(weak));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(method).monospace().color(weak))
                .on_hover_text(
                    "the served row this section is a direct read of, so the panel and a client \
                         asking the same question see the same answer",
                );
        });
    });
}

/// The width `text` needs in `face`, measured with the font the theme actually installed.
fn text_w(ui: &egui::Ui, face: &egui::FontId, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), face.clone(), egui::Color32::PLACEHOLDER)
        .size()
        .x
}

/// The face a **header** cell is measured and drawn in.
///
/// ⚑ One function because the two used to disagree, and that is half of the owner's fourth finding.
/// [`column_widths`] measured every header in [`egui::TextStyle::Small`] and [`table_cell`] then drew it
/// in the *cell's* face, which is the body or the monospace face and is larger. A header wider than
/// anything in its column was therefore laid out in a box measured for a smaller font and ran into its
/// neighbour. There is now one face, and [`header_cell`] is what draws it.
fn head_face(ui: &egui::Ui) -> egui::FontId {
    ui.style()
        .text_styles
        .get(&egui::TextStyle::Small)
        .cloned()
        .unwrap_or_else(|| egui::FontId::proportional(10.0))
}

/// Column widths for `cols` over `cells`: the widest cell in each column, header included.
///
/// The **last** column is left out of that and takes whatever room remains, because it is the name column
/// and a symbol name has no bound: sized to its widest entry it would push the numeric columns off the
/// panel, which is the one failure a fixed-column table exists to prevent.
///
/// ⚑ **The gutter is NOT folded in here any more.** See [`fit_columns`], which is where the arithmetic
/// and the reason both live.
fn column_widths(ui: &egui::Ui, cols: &[objects::Col], cells: &[Vec<String>]) -> Vec<f32> {
    if cols.is_empty() {
        return Vec::new();
    }
    let head = head_face(ui);
    let faces: Vec<egui::FontId> = cols.iter().map(|c| cell_face(ui, c)).collect();
    let mut w: Vec<f32> = cols.iter().map(|c| text_w(ui, &head, c.head)).collect();
    for row in cells {
        for (i, cell) in row.iter().enumerate().take(cols.len()) {
            w[i] = w[i].max(text_w(ui, &faces[i], cell));
        }
    }
    fit_columns(&w, ui.available_width(), COL_GUTTER, NAME_COL_FLOOR)
}

/// **Natural widths in, drawn widths out** — the whole column arithmetic, with no `Ui` in it so the rule
/// is checkable rather than merely visible.
///
/// `natural` is what each column's widest entry actually measures, header included and **with no gutter
/// folded in**. That distinction is the owner's fourth finding, 2026-09-09: *"look how difficult objects
/// is to read"*, with headers reading as `slotaddrcode  x  yname`.
///
/// The gutter used to be added to every column's own width, and a width is not a gap: `slot` is
/// right-aligned (it is numeric) so its glyphs hug the RIGHT edge of its box, and `addr` is left-aligned
/// so its glyphs hug the LEFT edge of the next one. Two adjacent boxes, two sets of glyphs at the seam,
/// and the fourteen points of "gutter" sitting harmlessly at the far end of each column where nothing
/// needed separating. `slot` and `addr` were drawn touching.
///
/// So the gutter leaves the widths and becomes `item_spacing.x` in the row layouts, where it is a real
/// gap between every pair of columns whatever either one's alignment is. This function's job is then the
/// one thing that changes: the last column's remainder has to pay for the gutters as well as for the
/// other columns.
///
/// The last column takes what is left because it is the name column and a symbol name has no bound;
/// `floor` is the width below which a name stops being a name, and a column narrower than its own
/// content is left at its content width rather than stretched to the floor.
fn fit_columns(natural: &[f32], avail: f32, gutter: f32, floor: f32) -> Vec<f32> {
    let mut w = natural.to_vec();
    let Some((last, others)) = w.split_last_mut() else {
        return w;
    };
    let used: f32 = others.iter().sum::<f32>() + gutter * others.len() as f32;
    // Whatever is left, but never so little that the column is unreadable: a narrow panel gets a
    // truncated name with the whole of it on the hover, not a name reduced to one letter.
    *last = (avail - used).max(last.min(floor));
    w
}

/// One cell, `w` points wide, right-aligned when the column is numeric.
///
/// Right alignment is not a nicety on a numeric column: it is the whole reason a column of coordinates can
/// be compared down the page at all, and it is exactly what a padded `{:>7}` was reaching for and could
/// only approximate.
fn table_cell(ui: &mut egui::Ui, c: &objects::Col, w: f32, text: &str, colour: egui::Color32) {
    let rich = egui::RichText::new(text).color(colour);
    let rich = if c.mono { rich.monospace() } else { rich };
    let layout = if c.numeric {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    let h = ui.spacing().interact_size.y;
    // Whether this cell is going to be cut off, measured before it is drawn rather than inferred from
    // the response afterwards: `Label::truncate` allocates the width it was given either way, so the
    // rect it hands back says nothing about whether any glyphs were dropped.
    let cut = text_w(ui, &cell_face(ui, c), text) > w;
    ui.allocate_ui_with_layout(egui::vec2(w, h), layout, |ui| {
        let r = ui.add(egui::Label::new(rich).truncate());
        // The short cell says the object has no name; the sentence saying *why* is a fact about the
        // listing and belongs on the hover, not in a column six characters wide.
        if text == objects::NO_NAME {
            r.on_hover_text(objects::NO_NAME_WHY);
        } else if cut {
            // A truncated cell is unreadable, not merely tidy, so the whole of it is one hover away.
            r.on_hover_text(text);
        }
    });
}

/// One **header** cell, `w` points wide, aligned as its column's cells are so the header sits over the
/// digits it names.
///
/// Separate from [`table_cell`] because a header is not a cell: it is drawn in [`head_face`], which is
/// the face [`column_widths`] measured it in, and it carries none of that function's hovers — a header
/// is never `objects::NO_NAME` and never truncated, because its own width is one of the terms its
/// column's width is the maximum of.
fn header_cell(ui: &mut egui::Ui, c: &objects::Col, w: f32, colour: egui::Color32) {
    let layout = if c.numeric {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    let h = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(egui::vec2(w, h), layout, |ui| {
        ui.label(
            egui::RichText::new(c.head)
                .text_style(egui::TextStyle::Small)
                .color(colour),
        );
    });
}

/// The face a cell of `c` is drawn in. One function, so the width measurement and the `RichText` cannot
/// disagree about which font is about to be used.
fn cell_face(ui: &egui::Ui, c: &objects::Col) -> egui::FontId {
    let want = if c.mono {
        egui::TextStyle::Monospace
    } else {
        egui::TextStyle::Body
    };
    ui.style()
        .text_styles
        .get(&want)
        .cloned()
        .unwrap_or_else(|| egui::FontId::proportional(13.0))
}

/// **What one row of a table is filled with**, in one function, so a selected row cannot be one colour in
/// the object pool and a different one in the spawn picker.
///
/// The order is the meaning: selection beats hover beats banding, because the selection is a fact about
/// the machine and the other two are facts about the pointer and the eye.
fn row_fill(ui: &egui::Ui, chosen: bool, hovered: bool, i: usize) -> egui::Color32 {
    if chosen {
        crate::theme::selection()
    } else if hovered {
        ui.visuals().widgets.hovered.bg_fill
    } else if i % 2 == 1 {
        ui.visuals().faint_bg_color
    } else {
        egui::Color32::TRANSPARENT
    }
}

/// **A one-column selectable list**: a header, a hairline, and one banded row per entry, with the
/// selection carried by fill. Returns the entry clicked this frame.
///
/// The caller owns the selection, exactly as it does for [`slot_table`]: a list that decided its own
/// selection would need a second copy of it, and the copy that matters is `spawn::Mode`'s, because that
/// is the one a click on the picture reads.
///
/// **Why this is not [`slot_table`].** That table is keyed on `objects::Row`/`Col`/`Field` and draws a
/// served object record; the audit's build order books generalising its four helpers off `objects::Col`
/// as the prerequisite for the Profiler, Watchpoints and Breakpoints parcels. This is a single column of
/// symbol names with no header cells to align, so it takes the piece of that furniture that is genuinely
/// shared today ([`row_fill`]) and leaves the rest for that parcel. **When the table furniture is
/// generalised, this folds into it** rather than growing a second column model of its own.
///
/// The name is drawn in the monospace face, which is P3's carve-out for *symbol names as they appear in a
/// listing* and not a licence for the prose around it.
/// **The subtype list**: one banded, selectable row per form, with the listing's own value beside it.
///
/// Not [`select_list`], and the difference is two columns rather than a preference. A subtype row carries
/// a **label and a value**, and the value is the whole point of the row: it is the byte a placement will
/// carry, read out of the listing, and a picker that showed only names would be offering a choice whose
/// consequence is invisible. It takes the piece of furniture that genuinely is shared ([`row_fill`]) and
/// nothing else, exactly as [`select_list`] does, and folds into the generalised table when that lands.
///
/// **A row that cannot be chosen is drawn and not offered.** Its reason is on the hover and it takes no
/// click, because hiding it would be this window deciding the listing is wrong, and cutting the value down
/// to a byte would place a different form than the row names.
///
/// Capped in height, unlike [`select_list`], and for that function's own stated reason inverted: the
/// archetype list is this pane's whole content and should take the pane, so the short list above it must
/// not grow into the space the long one needs.
fn subtype_list(
    ui: &mut egui::Ui,
    listing: &crate::spawn_picker::SubtypeListing,
    salt: &str,
) -> Option<String> {
    let mut hit = None;
    ui.scope(|ui| {
        // ⚑ **The x spacing is a REAL GUTTER now, and that is the whole of the first defect.**
        //
        // It was `vec2(0.0, 1.0)` — zero horizontally — which is what made the owner read a row as
        // `$20Down_Red`: the value and the name were two labels in one `horizontal` with nothing between
        // them. The listing has carried them as two fields all along (`SubtypeRow::value`,
        // `SubtypeRow::label`), so nothing about the model changed; the presentation was joining what the
        // model had kept apart. The y half stays 1.0, because the rows are meant to sit tight.
        ui.spacing_mut().item_spacing = egui::vec2(LIST_GUTTER, 1.0);
        let weak = ui.visuals().weak_text_color();
        ui.label(
            egui::RichText::new(&listing.count)
                .text_style(egui::TextStyle::Small)
                .color(weak),
        );
        let y = ui.cursor().top();
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        ui.add_space(3.0);
        // P7: an explicit, stable salt, so this scroll position is its own and not the archetype list's.
        //
        // ⚑ **`auto_shrink([false, true])`, and the ACROSS half is the second defect.** A vertical
        // `ScrollArea` has `auto_shrink.x = true` by default, so its width follows its content: the list
        // ended where the longest name ended and drew its scrollbar *inside* itself, immediately right of
        // the names, which is what the owner's arrow points at. Off across, the bar is pinned to the
        // panel's right edge — the exact remedy the watch-hits box already names in this file.
        //
        // **Vertically it stays ON**, deliberately: `max_height` is a cap, not a reservation, and a
        // listing with two subtypes must not hold 132 points of empty box open under it.
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .max_height(132.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let strong = ui.visuals().strong_text_color();
                let mono = ui
                    .style()
                    .text_styles
                    .get(&egui::TextStyle::Monospace)
                    .cloned()
                    .unwrap_or_else(|| egui::FontId::monospace(12.0));
                // The value column, measured once over the whole listing rather than per row, because a
                // column that is as wide as its own cell is not a column: the rows line up on the value
                // and that is the only reason it leads.
                let value_w = listing
                    .rows
                    .iter()
                    .map(|r| text_w(ui, &mono, &r.value))
                    .fold(0.0_f32, f32::max);
                // ⚑ **How far the band and the highlight reach, and that is the third defect.** It used
                // to be `ui.max_rect().x_range()` — the whole pane — so a selected row was a long empty
                // bar and the zebra banding striped a span with nothing in it. That empty striping is
                // what he called cluttered.
                //
                // Bounded to the LIST's own width instead: the widest row's content plus a margin, so
                // the click target still comfortably outruns the glyphs (which is why the band was full
                // width in the first place) without painting across a pane the list does not occupy.
                // Capped at the pane, so a name longer than the panel cannot push it off the edge.
                let label_w = listing
                    .rows
                    .iter()
                    .map(|r| text_w(ui, &mono, &r.label))
                    .fold(0.0_f32, f32::max);
                let band_w = (value_w + LIST_GUTTER + label_w + LIST_GUTTER * 2.0)
                    .min(ui.max_rect().width());
                let cell_h = ui.spacing().interact_size.y;
                for (i, r) in listing.rows.iter().enumerate() {
                    let ink = if r.offered { strong } else { weak };
                    let bg = ui.painter().add(egui::Shape::Noop);
                    let inner = ui.horizontal(|ui| {
                        // P3's carve-out: the value is hex out of a listing, so it is monospace, and it
                        // leads because the rows line up on it. Right-aligned inside a column of its own
                        // for `table_cell`'s reason, which is not a nicety: it is what makes a column of
                        // machine values comparable down the page, and it is what a padded `{:>4}` in the
                        // string would only have approximated.
                        ui.allocate_ui_with_layout(
                            egui::vec2(value_w, cell_h),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(egui::RichText::new(&r.value).monospace().color(ink));
                            },
                        );
                        ui.label(egui::RichText::new(&r.label).monospace().color(ink));
                    });
                    let left = ui.max_rect().left();
                    let band = egui::Rect::from_x_y_ranges(
                        left..=left + band_w,
                        inner.response.rect.y_range(),
                    )
                    .expand2(egui::vec2(0.0, 1.0));
                    let resp = ui.interact(band, ui.id().with((salt, i)), egui::Sense::click());
                    ui.painter().set(
                        bg,
                        egui::Shape::rect_filled(
                            band,
                            0.0,
                            row_fill(ui, r.selected, r.offered && resp.hovered(), i),
                        ),
                    );
                    match &r.note {
                        // The reason it cannot be chosen, on the row itself. No click is taken.
                        Some(n) => {
                            resp.on_hover_text(n);
                        }
                        None => {
                            if resp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                hit = Some(r.name.clone());
                            }
                        }
                    }
                }
            });
    });
    hit
}

/// **The Effects tab's row list**, [`select_list`]'s shape with one addition it needs and the spawn
/// picker's archetype list does not: a row that is **drawn and not selectable**, with the reason on it.
///
/// A separate function rather than a parameter on [`select_list`] because the archetype list has no such
/// row and never will — every `ObjDef_` it draws is placeable — and a bool nobody passes `true` for is a
/// branch nothing exercises. The shape it does share is deliberate: fill carries the selection, the band
/// is the full pane width so a click target does not stop where the glyphs do, and the salt keeps this
/// scroll position out of the hex dump's.
///
/// ⚑ The unselectable row is [`crate::effects::listing`]'s doing, not this function's: it is a name that
/// is one of the channel's **own cells**, and pointing a selector at itself makes the engine read the
/// pointer's bytes as the thing it points to. Drawn rather than dropped, because a row that vanishes
/// teaches nothing and a person who typed a prefix is owed the difference between what it matched and
/// what they may pick.
fn effects_list(
    ui: &mut egui::Ui,
    head: &str,
    rows: &[crate::effects::Row],
    salt: &str,
) -> Option<String> {
    let mut hit = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 1.0);
        let weak = ui.visuals().weak_text_color();
        ui.label(
            egui::RichText::new(head)
                .text_style(egui::TextStyle::Small)
                .color(weak),
        );
        let y = ui.cursor().top();
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        ui.add_space(3.0);
        egui::ScrollArea::vertical().id_salt(salt).show(ui, |ui| {
            let strong = ui.visuals().strong_text_color();
            for (i, r) in rows.iter().enumerate() {
                let bg = ui.painter().add(egui::Shape::Noop);
                let ink = if r.offered { strong } else { weak };
                let inner = ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&r.name).monospace().color(ink));
                });
                let band = egui::Rect::from_x_y_ranges(
                    ui.max_rect().x_range(),
                    inner.response.rect.y_range(),
                )
                .expand2(egui::vec2(0.0, 1.0));
                let sense = if r.offered {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                };
                let resp = ui.interact(band, ui.id().with((salt, i)), sense);
                ui.painter().set(
                    bg,
                    egui::Shape::rect_filled(
                        band,
                        0.0,
                        row_fill(ui, r.selected, r.offered && resp.hovered(), i),
                    ),
                );
                match &r.note {
                    // The reason rides the row's own hover, so it is attached to the thing it is about
                    // rather than filed in a legend somewhere below the list.
                    Some(note) => {
                        resp.on_hover_text(note);
                    }
                    None => {
                        if resp
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            hit = Some(r.name.clone());
                        }
                    }
                }
            }
        });
    });
    hit
}

fn select_list(
    ui: &mut egui::Ui,
    head: &str,
    rows: &[crate::spawn_picker::Row],
    salt: &str,
) -> Option<String> {
    let mut hit = None;
    ui.scope(|ui| {
        // Rows sit tighter than a panel's default flow; the zebra band is what separates them.
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 1.0);
        let weak = ui.visuals().weak_text_color();
        ui.label(
            egui::RichText::new(head)
                .text_style(egui::TextStyle::Small)
                .color(weak),
        );
        let y = ui.cursor().top();
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        ui.add_space(3.0);
        // P7: one panel, one scroll position, and the salt is what keeps it from colliding with the hex
        // dump's or the hit log's.
        // ⚑ **No `max_height`, since the picker became [`Tab::Spawn`].** It used to be capped at 140px
        // because it sat in the Screen strip and every pixel it took was a pixel off the game view --
        // which is the complaint that moved it. In a tab of its own the list is the whole content, so
        // it takes the pane's own height: `ScrollArea` bounds itself by the `Ui`'s available space, and
        // a fixed cap here would leave the bottom of a dedicated pane empty on purpose.
        //
        // ⚑ **Off ACROSS only**, [`subtype_list`]'s finding and the same fault: a list that shrinks to its
        // content puts its own scrollbar immediately right of the longest name instead of at the list's
        // outer edge. Vertically it stays on, for that function's reason and because the paragraph above
        // is about how much height this list may TAKE, which is not the same as how much it must HOLD.
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let strong = ui.visuals().strong_text_color();
                let mono = ui
                    .style()
                    .text_styles
                    .get(&egui::TextStyle::Monospace)
                    .cloned()
                    .unwrap_or_else(|| egui::FontId::monospace(12.0));
                // ⚑ **The band reaches past the glyphs, not across the pane.** [`subtype_list`]'s finding
                // applied to the list beside it, so a selected row is not a long empty bar and the zebra
                // banding does not stripe a span with nothing in it. The margin is what keeps the original
                // reason intact: a click target that stops exactly where the text stops is one a person
                // misses.
                let band_w = (rows
                    .iter()
                    .map(|r| text_w(ui, &mono, &r.name))
                    .fold(0.0_f32, f32::max)
                    + LIST_GUTTER * 2.0)
                    .min(ui.max_rect().width());
                for (i, r) in rows.iter().enumerate() {
                    // Reserved before the row so the band paints behind it, which is egui's own idiom and
                    // the one `slot_table` already uses.
                    let bg = ui.painter().add(egui::Shape::Noop);
                    let inner = ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&r.name).monospace().color(strong));
                    });
                    let left = ui.max_rect().left();
                    let band = egui::Rect::from_x_y_ranges(
                        left..=left + band_w,
                        inner.response.rect.y_range(),
                    )
                    .expand2(egui::vec2(0.0, 1.0));
                    let resp = ui.interact(band, ui.id().with((salt, i)), egui::Sense::click());
                    ui.painter().set(
                        bg,
                        egui::Shape::rect_filled(
                            band,
                            0.0,
                            row_fill(ui, r.selected, resp.hovered(), i),
                        ),
                    );
                    if resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        hit = Some(r.name.clone());
                    }
                }
            });
    });
    hit
}

/// A table of slots: a header row, hairline-separated, then one banded row per slot.
///
/// `selected` is `None` for a table whose rows are not clickable (the player section, which is a list of
/// two) and `Some(current)` for one whose rows are (the pool table, where a click opens the expansion).
/// Returns the slot clicked this frame, if any -- the caller owns the selection, because a table that
/// decided its own selection would need a second copy of it.
fn slot_table(
    ui: &mut egui::Ui,
    cols: &[objects::Col],
    rows: &[objects::Row],
    selected: Option<Option<u32>>,
    salt: &str,
) -> Option<u32> {
    let cells: Vec<Vec<String>> = rows.iter().map(|r| r.cells(cols)).collect();
    let widths = column_widths(ui, cols, &cells);
    let mut hit = None;

    ui.scope(|ui| {
        // Table rows sit tighter than a panel's default flow; the y half is the row gap, and the zebra
        // band below is what separates rows rather than whitespace.
        //
        // ⚑ **The x half is the COLUMN gutter, and it used to be zero.** See [`fit_columns`] for the
        // whole finding: the gutter was folded into each column's own width, which puts the space at the
        // far end of a column instead of at the seam between two, so a right-aligned column's digits and
        // the next left-aligned column's glyphs were drawn touching. Here it is a real gap between every
        // pair of columns, whatever either one's alignment is.
        ui.spacing_mut().item_spacing = egui::vec2(COL_GUTTER, 1.0);

        let weak = ui.visuals().weak_text_color();
        ui.horizontal(|ui| {
            for (c, w) in cols.iter().zip(&widths) {
                // [`header_cell`], not [`table_cell`]: a header is drawn in the face it was measured in.
                header_cell(ui, c, *w, weak);
            }
        });
        let y = ui.cursor().top();
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        ui.add_space(3.0);

        for (i, (r, cs)) in rows.iter().zip(&cells).enumerate() {
            // Reserved BEFORE the cells so the band paints behind them. `Painter::add(Shape::Noop)` then
            // `Painter::set` is egui's own idiom for painting under content that has not been laid out
            // yet; there is no z-order to fight and no second pass.
            let bg = ui.painter().add(egui::Shape::Noop);
            let inner = ui.horizontal(|ui| {
                for ((c, w), text) in cols.iter().zip(&widths).zip(cs) {
                    table_cell(ui, c, *w, text, cell_colour(ui, c, text, r.active));
                }
            });
            // Full panel width, not the width of the text: a click target that stops where the last
            // column's glyphs stop is a click target a person misses.
            let band =
                egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), inner.response.rect.y_range())
                    .expand2(egui::vec2(0.0, 1.0));

            let chosen = selected == Some(Some(r.slot));
            let resp = selected
                .map(|_| ui.interact(band, ui.id().with((salt, r.slot)), egui::Sense::click()));
            let fill = row_fill(
                ui,
                chosen,
                resp.as_ref().is_some_and(egui::Response::hovered),
                i,
            );
            ui.painter()
                .set(bg, egui::Shape::rect_filled(band, 0.0, fill));

            if let Some(resp) = resp {
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                    hit = Some(r.slot);
                }
            }
        }
    });
    hit
}

/// What colour a cell is drawn in, and every branch of it is a fact rather than a taste.
///
/// * An **absent** value and an **unnamed** object recede: they are answers, but they are not the answer a
///   reader is scanning for, and at full weight sixty of them would be the loudest thing on the tab.
/// * An **inactive slot's** whole row recedes for the same reason: the row exists to say "not present".
/// * A **role** on a live slot is [`crate::theme::SUCCESS`], because "player 1 is here" is the one thing
///   the player section is asked and it should be answerable without reading.
/// * The **name** is the emphasis colour: it is what a human scans a pool table for.
fn cell_colour(ui: &egui::Ui, c: &objects::Col, text: &str, active: bool) -> egui::Color32 {
    if text == objects::ABSENT || text == objects::NO_NAME || text == objects::NOT_PRESENT {
        return ui.visuals().weak_text_color();
    }
    if !active {
        return ui.visuals().weak_text_color();
    }
    match c.field {
        objects::Field::Role => crate::theme::SUCCESS,
        objects::Field::Name => ui.visuals().strong_text_color(),
        _ => ui.visuals().text_color(),
    }
}

/// A served value as the panel prints it. **Exhaustive by construction, and that is the point.**
///
/// This used to be two arms: `String(s) => s.clone()` and `other => other.to_string()`. It was correct
/// for every value the server actually sends today, because
/// [`DecodedRecord::to_json`](oracle_core::decoders) emits only scalars and the one composite key
/// (`"fields"`) is skipped by its caller before it ever gets here. It was correct **by luck about the
/// wire**, not by anything this function does: the day a served key becomes an object or an array, that
/// catch-all `to_string()` puts `{"a":1,"b":[2,3]}` on the owner's screen, which is exactly the raw JSON
/// the style page's **P1** forbids, and no test in the crate would have gone red.
///
/// So the catch-all is gone and every `serde_json::Value` variant is spelled out. The two composite arms
/// **state what arrived** instead of dumping it: a nested value is a fact about the served shape, and
/// telling the reader that a shape they cannot see has appeared is useful, whereas printing its
/// punctuation at them is not. If that ever becomes the wrong answer it will be because a real composite
/// key is worth drawing, and drawing it is a panel decision with a layout attached, not a fallthrough.
///
/// `Null` is a stated absence rather than the four characters `null`, per **P6**: an absent fact is a
/// line that says so.
///
/// Guarded by [`json_tests::no_served_value_can_put_raw_json_on_the_screen`], which walks every variant.
fn render(v: &serde_json::Value) -> String {
    match v {
        // A string is the payload without its quotes. This is the overwhelmingly common case.
        serde_json::Value::String(s) => s.clone(),
        // Numbers and booleans spell identically in JSON and in prose, so there is no punctuation to leak.
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        // P6: `null` on the wire means the server had nothing, and a reader is owed that in words.
        serde_json::Value::Null => NO_VALUE.to_owned(),
        serde_json::Value::Array(a) => format!(
            "{} value{} in a list. This panel draws single values, so the list itself is not shown.",
            a.len(),
            if a.len() == 1 { "" } else { "s" }
        ),
        serde_json::Value::Object(m) => format!(
            "{} key{} in a nested record. This panel draws single values, so the record is not shown.",
            m.len(),
            if m.len() == 1 { "" } else { "s" }
        ),
    }
}

/// What [`render`] prints for a served `null`. A stated absence, never the token `null` and never a zero.
const NO_VALUE: &str = "no value (the server sent nothing here)";

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
                None => "NOT MEASURED: this strip was built with no bus to ask".into(),
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
                "NOT MEASURED: this strip was built with no bus to ask".into(),
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
                "{} (a bus client is holding these, not you; emulator/release_all clears them)",
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
                "NOT MEASURED: this strip was built with no bus to ask".into(),
            ));
        };
        let head = h.headline()?;
        match h.advice() {
            Some(a) => Some((HALTING_LABEL, format!("{head} · {a}"))),
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
                    (None, None) => "no listing loaded".into(),
                    // A table that resolves nothing at this address is a real answer and a different one:
                    // the listing is there and the PC is before its first symbol (or past its end).
                    (None, Some(_)) => "the listing names no symbol at or before pc".into(),
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
    // **`Planes` shares the picture leaf with `Screen`** rather than taking a pane of its own: they are
    // two views of the same frame, a person switches between them rather than watching both, and a plane
    // wants every pixel of the widest pane in the window.
    let mut dock = egui_dock::DockState::new(vec![Tab::Screen, Tab::Planes]);
    let surface = dock.main_surface_mut();
    // ⚑ **`Spawn` shares the top-right leaf with `Pacing`, and the pairing is the point.** The owner
    // moved the picker out of the Screen strip because it was taking the game view's height, so the one
    // place it must not go is the leaf holding the picture. It must also not be mutually exclusive with
    // the tab a placement is checked against — `Objects` lists the pool the spawn lands in — which rules
    // out that leaf too. `Pacing` is the right column's one tab that is never part of a spawn gesture,
    // so a spawn never costs the reader a tab they were about to look at.
    // ⚑ **`Effects` joins that same leaf**, and for the same argument one step on. It is the other
    // standing list you read while looking at the picture, and the one leaf it must not share is the
    // picture's: the whole point of the panel is watching the game change while you pick, so a tab that
    // covered the game would make the switchboard unusable in exactly the gesture it exists for.
    let [_, right] = surface.split_right(
        egui_dock::NodeIndex::root(),
        0.68,
        vec![Tab::Pacing, Tab::Spawn, Tab::Effects],
    );
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
/// measurement of the arrangement rather than of the panels. This function puts every [`Tab::ALL`] variant
/// — today eleven, and however many there are tomorrow, because it iterates the array — in a leaf of its
/// own, so every body runs on every frame: the worst case a user could arrange, and the only
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
    // Alternating right/below, so the leaves stay roughly square rather than becoming slivers in
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
/// **The two machine keys that are served methods** (S3, `F1` and `F5`). Here rather than in
/// `crate::input` for the reason the three above are here: the test below checks these strings against
/// the engine's own `METHODS` registry, and a second copy of a wire name somewhere else is a name that
/// can rot without failing anything.
pub const RESET: &str = "emulator/reset";
pub const RELOAD_ROM: &str = "emulator/reload_rom";

/// **The bar's own labels, as constants**, because [`crate::screen`] reports the bar over
/// `emulator/screen_text` and a label written twice is a window and a tool describing one button
/// differently. Nothing here is a *copy* of what the bar draws — these are the strings the bar draws.
pub const PAUSE_LABEL: &str = "⏸ pause";
pub const RESUME_LABEL: &str = "▶ resume";
pub const STEP_LABEL: &str = "⏭ step";

/// ⚑ **The destructive per-row control is a WORD, and the same word in every tab that has one.**
///
/// It used to be `\u{2715}` — a sensible delete icon that **no face in egui's bundled set carries**, so
/// the whole label drew as the replacement box. In Breakpoints that box landed 36 px right of a real
/// tick-box, which is what an unticked tick-box looks like; a UX seat destroyed a breakpoint with it while
/// trying to *re-enable* one, before it knew the control existed. In Watchpoints the same box was the
/// **first** control in a row with no tick-box at all, so one glyph meant *disable* in one tab and *delete
/// forever* in the next.
///
/// **A word rather than a bundled font, deliberately.** Shipping a face that covers `\u{2715}` would fix
/// the drawing and leave the ambiguity exactly where it was: an icon that reads as "off" in one tab and
/// "gone" in the other is a wrong meaning, not a missing glyph, and no font can correct it. The glyph half
/// is separately guarded by `screen::tests::every_string_literal_the_player_can_show_is_drawable`, which
/// is what makes the *next* icon safe; this constant is what makes this control legible.
pub const REMOVE_LABEL: &str = "remove";

/// The second press, after [`REMOVE_LABEL`] has armed a row.
///
/// **Confirmation and not undo, and the choice is not a shortcut.** Re-adding a cleared breakpoint is one
/// `breakpoint_add` away, but what comes back is a **new** row: a fresh handle, and `hits` reset to zero.
/// A row wearing the old row's clothes while carrying none of its evidence is a believable wrong answer,
/// and this surface's whole rule about `hits` is that it never resets a count. So the destructive gesture
/// is made deliberate instead of reversible, and the irreversibility is stated on the hover rather than
/// papered over.
pub const REMOVE_CONFIRM_LABEL: &str = "confirm remove";

/// Beside [`REMOVE_CONFIRM_LABEL`], so the armed state has a way out that is not "press the other one".
pub const REMOVE_CANCEL_LABEL: &str = "cancel";

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

        // --- ⚑ F-AETHER-BIND-FAILURE-SILENT: the window was asked to serve and could not. ---
        //
        // **On the top bar for the halting alarm's reason, ten lines up, and it is the same defect one
        // instrument over.** The status strip's `aether` row already states all three bus outcomes
        // persistently and has since `PLAYER-SERVE` — but the strip is drawn inside the Registers tab,
        // and `egui_dock` draws only each leaf's active tab, so the statement is behind a tab title. The
        // reported incident is a relaunch whose bind failed: the window played, every command sent to it
        // failed against nothing, and the sole evidence was one launch line that scrolled away.
        //
        // The launch line is **unchanged and still unconditional** (`main` prints `announcement()` with
        // no per-case arm), and so is the strip's row. This adds the standing surface; it replaces
        // neither. `Failed` is the only arm that raises it — see `AetherStatus::alarm` for why a
        // permanent all-clear row is refused here and belongs to the strip.
        let aether = bus.aether_status();
        if let Some(head) = aether.alarm() {
            ui.separator();
            // Error-coloured from the *outcome*, never from the shape of the string — `Halting`'s rule:
            // an alarm that reads its own text back to decide how loud to be is one refactor away from
            // being quiet.
            ui.colored_label(ui.visuals().error_fg_color, &head)
                .on_hover_text(aether.advice().unwrap_or_default());
            drew.push(screen::Run::after_sep(head));
        }

        // --- ⚑ The Aether CONTROL, and it is drawn in every one of the four states. ---
        //
        // **This is the parcel the owner asked for in so many words** — *"there's no 'connect to aether
        // network' button in the new ui"* — and the reason the question could be asked at all is
        // measured: his saved layout parks the status strip's leaf on `Objects`, collapsed, so the
        // `aether` row that has stated all three outcomes since `PLAYER-SERVE` has never once executed
        // for him. `StatusStrip::aether_row`'s own doc argues at length that the row must never hide,
        // *"an absence is not a statement"* — and then placed itself inside a tab, which defeats it. The
        // row is unchanged and still right; this is the surface that cannot be tabbed away, beside the
        // halting alarm that made the identical repair for the identical reason.
        //
        // **A control, not a row** (the standing ruling: things you look at are tabs and rows, things you
        // *do* are controls). That is what answers `AetherStatus::alarm`'s refusal of a permanent
        // all-clear line one door up: the bar has carried `⏸ pause` permanently without anybody calling
        // it noise, because a control is looked *for* when it is wanted rather than read every frame.
        //
        // ⚑ **The serving arm draws a weak label and NOT a button, and the held arm draws no button
        // either.** Every arm draws something, because a control that appeared only when something was
        // wrong renders "this window is serving" and "this build has no Aether control" identically —
        // the absence-is-not-a-statement mistake, one instrument over. What each arm may *do* is
        // `AetherOffer`'s decision and not this call site's; see `AetherStatus::offer` for why a live
        // peer holding the path gets prose instead of a retry that would refuse forever.
        let offer = aether.offer();
        let label = offer.label();
        let hover = format!(
            "{}\n\n{}",
            aether.sentence(),
            match &offer {
                crate::bus::AetherOffer::Serve(p) => format!(
                    "Open the bus now, on {}. That is the path --aether itself would have bound; this \
                     window will not fall back to another one. Tools, lanes and other windows can then \
                     attach to THIS window.",
                    p.display()
                ),
                crate::bus::AetherOffer::Retry(p) => format!(
                    "Try {} again, the same path and no other. Nothing is serving it; clear what the \
                     error above names and press this.",
                    p.display()
                ),
                crate::bus::AetherOffer::Held(_) | crate::bus::AetherOffer::Serving { .. } =>
                    aether.advice().unwrap_or_else(|| String::from(
                        "Anything outside this process can attach here. Nothing to do.",
                    )),
            }
        );
        let mut open_bus = false;
        ui.separator();
        if offer.action_path().is_some() {
            open_bus = ui.button(&label).on_hover_text(&hover).clicked();
        } else {
            // Weak, like `recording` below: a standing fact, read once and then ignored, not an alarm.
            ui.weak(&label).on_hover_text(&hover);
        }
        // Pushed unconditionally, INCLUDING the frame the button is pressed on, on the swap notice's
        // precedent: `crate::screen`'s guarantee is that a run is what the bar DREW, and the pre-click
        // label is what was on the glass when this frame was painted.
        drew.push(screen::Run::after_sep(&label));
        if open_bus {
            // No `issue`: this is not a served method and must not become one. `Host::call` is
            // in-process and reachable with the socket shut (D15), so a `emulator/serve` would be a
            // method whose only caller is a client that by definition cannot reach it yet.
            bus.serve_now();
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
                 discriminant clients branch on, never the message text.",
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
    /// `pub(crate)` as of S3: the `F1` and `F5` machine keys are the same three gestures the bar makes,
    /// issued from the keyboard instead of from a button, and they land in the same [`Echo`] — so the
    /// refusal a running machine gives `emulator/reload_rom` is shown in the same place, in the same
    /// words, by the same code, whichever way it was asked for. A second echo for the keyboard would have
    /// been a second sentence about one server.
    pub(crate) fn issue(&mut self, machine: &mut Machine, bus: &mut Bus, method: &'static str) {
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

    /// A machine and a bus that was **asked to serve and could not** — a real bind failure through the
    /// real path, never a hand-built [`crate::bus::ServeOutcome`].
    ///
    /// The failure is a regular file standing where the socket's parent directory must be, so
    /// `create_dir_all` fails: the same arrangement `bus::tests::a_bind_failure_is_loud_specific_and_not_fatal`
    /// uses, and chosen for the same reason — its `io::Error` is a bare `Not a directory (os error 20)`
    /// with no path in it, so nothing downstream can get the path right by accident.
    fn failing_rig() -> (Machine, Bus, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("oracle-bar-alarm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let blocker = dir.join("not-a-directory");
        std::fs::write(&blocker, b"x").expect("blocker");
        let doomed = blocker.join("s");
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let bus = Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo::default(),
            false,
            Some(Some(doomed.clone())),
        );
        (machine, bus, dir)
    }

    /// ★ **`F-AETHER-BIND-FAILURE-SILENT`: the top bar actually DRAWS the alarm, and a quiet launch draws
    /// nothing in its place.**
    ///
    /// This drives [`Transport::bar`] itself under a headless `egui` ui and reads back the [`screen::Run`]s
    /// the bar **hands over having painted them** — which is why this is a claim about the window and not
    /// about a struct. `bar` returns what it drew rather than being read back (see [`crate::screen`]), so a
    /// run in this vector is a widget that was added to a `Ui`; there is no second expression that could
    /// agree with a draw that never happened.
    ///
    /// It is deliberately NOT a claim that the alarm is *visible or legible on the owner's screen*. Layout,
    /// clipping and font are the toolkit's, and a headless harness answers a different question. That
    /// confirmation is owed and is the owner's to give.
    ///
    /// **Three alternative green paths, ruled out in order:**
    ///
    /// 1. *The bar draws the alarm unconditionally*, so the failing case proves nothing about the
    ///    condition. Ruled out by the quiet half: a `NotAsked` bus must draw no such run.
    /// 2. *The quiet bar draws nothing at all* — a `bar` that returned an empty vector, or a harness that
    ///    never ran the closure, would satisfy the absence in 1 perfectly. Ruled out by the positive
    ///    control: the quiet bar must still draw its `pause` button, so the absence is measured.
    /// 3. *The alarm is drawn but says nothing actionable.* Ruled out by asserting the drawn text carries
    ///    the socket path, which is the field this parcel added to `ServeOutcome::Failed`.
    #[test]
    fn the_top_bar_draws_the_bind_failure_alarm_and_a_quiet_launch_draws_nothing_there() {
        const ALARM: &str = "AETHER NOT SERVING";

        let (mut machine, mut bus, dir) = failing_rig();
        assert!(
            !bus.is_serving(),
            "the fixture must have actually failed to bind, or the alarm below is not the one being \
             tested"
        );
        let path_shown = match bus.aether_status().outcome {
            crate::bus::ServeOutcome::Failed { ref path, .. } => path.display().to_string(),
            ref other => panic!("the fixture must produce a bind FAILURE, got {other:?}"),
        };

        let mut t = Transport::default();
        let mut drew: Vec<screen::Run> = Vec::new();
        egui::__run_test_ui(|ui| {
            drew = t.bar(ui, &mut machine, &mut bus, None);
        });
        let painted = drew
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            painted.contains(ALARM),
            "the top bar drew nothing about a bus it was asked to open and could not — this is the \
             defect, reproduced: {painted}"
        );
        // 3: and it is actionable, not a shrug.
        assert!(
            painted.contains(&path_shown),
            "the bar's alarm must name the socket it could not bind; `bus unavailable` is not something \
             a reader can act on: {painted}"
        );
        let _ = std::fs::remove_dir_all(&dir);

        // --- 1 and 2: the control. A launch nobody asked to serve is not in a false state. ---
        let (mut machine, mut bus) = rig();
        assert!(
            matches!(
                bus.aether_status().outcome,
                crate::bus::ServeOutcome::NotAsked
            ),
            "the control must be the never-asked launch, not a second failure"
        );
        let mut t = Transport::default();
        let mut quiet: Vec<screen::Run> = Vec::new();
        egui::__run_test_ui(|ui| {
            quiet = t.bar(ui, &mut machine, &mut bus, None);
        });
        let quiet_text = quiet
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        // ⚑ The positive control FIRST: an absence measured against a bar that drew nothing is not a
        // measurement. `PAUSE_LABEL` is drawn unconditionally by `bar`'s first widget.
        assert!(
            quiet_text.contains(PAUSE_LABEL),
            "the control bar drew no pause button, so it drew nothing — the assertion below would hold \
             against a harness that never ran the closure: {quiet_text}"
        );
        assert!(
            !quiet_text.contains(ALARM),
            "a window nobody asked to serve is not in a false state, and a permanent banner on the \
             ordinary launch is how readers learn to ignore banners: {quiet_text}"
        );
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
        // `RESET` and `RELOAD_ROM` are the `F1`/`F5` machine keys (S3). They are not buttons, but they
        // land in this bar's `Echo` through the same `Transport::issue`, so a typo in either would
        // produce the same unexplainable `-32601` on the same line.
        for m in [PAUSE, RESUME, STEP, RESET, RELOAD_ROM] {
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
        assert_eq!(row(&rows, "symbol at pc"), "no listing loaded");
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
        // "Every button there is" is the engine's own table, imported rather than copied (lens M69). A copy
        // here stayed at eight names whatever the pad the engine models grew or lost, so when the table
        // moved this control degraded instead of reddening.
        assert_ne!(
            shown,
            oracle_aether::engine::BUTTONS_3.join(", "),
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
        // The row is `port N: a, b [· port M: …] (<remedy>)`; stop at whichever separator comes first.
        // The remedy used to be introduced by ` — `, and P10 replaced it with a parenthesis rather than
        // a colon precisely so this parse stays unambiguous against the row's own `port N: ` colons.
        let end = [" · ", " ("]
            .iter()
            .filter_map(|s| after.find(s))
            .min()
            .unwrap_or(after.len());
        after[..end].to_string()
    }
}

/// **P1's gate.** Nothing a server can put in a value slot can put raw JSON on the screen.
///
/// This module exists because the hazard it covers is invisible to every other test in the crate: the old
/// two-arm [`render`] was safe only for as long as `DecodedRecord::to_json` happened to emit scalars, and
/// a wire change would have made it wrong with the whole suite still green.
///
/// Plain `#[cfg(test)]`, deliberately, unlike `transport_tests` above: this is a statement about a pure
/// string function and there is nothing unix-shaped about it.
#[cfg(test)]
mod json_tests {
    use super::*;
    use serde_json::{json, Value};

    /// The characters JSON uses to build a composite. Derived rather than remembered: `serde_json`'s own
    /// serialisation of a nested value is asked which of them it actually emits, so this list cannot
    /// quietly stop matching the encoder.
    fn structural() -> Vec<char> {
        let encoded = json!({"a": [1, "b"]}).to_string();
        let punctuation: Vec<char> = "{}[]\"".chars().filter(|c| encoded.contains(*c)).collect();
        assert_eq!(
            punctuation.len(),
            5,
            "serde_json no longer emits all five structural characters, so this gate is measuring a \
             smaller set than it thinks: {encoded}"
        );
        punctuation
    }

    /// Every `serde_json::Value` variant, the two composite ones included, rendered without a single
    /// character of JSON punctuation reaching the string a panel draws.
    ///
    /// The composite cases are the ones that matter. Their assertion is deliberately **two-sided**: the
    /// output must carry no punctuation *and* must differ from `Value::to_string()`, because a rendering
    /// that merely stripped braces would pass the first half while still being a dump.
    #[test]
    fn no_served_value_can_put_raw_json_on_the_screen() {
        let punctuation = structural();
        let cases: Vec<Value> = vec![
            Value::Null,
            json!(true),
            json!(false),
            json!(0),
            json!(-42),
            json!(1.5),
            json!("plain"),
            json!([]),
            json!([1, 2, 3]),
            json!({}),
            json!({"pool": {"base": "0x00FF8000"}}),
            json!({"provenance": ["symbol", "scan"], "confidence": 0.9}),
        ];
        for v in &cases {
            // A string is printed as the server meant it, quotes and all if it contains any: it is the
            // payload, not a container. Every other variant is under the punctuation rule.
            if v.is_string() {
                continue;
            }
            let out = render(v);
            for c in &punctuation {
                assert!(
                    !out.contains(*c),
                    "render({v}) put the JSON character {c:?} on the screen: {out:?}"
                );
            }
        }

        // A string is still the payload with its quotes off, which is the whole reason the function has a
        // `String` arm at all.
        assert_eq!(render(&json!("plain")), "plain");

        // A composite says what arrived rather than dumping it, and is not merely `to_string` with the
        // braces filed off.
        let obj = json!({"provenance": ["symbol", "scan"], "confidence": 0.9});
        let rendered = render(&obj);
        assert_ne!(
            rendered,
            obj.to_string(),
            "a nested record is being dumped rather than described"
        );
        assert!(
            rendered.contains('2'),
            "a nested record must say how many keys arrived, got {rendered:?}"
        );
        let arr = json!([1, 2, 3]);
        assert_ne!(render(&arr), arr.to_string());
        assert!(render(&arr).contains('3'));
        // ...and the singular is real, not a stray `s` on everything.
        assert!(render(&json!([7])).contains("1 value in a list"));
        assert!(render(&json!({"k": 1})).contains("1 key in a nested record"));
    }

    /// **P6.** A served `null` is a stated absence, never the token `null` and never a zero.
    ///
    /// Separated from the punctuation walk above because `null` carries no punctuation at all: the
    /// catch-all this gate replaced would have passed that walk on `Value::Null` while printing the four
    /// characters `null` at a person, which is not a fact anybody at this window can act on.
    #[test]
    fn a_served_null_is_a_stated_absence_and_not_the_token_null() {
        let out = render(&Value::Null);
        assert_eq!(out, NO_VALUE);
        assert_ne!(out, Value::Null.to_string());
        assert!(
            !out.split_whitespace().any(|w| w == "null"),
            "the absence is spelled in the wire's word rather than the reader's: {out:?}"
        );
        assert_ne!(out, "0", "an unknown rendered as a zero is a measurement");
    }
}

/// ⚑ **Temporary, and it dies with the block it guards.** See the marked region above [`StatShape`].
///
/// The one property that makes the on-screen comparison worth anything: **both treatments draw the same
/// live numbers.** A block in which treatment B showed placeholder values would render perfectly, read
/// as a working comparison, and settle nothing, which is a failure in the direction that looks like
/// success. So this renders the real thing headlessly and counts what actually reached the screen.
///
/// **What it does if the thing it guards is removed:** delete `stat`'s `Tile` arm, or feed the second
/// row anything other than `stats`, and every count below falls from two to one and the test is red.
/// Delete the whole comparison and it does not compile. The positive control at the top is there for the
/// third failure, the one that would otherwise pass in silence: a collector that finds no text at all.
#[cfg(test)]
mod stat_shape_tests {
    use super::*;
    use std::time::Instant;

    /// Every string that reached the screen, in draw order. `Shape::Vec` nests, so this walks.
    fn drawn(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(s: &egui::epaint::Shape, out: &mut Vec<String>) {
            match s {
                egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_owned()),
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        walk(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut out);
        }
        out
    }

    /// Numbers no other part of the block can produce by accident, so a count of two is a count of the
    /// two treatments rather than of a coincidence.
    fn headline() -> Vec<pacing::Stat> {
        let now = Instant::now();
        let g = Governor::start(now, pacing::FRAME_PERIOD);
        // Presents fed to the meter rather than a `PacingFacts` typed here: the block under test draws
        // the LIVE projection, and a hand-assembled struct would make that claim false.
        let mut p = pacing::Presents::start(now);
        for i in 1..=3u32 {
            p.note(now + pacing::FRAME_PERIOD * i);
        }
        let facts = p.facts(
            now + pacing::FRAME_PERIOD * 4,
            &g,
            oracle_aether::engine::PacingAudio::Unmeasured,
        );
        pacing::Readout::of(987_654, &facts, &g, None, "").headline
    }

    #[test]
    fn both_treatments_draw_the_same_live_numbers() {
        let stats = headline();
        assert_eq!(stats.len(), 4, "the exemplar's four headline numbers");

        let ctx = egui::Context::default();
        crate::theme::install(&ctx, crate::theme::DEFAULT_FAMILY);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(900.0, 700.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| headline_comparison(ui, &stats));
        let texts = drawn(&out.shapes);
        out.textures_delta.clear();

        // Positive control: the walk found the block at all. Without this every assertion below passes
        // vacuously on an empty vector, which is exactly the shape of gate that cannot fail.
        assert!(
            !texts.is_empty(),
            "nothing was drawn, so nothing below is a measurement"
        );
        for name in [TREATMENT_A, TREATMENT_B] {
            assert_eq!(
                texts.iter().filter(|t| t.as_str() == name).count(),
                1,
                "each treatment is labelled exactly once, or a reader cannot tell which is which. \
                 Drawn: {texts:?}"
            );
        }

        // The property. Every headline number and every headline label reaches the screen twice: once
        // per treatment, from one projection.
        for s in &stats {
            assert_eq!(
                texts.iter().filter(|t| t.as_str() == s.value).count(),
                2,
                "{:?} was drawn a different number of times than twice, so the two treatments are not \
                 showing the same live number. Drawn: {texts:?}",
                s.value
            );
            assert_eq!(
                texts.iter().filter(|t| t.as_str() == s.label).count(),
                2,
                "the label {:?} does not appear under both treatments. Drawn: {texts:?}",
                s.label
            );
        }
    }

    /// **P3 and P10 over the strings this temporary block adds.** The exemplar's own gates walk
    /// `pacing::Readout`; these three strings are composed in the render and no existing gate can see
    /// them, which is precisely how a temporary block accretes a rule violation nobody sweeps.
    #[test]
    fn the_comparison_blocks_own_strings_keep_the_panel_rules() {
        for s in [HEADLINE_CHOICE, TREATMENT_A, TREATMENT_B] {
            for bad in ['\u{2014}', '\u{2013}'] {
                assert!(
                    !s.contains(bad),
                    "P10: {bad:?} in user-facing text, which the owner's 2026-09-05 ruling bars: {s:?}"
                );
            }
            assert!(
                !s.contains("  ") && !s.contains('\t'),
                "P2: a run of spaces or a tab is a column drawn inside a string: {s:?}"
            );
        }
    }
}

/// **The Planes tab's layout, driven headless.**
///
/// The defect these gates exist for shipped and was visible: planes A and B drew no picture at all, while
/// the window plane drew one. The cause was [`plane_split`] using `allocate_ui`, which inherits the
/// caller's layout; the caller is `horizontal_top`, and egui does not wrap text in a non-wrapping
/// horizontal layout, so the side column's `unestablished` paragraph ran off to the right and consumed the
/// whole row. The window plane looked fine because [`crate::planes::scroll_note`] returns no caveat for it.
///
/// Nothing here is a stand-in: the note is the **real** `scroll_note` output for a plane with an
/// H-interrupt armed, and the column is the **real** [`plane_side_column`]. Only [`Panels::plane_picture`]
/// is replaced, by a closure that records the space the split handed it, because that space is precisely
/// the input `plane_picture` was failing on.
#[cfg(test)]
mod planes_layout_tests {
    use super::*;
    use oracle_core::render::Plane;

    /// A window wide enough for the side-by-side branch, and taller than the column needs.
    const SCREEN: egui::Vec2 = egui::vec2(1200.0, 800.0);

    /// A gather carrying only what [`crate::planes::scroll_note`] reads. The vectors are empty because it
    /// reads none of them, and leaving them empty is what keeps this a test of the layout rather than a
    /// second, hand-written copy of the gather.
    fn inputs(plane: Plane, hint_line: Option<u8>) -> crate::planes::Inputs {
        crate::planes::Inputs {
            plane,
            scrolled: false,
            base: 0xC000,
            cols: 64,
            rows: 32,
            display: (320, 224),
            cells: Vec::new(),
            scroll: Vec::new(),
            spans: Vec::new(),
            hmode: 3,
            vcolumns: false,
            htable: 0xFC00,
            hint_line,
            fingerprint: 0,
        }
    }

    fn facts() -> Vec<objects::Fact> {
        [
            ("nametable at", "$C000"),
            ("map", "64 by 32 cells"),
            ("plane", "512 by 256 pixels"),
            ("drawn", "512 by 256 pixels"),
            ("scroll table at", "$FC00"),
            ("rasterised", "3 times in 9 repaints"),
        ]
        .into_iter()
        .map(|(label, value)| objects::Fact {
            label: label.into(),
            value: value.into(),
            mono: false,
        })
        .collect()
    }

    /// What the split left for each half. `picture` is `ui.available_size()` at the moment
    /// [`Panels::plane_picture`] would have read it; `side` is the width the column actually occupied.
    struct Room {
        picture: egui::Vec2,
        side: f32,
        /// The gap egui itself puts between two things in a row, read off the styled context rather than
        /// remembered, so the accounting below cannot drift when the theme changes its spacing.
        item_spacing: f32,
    }

    /// Lay the split out at `screen` and report both halves.
    ///
    /// Two frames, and the second is the answer: `fact_grid` is an `egui::Grid`, whose column widths come
    /// from state stored on the previous frame, so a one-frame reading would be measuring a grid that has
    /// not settled rather than the layout under test.
    fn lay_out(screen: egui::Vec2, note: &crate::planes::ScrollNote) -> Room {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, crate::theme::DEFAULT_FAMILY);
        let facts = facts();
        let (mut picture, mut side, mut item_spacing) = (None, None, None);
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), screen)),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                item_spacing = Some(ui.spacing().item_spacing.x);
                plane_split(
                    ui,
                    |ui| {
                        plane_side_column(ui, None, &facts, note, false);
                        side = Some(ui.min_rect().width());
                    },
                    |ui| picture = Some(ui.available_size()),
                );
            });
            // The context is never painted, so a delta nobody consumes would otherwise be leaked.
            out.textures_delta.clear();
        }
        Room {
            picture: picture.expect("the split ran its picture half"),
            side: side.expect("the split ran its side half"),
            item_spacing: item_spacing.expect("the closure ran"),
        }
    }

    /// The plane the owner was looking at: an H-interrupt armed, so the caveat paragraph is present.
    fn armed_note() -> crate::planes::ScrollNote {
        let note = crate::planes::scroll_note(&inputs(Plane::A, Some(0)));
        // Positive control. Without this the whole gate passes vacuously the day `scroll_note` stops
        // emitting a caveat, which is the one condition it is here to survive.
        assert!(
            note.unestablished.is_some(),
            "this gate measures the layout under the long caveat, and there is no caveat to measure"
        );
        note
    }

    #[test]
    fn the_caveat_paragraph_leaves_the_picture_room_to_draw() {
        let room = lay_out(SCREEN, &armed_note());

        // ⚑ **The defect, in the exact terms the panel failed in.** Not a width compared against a number
        // read off a run: the space the split handed over is put through the same `screen_pick::fit` the
        // picture puts it through, with the plane's own 512x256 raster, and asked whether anything comes
        // back. Under the bug this returned `Vec2::ZERO` and `plane_picture` returned, drawing nothing.
        let size = screen_pick::fit(
            room.picture,
            512,
            256,
            1.0,
            oracle_frontend::present::Aspect::Square,
        );
        assert!(
            size.x > 0.0 && size.y > 0.0,
            "the picture was left {:?}, which fits to {size:?}, so `plane_picture` draws nothing. The \
             side column took {} of the {} available.",
            room.picture,
            room.side,
            SCREEN.x,
        );

        // **The cause**, stated against the budget the split declares rather than against a measurement:
        // the column was allocated `PLANE_SIDE_W` and has to wrap inside it. A column laid out
        // horizontally does not wrap, and this is what runs away when it does not.
        assert!(
            room.side <= PLANE_SIDE_W,
            "the side column occupied {} points against a {PLANE_SIDE_W}-point budget, so its text is \
             not wrapping",
            room.side,
        );
    }

    /// The window plane is the control that shipped: it drew correctly under the bug, because its note is
    /// two short lines and no caveat. It has to keep working, and it must not be the only one that does.
    ///
    /// **The two are deliberately not asserted equal.** A first draft of this gate did assert that, and it
    /// failed on the fixed code at 981.8 against 930: `allocate_ui_with_layout` allocates the space the
    /// column *used*, so a short note legitimately gives some of its 260 back. The premise was wrong, not
    /// the fix. What actually holds is an accounting identity, and it is the stronger statement anyway:
    /// the row is the side column, the gaps, and the picture, and **nothing else** may take width from it.
    /// Under the bug the column alone took the row and this failed by hundreds of points.
    #[test]
    fn no_note_takes_more_of_the_row_than_the_side_column_is_worth() {
        let window = crate::planes::scroll_note(&inputs(Plane::Window, Some(0)));
        assert!(
            window.unestablished.is_none(),
            "the window note is the short case; if it grew a caveat this control means something else"
        );
        for (what, note) in [("window", window), ("caveat", armed_note())] {
            let room = lay_out(SCREEN, &note);
            assert!(
                room.side <= PLANE_SIDE_W,
                "the {what} column occupied {} points against a {PLANE_SIDE_W}-point budget",
                room.side,
            );
            // Everything in the row is accounted for: what the column took, the explicit `SECTION_GAP`,
            // egui's own spacing on either side of it, and the rest, which is the picture's.
            let accounted = room.side + SECTION_GAP + 2.0 * room.item_spacing + room.picture.x;
            assert!(
                accounted >= SCREEN.x,
                "the {what} row accounts for only {accounted} of {}: side {}, gap {SECTION_GAP}, \
                 spacing {}, picture {}. Width went somewhere the split does not name.",
                SCREEN.x,
                room.side,
                room.item_spacing,
                room.picture.x,
            );
        }
    }

    /// The narrow branch stacks instead, and the picture is above the facts rather than beside them. It
    /// gets the full width there, so the caveat cannot squeeze it either way.
    #[test]
    fn the_narrow_branch_gives_the_picture_the_whole_width() {
        let narrow = egui::vec2(PLANE_SIDE_BY_SIDE_MIN - 60.0, 800.0);
        let room = lay_out(narrow, &armed_note());
        assert!(
            room.picture.x > narrow.x - 40.0,
            "the stacked branch handed the picture {:?} of a {}-point pane",
            room.picture,
            narrow.x,
        );
    }

    /// [`NO_ROOM_FOR_PICTURE`] is shipped text and lives under the same rules as the rest of the panel.
    #[test]
    fn the_no_room_line_keeps_the_panel_text_rules() {
        for bad in ['\u{2014}', '\u{2013}'] {
            assert!(
                !NO_ROOM_FOR_PICTURE.contains(bad),
                "P10: {bad:?} in user-facing text: {NO_ROOM_FOR_PICTURE:?}"
            );
        }
        assert!(
            !NO_ROOM_FOR_PICTURE.contains("  ") && !NO_ROOM_FOR_PICTURE.contains('\t'),
            "P2: a run of spaces or a tab is a column drawn inside a string"
        );
    }
}

/// **Nothing drawn over the picture may cost the picture a pixel of layout.**
///
/// The owner's second finding, 2026-09-09: *"whenever I place something this text box comes up and shifts
/// the window for the game."* The readout was a card **above** the picture in the same vertical stack, so
/// the picture moved down by the height of whatever sentence the last click produced.
///
/// This is the property that fix rests on, and it is checkable without a window because `egui` will build
/// a `Ui` on a headless `Context` — the same harness `crate::input`'s key tests already use. The two
/// overlays are run against a real `Ui` and the `Ui`'s own layout state is compared before and after: a
/// helper that reached for `ui.label` instead of `Painter` would move the cursor, and that is exactly the
/// regression.
#[cfg(test)]
mod overlay_layout_tests {
    use super::*;

    fn readout() -> screen_pick::Readout {
        screen_pick::Readout {
            head: "a long enough sentence that laying it out would visibly move whatever came after it"
                .into(),
            detail: Some("vram 0x0000C123..0x0000C124".into()),
            outcome: Some("2 watches armed by this click".into()),
            refused: false,
        }
    }

    #[test]
    fn an_overlay_on_the_picture_takes_no_layout_space() {
        let ctx = egui::Context::default();
        let mut before = (egui::Rect::NOTHING, egui::Rect::NOTHING);
        let mut after = (egui::Rect::NOTHING, egui::Rect::NOTHING);
        let r = readout();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            // A picture rect the caller allocated for the image, exactly as `Panels::screen` does.
            let picture = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(320.0, 224.0));
            before = (ui.min_rect(), ui.cursor());
            readout_overlay(ui, picture, &r);
            armed_frame(
                ui,
                picture,
                "SPAWN: ObjDef_Spring. Press Esc to leave the mode.",
            );
            after = (ui.min_rect(), ui.cursor());
        });
        // `FullOutput` panics on drop with unapplied texture deltas; no backend applies them here.
        out.textures_delta.clear();
        assert_eq!(
            before.0, after.0,
            "the overlays grew the Ui's minimum rect, so whatever is laid out next moves"
        );
        assert_eq!(
            before.1, after.1,
            "the overlays advanced the layout cursor, which is how the picture got shifted"
        );
    }

    /// The control for the assertion above: the **card** form, which is what the overlay replaced, does
    /// move the cursor. Without this leg a broken headless `Ui` would make the test above pass by
    /// measuring nothing at all.
    #[test]
    fn the_card_form_the_overlay_replaced_does_move_the_layout() {
        let ctx = egui::Context::default();
        let mut before = egui::Rect::NOTHING;
        let mut after = egui::Rect::NOTHING;
        let r = readout();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            before = ui.cursor();
            readout_card(ui, &r);
            after = ui.cursor();
        });
        out.textures_delta.clear();
        assert_ne!(
            before, after,
            "the control witnesses nothing: this Ui does not move its cursor for a laid-out card \
             either, so the overlay test above is measuring an inert harness"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ The height bound, which is about producers that do not exist yet
    // ---------------------------------------------------------------------------------------------

    /// **The budget arithmetic**, at its own stated precondition rather than at a convenient number.
    ///
    /// `overlay_min_bounded_height` is where the `max(lines)` exception stops applying, so the fraction
    /// is asserted from there upward. A height picked because it passed would leave the boundary
    /// untested, which is where the exception and the rule meet.
    #[test]
    fn a_block_is_never_taller_than_its_share_of_the_picture_once_the_picture_can_afford_one_row_each(
    ) {
        let row_h = 14.0_f32;
        for lines in 1..=4_usize {
            let floor = overlay_min_bounded_height(row_h, lines);
            for h in [floor, floor + 1.0, 240.0, 480.0, 1080.0, 4000.0] {
                if h < floor {
                    continue;
                }
                let budget = overlay_row_budget(h, row_h, lines);
                assert!(
                    budget >= lines,
                    "every line owes at least one row or a refusal can go unpainted: {lines} lines, \
                     {h} tall, budget {budget}"
                );
                assert!(
                    budget <= OVERLAY_MAX_ROWS,
                    "the absolute cap is what binds at the sizes he plays at: {budget}"
                );
                let gaps = OVERLAY_PAD * 2.0 + OVERLAY_PAD * 0.5 * (lines - 1) as f32;
                let painted = budget as f32 * row_h + gaps;
                assert!(
                    painted <= h * OVERLAY_MAX_FRACTION + 0.001,
                    "a block {painted} tall on a {h} picture is more than the {OVERLAY_MAX_FRACTION} \
                     share the owner's 2026-09-10 finding bounds it to"
                );
            }
        }
    }

    /// **Below the precondition the exception fires, and it fires the safe way.**
    ///
    /// The control for the test above: on a picture too short to afford a row per line the budget goes
    /// over the fraction rather than under the line count, because dropping a line is how a verbosity
    /// bound turns into a swallowed refusal.
    #[test]
    fn a_picture_too_short_for_the_bound_keeps_every_line_rather_than_keeping_the_fraction() {
        let (row_h, lines) = (14.0_f32, 3_usize);
        let budget = overlay_row_budget(20.0, row_h, lines);
        assert_eq!(
            budget, lines,
            "one row each and not one more, so the overshoot is the smallest one that keeps the lines"
        );
        assert!(
            budget as f32 * row_h > 20.0 * OVERLAY_MAX_FRACTION,
            "the control witnesses nothing unless this really is the case where the fraction loses"
        );
    }

    /// **The surplus is spent on measured demand, and no line can be starved by the one above it.**
    #[test]
    fn the_row_budget_goes_to_the_lines_that_want_it_and_never_leaves_one_unpainted() {
        // A head that would eat everything. The two lines under it still get their row.
        let got = overlay_spend_rows(&[40, 2, 1], 5);
        assert_eq!(
            got,
            vec![3, 1, 1],
            "the head takes the surplus, not the floor"
        );
        assert_eq!(
            got.iter().sum::<usize>(),
            5,
            "and it spends the budget, all of it"
        );

        // A short head does NOT hold rows the addressing under it needs.
        assert_eq!(
            overlay_spend_rows(&[1, 3, 1], 5),
            vec![1, 3, 1],
            "demand is measured per line, so a one-row head cannot hoard the block"
        );

        // Nothing to truncate: everybody gets what they asked for and the block is short.
        assert_eq!(overlay_spend_rows(&[2, 1, 1], 5), vec![2, 1, 1]);

        // The degenerate budget still paints every line.
        assert_eq!(
            overlay_spend_rows(&[9, 9, 9], 0),
            vec![1, 1, 1],
            "a budget of nothing still owes every line the row that proves it exists"
        );
    }

    /// ⚑ **The property in the owner's own terms, through the real painter: a producer may hand this any
    /// string at all and it still cannot cover his screen.**
    ///
    /// The budget tests above are arithmetic. This one runs the actual `overlay_block` on a headless
    /// `Context` with a deliberately absurd sentence, and measures the rect it says it covered. It is the
    /// leg that would catch the bound being computed correctly and then not applied.
    #[test]
    fn a_producer_that_does_not_exist_yet_still_cannot_cover_a_fifth_of_the_picture() {
        let ctx = egui::Context::default();
        let picture_h = 448.0_f32;
        let mut covered = egui::Rect::NOTHING;
        let mut picture = egui::Rect::NOTHING;
        let wall = "the standing statement, the whole design rationale, and a paragraph about the \
                    engine, repeated until it would fill the window. "
            .repeat(12);
        let r = screen_pick::Readout {
            head: wall.clone(),
            detail: Some(wall.clone()),
            outcome: Some(wall),
            refused: false,
        };
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            picture = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(640.0, picture_h));
            // The control: laid out unbounded, this really would be a wall. Measured through the same
            // painter, so a harness that lays nothing out cannot make the assertion below vacuous.
            let free = ui
                .painter()
                .layout(r.head.clone(), egui::FontId::proportional(13.0), egui::Color32::WHITE, 600.0)
                .size()
                .y;
            assert!(
                free > picture_h * OVERLAY_MAX_FRACTION,
                "the fixture is not a wall in this harness, so nothing below is being bounded: {free}"
            );
            covered = readout_overlay(ui, picture, &r);
        });
        out.textures_delta.clear();
        assert!(
            covered.height() <= picture_h * OVERLAY_MAX_FRACTION + 0.001,
            "an overlay covering {} of a {picture_h} picture is the wall he asked us to bound",
            covered.height()
        );
        assert!(
            picture.contains_rect(covered),
            "and it stays inside the picture rather than escaping onto the letterbox"
        );
    }

    /// **The trimmed ring readout does not hit the cap**, which is what makes the cap a backstop rather
    /// than a censor.
    ///
    /// It matters because there is no on-demand surface for the remainder yet (a scroll region would
    /// reintroduce the layout cost of the owner's 2026-09-09 finding). While every producer fits, the
    /// ellipsis never fires and nothing is lost; the day one does not, this test is what says so.
    #[test]
    fn todays_ring_readout_fits_the_bound_with_room_to_spare() {
        let ctx = egui::Context::default();
        let picture_h = 448.0_f32;
        let mut covered = egui::Rect::NOTHING;
        let r = screen_pick::Readout {
            head: oracle_frontend::rings::Placed {
                world: (2048, 512),
                section_id: 3,
                list_index: 8,
                real_rings: 7,
                slot: 9,
                addr: 0x00FF_AF3C,
                buffer_used: 9,
                buffer_max: 128,
            }
            .terminal(),
            detail: None,
            outcome: Some("the machine was paused for this click and put back".into()),
            refused: false,
        };
        let mut unbounded = 0.0_f32;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let picture = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(640.0, picture_h));
            // What the same lines take with no budget at all, through the same painter and the same
            // wrap. If the bounded block is this tall, the cap did not bite and nothing was elided.
            let wrap = (picture.width() - OVERLAY_PAD * 4.0).max(40.0);
            let faces = [egui::TextStyle::Body, egui::TextStyle::Small];
            let texts = [r.head.clone(), r.outcome.clone().unwrap()];
            let mut sum = OVERLAY_PAD * 2.0 + OVERLAY_PAD * 0.5;
            for (t, s) in texts.iter().zip(faces) {
                let face = ui
                    .style()
                    .text_styles
                    .get(&s)
                    .cloned()
                    .unwrap_or_else(|| egui::FontId::proportional(13.0));
                sum += ui
                    .painter()
                    .layout(t.clone(), face, egui::Color32::WHITE, wrap)
                    .size()
                    .y;
            }
            unbounded = sum;
            covered = readout_overlay(ui, picture, &r);
        });
        out.textures_delta.clear();
        assert!(
            covered.height() <= picture_h * OVERLAY_MAX_FRACTION,
            "the trimmed readout is inside the bound: {} vs {}",
            covered.height(),
            picture_h * OVERLAY_MAX_FRACTION
        );
        assert!(
            (covered.height() - unbounded).abs() < 0.5,
            "the cap BIT on today's own readout, which means the ellipsis is firing and a person is \
             reading a cut sentence with no way to see the rest: bounded {} vs unbounded {}",
            covered.height(),
            unbounded
        );
    }
}

/// **A pane too small for the picture says so** (`F-SCREEN-PICTURE-SILENT`).
///
/// [`Panels::screen`] used to read [`screen_pick::fit`] straight and `return` on a zero result: no
/// picture, no sentence, and the standing readout of the last click gone with it. The pane was then
/// indistinguishable from a broken one, which is the failure `plane_picture` records one tab over, and the
/// loud-on-unmeasurable rule: *no picture rather than a black rectangle presented as one*. (⚑ This cited
/// `machine.rs` as stating that rule until lens M42 removed the unreachable guard it was stated on.)
///
/// **What is asserted here and what is not.** [`screen_room`] and [`no_picture`] are the decision and the
/// drawing, and both are checkable on a headless `egui::Context` the way `overlay_layout_tests` already
/// is. What no test here reaches is [`Panels::screen`] itself wiring the two together: `Panels` borrows a
/// live `Machine` and `Bus`, so there is no headless value to build one from. The `Result` is what stands
/// in for that leg — the give-up carries the sentence with it, so a caller that handles it at all cannot
/// handle it silently.
#[cfg(test)]
mod screen_room_tests {
    use super::*;

    /// The native frame, which is what the Screen tab's texture is.
    const SRC: (usize, usize) = (320, 224);

    /// The panel's shipped default fit, read rather than restated: pinning `Aspect::Tv` here would be a
    /// second spelling of a choice `screen_pick::Panel` already makes.
    fn aspect() -> oracle_frontend::present::Aspect {
        screen_pick::Panel::default().aspect
    }

    /// **Derived from the condition, not from a run.** `screen_pick::fit` floors `avail * ppp` into whole
    /// device pixels and gives up when that is zero, so the collapse is exactly "less than one device
    /// pixel" and this is half of one.
    fn under_one_device_pixel(ppp: f32) -> f32 {
        0.5 / ppp
    }

    /// Room enough that nothing about the fit is marginal, in the same units.
    fn plenty(ppp: f32) -> f32 {
        400.0 / ppp
    }

    /// The scales the panel is actually drawn at. **1.0 is not enough on its own**: the owner's display is
    /// not at 1.0, and a threshold expressed in device pixels is exactly where a points-only harness would
    /// agree with a broken build.
    const SCALES: [f32; 3] = [1.0, 1.25, 2.0];

    #[test]
    fn a_pane_with_no_room_answers_with_the_sentence_rather_than_a_zero() {
        for ppp in SCALES {
            for (axis, avail) in [
                (
                    "width",
                    egui::vec2(under_one_device_pixel(ppp), plenty(ppp)),
                ),
                (
                    "height",
                    egui::vec2(plenty(ppp), under_one_device_pixel(ppp)),
                ),
                ("both", egui::Vec2::ZERO),
                ("not a number", egui::vec2(f32::NAN, plenty(ppp))),
            ] {
                assert_eq!(
                    screen_room(avail, SRC.0, SRC.1, ppp, aspect()),
                    Err(NO_ROOM_FOR_SCREEN),
                    "a pane collapsed on {axis} at {ppp} points per pixel ({avail:?}) gave up without \
                     naming a reason, which is how this shipped: an empty pane and no statement"
                );
            }
        }
    }

    /// The control. Without it the assertion above passes on a `screen_room` that refuses everything, and
    /// a Screen tab that never draws a picture is a worse defect than the one being fixed.
    #[test]
    fn a_pane_with_room_is_not_refused() {
        for ppp in SCALES {
            let avail = egui::vec2(plenty(ppp), plenty(ppp) * 0.75);
            let size = screen_room(avail, SRC.0, SRC.1, ppp, aspect()).unwrap_or_else(|why| {
                panic!("an ordinary {avail:?} pane at {ppp} was refused: {why}")
            });
            assert!(
                size.x > 0.0 && size.y > 0.0,
                "the accepted branch handed back {size:?}, which draws nothing either"
            );
        }
    }

    /// **A narrow pane is not the collapse**, and this is the fact the fix rests on: `present::dest_rect`
    /// floors its scale at one whole aspect unit, so however little width there is the picture still has a
    /// size. The give-up is a pane with no room at all, which is why the sentence advises making the pane
    /// larger rather than wider.
    #[test]
    fn a_merely_narrow_pane_still_gets_a_picture() {
        for ppp in SCALES {
            for w in [4.0, 12.0, 40.0] {
                let avail = egui::vec2(w / ppp, plenty(ppp));
                assert!(
                    screen_room(avail, SRC.0, SRC.1, ppp, aspect()).is_ok(),
                    "{w} device pixels of width was treated as no room at {ppp}; the give-up has moved \
                     and the sentence's advice is now wrong"
                );
            }
        }
    }

    fn readout() -> screen_pick::Readout {
        screen_pick::Readout {
            head: "clicked dot 100, 50".into(),
            detail: Some("vram 0x0000C123".into()),
            outcome: None,
            refused: false,
        }
    }

    /// Every string [`no_picture`] painted, through the same flattening the subtype list's gates use.
    fn painted(why: &str, r: Option<&screen_pick::Readout>) -> Vec<String> {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 300.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| no_picture(ui, why, r));
        out.textures_delta.clear();
        super::subtype_list_tests::text_runs(&out.shapes)
            .into_iter()
            .map(|(_, t)| t)
            .collect()
    }

    /// **The branch draws.** This is the assertion the shipped code failed: it painted nothing at all.
    #[test]
    fn the_no_room_branch_paints_its_sentence_and_keeps_the_readout() {
        let r = readout();
        let runs = painted(NO_ROOM_FOR_SCREEN, Some(&r));
        assert!(
            runs.iter().any(|t| t == NO_ROOM_FOR_SCREEN),
            "the no-room branch painted {runs:?}, which does not contain \
             {NO_ROOM_FOR_SCREEN:?}: the pane is empty and silent"
        );
        assert!(
            runs.iter().any(|t| t.contains(&r.head)),
            "the standing readout of the last click went missing with the picture: {runs:?}"
        );
    }

    /// The other branch through the same helper, so a change that silenced one would have to silence both
    /// to pass, and so the harness above is witnessed against text that shipped long before this fix.
    #[test]
    fn the_no_frame_branch_still_paints_its_own_sentence() {
        let runs = painted("no frame yet", None);
        assert_eq!(
            runs,
            vec!["no frame yet".to_string()],
            "the no-frame branch painted something other than its one line"
        );
    }

    /// [`NO_ROOM_FOR_SCREEN`] is shipped text, under the same rules as [`NO_ROOM_FOR_PICTURE`]'s gate.
    #[test]
    fn the_no_room_line_keeps_the_panel_text_rules() {
        for bad in ['\u{2014}', '\u{2013}'] {
            assert!(
                !NO_ROOM_FOR_SCREEN.contains(bad),
                "P10: {bad:?} in user-facing text: {NO_ROOM_FOR_SCREEN:?}"
            );
        }
        assert!(
            !NO_ROOM_FOR_SCREEN.contains("  ") && !NO_ROOM_FOR_SCREEN.contains('\t'),
            "P2: a run of spaces or a tab is a column drawn inside a string"
        );
    }
}

/// **The control strip cannot eat the picture** (`F-SCREEN-TAB-STRIP-UNBOUNDED`).
///
/// The owner's symptom was a Screen tab with no picture in it. The strip above the picture is a column of
/// up to nine rows, six of them conditional, and nothing bounded it; [`Panels::screen`] takes
/// `ui.available_size()`, so the strip took its natural height and the picture absorbed the whole
/// deficit, down to zero.
///
/// **The row's stated cure was "make that tab scroll", and
/// [`the_tab_bodys_own_scroll_area_was_there_all_along`] is why it is not this fix.** The tab is already
/// inside a scroll area — `egui_dock` puts every body in one — and that gate lays the *unfixed*
/// arrangement out inside it and shows both halves at once: the tab offers to scroll, and the picture is
/// still nothing. A scroll area moves content that overflows; an elastic picture never overflows,
/// it collapses.
///
/// **Everything here is laid out inside the wrapper the window really uses**, so the pane measured is the
/// pane the tab gets. What is not the real thing is the strip's *content*: [`Panels::screen_controls`]
/// needs a live `Machine` and `Bus`, exactly as [`screen_room_tests`] records, so the stand-in is a
/// column of rows sized off the style. That makes this a gate on the **bound**, which is what changed;
/// the strip's own rows are unchanged by this parcel and are not what these tests are about.
#[cfg(test)]
mod screen_strip_tests {
    use super::*;

    /// The native frame, which is what the Screen tab's texture is.
    const SRC: (usize, usize) = (320, 224);

    /// The scales the panel is actually drawn at. **1.0 is not enough on its own**: the bound is spent in
    /// points but consumed in device pixels, the owner's display is not at 1.0, and a points-only harness
    /// is exactly where it would agree with a broken build. Same list, same reason, as
    /// [`screen_room_tests`].
    const SCALES: [f32; 3] = [1.0, 1.25, 2.0];

    /// Pane heights from comfortable down to absurd. The tall ones are the no-regression cases (the cap
    /// is far above anything the strip wants); the short ones are the defect.
    const PANE_HEIGHTS: [f32; 5] = [800.0, 400.0, 240.0, 160.0, 90.0];

    /// A pane wide enough that width is never the constraint — the predecessor proved a narrow pane still
    /// gets a picture, so width is not the axis under test here.
    const PANE_W: f32 = 520.0;

    /// One point of slack for the pixel grid: egui rounds a scroll area's content rect to whole device
    /// pixels, which at 1.25 points-per-pixel moves an edge by a fraction of a point.
    const EPS: f32 = 1.0;

    /// The panel's shipped default fit, read rather than restated.
    fn aspect() -> oracle_frontend::present::Aspect {
        screen_pick::Panel::default().aspect
    }

    /// What one lay-out of the Screen tab's body left behind.
    struct Room {
        /// What [`screen_strip`] budgeted and what it spent.
        strip: StripRoom,
        /// `ui.available_size()` at the moment [`Panels::screen`] would read it.
        picture: egui::Vec2,
        /// The height the tab body actually offered, inside `egui_dock`'s wrapper.
        pane: f32,
        /// The tab body's own scroll area: what its content wanted, and the viewport it was given.
        tab: (f32, f32),
        /// How far the tab body's own scroll area had moved after a wheel was turned over the pane.
        tab_offset: f32,
    }

    impl Room {
        /// Whether the **tab's** scroll area had anything to scroll — `content_size` past `inner_rect` is
        /// egui's own `content_is_too_large` test.
        /// Whether the **tab** actually scrolled when a wheel was turned over it.
        ///
        /// ⚑ **Measured by turning a wheel, because the obvious measure is not one.** This began as
        /// `content_size > inner_rect`, and a mutation that turned the wrapper's scrolling *off*
        /// (`ScrollArea::new([false, false])`) left it green: `ScrollAreaOutput::inner_rect` is the
        /// pre-shrink viewport, so content past the viewport reads the same whether that content can be
        /// reached or is merely clipped. `content_is_too_large` and `show_scroll`, which would say, are
        /// private to egui. `State::offset` is not — so the harness sends a real wheel event over the
        /// pane and asks whether the content moved.
        fn tab_scrolled(&self) -> bool {
            self.tab_offset > 0.0
        }
    }

    /// Lay a Screen-tab body out headless **inside the wrapper `egui_dock` really puts it in**.
    ///
    /// `egui_dock-0.21.1/src/widgets/dock_area/show/leaf.rs:1390` shows every tab body inside
    /// `ScrollArea::new(tab_viewer.scroll_bars(tab))`; `TabViewer::scroll_bars` defaults to `[true, true]`
    /// (`.../src/widgets/tab_viewer.rs:106`) and [`Panels`] does not override it. A harness without that
    /// wrapper would be measuring a pane this window does not have — and would have no way to state the
    /// finding that the wrapper is already there.
    ///
    /// Three frames. The first two are [`planes_layout_tests`]'s reason — egui settles some sizes off
    /// the previous frame's state and a scroll area is one of the things that does — and the third turns
    /// a **wheel** over the bottom of the pane, which is the only way from outside egui to ask whether
    /// the tab body's own scroll area can actually be scrolled. See [`Room::tab_scrolled`].
    ///
    /// The pointer goes to the bottom edge on purpose: that is over the picture, not over the strip, so
    /// the wheel reaches the tab's scroll area rather than the strip's own.
    fn lay_out(pane_h: f32, ppp: f32, strip: impl Fn(&mut egui::Ui) + Copy, bound: bool) -> Room {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, crate::theme::DEFAULT_FAMILY);
        let (mut room, mut picture, mut pane, mut tab) = (None, None, None, None);
        let mut tab_offset = None;
        for frame in 0..3 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(PANE_W, pane_h),
                )),
                ..Default::default()
            };
            let id = raw.viewport_id;
            raw.viewports
                .get_mut(&id)
                .expect("egui's own RawInput::default carries the root viewport")
                .native_pixels_per_point = Some(ppp);
            if frame == 2 {
                let at = egui::pos2(PANE_W / 2.0, pane_h - 4.0);
                raw.events.push(egui::Event::PointerMoved(at));
                raw.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    // Down the page, and further than any pane here is tall, so a wrapper that can
                    // scroll at all ends up with a non-zero offset rather than a rounding artefact.
                    delta: egui::vec2(0.0, -4.0 * pane_h),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::default(),
                });
            }
            let mut out = ctx.run_ui(raw, |ui| {
                let outer = egui::ScrollArea::new([true, true]).show(ui, |ui| {
                    // `leaf.rs` expands the body to the pane before handing it to the viewer.
                    let body = ui.available_rect_before_wrap();
                    ui.expand_to_include_rect(body);
                    pane = Some(ui.available_height());
                    room = Some(if bound {
                        screen_strip(ui, strip)
                    } else {
                        // ⚑ **The arrangement that shipped**, for the gate that proves the tab's own
                        // scroll area cannot reach it: the strip straight into the stack, then the rule,
                        // then whatever is left.
                        let before = ui.available_height();
                        strip(ui);
                        // The same measure [`screen_strip`] reports, so the two arms are comparable.
                        let taken = before - ui.available_height() - ui.spacing().item_spacing.y;
                        ui.separator();
                        StripRoom {
                            cap: f32::INFINITY,
                            shown: taken,
                            wanted: taken,
                        }
                    });
                    picture = Some(ui.available_size());
                });
                tab = Some((outer.content_size.y, outer.inner_rect.height()));
                tab_offset = Some(outer.state.offset.y);
            });
            // The context is never painted, so a delta nobody consumes would otherwise be leaked.
            out.textures_delta.clear();
        }
        // **Positive control on the scale itself.** Without this the whole `SCALES` loop is three runs of
        // the same 1.0 pass wearing different labels, which is the precise shape of a points-only harness
        // agreeing with a broken build.
        assert_eq!(
            ctx.pixels_per_point(),
            ppp,
            "the context ran at {} rather than the {ppp} this case is about",
            ctx.pixels_per_point()
        );
        Room {
            strip: room.expect("the body ran"),
            picture: picture.expect("the body ran"),
            pane: pane.expect("the body ran"),
            tab: tab.expect("the wrapper ran"),
            tab_offset: tab_offset.expect("the wrapper ran"),
        }
    }

    /// A strip taller than the pane, made tall the way the real one gets tall: rows of text stacked one
    /// under another. The count is derived from the pane and **the style's own row height** rather than
    /// picked, so it stays over-long if the theme's text grows or shrinks.
    fn over_long(pane_h: f32) -> impl Fn(&mut egui::Ui) + Copy {
        move |ui: &mut egui::Ui| {
            let row = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
            let rows = (pane_h / row.max(1.0)).ceil() as usize + 2;
            for i in 0..rows {
                ui.label(format!("strip row {i}"));
            }
        }
    }

    /// A strip that fits anywhere: the two rows the real one always has something like.
    fn short(ui: &mut egui::Ui) {
        ui.label("layers:");
        ui.label("slot 0 (empty)");
    }

    /// **The bound.** The strip never takes more of the pane than it leaves for the picture, at any pane
    /// height and any scale.
    #[test]
    fn the_strip_never_takes_more_of_the_pane_than_it_leaves() {
        for ppp in SCALES {
            for pane_h in PANE_HEIGHTS {
                let room = lay_out(pane_h, ppp, over_long(pane_h), true);
                // Positive control: a strip that fits would make every assertion below vacuous.
                assert!(
                    room.strip.wanted > room.pane,
                    "the stand-in strip wanted {} of a {} pane at {ppp}, so it is not the over-long \
                     case this gate is about",
                    room.strip.wanted,
                    room.pane,
                );
                assert!(
                    room.strip.shown <= pane_h * SCREEN_STRIP_MAX_SHARE + EPS,
                    "the strip took {} of a {pane_h}-point pane at {ppp}, past its \
                     {SCREEN_STRIP_MAX_SHARE} share ({}). The picture is the remainder, so this is the \
                     defect: budget {}, wanted {}.",
                    room.strip.shown,
                    pane_h * SCREEN_STRIP_MAX_SHARE,
                    room.strip.cap,
                    room.strip.wanted,
                );
            }
        }
    }

    /// **The defect, in the exact terms the tab failed in.** Not a height compared against a number read
    /// off a run: what the strip left is put through the same [`screen_room`] the picture is put through,
    /// with the native frame, and asked whether anything comes back. Under the shipped arrangement this
    /// was `Err` and the tab drew the give-up sentence where the game should be.
    #[test]
    fn the_picture_keeps_room_to_draw_under_an_over_long_strip() {
        for ppp in SCALES {
            for pane_h in PANE_HEIGHTS {
                let room = lay_out(pane_h, ppp, over_long(pane_h), true);
                let fit = screen_room(room.picture, SRC.0, SRC.1, ppp, aspect());
                assert!(
                    fit.is_ok(),
                    "a {pane_h}-point pane at {ppp} left the picture {:?}, which draws nothing ({fit:?}). \
                     The strip took {} of it against a {} budget.",
                    room.picture,
                    room.strip.shown,
                    room.strip.cap,
                );
            }
        }
    }

    /// The rows the bound pushes off the strip are **reachable**, which is the whole difference between a
    /// cap with a scrollbar and a crop. Under the shipped arrangement the strip's overflow was cut off at
    /// the pane's edge with no way to get at it.
    #[test]
    fn an_over_long_strip_is_scrollable_rather_than_cropped() {
        for ppp in SCALES {
            for pane_h in PANE_HEIGHTS {
                let room = lay_out(pane_h, ppp, over_long(pane_h), true);
                assert!(
                    room.strip.wanted > room.strip.shown + EPS,
                    "the strip showed {} of the {} it wanted at {pane_h}/{ppp}: nothing is off the \
                     bottom, so nothing proves the rest can be reached",
                    room.strip.shown,
                    room.strip.wanted,
                );
            }
        }
    }

    /// **The no-regression control, and the reason the share can be applied unconditionally.** A strip
    /// that fits is laid out exactly as it was: it takes its natural height, the cap is nowhere near it,
    /// and nothing scrolls. If this ever fails, the bound has started charging panes it was never meant
    /// to touch — and with it the parked look call would have been changed by the back door.
    #[test]
    fn a_strip_that_fits_is_left_exactly_as_it_was() {
        for ppp in SCALES {
            let room = lay_out(800.0, ppp, short, true);
            assert!(
                (room.strip.shown - room.strip.wanted).abs() <= EPS,
                "a two-row strip showed {} of the {} it wanted at {ppp}: the cap is biting a strip that \
                 fits",
                room.strip.shown,
                room.strip.wanted,
            );
            assert!(
                room.strip.wanted < room.strip.cap,
                "a two-row strip wanted {} against a {} budget at {ppp}, so this case is not the \
                 comfortable one it is here to measure",
                room.strip.wanted,
                room.strip.cap,
            );
            assert!(
                !room.tab_scrolled(),
                "a wheel over a comfortable pane at {ppp} moved the tab body {} points: the strip and \
                 the picture no longer fit in it, which is the defect one size up",
                room.tab_offset,
            );
            assert!(
                screen_room(room.picture, SRC.0, SRC.1, ppp, aspect()).is_ok(),
                "the comfortable case left the picture {:?}, which draws nothing",
                room.picture,
            );
        }
    }

    /// **Why the share is one half and not a number someone liked**, pinned from both sides.
    ///
    /// The rule is *the strip may not take more of the pane than it leaves for the picture*, and the two
    /// halves of that sentence fail in opposite directions:
    ///
    /// * a **larger** share breaks the ceiling — the strip's own budget becomes more than the remainder,
    ///   which is the sentence read backwards;
    /// * a **smaller** share breaks the floor under the strip — the bound starts charging strips that
    ///   were not the larger half, changing panes that render correctly today.
    ///
    /// ⚑ **The first of the two was missing when this gate was written**, and a mutation caught it: at a
    /// `0.9` share only the second assertion existed and it stayed green, because "bites only where the
    /// strip was already the majority" is implied by *every* share at or above a half. One assertion for
    /// a two-sided property is one assertion short.
    #[test]
    fn the_bound_bites_only_where_the_picture_was_already_the_minority() {
        for pane in PANE_HEIGHTS {
            let cap = screen_strip_cap(pane);
            // The ceiling. The budget itself is never more than what the budget leaves.
            assert!(
                cap <= pane - cap,
                "a {pane}-point pane budgets the strip {cap} and leaves the picture {}, so the strip is \
                 allowed to be the larger half. {SCREEN_STRIP_MAX_SHARE} is above the rule it is \
                 documented as.",
                pane - cap,
            );
            for step in 0..=40 {
                let natural = pane * step as f32 / 40.0;
                if natural <= cap {
                    continue;
                }
                let picture_before = pane - natural;
                assert!(
                    picture_before < natural,
                    "a {natural}-point strip in a {pane}-point pane is bounded to {cap}, but it was \
                     leaving the picture {picture_before} — more than it took. The bound is charging a \
                     strip that was not the larger half, so {SCREEN_STRIP_MAX_SHARE} is a preference \
                     rather than the rule it is documented as."
                );
            }
        }
    }

    /// **Loud on unmeasurable.** A pane whose height is not a finite positive number is broken rather
    /// than short, and a cap of zero there would erase the strip — the mask statement and the effects
    /// statement with it — silently. The bound stands down, and the pane is refused one level down by
    /// [`screen_room`], which is where the sentence gets painted.
    #[test]
    fn an_unmeasurable_pane_stands_the_bound_down_and_is_loud_one_level_down() {
        for h in [f32::INFINITY, f32::NAN, 0.0, -10.0, f32::NEG_INFINITY] {
            let cap = screen_strip_cap(h);
            assert!(
                cap.is_infinite() && cap.is_sign_positive(),
                "a {h}-point pane produced a {cap}-point budget: a finite cap on an unmeasurable pane \
                 is a strip erased with nothing on screen to say why"
            );
        }
        for ppp in SCALES {
            assert_eq!(
                screen_room(
                    egui::vec2(f32::INFINITY, f32::INFINITY),
                    SRC.0,
                    SRC.1,
                    ppp,
                    aspect()
                ),
                Err(NO_ROOM_FOR_SCREEN),
                "an unmeasurable pane at {ppp} was not refused downstream either, so standing the \
                 bound down leaves nothing saying anything"
            );
        }
    }

    /// **The row's stated cure, refuted where it can be checked** (`F-SCREEN-TAB-STRIP-UNBOUNDED` asked
    /// for *"making that tab scroll"*).
    ///
    /// The tab already scrolls. `egui_dock` wraps every body in a `ScrollArea` and this window never
    /// turned it off, so the arrangement that shipped was **already inside one**. This lays that exact
    /// arrangement out and reports both halves at once: the wrapper says it has more content than it can
    /// show — it was offering to scroll the whole time — and the picture is still nothing at every
    /// offset, because a scroll area moves content that *overflows* and the picture is elastic. It does
    /// not overflow; it collapses, and the deficit disappears into it.
    ///
    /// This gate does not test the fix. It is the standing statement of why the fix is a bound and not a
    /// scroll area, and it fails the day someone deletes the bound and calls the wrapper the cure.
    #[test]
    fn the_tab_bodys_own_scroll_area_was_there_all_along() {
        for ppp in SCALES {
            for pane_h in [240.0, 160.0, 90.0] {
                let room = lay_out(pane_h, ppp, over_long(pane_h), false);
                assert!(
                    room.tab_scrolled(),
                    "a wheel over the pane did not move the tab body at {pane_h}/{ppp} (offset {}, \
                     content {} in {}), so it was NOT offering to scroll and this gate's premise — \
                     that the cure the row asked for is already in the build — is wrong",
                    room.tab_offset,
                    room.tab.0,
                    room.tab.1,
                );
                assert_eq!(
                    screen_room(room.picture, SRC.0, SRC.1, ppp, aspect()),
                    Err(NO_ROOM_FOR_SCREEN),
                    "the unbounded strip left the picture {:?} in a {pane_h}-point pane at {ppp}, which \
                     still draws something — the defect this parcel is named for does not reproduce, \
                     and the argument against the row's cure rests on it",
                    room.picture,
                );
            }
        }
    }
}

/// **The subtype list, measured off what it actually paints.**
///
/// The owner's third finding, 2026-09-09: *"In spawn there's a scrollbar and to the left is the names
/// but the highlight for select goes all the way through to the right and the names kind of stink too
/// ($20Down_Red), it's just cluttered looking and gross imo."*
///
/// Two of the three defects in that sentence are geometry, and geometry is checkable: `egui` builds a
/// `Ui` on a headless `Context` and hands back the shapes it painted. So this reads the **drawn** run
/// positions rather than any intermediate the renderer might get right on its own — the value and the
/// name must not touch, and the selected row's fill must not run to the edge of the pane.
///
/// The third (the scrollbar's position) is `ScrollArea`'s own layout and is **not asserted**; it is
/// `auto_shrink([false, true])` and a foreground look.
#[cfg(test)]
mod subtype_list_tests {
    use super::*;
    use crate::spawn_picker::{SubtypeListing, SubtypeRow};

    /// The pane the list is drawn into. Deliberately far wider than the rows need, because that gap is
    /// exactly where the old band and the old zebra striping went.
    const PANE: egui::Vec2 = egui::vec2(520.0, 400.0);

    fn row(value: &str, label: &str, selected: bool) -> SubtypeRow {
        SubtypeRow {
            name: format!("ObjDef_Spring_{label}"),
            label: label.into(),
            value: value.into(),
            selected,
            offered: true,
            note: None,
        }
    }

    /// His own rows, spellings included.
    fn listing() -> SubtypeListing {
        SubtypeListing {
            rows: vec![
                row("$00", "Up_Red", true),
                row("$02", "Up_Yellow", false),
                row("$20", "Down_Red", false),
            ],
            count: "3 subtypes".into(),
            absence: None,
            truncation: None,
            collision: None,
            armed: "A click places a spring.".into(),
        }
    }

    /// Every `Shape::Text` the frame painted, flattened out of the nesting `egui` produces, as
    /// (rect, text).
    pub(super) fn text_runs(shapes: &[egui::epaint::ClippedShape]) -> Vec<(egui::Rect, String)> {
        fn walk(s: &egui::Shape, out: &mut Vec<(egui::Rect, String)>) {
            match s {
                egui::Shape::Text(t) => out.push((
                    t.galley.rect.translate(t.pos.to_vec2()),
                    t.galley.text().into(),
                )),
                egui::Shape::Vec(v) => {
                    for s in v {
                        walk(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut out);
        }
        out
    }

    /// Every filled rectangle the frame painted, flattened the same way.
    fn fills(shapes: &[egui::epaint::ClippedShape]) -> Vec<(egui::Rect, egui::Color32)> {
        fn walk(s: &egui::Shape, out: &mut Vec<(egui::Rect, egui::Color32)>) {
            match s {
                egui::Shape::Rect(r) => out.push((r.rect, r.fill)),
                egui::Shape::Vec(v) => {
                    for s in v {
                        walk(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut out);
        }
        out
    }

    fn draw() -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, PANE)),
            ..Default::default()
        };
        let l = listing();
        let mut out = ctx.run_ui(raw, |ui| {
            subtype_list(ui, &l, "subtype-list-test");
        });
        out.textures_delta.clear();
        out.shapes
    }

    /// **The value and the name are two columns**, and there is daylight between them.
    ///
    /// He read a row as `$20Down_Red` because `item_spacing.x` was zero: the model had carried
    /// `SubtypeRow::value` and `SubtypeRow::label` apart all along and the renderer drew them touching.
    #[test]
    fn a_subtype_row_draws_its_value_and_its_name_as_separate_columns() {
        let shapes = draw();
        let runs = text_runs(&shapes);
        for (value, label) in [("$00", "Up_Red"), ("$02", "Up_Yellow"), ("$20", "Down_Red")] {
            let v = runs
                .iter()
                .find(|(_, t)| t == value)
                .unwrap_or_else(|| panic!("the value {value:?} was never painted: {runs:?}"));
            let n = runs
                .iter()
                .find(|(_, t)| t == label)
                .unwrap_or_else(|| panic!("the name {label:?} was never painted: {runs:?}"));
            assert!(
                (v.0.center().y - n.0.center().y).abs() < 2.0,
                "{value} and {label} are not on the same row: {v:?} {n:?}"
            );
            let gap = n.0.left() - v.0.right();
            assert!(
                gap >= LIST_GUTTER * 0.5,
                "{value} and {label} are drawn {gap} points apart, which is how the owner read one \
                 row as `{value}{label}`"
            );
        }
        // The value column is a column: every value ends on the same x, or it is not aligned and the
        // rows do not line up on the thing they are ordered by.
        let rights: Vec<f32> = ["$00", "$02", "$20"]
            .iter()
            .map(|v| {
                runs.iter()
                    .find(|(_, t)| t == v)
                    .expect("painted above")
                    .0
                    .right()
            })
            .collect();
        for r in &rights {
            assert!(
                (r - rights[0]).abs() < 1.0,
                "the values do not share a right edge, so they are not a column: {rights:?}"
            );
        }
    }

    /// **The selected row's highlight stops with the list, not with the pane.**
    ///
    /// It used to be `ui.max_rect().x_range()`, so a selected row was a long empty bar and the zebra
    /// banding striped the same emptiness. The pane here is far wider than the rows need, which is what
    /// gives the assertion something to catch.
    #[test]
    fn the_selection_highlight_does_not_run_to_the_edge_of_the_pane() {
        let shapes = draw();
        let runs = text_runs(&shapes);
        let widest_text = runs
            .iter()
            .filter(|(_, t)| t == "Up_Red" || t == "Up_Yellow" || t == "Down_Red")
            .map(|(r, _)| r.right())
            .fold(0.0_f32, f32::max);
        let selection = crate::theme::selection();
        let bands: Vec<egui::Rect> = fills(&shapes)
            .into_iter()
            .filter(|(_, c)| *c == selection)
            .map(|(r, _)| r)
            .collect();
        assert_eq!(
            bands.len(),
            1,
            "the fixture arms exactly one row, so exactly one band carries the selection fill: \
             {bands:?}"
        );
        let band = bands[0];
        assert!(
            band.right() < PANE.x - 1.0,
            "the highlight runs to the pane's edge ({} of {}), which is the long empty bar he \
             reported",
            band.right(),
            PANE.x
        );
        assert!(
            band.right() >= widest_text,
            "the band stops short of the widest name ({} < {widest_text}), so a click target now \
             stops where the glyphs do",
            band.right()
        );
    }
}

/// **The Objects table's columns, in arithmetic and in paint.**
///
/// The owner's fourth finding, 2026-09-09: *"look how difficult objects is to read"*, *"especially if
/// it's not fully open"*, with the headers running together as `slotaddrcode  x  yname`.
#[cfg(test)]
mod slot_table_tests {
    use super::*;

    /// Natural widths with an obvious total, so every number below is derived rather than observed.
    const NATURAL: [f32; 4] = [30.0, 50.0, 20.0, 40.0];
    const GUTTER: f32 = 14.0;
    const FLOOR: f32 = 120.0;

    /// **A column's width is its content's width, and the gutter is what goes BETWEEN two of them.**
    ///
    /// This is the fault, stated: a gutter folded into a column's own width sits at that column's far
    /// end, which for a right-aligned column is nowhere near the seam. Two adjacent boxes could then be
    /// laid out with their glyphs touching while every width in the table claimed to carry fourteen
    /// points of separation.
    #[test]
    fn a_columns_width_carries_no_gutter_of_its_own() {
        let w = fit_columns(&NATURAL, 400.0, GUTTER, FLOOR);
        assert_eq!(
            &w[..3],
            &NATURAL[..3],
            "a fixed column was handed anything other than what it measures, so the gap between two \
             columns is once again hidden inside one of them"
        );
    }

    /// The last column takes what is left **after the gutters as well as the other columns**, so a table
    /// laid out with a real gap between every pair still ends inside the pane.
    #[test]
    fn the_name_column_pays_for_the_gutters_it_sits_between() {
        let avail = 400.0;
        let w = fit_columns(&NATURAL, avail, GUTTER, FLOOR);
        let total: f32 = w.iter().sum::<f32>() + GUTTER * (w.len() - 1) as f32;
        assert!(
            (total - avail).abs() < 0.01,
            "the columns and the three gutters between them come to {total}, not the {avail} the \
             pane has: the last column is not paying for the gaps and the table runs off the edge"
        );
    }

    /// A pane too narrow for the table leaves the name readable rather than shrinking it to a letter,
    /// and a name column narrower than the floor to begin with is left at its own width.
    #[test]
    fn a_cramped_pane_keeps_the_name_column_readable() {
        let w = fit_columns(&NATURAL, 60.0, GUTTER, FLOOR);
        assert!(
            w[3] >= NATURAL[3].min(FLOOR),
            "the name column was squeezed to {}, below what a name needs",
            w[3]
        );
        // A column whose own content is narrower than the floor is not stretched to it: the floor is a
        // minimum for a name, not a reservation for one that is not there.
        let short = fit_columns(&[30.0, 8.0], 10.0, GUTTER, FLOOR);
        assert!(
            (short[1] - 8.0).abs() < 0.01,
            "a short last column was stretched to the floor: {short:?}"
        );
    }

    /// **The drawn headers do not touch**, measured off what the table actually paints.
    ///
    /// `slot` is numeric and therefore right-aligned; `addr` is not and is left-aligned. Their two boxes
    /// are adjacent, so before the gutter moved out of the widths those two headers were drawn with
    /// nothing between them at all: `slotaddr`. Every adjacent pair is checked, because which pair
    /// collides depends on which way each column happens to align.
    #[test]
    fn every_pair_of_headers_is_drawn_with_daylight_between_them() {
        let ctx = egui::Context::default();
        let rows: Vec<objects::Row> = Vec::new();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 400.0),
                )),
                ..Default::default()
            },
            |ui| {
                slot_table(ui, &objects::POOL_COLS, &rows, None, "header-gap-test");
            },
        );
        out.textures_delta.clear();
        let runs = super::subtype_list_tests::text_runs(&out.shapes);
        let heads: Vec<(egui::Rect, String)> = objects::POOL_COLS
            .iter()
            .map(|c| {
                runs.iter()
                    .find(|(_, t)| t == c.head)
                    .unwrap_or_else(|| {
                        panic!("the header {:?} was never painted: {runs:?}", c.head)
                    })
                    .clone()
            })
            .collect();
        for pair in heads.windows(2) {
            let gap = pair[1].0.left() - pair[0].0.right();
            assert!(
                gap >= COL_GUTTER * 0.5,
                "the headers {:?} and {:?} are {gap} points apart, which is how they read as one \
                 run-together word",
                pair[0].1,
                pair[1].1
            );
        }
    }
}
