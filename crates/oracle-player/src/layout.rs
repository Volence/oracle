//! **Layout persistence** — the third clause of the toolkit player's own goal: *debug views live in panels
//! that tab together, drag anywhere, and keep their layout between runs.* Tabs and drag shipped with parcel
//! 1; this module is the "between runs" half, and it was deliberately held back until the [`Tab`] enum
//! stopped moving (design §6). It has `Screen | Pacing | Registers | Memory | Objects` and, since the
//! stopping parcel, `Breakpoints | Watchpoints | Profiler` — all eight real.
//!
//! # The shape, and why it is this shape
//!
//! **eframe's own storage, not a hand-rolled config file.** [`eframe::Storage`] is a four-method key/value
//! trait that the native backend backs with a RON file under `storage_dir("oracle-player")`; the framework
//! already owns the "where does this go on this OS" question, and turning it on gets window geometry
//! remembered as well as the dock. The cost is real and is paid in `Cargo.toml`: `eframe/persistence`
//! pulls `ron`, `serde`, `egui-winit/serde` and `egui/persistence` into this crate.
//!
//! **RON, not JSON, and that is not a taste call.** A `DockState` stores an `egui::Rect` on every node, and
//! a node that has not been laid out yet holds [`egui::Rect::NOTHING`] — `±f32::INFINITY`. `serde_json`
//! cannot represent a non-finite float: it writes `null` and then fails to read one back as an `f32`. So a
//! `serde_json` round-trip of a freshly built layout is *lossy by construction*, and would have failed on
//! the very first save, before the first repaint filled the rects in. RON spells infinity `inf` and reads
//! it back. `eframe::set_value`/`get_value` are the RON pair, so using the framework's storage and using a
//! format that can hold the data are the same decision here.
//!
//! # The version integer, and why discard beats migrate
//!
//! [`LAYOUT_VERSION`] lives in its **own storage key**, beside the blob rather than inside it, so a stale
//! blob is never parsed at all — the version is read and compared first, and a mismatch returns
//! [`initial_dock`] without `ron` ever seeing the old tree.
//!
//! That matters because of what a `DockState<Tab>` actually contains. It serializes the [`Tab`] *values*,
//! as serde's external tagging of unit variants — the literal text `Objects` sits in the file. A plain
//! externally-tagged enum **errors on an unknown variant**, and serde offers no catch-all for that shape
//! (`#[serde(other)]` is internally-tagged only). So renaming or removing a variant does not cost the user
//! one tab; it costs them the whole layout, because the entire `DockState` fails to deserialize. The
//! remedy taken here is the cheap honest one: **discard wholesale, never migrate.** On any version
//! mismatch, and on any deserialize failure whatever, the user silently gets the default layout back. That
//! is the right behaviour for a layout and the wrong behaviour for a document, and it is why this file has
//! no migration code and no `Tab::Unknown(String)` placeholder to render.
//!
//! **So: bump [`LAYOUT_VERSION`] whenever the [`Tab`] enum changes** — a variant added, removed, renamed or
//! reordered. Forgetting to is not a crash (the unknown-variant failure lands in [`Discard::Blob`] and the
//! user still gets a working default), but the bump is what makes the discard *quiet and intended* rather
//! than an error path that happens to be survivable. `mod tests` below covers both routes.

use crate::ui::{self, Tab};
use egui_dock::DockState;

/// The layout format's version. **Bump on any change to [`Tab`]** — see this module's header.
///
/// 1: `Screen | Pacing | Registers | Memory | Objects`, the five-panel set parcel 2c finished.
/// 2: the same five plus `Breakpoints | Watchpoints | Profiler`, the three stopping tabs.
///
/// **This is not a number anybody bumps.** It is [`VOCABULARIES`]' length: the version *is* the answer to
/// "which tab vocabulary is this", so appending a row is the bump, and there is no second place to forget.
/// What is left to get wrong — changing [`Tab`] and appending no row — is what
/// `layout_version_is_the_last_row_of_the_tab_vocabulary` is red for.
pub const LAYOUT_VERSION: u32 = VOCABULARIES.len() as u32;

/// **Every [`Tab`] vocabulary this player has shipped, oldest first — append only.**
///
/// The entry at index `n` is version `n + 1`. The names are serde's, which is what a `DockState<Tab>`
/// actually writes into the layout file (external tagging of unit variants — the literal text `Objects`
/// sits in the RON), so this table is in the same alphabet as the thing it is versioning.
///
/// **Why a table rather than an `assert_eq!(LAYOUT_VERSION, 2)`.** A test that pins an integer against a
/// second copy of the integer goes green the moment somebody edits both — which is one edit. Here
/// [`LAYOUT_VERSION`] is this table's *length*, so appending a row **is** the bump; the only remaining
/// slip is changing [`Tab`] and appending nothing, and that is what the test is red for. The one hole left
/// is editing an existing row in place, which is not a slip — it is rewriting history on purpose, and the
/// row's own `// version N` comment is aimed at whoever tries.
pub const VOCABULARIES: &[&[&str]] = &[
    // version 1 — parcel 2c's five. **Historical: never edit this row.**
    &["Screen", "Pacing", "Registers", "Memory", "Objects"],
    // version 2 — parcel 3's three stopping tabs added.
    &[
        "Screen",
        "Pacing",
        "Registers",
        "Memory",
        "Objects",
        "Breakpoints",
        "Watchpoints",
        "Profiler",
    ],
];

/// The storage key holding the RON-encoded `DockState<Tab>`.
pub const LAYOUT_KEY: &str = "oracle_player_dock_layout";

/// The storage key holding [`LAYOUT_VERSION`] as decimal text. Deliberately a *separate* key: the version
/// is read and compared before the blob is handed to a deserializer, so a layout from a different `Tab`
/// vocabulary is discarded rather than parsed.
pub const VERSION_KEY: &str = "oracle_player_dock_layout_version";

/// Why a load produced the layout it did. The window logs this once at startup; nothing in the UI reacts
/// to it, because a discarded layout is not an error the user has to answer.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A stored layout at [`LAYOUT_VERSION`], deserialized. The window opens where it was left.
    Restored,
    /// Nothing stored under [`LAYOUT_KEY`] — a first run, or a run whose storage eframe could not open.
    /// Distinguished from [`Outcome::Discarded`] on purpose: "you have never saved one" and "the one you
    /// saved was unusable" are different facts, and collapsing them would make a persistence bug look
    /// exactly like a fresh install.
    Absent,
    /// Something was stored and is not usable. The layout is [`ui::initial_dock`].
    Discarded(Discard),
}

/// The two ways a stored layout is refused. Both take the identical fallback path.
#[derive(Debug, PartialEq, Eq)]
pub enum Discard {
    /// The version key was missing, unparseable, or named a different version. **The blob was not
    /// parsed** — this arm is decided before the deserializer is reached.
    Version {
        /// What stood in [`VERSION_KEY`], verbatim, for the startup log.
        stored: Option<String>,
    },
    /// The version matched and the blob still would not decode: truncated, corrupt, RON for some other
    /// type, or naming a [`Tab`] variant this build does not have (the migration hazard above, reached
    /// when someone edits `Tab` and forgets to bump [`LAYOUT_VERSION`]).
    Blob,
}

/// Write the current layout. Called from `eframe::App::save`, which the framework invokes on shutdown and
/// on its own auto-save interval.
///
/// The version is written **after** the blob, so a save interrupted between the two writes leaves a blob
/// with a stale (or absent) version — which the loader discards. The other order would leave a fresh
/// version stamped on an old blob, which is the one combination that could restore a wrong layout.
pub fn save(storage: &mut dyn eframe::Storage, dock: &DockState<Tab>) {
    eframe::set_value(storage, LAYOUT_KEY, dock);
    storage.set_string(VERSION_KEY, LAYOUT_VERSION.to_string());
}

/// Read the stored layout, or [`ui::initial_dock`] if there is not a usable one.
///
/// **This function never fails and never panics.** Every refusal — no storage, no blob, wrong version,
/// corrupt bytes, an unknown `Tab` name — lands on the same default layout, and the [`Outcome`] says which
/// so the caller can say so on stderr. Nothing here can wedge the window.
pub fn load(storage: Option<&dyn eframe::Storage>) -> (DockState<Tab>, Outcome) {
    let Some(storage) = storage else {
        return (ui::initial_dock(), Outcome::Absent);
    };
    if storage.get_string(LAYOUT_KEY).is_none() {
        return (ui::initial_dock(), Outcome::Absent);
    }
    // The version, first and on its own. A blob from another `Tab` vocabulary never reaches `ron`.
    let stored = storage.get_string(VERSION_KEY);
    if stored.as_deref().and_then(|s| s.trim().parse::<u32>().ok()) != Some(LAYOUT_VERSION) {
        return (
            ui::initial_dock(),
            Outcome::Discarded(Discard::Version { stored }),
        );
    }
    // `eframe::get_value` is `ron::from_str` with the decode error logged at debug and swallowed into a
    // `None` — which is exactly the contract this parcel wants: a layout that will not read is not an
    // error the user is asked about.
    match eframe::get_value::<DockState<Tab>>(storage, LAYOUT_KEY) {
        Some(dock) => (dock, Outcome::Restored),
        None => (ui::initial_dock(), Outcome::Discarded(Discard::Blob)),
    }
}

// ---------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage as _;
    use egui_dock::{Node, NodePath, SurfaceIndex, TabIndex};
    use std::collections::BTreeMap;

    /// [`eframe::Storage`] over a map. The trait is four methods and the native backend's own
    /// implementation is a `BTreeMap` plus a RON file, so this exercises the *shipped* seam — `save` and
    /// `load` below are the same calls the window makes, not test-only spellings of them.
    #[derive(Default)]
    struct MemStorage(BTreeMap<String, String>);

    impl eframe::Storage for MemStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    /// A readable description of **the whole layout**, walked through `egui_dock`'s public API rather than
    /// read off the serialized text: every surface, every node in tree order with its kind and split
    /// fraction, every leaf's tab list, open tab, scroll, collapse and tab-bar state, and which leaf holds
    /// focus. Two `DockState`s with the same shape string are the same layout as far as anything a user
    /// can see is concerned.
    ///
    /// This exists because `DockState` does not implement `PartialEq`, so "assert the restored state
    /// equals what was saved" has to be spelled out rather than derived.
    fn shape(dock: &DockState<Tab>) -> String {
        let mut out = String::new();
        for (si, surface) in dock.iter_surfaces_indexed() {
            let kind = match surface {
                egui_dock::Surface::Empty => "empty",
                egui_dock::Surface::Main(_) => "main",
                egui_dock::Surface::Window(..) => "window",
            };
            out.push_str(&format!("surface {si:?} {kind}\n"));
            let Some(tree) = surface.node_tree() else {
                continue;
            };
            for (i, node) in tree.iter().enumerate() {
                out.push_str(&match node {
                    Node::Empty => format!("  {i} empty\n"),
                    Node::Leaf(l) => format!(
                        "  {i} leaf tabs={:?} active={} scroll={} collapsed={} bar_hidden={}\n",
                        l.tabs, l.active.0, l.scroll, l.collapsed, l.tab_bar_hidden
                    ),
                    Node::Vertical(s) => format!(
                        "  {i} vertical fraction={} fully_collapsed={} collapsed_leaves={}\n",
                        s.fraction, s.fully_collapsed, s.collapsed_leaf_count
                    ),
                    Node::Horizontal(s) => format!(
                        "  {i} horizontal fraction={} fully_collapsed={} collapsed_leaves={}\n",
                        s.fraction, s.fully_collapsed, s.collapsed_leaf_count
                    ),
                });
            }
        }
        out.push_str(&format!("focused {:?}\n", dock.focused_leaf()));
        out
    }

    /// A layout a human could have made and [`ui::initial_dock`] never produces: `Objects` dragged out of
    /// the Registers/Memory/Objects pane into a pane of its own under the screen, `Memory` left open
    /// rather than `Registers`, and focus moved.
    ///
    /// Every test that saves a layout saves *this* one, and every one of them asserts it differs from the
    /// default first — see [`the_layout_under_test_is_not_the_default`] for why that assertion is the
    /// difference between this file proving something and proving nothing.
    fn rearranged() -> DockState<Tab> {
        let mut dock = ui::initial_dock();
        let below = {
            let surface = dock.main_surface_mut();

            let (node, tab) = surface
                .find_tab(&Tab::Objects)
                .expect("initial_dock() should contain an Objects tab to move");
            let objects = surface
                .remove_tab((node, tab))
                .expect("the tab just located should remove");

            let (screen, _) = surface
                .find_tab(&Tab::Screen)
                .expect("initial_dock() should contain a Screen tab");
            let [_, below] = surface.split_below(screen, 0.7, vec![objects]);

            let (regs, _) = surface
                .find_tab(&Tab::Memory)
                .expect("Memory should survive the move");
            surface
                .leaf_mut(regs)
                .expect("the Registers/Memory pane is a leaf")
                .set_active_tab(TabIndex(1))
                .expect("index 1 is in range for a two-tab leaf");
            below
        };

        // **`DockState::set_focused_node_and_surface`, not `Tree::set_focused_node`.** The tree method
        // sets the tree's own focus but leaves `DockState::focused_surface` at `None`, and
        // `DockState::focused_leaf` — which is what `shape()` reads and what the round trip therefore
        // checks — returns `None` unless the surface is set too. Using the tree method made the `focused`
        // line of the fingerprint read `None` on both sides of every comparison: present, and witnessing
        // nothing.
        dock.set_focused_node_and_surface(NodePath::new(SurfaceIndex::main(), below));
        dock
    }

    /// ⚠ **The anti-vacuity control, and it is a real one in this repo.** A round-trip test that saves the
    /// *default* layout passes under any breakage whatever — including a `load` hard-wired to return
    /// `initial_dock()` and ignore storage entirely — because it then compares the default against the
    /// default. Every test below leans on [`rearranged`] being genuinely different; this is where that is
    /// checked, once, loudly.
    #[test]
    fn the_layout_under_test_is_not_the_default() {
        let rearranged = rearranged();
        // Every line `shape()` emits should be capable of differing, or it is scenery. `focused` is the
        // one that nearly was not: `Tree::set_focused_node` leaves `DockState::focused_surface` unset, so
        // `focused_leaf()` answered `None` for both the rearranged layout and the default and that line
        // compared nothing against nothing.
        assert!(
            rearranged.focused_leaf().is_some(),
            "the rearranged layout has no focused leaf, so `shape()`'s `focused` line reads None on both \
             sides of every comparison in this file and witnesses nothing"
        );
        assert!(
            ui::initial_dock().focused_leaf().is_none(),
            "the default layout now focuses a leaf too — check that `focused` still discriminates"
        );

        let a = shape(&rearranged);
        let b = shape(&ui::initial_dock());
        assert_ne!(
            a, b,
            "`rearranged()` produced the DEFAULT layout. Every round-trip test in this file would then \
             compare initial_dock() against initial_dock() and stay green with persistence completely \
             broken. Fix `rearranged()`, not this assertion.\n--- rearranged ---\n{a}\n--- default ---\n{b}"
        );
    }

    /// The round trip: a non-default layout, saved and read back, restores **the whole structure**.
    ///
    /// Two independent assertions, because either alone has a way to be green for the wrong reason:
    ///
    /// * the shape string is derived from the public API and would miss a field `shape()` does not walk;
    /// * re-encoding the restored state and comparing the two blobs covers *every* serialized field
    ///   (rects, viewports, window states, `focused_surface`) but would be green if `save` ignored its
    ///   argument and wrote a constant — which the shape assertion and the control above rule out.
    ///
    /// The one thing neither can see is `DockState::translations`, which is `#[serde(skip)]` upstream and
    /// is UI strings rather than layout.
    #[test]
    fn a_rearranged_layout_round_trips_through_storage() {
        let saved = rearranged();
        let mut store = MemStorage::default();
        save(&mut store, &saved);

        let (restored, outcome) = load(Some(&store));
        assert_eq!(
            outcome,
            Outcome::Restored,
            "a layout just saved should load"
        );
        assert_eq!(
            shape(&restored),
            shape(&saved),
            "the restored layout is not the one that was saved"
        );

        let mut again = MemStorage::default();
        save(&mut again, &restored);
        assert_eq!(
            again.0.get(LAYOUT_KEY),
            store.0.get(LAYOUT_KEY),
            "re-encoding the restored layout does not reproduce the stored blob, so some serialized \
             field did not survive the trip"
        );
    }

    /// Nothing stored is [`Outcome::Absent`] and the default layout — not a discard, and not a panic.
    #[test]
    fn an_empty_storage_is_the_default_layout_and_reads_as_absent() {
        let store = MemStorage::default();
        let (dock, outcome) = load(Some(&store));
        assert_eq!(outcome, Outcome::Absent);
        assert_eq!(shape(&dock), shape(&ui::initial_dock()));

        let (dock, outcome) = load(None);
        assert_eq!(outcome, Outcome::Absent, "no storage at all is also absent");
        assert_eq!(shape(&dock), shape(&ui::initial_dock()));
    }

    /// A blob carrying the **wrong version integer** yields `initial_dock()`'s layout and no error.
    ///
    /// ⚠ *If this row went green for a reason other than the version check firing, what would it be?* It
    /// would be the blob failing to parse — then the fallback happens for the wrong reason and the version
    /// gate could be missing entirely. That alternative green path is ruled out **inside this test**: the
    /// same storage, with only the version string put back, restores the rearranged layout. So the blob is
    /// provably good and the version is provably the thing that refused it. The `Discard::Version` arm is
    /// asserted rather than a bare "not Restored" for the same reason.
    #[test]
    fn a_blob_from_another_version_is_discarded_for_the_default_layout() {
        let saved = rearranged();
        let mut store = MemStorage::default();
        save(&mut store, &saved);
        store.set_string(VERSION_KEY, (LAYOUT_VERSION + 1).to_string());

        let (dock, outcome) = load(Some(&store));
        assert_eq!(
            outcome,
            Outcome::Discarded(Discard::Version {
                stored: Some((LAYOUT_VERSION + 1).to_string())
            }),
            "a future version should be refused by the version gate, not by the deserializer"
        );
        assert_eq!(
            shape(&dock),
            shape(&ui::initial_dock()),
            "a version mismatch must give back the default layout"
        );

        // The control: the blob itself is fine. Only the version was wrong.
        store.set_string(VERSION_KEY, LAYOUT_VERSION.to_string());
        let (dock, outcome) = load(Some(&store));
        assert_eq!(
            outcome,
            Outcome::Restored,
            "the stored blob is unusable for some reason OTHER than its version, so the test above \
             proved nothing about the version gate"
        );
        assert_eq!(shape(&dock), shape(&saved));
    }

    /// A missing or unparseable version key is the same refusal. A blob with no version beside it is
    /// exactly what an interrupted save leaves behind.
    #[test]
    fn a_missing_or_junk_version_is_discarded_too() {
        for stored in [None, Some("".to_owned()), Some("one".to_owned())] {
            let mut store = MemStorage::default();
            save(&mut store, &rearranged());
            match &stored {
                None => store.remove_string(VERSION_KEY),
                Some(v) => store.set_string(VERSION_KEY, v.clone()),
            }
            let (dock, outcome) = load(Some(&store));
            assert_eq!(
                outcome,
                Outcome::Discarded(Discard::Version {
                    stored: stored.clone()
                }),
                "version {stored:?} should be refused"
            );
            assert_eq!(shape(&dock), shape(&ui::initial_dock()));
        }
    }

    /// **Garbage bytes yield the same fallback and no panic.** Five kinds, because "corrupt" has more than
    /// one shape and the truncation case is the one a killed process actually produces.
    #[test]
    fn garbage_bytes_are_discarded_without_a_panic() {
        let mut good = MemStorage::default();
        save(&mut good, &rearranged());
        let blob = good.0[LAYOUT_KEY].clone();

        let truncated = blob[..blob.len() / 2].to_owned();
        let cases: Vec<(&str, String)> = vec![
            ("empty string", String::new()),
            (
                "not RON at all",
                "\u{0}\u{1}\u{2}not a layout\u{ff}".to_owned(),
            ),
            ("truncated mid-blob", truncated),
            ("valid RON, wrong type", "(a: 1, b: \"two\")".to_owned()),
            ("plausible but nonsense", "(surfaces: 3)".to_owned()),
        ];

        for (label, bytes) in cases {
            let mut store = MemStorage::default();
            store.set_string(LAYOUT_KEY, bytes);
            store.set_string(VERSION_KEY, LAYOUT_VERSION.to_string());
            let (dock, outcome) = load(Some(&store));
            assert_eq!(
                outcome,
                Outcome::Discarded(Discard::Blob),
                "{label}: should be discarded as an unreadable blob"
            );
            assert_eq!(
                shape(&dock),
                shape(&ui::initial_dock()),
                "{label}: should fall back to the default layout"
            );
        }
    }

    /// **The migration hazard itself, demonstrated rather than asserted.** Design §6 claims that a
    /// `DockState<Tab>` carries the `Tab` variant *names* and that an unknown one fails the whole
    /// deserialize. Here it is: take a good blob at the right version and rename one tab, as removing or
    /// renaming a `Tab` variant would. The user does not lose one panel — the whole layout is refused —
    /// and this is what [`LAYOUT_VERSION`] exists to make deliberate.
    #[test]
    fn an_unknown_tab_name_costs_the_whole_layout_which_is_why_the_version_exists() {
        let mut store = MemStorage::default();
        save(&mut store, &rearranged());
        let blob = store.0[LAYOUT_KEY].clone();
        assert!(
            blob.contains("Objects"),
            "the serialized layout should name its tabs verbatim; §6's whole argument rests on it. Got:\n\
             {blob}"
        );
        store.set_string(LAYOUT_KEY, blob.replace("Objects", "Splines"));

        let (dock, outcome) = load(Some(&store));
        assert_eq!(outcome, Outcome::Discarded(Discard::Blob));
        assert_eq!(shape(&dock), shape(&ui::initial_dock()));
    }

    /// Why this file uses RON and not `serde_json`, checked rather than claimed: an un-laid-out node holds
    /// `Rect::NOTHING`, whose corners are `±f32::INFINITY`, and JSON has no spelling for those. If this
    /// ever stops being true the format choice can be revisited; while it is true, a JSON layout file
    /// would have been lossy from the first save.
    #[test]
    fn the_default_layout_holds_non_finite_rects_which_is_why_this_is_ron() {
        let dock = ui::initial_dock();
        let non_finite = dock
            .iter_all_nodes()
            .filter_map(|(_, n)| n.rect())
            .any(|r| !r.min.x.is_finite() || !r.max.x.is_finite());
        assert!(
            non_finite,
            "no node in initial_dock() carries a non-finite rect any more — the RON-over-JSON argument in \
             this module's header no longer holds and should be re-derived, not deleted on this test's say-so"
        );

        // And the consequence, made concrete: serde_json writes `null` for those and cannot read one back.
        let json =
            serde_json::to_string(&dock).expect("serde_json writes non-finite floats as null");
        assert!(
            json.contains("null"),
            "expected null-ed infinities in {json}"
        );
        assert!(
            serde_json::from_str::<DockState<Tab>>(&json).is_err(),
            "serde_json round-tripped a layout with non-finite rects, which it should not be able to"
        );
    }

    /// ★★ **The layout version is derived from the tab vocabulary, not remembered.**
    ///
    /// This is the gate for the easiest mistake in this parcel: changing the [`Tab`] enum and leaving
    /// [`LAYOUT_VERSION`] naming the *old* vocabulary. A stored layout would then pass the version check,
    /// reach `ron`, and fail on an unknown variant — the survivable-but-unintended [`Discard::Blob`] path
    /// this module's header says the bump exists to avoid.
    ///
    /// Ways to be red, which is what makes it a gate rather than a restatement:
    ///
    /// 1. a `Tab` change with nothing appended to [`VOCABULARIES`] — today's names match no shipped row;
    /// 2. a row appended that is not today's enum — the newest row and the enum disagree;
    /// 3. a row repeated — two versions cannot stand for two different tab sets.
    #[test]
    fn layout_version_is_the_last_row_of_the_tab_vocabulary() {
        let today: Vec<String> = Tab::ALL
            .iter()
            .map(|t| match serde_json::to_value(t).expect("a unit variant serialises") {
                serde_json::Value::String(s) => s,
                other => panic!(
                    "a Tab no longer serialises as a bare name ({other}), so the layout file's alphabet \
                     has changed and this table is in the wrong one"
                ),
            })
            .collect();
        let today: Vec<&str> = today.iter().map(String::as_str).collect();

        let idx = VOCABULARIES
            .iter()
            .position(|v| *v == today.as_slice())
            .unwrap_or_else(|| {
                panic!(
                    "the `Tab` enum spells a vocabulary no shipped LAYOUT_VERSION stands for. Append it \
                     to `VOCABULARIES` and set LAYOUT_VERSION to its 1-based index — every layout \
                     already on disk names the old set and must be discarded, not parsed.\n  today: \
                     {today:?}"
                )
            });
        assert_eq!(
            idx + 1,
            VOCABULARIES.len(),
            "the `Tab` enum spells vocabulary #{}, which is not the NEWEST row. LAYOUT_VERSION is this \
             table's length ({LAYOUT_VERSION}), so a layout saved by this build would be stamped \
             {LAYOUT_VERSION} while naming an older vocabulary — and on the next run it would pass the \
             version gate and then fail inside `ron`, which is the accidental discard the version exists \
             to prevent.",
            idx + 1
        );

        for (i, v) in VOCABULARIES.iter().enumerate() {
            assert!(
                !VOCABULARIES[..i].contains(v),
                "vocabulary #{} repeats an earlier one, so two LAYOUT_VERSIONs stand for the same tab \
                 set and the position() above cannot tell them apart",
                i + 1
            );
        }
        // The anti-vacuity clause: this test would pass on an empty `Tab::ALL` matching an empty row.
        assert!(
            today.len() > 1 && !VOCABULARIES.is_empty(),
            "Tab::ALL has {} entries — a degenerate vocabulary matches trivially",
            today.len()
        );
    }

    /// **What the bump buys, demonstrated on a real blob rather than asserted.**
    ///
    /// A layout saved by the previous build — the five-tab vocabulary, stamped with *its* version — is
    /// refused by the version gate and never reaches `ron`. That is the whole reason
    /// [`LAYOUT_VERSION`] moved, and it is checked against `VOCABULARIES`' own record of what the
    /// previous version spelled rather than against a literal.
    ///
    /// ⚠ *If this went green for a reason other than the version gate firing, what would it be?* It would
    /// be the blob failing to parse anyway — plausible here, because the blob genuinely names tabs this
    /// build still has. That alternative is ruled out inside the test: with the current version stamped on
    /// the identical bytes, the same blob **restores**.
    #[test]
    fn a_layout_from_the_previous_tab_vocabulary_is_discarded_by_the_version_gate() {
        let previous = LAYOUT_VERSION
            .checked_sub(1)
            .filter(|p| *p >= 1)
            .expect("there is no previous LAYOUT_VERSION for this gate to be tested against");
        let old_vocab = VOCABULARIES[previous as usize - 1];
        assert!(
            !old_vocab.contains(&"Breakpoints"),
            "the previous vocabulary already had the stopping tabs, so this test is about nothing"
        );

        // A layout built out of the previous vocabulary's tabs only — which is what a v1 file holds.
        let mut dock = DockState::new(vec![Tab::Screen]);
        dock.main_surface_mut().split_right(
            egui_dock::NodeIndex::root(),
            0.5,
            vec![Tab::Registers, Tab::Memory],
        );
        let mut store = MemStorage::default();
        eframe::set_value(&mut store, LAYOUT_KEY, &dock);
        store.set_string(VERSION_KEY, previous.to_string());

        let (got, outcome) = load(Some(&store));
        assert_eq!(
            outcome,
            Outcome::Discarded(Discard::Version {
                stored: Some(previous.to_string())
            }),
            "a layout stamped with the previous version must be refused BEFORE the deserializer"
        );
        assert_eq!(shape(&got), shape(&ui::initial_dock()));

        // The control: those exact bytes are readable. Only the stamp was wrong.
        store.set_string(VERSION_KEY, LAYOUT_VERSION.to_string());
        let (got, outcome) = load(Some(&store));
        assert_eq!(
            outcome,
            Outcome::Restored,
            "the blob is unusable for some reason OTHER than its version stamp, so the assertion above \
             proved nothing about the gate"
        );
        assert_eq!(shape(&got), shape(&dock));
    }

    /// The default layout carries every tab the enum has, and the measurement layout gives each one a leaf
    /// of its own.
    ///
    /// The first half is why a new `Tab` is reachable at all: a variant nothing docks is a panel body no
    /// human can open. The second is what makes the panel-cost measurement honest — `egui_dock` draws only
    /// a leaf's *active* tab, so three tabs sharing a pane execute one body per frame.
    #[test]
    fn both_layouts_carry_every_tab_and_the_bench_layout_gives_each_its_own_leaf() {
        for t in Tab::ALL {
            assert!(
                ui::initial_dock().main_surface().find_tab(&t).is_some(),
                "{t:?} is in the Tab enum and in no default pane, so nothing can open it"
            );
        }

        let bench = ui::every_tab_dock();
        let surface = bench.main_surface();
        let mut leaves: Vec<usize> = Vec::new();
        for t in Tab::ALL {
            let (node, _) = surface
                .find_tab(&t)
                .unwrap_or_else(|| panic!("{t:?} is missing from every_tab_dock()"));
            assert!(
                !leaves.contains(&node.0),
                "{t:?} shares a leaf with another tab in every_tab_dock(); egui_dock draws only a leaf's \
                 ACTIVE tab, so a bench run would execute one of the two bodies and report it as both"
            );
            leaves.push(node.0);
        }
        assert_eq!(leaves.len(), Tab::ALL.len());
        // …and the arrangement is genuinely different from the default, or the flag does nothing.
        assert_ne!(
            shape(&bench),
            shape(&ui::initial_dock()),
            "every_tab_dock() produced the default layout, so `--dock every-tab` changes nothing and the \
             AFTER measurement would be taken under the arrangement it was meant to replace"
        );
    }

    /// [`rearranged`] is written against a particular starting layout, and a silently restructured
    /// [`ui::initial_dock`] would make it degenerate rather than fail. This pins the two facts it uses:
    /// there is a `Screen` leaf to split under, and `Objects` starts in the same pane as `Registers` so
    /// that pulling it out is a real move.
    #[test]
    fn initial_dock_is_the_layout_the_helpers_assume() {
        let dock = ui::initial_dock();
        let surface = dock.main_surface();
        let screen = surface
            .find_tab(&Tab::Screen)
            .expect("a Screen tab to split under");
        assert!(
            matches!(&surface[screen.0], Node::Leaf(_)),
            "`rearranged()` calls split_below on the Screen node, which must be a leaf"
        );
        assert_eq!(
            surface.find_tab(&Tab::Objects).map(|(n, _)| n),
            surface.find_tab(&Tab::Registers).map(|(n, _)| n),
            "Objects and Registers should start in one pane; `rearranged()` moves Objects out of it"
        );
        assert_ne!(
            surface.find_tab(&Tab::Objects).map(|(n, _)| n),
            Some(screen.0),
            "Objects must not already be beside Screen, or `rearranged()` moves nothing"
        );
    }
}
