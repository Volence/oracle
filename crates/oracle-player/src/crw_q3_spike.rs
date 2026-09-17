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
        /// Record indices and clip only, no text read at `leave`: the cost arms' probe, which must cost
        /// what the shipped hook would.
        pub lean: bool,
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
                lean: false,
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
        let lean = PROBE.with(|p| p.borrow().as_ref().is_some_and(|p| p.lean));
        let (end, at_leave) = ui.ctx().graphics(|g| {
            let l = g.get(layer).expect("the body's layer has a paint list");
            let end = l.next_idx().0;
            if lean {
                (end, Vec::new())
            } else {
                (end, texts(l.all_entries().skip(start).take(end - start)))
            }
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

/// **What the implementing parcel's harvest would compute for one run, per CR §10**: the source with its
/// own TAB/LF folded, the glyphs whose logical rectangles meet the clip, and whether the galley is elided.
/// Rows are grouped by glyph-row top and runs sorted by left edge; runs on a row joined by TAB, rows by
/// LF. Here for the cost measurement, so it does the work the real one would; its output is not a
/// contract.
pub(crate) fn harvest_runs<'a>(
    shapes: impl IntoIterator<Item = &'a ClippedShape>,
) -> (String, String) {
    struct Run {
        top: i64,
        left: f32,
        source: String,
        rendered: String,
    }
    fn walk(s: &Shape, clip: Rect, out: &mut Vec<Run>) {
        match s {
            Shape::Text(t) => {
                let mut rendered = String::new();
                let mut top = None;
                let mut left = f32::INFINITY;
                for row in &t.galley.rows {
                    for g in &row.glyphs {
                        let r = g
                            .logical_rect()
                            .translate(t.pos.to_vec2() + row.pos.to_vec2());
                        if r.intersects(clip) {
                            rendered.push(if g.chr == '\t' || g.chr == '\n' {
                                ' '
                            } else {
                                g.chr
                            });
                            top.get_or_insert((r.min.y * 4.0).round() as i64);
                            left = left.min(r.min.x);
                        }
                    }
                }
                if let Some(top) = top {
                    let source = t.galley.text().replace(['\t', '\n'], " ");
                    let _elided = t.galley.elided;
                    out.push(Run {
                        top,
                        left,
                        source,
                        rendered,
                    });
                }
            }
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    let mut runs = Vec::new();
    for c in shapes {
        walk(&c.shape, c.clip_rect, &mut runs);
    }
    runs.sort_by(|a, b| a.top.cmp(&b.top).then(a.left.total_cmp(&b.left)));
    let (mut text, mut rendered) = (String::new(), String::new());
    let mut row = None;
    for r in &runs {
        if let Some(prev) = row {
            let sep = if prev == r.top { '\t' } else { '\n' };
            text.push(sep);
            rendered.push(sep);
        }
        row = Some(r.top);
        text.push_str(&r.source);
        rendered.push_str(&r.rendered);
    }
    (text, rendered)
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
    /// Rebuild the dock at the start of every arm. A floating window's position is handed to egui once
    /// and then lives in the context's memory, so a dock reused across fresh contexts puts the window
    /// somewhere else on the second arm (measured: 28 runs moved).
    pub dock: Option<fn() -> egui_dock::DockState<Tab>>,
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
    if let Some(dock) = setup.dock {
        lp.dock = dock();
    }
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

    /// ★ **A tab dragged out into a floating window, over another tab's body: form 1 stays exact, form 2
    /// does not.** `egui_dock` draws a window surface's body in the window's own `Middle` layer, so form
    /// 1's span is in a different paint list and nothing crosses. Form 2 has only rectangles: every run of
    /// the window's body, and the window's own tab title, has a clip rectangle inside the body clip of the
    /// Screen tab beneath it, so all of them are attributed to Screen as well. Reachable in the shipped
    /// window: `draggable_tabs(true)`, `TabViewer::allowed_in_windows` is not overridden, and
    /// `crate::layout` stores window surfaces.
    #[test]
    fn a_floating_window_over_a_body_is_exact_in_form_1_and_leaks_into_that_body_in_form_2() {
        fn dock() -> egui_dock::DockState<Tab> {
            let mut dock = crate::ui::initial_dock();
            let w = dock.add_window(vec![Tab::Profiler]);
            dock.get_window_state_mut(w)
                .expect("the window just added")
                .set_position(egui::pos2(100.0, 150.0))
                .set_size(egui::vec2(500.0, 400.0));
            dock
        }
        let setup = Setup {
            dock: Some(dock),
            ..Setup::default()
        };
        let m = measure_with(
            "window over Screen",
            dock(),
            WARM,
            &|_| Vec::new(),
            &mut |_| {},
            &setup,
        );
        check_controls("window over Screen", &m);
        let spans = &m.full.spans;
        let screen = spans
            .iter()
            .find(|s| s.tab == Tab::Screen)
            .expect("Screen drawn");
        let window = spans
            .iter()
            .find(|s| s.tab == Tab::Profiler)
            .expect("the window's body drawn");
        // Controls: the window really is a separate layer, and really does sit over Screen's body.
        assert_ne!(
            window.layer, screen.layer,
            "the window body shares the main layer"
        );
        assert!(
            screen.body_clip.contains_rect(window.body_clip),
            "the window is not over Screen's body, so form 2 was never tested: {:?} {:?}",
            window.body_clip,
            screen.body_clip
        );
        for (tab, truth, f1, _) in &m.rows {
            assert!(*truth > 0, "{tab:?}: vacuous");
            assert!(f1.pass(), "form 1, {tab:?}: {f1:?}");
        }
        let (_, profiler_truth, _, f2_profiler) = row(&m, Tab::Profiler);
        assert!(
            f2_profiler.pass(),
            "form 2, the window's own body: {f2_profiler:?}"
        );
        let (_, _, _, f2_screen) = row(&m, Tab::Screen);
        assert!(f2_screen.missing.is_empty(), "{:?}", f2_screen.missing);
        let leaked: Vec<&str> = f2_screen.foreign.iter().map(|k| k.text.as_str()).collect();
        assert_eq!(
            f2_screen.foreign.len(),
            profiler_truth + 1,
            "form 2 should take the window's whole body and its title into Screen: {leaked:?}"
        );
        assert!(
            leaked.contains(&Tab::Profiler.title()),
            "the window's tab title: {leaked:?}"
        );
    }

    /// **The Screen tab's picture overlay, painted through `Painter::with_clip_rect(picture)`, is the
    /// tab's in both forms** even when the pane is narrower than the picture. `with_clip_rect` intersects
    /// with the painter's own clip (`egui-0.36.1/src/painter.rs:73`), so the overlay's clip cannot leave
    /// the body. Control: the armed notice's clip is cut at the body clip's right edge, so the picture
    /// really was wider than the pane.
    #[test]
    fn the_screen_overlay_in_a_pane_narrower_than_the_picture_is_attributed_in_both_forms() {
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
        for (tab, truth, f1, f2) in &m.rows {
            assert!(
                *truth > 0 && f1.pass() && f2.pass(),
                "{tab:?}: {f1:?} {f2:?}"
            );
        }
        let screen = m.full.spans.iter().find(|s| s.tab == Tab::Screen).unwrap();
        let notice = lp_notice(&m);
        assert_eq!(
            notice.clip[2],
            (screen.body_clip.max.x * 100.0).round() as i64,
            "the overlay was not cut by the pane edge, so this is not the narrow case: {notice:?}"
        );
        assert!(
            notice.clip[0] > (screen.body_clip.min.x * 100.0).round() as i64,
            "the overlay's clip is the pane's rather than the picture's: {notice:?}"
        );
    }

    fn lp_notice(m: &Measured) -> Key {
        span_texts(&m.full, Tab::Screen)
            .iter()
            .find(|k| k.text.starts_with("ring placement armed"))
            .cloned()
            .expect("the armed chip on the picture")
    }

    /// **The harvest's cost per present**, form 1 and form 2 against a null arm, under the default dock
    /// and `--dock every-tab`'s. Ignored: it is a measurement, run in release:
    /// `cargo test --release -p oracle-player harvest_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "measurement; run in release with --ignored --nocapture"]
    fn harvest_cost_per_present() {
        use std::time::Duration;
        let rounds: usize = std::env::var("Q3_ROUNDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);
        for (name, dock) in [
            ("default", crate::ui::initial_dock()),
            ("every-tab", crate::ui::every_tab_dock()),
        ] {
            let mut lp = fixture(dock);
            let ctx = egui::Context::default();
            crate::theme::install(&ctx, crate::theme::DEFAULT_FAMILY);
            // [null, form 1, form 2]: whole present, and the harvest alone.
            let mut whole: [Vec<Duration>; 3] = Default::default();
            let mut part: [Vec<Duration>; 3] = Default::default();
            let mut bytes = [0usize; 3];
            for i in 0..(rounds + 30) {
                for k in 0..3 {
                    let arm = (i + k) % 3;
                    if arm > 0 {
                        probe::install(Mode::Record);
                        probe::with(|p| p.lean = true);
                    }
                    let t0 = Instant::now();
                    let mut spent = Duration::ZERO;
                    let mut made = 0usize;
                    let mut out = ctx.run_ui(raw(i as u32, Vec::new()), |root| {
                        if arm > 0 {
                            probe::with(|p| p.spans.clear());
                        }
                        let _ = lp.build_ui(root);
                        if arm == 1 {
                            let t = Instant::now();
                            let spans = probe::with(|p| std::mem::take(&mut p.spans));
                            let got: Vec<(Tab, String, String)> = root.ctx().graphics(|g| {
                                spans
                                    .iter()
                                    .map(|s| {
                                        let l = g.get(s.layer).expect("paint list");
                                        let (a, b) = harvest_runs(
                                            l.all_entries().skip(s.start).take(s.end - s.start),
                                        );
                                        (s.tab, a, b)
                                    })
                                    .collect()
                            });
                            made = got.iter().map(|(_, a, b)| a.len() + b.len()).sum();
                            spent = t.elapsed();
                        }
                    });
                    if arm == 2 {
                        let t = Instant::now();
                        let spans = probe::with(|p| std::mem::take(&mut p.spans));
                        let got: Vec<(Tab, String, String)> = spans
                            .iter()
                            .map(|s| {
                                let (a, b) = harvest_runs(out.shapes.iter().filter(|c| {
                                    s.body_clip.expand(0.01).contains_rect(c.clip_rect)
                                }));
                                (s.tab, a, b)
                            })
                            .collect();
                        made = got.iter().map(|(_, a, b)| a.len() + b.len()).sum();
                        spent = t.elapsed();
                    }
                    let dt = t0.elapsed();
                    out.textures_delta.clear();
                    probe::uninstall();
                    if i >= 30 {
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
            for (arm, label) in [
                "null (harvest off)",
                "form 1 (in-pass span)",
                "form 2 (FullOutput by clip)",
            ]
            .iter()
            .enumerate()
            {
                let w = stat(&mut whole[arm]);
                let h = stat(&mut part[arm]);
                println!(
                    "COST {name:<9} {label:<28} present median {:.3} ms p95 {:.3} ms | harvest median {:.4} ms p95 {:.4} ms | n {} | {} bytes of text+rendered",
                    w.0, w.1, h.0, h.1, w.2, bytes[arm]
                );
            }
        }
    }

    /// **A present egui runs twice is harvested from the pass it keeps.** A fresh context's first
    /// present is two passes (measured: `[2, 1, 1, ...]` under the default dock, `every-tab` and a focus
    /// arrangement), because the first pass requests a discard. `Context::run_ui` re-runs the whole
    /// closure, so a per-present span list must be reset at the start of each pass or the discarded
    /// pass's spans, whose indices point into a paint list that no longer exists, ride along. Here the
    /// spans are the drawn set once, and every span's in-pass text equals what form 2 found in the
    /// `FullOutput` egui kept.
    #[test]
    fn a_discarded_first_pass_is_harvested_from_the_pass_that_is_kept() {
        let dock = crate::ui::initial_dock();
        let expect = active_tabs(&dock);
        let mut lp = fixture(dock);
        lp.planes = crate::planes::Panel::default();
        let ctx = context();
        let p = present(&mut lp, &ctx, raw(0, Vec::new()), Mode::Record);
        assert_eq!(p.passes, 2, "control: the first present was not multi-pass");
        let got: Vec<Tab> = p.spans.iter().map(|s| s.tab).collect();
        assert_eq!(got, expect, "spans from more than the kept pass");
        for (tab, keys) in &p.in_pass {
            let f2 = &p
                .form2
                .iter()
                .find(|(t, _)| t == tab)
                .expect("form 2 row")
                .1;
            assert!(!keys.is_empty(), "{tab:?}: vacuous");
            assert_eq!(
                keys, f2,
                "{tab:?}: the in-pass read is not the kept pass's text"
            );
            let (stray, _) = diff(keys, &p.out);
            assert!(
                stray.is_empty(),
                "{tab:?}: harvested text egui did not keep: {stray:?}"
            );
        }
    }
}
