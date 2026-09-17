//! **CR-W Q3, the feasibility spike: can a dock tab body's painted text be attributed to that tab,
//! completely and exclusively, from what egui painted?**
//!
//! `docs/proposed/2026-09-17-cr-w-panel-screen-text.md` §2.3 and §11 Q3 name two forms:
//!
//! * **Form 1 (in pass).** `TabViewer::ui` notes `ui.layer_id()` and that layer's
//!   `PaintList::next_idx()` before and after the body's `match`; the shapes in `[start, end)` are read
//!   through `Context::graphics` while the pass is still open.
//! * **Form 2 (fallback).** A `Plugin::output_hook` sees the flattened `FullOutput` after the pass and
//!   attributes each text shape to the tab whose recorded body clip rectangle contains the shape's clip
//!   rectangle.
//!
//! **The recording seam is the production `TabViewer::ui` itself** (`crate::ui`), through a hook compiled
//! only under `cfg(test)`: [`probe::enter`] and [`probe::leave`]. Everything else this module drives is the
//! shipped code: `Loop::new`, `arm_for_measurement`, `Loop::iterate` to put rows in the panels, and
//! `Loop::build_ui` (top bar, nav, transport, the real `DockArea` with its real style, all eleven real
//! bodies) for every measured present.
//!
//! # Ground truth, and why it is independent of the harvest
//!
//! The truth for tab `T` is a **multiset difference of two whole `FullOutput`s**, taken after `end_pass`
//! has flattened every layer:
//!
//! `truth(T) = text(FullOutput with ONLY T's body executed) - text(FullOutput with NO body executed)`
//!
//! Both runs use the same `Loop`, the same dock, the same window size, the same input, the same number of
//! warm-up presents, each in a fresh `egui::Context`. The probe suppresses a body by returning before its
//! `match` (the leaf, its tab strip, its frame and its scroll area are still drawn by `egui_dock`). That
//! instrument never looks at a layer id, a paint-list index or a clip rectangle, so it cannot agree with
//! the harvest by sharing a mistake with it. What it does share is egui and the bodies, which is the thing
//! under test. Two controls make the difference meaningful: **determinism** (two independent full runs
//! paint the identical text multiset) and **additivity** (the full run minus the no-body run equals the sum
//! of every `truth(T)`), so no body's text depends on another body having run.
//!
//! A text key is the galley's source string, its origin and its clip rectangle, quantised to 1/100 point,
//! so two equal labels in different places are two keys.

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

/// Every `Shape::Text` in `shapes`, `Shape::Vec` walked, keyed.
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

/// **The attribution verdict for one tab in one form.** `missing` is truth the harvest did not find
/// (incomplete); `foreign` is harvest the truth does not hold (not exclusive).
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

/// The hook `crate::ui`'s `TabViewer::ui` calls under `cfg(test)`. Thread-local, so tests running in
/// parallel never see one another's probe, and a test that installs none gets the shipped behaviour.
pub(crate) mod probe {
    use super::*;
    use std::cell::RefCell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Mode {
        /// Every body runs; each is recorded.
        Record,
        /// Only this tab's body runs (the ground-truth arm).
        Only(Tab),
        /// No body runs (the ground-truth baseline).
        NoBodies,
    }

    /// One body's recorded span.
    #[derive(Clone, Debug)]
    pub struct Span {
        pub tab: Tab,
        pub layer: LayerId,
        pub start: usize,
        pub end: usize,
        /// `ui.clip_rect()` on entry, the rectangle form 2 attributes by.
        pub body_clip: Rect,
        /// `ui.layer_id()` on the way out; a body that changed its own layer would show here.
        pub layer_at_leave: LayerId,
        /// The span's text read at `leave`, inside the body's own call.
        pub at_leave: Vec<Key>,
    }

    pub struct Probe {
        pub mode: Mode,
        pub spans: Vec<Span>,
        /// Paint this string into the body's own painter just before `end` is recorded: a planted
        /// interleaving, the control that the checks can see one.
        pub plant: Option<String>,
        /// Form 2's per-pass result, written by [`Form2`] from `output_hook`.
        pub form2: Vec<(Tab, Vec<Key>)>,
    }

    thread_local! {
        static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
    }

    pub fn install(mode: Mode) {
        PROBE.with(|p| {
            *p.borrow_mut() = Some(Probe {
                mode,
                spans: Vec::new(),
                plant: None,
                form2: Vec::new(),
            })
        });
    }

    pub fn set_plant(s: Option<String>) {
        PROBE.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                p.plant = s;
            }
        });
    }

    pub fn uninstall() {
        PROBE.with(|p| *p.borrow_mut() = None);
    }

    pub fn with<R>(f: impl FnOnce(&mut Probe) -> R) -> R {
        PROBE.with(|p| f(p.borrow_mut().as_mut().expect("probe installed")))
    }

    pub fn installed() -> bool {
        PROBE.with(|p| p.borrow().is_some())
    }

    pub struct Token(Option<(Tab, LayerId, usize, Rect)>);

    /// `None` means: do not run this body.
    pub fn enter(ui: &egui::Ui, tab: Tab) -> Option<Token> {
        let mode = PROBE.with(|p| p.borrow().as_ref().map(|p| p.mode));
        match mode {
            None => Some(Token(None)),
            Some(Mode::NoBodies) => None,
            Some(Mode::Only(t)) if t != tab => None,
            Some(Mode::Only(_)) | Some(Mode::Record) => {
                let layer = ui.layer_id();
                let start = ui
                    .ctx()
                    .graphics(|g| g.get(layer).map_or(0, |l| l.next_idx().0));
                Some(Token(Some((tab, layer, start, ui.clip_rect()))))
            }
        }
    }

    pub fn leave(ui: &egui::Ui, token: Token) {
        let Token(Some((tab, layer, start, body_clip))) = token else {
            return;
        };
        // Planted only while recording, never into a ground-truth arm.
        let plant = PROBE.with(|p| {
            p.borrow()
                .as_ref()
                .filter(|p| p.mode == Mode::Record)
                .and_then(|p| p.plant.clone())
        });
        if let Some(s) = plant {
            ui.painter().text(
                ui.clip_rect().center(),
                egui::Align2::CENTER_CENTER,
                s,
                egui::FontId::monospace(12.0),
                egui::Color32::WHITE,
            );
        }
        let (end, at_leave) = ui.ctx().graphics(|g| {
            let l = g.get(layer).expect("the body's layer has a paint list");
            let end = l.next_idx().0;
            (end, texts(l.all_entries().skip(start).take(end - start)))
        });
        let span = Span {
            tab,
            layer,
            start,
            end,
            body_clip,
            layer_at_leave: ui.layer_id(),
            at_leave,
        };
        PROBE.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                p.spans.push(span);
            }
        });
    }
}

/// **Form 2**: attribute each text shape of the finished `FullOutput` to the tab whose recorded body clip
/// rectangle contains the shape's clip rectangle.
pub(crate) fn attribute_by_clip(
    spans: &[probe::Span],
    shapes: &[ClippedShape],
) -> Vec<(Tab, Vec<Key>)> {
    spans
        .iter()
        .map(|s| {
            let within = shapes
                .iter()
                .filter(|c| s.body_clip.expand(0.01).contains_rect(c.clip_rect));
            (s.tab, texts(within))
        })
        .collect()
}

struct Form2;

impl egui::Plugin for Form2 {
    fn debug_name(&self) -> &'static str {
        "crw-q3-form2"
    }

    fn output_hook(&mut self, _ctx: &egui::Context, output: &mut egui::FullOutput) {
        if !probe::installed() {
            return;
        }
        probe::with(|p| p.form2 = attribute_by_clip(&p.spans, &output.shapes));
    }
}

/// What one measured present produced.
pub(crate) struct Present {
    /// Every text run in the finished `FullOutput`.
    pub out: Vec<Key>,
    pub spans: Vec<probe::Span>,
    /// Form 1: each span's text, read from the present loop after `build_ui` returned (so after the dock
    /// was shown) and before the pass ended.
    pub in_pass: Vec<(Tab, Vec<Key>)>,
    /// Text living in layers other than the spans' own, read in the same place: where popups, tooltips
    /// and windows went.
    pub other_layers: Vec<(LayerId, Vec<Key>)>,
    pub form2: Vec<(Tab, Vec<Key>)>,
    /// The spans' layer's paint-list length read after `run_ui` returned, i.e. after `end_pass`.
    pub after_pass_len: Option<usize>,
    pub passes: u32,
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
    ctx.add_plugin(Form2);
    ctx
}

thread_local! {
    static PLANT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// One present of the real `build_ui` under `mode`, harvested in both forms.
pub(crate) fn present(
    lp: &mut Loop,
    ctx: &egui::Context,
    raw: egui::RawInput,
    mode: probe::Mode,
) -> Present {
    probe::install(mode);
    probe::set_plant(PLANT.with(|p| p.borrow().clone()));
    let mut in_pass = Vec::new();
    let mut other_layers = Vec::new();
    let mut out = ctx.run_ui(raw, |root| {
        // A discarded pass re-runs this closure; only the last pass's shapes are kept, so only the last
        // pass's spans are.
        probe::with(|p| p.spans.clear());
        let _ = lp.build_ui(root);
        let spans = probe::with(|p| p.spans.clone());
        in_pass = spans
            .iter()
            .map(|s| {
                let got = root.ctx().graphics(|g| {
                    texts(
                        g.get(s.layer)
                            .expect("paint list")
                            .all_entries()
                            .skip(s.start)
                            .take(s.end - s.start),
                    )
                });
                (s.tab, got)
            })
            .collect();
        let layers: Vec<LayerId> = root.ctx().memory(|m| m.layer_ids().collect());
        other_layers = layers
            .into_iter()
            .filter(|l| !spans.iter().any(|s| s.layer == *l))
            .map(|l| {
                let t = root.ctx().graphics(|g| {
                    g.get(l)
                        .map(|pl| texts(pl.all_entries()))
                        .unwrap_or_default()
                });
                (l, t)
            })
            .filter(|(_, t)| !t.is_empty())
            .collect();
    });
    out.textures_delta.clear();
    let (spans, form2) = probe::with(|p| (p.spans.clone(), p.form2.clone()));
    let after_pass_len = spans
        .first()
        .and_then(|s| ctx.graphics(|g| g.get(s.layer).map(|l| l.all_entries().len())));
    probe::uninstall();
    Present {
        out: texts(&out.shapes),
        spans,
        in_pass,
        other_layers,
        form2,
        after_pass_len,
        passes: out.platform_output.num_completed_passes as u32,
    }
}

/// `n` warm-up presents then the measured one, in a fresh context, all under `mode` and `events_at_last`
/// delivered on the measured present only.
pub(crate) fn settled(
    lp: &mut Loop,
    mode: probe::Mode,
    warm: u32,
    script: &dyn Fn(u32) -> Vec<egui::Event>,
) -> Present {
    settled_with(lp, mode, warm, script, &Setup::default())
}

/// What an arm changes about its context or its probe, identically in every arm of one measurement.
#[derive(Default, Clone)]
pub(crate) struct Setup {
    /// Show a tooltip as soon as the pointer rests, instead of after egui's 0.5 s.
    pub tooltip_now: bool,
    /// See [`probe::Probe::plant`]. Applied only in the recording arm.
    pub plant: Option<String>,
}

pub(crate) fn settled_with(
    lp: &mut Loop,
    mode: probe::Mode,
    warm: u32,
    script: &dyn Fn(u32) -> Vec<egui::Event>,
    setup: &Setup,
) -> Present {
    // The Planes tab counts its own repaints and prints the count, so an arm that followed another on
    // the same loop would differ by that count alone. Each arm starts the panel from its default, which
    // every arm then advances by the same `warm + 1` presents.
    lp.planes = crate::planes::Panel::default();
    let ctx = context();
    if setup.tooltip_now {
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    }
    PLANT.with(|p| *p.borrow_mut() = setup.plant.clone());
    for i in 0..warm {
        let _ = present(lp, &ctx, raw(i, script(i)), mode);
    }
    present(lp, &ctx, raw(warm, script(warm)), mode)
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
    let mut lp = Loop::new(
        machine,
        Instant::now(),
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
            lp.iterate(&c, root, Instant::now());
        });
        out.textures_delta.clear();
    }
    lp.dock = dock;
    lp
}

/// **Tab `t` in a leaf of 55% of the window, and every other tab in a leaf of its own, stacked in the
/// remaining column**: all eleven bodies run, and the one under test has room to draw its tables, its
/// inner scroll areas and its text boxes. `every_tab_dock` halves the leaves as it goes, so its last few
/// bodies are slivers that paint almost nothing; this arrangement is what makes each tab's verdict a
/// verdict about a body that drew.
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
/// collapsed. Derived from the dock, independently of the probe.
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

/// One arrangement's measurement: both forms against the truth, for every drawn tab.
pub(crate) struct Measured {
    pub name: String,
    pub full: Present,
    pub rows: Vec<(Tab, usize, Verdict, Verdict)>,
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
    let full = settled_with(&mut lp, probe::Mode::Record, warm, script, setup);
    let again = settled_with(&mut lp, probe::Mode::Record, warm, script, setup);
    let base = settled_with(&mut lp, probe::Mode::NoBodies, warm, script, setup);
    let determinism = diff(&full.out, &again.out);
    let mut rows = Vec::new();
    let mut sum = Vec::new();
    for s in &full.spans {
        let only = settled_with(&mut lp, probe::Mode::Only(s.tab), warm, script, setup);
        let (truth, _) = diff(&only.out, &base.out);
        let pick = |v: &[(Tab, Vec<Key>)]| {
            v.iter()
                .find(|(t, _)| *t == s.tab)
                .map(|(_, k)| k.clone())
                .unwrap_or_default()
        };
        let f1 = Verdict::of(&pick(&full.in_pass), &truth);
        let f2 = Verdict::of(&pick(&full.form2), &truth);
        rows.push((s.tab, truth.len(), f1, f2));
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
    use super::probe::Mode;
    use super::*;

    /// Warm-up presents before the measured one. **Twelve, not four, and measured:** at four, the focus
    /// arrangement for Watchpoints painted its body clip at y max 991.2 with every body running and 992.5
    /// with only its own, a scroll bar still animating in `egui_dock`'s outer `ScrollArea`; at eight 989.5
    /// against 989.6; at twelve both 989.5. A transient of the ground-truth comparison, not of the harvest.
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

    /// The controls every measurement must pass before its verdicts mean anything; see the gate's doc.
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

    fn row(m: &Measured, tab: Tab) -> &(Tab, usize, Verdict, Verdict) {
        m.rows
            .iter()
            .find(|r| r.0 == tab)
            .unwrap_or_else(|| panic!("{}: {tab:?} was not drawn", m.name))
    }

    fn span_texts(p: &Present, tab: Tab) -> &[Key] {
        &p.in_pass.iter().find(|(t, _)| *t == tab).expect("a span").1
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

    /// ★ **The Q3 gate.** For the default dock, `--dock every-tab`'s arrangement and one focus arrangement
    /// per tab: every drawn body's text, harvested in form 1 (in-pass paint-list span) and in form 2
    /// (`output_hook`, by clip rectangle), equals that tab's ground truth exactly.
    ///
    /// **Controls, each asserted before the verdict they make meaningful:** determinism (two independent
    /// full runs paint the same text), additivity (full minus no-body equals the sum of the truths, so a
    /// body's text does not depend on another body), the drawn set (the probe recorded exactly the leaves'
    /// active tabs), one layer (a body never changed layer, including through its own scroll areas),
    /// in-pass read equals the read inside the body's own call, the drain (the layer's paint list is empty
    /// once `run_ui` has returned, so form 1 MUST read in the pass), and anti-vacuity (every tab has text
    /// in its own focus arrangement).
    #[test]
    fn every_drawn_tab_body_is_attributed_completely_and_exclusively_in_both_forms() {
        let mut table: Vec<(Tab, (usize, bool, bool))> = Vec::new();
        let mut failures = Vec::new();
        for (name, dock) in arrangements() {
            let expect = active_tabs(&dock);
            let m = measure(&name, dock, WARM);
            assert!(
                m.determinism.0.is_empty() && m.determinism.1.is_empty(),
                "{name}: two identical full runs painted different text, so no comparison below means \
                 anything: {:?}",
                m.determinism
            );
            assert!(
                m.additivity.0.is_empty()
                    && m.additivity.1.is_empty()
                    && m.base_minus_full.is_empty(),
                "{name}: the bodies' text is not the sum of each body alone: {:?} / {:?}",
                m.additivity,
                m.base_minus_full
            );
            let got: Vec<Tab> = m.full.spans.iter().map(|s| s.tab).collect();
            assert_eq!(
                got, expect,
                "{name}: the probe recorded a different drawn set"
            );
            check_controls(&name, &m);
            assert_eq!(
                m.full.passes, 1,
                "{name}: the measured present was multi-pass"
            );
            for s in &m.full.spans {
                assert_eq!(
                    s.layer, s.layer_at_leave,
                    "{name}: {:?} changed layer",
                    s.tab
                );
                assert_eq!(
                    s.layer, m.full.spans[0].layer,
                    "{name}: {:?} drew into a different layer from the first body",
                    s.tab
                );
                let in_pass = &m.full.in_pass.iter().find(|(t, _)| *t == s.tab).unwrap().1;
                assert_eq!(
                    in_pass, &s.at_leave,
                    "{name}: {:?}: the span read after the dock differs from the read inside the body",
                    s.tab
                );
            }
            // Spans are disjoint and in draw order.
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
            for (tab, truth, f1, f2) in &m.rows {
                println!(
                    "  {:<12} truth {:>3}  form1 {}  form2 {}",
                    tab.title(),
                    truth,
                    short(f1),
                    short(f2)
                );
                if !f1.pass() || !f2.pass() {
                    failures.push(format!("{name} {tab:?}: form1 {f1:?} form2 {f2:?}"));
                }
                if name == format!("focus {}", tab.title()) {
                    table.push((*tab, (*truth, f1.pass(), f2.pass())));
                }
            }
        }
        println!("--- tab x form (focus arrangement)");
        for (tab, (truth, f1, f2)) in &table {
            println!(
                "  {:<12} {:>3} runs  form1 {}  form2 {}",
                tab.title(),
                truth,
                f1,
                f2
            );
        }
        for t in Tab::ALL {
            let (truth, _, _) = table
                .iter()
                .find(|(x, _)| *x == t)
                .expect("a focus row per tab")
                .1;
            assert!(
                truth > 0,
                "{t:?} painted no text even with 55% of the window, so its verdict is vacuous"
            );
        }
        assert!(
            failures.is_empty(),
            "attribution failed:\n{}",
            failures.join("\n")
        );
    }

    /// ★ **The instrument can see an interleaving.** Every "nothing foreign" verdict above rests on an
    /// absence, so here one is planted: each recorded body paints one extra string into its own painter
    /// just before its span's end is taken, and the ground-truth arms do not. Both forms must report it as
    /// foreign on every drawn tab, and as nothing else.
    #[test]
    fn a_planted_interleaving_is_reported_foreign_in_both_forms() {
        const PLANT: &str = "PLANTED Q3 INTERLOPER";
        let setup = Setup {
            plant: Some(PLANT.into()),
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
        for (tab, truth, f1, f2) in &m.rows {
            assert!(*truth > 0, "{tab:?}: nothing to plant beside");
            for (form, v) in [("form1", f1), ("form2", f2)] {
                assert!(v.missing.is_empty(), "{tab:?} {form}: {:?}", v.missing);
                let foreign: Vec<&str> = v.foreign.iter().map(|k| k.text.as_str()).collect();
                assert_eq!(
                    foreign,
                    [PLANT],
                    "{tab:?} {form}: the plant was not reported foreign"
                );
            }
        }
    }

    /// ★ **A combo box's open list is its own layer, and neither form attributes it to the tab.** The
    /// Watchpoints tab's address-space combo is clicked open on every arm. The popup's text is found in a
    /// layer other than the bodies' (`other_layers`), the ground truth (which is a whole `FullOutput`, so it
    /// sees every layer) holds it, and each form's verdict for the tab is exactly: missing the popup, and
    /// nothing foreign.
    #[test]
    fn an_open_combo_box_list_lands_in_its_own_layer_and_is_not_attributed() {
        let dock = || focus_dock(Tab::Watchpoints);
        let mut lp = fixture(dock());
        let first = settled(&mut lp, Mode::Record, WARM, &|_| Vec::new());
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
        println!(
            "combo popup layers: {:?}",
            m.full
                .other_layers
                .iter()
                .map(|(l, t)| (l, t.len()))
                .collect::<Vec<_>>()
        );
        for want in crate::stopping::WATCH_SPACES {
            assert!(
                items.contains(&want),
                "the combo list did not open into another layer (control): {items:?}"
            );
        }
        let (_, truth, f1, f2) = row(&m, Tab::Watchpoints);
        println!("combo: truth {truth} form1 {f1:?}\nform2 {f2:?}");
        for (form, v) in [("form1", f1), ("form2", f2)] {
            assert!(v.foreign.is_empty(), "{form}: {:?}", v.foreign);
            let (a, b) = diff(&v.missing, &popup);
            assert!(
                a.is_empty() && b.is_empty(),
                "{form}: what the harvest missed is not exactly the popup: extra {a:?}, popup not missed {b:?}"
            );
        }
        for (tab, _, f1, f2) in &m.rows {
            if *tab != Tab::Watchpoints {
                assert!(f1.pass() && f2.pass(), "{tab:?}: {f1:?} {f2:?}");
            }
        }
    }

    /// ★ **A tooltip is its own layer too.** The pointer rests on the Watchpoints tab's `write` checkbox,
    /// which carries hover text; the tooltip's text is in another layer, the truth holds it, and each
    /// form misses exactly it and takes nothing foreign.
    #[test]
    fn a_tooltip_lands_in_its_own_layer_and_is_not_attributed() {
        let dock = || focus_dock(Tab::Watchpoints);
        let mut lp = fixture(dock());
        let first = settled(&mut lp, Mode::Record, WARM, &|_| Vec::new());
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
        println!(
            "tooltip layers: {:?}",
            m.full
                .other_layers
                .iter()
                .map(|(l, t)| (
                    l,
                    t.iter()
                        .map(|k| k.text.chars().take(40).collect::<String>())
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>()
        );
        assert!(
            tip.iter().any(|k| k.text.contains("BOOLEANS")),
            "no tooltip was shown, so this test measures nothing: {tip:?}"
        );
        let (_, _, f1, f2) = row(&m, Tab::Watchpoints);
        for (form, v) in [("form1", f1), ("form2", f2)] {
            assert!(v.foreign.is_empty(), "{form}: {:?}", v.foreign);
            let (a, b) = diff(&v.missing, &tip);
            assert!(
                a.is_empty() && b.is_empty(),
                "{form}: what the harvest missed is not exactly the tooltip: extra {a:?}, tooltip not missed {b:?}"
            );
        }
    }

    /// ★ **A text box's contents and its hint text are the tab's**, in both forms. `w_target` is typed
    /// into; `w_stop_after` stays empty and so shows its hint `∞`.
    #[test]
    fn text_edit_contents_and_hint_text_are_attributed_to_their_tab() {
        const TYPED: &str = "q3 typed into the watch target";
        let m = measure_with(
            "text edit",
            focus_dock(Tab::Watchpoints),
            WARM,
            &|_| Vec::new(),
            &mut |lp| lp.stopping.w_target = TYPED.into(),
            &Setup::default(),
        );
        check_controls("text edit", &m);
        let (_, _, f1, f2) = row(&m, Tab::Watchpoints);
        assert!(f1.pass() && f2.pass(), "{f1:?} {f2:?}");
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
    }
}
