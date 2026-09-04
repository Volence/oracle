//! **The panel nav** — the affordance that reaches every [`Tab`] the player ships, from the window,
//! without prior knowledge.
//!
//! # The defect this repairs, which was a design defect and not a code one
//!
//! `egui_dock` draws only **each leaf's active tab**. [`ui::initial_dock`] puts Registers/Memory/Objects
//! in one pane and Breakpoints/Watchpoints/Profiler in another, so on any frame **four of the eight panel
//! bodies do not run** and their titles sit behind other titles in a tab bar. That fact was already
//! written down twice in this crate — [`crate::screen`]'s header leans on it to argue what
//! `emulator/screen_text` may report, and `--dock every-tab` exists because a panel-cost measurement
//! taken under the default layout measures the arrangement instead of the panels.
//!
//! ⚑ **Four, not six.** `screen.rs` said six and the brief for this row repeated it; the count is
//! measured in `the_default_layout_hides_one_body_per_shared_pane_and_the_count_is_measured` below and it
//! is four. `egui_dock` draws one body per *leaf* and the default has four leaves. The six counts every
//! tab that shares a pane, but two of those six are their own leaf's active tab and do run. Nothing in
//! either argument turns on which number it is — but a wrong number in a header is a wrong number, and
//! this one had been copied twice before it was checked.
//!
//! What nobody wrote down is the consequence for a human: **the window shipped with no menu, no tab list
//! and no other affordance for bringing a hidden panel forward.** Nothing was broken — the panels were
//! behind other panels. The owner's own look at the window found it in one sentence: *"there's no way to
//! open any of those tabs though."*
//!
//! ⚠ **What that report does not establish, said rather than smoothed.** `egui_dock` draws *every* tab of
//! a leaf in that leaf's tab bar, so in principle those four had clickable titles and were not literally
//! unreachable. Why they could not be opened on his screen — a bar too narrow for three titles and
//! scrolling, panes too small to notice, or nothing that reads as a way in — is a question about pixels,
//! and no window is opened from a test. It is not answered here. The repair does not depend on the
//! answer: a menu that names all eight in one list works whatever the pixel-level cause was, and unlike a
//! tab bar it does not need a pane wide enough to read.
//!
//! # The shape, and why it is this shape
//!
//! **A menu button in the top bar, outside the dock.** Two decisions in that sentence, both load-bearing:
//!
//! * **Outside the dock, not a [`Tab`].** A nav that lived in the `DockState` would be a panel that can
//!   be hidden behind another panel — which is the exact failure it exists to repair, and the one
//!   arrangement in which the escape hatch is behind the door it opens. It would also owe
//!   [`crate::layout::LAYOUT_VERSION`] a bump and discard every layout already on the owner's disk, to
//!   buy a strictly worse nav. So it sits in `Panel::top("bar")` beside the transport bar, drawn
//!   unconditionally on every frame, for the same reason the transport bar does: *things you do are
//!   controls; the `Tab` enum is for things you look at.* The **saved layout is untouched by the nav's
//!   existence** — the nav has no state of its own to save, and `entries` derives what it shows from the
//!   `DockState` every repaint rather than caching it.
//! * **A menu rather than eight buttons.** Eight always-visible titles beside the app name, the
//!   pause/step buttons and the status line would be the widest thing in the window at the moment the
//!   window is at its narrowest. One labelled button that opens a list of eight is the traditional shape
//!   and the one the owner named, and its label is on the glass at all times, which is the property the
//!   defect was about.
//!
//! # Focus, never a second copy
//!
//! The owner's case was **panels that were open and behind other panels**. Opening a second copy of an
//! already-docked tab is a fix that looks right and is wrong: `egui_dock` will happily hold two `Screen`
//! tabs, they would both draw, and the layout the user arranged would grow a tab every time they used
//! the nav. So [`reveal`] finds the tab first and **only** pushes one when there is none:
//!
//! * **docked** → make it its leaf's active tab and focus that leaf ([`Reveal::Focused`]);
//! * **closed** → put it back beside a pane-mate from [`ui::initial_dock`] if one of those is still on
//!   screen, else in the focused leaf, and make it active ([`Reveal::Reopened`]).
//!
//! "Beside a pane-mate" is where *the default layout* would have put it, read out of `initial_dock()`
//! rather than restated here — so a panel reopened after being closed lands next to its neighbours
//! instead of on top of the game screen, and a future rearrangement of the default moves this with it.
//!
//! # What `reveal` deliberately does not touch
//!
//! A leaf the user has **collapsed** keeps its tab bar (`egui_dock` gives a collapsed leaf exactly
//! `tab_bar_height`), so a tab inside one is visible and clickable, and focusing it is honest. Expanding
//! it is not available to us: `Tree::node_update_collapsed`, which repairs the ancestor splits'
//! `collapsed_leaf_count` and `fully_collapsed`, is `pub(crate)` in `egui_dock-0.21.1`, and setting
//! `LeafNode::collapsed = false` without it leaves the parent counters overstated and the layout wrong.
//! Registered as `F-NAV-COLLAPSED-LEAF`: an upstream `pub` on that method, or a `DockState` method that
//! expands a leaf, closes it.

use crate::screen;
use crate::ui::{self, Tab};
use egui_dock::{DockState, TabIndex};

/// The nav button's label, as a constant.
///
/// A constant rather than a literal at the draw site because [`crate::screen`] reports this bar over
/// `emulator/screen_text`, and a label written twice is a window and a tool naming one control two ways
/// — the rule [`ui::PAUSE_LABEL`] and its neighbours were made constants for.
///
/// **Plain letters, no chevron, and the rule is ASCII.** The bar's text goes through `screen_text`'s
/// glyph probe, so a decorative `▾` that egui's bundled fonts happen not to carry would be reported as an
/// unrenderable glyph in the player's *own* readback — a defect manufactured for an ornament. Whether a
/// given ornament is carried cannot be measured from a test (no frame, no atlas — see
/// `the_navs_own_text_stays_inside_ascii`), so the nav does not spend one.
pub const PANELS_LABEL: &str = "panels";

/// The suffix the menu puts after a tab that is **not in the dock at all**.
///
/// A constant for the same reason as [`PANELS_LABEL`], and a *word* rather than a symbol because the
/// distinction it draws — closed versus merely behind something — is the one thing a human cannot infer
/// from the menu's own highlighting.
pub const CLOSED_SUFFIX: &str = " (closed)";

/// Where a [`Tab`] stands relative to what the window is showing.
///
/// Three states rather than a bool, because *behind another tab* and *not in the layout at all* are
/// different facts with different remedies, and the nav is the only place a human can tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Docked and its leaf's active tab: its body runs this frame. (It may still be scrolled out of
    /// sight or in a collapsed leaf — see this module's header.)
    Showing,
    /// Docked, and some other tab of the same leaf is active. **The owner's case.**
    Hidden,
    /// Not in the `DockState` at all. The user closed it, and without the nav nothing could bring it
    /// back short of discarding the stored layout.
    Closed,
}

/// What a [`reveal`] did. Returned rather than inferred so a test can tell the two apart, and so a
/// duplicate can never be mistaken for a focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reveal {
    /// The tab was already docked. Its leaf's active tab and the focus moved to it; **no tab was added**.
    Focused,
    /// The tab was not docked. Exactly one was pushed, made active, and focused.
    Reopened,
}

/// One row the nav offers.
///
/// `label` is [`Tab::title`] — *the same function* `egui_dock::TabViewer::title` returns, not a second
/// spelling of it, so the name in the menu and the name on the tab bar cannot become two names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub tab: Tab,
    pub label: &'static str,
    pub state: State,
}

impl Entry {
    /// The text the menu draws for this row.
    ///
    /// [`State::Hidden`] and [`State::Showing`] read identically here on purpose — the menu marks the
    /// showing one by selection highlight, which is what a menu does — while [`State::Closed`] is said in
    /// words, because "this panel is not in your layout" is not something a highlight can say.
    pub fn menu_label(&self) -> String {
        match self.state {
            State::Closed => format!("{}{CLOSED_SUFFIX}", self.label),
            _ => self.label.to_owned(),
        }
    }
}

/// **Every entry the nav offers, derived from [`Tab::ALL`] and the dock — never a list kept here.**
///
/// ⚑ *Why this function is a `map` over the enum and not eight rows.* A hand-written nav would be a
/// third copy of "which panels does this player have", beside the [`Tab`] enum and
/// [`crate::layout::VOCABULARIES`], and the failure mode of a third copy is silence: the next panel
/// somebody adds gets a `Tab` variant, a body, a place in the default dock — and no way to open it,
/// which is this row's own defect returning under a new name. Deriving it means the omission is not
/// expressible. `every_tab_the_player_ships_is_reachable_from_the_nav` below is what says so out loud,
/// and it takes its expectation from the *vocabulary table*, not from `Tab::ALL`.
pub fn entries(dock: &DockState<Tab>) -> Vec<Entry> {
    Tab::ALL
        .iter()
        .map(|&tab| Entry {
            tab,
            label: tab.title(),
            state: state_of(dock, tab),
        })
        .collect()
}

/// Where `tab` stands in `dock` right now. Re-derived every repaint; nothing about it is cached.
pub fn state_of(dock: &DockState<Tab>, tab: Tab) -> State {
    match dock.find_tab(&tab) {
        None => State::Closed,
        Some(path) => match dock.leaf(path.node_path()) {
            Ok(leaf) if leaf.active == path.tab => State::Showing,
            _ => State::Hidden,
        },
    }
}

/// How many copies of `tab` the dock holds, across every surface.
///
/// Exists because *"the nav must focus, not duplicate"* is a claim about a **count**, and the only honest
/// way to check a count is to take one. `DockState::find_tab` returns the first hit and would report a
/// duplicated tab as a present one — so every test in this crate that asserts a reveal did not add a tab
/// has to walk the leaves, and this is that walk written once.
///
/// **A measurement device, `#[cfg(test)]` on purpose.** The window itself never asks; the shipped nav
/// prevents duplicates by construction rather than by counting them afterwards, and a helper the binary
/// carries but never calls is a claim that something checks this at runtime. Nothing does — the tests do.
#[cfg(test)]
pub fn occurrences(dock: &DockState<Tab>, tab: Tab) -> usize {
    let mut n = 0;
    for (_, surface) in dock.iter_surfaces_indexed() {
        let Some(tree) = surface.node_tree() else {
            continue;
        };
        for node in tree.iter() {
            if let egui_dock::Node::Leaf(leaf) = node {
                n += leaf.tabs.iter().filter(|t| **t == tab).count();
            }
        }
    }
    n
}

/// **Bring `tab` in front of the human, and say which of the two things that meant.**
///
/// Never adds a second copy of a tab the dock already holds — see this module's header for why that is
/// the whole point rather than an optimisation. Never panics and never leaves the dock without the tab:
/// every branch ends with `tab` present, active in its leaf, and that leaf focused.
pub fn reveal(dock: &mut DockState<Tab>, tab: Tab) -> Reveal {
    if let Some(path) = dock.find_tab(&tab) {
        // `set_active_tab` errors only on a path that does not name a leaf, which `find_tab` cannot
        // return; the dock is left untouched in that impossible case rather than the window taken down.
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
        return Reveal::Focused;
    }

    // Closed. Put it back where the default layout keeps it, if any of the pane-mates it keeps it with
    // are still on screen.
    match home_leaf(dock, tab) {
        Some(node) => {
            if let Ok(leaf) = dock.leaf_mut(node) {
                leaf.tabs.push(tab);
                let last = TabIndex(leaf.tabs.len() - 1);
                let _ = leaf.set_active_tab(last);
            }
            dock.set_focused_node_and_surface(node);
        }
        // No pane-mate left (or the tab is alone in the default layout, as `Screen` is). The focused
        // leaf is where a human's attention already is; `push_to_focused_leaf` falls back to the first
        // leaf, and creates one if the dock is empty, so this cannot fail to place the tab.
        None => dock.push_to_focused_leaf(tab),
    }
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
    }
    Reveal::Reopened
}

/// The node of `dock` holding a tab that shares `tab`'s pane in [`ui::initial_dock`], if one is still
/// docked.
///
/// **Read out of the default layout rather than restated.** "Objects belongs with Registers and Memory"
/// is a fact `initial_dock` already owns; writing it again here would be a second arrangement to keep in
/// step with the first, and the drift would be invisible — a reopened panel landing in the wrong pane is
/// not something a test looks for unless it is told to.
fn home_leaf(dock: &DockState<Tab>, tab: Tab) -> Option<egui_dock::NodePath> {
    let home = ui::initial_dock();
    let (node, _) = home.main_surface().find_tab(&tab)?;
    let egui_dock::Node::Leaf(leaf) = &home.main_surface()[node] else {
        return None;
    };
    leaf.tabs
        .iter()
        .filter(|mate| **mate != tab)
        .find_map(|mate| dock.find_tab(mate))
        .map(|path| path.node_path())
}

/// **Draw the nav and act on whatever the human picked.** Returns the [`screen::Run`]s it drew, in draw
/// order, for `emulator/screen_text` — the same hand-back contract [`ui::Transport::bar`] has, and for
/// the same reason: there is then no second expression describing the bar that could drift from it.
///
/// Only the **button** is a run. The menu's rows are painted in a popup layer that exists for the frames
/// the menu is open, and reporting them as part of the top bar would tell a client the window says eight
/// things it says only while a mouse is held over a button. That is the same call [`crate::screen`]'s
/// header makes about panel bodies: report what is unconditionally on the glass.
pub fn bar(ui: &mut egui::Ui, dock: &mut DockState<Tab>) -> Vec<screen::Run> {
    let mut picked: Option<Tab> = None;
    ui.menu_button(PANELS_LABEL, |ui| {
        for entry in entries(dock) {
            let response = ui
                .selectable_label(entry.state == State::Showing, entry.menu_label())
                .on_hover_text(match entry.state {
                    State::Showing => "in front — this panel's body is drawing",
                    State::Hidden => {
                        "open, behind another tab in its pane — click to bring it forward"
                    }
                    State::Closed => "not in your layout — click to put it back",
                });
            if response.clicked() {
                picked = Some(entry.tab);
                ui.close();
            }
        }
    });
    if let Some(tab) = picked {
        reveal(dock, tab);
    }
    vec![screen::Run::label(PANELS_LABEL)]
}

// ---------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use egui_dock::{Node, NodeIndex};
    use serde_json::json;

    /// **Every variant the `Tab` enum has, according to serde's derive** — the compiler's own view of the
    /// enum, not a list anybody maintains.
    ///
    /// `#[derive(Deserialize)]` emits a `VARIANTS` slice and names all of it in the `unknown variant`
    /// error, so feeding the deserializer a name that is not a variant makes it recite the ones that are.
    /// This is the leg the existing layout gate does not have: that test derives "today's vocabulary"
    /// from [`Tab::ALL`], so a variant added to `Tab` and forgotten in `ALL` is invisible to it — and
    /// would be a panel with a body, a place in the default dock, and no nav row. Here `ALL` is the thing
    /// under test rather than the ruler.
    fn variants_according_to_serde() -> Vec<String> {
        let err = serde_json::from_value::<Tab>(json!("NotATabVariant"))
            .expect_err("`NotATabVariant` must not be a Tab");
        let msg = err.to_string();
        let (_, list) = msg.split_once("expected one of ").unwrap_or_else(|| {
            panic!(
                "serde no longer recites the variant list in its unknown-variant error ({msg}), so this \
                 helper is reading a different string than it thinks. Re-derive it or delete it — a \
                 silently-empty ruler measures nothing."
            )
        });
        let names: Vec<String> = list
            .split(", ")
            .map(|s| s.trim().trim_matches('`').to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        assert!(
            names.len() > 1,
            "serde's variant list parsed to {names:?}, which cannot be this enum"
        );
        names
    }

    /// Every tab in ONE leaf: the owner's complaint at its worst, and the arrangement in which every tab
    /// but one is provably [`State::Hidden`] at any moment.
    fn stacked() -> DockState<Tab> {
        DockState::new(Tab::ALL.to_vec())
    }

    /// Make some tab **other** than `tab` active in `tab`'s leaf, so a following [`reveal`] is a real
    /// move rather than a no-op that happens to end in the right state.
    fn hide(dock: &mut DockState<Tab>, tab: Tab) {
        let path = dock
            .find_tab(&tab)
            .expect("tab must be docked to be hidden");
        let leaf = dock
            .leaf_mut(path.node_path())
            .expect("find_tab gives a leaf");
        let other = (0..leaf.tabs.len())
            .find(|i| *i != path.tab.0)
            .expect("a leaf of one tab cannot hide it");
        leaf.set_active_tab(TabIndex(other)).expect("in bounds");
    }

    /// ★★ **The gate: every `Tab` the player ships is reachable from the nav.**
    ///
    /// Not a count. The expectation is **enumerated from the shipped tab vocabulary** —
    /// [`crate::layout::VOCABULARIES`]' newest row, a table of serde *names* kept in another module —
    /// and each member is turned into a `Tab` through the deserializer. The nav is built from
    /// [`Tab::ALL`] in [`entries`]. Two artifacts, two alphabets, one claim.
    ///
    /// ⚠ *If this went green for a reason other than the rule holding, what would it be?*
    ///
    /// 1. **The expectation and the nav reading the same list.** That is the failure the brief for this
    ///    row names, and it is ruled out by construction above — plus a **third** ruler,
    ///    [`variants_according_to_serde`], which is the derive's own list and is what catches the case
    ///    neither of the other two can: a variant added to `Tab` and left out of `Tab::ALL`. All three
    ///    are asserted equal here, so the pair below cannot be two copies agreeing.
    /// 2. **`reveal` doing nothing, on a tab that was already in front.** Ruled out per tab: [`hide`]
    ///    activates a *different* tab of the same leaf first, and the pre-state is asserted
    ///    [`State::Hidden`] before the call. A `reveal` that returned without touching the dock fails on
    ///    the line after it.
    /// 3. **`reveal` "succeeding" by adding a second copy.** Ruled out by [`occurrences`] before and
    ///    after: focusing must leave the count at exactly one, and reopening must take it from zero to
    ///    exactly one.
    #[test]
    fn every_tab_the_player_ships_is_reachable_from_the_nav() {
        let vocabulary: Vec<String> = crate::layout::VOCABULARIES
            .last()
            .expect("VOCABULARIES is never empty")
            .iter()
            .map(|s| (*s).to_owned())
            .collect();

        // The three rulers agree, or the pair below proves nothing.
        assert_eq!(
            vocabulary,
            variants_according_to_serde(),
            "the newest row of VOCABULARIES is not the set of variants `Tab` actually has. Whatever the \
             nav offers, it is not 'every tab the player ships'."
        );
        let from_all: Vec<String> = Tab::ALL
            .iter()
            .map(|t| {
                serde_json::to_value(t)
                    .expect("a unit variant serialises")
                    .as_str()
                    .expect("a bare name")
                    .to_owned()
            })
            .collect();
        assert_eq!(
            from_all,
            variants_according_to_serde(),
            "`Tab::ALL` is not every variant of `Tab`. `entries()` maps over ALL, so the missing one has \
             a panel body and no way to open it — which is the defect this module exists to repair."
        );

        // The nav offers exactly these, once each, and nothing else.
        let offered = entries(&ui::initial_dock());
        let offered_names: Vec<String> = offered
            .iter()
            .map(|e| {
                serde_json::to_value(e.tab)
                    .expect("a unit variant serialises")
                    .as_str()
                    .expect("a bare name")
                    .to_owned()
            })
            .collect();
        assert_eq!(
            offered_names, vocabulary,
            "the nav does not offer the shipped tab vocabulary, in its order"
        );

        // Labels a human can act on: non-empty and distinct. Two rows reading the same is a nav that
        // cannot be used even though every tab is 'offered'.
        for e in &offered {
            assert!(
                !e.label.trim().is_empty(),
                "{:?} has a blank nav label",
                e.tab
            );
            assert_eq!(
                offered.iter().filter(|o| o.label == e.label).count(),
                1,
                "two nav rows both read {:?}",
                e.label
            );
        }

        // Per member: hidden -> focused, and closed -> reopened. Both against a real dock.
        for name in &vocabulary {
            let tab: Tab = serde_json::from_value(json!(name))
                .unwrap_or_else(|e| panic!("vocabulary names {name}, which is not a Tab: {e}"));

            // --- hidden behind another tab: FOCUSED, not duplicated. The owner's own case.
            let mut dock = stacked();
            hide(&mut dock, tab);
            assert_eq!(
                state_of(&dock, tab),
                State::Hidden,
                "{tab:?} was not actually hidden before the reveal, so the assertion after it would hold \
                 under a `reveal` that did nothing"
            );
            assert_eq!(occurrences(&dock, tab), 1);
            assert_eq!(reveal(&mut dock, tab), Reveal::Focused);
            assert_eq!(
                state_of(&dock, tab),
                State::Showing,
                "the nav's entry for {tab:?} did not bring it in front"
            );
            assert_eq!(
                occurrences(&dock, tab),
                1,
                "{tab:?} was DUPLICATED rather than focused — the fix that looks right and is wrong"
            );
            assert_eq!(
                dock.focused_leaf(),
                dock.find_tab(&tab).map(|p| p.node_path()),
                "{tab:?} is in front but its pane is not focused"
            );

            // --- closed entirely: REOPENED, exactly once, in front.
            let mut dock = ui::initial_dock();
            let path = dock.find_tab(&tab).expect("initial_dock carries every tab");
            dock.remove_tab(path).expect("the tab we just found");
            assert_eq!(
                state_of(&dock, tab),
                State::Closed,
                "{tab:?} survived being removed, so the reopen below starts from the wrong state"
            );
            assert_eq!(occurrences(&dock, tab), 0);
            assert_eq!(reveal(&mut dock, tab), Reveal::Reopened);
            assert_eq!(
                occurrences(&dock, tab),
                1,
                "reopening {tab:?} did not put back exactly one of it"
            );
            assert_eq!(
                state_of(&dock, tab),
                State::Showing,
                "{tab:?} came back into the layout without coming to the front"
            );
        }
    }

    /// **A reopened panel lands with its neighbours**, not on top of the game screen.
    ///
    /// The three inspect tabs share a pane in [`ui::initial_dock`]; closing one and reopening it through
    /// the nav must put it back in that pane while the other two are still there. Checked against
    /// `initial_dock`'s own grouping rather than against a literal, so rearranging the default
    /// rearranges this.
    #[test]
    fn a_reopened_tab_returns_to_the_pane_the_default_layout_keeps_it_in() {
        let home = ui::initial_dock();
        // Derive a tab that has pane-mates, from the default layout. `Objects` is one today; taking it
        // from the layout means this test follows a rearrangement instead of rotting against one.
        let (tab, mate) = home
            .main_surface()
            .iter()
            .find_map(|node| match node {
                Node::Leaf(leaf) if leaf.tabs.len() >= 2 => Some((leaf.tabs[0], leaf.tabs[1])),
                _ => None,
            })
            .expect("initial_dock has a pane holding more than one tab");

        let mut dock = ui::initial_dock();
        let path = dock.find_tab(&tab).expect("docked");
        dock.remove_tab(path);
        assert_eq!(reveal(&mut dock, tab), Reveal::Reopened);

        assert_eq!(
            dock.find_tab(&tab).map(|p| p.node_path()),
            dock.find_tab(&mate).map(|p| p.node_path()),
            "{tab:?} came back somewhere other than the pane it shares with {mate:?} by default"
        );
    }

    /// **The last panel standing.** Every other tab closed, then the nav asked for one of them: there is
    /// no pane-mate to land beside, and the fallback must still place it, once, in front.
    ///
    /// This is the branch [`home_leaf`] returns `None` on, and it is reachable in the real window —
    /// `Screen` is alone in its pane in the default layout, so it takes it every time.
    #[test]
    fn a_tab_with_no_surviving_pane_mate_still_reopens() {
        for &tab in Tab::ALL.iter() {
            let mut dock = DockState::new(vec![Tab::Screen]);
            if tab == Tab::Screen {
                dock = DockState::new(vec![Tab::Pacing]);
            }
            assert_eq!(state_of(&dock, tab), State::Closed);
            assert_eq!(reveal(&mut dock, tab), Reveal::Reopened);
            assert_eq!(occurrences(&dock, tab), 1, "{tab:?}");
            assert_eq!(state_of(&dock, tab), State::Showing, "{tab:?}");
        }
    }

    /// ★ **How many panels the default layout hides — MEASURED, and it is not the number this repo has
    /// been saying.**
    ///
    /// `crates/oracle-player/src/screen.rs:12` states that `initial_dock`'s arrangement means *"six of
    /// the eight panel bodies do not run on a given frame"*, and the `PANELS-NAV` brief repeats it. **It
    /// is four.** `egui_dock` draws one body per *leaf*, and the default layout has four leaves —
    /// `[Screen]`, `[Pacing]`, `[Registers, Memory, Objects]`, `[Breakpoints, Watchpoints, Profiler]` —
    /// so four bodies run and four do not. The six counts every tab in a shared pane, but two of those
    /// six (`Registers` and `Breakpoints`) are their own leaf's active tab and do run. Corrected in
    /// `screen.rs`, in this module's header, and in design §5.9.
    ///
    /// This test therefore does not restate a number at all: it derives `Showing` from **the leaf count**,
    /// which is `egui_dock`'s actual rule, so a rearranged default moves the expectation with it and no
    /// figure in prose can go stale again without going red.
    ///
    /// ⚠ The `hidden > 0` clause is the anti-vacuity one for the whole file: if `initial_dock` gave every
    /// tab its own leaf, nothing would ever be `Hidden`, the nav would have no defect to repair, and
    /// `every_tab_the_player_ships_is_reachable_from_the_nav` would pass on a `reveal` that only ever
    /// reopened.
    #[test]
    fn the_default_layout_hides_one_body_per_shared_pane_and_the_count_is_measured() {
        let dock = ui::initial_dock();
        let showing = entries(&dock)
            .iter()
            .filter(|e| e.state == State::Showing)
            .count();
        let hidden = entries(&dock)
            .iter()
            .filter(|e| e.state == State::Hidden)
            .count();
        assert_eq!(
            showing + hidden,
            Tab::ALL.len(),
            "a tab in the default layout is neither showing nor hidden, so it is Closed — and the \
             default layout is supposed to carry every tab"
        );
        // `egui_dock` draws exactly one body per leaf, so the number of panels in front IS the number
        // of leaves. Derived, never typed.
        let leaves = dock
            .main_surface()
            .iter()
            .filter(|n| matches!(n, Node::Leaf(_)))
            .count();
        assert_eq!(
            showing, leaves,
            "the default layout has {leaves} panes but {showing} panels in front; egui_dock draws one \
             body per leaf, so these are the same number or `state_of` is wrong"
        );
        assert!(
            hidden > 0,
            "the default layout hides nothing, so the nav repairs nothing and every Hidden->Showing \
             assertion in this file is exercising a state the shipped layout cannot reach"
        );
        assert_eq!(
            hidden,
            Tab::ALL.len() - leaves,
            "hidden is not 'every tab that is not its leaf's active one'"
        );

        // Closed is a state this dock can reach and does not have.
        let mut closed = ui::initial_dock();
        let path = closed.find_tab(&Tab::Profiler).expect("docked");
        closed.remove_tab(path);
        assert_eq!(state_of(&closed, Tab::Profiler), State::Closed);
        assert!(entries(&closed)
            .iter()
            .any(|e| e.state == State::Closed && e.menu_label().ends_with(CLOSED_SUFFIX)));
    }

    /// The nav's label for a tab is [`Tab::title`] — the *same call* the tab bar's title comes from.
    ///
    /// A nav row naming a panel differently from its tab is a nav a human cannot follow, and this is the
    /// cheapest place to make that impossible rather than merely unlikely.
    #[test]
    fn the_nav_label_is_the_tab_bar_title() {
        for e in entries(&ui::initial_dock()) {
            assert_eq!(e.label, e.tab.title());
            assert!(e.menu_label().starts_with(e.label));
        }
    }

    /// [`reveal`] on a dock that does not hold the tab and has **no leaf at all** must still work. A
    /// user can close every panel; the nav is then the only way back, and a panic here would be the
    /// window dying at the exact moment the repair is needed.
    #[test]
    fn reveal_into_an_empty_dock_does_not_panic() {
        let mut dock: DockState<Tab> = DockState::new(vec![]);
        assert_eq!(reveal(&mut dock, Tab::Memory), Reveal::Reopened);
        assert_eq!(occurrences(&dock, Tab::Memory), 1);
        assert_eq!(state_of(&dock, Tab::Memory), State::Showing);
    }

    /// Focus is idempotent: asking twice for a tab already in front changes nothing and adds nothing.
    #[test]
    fn revealing_the_same_tab_twice_adds_nothing() {
        let mut dock = ui::initial_dock();
        assert_eq!(reveal(&mut dock, Tab::Memory), Reveal::Focused);
        assert_eq!(reveal(&mut dock, Tab::Memory), Reveal::Focused);
        assert_eq!(occurrences(&dock, Tab::Memory), 1);
        assert_eq!(
            dock.main_surface()
                .iter()
                .filter(|n| matches!(n, Node::Leaf(_)))
                .count(),
            ui::initial_dock()
                .main_surface()
                .iter()
                .filter(|n| matches!(n, Node::Leaf(_)))
                .count(),
            "revealing an already-docked tab restructured the layout"
        );
    }

    /// A tab hidden in a **second surface** (a floating window) is focused there, not copied into the
    /// main one. `DockState::find_tab` searches every surface and the nav must not be narrower.
    #[test]
    fn a_tab_in_another_surface_is_focused_where_it_is() {
        let mut dock = ui::initial_dock();
        let path = dock.find_tab(&Tab::Profiler).expect("docked");
        dock.remove_tab(path);
        let surface = dock.add_window(vec![Tab::Watchpoints, Tab::Profiler]);
        assert_eq!(
            reveal(&mut dock, Tab::Profiler),
            Reveal::Focused,
            "the nav reopened a tab that was open in a floating window"
        );
        assert_eq!(occurrences(&dock, Tab::Profiler), 1);
        assert_eq!(
            dock.find_tab(&Tab::Profiler).map(|p| p.surface),
            Some(surface),
            "the tab moved out of the window it was in"
        );
        assert_eq!(state_of(&dock, Tab::Profiler), State::Showing);
    }

    /// `NodeIndex` arithmetic in [`home_leaf`] is `egui_dock`'s; this pins the one assumption the
    /// function makes about the default layout — that every tab is in a leaf of it, so `find_tab`
    /// followed by an indexed `Node::Leaf` match cannot fall through for a real tab.
    #[test]
    fn every_tab_sits_in_a_leaf_of_the_default_layout() {
        let home = ui::initial_dock();
        for &tab in Tab::ALL.iter() {
            let (node, _) = home
                .main_surface()
                .find_tab(&tab)
                .unwrap_or_else(|| panic!("{tab:?} is in no pane of the default layout"));
            assert!(matches!(&home.main_surface()[node], Node::Leaf(_)));
            assert_ne!(node, NodeIndex(usize::MAX));
        }
    }

    /// **[`bar`] draws, and hands back the run it drew** — the `emulator/screen_text` half.
    ///
    /// Two claims in one pass over a real `egui::Ui`: the nav reports exactly the run it painted, and
    /// **drawing the nav with nothing clicked changes no layout**. The second is not idle: `bar` takes
    /// `&mut DockState` and is called on every frame of the window, so a `reveal` that leaked out of the
    /// click branch would rearrange the user's panels sixty times a second.
    #[test]
    fn the_bar_draws_hands_back_its_run_and_changes_nothing_by_itself() {
        let mut dock = ui::initial_dock();
        let before = entries(&dock);
        let mut runs: Vec<screen::Run> = Vec::new();
        egui::__run_test_ui(|ui| {
            runs = bar(ui, &mut dock);
        });
        assert_eq!(runs, vec![screen::Run::label(PANELS_LABEL)]);
        assert_eq!(
            entries(&dock),
            before,
            "drawing the panel menu moved the layout with nothing clicked"
        );
    }

    /// ⚑ **The nav's own text is plain ASCII**, which is the rule [`PANELS_LABEL`]'s doc states and the
    /// most of it that can be checked in a unit test.
    ///
    /// The claim I wanted to make is stronger — *every character the nav draws is one the bundled fonts
    /// can draw* — and it is **not available here.** [`crate::screen::Glyphs`] answers through
    /// `Fonts::layout_no_wrap` on a live `egui::Context`, and under `egui::__run_test_ctx` that call
    /// produces **no glyph rows at all**: measured, `'漢'`, `'ꨀ'` and `'p'` alike come back `None`, the
    /// probe's own *"this family cannot be measured"* state. So a headless assertion would be measuring
    /// the absence of a font atlas, not the label — and tolerating `None` to get a green would be exactly
    /// the vacuity the rest of this file refuses. **Booked as `F-NAV-GLYPH-UNMEASURED`**; it needs a
    /// frame, which means the windowed run, and no window is opened from here.
    ///
    /// What is left is a real rule and a sufficient one: ASCII is drawable by any font egui ships, so a
    /// nav that stays inside it cannot manufacture an unrenderable glyph in the player's own
    /// `screen_text` readback. It is deliberately **stricter** than the bar as a whole — [`ui::PAUSE_LABEL`]
    /// and its neighbours carry `⏸ ▶ ⏭`, which the emoji font does have — because the nav has no ornament
    /// worth the risk of the measurement it cannot take.
    #[test]
    fn the_navs_own_text_stays_inside_ascii() {
        let mut text = format!("{PANELS_LABEL}{CLOSED_SUFFIX}");
        for &tab in Tab::ALL.iter() {
            text.push_str(tab.title());
        }
        for c in text.chars() {
            assert!(
                c.is_ascii_graphic() || c == ' ',
                "the nav draws {c:?}. Whether the bundled fonts carry it cannot be measured without a \
                 frame (see this test's doc), so the nav does not spend one."
            );
        }
        // The anti-vacuity clause: the string under test is the real one and is not empty.
        assert!(text.len() > PANELS_LABEL.len() + CLOSED_SUFFIX.len());
    }
}
