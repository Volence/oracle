//! **CR-W's conformance gates for `emulator/screen_text`'s `panel` surfaces** (§11.50), driven through the
//! shipped harvest: the spans `TabViewer::ui` records ([`crate::screen::PanelMark`]), the in-pass read
//! ([`crate::screen::painted`]), the surfaces ([`crate::screen::panels`]) and the reply the bus serves
//! (`Loop::publish_screen_text`, then `emulator/screen_text` through `Bus::call`).
//!
//! This file began as the CR-W Q3 spike (`crw_q3_spike.rs`, landed at `5a51da1`,
//! `docs/2026-09-17-cr-w-q3-spike.md`), which recorded spans through a test-only probe. **That probe is
//! gone.** The recording is production code now, and the spike's ground-truth gates run against it
//! unchanged in method. What is still test-only is exactly what a ground truth needs and production must
//! never do — [`hook`]: run no body, or only one, and plant extra text into a body.
//!
//! # Ground truth, and why it is independent of the harvest
//!
//! The truth for tab `T` is a **multiset difference of two whole `FullOutput`s**, taken after `end_pass`
//! has flattened every layer:
//!
//! `truth(T) = text(FullOutput with ONLY T's body executed) - text(FullOutput with NO body executed)`
//!
//! Both runs use the same `Loop`, the same dock, the same window size, the same input, the same number of
//! warm-up presents, each in a fresh `egui::Context`. [`hook`] suppresses a body by returning before its
//! `match` (the leaf, its tab strip, its frame and its scroll area are still drawn by `egui_dock`). That
//! instrument never looks at a layer id, a paint-list index or a clip rectangle, so it cannot agree with
//! the harvest by sharing a mistake with it. What it does share is egui and the bodies, which is the thing
//! under test. Two controls make the difference meaningful: **determinism** (two independent full runs
//! paint the identical text multiset) and **additivity** (the full run minus the no-body run equals the sum
//! of every `truth(T)`), so no body's text depends on another body having run.
//!
//! A text key is the galley's source string, its origin and its clip rectangle, quantised to 1/100 point,
//! so two equal labels in different places are two keys.
//!
//! # What replaced each spike test
//!
//! | Spike test (`5a51da1`) | Here |
//! |---|---|
//! | `every_drawn_tab_body_is_attributed_completely_and_exclusively_in_both_forms` | [`tests::every_drawn_tab_body_is_attributed_completely_and_exclusively`] — form 1 only, through the production spans; the drawn-set, disjointness, drain and anti-vacuity controls kept; W2 (names) added. The two controls that compared the probe's own read inside the body (`at_leave`, `layer_at_leave`) have no production counterpart and went with the probe. |
//! | `a_planted_interleaving_is_reported_foreign_in_both_forms` | `a_planted_interleaving_is_reported_foreign` |
//! | `an_open_combo_box_list_lands_in_its_own_layer_and_is_not_attributed` | same name, form 1 |
//! | `a_tooltip_lands_in_its_own_layer_and_is_not_attributed` | same name, form 1 |
//! | `text_edit_contents_and_hint_text_are_attributed_to_their_tab` | same name, form 1 |
//! | `a_floating_window_over_a_body_is_exact_in_form_1_and_leaks_into_that_body_in_form_2` | `a_floating_window_over_a_body_is_attributed_exactly_and_reported_after_the_main_surface` |
//! | `the_screen_overlay_in_a_pane_narrower_than_the_picture_is_attributed_in_both_forms` | `the_screen_overlay_in_a_pane_narrower_than_the_picture_is_attributed` |
//! | `a_discarded_first_pass_is_harvested_from_the_pass_that_is_kept` | same name; the kept-pass witness is now `FullOutput` itself rather than form 2 |
//! | `attribution_holds_at_the_scales_the_panel_is_drawn_at` | same name, form 1 |
//! | `harvest_cost_per_present` (ignored) | `w10_publish_cost_per_present` (ignored): the whole `publish_screen_text`, glyph probe and push included, plus one reply's serialisation |
//!
//! **Form 2** (attributing `FullOutput` shapes by clip rectangle) is not production and is not re-tested:
//! the spike measured it non-exclusive under a floating window, and its record stays at `5a51da1`.

use crate::screen::{Painted, PanelSpan};
use crate::ui::Tab;
use crate::{arm_for_measurement, symbols, Loop, Machine};
use egui::epaint::{ClippedShape, Shape};
use egui::{LayerId, Rect};
use std::collections::BTreeMap;
use std::time::Instant;

/// One painted text run, identified by what it says and exactly where and under which clip it was painted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Key {
    pub text: String,
    pub pos: (i64, i64),
    pub clip: [i64; 4],
}

fn q(v: f32) -> i64 {
    (v * 100.0).round() as i64
}

fn key_of(text: &str, pos: egui::Pos2, clip: Rect) -> Key {
    Key {
        text: text.to_owned(),
        pos: (q(pos.x), q(pos.y)),
        clip: [q(clip.min.x), q(clip.min.y), q(clip.max.x), q(clip.max.y)],
    }
}

/// Every `Shape::Text` in `shapes`, `Shape::Vec` walked, keyed. The ground truth's reader: it walks the
/// flattened `FullOutput`, never a paint list.
pub(crate) fn texts<'a>(shapes: impl IntoIterator<Item = &'a ClippedShape>) -> Vec<Key> {
    fn walk(s: &Shape, clip: Rect, out: &mut Vec<Key>) {
        match s {
            Shape::Text(t) => out.push(key_of(t.galley.text(), t.pos, clip)),
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in shapes {
        walk(&c.shape, c.clip_rect, &mut out);
    }
    out
}

/// The production harvest's shapes, keyed the same way as [`texts`].
fn keys_of(painted: &[Painted]) -> Vec<Key> {
    painted
        .iter()
        .map(|p| key_of(p.galley.text(), p.pos, p.clip))
        .collect()
}

/// Multiset `a - b`: what `a` holds that `b` does not, with multiplicity. Negative counts (in `b`, not
/// in `a`) are returned separately.
pub(crate) fn diff(a: &[Key], b: &[Key]) -> (Vec<Key>, Vec<Key>) {
    let mut m: BTreeMap<&Key, i64> = BTreeMap::new();
    for k in a {
        *m.entry(k).or_default() += 1;
    }
    for k in b {
        *m.entry(k).or_default() -= 1;
    }
    let (mut more, mut less) = (Vec::new(), Vec::new());
    for (k, n) in m {
        for _ in 0..n.max(0) {
            more.push(k.clone());
        }
        for _ in 0..(-n).max(0) {
            less.push(k.clone());
        }
    }
    (more, less)
}

/// **The attribution verdict for one tab.** `missing` is truth the harvest did not find (incomplete);
/// `foreign` is harvest the truth does not hold (not exclusive).
#[derive(Debug, Default)]
pub(crate) struct Verdict {
    pub missing: Vec<Key>,
    pub foreign: Vec<Key>,
}

impl Verdict {
    pub fn of(harvest: &[Key], truth: &[Key]) -> Self {
        let (foreign, missing) = diff(harvest, truth);
        Verdict { missing, foreign }
    }
    pub fn pass(&self) -> bool {
        self.missing.is_empty() && self.foreign.is_empty()
    }
}

/// The `Tab` whose title is `name`. Every span's name comes from `Tab::title`.
pub(crate) fn tab_of(name: &str) -> Tab {
    *Tab::ALL
        .iter()
        .find(|t| t.title() == name)
        .unwrap_or_else(|| panic!("no tab is titled {name:?}"))
}

/// **The only test-only code on the production path**: `TabViewer::ui` asks [`hook::suppressed`] before a
/// body and calls [`hook::plant`] after it, both under `cfg(test)`. Thread-local, so tests running in
/// parallel never see one another's hook, and a test that installs none gets the shipped behaviour.
pub(crate) mod hook {
    use super::*;
    use std::cell::RefCell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Mode {
        /// Every body runs (the shipped behaviour, and the harvest's arm).
        Record,
        /// Only this tab's body runs (the ground-truth arm).
        Only(Tab),
        /// No body runs (the ground-truth baseline).
        NoBodies,
    }

    /// Extra text a recorded body paints just before its span closes. Only ever in [`Mode::Record`].
    #[derive(Clone, Debug, PartialEq)]
    pub enum Plant {
        /// A painter string at the centre of the body's clip: the planted interleaving.
        Centre(String),
        /// A wrapping `ui.label` in a box `width` points wide.
        Label { text: String, width: f32 },
        /// A `Label::truncate` squeezed into a box `width` points wide, so the toolkit elides it.
        Truncated { text: String, width: f32 },
        /// A painter string starting `inside` points left of the clip's right edge, so it straddles it.
        Straddle { text: String, inside: f32 },
        /// A painter string wholly right of the clip.
        Outside(String),
        /// Two painter strings on ONE row at the same baseline, in the same font: `shown` ends inside the
        /// clip, `eaten` starts fifty points past its right edge. The clip eats the second one whole while
        /// the first one keeps their shared row on the glass — §11.50's total loss, on a reported row.
        Beside { shown: String, eaten: String },
        /// A painter string wholly BELOW the clip: laid out on no row the surface reports, which is the
        /// boundary the amended clause leaves where it was (`F-PANEL-SCROLL-UNSTATED`).
        Below(String),
    }

    struct Hook {
        mode: Mode,
        plant: Option<(Option<Tab>, Plant)>,
    }

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
        static CURRENT: RefCell<Option<Tab>> = const { RefCell::new(None) };
    }

    pub fn install(mode: Mode, plant: Option<(Option<Tab>, Plant)>) {
        HOOK.with(|h| *h.borrow_mut() = Some(Hook { mode, plant }));
    }

    pub fn uninstall() {
        HOOK.with(|h| *h.borrow_mut() = None);
    }

    /// `true`: do not run this body. Also notes which body is running, for [`plant`].
    pub fn suppressed(tab: Tab) -> bool {
        CURRENT.with(|c| *c.borrow_mut() = Some(tab));
        HOOK.with(|h| match h.borrow().as_ref().map(|h| h.mode) {
            None | Some(Mode::Record) => false,
            Some(Mode::NoBodies) => true,
            Some(Mode::Only(t)) => t != tab,
        })
    }

    pub fn plant(ui: &mut egui::Ui) {
        let tab = CURRENT.with(|c| *c.borrow());
        let plant = HOOK.with(|h| {
            h.borrow()
                .as_ref()
                .filter(|h| h.mode == Mode::Record)
                .and_then(|h| h.plant.clone())
        });
        let Some((only, plant)) = plant else {
            return;
        };
        if only.is_some() && only != tab {
            return;
        }
        let clip = ui.clip_rect();
        fn paint(ui: &egui::Ui, at: egui::Pos2, align: egui::Align2, s: String) {
            ui.painter().text(
                at,
                align,
                s,
                egui::FontId::monospace(12.0),
                egui::Color32::WHITE,
            );
        }
        match plant {
            Plant::Centre(s) => paint(ui, clip.center(), egui::Align2::CENTER_CENTER, s),
            Plant::Straddle { text, inside } => paint(
                ui,
                egui::pos2(clip.max.x - inside, clip.center().y),
                egui::Align2::LEFT_CENTER,
                text,
            ),
            Plant::Outside(s) => paint(
                ui,
                egui::pos2(clip.max.x + 50.0, clip.center().y),
                egui::Align2::LEFT_CENTER,
                s,
            ),
            Plant::Beside { shown, eaten } => {
                let y = clip.center().y;
                paint(
                    ui,
                    egui::pos2(clip.max.x - 80.0, y),
                    egui::Align2::RIGHT_CENTER,
                    shown,
                );
                paint(
                    ui,
                    egui::pos2(clip.max.x + 50.0, y),
                    egui::Align2::LEFT_CENTER,
                    eaten,
                );
            }
            Plant::Below(s) => paint(
                ui,
                egui::pos2(clip.min.x + 20.0, clip.max.y + 50.0),
                egui::Align2::LEFT_TOP,
                s,
            ),
            Plant::Label { text, width } => {
                ui.allocate_ui(egui::vec2(width, 200.0), |ui| {
                    ui.add(egui::Label::new(text).wrap());
                });
            }
            Plant::Truncated { text, width } => {
                ui.allocate_ui(egui::vec2(width, 20.0), |ui| {
                    ui.add(egui::Label::new(text).truncate());
                });
            }
        }
    }
}

/// What one measured present produced.
pub(crate) struct Present {
    /// Every text run in the finished `FullOutput`.
    pub out: Vec<Key>,
    /// The spans `build_ui` returned — the production recording.
    pub spans: Vec<PanelSpan>,
    /// The production in-pass read of each span, keyed.
    pub in_pass: Vec<(Tab, Vec<Key>)>,
    /// The same read, unkeyed, for the rows that inspect glyphs.
    pub painted: Vec<Vec<Painted>>,
    /// **The reply a client reads**: `Loop::publish_screen_text` in the pass, then `emulator/screen_text`
    /// through `Bus::call`.
    pub reply: serde_json::Value,
    /// Text living in layers other than the spans' own, read in the same place: where popups, tooltips
    /// and windows went.
    pub other_layers: Vec<(LayerId, Vec<Key>)>,
    /// The first span's layer's paint-list length read after `run_ui` returned, i.e. after `end_pass`.
    pub after_pass_len: Option<usize>,
    pub passes: u32,
}

impl Present {
    /// The reply's `panel` surfaces as `(name, text, rendered, truncated)`.
    pub fn panel_surfaces(&self) -> Vec<(String, String, String, bool)> {
        self.reply["surfaces"]
            .as_array()
            .expect("surfaces")
            .iter()
            .filter(|s| s["kind"] == "panel")
            .map(|s| {
                (
                    s["panel"]
                        .as_str()
                        .expect("a panel names itself")
                        .to_owned(),
                    s["text"].as_str().unwrap().to_owned(),
                    s["rendered"].as_str().unwrap().to_owned(),
                    s["truncated"].as_bool().unwrap(),
                )
            })
            .collect()
    }

    pub fn surface(&self, tab: Tab) -> serde_json::Value {
        self.reply["surfaces"]
            .as_array()
            .expect("surfaces")
            .iter()
            .find(|s| s["kind"] == "panel" && s["panel"] == tab.title())
            .unwrap_or_else(|| panic!("no panel surface for {tab:?}: {}", self.reply))
            .clone()
    }
}

pub(crate) const SIZE: egui::Vec2 = egui::vec2(1600.0, 1000.0);

pub(crate) fn raw(i: u32, events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        time: Some(f64::from(i) / 60.0),
        events,
        ..Default::default()
    }
}

pub(crate) fn context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::theme::install(&ctx, crate::theme::DEFAULT_FAMILY);
    ctx
}

/// One present of the real `build_ui` under `mode`, harvested by the production path.
pub(crate) fn present(
    lp: &mut Loop,
    ctx: &egui::Context,
    raw: egui::RawInput,
    mode: hook::Mode,
    plant: Option<(Option<Tab>, hook::Plant)>,
) -> Present {
    hook::install(mode, plant);
    let mut spans = Vec::new();
    let mut in_pass = Vec::new();
    let mut painted = Vec::new();
    let mut other_layers = Vec::new();
    let mut reply = serde_json::Value::Null;
    let mut out = ctx.run_ui(raw, |root| {
        let c = root.ctx().clone();
        // Exactly `Loop::iterate`'s sequence: build, then publish in the same pass.
        let (drew, drawn) = lp.build_ui(root, true);
        painted = crate::screen::painted(&c, &drawn);
        in_pass = drawn
            .iter()
            .zip(&painted)
            .map(|(s, p)| (tab_of(s.name), keys_of(p)))
            .collect();
        lp.publish_screen_text(&c, &drew, &drawn);
        reply = match lp.bus.call(
            lp.machine.system_mut(),
            "emulator/screen_text",
            &serde_json::json!({}),
        ) {
            crate::bus::Answer::Ok(v) => v,
            crate::bus::Answer::Err(e) => panic!("screen_text refused after a publish: {e:?}"),
        };
        let layers: Vec<LayerId> = c.memory(|m| m.layer_ids().collect());
        other_layers = layers
            .into_iter()
            .filter(|l| !drawn.iter().any(|s| s.layer == *l))
            .map(|l| {
                let t = c.graphics(|g| {
                    g.get(l)
                        .map(|pl| texts(pl.all_entries()))
                        .unwrap_or_default()
                });
                (l, t)
            })
            .filter(|(_, t)| !t.is_empty())
            .collect();
        spans = drawn;
    });
    out.textures_delta.clear();
    let after_pass_len = spans
        .first()
        .and_then(|s| ctx.graphics(|g| g.get(s.layer).map(|l| l.all_entries().len())));
    hook::uninstall();
    Present {
        out: texts(&out.shapes),
        spans,
        in_pass,
        painted,
        reply,
        other_layers,
        after_pass_len,
        passes: out.platform_output.num_completed_passes as u32,
    }
}

/// What an arm changes about its context or its hook, identically in every arm of one measurement.
#[derive(Default, Clone)]
pub(crate) struct Setup {
    /// Show a tooltip as soon as the pointer rests, instead of after egui's 0.5 s.
    pub tooltip_now: bool,
    /// See [`hook::Plant`]; `Some(tab)` plants into that body only. Applied only in the recording arm.
    pub plant: Option<(Option<Tab>, hook::Plant)>,
    /// Rebuild the dock at the start of every arm. A floating window's position is handed to egui once
    /// and then lives in the context's memory, so a dock reused across fresh contexts puts the window
    /// somewhere else on the second arm (measured by the spike: 28 runs moved).
    pub dock: Option<fn() -> egui_dock::DockState<Tab>>,
    /// Device pixels per point for every present of the arm; `None` is egui's default of 1.0.
    pub ppp: Option<f32>,
}

pub(crate) fn settled_with(
    lp: &mut Loop,
    mode: hook::Mode,
    warm: u32,
    script: &dyn Fn(u32) -> Vec<egui::Event>,
    setup: &Setup,
) -> Present {
    // The Planes tab counts its own repaints and prints the count, so an arm that followed another on
    // the same loop would differ by that count alone. Each arm starts the panel from its default, which
    // every arm then advances by the same `warm + 1` presents.
    lp.planes = crate::planes::Panel::default();
    if let Some(dock) = setup.dock {
        lp.dock = dock();
    }
    let ctx = context();
    if setup.tooltip_now {
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    }
    let raw_at = |i: u32| {
        let mut r = raw(i, script(i));
        if let Some(ppp) = setup.ppp {
            // The screen stays SIZE device pixels, so it is SIZE / ppp points.
            r.screen_rect = Some(Rect::from_min_size(egui::Pos2::ZERO, SIZE / ppp));
            let id = r.viewport_id;
            r.viewports
                .get_mut(&id)
                .expect("RawInput::default carries the root viewport")
                .native_pixels_per_point = Some(ppp);
        }
        r
    };
    let plant = || setup.plant.clone();
    for i in 0..warm {
        let _ = present(lp, &ctx, raw_at(i), mode, plant());
    }
    let p = present(lp, &ctx, raw_at(warm), mode, plant());
    if let Some(ppp) = setup.ppp {
        assert_eq!(ctx.pixels_per_point(), ppp, "the arm did not run at {ppp}");
    }
    p
}

/// **A listing that names an object pool**, so the Objects tab draws its table instead of its no-symbols
/// refusal (one paragraph, which would make its verdict a verdict about one run). The shape is
/// `objects::tests::pool_rows`': aeon's `Object_RAM` block, 2 player, 40 dynamic, 8 system and 16 effect
/// slots of `$50` bytes at `$FF8000`, and `ObjCodeBase` at `$10000`.
fn object_listing() -> oracle_core::symbols::SymbolTable {
    let (base, stride) = (0x00FF_8000u32, 0x50u32);
    let dynamic = base + 2 * stride;
    let system = dynamic + 40 * stride;
    let effect = system + 8 * stride;
    let rows = [
        ("Object_RAM", base),
        ("Player_1", base),
        ("Player_2", base + stride),
        ("Dynamic_Slots", dynamic),
        ("System_Slots", system),
        ("Effect_Slots", effect),
        ("Object_RAM_End", effect + 16 * stride),
        ("ObjCodeBase", 0x0001_0000),
    ];
    let mut s = String::from("  Symbol Table (* = unused):\n\n");
    for (name, addr) in rows {
        s.push_str(&format!(" {name} : {addr:X} C |\n"));
    }
    s.push_str(&format!("\n{:>4} symbols\n", rows.len()));
    oracle_core::symbols::SymbolTable::parse(&s).expect("a parsable listing")
}

/// The loop every gate measures: the fixture ROM, sixteen breakpoints, a RAM watch and the profiler armed,
/// and eight real iterations run so the panels have rows.
pub(crate) fn fixture(dock: egui_dock::DockState<Tab>) -> Loop {
    let mut machine = Machine::new(oracle_core::testrom::build(), None);
    machine.system_mut().set_pad(
        oracle_core::io::PadPort::P1,
        oracle_core::io::Pad::default(),
    );
    // ⚑ **A SYNTHETIC clock, and it is the reason two fixtures are comparable at all.**
    // `Loop::iterate` is the one place pacing figures are derived (`Loop::derive_pacing`), and what it
    // leaves in `Loop::pacing` is what the Pacing body then PRINTS: a rate, the window it was measured
    // over, two frame-time percentiles. Handed `Instant::now()` those figures are *this box's speed on
    // the day*, so two fixtures built moments apart print different strings, and any gate that compares
    // a present of one against a present of the other is comparing the machine's load. That is what took
    // CI red on `462e9cf` — see
    // `tests::a_run_on_no_reported_row_is_in_neither_string_and_changes_nothing`.
    //
    // Every figure in `PacingFacts` is a DIFFERENCE from the instant this loop started, never a reading
    // of the clock, so laying a nominal 60 Hz over the real iterations makes all of them identical on
    // every box: eight presents one `FRAME_PERIOD` apart, whatever the wall clock did in between. The
    // machine, the UI and the bus still run for real and still take as long as they take; only the
    // *reported* pacing is fixed. Nothing here touches production — `Loop::iterate` takes its `now` as an
    // argument precisely so the caller owns the clock, and the window's caller is still `Instant::now()`.
    let t0 = Instant::now();
    let mut lp = Loop::new(
        machine,
        t0,
        Some(0.0),
        String::from("(fixture)"),
        symbols::Loaded {
            table: Some(object_listing()),
            path: None,
            fatal: None,
        },
        None,
    );
    arm_for_measurement(&mut lp);
    let ctx = egui::Context::default();
    for i in 0..8 {
        let mut out = ctx.run_ui(raw(i, Vec::new()), |root| {
            let c = root.ctx().clone();
            lp.iterate(&c, root, t0 + crate::pacing::FRAME_PERIOD * (i + 1));
        });
        out.textures_delta.clear();
    }
    lp.dock = dock;
    lp
}

/// **Tab `t` in a leaf of 55% of the window, and every other tab in a leaf of its own, stacked in the
/// remaining column**: all eleven bodies run, and the one under test has room to draw its tables, its
/// inner scroll areas and its text boxes.
pub(crate) fn focus_dock(t: Tab) -> egui_dock::DockState<Tab> {
    let rest: Vec<Tab> = Tab::ALL.iter().copied().filter(|x| *x != t).collect();
    let mut dock = egui_dock::DockState::new(vec![t]);
    let s = dock.main_surface_mut();
    let [_, mut at] = s.split_right(egui_dock::NodeIndex::root(), 0.55, vec![rest[0]]);
    for (i, tab) in rest[1..].iter().enumerate() {
        // `at` holds one tab and must end up with an equal share of what is left below it.
        let below = (rest.len() - 1 - i) as f32;
        let [_, next] = s.split_below(at, 1.0 / (below + 1.0), vec![*tab]);
        at = next;
    }
    dock
}

/// The tabs whose bodies `egui_dock` will run for `dock`: the active tab of every leaf that is not
/// collapsed, main surface first. Derived from the dock, independently of the recording.
pub(crate) fn active_tabs(dock: &egui_dock::DockState<Tab>) -> Vec<Tab> {
    let mut out = Vec::new();
    for surface in dock.iter_surfaces() {
        for node in surface.iter_nodes() {
            if let egui_dock::Node::Leaf(leaf) = node {
                if !leaf.collapsed {
                    if let Some(t) = leaf.tabs.get(leaf.active.0) {
                        out.push(*t);
                    }
                }
            }
        }
    }
    out
}

/// The active tabs of the MAIN surface alone, in its own node order.
pub(crate) fn active_tabs_of_main(dock: &egui_dock::DockState<Tab>) -> Vec<Tab> {
    let mut out = Vec::new();
    for node in dock.main_surface().iter() {
        if let egui_dock::Node::Leaf(leaf) = node {
            if !leaf.collapsed {
                if let Some(t) = leaf.tabs.get(leaf.active.0) {
                    out.push(*t);
                }
            }
        }
    }
    out
}

/// One arrangement's measurement: the production harvest against the truth, for every drawn tab.
pub(crate) struct Measured {
    pub name: String,
    pub full: Present,
    pub rows: Vec<(Tab, usize, Verdict)>,
    pub determinism: (Vec<Key>, Vec<Key>),
    pub additivity: (Vec<Key>, Vec<Key>),
    pub base_minus_full: Vec<Key>,
}

pub(crate) fn measure(name: &str, dock: egui_dock::DockState<Tab>, warm: u32) -> Measured {
    measure_with(
        name,
        dock,
        warm,
        &|_| Vec::new(),
        &mut |_| {},
        &Setup::default(),
    )
}

pub(crate) fn measure_with(
    name: &str,
    dock: egui_dock::DockState<Tab>,
    warm: u32,
    script: &dyn Fn(u32) -> Vec<egui::Event>,
    prepare: &mut dyn FnMut(&mut Loop),
    setup: &Setup,
) -> Measured {
    let mut lp = fixture(dock);
    prepare(&mut lp);
    let full = settled_with(&mut lp, hook::Mode::Record, warm, script, setup);
    let again = settled_with(&mut lp, hook::Mode::Record, warm, script, setup);
    let base = settled_with(&mut lp, hook::Mode::NoBodies, warm, script, setup);
    let determinism = diff(&full.out, &again.out);
    let mut rows = Vec::new();
    let mut sum = Vec::new();
    for s in &full.spans {
        let tab = tab_of(s.name);
        let only = settled_with(&mut lp, hook::Mode::Only(tab), warm, script, setup);
        let (truth, _) = diff(&only.out, &base.out);
        let harvest = full
            .in_pass
            .iter()
            .find(|(t, _)| *t == tab)
            .map(|(_, k)| k.clone())
            .unwrap_or_default();
        rows.push((tab, truth.len(), Verdict::of(&harvest, &truth)));
        sum.extend(truth);
    }
    let (full_minus_base, base_minus_full) = diff(&full.out, &base.out);
    let additivity = diff(&full_minus_base, &sum);
    Measured {
        name: name.to_owned(),
        full,
        rows,
        determinism,
        additivity,
        base_minus_full,
    }
}

#[cfg(test)]
mod tests {
    use super::hook::{Mode, Plant};
    use super::*;
    use crate::screen::glass_run;

    /// Warm-up presents before the measured one. **Twelve, not four, and measured** (the spike): at four,
    /// the focus arrangement for Watchpoints painted its body clip at y max 991.2 with every body running
    /// and 992.5 with only its own, a scroll bar still animating in `egui_dock`'s outer `ScrollArea`; at
    /// eight 989.5 against 989.6; at twelve both 989.5. Every clip-based row below warms up by this much.
    const WARM: u32 = 12;

    fn arrangements() -> Vec<(String, egui_dock::DockState<Tab>)> {
        let mut v = vec![
            ("default".to_owned(), crate::ui::initial_dock()),
            ("every-tab".to_owned(), crate::ui::every_tab_dock()),
        ];
        for t in Tab::ALL {
            v.push((format!("focus {}", t.title()), focus_dock(t)));
        }
        v
    }

    /// The controls every measurement must pass before its verdicts mean anything.
    fn check_controls(name: &str, m: &Measured) {
        assert!(
            m.determinism.0.is_empty() && m.determinism.1.is_empty(),
            "{name}: two identical full runs painted different text: {:?}",
            m.determinism
        );
        assert!(
            m.additivity.0.is_empty() && m.additivity.1.is_empty() && m.base_minus_full.is_empty(),
            "{name}: the bodies' text is not the sum of each body alone: {:?} / {:?}",
            m.additivity,
            m.base_minus_full
        );
        assert_eq!(
            m.full.passes, 1,
            "{name}: the measured present was multi-pass"
        );
        assert!(!m.full.spans.is_empty(), "{name}: no body was recorded");
    }

    fn row(m: &Measured, tab: Tab) -> &(Tab, usize, Verdict) {
        m.rows
            .iter()
            .find(|r| r.0 == tab)
            .unwrap_or_else(|| panic!("{}: {tab:?} was not drawn", m.name))
    }

    fn span_texts(p: &Present, tab: Tab) -> &[Key] {
        &p.in_pass.iter().find(|(t, _)| *t == tab).expect("a span").1
    }

    fn painted_of(p: &Present, tab: Tab) -> &[Painted] {
        let i = p
            .spans
            .iter()
            .position(|s| s.name == tab.title())
            .unwrap_or_else(|| panic!("{tab:?} was not drawn"));
        &p.painted[i]
    }

    fn at(p: &Present, tab: Tab, text: &str) -> egui::Pos2 {
        let k = span_texts(p, tab)
            .iter()
            .find(|k| k.text == text)
            .unwrap_or_else(|| panic!("{tab:?} painted no {text:?}"));
        egui::pos2(k.pos.0 as f32 / 100.0, k.pos.1 as f32 / 100.0)
    }

    fn short(v: &Verdict) -> String {
        if v.pass() {
            "pass".into()
        } else {
            format!(
                "FAIL (missing {}, foreign {})",
                v.missing.len(),
                v.foreign.len()
            )
        }
    }

    /// **W3's check on one reply**: every panel surface has as many rows in `rendered` as in `text`, and
    /// the same number of runs on each row. Returns how many panel surfaces it checked.
    fn assert_aligned(what: &str, p: &Present) -> usize {
        let surfaces = p.panel_surfaces();
        for (name, text, rendered, _) in &surfaces {
            let (t, r): (Vec<&str>, Vec<&str>) =
                (text.split('\n').collect(), rendered.split('\n').collect());
            assert_eq!(
                t.len(),
                r.len(),
                "{what} {name}: rows differ between text and rendered\n{text:?}\n{rendered:?}"
            );
            for (k, (a, b)) in t.iter().zip(&r).enumerate() {
                assert_eq!(
                    a.matches('\t').count(),
                    b.matches('\t').count(),
                    "{what} {name}: row {k} has different run counts\n{a:?}\n{b:?}"
                );
            }
        }
        surfaces.len()
    }

    /// ★ **The attribution gate (the spike's Q3 gate, on the production path).** For the default dock,
    /// `--dock every-tab`'s arrangement and one focus arrangement per tab: every drawn body's text, as the
    /// production spans and in-pass read collect it, equals that tab's ground truth exactly.
    ///
    /// **Controls, each asserted before the verdict it makes meaningful:** determinism, additivity, the
    /// drawn set (the spans are exactly the leaves' active tabs, in draw order — W1's drawn-set half),
    /// one layer on the main surface, disjoint spans, the drain (the layer's paint list is empty once
    /// `run_ui` has returned, so the read MUST be in the pass), and anti-vacuity (every tab has text in its
    /// own focus arrangement). **W2**: the reply's panel names are exactly the drawn tabs' titles, in the
    /// same order. **W3**: every panel surface of every reply is aligned.
    #[test]
    fn every_drawn_tab_body_is_attributed_completely_and_exclusively() {
        let mut table: Vec<(Tab, usize, bool)> = Vec::new();
        let mut failures = Vec::new();
        let mut aligned = 0;
        for (name, dock) in arrangements() {
            let expect = active_tabs(&dock);
            let m = measure(&name, dock, WARM);
            check_controls(&name, &m);
            let got: Vec<Tab> = m.full.spans.iter().map(|s| tab_of(s.name)).collect();
            assert_eq!(got, expect, "{name}: the spans are a different drawn set");
            let names: Vec<String> = m.full.panel_surfaces().into_iter().map(|s| s.0).collect();
            let titles: Vec<&str> = expect.iter().map(|t| t.title()).collect();
            assert_eq!(
                names, titles,
                "{name}: W2, the reply's panel names are not the drawn tabs' titles in draw order"
            );
            aligned += assert_aligned(&name, &m.full);
            for s in &m.full.spans {
                assert_eq!(
                    s.layer, m.full.spans[0].layer,
                    "{name}: {} drew into a different layer from the first body",
                    s.name
                );
            }
            for w in m.full.spans.windows(2) {
                assert!(w[0].end <= w[1].start, "{name}: overlapping spans {w:?}");
            }
            assert_eq!(
                m.full.after_pass_len,
                Some(0),
                "{name}: the layer still held shapes after the pass, so the in-pass requirement is not \
                 what this gate says it is"
            );
            println!("--- {name}: {} bodies drawn", m.rows.len());
            for (tab, truth, v) in &m.rows {
                println!("  {:<12} truth {:>3}  {}", tab.title(), truth, short(v));
                if !v.pass() {
                    failures.push(format!("{name} {tab:?}: {v:?}"));
                }
                if name == format!("focus {}", tab.title()) {
                    table.push((*tab, *truth, v.pass()));
                }
            }
        }
        for t in Tab::ALL {
            let (_, truth, _) = table
                .iter()
                .find(|(x, _, _)| *x == t)
                .expect("a focus row per tab");
            assert!(
                *truth > 0,
                "{t:?} painted no text even with 55% of the window, so its verdict is vacuous"
            );
        }
        assert!(aligned > 100, "W3 checked only {aligned} panel surfaces");
        assert!(
            failures.is_empty(),
            "attribution failed:\n{}",
            failures.join("\n")
        );
    }

    /// ★ **The instrument can see an interleaving.** Every "nothing foreign" verdict above rests on an
    /// absence, so here one is planted: each recorded body paints one extra string into its own painter
    /// just before its span closes, and the ground-truth arms do not. The harvest must report it as
    /// foreign on every drawn tab, and as nothing else — and the reply's panel text must carry it.
    #[test]
    fn a_planted_interleaving_is_reported_foreign() {
        const PLANT: &str = "PLANTED CR-W INTERLOPER";
        let setup = Setup {
            plant: Some((None, Plant::Centre(PLANT.into()))),
            ..Setup::default()
        };
        let m = measure_with(
            "planted",
            crate::ui::initial_dock(),
            WARM,
            &|_| Vec::new(),
            &mut |_| {},
            &setup,
        );
        assert_eq!(m.rows.len(), 4, "the default dock draws four bodies");
        for (tab, truth, v) in &m.rows {
            assert!(*truth > 0, "{tab:?}: nothing to plant beside");
            assert!(v.missing.is_empty(), "{tab:?}: {:?}", v.missing);
            let foreign: Vec<&str> = v.foreign.iter().map(|k| k.text.as_str()).collect();
            assert_eq!(
                foreign,
                [PLANT],
                "{tab:?}: the plant was not reported foreign"
            );
            let s = m.full.surface(*tab);
            assert!(
                s["text"].as_str().unwrap().contains(PLANT),
                "{tab:?}: the planted run is not in the served text: {s}"
            );
        }
    }

    /// ★ **A combo box's open list is its own layer, and the harvest does not attribute it to the tab.**
    #[test]
    fn an_open_combo_box_list_lands_in_its_own_layer_and_is_not_attributed() {
        let dock = || focus_dock(Tab::Watchpoints);
        let mut lp = fixture(dock());
        let first = settled_with(
            &mut lp,
            Mode::Record,
            WARM,
            &|_| Vec::new(),
            &Setup::default(),
        );
        let combo =
            at(&first, Tab::Watchpoints, crate::stopping::WATCH_SPACES[0]) + egui::vec2(4.0, 4.0);
        let click = move |i: u32| match i {
            2 => vec![egui::Event::PointerMoved(combo)],
            3 | 4 => vec![egui::Event::PointerButton {
                pos: combo,
                button: egui::PointerButton::Primary,
                pressed: i == 3,
                modifiers: egui::Modifiers::NONE,
            }],
            _ => Vec::new(),
        };
        let m = measure_with(
            "combo open",
            dock(),
            WARM,
            &click,
            &mut |_| {},
            &Setup::default(),
        );
        check_controls("combo open", &m);
        let popup: Vec<Key> = m
            .full
            .other_layers
            .iter()
            .flat_map(|(_, t)| t.iter().cloned())
            .collect();
        let items: Vec<&str> = popup.iter().map(|k| k.text.as_str()).collect();
        for want in crate::stopping::WATCH_SPACES {
            assert!(
                items.contains(&want),
                "the combo list did not open into another layer (control): {items:?}"
            );
        }
        let (_, _, v) = row(&m, Tab::Watchpoints);
        assert!(v.foreign.is_empty(), "{:?}", v.foreign);
        let (a, b) = diff(&v.missing, &popup);
        assert!(
            a.is_empty() && b.is_empty(),
            "what the harvest missed is not exactly the popup: extra {a:?}, popup not missed {b:?}"
        );
        for (tab, _, v) in &m.rows {
            if *tab != Tab::Watchpoints {
                assert!(v.pass(), "{tab:?}: {v:?}");
            }
        }
    }

    /// ★ **A tooltip is its own layer too.**
    #[test]
    fn a_tooltip_lands_in_its_own_layer_and_is_not_attributed() {
        let dock = || focus_dock(Tab::Watchpoints);
        let mut lp = fixture(dock());
        let first = settled_with(
            &mut lp,
            Mode::Record,
            WARM,
            &|_| Vec::new(),
            &Setup::default(),
        );
        let hover = at(&first, Tab::Watchpoints, "write") + egui::vec2(4.0, 4.0);
        let rest = move |i: u32| {
            if i == 2 {
                vec![egui::Event::PointerMoved(hover)]
            } else {
                Vec::new()
            }
        };
        let setup = Setup {
            tooltip_now: true,
            ..Setup::default()
        };
        let m = measure_with("tooltip", dock(), WARM, &rest, &mut |_| {}, &setup);
        check_controls("tooltip", &m);
        let tip: Vec<Key> = m
            .full
            .other_layers
            .iter()
            .flat_map(|(_, t)| t.iter().cloned())
            .collect();
        assert!(
            tip.iter().any(|k| k.text.contains("BOOLEANS")),
            "no tooltip was shown, so this test measures nothing: {tip:?}"
        );
        let (_, _, v) = row(&m, Tab::Watchpoints);
        assert!(v.foreign.is_empty(), "{:?}", v.foreign);
        let (a, b) = diff(&v.missing, &tip);
        assert!(
            a.is_empty() && b.is_empty(),
            "what the harvest missed is not exactly the tooltip: extra {a:?}, tooltip not missed {b:?}"
        );
    }

    /// ★ **A text box's contents and its hint text are the tab's**, and both reach the served text.
    #[test]
    fn text_edit_contents_and_hint_text_are_attributed_to_their_tab() {
        const TYPED: &str = "crw typed into the watch target";
        let m = measure_with(
            "text edit",
            focus_dock(Tab::Watchpoints),
            WARM,
            &|_| Vec::new(),
            &mut |lp| lp.stopping.w_target = TYPED.into(),
            &Setup::default(),
        );
        check_controls("text edit", &m);
        let (_, _, v) = row(&m, Tab::Watchpoints);
        assert!(v.pass(), "{v:?}");
        let got = span_texts(&m.full, Tab::Watchpoints);
        for want in [TYPED, "∞"] {
            assert!(
                got.iter().any(|k| k.text == want),
                "{want:?} is not in the Watchpoints span"
            );
        }
        assert!(
            !got.iter().any(|k| k.text == "0xFF0000 / symbol"),
            "the target box shows its hint although it holds text, so the fixture did not type"
        );
        let text = m.full.surface(Tab::Watchpoints)["text"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            text.contains(TYPED) && text.contains('∞'),
            "the served panel text lacks the box contents or its hint: {text:?}"
        );
        // Spike correction 5: a TextEdit paints EMPTY galleys too, and they are not runs.
        assert!(
            got.iter().any(|k| k.text.is_empty()),
            "control: no empty galley was painted, so the exclusion below is untested"
        );
        let empty_runs = painted_of(&m.full, Tab::Watchpoints)
            .iter()
            .filter(|p| p.galley.text().is_empty())
            .filter_map(glass_run)
            .count();
        assert_eq!(empty_runs, 0, "an empty galley became a run");
    }

    /// ★ **Floating windows: attributed exactly, IN the drawn set, and reported main surface first, then
    /// the windows in the order the window drew them** — §11.50's three normative corrections, two of them
    /// here (the third, "an empty text shape is not a run", is in the TextEdit row).
    ///
    /// `egui_dock` draws a window surface's body in the window's own `Middle` layer; the span records that
    /// layer and the reader reads it. The spike measured that attributing by clip rectangle instead takes
    /// the window's whole body into the tab beneath it.
    ///
    /// **Two windows, not one**, because one window cannot tell "windows after the main surface" from
    /// "this window last": the served order is asserted against the dock's own surface order, which is
    /// derived independently of the recording (`active_tabs`), and the main-surface count is asserted so
    /// the partition itself is checked rather than inferred.
    #[test]
    fn a_floating_window_over_a_body_is_attributed_exactly_and_reported_after_the_main_surface() {
        fn dock() -> egui_dock::DockState<Tab> {
            let mut dock = crate::ui::initial_dock();
            for (tab, at) in [
                (Tab::Profiler, egui::pos2(100.0, 150.0)),
                (Tab::Objects, egui::pos2(700.0, 500.0)),
            ] {
                let w = dock.add_window(vec![tab]);
                dock.get_window_state_mut(w)
                    .expect("the window just added")
                    .set_position(at)
                    .set_size(egui::vec2(500.0, 400.0));
            }
            dock
        }
        let setup = Setup {
            dock: Some(dock),
            ..Setup::default()
        };
        let m = measure_with(
            "two windows over the dock",
            dock(),
            WARM,
            &|_| Vec::new(),
            &mut |_| {},
            &setup,
        );
        check_controls("two windows over the dock", &m);
        let spans = &m.full.spans;
        let screen = spans
            .iter()
            .find(|s| s.name == "Screen")
            .expect("Screen drawn");
        for name in ["Profiler", "Objects"] {
            let w = spans.iter().find(|s| s.name == name).unwrap_or_else(|| {
                panic!(
                    "{name}'s window body was not drawn, so it is not in the \
                     drawn set — §11.50 says it must be"
                )
            });
            assert_ne!(
                w.layer, screen.layer,
                "control: {name}'s window body shares the main layer, so the per-layer read is untested"
            );
        }
        // Control: a window really does cover part of Screen's body, so "nothing foreign" below is a
        // verdict about an overlap. Measured from the clip rectangles the runs were painted under, since
        // a production span carries no rectangle of its own.
        let area = |tab: Tab| {
            painted_of(&m.full, tab)
                .iter()
                .map(|p| p.clip)
                .reduce(|a, b| a.union(b))
                .unwrap_or_else(|| panic!("{tab:?} painted nothing"))
        };
        let (under, over) = (area(Tab::Screen), area(Tab::Profiler));
        assert!(
            under.intersects(over),
            "control: the window at {over:?} does not overlap Screen's body at {under:?}, so \
             exclusivity under a window is untested"
        );
        for (tab, truth, v) in &m.rows {
            assert!(*truth > 0, "{tab:?}: vacuous");
            assert!(v.pass(), "{tab:?}: {v:?}");
        }
        // The served order, against the dock's own surfaces (main first, then windows) — derived
        // independently of the spans.
        let names: Vec<String> = m.full.panel_surfaces().into_iter().map(|s| s.0).collect();
        let expect: Vec<String> = active_tabs(&dock())
            .iter()
            .map(|t| t.title().to_owned())
            .collect();
        assert_eq!(
            names, expect,
            "§11.50: main surface's panels, then the windows"
        );
        let main_count = active_tabs_of_main(&dock()).len();
        assert_eq!(
            main_count, 4,
            "the default dock draws four bodies on the main surface"
        );
        assert_eq!(
            names.len(),
            main_count + 2,
            "…and the two windows follow them: {names:?}"
        );
        assert_eq!(
            &names[main_count..],
            ["Profiler", "Objects"],
            "the windows follow in the order they were added and drawn, not tab order: {names:?}"
        );
    }

    /// **The Screen tab's picture overlay, painted through `Painter::with_clip_rect(picture)`, is the
    /// tab's** even when the pane is narrower than the picture.
    #[test]
    fn the_screen_overlay_in_a_pane_narrower_than_the_picture_is_attributed() {
        fn dock() -> egui_dock::DockState<Tab> {
            let mut dock = egui_dock::DockState::new(vec![Tab::Screen]);
            dock.main_surface_mut().split_right(
                egui_dock::NodeIndex::root(),
                0.12,
                vec![Tab::Registers],
            );
            dock
        }
        let m = measure_with(
            "narrow Screen, ring placement armed",
            dock(),
            WARM,
            &|_| Vec::new(),
            &mut |lp| lp.screen.arm_rings(),
            &Setup::default(),
        );
        check_controls("narrow Screen", &m);
        for (tab, truth, v) in &m.rows {
            assert!(*truth > 0 && v.pass(), "{tab:?}: {v:?}");
        }
        let notice = span_texts(&m.full, Tab::Screen)
            .iter()
            .find(|k| k.text.starts_with("ring placement armed"))
            .cloned()
            .expect("the armed chip on the picture");
        assert!(
            notice.clip[2] - notice.clip[0] > 0,
            "control: the notice has a clip: {notice:?}"
        );
    }

    /// **A present egui runs twice is harvested from the pass it keeps.** A fresh context's first present
    /// is two passes (the spike measured `[2, 1, 1, ...]`). The span list is a local of `build_ui`, so the
    /// discarded pass's spans cannot ride along; here the spans are the drawn set once, and every span's
    /// in-pass text is text egui kept.
    #[test]
    fn a_discarded_first_pass_is_harvested_from_the_pass_that_is_kept() {
        let dock = crate::ui::initial_dock();
        let expect = active_tabs(&dock);
        let mut lp = fixture(dock);
        lp.planes = crate::planes::Panel::default();
        let ctx = context();
        let p = present(&mut lp, &ctx, raw(0, Vec::new()), Mode::Record, None);
        assert_eq!(p.passes, 2, "control: the first present was not multi-pass");
        let got: Vec<Tab> = p.spans.iter().map(|s| tab_of(s.name)).collect();
        assert_eq!(got, expect, "spans from more than the kept pass");
        assert_eq!(
            p.panel_surfaces().len(),
            expect.len(),
            "the reply carries panels from more than the kept pass"
        );
        for (tab, keys) in &p.in_pass {
            assert!(!keys.is_empty(), "{tab:?}: vacuous");
            let (stray, _) = diff(keys, &p.out);
            assert!(
                stray.is_empty(),
                "{tab:?}: harvested text egui did not keep: {stray:?}"
            );
        }
    }

    /// **The verdict does not depend on the display scale** (1.25 and 2.0 device pixels per point).
    #[test]
    fn attribution_holds_at_the_scales_the_panel_is_drawn_at() {
        fn every_tab() -> egui_dock::DockState<Tab> {
            crate::ui::every_tab_dock()
        }
        fn default() -> egui_dock::DockState<Tab> {
            crate::ui::initial_dock()
        }
        for ppp in [1.25_f32, 2.0] {
            for (name, dock) in [("default", default as fn() -> _), ("every-tab", every_tab)] {
                let setup = Setup {
                    ppp: Some(ppp),
                    dock: Some(dock),
                    ..Setup::default()
                };
                let name = format!("{name} at {ppp}");
                let m = measure_with(&name, dock(), WARM, &|_| Vec::new(), &mut |_| {}, &setup);
                check_controls(&name, &m);
                assert_eq!(
                    m.rows.len(),
                    active_tabs(&dock()).len(),
                    "{name}: drawn set"
                );
                let text: usize = m.rows.iter().map(|r| r.1).sum();
                assert!(text > 0, "{name}: vacuous");
                assert_aligned(&name, &m.full);
                for (tab, _, v) in &m.rows {
                    assert!(v.pass(), "{name} {tab:?}: {v:?}");
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------------------
    // CR-W §9 conformance rows W1, W4-W8 (W2 and W3 are asserted by the attribution gate above; W9 was
    // rider R1, DROPPED by the hub's ruling; W10 is the ignored cost harness at the bottom)
    // ---------------------------------------------------------------------------------------------------

    /// One settled present of `lp` under its own dock, no ground truth: the rows below assert on the
    /// reply, not on attribution.
    fn one(lp: &mut Loop, setup: &Setup) -> Present {
        settled_with(lp, Mode::Record, WARM, &|_| Vec::new(), setup)
    }

    fn leaf_with(dock: &mut egui_dock::DockState<Tab>, tab: Tab) -> &mut egui_dock::LeafNode<Tab> {
        dock.main_surface_mut()
            .iter_mut()
            .find_map(|n| match n {
                egui_dock::Node::Leaf(l) if l.tabs.contains(&tab) => Some(l),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no leaf holds {tab:?}"))
    }

    /// ★ **W1, the drawn set** (§8 item 31's recipe, in-process). Registers, Memory and Objects share a
    /// leaf in the default dock: exactly the active one has a surface; making another active swaps them;
    /// collapsing the leaf removes all three and the reply still succeeds, with the other leaves' panels.
    ///
    /// *Anti-vacuity* (item 31's): the first read carries a non-empty panel text, or the host reports no
    /// panels at all and the swap proves nothing.
    #[test]
    fn w1_a_panel_surface_exists_exactly_when_its_body_is_drawn() {
        let mut lp = fixture(crate::ui::initial_dock());
        let shared = [Tab::Registers, Tab::Memory, Tab::Objects];
        let names =
            |p: &Present| -> Vec<String> { p.panel_surfaces().into_iter().map(|s| s.0).collect() };
        let first = one(&mut lp, &Setup::default());
        let n1 = names(&first);
        assert!(
            first.panel_surfaces().iter().any(|s| !s.1.is_empty()),
            "anti-vacuity: no panel text in the first read"
        );
        let in_leaf = |n: &[String]| -> Vec<String> {
            n.iter()
                .filter(|x| shared.iter().any(|t| t.title() == x.as_str()))
                .cloned()
                .collect()
        };
        assert_eq!(
            in_leaf(&n1),
            ["Registers"],
            "exactly the active tab: {n1:?}"
        );

        let leaf = leaf_with(&mut lp.dock, Tab::Memory);
        let memory = leaf.tabs.iter().position(|t| *t == Tab::Memory).unwrap();
        leaf.set_active_tab(memory).expect("Memory is in this leaf");
        let second = one(&mut lp, &Setup::default());
        let n2 = names(&second);
        assert_eq!(in_leaf(&n2), ["Memory"], "the names swap: {n2:?}");
        assert_eq!(n1.len(), n2.len(), "and nothing else moves");

        leaf_with(&mut lp.dock, Tab::Memory).collapsed = true;
        let third = one(&mut lp, &Setup::default());
        let n3 = names(&third);
        assert!(
            in_leaf(&n3).is_empty(),
            "a collapsed pane has no panel surface: {n3:?}"
        );
        assert_eq!(
            n3.len(),
            n1.len() - 1,
            "the other leaves still report: {n3:?}"
        );
        assert_eq!(
            third.reply["total"],
            serde_json::json!(2 + n3.len()),
            "the reply succeeded and counts the bar's two surfaces plus the drawn panels"
        );
    }

    /// ★ **W4, no false truncation, on every run of every real body**, plus a planted wrapped label and a
    /// planted TAB. A run whose galley is not elided and whose glyphs ALL meet the clip must have
    /// `rendered == text`. *Anti-vacuity:* the wrapped label occupies at least two glyph rows, and the
    /// sweep covers hundreds of runs.
    ///
    /// ⚑ **The whole population is counted as well as the unelided share, and the reason is worth keeping.**
    /// A label the toolkit truncates is ELIDED, which is exactly the population this row excludes, so
    /// `F-PANEL-TEXT-CUT-UNMARKED`'s treatment could in principle hollow this row out and leave it green.
    /// It nearly did: the first form of the fix put [`crate::ui::fitted_label`] under BOTH columns of the
    /// fact grids and this count fell 322 → 270. That turned out not to be the treatment working — it was
    /// the grid-column ratchet described in `fitted_label`, collapsing the label column to a bare `…`. The
    /// count is 322 again with the ratchet gone, plus 5 whole elided runs. The floor on the unelided share
    /// is still 300; the floor on the whole is what would have caught a real hollowing out.
    #[test]
    fn w4_an_unelided_unclipped_run_renders_exactly_its_source() {
        const WRAPPED: &str =
            "a label long enough that the pane it is planted in has to wrap it onto \
                               a second glyph row, and wrapping is not truncation";
        const TABBED: &str = "a\trun\twith\ttabs";
        let mut checked = 0usize;
        let mut elided_whole = 0usize;
        let mut wrapped_rows = 0usize;
        let mut tabbed_seen = false;
        for (name, dock, plant) in [
            (
                "focus Spawn + wrapped label",
                focus_dock(Tab::Spawn),
                Some((
                    Some(Tab::Spawn),
                    Plant::Label {
                        text: WRAPPED.into(),
                        width: 160.0,
                    },
                )),
            ),
            (
                "focus Spawn + tabbed label",
                focus_dock(Tab::Spawn),
                Some((
                    Some(Tab::Spawn),
                    Plant::Label {
                        text: TABBED.into(),
                        width: 400.0,
                    },
                )),
            ),
            ("every-tab", crate::ui::every_tab_dock(), None),
            ("default", crate::ui::initial_dock(), None),
        ] {
            let mut lp = fixture(dock);
            let p = one(
                &mut lp,
                &Setup {
                    plant,
                    ..Setup::default()
                },
            );
            for (span, painted) in p.spans.iter().zip(&p.painted) {
                for g in painted {
                    let Some(run) = glass_run(g) else { continue };
                    let all_visible = g.galley.rows.iter().all(|row| {
                        row.glyphs.iter().all(|gl| {
                            let r = gl
                                .logical_rect()
                                .translate(g.pos.to_vec2() + row.pos.to_vec2());
                            g.clip.contains_rect(r)
                        })
                    });
                    if !all_visible {
                        continue;
                    }
                    if run.elided {
                        // The toolkit cut it and said so: not this row's business, but counted, so the
                        // population below cannot shrink unnoticed.
                        elided_whole += 1;
                        continue;
                    }
                    checked += 1;
                    assert_eq!(
                        run.rendered, run.text,
                        "{name} {}: a whole, unelided run is reported cut",
                        span.name
                    );
                    if g.galley.text() == WRAPPED {
                        wrapped_rows = g.galley.rows.len();
                    }
                    if g.galley.text() == TABBED {
                        tabbed_seen = true;
                        assert_eq!(run.text, "a run with tabs", "the TAB fold");
                    }
                }
            }
            assert_aligned(name, &p);
        }
        assert!(
            wrapped_rows >= 2,
            "anti-vacuity: the planted label occupied {wrapped_rows} glyph row(s), so no wrap was tested"
        );
        assert!(
            tabbed_seen,
            "anti-vacuity: the tabbed run was never checked"
        );
        assert!(checked > 300, "anti-vacuity: only {checked} runs checked");
        assert!(
            checked + elided_whole > 300,
            "anti-vacuity: {checked} unelided + {elided_whole} elided whole runs, so the population this \
             row draws from shrank rather than merely splitting"
        );
        println!(
            "W4: {checked} whole unelided runs, rendered == text ({elided_whole} whole runs the toolkit \
             elided, not this row's arm); wrapped label on {wrapped_rows} rows"
        );
    }

    /// ★ **W5, elision.** A label squeezed until `Galley::elided` is true yields `truncated: true`, and its
    /// run in `rendered` ends in the elision mark. The oracle is the galley's own `elided`, never a
    /// width predicted before drawing. Also swept over every real body: every elided run ends in `…` and
    /// differs from its source.
    #[test]
    fn w5_an_elided_run_is_truncated_and_ends_in_the_elision_mark() {
        const LONG: &str = "a cell whose text is far wider than the forty points it is given";
        let mut lp = fixture(focus_dock(Tab::Registers));
        let p = one(
            &mut lp,
            &Setup {
                plant: Some((
                    Some(Tab::Registers),
                    Plant::Truncated {
                        text: LONG.into(),
                        width: 40.0,
                    },
                )),
                ..Setup::default()
            },
        );
        let g = painted_of(&p, Tab::Registers)
            .iter()
            .find(|g| g.galley.text() == LONG)
            .expect("the planted cell was painted");
        assert!(
            g.galley.elided,
            "control: the toolkit did not elide the squeezed cell"
        );
        let run = glass_run(g).expect("on the glass");
        assert!(run.rendered.ends_with('…'), "{:?}", run.rendered);
        assert_ne!(run.rendered, run.text);
        assert_eq!(run.text, LONG, "text carries the whole source");
        let s = p.surface(Tab::Registers);
        assert_eq!(s["truncated"], serde_json::json!(true), "{s}");
        let (text, rendered) = (s["text"].as_str().unwrap(), s["rendered"].as_str().unwrap());
        let ti = text
            .split('\n')
            .position(|r| r.split('\t').any(|x| x == LONG));
        let ri = ti.map(|k| rendered.split('\n').nth(k).unwrap());
        assert!(
            ri.is_some_and(|r| r.split('\t').any(|x| x.ends_with('…'))),
            "the cut run is found at the same row in rendered: {text:?} / {rendered:?}"
        );

        // The real half: the stopping tables' cells are `Label::truncate`d (`ui.rs` `table_cell`), so a
        // narrow pane elides them. Measured while writing this row: the every-tab and default docks at
        // 1600x1000 elide NO run at all, so a sweep of those alone asserted nothing; hence the narrow
        // panes, and the count is asserted rather than printed.
        let narrow = |t: Tab| {
            let mut dock = egui_dock::DockState::new(vec![Tab::Screen]);
            dock.main_surface_mut()
                .split_right(egui_dock::NodeIndex::root(), 0.8, vec![t]);
            dock
        };
        let mut elided = 0;
        for (name, dock) in [
            ("narrow Breakpoints", narrow(Tab::Breakpoints)),
            ("narrow Watchpoints", narrow(Tab::Watchpoints)),
            ("narrow Profiler", narrow(Tab::Profiler)),
            ("every-tab", crate::ui::every_tab_dock()),
        ] {
            let mut lp = fixture(dock);
            let p = one(&mut lp, &Setup::default());
            for painted in &p.painted {
                for g in painted {
                    let Some(run) = glass_run(g).filter(|r| r.elided) else {
                        continue;
                    };
                    let whole = g
                        .galley
                        .rows
                        .iter()
                        .flat_map(|row| {
                            row.glyphs.iter().map(move |gl| {
                                gl.logical_rect()
                                    .translate(g.pos.to_vec2() + row.pos.to_vec2())
                            })
                        })
                        .all(|r| g.clip.contains_rect(r));
                    if whole {
                        elided += 1;
                        // ⚑ A ONE-row run ends in the mark. A wrapped paragraph the PANE cut carries the
                        // mark on every row the pane cut (`crate::cut_mark`, `d-54` mark-only), and its last
                        // row may be one the pane did not cut, so for it the mark is inside `rendered`
                        // rather than at its end. Before 2026-09-25 only the toolkit elided, and it elides
                        // only at the end, so `ends_with` held for every run; the rule did not change, the
                        // glass did.
                        if g.galley.rows.len() == 1 {
                            assert!(
                                run.rendered.ends_with('\u{2026}'),
                                "{name}: an elided, unclipped run does not end in the mark: {run:?}"
                            );
                        } else {
                            assert!(
                                run.rendered.contains('\u{2026}'),
                                "{name}: an elided, unclipped paragraph carries no mark: {run:?}"
                            );
                        }
                        assert_ne!(run.rendered, run.text, "{name}: {run:?}");
                    }
                }
            }
            assert_aligned(name, &p);
        }
        assert!(
            elided > 0,
            "anti-vacuity: no real body elided an unclipped run, so the real half measured nothing"
        );
        println!("W5: {elided} real elided, unclipped runs in narrow stopping panes");
    }

    /// ★ **W6, clipping.** A run half outside its clip appears in `text` whole and in `rendered` as the
    /// glyphs on the glass; a run wholly outside its clip, on a row the pane reports, appears in `text`
    /// whole and in `rendered` as nothing.
    ///
    /// ⚑ **The second half USED TO ASSERT THE OPPOSITE** — *in neither string* — which was §11.50's text
    /// before the 2026-09-18 amendment (`F-PANEL-CLIP-TOTAL-LOSS-UNSIGNALLED`), and it is the defect itself:
    /// this plant lands on the `governor / the loop's own rate limiter` row, a row the pane DOES show, and
    /// the run the clip ate there was in neither string, the two compared equal and `truncated` derived
    /// false. The plant did not move; the rule did. What is genuinely absent is a run on a row the surface
    /// does not report, and that is asserted by `a_run_on_no_reported_row_is_in_neither_string_and_changes_nothing`
    /// below — with the control this row cannot have, that nothing shown shares its row.
    #[test]
    fn w6_a_clipped_run_is_whole_in_text_and_a_wholly_clipped_one_is_empty_in_rendered() {
        const STRADDLE: &str = "STRADDLING THE RIGHT EDGE OF THE PANE";
        const OUTSIDE: &str = "WHOLLY OUTSIDE THE PANE";
        let straddle_setup = Setup {
            plant: Some((
                Some(Tab::Pacing),
                Plant::Straddle {
                    text: STRADDLE.into(),
                    inside: 40.0,
                },
            )),
            ..Setup::default()
        };
        // ⚑ **Two arms since `d-54` was ruled mark-only (2026-09-25).** The straddle sits on the pane's
        // edge, so the window now marks it (`crate::cut_mark`) and the glyphs on the glass are a prefix
        // FOLLOWED BY the mark. §11.50 did not change — `rendered` is still the glyphs on the glass — the
        // glass did. So the harvest's own clipping reading is proven where nothing re-lays the glass (the
        // control arm, marking off: exactly this row's old assertion), and the window's glass is pinned
        // beside it.
        let mut lp = fixture(crate::ui::initial_dock());
        let bare = crate::cut_mark::with_marking_off(|| one(&mut lp, &straddle_setup));
        let g = painted_of(&bare, Tab::Pacing)
            .iter()
            .find(|g| g.galley.text() == STRADDLE)
            .expect("control: the straddling run was painted into the span");
        let run = glass_run(g).expect("part of it is on the glass");
        assert_eq!(run.text, STRADDLE);
        assert!(
            !run.rendered.is_empty()
                && STRADDLE.starts_with(&run.rendered)
                && run.rendered != STRADDLE,
            "rendered is the visible prefix: {:?}",
            run.rendered
        );
        let s = bare.surface(Tab::Pacing);
        assert!(s["text"].as_str().unwrap().contains(STRADDLE));
        assert!(!s["rendered"].as_str().unwrap().contains(STRADDLE));
        assert_eq!(s["truncated"], serde_json::json!(true));
        assert_aligned("straddle, marking off", &bare);

        let straddle = one(&mut lp, &straddle_setup);
        let g = painted_of(&straddle, Tab::Pacing)
            .iter()
            .find(|g| g.galley.text() == STRADDLE)
            .expect("control: the straddling run was painted into the span");
        let run = glass_run(g).expect("part of it is on the glass");
        assert_eq!(run.text, STRADDLE, "text carries the whole source");
        let kept = run.rendered.strip_suffix('\u{2026}').unwrap_or_else(|| {
            panic!(
                "the window's glass ends the cut run in the mark: {:?}",
                run.rendered
            )
        });
        assert!(
            !kept.is_empty() && STRADDLE.starts_with(kept) && kept != STRADDLE,
            "rendered is a visible prefix and the mark: {:?}",
            run.rendered
        );
        let s = straddle.surface(Tab::Pacing);
        assert!(s["text"].as_str().unwrap().contains(STRADDLE));
        assert!(!s["rendered"].as_str().unwrap().contains(STRADDLE));
        assert_eq!(s["truncated"], serde_json::json!(true));
        assert_aligned("straddle", &straddle);

        let outside = one(
            &mut lp,
            &Setup {
                plant: Some((Some(Tab::Pacing), Plant::Outside(OUTSIDE.into()))),
                ..Setup::default()
            },
        );
        let g = painted_of(&outside, Tab::Pacing)
            .iter()
            .find(|g| g.galley.text() == OUTSIDE)
            .expect("control: the outside run WAS painted into the span");
        assert!(
            !laid(g).touches,
            "control: a glyph of it reaches the clip, so this is the straddle above and not total loss"
        );
        let run = glass_run(g).expect("the toolkit laid it out, so it is a run");
        assert_eq!((run.text.as_str(), run.rendered.as_str()), (OUTSIDE, ""));
        let s = outside.surface(Tab::Pacing);
        let (text, rendered) = (s["text"].as_str().unwrap(), s["rendered"].as_str().unwrap());
        assert!(runs_of(text).contains(&OUTSIDE), "{s}");
        assert!(!runs_of(rendered).contains(&OUTSIDE), "{s}");
        assert_eq!(
            s["truncated"],
            serde_json::json!(true),
            "the cut a client could not see before the amendment: {s}"
        );
        assert_aligned("outside", &outside);
    }

    // ---------------------------------------------------------------------------------------------------
    // F-PANEL-CLIP-TOTAL-LOSS-UNSIGNALLED: a run the clip ate WHOLE is in `text` with an empty `rendered`
    // (§11.50 as amended 2026-09-18), and the row it lands on is derived, not guessed
    // ---------------------------------------------------------------------------------------------------

    /// One painted galley's geometry, read **straight off the galley** and never through `glass_run`, so
    /// these rows can disagree with the harvest instead of sharing a mistake with it. `row_cuts` above is
    /// the same discipline for the right-edge gate.
    ///
    /// The three readings are deliberately one-sided, so a glyph that sits exactly on an edge lands in
    /// none of them: `visible_bands` wants a glyph **wholly inside** the clip, `touches` wants any overlap
    /// at all, and a galley that is neither is left unasserted by the sweep rather than guessed at.
    struct Laid {
        /// The source as both strings carry it: the run's own TAB and LF folded to a space (§11.50 Q7),
        /// spelled out here rather than borrowed from `screen.rs`.
        source: String,
        /// `(top, bottom)` of the first row the toolkit laid a glyph out on, whatever the clip kept.
        band: Option<(f32, f32)>,
        /// `(top, bottom)` of every row with a glyph wholly inside the clip.
        visible_bands: Vec<(f32, f32)>,
        /// Any glyph overlaps the clip at all, however little.
        touches: bool,
    }

    fn laid(p: &Painted) -> Laid {
        let origin = p.pos.to_vec2();
        let (mut band, mut visible_bands, mut touches) = (None, Vec::new(), false);
        for row in &p.galley.rows {
            let rr = row.rect().translate(origin);
            if band.is_none() && !row.glyphs.is_empty() {
                band = Some((rr.min.y, rr.max.y));
            }
            let mut seen = false;
            for g in &row.glyphs {
                let r = g.logical_rect().translate(origin + row.pos.to_vec2());
                if r.width() > 0.0 && r.height() > 0.0 && p.clip.contains_rect(r) {
                    seen = true;
                }
                if r.max.x > p.clip.min.x
                    && r.min.x < p.clip.max.x
                    && r.max.y > p.clip.min.y
                    && r.min.y < p.clip.max.y
                {
                    touches = true;
                }
            }
            if seen {
                visible_bands.push((rr.min.y, rr.max.y));
            }
        }
        Laid {
            source: p.galley.text().replace(['\t', '\n'], " "),
            band,
            visible_bands,
            touches,
        }
    }

    /// Every run of `text`, in row-major order: the rows a client reads, split by the joins the server made.
    fn runs_of(text: &str) -> Vec<&str> {
        text.split('\n').flat_map(|row| row.split('\t')).collect()
    }

    /// ★ **A run the clip ate WHOLE is in `text`, with an empty run in its place in `rendered`, on the row
    /// it shares with the run that WAS shown** (`F-PANEL-CLIP-TOTAL-LOSS-UNSIGNALLED`, §11.50 as amended
    /// 2026-09-18). Before the fix both strings omitted it, they compared equal, and `truncated` derived
    /// FALSE for the cut that loses everything.
    ///
    /// **The row is derived, not guessed, and this row proves the derivation rather than the outcome.** The
    /// two planted strings are painted in the same font at the same baseline, so the toolkit lays them out
    /// on rows with the SAME band — asserted here off the galleys, independently of `screen.rs` — and one is
    /// inside the clip while the other starts fifty points past its right edge. So the answer to *which row*
    /// is a fact about the layout, and the assertions below are written from the plant, not from the
    /// harvest: row *k* of `text` carries `shown` then `eaten`, and row *k* of `rendered` carries `shown`
    /// then an EMPTY run at the same index.
    ///
    /// *Controls, each before the verdict it makes meaningful:* both galleys were painted into the span;
    /// the shown one is whole on the glass (so their row is reported); not one glyph of the eaten one
    /// touches the clip (so this is total loss and not the straddle W6 covers).
    #[test]
    fn a_run_the_clip_ate_whole_is_in_text_with_an_empty_rendered_on_its_row() {
        const SHOWN: &str = "SHOWN INSIDE THE PANE";
        const EATEN: &str = "EATEN BY THE RIGHT EDGE";
        let mut lp = fixture(crate::ui::initial_dock());
        let p = one(
            &mut lp,
            &Setup {
                plant: Some((
                    Some(Tab::Pacing),
                    Plant::Beside {
                        shown: SHOWN.into(),
                        eaten: EATEN.into(),
                    },
                )),
                ..Setup::default()
            },
        );
        let find = |t: &str| -> &Painted {
            painted_of(&p, Tab::Pacing)
                .iter()
                .find(|g| g.galley.text() == t)
                .unwrap_or_else(|| panic!("control: {t:?} was not painted into the span"))
        };
        let (shown, eaten) = (find(SHOWN), find(EATEN));
        let (ls, le) = (laid(shown), laid(eaten));
        assert!(
            !ls.visible_bands.is_empty(),
            "control: the shown string is not on the glass, so the row is not reported and this row \
             measures the scroll boundary instead of the clip"
        );
        assert!(
            !le.touches,
            "control: a glyph of the eaten string touches the clip, so this is a straddle (W6) and not \
             total loss"
        );
        let band = le.band.expect("control: the eaten string was laid out");
        assert!(
            ls.visible_bands
                .iter()
                .any(|b| (b.0 - band.0).abs() < 0.01 && (b.1 - band.1).abs() < 0.01),
            "control: the plant did not put the two strings on one row after all: shown {:?} vs eaten \
             {band:?}",
            ls.visible_bands
        );

        let run =
            glass_run(eaten).expect("a run the toolkit laid out is a run whatever the clip kept");
        assert_eq!(run.text, EATEN, "the source is whole in the run");
        assert_eq!(run.rendered, "", "and nothing of it reached the glass");
        assert_eq!(
            glass_run(shown).expect("on the glass").rendered,
            SHOWN,
            "control: the shown run is whole, so a difference below is the eaten one's"
        );

        let s = p.surface(Tab::Pacing);
        let (text, rendered) = (s["text"].as_str().unwrap(), s["rendered"].as_str().unwrap());
        assert_eq!(s["truncated"], serde_json::json!(true), "{s}");
        let k = text
            .split('\n')
            .position(|row| row.split('\t').any(|x| x == SHOWN))
            .expect("the shown run is a run of some row of text");
        let (trow, rrow) = (
            text.split('\n')
                .nth(k)
                .unwrap()
                .split('\t')
                .collect::<Vec<_>>(),
            rendered
                .split('\n')
                .nth(k)
                .expect("rendered has the same rows as text")
                .split('\t')
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            trow.len(),
            rrow.len(),
            "row {k} lost the alignment the whole section rests on:\n{trow:?}\n{rrow:?}"
        );
        let j = trow
            .iter()
            .position(|x| *x == EATEN)
            .unwrap_or_else(|| panic!("the eaten run is not on the shown run's row: {trow:?}"));
        assert_eq!(
            rrow[j], "",
            "row {k} run {j} of rendered is the glass rendering of row {k} run {j} of text, and the glass \
             got none of it: {rrow:?}"
        );
        let i = trow
            .iter()
            .position(|x| *x == SHOWN)
            .expect("the shown run is on the row this index came from");
        assert!(
            i < j,
            "the plant painted the shown string left of the eaten one, so the row orders them that way: \
             {trow:?}"
        );
        assert_eq!(
            rrow[i], SHOWN,
            "and the shown run rendered whole in its own place: {rrow:?}"
        );
        assert_aligned("beside", &p);
    }

    /// ★ **The boundary, and it is the load-bearing half: a run on NO reported row is in neither string**
    /// (`F-PANEL-SCROLL-UNSTATED`, unchanged by the total-loss ruling). A string painted below the clip
    /// changes the surface **not at all** — asserted against the same present without the plant, character
    /// for character, which is a stronger statement than "the source does not appear": a fix that reopened
    /// the scroll question by reporting rows the panel does not show would have to change this string.
    ///
    /// ⚑ **The two sides come from two fixtures, and what makes them comparable is [`fixture`]'s
    /// synthetic clock — not the digit mask below.** The Pacing body prints the loop's own measured rate
    /// (`presented fps`, `fps window`, the frame-time percentiles), so on a real clock the two arms report
    /// two different speeds and this comparison measures the runner's load. `fixture` drives
    /// `Loop::iterate` at a nominal `FRAME_PERIOD` from a base it captures itself, so every figure in
    /// `PacingFacts` is the same string in both arms on any box.
    ///
    /// ⚑ **The comparison is RAW, byte for byte; the digit mask that used to BE the comparison is now
    /// only a message.** The mask mapped each ASCII digit to `#`, which removes a digit's *identity* but
    /// not its COUNT: `9.09` masks to `#.##` and `272.73` to `###.##`, `312 ms` to `### ms` and
    /// `1000 ms` to `#### ms`. That is how each repair failed in turn. A raw comparison failed first on
    /// `272.73` vs `243.24` — the same width, which masking did fix — and the mask then failed on CI
    /// (`462e9cf`, run `35418036061`) on exactly the width it cannot see: the repair had been tested
    /// against the instance that had occurred rather than against the mechanism. With the clock fixed at
    /// its source the two surfaces are *identical unmasked*, measured under a forced 900 ms skew between
    /// the two fixtures, so the character-for-character claim in the first paragraph is the one actually
    /// asserted. The mask survives only as a first cut at which of the two ways this row fired:
    /// masked-EQUAL can only be a time-derived figure that escaped the synthetic clock at the same width,
    /// while masked-DIFFERENT is either such a figure that changed WIDTH or the scroll rule itself — the
    /// mask cannot separate those two, which is the same blindness that let it pass as a repair.
    ///
    /// ⚑ **What none of this covers.** A figure read from `Instant::now()` *inside* a panel body, instead
    /// of derived from the `now` `Loop::iterate` is handed, would be back on the wall clock — the clock is
    /// fixed where the loop takes it, not everywhere one could be read. Nor is either instrument any
    /// defence against a *layout* difference: a wider number changes what FITS inside a pane, so a width
    /// difference is able to change which runs exist at all. No mask over a rendered string could have
    /// repaired that, which is the second reason the fix belongs at the clock and not in the comparison.
    ///
    /// *Controls:* the string was painted into the span; no glyph of it touches the clip; and no other
    /// galley of that body has a band overlapping its row, so the row really is one the surface does not
    /// report rather than one it does.
    #[test]
    fn a_run_on_no_reported_row_is_in_neither_string_and_changes_nothing() {
        // U+6F22 rides along so the `unrenderable` half can be asserted too: that field describes the
        // runs the two strings carry, so a run in neither string may not put a hollow box in it.
        const BELOW: &str = "BELOW EVERY ROW THE PANE SHOWS \u{6F22}";
        let plain = {
            let mut lp = fixture(crate::ui::initial_dock());
            one(&mut lp, &Setup::default()).surface(Tab::Pacing)
        };
        let mut lp = fixture(crate::ui::initial_dock());
        let p = one(
            &mut lp,
            &Setup {
                plant: Some((Some(Tab::Pacing), Plant::Below(BELOW.into()))),
                ..Setup::default()
            },
        );
        let all = painted_of(&p, Tab::Pacing);
        let g = all
            .iter()
            .find(|g| g.galley.text() == BELOW)
            .expect("control: the string below the pane WAS painted into the span");
        let l = laid(g);
        assert!(!l.touches, "control: it touches the clip: {:?}", g.clip);
        let band = l.band.expect("control: it was laid out");
        assert!(
            all.iter()
                .filter(|o| o.galley.text() != BELOW)
                .map(laid)
                .flat_map(|o| o.visible_bands)
                .all(|b| b.1 <= band.0 || b.0 >= band.1),
            "control: something the pane showed shares its row, so the row IS reported"
        );

        let s = p.surface(Tab::Pacing);
        assert!(
            !runs_of(s["text"].as_str().unwrap()).contains(&BELOW),
            "a row the panel does not show is in neither string: {s}"
        );
        let both =
            |v: &serde_json::Value| -> (String, String, serde_json::Value, serde_json::Value) {
                let get = |k: &str| v[k].as_str().unwrap().to_owned();
                (
                    get("text"),
                    get("rendered"),
                    v["truncated"].clone(),
                    v["unrenderable"].clone(),
                )
            };
        // Digits mapped to `#`, used ONLY in the message below: it separates the two ways this row can
        // fire. See the doc above for why it is no longer the comparison.
        let masked = |v: &serde_json::Value| -> (String, String) {
            let mask = |k: &str| {
                v[k].as_str()
                    .unwrap()
                    .chars()
                    .map(|c| if c.is_ascii_digit() { '#' } else { c })
                    .collect::<String>()
            };
            (mask("text"), mask("rendered"))
        };
        assert_eq!(
            plain["unrenderable"],
            serde_json::json!([]),
            "control: this panel names a box of its own, so the comparison below would pass on a leak"
        );
        assert_eq!(
            both(&s),
            both(&plain),
            "planting a run off every reported row moved the surface, so the scroll rule has been \
             reopened. With every digit masked the two sides are {}: EQUAL means only digit VALUES moved, \
             at the same width, which can only be a time-derived figure escaping `fixture`'s synthetic \
             clock; DIFFERENT means either such a figure changed WIDTH — which the mask cannot hide — or \
             the surface really changed shape, and the two strings above say which.",
            if masked(&s) == masked(&plain) {
                "EQUAL"
            } else {
                "DIFFERENT"
            }
        );
    }

    /// ★ **The sweep: across every arrangement, a real run the clip ate whole is in `text` exactly when its
    /// row is reported** — the same instrument the cut gate uses, aimed at the runs that put NOTHING on the
    /// glass instead of the ones cut at the edge.
    ///
    /// **Two-sided, by multiplicity, and independent.** For every drawn body of every arrangement in
    /// [`cut_arrangements`], each galley is read by [`laid`] rather than by `glass_run`. A galley that laid
    /// glyphs out and touches the clip nowhere is classified by geometry alone:
    /// * its band equals the band of a row **another galley showed** → its row is reported → its source
    ///   must be a run of `text`, and the panel must report `truncated`;
    /// * no band of any other galley overlaps it → its row is reported by nothing → its source must be a
    ///   run of neither string;
    /// * anything else (a partial overlap, where this gate's plain geometry and the harvest's centre rule
    ///   may honestly differ) is **counted and printed, never asserted** — a bound on both sides rather
    ///   than a copy of the implementation's own rule, which could only agree with it.
    ///
    /// Sources that occur twice in one body are also left unasserted: with two galleys of the same string
    /// the count in `text` cannot say which one it came from.
    ///
    /// ⚑ **What this is blind to, said out loud.** A run RATCHETED down to fit its clip — the `Grid`
    /// column collapsed to a bare `…` that `no_drawn_run_is_cut_at_a_pane_edge_without_the_mark` was
    /// structurally blind to — is *visible*, so it is not in this population either; that defect has its
    /// own assertion in the cut gate (`blanked`), and whole-widget overflow stays the owner's half under
    /// `d-54`. This row sees exactly the runs the clip ate whole.
    #[test]
    fn a_run_the_clip_ate_is_in_text_exactly_when_its_row_is_reported() {
        let mut arrangements_driven = 0usize;
        let mut runs = 0usize;
        let (mut reported, mut unreported, mut unasserted) = (0usize, 0usize, 0usize);
        let mut examples: Vec<String> = Vec::new();
        for (name, dock, ppp) in cut_arrangements() {
            let mut lp = fixture(dock);
            let p = one(
                &mut lp,
                &Setup {
                    ppp,
                    ..Setup::default()
                },
            );
            arrangements_driven += 1;
            assert!(
                !p.spans.is_empty(),
                "{name}: no body was drawn, so this arrangement witnesses nothing"
            );
            assert_aligned(&name, &p);
            // ⚑ **By POSITION, never by name.** `narrow Screen` docks the Screen tab twice, so two spans
            // carry the title "Screen" and `Present::surface` hands back the first of them whatever body
            // the galleys came from. The contract's order is the window's draw order and the spans are in
            // that order, so the k-th panel surface is the k-th span's — asserted, not assumed. (Found by
            // running this row: a real ate-whole run in the narrow pane was checked against the wide
            // pane's strings and read as a defect in the harvest.)
            let surfaces = p.panel_surfaces();
            assert_eq!(
                surfaces.len(),
                p.spans.len(),
                "{name}: one panel surface per drawn body"
            );
            for ((span, painted), surface) in p.spans.iter().zip(&p.painted).zip(&surfaces) {
                assert_eq!(
                    surface.0, span.name,
                    "{name}: the k-th surface is the k-th span's"
                );
                let ls: Vec<Laid> = painted.iter().map(laid).collect();
                runs += ls.len();
                let s = serde_json::json!({
                    "panel": surface.0, "text": surface.1, "rendered": surface.2,
                    "truncated": surface.3,
                });
                let text = surface.1.as_str();
                let in_text = |src: &str| runs_of(text).iter().filter(|r| **r == src).count();
                for (i, l) in ls.iter().enumerate() {
                    let Some(band) = l.band.filter(|_| !l.touches) else {
                        continue;
                    };
                    let others = || ls.iter().enumerate().filter(move |(j, _)| *j != i);
                    if others().any(|(_, o)| o.source == l.source) {
                        unasserted += 1;
                        continue;
                    }
                    let overlaps = |b: &(f32, f32)| b.1 > band.0 && b.0 < band.1;
                    let shares_a_shown_row = others().any(|(_, o)| {
                        o.visible_bands
                            .iter()
                            .any(|b| (b.0 - band.0).abs() < 0.01 && (b.1 - band.1).abs() < 0.01)
                    });
                    let touched_by_anything = others().any(|(_, o)| {
                        o.band.iter().any(overlaps) || o.visible_bands.iter().any(overlaps)
                    });
                    if shares_a_shown_row {
                        reported += 1;
                        examples.push(format!("{name} / {} :: {:?}", span.name, l.source));
                        assert_eq!(
                            in_text(&l.source),
                            1,
                            "{name} / {}: the clip ate {:?} whole on a row the pane DOES show, so it owes \
                             a run of text with an empty rendered: {s}",
                            span.name,
                            l.source
                        );
                        assert!(
                            surface.3,
                            "{name} / {}: {:?} is in text and not on the glass, so the panel is \
                             truncated: {s}",
                            span.name, l.source
                        );
                    } else if !touched_by_anything {
                        unreported += 1;
                        assert_eq!(
                            in_text(&l.source),
                            0,
                            "{name} / {}: {:?} is on a row nothing on the pane shows, which is in neither \
                             string: {s}",
                            span.name,
                            l.source
                        );
                    } else {
                        unasserted += 1;
                    }
                }
            }
        }
        assert_eq!(
            arrangements_driven,
            cut_arrangements().len(),
            "the sweep did not drive every arrangement"
        );
        assert!(
            arrangements_driven >= 28,
            "only {arrangements_driven} arrangements are swept"
        );
        assert!(
            runs > 2000,
            "anti-vacuity: only {runs} galleys were laid out, so this sweep is not measuring the panels"
        );
        // **Floors on both populations, measured when this row was written and printed below.** The pane
        // the state strip is cut in eats the buttons `8` and `9` whole on a row it DOES show — the real
        // instance of the defect, found by this sweep rather than planted — and 37 runs sit on rows the
        // pane shows nothing of. Floors, not pins: they may rise. A fall means this sweep has stopped
        // measuring the side that matters and only the two planted rows above still do, which is a thing
        // to be TOLD rather than to pass in silence.
        assert!(
            reported >= 2,
            "only {reported} real run(s) were eaten whole on a row their pane reports, so the positive \
             half of this sweep has gone vacuous: {examples:?}"
        );
        assert!(
            unreported >= 20,
            "only {unreported} real run(s) sit on a row their pane reports nothing of, so the boundary \
             half of this sweep has gone vacuous"
        );
        println!(
            "TOTAL LOSS: {arrangements_driven} arrangements, {runs} galleys; {reported} ate-whole runs on \
             a reported row (asserted present), {unreported} on no reported row (asserted absent), \
             {unasserted} unasserted (partial overlap or a repeated source)"
        );
        examples.sort();
        examples.dedup();
        println!("TOTAL LOSS on a reported row: {examples:?}");
    }

    /// ★ **W7, boxes.** A TextEdit holding U+6F22 names it in `unrenderable`; the same panel holding `A`
    /// names nothing (the control `screen.rs`'s own glyph test uses).
    #[test]
    fn w7_a_character_the_window_cannot_draw_is_named_and_a_drawable_one_is_not() {
        let mut lp = fixture(focus_dock(Tab::Watchpoints));
        lp.stopping.w_target = "A".into();
        let control = one(&mut lp, &Setup::default());
        let s = control.surface(Tab::Watchpoints);
        assert!(s["text"].as_str().unwrap().contains('A'));
        assert_eq!(s["unrenderable"], serde_json::json!([]), "control: {s}");

        lp.stopping.w_target = "\u{6F22}".into();
        let boxed = one(&mut lp, &Setup::default());
        let s = boxed.surface(Tab::Watchpoints);
        assert!(s["text"].as_str().unwrap().contains('\u{6F22}'), "{s}");
        assert_eq!(s["unrenderable"], serde_json::json!(["\u{6F22}"]), "{s}");
    }

    /// ★ **W8, blank.** A drawn panel with no text on the glass is present with `""`, never omitted.
    ///
    /// **Real panels, not a fake one.** `--dock every-tab`'s arrangement halves the leaves as it goes, so
    /// at 1600x1000 its last two bodies are slivers. Measured while writing this row (a scratch sweep of
    /// that dock at four window sizes, not committed): **Profiler's body runs and paints no text shape at
    /// all**, and **Watchpoints' body paints two text shapes, neither with a glyph on the glass**. Both are
    /// the blank case, reached two different ways, and both are asserted — with the control that each body
    /// really ran (it has a span), so an absent surface cannot pass as a blank one.
    #[test]
    fn w8_a_drawn_panel_with_no_text_on_the_glass_is_present_and_empty() {
        let mut lp = fixture(crate::ui::every_tab_dock());
        let p = one(&mut lp, &Setup::default());
        for (tab, shapes_painted) in [(Tab::Profiler, false), (Tab::Watchpoints, true)] {
            assert!(
                p.spans.iter().any(|s| s.name == tab.title()),
                "control: {tab:?}'s body did not run, so no surface is owed"
            );
            assert_eq!(
                !painted_of(&p, tab).is_empty(),
                shapes_painted,
                "control: {tab:?} is no longer the case this row names (text shapes painted: {})",
                painted_of(&p, tab).len()
            );
            let s = p.surface(tab);
            assert_eq!(s["text"], serde_json::json!(""), "{s}");
            assert_eq!(s["rendered"], serde_json::json!(""), "{s}");
            assert_eq!(s["truncated"], serde_json::json!(false), "{s}");
            assert_eq!(s["unrenderable"], serde_json::json!([]), "{s}");
        }
        assert!(
            p.panel_surfaces().iter().any(|s| !s.1.is_empty()),
            "anti-vacuity: every panel is blank, so the harvest may be reading nothing at all"
        );
    }

    // ---------------------------------------------------------------------------------------------------
    // F-PANEL-TEXT-CUT-UNMARKED: no drawn run is cut at a pane's edge with nothing on the glass to say so
    // ---------------------------------------------------------------------------------------------------

    /// One glyph row of one painted galley, as the right edge of its clip left it.
    #[derive(Debug)]
    struct RowCut {
        source: String,
        visible: String,
        /// How far past the clip's right edge the row's furthest non-blank glyph reached, in points.
        over: f32,
        wrap_max: f32,
        rows: usize,
        elided: bool,
        clip_w: f32,
        /// How far the galley's own layout BOX reaches past the clip's right edge.
        box_over: f32,
    }

    /// The right edge of the BOX the toolkit laid this galley out in: the width it was told, placed by the
    /// job's horizontal alignment (`Align::Max` anchors the box's right edge at the shape's position,
    /// which is how a `right_to_left` row is drawn). Infinite when the galley was told to extend.
    fn box_right(p: &Painted) -> f32 {
        let w = p.galley.job.wrap.max_width;
        match p.galley.job.halign {
            egui::Align::Min => p.pos.x + w,
            egui::Align::Center => p.pos.x + w / 2.0,
            egui::Align::Max => p.pos.x,
        }
    }

    /// Every glyph row of `p` that put glyphs on the glass AND ran past its clip's right edge.
    ///
    /// Geometry read straight off the galley, not through `glass_run`: the gate must be able to disagree
    /// with the harvest rather than share a mistake with it. Blank glyphs are not counted as overrunning —
    /// a trailing space whose advance crosses the edge is not a cut sentence.
    fn row_cuts(p: &Painted) -> Vec<RowCut> {
        let origin = p.pos.to_vec2();
        let mut out = Vec::new();
        for row in &p.galley.rows {
            let o = origin + row.pos.to_vec2();
            let (mut visible, mut over) = (String::new(), f32::NEG_INFINITY);
            for g in &row.glyphs {
                let r = g.logical_rect().translate(o);
                let on = r.min.x < p.clip.max.x
                    && r.max.x > p.clip.min.x
                    && r.min.y < p.clip.max.y
                    && r.max.y > p.clip.min.y;
                if on {
                    visible.push(g.chr);
                }
                if !g.chr.is_whitespace() {
                    over = over.max(r.max.x - p.clip.max.x);
                }
            }
            if !visible.trim().is_empty() && over > 0.0 {
                out.push(RowCut {
                    source: p.galley.text().to_owned(),
                    visible,
                    over,
                    wrap_max: p.galley.job.wrap.max_width,
                    rows: p.galley.rows.len(),
                    elided: p.galley.elided,
                    clip_w: p.clip.width(),
                    box_over: box_right(p) - p.clip.max.x,
                });
            }
        }
        out
    }

    /// **Tab `t` in a pane a fifth of the window wide**, beside the Screen tab: the arrangement that makes
    /// the stopping tables and the fact grids too narrow for their own text, so something is really cut.
    fn narrow_dock(t: Tab) -> egui_dock::DockState<Tab> {
        let mut dock = egui_dock::DockState::new(vec![Tab::Screen]);
        dock.main_surface_mut()
            .split_right(egui_dock::NodeIndex::root(), 0.8, vec![t]);
        dock
    }

    /// A point on the glass inside `p`'s first visible glyph — the place a pointer must be to hover this
    /// run. Read off the glyph rather than off `Painted::pos`, which for a right-aligned galley is the
    /// box's RIGHT edge and lands outside the text.
    fn glyph_point(p: &Painted) -> Option<egui::Pos2> {
        let origin = p.pos.to_vec2();
        for row in &p.galley.rows {
            for g in &row.glyphs {
                let r = g.logical_rect().translate(origin + row.pos.to_vec2());
                if p.clip.contains_rect(r) && r.width() > 0.0 {
                    return Some(r.center());
                }
            }
        }
        None
    }

    /// ★ **The whole of a truncated panel line is one hover away — exactly one.**
    ///
    /// The mark says a line was cut; this says the reader can still read it, and reads it once. Without
    /// this row the hover is an absence nothing measures: the mark could ship with the text unreachable,
    /// or reachable twice, and every other gate here would stay green. Both halves were found by running
    /// it — see [`crate::ui::fitted_label`] for the duplicate it caught.
    ///
    /// Real lines, not planted: every run **the toolkit itself elided** in the Registers strip in a
    /// fifth-width pane, which is where the line the owner reported is cut. Each is hovered on a glyph
    /// that is really on the glass, with the tooltip delay at zero, and the tooltip is read out of the
    /// other-layer text the harvest already records. `table_cell`'s half of the same treatment has its own
    /// row, `ui::table_tests::a_cut_cell_carries_its_whole_text_on_one_hover`.
    ///
    /// *Controls:* the reported line must be among the cut ones, or the pane had room and this proves
    /// nothing; and at least two lines must be hovered.
    #[test]
    fn the_whole_of_a_truncated_panel_line_is_on_its_hover() {
        let tab = Tab::Registers;
        let mut lp = fixture(narrow_dock(tab));
        let first = one(&mut lp, &Setup::default());
        let cuts: Vec<(String, egui::Pos2)> = painted_of(&first, tab)
            .iter()
            .filter(|g| g.galley.elided)
            .filter_map(|g| glyph_point(g).map(|at| (g.galley.text().to_owned(), at)))
            .collect();
        assert!(
            cuts.iter()
                .any(|(t, _)| t.starts_with("not serving. No --aether")),
            "control: the reported `aether` line is not cut in this pane, so this row measures nothing: \
             {cuts:?}"
        );
        assert!(
            cuts.len() >= 2,
            "control: only {} line(s) are cut here: {cuts:?}",
            cuts.len()
        );
        for (whole, at) in &cuts {
            let at = *at;
            let script = move |i: u32| {
                if i >= 2 {
                    vec![egui::Event::PointerMoved(at)]
                } else {
                    Vec::new()
                }
            };
            let mut lp = fixture(narrow_dock(tab));
            let p = settled_with(
                &mut lp,
                Mode::Record,
                WARM,
                &script,
                &Setup {
                    tooltip_now: true,
                    ..Setup::default()
                },
            );
            let tip: Vec<String> = p
                .other_layers
                .iter()
                .flat_map(|(_, t)| t.iter().map(|k| k.text.clone()))
                .collect();
            let n = tip.iter().filter(|t| *t == whole).count();
            assert_eq!(
                n, 1,
                "the cut line {whole:?} must be on its hover exactly once. Other layers: {tip:?}"
            );
        }
        println!("HOVER: {} cut lines, each on one hover", cuts.len());
    }

    /// ★ **A line the PANE cut is one hover away too — exactly one** (`PANEL-CLIP-MARK`, `d-54`).
    ///
    /// The row above covers lines the toolkit elided at a width it was told. This one covers the class
    /// [`crate::cut_mark`] marks: text whose container is wider than its pane, cut by the pane's edge. Its
    /// hover has two sources and the double hover the first parcel found is exactly what happens if both
    /// fire, so both are driven:
    ///
    /// * a run the toolkit had NOT elided, which the pass is the first to cut — its hover is the pass's own
    ///   region (a table cell, a hex row, a wrapped paragraph);
    /// * a run the toolkit HAD elided at a width wider than the pane, whose `…` was off the glass until the
    ///   pass drew one where it can be seen — its hover is the `Label`'s own, and the pass must add none.
    ///
    /// Which is which is read off the CONTROL arm ([`crate::cut_mark::with_marking_off`]), not assumed:
    /// a run is in play when the control arm cut it with no mark and the treatment arm shows it marked.
    /// Each is hovered on a glyph really on the glass, with the tooltip delay at zero.
    ///
    /// *Controls:* both kinds, and a wrapped paragraph among them, must be hovered, or a half of this is
    /// measured by nothing.
    #[test]
    fn the_whole_of_a_line_the_pane_cut_is_on_its_hover() {
        // (tab, whole text, a point on it, the toolkit had elided it, rows)
        let mut cases: Vec<(Tab, String, egui::Pos2, bool, usize)> = Vec::new();
        for tab in [Tab::Memory, Tab::Objects, Tab::Pacing, Tab::Watchpoints] {
            let off = crate::cut_mark::with_marking_off(|| {
                let mut lp = fixture(narrow_dock(tab));
                one(&mut lp, &Setup::default())
            });
            let cut_off: Vec<(String, bool)> = painted_of(&off, tab)
                .iter()
                .filter(|g| {
                    row_cuts(g)
                        .iter()
                        .any(|c| !c.visible.trim_end().ends_with('\u{2026}') && c.box_over > 0.0)
                })
                .map(|g| (g.galley.text().to_owned(), g.galley.elided))
                .collect();
            let mut lp = fixture(narrow_dock(tab));
            let on = one(&mut lp, &Setup::default());
            let mut taken = 0;
            for g in painted_of(&on, tab) {
                let whole = g.galley.text();
                let Some(&(_, was_elided)) = cut_off.iter().find(|(t, _)| t == whole) else {
                    continue;
                };
                if announced_rows(g) == 0 || cases.iter().any(|c| c.1 == whole) {
                    continue;
                }
                // Enough of each kind per tab to cover it, not every hex row in the Memory pane.
                let kind = cases
                    .iter()
                    .filter(|c| c.0 == tab && c.3 == was_elided)
                    .count();
                if kind >= 2 || taken >= 4 {
                    continue;
                }
                if let Some(at) = glyph_point(g) {
                    cases.push((tab, whole.to_owned(), at, was_elided, g.galley.rows.len()));
                    taken += 1;
                }
            }
        }
        assert!(
            cases.iter().any(|c| !c.3),
            "control: no run the pass was the first to cut was found to hover: {cases:?}"
        );
        assert!(
            cases.iter().any(|c| c.3),
            "control: no run the toolkit had already elided past the pane was found, so the no-double-hover \
             half is measured by nothing: {cases:?}"
        );
        assert!(
            cases.iter().any(|c| c.4 > 1),
            "control: no wrapped paragraph is among the hovered runs: {cases:?}"
        );
        for (tab, whole, at, was_elided, _) in &cases {
            let at = *at;
            let script = move |i: u32| {
                if i >= 2 {
                    vec![egui::Event::PointerMoved(at)]
                } else {
                    Vec::new()
                }
            };
            let mut lp = fixture(narrow_dock(*tab));
            let p = settled_with(
                &mut lp,
                Mode::Record,
                WARM,
                &script,
                &Setup {
                    tooltip_now: true,
                    ..Setup::default()
                },
            );
            let tip: Vec<String> = p
                .other_layers
                .iter()
                .flat_map(|(_, t)| t.iter().map(|k| k.text.clone()))
                .collect();
            let n = tip.iter().filter(|t| *t == whole).count();
            assert_eq!(
                n, 1,
                "{tab:?}: the line the pane cut {whole:?} (toolkit-elided: {was_elided}) must be on its \
                 hover exactly once. Other layers: {tip:?}"
            );
        }
        println!(
            "HOVER (pane cuts): {} lines, {} first cut by the pass, {} already elided by the toolkit, {} \
             wrapped; each on one hover",
            cases.len(),
            cases.iter().filter(|c| !c.3).count(),
            cases.iter().filter(|c| c.3).count(),
            cases.iter().filter(|c| c.4 > 1).count()
        );
    }

    /// Every arrangement the cut sweep drives: the default dock, every-tab, the eleven focus layouts, a
    /// narrow pane per tab, and the two scales the attribution gate uses.
    fn cut_arrangements() -> Vec<(String, egui_dock::DockState<Tab>, Option<f32>)> {
        let mut v: Vec<(String, egui_dock::DockState<Tab>, Option<f32>)> = arrangements()
            .into_iter()
            .map(|(n, d)| (n, d, None))
            .collect();
        for t in Tab::ALL {
            v.push((format!("narrow {}", t.title()), narrow_dock(t), None));
        }
        for ppp in [1.25f32, 2.0] {
            v.push((
                format!("default @{ppp}"),
                crate::ui::initial_dock(),
                Some(ppp),
            ));
            v.push((
                format!("every-tab @{ppp}"),
                crate::ui::every_tab_dock(),
                Some(ppp),
            ));
        }
        v
    }

    /// **The captions the sweep still finds cut with no mark, and why each cannot take one.**
    ///
    /// Until `d-54` was ruled (`mark-only`, 2026-09-25) this list held five button-like captions whose
    /// WIDGET was half off the pane, booked for the owner because the only width that could mark them was a
    /// layout change. [`crate::cut_mark`] now marks the painted row instead, which needs no width, and four
    /// of the five carry the mark. What is left is a caption of **one glyph**: the toolkit's elision keeps at
    /// least one glyph on a row, so a one-glyph caption cut by a few points has nothing to trade for the mark
    /// except itself, and a caption replaced by a bare `…` is exactly the collapse the `blanked` check below
    /// refuses. `◀` (the Screen strip's step-back control, cut by about 3 points in every-tab @2) is the only
    /// such caption the sweep finds. Listed so a NEW unmarked cut cannot hide beside it.
    const CONTROLS_LEFT_FOR_THE_OWNER: [&str; 1] = ["\u{25c0}"];

    /// What one arm of the cut sweep saw, over every arrangement in [`cut_arrangements`].
    #[derive(Default)]
    struct CutArm {
        arrangements: usize,
        runs: usize,
        elided: usize,
        /// Glyph rows that ran a glyph past their clip's right edge.
        rows: usize,
        /// Glyph rows whose visible text ends in the mark (cut or not): what the reader sees announced.
        announced: usize,
        /// Of `rows`, the ones whose visible text ends in the mark anyway.
        marked: usize,
        offenders: Vec<(String, String, RowCut)>,
        blanked: Vec<(String, String, String)>,
        /// Runs laid out on a row the pane shows, with EVERY glyph past the clip's right edge: nothing of
        /// them is on the glass, so there is nowhere to put a mark. Printed, not asserted — see the gate.
        wholly_past: Vec<(String, String, String)>,
        /// `(arrangement, panel, text, rendered)` for every panel surface the reply served.
        surfaces: Vec<(String, String, String, String)>,
    }

    /// **One arm of the cut sweep**: drive every arrangement and read every row every drawn body painted.
    /// The arm is decided by the caller — [`crate::cut_mark::with_marking_off`] around it is the control.
    fn cut_arm() -> CutArm {
        let mut a = CutArm::default();
        for (name, dock, ppp) in cut_arrangements() {
            let mut lp = fixture(dock);
            let p = one(
                &mut lp,
                &Setup {
                    ppp,
                    ..Setup::default()
                },
            );
            a.arrangements += 1;
            // The arrangements where every drawn pane has real room: the dock the window opens in, and
            // the focus layouts, all at the window's own scale. A pane squeezed to fifty points may
            // honestly have nothing to show but the mark; these may not.
            let roomy = ppp.is_none() && (name == "default" || name.starts_with("focus "));
            assert!(
                !p.spans.is_empty(),
                "{name}: no body was drawn, so this arrangement witnesses nothing"
            );
            for (panel, text, rendered, _) in p.panel_surfaces() {
                a.surfaces.push((name.clone(), panel, text, rendered));
            }
            for (span, painted) in p.spans.iter().zip(&p.painted) {
                for g in painted {
                    a.runs += 1;
                    if g.galley.elided {
                        a.elided += 1;
                    }
                    // ⚑ A run truncated to NOTHING BUT the mark, in a pane with room for more. The sweep
                    // below cannot see this one: such a run fits its clip perfectly and is never cut. It
                    // is here because the first form of the fix did exactly that to the fact grids' label
                    // column — a `Grid` column that is not the last one is as wide as it measured last
                    // frame, so truncating to it is a ratchet that only turns down (see
                    // `ui::fitted_label`). Every label in the Registers strip drew as a bare `…` and a
                    // gate watching only the pane edge called it green.
                    if roomy {
                        if let Some(r) = crate::screen::glass_run(g) {
                            if !r.rendered.trim().is_empty()
                                && r.rendered.trim().chars().all(|c| c == '\u{2026}')
                            {
                                a.blanked.push((
                                    name.clone(),
                                    span.name.to_owned(),
                                    r.text.clone(),
                                ));
                            }
                        }
                    }
                    a.announced += announced_rows(g);
                    if wholly_past_the_edge(g) {
                        a.wholly_past.push((
                            name.clone(),
                            span.name.to_owned(),
                            g.galley.text().to_owned(),
                        ));
                    }
                    for c in row_cuts(g) {
                        a.rows += 1;
                        if c.visible.trim_end().ends_with('\u{2026}') {
                            a.marked += 1;
                        } else {
                            a.offenders.push((name.clone(), span.name.to_owned(), c));
                        }
                    }
                }
            }
        }
        a.offenders.sort_by(|a, b| b.2.over.total_cmp(&a.2.over));
        a
    }

    /// Whether every non-blank glyph of `p` lies wholly right of its clip, on a row the clip's vertical
    /// span shows: a run the pane ate whole, sideways. [`crate::cut_mark`] cannot mark it (no glyph of it
    /// is on the glass to end in the mark); §11.50 serves it with an empty `rendered`.
    fn wholly_past_the_edge(p: &Painted) -> bool {
        let origin = p.pos.to_vec2();
        let mut any = false;
        for row in &p.galley.rows {
            let o = origin + row.pos.to_vec2();
            for g in row.glyphs.iter().filter(|g| !g.chr.is_whitespace()) {
                let r = g.logical_rect().translate(o);
                if r.max.y <= p.clip.min.y || r.min.y >= p.clip.max.y || r.min.x < p.clip.max.x {
                    return false;
                }
                any = true;
            }
        }
        any
    }

    /// The glyph rows of `p` whose visible text, trailing blanks aside, ends in the elision mark.
    fn announced_rows(p: &Painted) -> usize {
        let origin = p.pos.to_vec2();
        p.galley
            .rows
            .iter()
            .filter(|row| {
                let o = origin + row.pos.to_vec2();
                row.glyphs
                    .iter()
                    .rfind(|g| {
                        let r = g.logical_rect().translate(o);
                        r.min.x < p.clip.max.x
                            && r.max.x > p.clip.min.x
                            && r.min.y < p.clip.max.y
                            && r.max.y > p.clip.min.y
                            && !g.chr.is_whitespace()
                    })
                    .is_some_and(|g| g.chr == '\u{2026}')
            })
            .count()
    }

    fn print_cuts(arm: &str, a: &CutArm) {
        for (arr, panel, c) in &a.offenders {
            println!(
                "CUT[{arm}] over={:8.2} wrap={:9.1} box_over={:9.2} rows={} elided={} clip_w={:7.1} {arr} / {panel} :: visible={:?} source={:?}",
                c.over, c.wrap_max, c.box_over, c.rows, c.elided, c.clip_w, c.visible, c.source
            );
        }
    }

    /// ★ **No drawn run is cut at a pane's edge without an elision mark** (`F-PANEL-TEXT-CUT-UNMARKED`,
    /// `PANEL-CLIP-MARK`, `d-54` ruled mark-only).
    ///
    /// The CR-W harvest is the instrument: for every drawn panel body in every arrangement below, this
    /// walks the galleys the body actually painted and finds each glyph row that put glyphs on the glass
    /// AND ran past its clip's right edge. A row like that must carry the elision mark, or the reader
    /// cannot tell it from a finished line — the defect §11.29 justifies serving `rendered` for, aimed at
    /// the person at the window instead of at a client.
    ///
    /// **One exit, named rather than counted away:** a caption of one glyph, in
    /// [`CONTROLS_LEFT_FOR_THE_OWNER`]. The box-overflow class the first parcel booked under a ceiling of
    /// 60 is now marked by [`crate::cut_mark`], and the ceiling is 0 (asserted empty).
    ///
    /// **Two arms, because the claim is an absence.** [`crate::cut_mark`] repairs a cut row by making it
    /// FIT (its glyphs now end in the mark before the clip), so on the treatment arm the rows it marked are
    /// no longer cuts at all and a sweep that saw nothing would read exactly like one that fixed
    /// everything. The CONTROL arm runs the same sweep with the pass switched off
    /// ([`crate::cut_mark::with_marking_off`]) and must find the class the pass exists for — box cuts, and
    /// among them wrapped paragraphs — and the treatment arm must show the reader MORE rows ending in the
    /// mark than the control did, by at least the cuts it removed.
    ///
    /// **And the contract, across the two arms:** every panel surface's served `text` is byte-identical
    /// with and without the pass (§11.50: `text` is every run the toolkit LAID OUT, which marking the glass
    /// must not touch, so row k / run j of `text` is the same run either way), while `rendered` differs
    /// wherever a mark was painted.
    ///
    /// **Loud when it cannot measure**: the sweep asserts it drove every arrangement, laid out thousands
    /// of runs, and saw runs the toolkit really elided (so a mark is a thing it can see).
    ///
    /// `cargo test -p oracle-player --bin oracle-player -- --nocapture no_drawn_run_is_cut` prints every
    /// remaining cut on both arms, widest first.
    #[test]
    fn no_drawn_run_is_cut_at_a_pane_edge_without_the_mark() {
        let on = cut_arm();
        let off = crate::cut_mark::with_marking_off(cut_arm);
        print_cuts("on", &on);
        print_cuts("off", &off);

        // --- loud when it cannot measure ---
        for (arm, a) in [("treatment", &on), ("control", &off)] {
            assert_eq!(
                a.arrangements,
                cut_arrangements().len(),
                "{arm}: the sweep did not drive every arrangement"
            );
        }
        // A floor on the LIST as well as on the loop over it: the assertion above compares the sweep with
        // the same function that fed it, so it cannot notice the list itself being cut down.
        assert!(
            on.arrangements >= 28,
            "only {} arrangements are swept; the defect was found by sweeping the default dock, \
             every-tab, every focus layout, a narrow pane per tab and two scales",
            on.arrangements
        );
        assert!(
            on.runs > 2000,
            "anti-vacuity: only {} runs were laid out, so this sweep is not measuring the panels",
            on.runs
        );
        assert!(
            off.elided > 0,
            "anti-vacuity: the toolkit elided nothing anywhere, so this gate cannot see a mark at all"
        );

        // --- the control arm: the sweep sees the cuts the pass removes ---
        let box_off: Vec<_> = off
            .offenders
            .iter()
            .filter(|(_, _, c)| c.wrap_max.is_finite() && c.box_over > 0.0)
            .collect();
        assert!(
            !box_off.is_empty(),
            "control: with the marking pass off the sweep found no row cut because its container is \
             wider than its pane, so nothing here shows it can see the class `cut_mark` exists for"
        );
        assert!(
            box_off.iter().any(|(_, _, c)| c.rows > 1),
            "control: no wrapped paragraph is cut with the pass off, so the rows-of-a-paragraph half of \
             the treatment is measured by nothing"
        );
        let removed = off.offenders.len() - on.offenders.len().min(off.offenders.len());
        assert!(
            on.offenders.len() < off.offenders.len(),
            "the marking pass changed nothing the sweep can see: {} unmarked cuts with it, {} without",
            on.offenders.len(),
            off.offenders.len()
        );
        assert!(
            on.announced >= off.announced + removed,
            "the pass removed {removed} unmarked cuts, but the reader sees only {} rows ending in the mark \
             against {} without it: a cut was hidden rather than announced",
            on.announced,
            off.announced
        );

        // --- the contract: `text` is the layout's, and marking the glass does not touch it ---
        assert_eq!(
            on.surfaces.len(),
            off.surfaces.len(),
            "the two arms served a different number of panel surfaces"
        );
        let mut rendered_changed = 0usize;
        for (a, b) in on.surfaces.iter().zip(&off.surfaces) {
            assert_eq!(
                (&a.0, &a.1),
                (&b.0, &b.1),
                "the two arms served their panels in a different order"
            );
            assert_eq!(
                a.2, b.2,
                "{} / {}: the panel's served `text` changed when the glass was marked; `text` is every run \
                 the toolkit laid out, and marking must leave it alone",
                a.0, a.1
            );
            if a.3 != b.3 {
                rendered_changed += 1;
            }
        }
        assert!(
            rendered_changed > 0,
            "no served `rendered` differs between the arms, so the mark never reached what a client reads"
        );

        assert!(
            on.blanked.is_empty(),
            "a run was truncated to nothing but the elision mark in a pane with room for more, which \
             tells the reader less than an empty cell would: {:?}",
            on.blanked
        );

        // --- what is left: the one-glyph captions, named ---
        let mut natural: Vec<(&str, &str, &str)> = on
            .offenders
            .iter()
            .filter(|(_, _, c)| c.wrap_max.is_infinite())
            .map(|(arr, panel, c)| (arr.as_str(), panel.as_str(), c.source.as_str()))
            .collect();
        let unlisted: Vec<_> = natural
            .iter()
            .filter(|(_, _, src)| !CONTROLS_LEFT_FOR_THE_OWNER.contains(src))
            .collect();
        assert!(
            unlisted.is_empty(),
            "text laid out at its natural width and then cut by the pane, with nothing on the glass to \
             say so, although `cut_mark` runs over every body. Find why it did not mark this run: {unlisted:?}"
        );
        let natural_rows = natural.len();
        natural.sort_unstable_by_key(|(_, _, src)| *src);
        natural.dedup_by_key(|(_, _, src)| *src);
        let seen: Vec<&str> = natural.iter().map(|(_, _, src)| *src).collect();
        assert!(
            seen.iter()
                .any(|s| CONTROLS_LEFT_FOR_THE_OWNER.contains(s)),
            "every control booked for the owner's half now fits: the waiver list is dead and must be \
             deleted rather than left standing"
        );

        let box_cuts: Vec<_> = on
            .offenders
            .iter()
            .filter(|(_, _, c)| !c.wrap_max.is_infinite())
            .collect();
        let inside: Vec<_> = box_cuts
            .iter()
            .filter(|(_, _, c)| c.box_over <= 0.0)
            .collect();
        assert!(
            inside.is_empty(),
            "a run was cut with no mark although its own layout box fits inside the pane, so a width was \
             available that would have marked it: {inside:?}"
        );
        assert!(
            // The ceiling the first parcel booked for the owner (`BOX_CUTS_BOOKED_FOR_THE_OWNER`: 60 on
            // 2026-09-17) came down to ZERO when `cut_mark` began marking the painted row (2026-09-25); a
            // ceiling of zero is this assertion, and the control arm above is what shows the sweep would
            // still see such a cut.
            box_cuts.is_empty(),
            "{} runs are cut with no mark because their container is wider than their pane, and none are \
             allowed. `cut_mark` exists to mark exactly these, so find why it did not: {box_cuts:?}",
            box_cuts.len()
        );

        let mut by_panel: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, panel, _) in &off.offenders {
            *by_panel.entry(panel.as_str()).or_default() += 1;
        }
        println!(
            "CUTS: {} arrangements, {} runs ({} elided by the toolkit, {} once marked). CONTROL \
             (marking off): {} rows cut, {} marked, {} unmarked ({} of them box cuts), {} rows \
             announced. TREATMENT: {} rows cut, {} marked, {} unmarked = {natural_rows} rows of {} \
             one-glyph captions + {} box cuts (ceiling 0), {} rows announced; {rendered_changed} of {} \
             served panel surfaces changed `rendered`, none changed `text`",
            on.arrangements,
            on.runs,
            off.elided,
            on.elided,
            off.rows,
            off.marked,
            off.offenders.len(),
            box_off.len(),
            off.announced,
            on.rows,
            on.marked,
            on.offenders.len(),
            seen.len(),
            box_cuts.len(),
            on.announced,
            on.surfaces.len()
        );
        println!("CUTS control arm by panel: {by_panel:?}");
        println!("CUTS controls left for the owner: {seen:?}");
        // Not asserted: a run the pane ate whole has no glyph on the glass to carry the mark, so it is
        // outside what mark-only can reach; whether such a control should sit off the pane at all is the
        // layout half `d-54` kept for the owner. Printed so the count is on the record, not discovered.
        for (arr, panel, text) in &on.wholly_past {
            println!("WHOLLY-PAST {arr} / {panel} :: {text:?}");
        }
        println!(
            "CUTS wholly past the pane's edge (no glyph on the glass, unmarkable): {}",
            on.wholly_past.len()
        );
    }

    /// **The `is_serving` gate, observed**: a window no client can reach runs `Loop::iterate` — the
    /// production caller, not `publish_screen_text` directly — and publishes nothing, so
    /// `emulator/screen_text` still refuses `noDisplay` in-process. Control: the same loop's bodies really
    /// drew (a direct `build_ui(root, true)` returns spans), so the silence is the gate and not an empty
    /// dock.
    #[test]
    fn a_window_no_client_can_reach_publishes_no_panels() {
        let mut lp = fixture(crate::ui::every_tab_dock());
        assert!(!lp.bus.is_serving(), "control: the fixture binds no socket");
        let ctx = context();
        let mut spans = 0;
        for i in 0..3 {
            let mut out = ctx.run_ui(raw(i, Vec::new()), |root| {
                let c = root.ctx().clone();
                lp.iterate(&c, root, Instant::now());
            });
            out.textures_delta.clear();
        }
        let mut out = ctx.run_ui(raw(3, Vec::new()), |root| {
            spans = lp.build_ui(root, true).1.len();
        });
        out.textures_delta.clear();
        assert_eq!(spans, Tab::ALL.len(), "control: every body draws");
        let answer = lp.bus.call(
            lp.machine.system_mut(),
            "emulator/screen_text",
            &serde_json::json!({}),
        );
        assert_eq!(
            answer.reason(),
            Some("noDisplay"),
            "a window that serves no client published its screen text anyway"
        );
    }

    /// **Real replies for the CR-W vector file's cases 1-4, captured OVER A REAL SOCKET** (CR-H's bar:
    /// hand-built populated vectors are replaced by replies the implementation produced). Ignored: a
    /// capture, not a gate. `cargo test -p oracle-player --bin oracle-player capture_cr_w_vector_replies --
    /// --ignored --nocapture` prints one `CRW-VECTOR <case> <reply>` line per case.
    ///
    /// ⚑ **The socket, not `Bus::call`.** The first version of this capture read the reply through
    /// `Bus::call` in-process, and the replies it produced were REFUSED by the vendored fragment: the D11
    /// stamp (`frame`, `mclk`, `running`, `droppedEvents`) is added by the serving path
    /// (`server::render`), not by `Engine::dispatch`, and `droppedEvents` is per connection. A vector is a
    /// whole wire document, so it is captured from a whole wire document — a `Loop` bound to a private
    /// socket, a real NDJSON client, `initialize`, then the read.
    ///
    /// 1. two leaves of the default dock expanded, the other two collapsed, with a cut cell;
    /// 2. every leaf collapsed: the bar's two surfaces and no panel;
    /// 3. `--dock every-tab`: two panels drawn with nothing on the glass (W8's case);
    /// 4. U+6F22 typed into the Watchpoints add box (W7's case).
    #[test]
    #[ignore = "capture for the CR-W vector file; run with --ignored --nocapture"]
    fn capture_cr_w_vector_replies() {
        let collapse_all_but = |dock: &mut egui_dock::DockState<Tab>, keep: &[Tab]| {
            for node in dock.main_surface_mut().iter_mut() {
                if let egui_dock::Node::Leaf(l) = node {
                    l.collapsed = !l.tabs.iter().any(|t| keep.contains(t));
                }
            }
        };
        let mut case1 = crate::ui::initial_dock();
        collapse_all_but(&mut case1, &[Tab::Registers, Tab::Breakpoints]);
        let mut case2 = crate::ui::initial_dock();
        collapse_all_but(&mut case2, &[]);
        let mut case4 = focus_dock(Tab::Watchpoints);
        collapse_all_but(&mut case4, &[Tab::Watchpoints]);
        let cases: [(egui_dock::DockState<Tab>, &str); 4] = [
            (case1, ""),
            (case2, ""),
            (crate::ui::every_tab_dock(), ""),
            (case4, "\u{6F22}"),
        ];
        for (n, (dock, typed)) in cases.into_iter().enumerate() {
            let reply = capture_over_socket(dock, typed);
            if n == 0 {
                assert!(
                    reply["surfaces"]
                        .as_array()
                        .expect("surfaces")
                        .iter()
                        .any(|s| s["kind"] == "panel" && s["truncated"] == true),
                    "case 1 must carry a cut panel: {reply}"
                );
            }
            println!("CRW-VECTOR {} {reply}", n + 1);
        }
    }

    /// One `emulator/screen_text` reply, read by a real client over a real socket from a `Loop` running
    /// `dock`. The window drives presents until the client has finished; the client waits for
    /// `status.display` and then reads several times, keeping the last, so the reply it keeps is from a
    /// settled window (`egui_dock`'s outer scroll bar animates the body clip over the first presents).
    fn capture_over_socket(dock: egui_dock::DockState<Tab>, typed: &str) -> serde_json::Value {
        use std::io::{BufRead as _, Write as _};
        let tag = format!("{}-{}", std::process::id(), line!());
        let socket = std::env::temp_dir().join(format!("crw-{tag}.sock"));
        let mut lp = Loop::new(
            Machine::new(oracle_core::testrom::build(), None),
            Instant::now(),
            Some(0.0),
            String::from("(fixture)"),
            symbols::Loaded {
                table: Some(object_listing()),
                path: None,
                fatal: None,
            },
            Some(Some(socket.clone())),
        );
        assert!(lp.bus.is_serving(), "the fixture did not bind {socket:?}");
        arm_for_measurement(&mut lp);
        lp.dock = dock;
        lp.stopping.w_target = typed.to_owned();
        let path = socket.clone();
        let client = std::thread::spawn(move || {
            let deadline = Instant::now() + std::time::Duration::from_secs(20);
            let stream = loop {
                match std::os::unix::net::UnixStream::connect(&path) {
                    Ok(s) => break s,
                    Err(e) => {
                        assert!(Instant::now() < deadline, "connect: {e}");
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(20)))
                .unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut id = 0i64;
            let mut call = |reader: &mut std::io::BufReader<std::os::unix::net::UnixStream>,
                            method: &str,
                            params: serde_json::Value| {
                id += 1;
                writeln!(
                    writer,
                    "{}",
                    serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
                )
                .unwrap();
                writer.flush().unwrap();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).expect("read") > 0, "hung up");
                    let v: serde_json::Value = serde_json::from_str(&line).expect("bad JSON");
                    if v.get("id").is_some_and(|i| !i.is_null()) {
                        assert!(v.get("error").is_none(), "{method}: {}", v["error"]);
                        return v["result"].clone();
                    }
                }
            };
            call(
                &mut reader,
                "initialize",
                serde_json::json!({"clientId":"crw-capture","clientName":"crw","clientVersion":"0",
                    "protocolVersion":1,"clientCapabilities":{"events":false}}),
            );
            let deadline = Instant::now() + std::time::Duration::from_secs(20);
            while call(&mut reader, "emulator/status", serde_json::json!({}))["display"] != true {
                assert!(Instant::now() < deadline, "the window never presented");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let mut last = serde_json::Value::Null;
            for _ in 0..16 {
                last = call(&mut reader, "emulator/screen_text", serde_json::json!({}));
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            last
        });
        let ctx = context();
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        let mut i = 0;
        while !client.is_finished() {
            assert!(Instant::now() < deadline, "the client never finished");
            i += 1;
            let mut out = ctx.run_ui(raw(i, Vec::new()), |root| {
                let c = root.ctx().clone();
                lp.iterate(&c, root, Instant::now());
            });
            out.textures_delta.clear();
        }
        let reply = client.join().expect("the client thread");
        drop(lp);
        let _ = std::fs::remove_file(&socket);
        reply
    }

    /// **W10, the cost of publishing per present** — ignored: a measurement, run in release:
    /// `CRW_ROUNDS=2000 cargo test --release -p oracle-player w10_publish_cost -- --ignored --nocapture`.
    ///
    /// Four arms on one context and one loop, per dock:
    /// * **null** — `build_ui(root, false)` and nothing published: a window no client can reach, which is
    ///   what the `is_serving` gate gives;
    /// * **bar only** — `build_ui(root, false)` then `Loop::publish_screen_text` with no spans: the
    ///   pre-CR-W publish (title bar and top bar), so the panels' increment is read off this arm, not null;
    /// * **bar + panels** — `build_ui(root, true)` then `publish_screen_text`: the production call, spans,
    ///   in-pass read, glyph probe, joins and push;
    /// * **bar + panels + one reply** — the same plus one `emulator/screen_text` dispatch serialised to a
    ///   string, which a client pays only when it asks (an upper bound: one read per present).
    ///
    /// ⚑ **Blocks, not per-present interleaving, and the reason was measured.** The first version of this
    /// harness rotated the arms present by present, and the publish arm came out BIMODAL: ~0.26 ms after a
    /// present that had not published, ~0.06 ms after one that had. `Glyphs` lays out each character it
    /// probes, and egui keeps a galley in its layout cache only while the previous pass used it, so an arm
    /// that follows a non-publishing present pays the layouts again. A serving window publishes EVERY
    /// present, so the steady state is the warm one, and interleaving measured a cold cache the product
    /// never has. So each arm runs in blocks of [`BLOCK`] consecutive presents, blocks rotating across arms
    /// (so load drift is still shared), and the first [`SETTLE`] presents of every block are discarded.
    #[test]
    #[ignore = "measurement; run in release with --ignored --nocapture"]
    fn w10_publish_cost_per_present() {
        use std::time::Duration;
        const BLOCK: usize = 20;
        const SETTLE: usize = 5;
        const ARMS: [&str; 4] = ["null", "bar only", "bar + panels", "bar + panels + reply"];
        let rounds: usize = std::env::var("CRW_ROUNDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);
        for (name, dock) in [
            ("default", crate::ui::initial_dock()),
            ("every-tab", crate::ui::every_tab_dock()),
        ] {
            let mut lp = fixture(dock);
            let ctx = context();
            let mut whole: [Vec<Duration>; 4] = Default::default();
            let mut part: [Vec<Duration>; 4] = Default::default();
            let mut bytes = [0usize; 4];
            let mut i = 0u32;
            // Two warm-up blocks per arm, discarded whole.
            let blocks = (rounds / (BLOCK - SETTLE)).max(1) * ARMS.len() + 2 * ARMS.len();
            for block in 0..blocks {
                let arm = block % ARMS.len();
                for n in 0..BLOCK {
                    i += 1;
                    let t0 = Instant::now();
                    let mut spent = Duration::ZERO;
                    let mut made = 0usize;
                    let mut out = ctx.run_ui(raw(i, Vec::new()), |root| {
                        let c = root.ctx().clone();
                        let (drew, drawn) = lp.build_ui(root, arm >= 2);
                        if arm >= 1 {
                            let t = Instant::now();
                            lp.publish_screen_text(&c, &drew, &drawn);
                            if arm == 3 {
                                if let crate::bus::Answer::Ok(v) = lp.bus.call(
                                    lp.machine.system_mut(),
                                    "emulator/screen_text",
                                    &serde_json::json!({}),
                                ) {
                                    made = serde_json::to_string(&v).expect("serialise").len();
                                }
                            }
                            spent = t.elapsed();
                        }
                    });
                    let dt = t0.elapsed();
                    out.textures_delta.clear();
                    if block >= 2 * ARMS.len() && n >= SETTLE {
                        whole[arm].push(dt);
                        part[arm].push(spent);
                        bytes[arm] = made;
                    }
                }
            }
            let stat = |v: &mut Vec<Duration>| {
                v.sort();
                let ms = |d: Duration| d.as_secs_f64() * 1000.0;
                (ms(v[v.len() / 2]), ms(v[v.len() * 95 / 100]), v.len())
            };
            for (arm, label) in ARMS.iter().enumerate() {
                let w = stat(&mut whole[arm]);
                let h = stat(&mut part[arm]);
                println!(
                    "W10 {name:<9} {label:<21} present median {:.3} ms p95 {:.3} ms | publish median {:.4} ms p95 {:.4} ms | n {} | reply {} bytes",
                    w.0, w.1, h.0, h.1, w.2, bytes[arm]
                );
            }
        }
    }
}
