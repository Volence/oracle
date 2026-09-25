//! **What the player's window says, as a snapshot a client can read** — the player half of
//! `emulator/screen_text` (contract §11.29, CR-H), booked as unwired by design §5.8.2 and closed here.
//!
//! # What is on the glass, and what is not
//!
//! `oracle-frontend` composes its snapshot from a title, an overlay and a status line, and this module
//! does the same job for a window whose chrome is a different shape: a top bar and **eleven dockable
//! panels** that are mostly text. It reports three kinds: `titleBar`, `statusLine`, and — since §11.50
//! (CR-W, `docs/proposed/2026-09-17-cr-w-panel-screen-text.md`) — one **`panel`** surface per panel whose
//! body was drawn on this present.
//!
//! This module used to refuse panels, in writing, on three arguments. CR-W §1.1 reversed that, and each
//! argument is answered here rather than stepped past:
//!
//! * ⚑ **"A snapshot listing all eleven would report text nobody can see." Agreed, and kept.**
//!   `egui_dock` draws only the ACTIVE tab of a leaf. [`crate::ui::initial_dock`] puts
//!   Registers/Memory/Objects in one pane and Breakpoints/Watchpoints/Profiler in another, so **seven of
//!   the eleven** panel bodies do not run on a given frame — that fact is load-bearing enough
//!   that this crate grew `--dock every-tab` ([`crate::ui::every_tab_dock`]) to make a cost measurement
//!   mean anything. *(This sentence has been wrong twice — `six of eight`, then `four of eight` — and the
//!   rule underneath it never changed: `egui_dock` draws one body per **leaf**, so bodies-in-front is the
//!   **leaf count** (four) and everything else is hidden. Chasing the figure is not the repair; the enum
//!   grows. Derived in
//!   `nav::tests::the_default_layout_hides_one_body_per_shared_pane_and_the_count_is_measured`, which has
//!   survived both drifts untouched, and these sentences are now pinned to `Tab::ALL` by
//!   `nav::tests::panel_counts_in_prose_match_the_enum`. Lens finding H19.)* So the answer is **drawn
//!   bodies only**: a panel surface exists exactly when `TabViewer::ui` ran that panel's body this pass.
//!   A tab behind another in its pane and a collapsed pane have no surface; a tab floated into a window is
//!   drawn, and is reported after the main surface's panels, in the order the window drew them.
//! * **"What a panel body reveals depends on the pane's pixel height and scroll offset, computed inside
//!   egui's painting loop; restating them is the drift `oracle-frontend`'s rule 2 forbids." True of
//!   restating, and nothing here restates.** The harvest reads what egui PAINTED, after the fact: the
//!   shapes each body appended to its layer's paint list, their clip rectangles, each galley's `elided`
//!   flag and its laid-out glyphs. Those are the renderer's own inputs, so there is no second layout to
//!   drift from the first. That seam did not exist in the design space the old paragraph considered, and
//!   the CR-W Q3 spike (`docs/2026-09-17-cr-w-q3-spike.md`) measured it: every drawn body's text is
//!   attributed completely and exclusively by the in-pass paint-list span ("form 1"), including under a
//!   floating window, where attributing by clip rectangle ("form 2") is NOT exclusive and is not used.
//! * **"Six of the eleven panels have another reader anyway." True of the values, and beside the point.**
//!   What an agent is asked about a panel is how it READS: a cut cell, a hollow box, a wrong sentence, an
//!   empty state. No served row carries that. The values stay on their rows, and §11.29's no-join clause
//!   reaches panels with full force: a number read off a panel is a snapshot of a rendering, never a
//!   source. Ask the bus.
//!
//! # How a panel surface is read off the paint list ([`PanelSpan`], [`panels`])
//!
//! * **Recording.** `TabViewer::ui` notes `ui.layer_id()` and that layer's `PaintList::next_idx()` before
//!   and after the body's `match` ([`PanelMark`]). The shapes between are exactly that body's. The list is
//!   a local of `build_ui`, so it is **per pass** by construction: `Context::run_ui` re-runs the whole
//!   closure on a discarded pass, and a span kept from a discarded pass would index a drained list.
//! * **Reading.** After `build_ui` returns and before the pass ends — the layer's list is drained by
//!   `end_pass` — each span is read **from its own recorded layer** (a floating window's body is in a
//!   `Middle` layer of its own). `Context::graphics` takes the context's WRITE lock, so the read clones the
//!   galleys out in one call and nothing else runs inside it: [`Glyphs`] takes the fonts lock later.
//! * **A run** is one `Shape::Text` the toolkit laid a glyph out in, on a row the surface reports. An
//!   empty galley (a TextEdit with nothing in it) is not a run — §11.50's own parenthetical, *an empty
//!   text SHAPE is not a run* — and neither is a label on a row nothing on the panel showed.
//!   `text` gets the galley's SOURCE (`Galley::text`); `rendered` gets the glyphs that meet the clip, the
//!   elision mark included. A run's own TAB or LF is folded to a space in both.
//! * ⚑ **A run whose glyphs the clip ate ENTIRELY is still a run**, with an empty `rendered`, so
//!   `truncated` derives TRUE and total loss is not the one cut a client cannot see (§11.50 as amended
//!   2026-09-18, `F-PANEL-CLIP-TOTAL-LOSS-UNSIGNALLED`; the clause *"laid out on a reported row"* replaced
//!   *"with at least one glyph on the glass"*, and the empty run is that clause's consequence, not a case
//!   [`glass_run`] knows about). It is placed by the galley's row geometry, which is computed whether or
//!   not the clip kept anything.
//! * **Rows.** Runs are grouped into visual rows by the band of the glyph row that places them, ordered
//!   top to bottom, and left to right within a row; runs on a row are joined by TAB and rows by LF, in
//!   both strings identically, so row *k* run *j* of `rendered` renders row *k* run *j* of `text`.
//! * **Scrolled out is not truncation.** A row nothing on the panel showed is in neither string
//!   ([`reported_rows`]), so rows off the view are absent as before. A panel surface is what the panel
//!   shows, never its content.
//! * **A drawn panel with no text is present, with `""`.** "Not on screen" and "on screen and blank" stay
//!   different artifacts.
//!
//! # One derivation, two consumers — enforced by the return type
//!
//! The bar does not get *read back*; it **hands over what it drew**. [`crate::ui::Transport::bar`] and
//! [`crate::main`]'s `build_ui` return the [`Run`]s they just painted, and `Loop::iterate` pushes them.
//! That is stronger than a helper both sides happen to call: there is no second expression to drift, and
//! the snapshot **cannot be composed before the bar draws it**, which is the ordering
//! [`oracle_aether::host::Host::set_screen_text`] requires. Pushing text that describes a frame not yet
//! presented is the trap that method's own doc names; here it is a type error rather than a rule. The
//! panel spans follow the same rule: `build_ui` returns them, so they cannot be read before the bodies
//! ran.
//!
//! # Kinds this module does not produce, said out loud
//!
//! The contract's `kind` enum has six values; this module produces **three**, `titleBar`, `statusLine` and
//! `panel`.
//!
//! * **`toast`** — the player has none. The nearest thing is the transport bar's [`crate::ui::Echo`], the
//!   bus's verbatim answer to the last button click; it is persistent chrome inside the top bar rather
//!   than a transient overlay, so it is reported as part of the `statusLine` run it is drawn in, not as a
//!   toast it is not.
//! * **`palette`**, **`lens`** — `oracle-frontend`'s reasons (`F-SCREEN-TEXT-PALETTE-LENS`). The player's
//!   command palette is still reported inside the `statusLine` run list (`build_ui` appends its runs);
//!   serving it as `palette` needs no CR and is not done here.
//! * **Tooltips, popups, combo lists** — each is its own `Area` layer, outside every body's span, so no
//!   panel surface carries it (measured by the Q3 spike). They have no kind; booked
//!   `F-SCREEN-TEXT-TRANSIENT-AREAS`.

use oracle_aether::engine::{ScreenSurface, ScreenSurfaceKind};

/// **One text run the top bar drew**, handed back by the code that drew it.
///
/// Three fields and every one is what a *faithful* readback needs rather than a convenience:
///
/// * `text` — the string the widget was given.
/// * `mono` — whether the bar drew it with `ui.monospace`. `epaint`'s `Fonts::has_glyph` resolves through
///   `font_id.family` and ignores the size entirely (`epaint-0.36.1/src/text/fonts.rs:858`), so this one
///   bool is the whole of what a glyph probe needs to ask the *right* font about the *right* run. Measuring
///   a monospace run against the proportional family — or the reverse — is a claim about a font that never
///   drew it, which is the mistake `oracle-frontend`'s title-bar note exists to name.
/// * `sep_before` — whether a `ui.separator()` (a drawn vertical rule) precedes this run. Set at the call
///   site that makes the separator, so the joined line groups the way the bar groups instead of this module
///   inventing a grouping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub mono: bool,
    pub sep_before: bool,
}

impl Run {
    /// A proportional run with no separator before it — `ui.strong`, `ui.button`, `ui.weak`,
    /// `ui.colored_label`.
    pub fn label(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            mono: false,
            sep_before: false,
        }
    }

    /// The same, immediately after a drawn `ui.separator()`.
    pub fn after_sep(text: impl Into<String>) -> Self {
        Self {
            sep_before: true,
            ..Self::label(text)
        }
    }

    /// A `ui.monospace` run after a drawn `ui.separator()`.
    pub fn mono_after_sep(text: impl Into<String>) -> Self {
        Self {
            mono: true,
            ..Self::after_sep(text)
        }
    }
}

/// What stands in for a drawn `ui.separator()` when the runs are joined into one line.
///
/// The bar's separator is a **vertical rule**, not a character, so this string is this module's and is the
/// one piece of the line that was not on the glass as text. It is a constant rather than a literal at the
/// join site because the test that checks the grouping reads *this*, and a separator a test retypes is a
/// separator that can quietly become the empty string.
pub const SEP: &str = " | ";

/// What stands between two runs the bar drew side by side with no separator (the two transport buttons).
pub const GAP: &str = "  ";

/// **Characters no face in egui's default font set can draw** — the reference the glyph probe measures
/// against, and the reason it needs two of them rather than one.
///
/// A character the window cannot draw is rendered as the *replacement glyph* (`◻`), so *"is this a hollow
/// box?"* is answerable exactly: lay the character out and compare the atlas rectangle its glyph will
/// sample against the rectangle a **known-absent** character samples. Same rectangle, same pixels, same box.
/// Two references rather than one because a single one proves nothing on its own — if the font set ever
/// gains a Han ideograph, one reference silently becomes a real glyph and every character in the bar is
/// suddenly "not a box", which is a measurement that has quietly stopped measuring. Two, drawn from
/// unrelated scripts, must agree; when they do not, [`Glyphs`] reports the family **unmeasurable** rather
/// than reporting confident nonsense.
///
/// U+6F22 (Han) and U+AA00 (Cham). Measured on egui 0.36's defaults — both share one rectangle in each
/// family, and `A`, `·`, `—` and `▶` each have their own.
pub const GLYPH_REFERENCES: [char; 2] = ['\u{6F22}', '\u{AA00}'];

/// The atlas rectangle one glyph samples: `(min, max)` of `epaint::text::Glyph::uv_rect`.
type Uv = ([u16; 2], [u16; 2]);

/// **Can this window draw this character?** — asked of the live `egui::Context`, and answered from the
/// glyph the window will actually sample.
///
/// # ⚑ Why this is not `Fonts::has_glyph`, which is the obvious answer and is wrong
///
/// `epaint`'s `Font::has_glyph` is `resolve_face(c) != cached_family.replacement_face_key`
/// (`epaint-0.36.1/src/text/font.rs:719`) — it asks *"is this char owned by the same **face** that owns
/// `◻`?"*, not *"can this char be drawn?"*. Its own `TODO` calls that a false negative for `◻` itself. It
/// is much worse than that in two ways this parcel measured, and both were live on the player's own bar:
///
/// ```text
/// Proportional: A=true  ▶=false  ⏸=true  ⏭=true    (▶ is DRAWN; its atlas rect is its own)
/// Monospace:    A=false ·=false  —=false ■=false    (all DRAWN; the whole family answers false)
/// ```
///
/// egui's default `Monospace` chain is `["Hack", "Ubuntu-Light", "NotoEmoji-Regular", "emoji-icon-font"]`
/// and its **primary** face owns `◻`, so every character resolves to the replacement face and the answer is
/// `false` for all of them. A snapshot that trusted `has_glyph` published **26 invented hollow boxes** on a
/// window drawing every one of those characters correctly — 25 for the status line, and `▶` on the resume
/// button. That is not a weaker readout, it is a wrong one, and a wrong answer here is worse than none.
///
/// The atlas comparison has no such failure mode: it reads the same `uv_rect` the renderer samples, so it
/// agrees with the pixels by construction. Its one known edge is `◻` *itself*, which is genuinely
/// indistinguishable from a replaced character because it renders identically — epaint's `TODO` names the
/// same edge, and the player's bar contains no `◻`.
pub struct Glyphs<'a> {
    ctx: &'a egui::Context,
    /// Per family (index 0 proportional, 1 monospace): `None` not yet measured; `Some(None)` the two
    /// references disagreed, so this family is unmeasurable; `Some(Some(uv))` the replacement rectangle.
    replacement: [Option<Option<Uv>>; 2],
    /// One layout per distinct character per family per frame, not one per occurrence. `epaint` caches
    /// galleys, so this saves a hash rather than a rasterisation — worth it anyway on a line that repeats
    /// `e` nine times, and it keeps the cost of the readback off the frame budget's hot edge.
    seen: std::collections::HashMap<(char, bool), Option<bool>>,
}

impl<'a> Glyphs<'a> {
    /// Borrow the live context. Must be called inside a frame: `Context::fonts_mut` panics before the
    /// first `Context::run`.
    pub fn new(ctx: &'a egui::Context) -> Self {
        Self {
            ctx,
            replacement: [None, None],
            seen: std::collections::HashMap::new(),
        }
    }

    fn uv(&self, c: char, mono: bool) -> Option<Uv> {
        let family = if mono {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        };
        self.ctx.fonts_mut(|f| {
            let galley = f.layout_no_wrap(
                c.to_string(),
                egui::FontId::new(12.0, family),
                egui::Color32::WHITE,
            );
            galley
                .rows
                .first()
                .and_then(|r| r.glyphs.first())
                .map(|g| (g.uv_rect.min, g.uv_rect.max))
        })
    }

    /// The rectangle this family draws for a character it does not have, or `None` when the two
    /// [`GLYPH_REFERENCES`] disagree and the family therefore cannot be measured.
    fn replacement(&mut self, mono: bool) -> Option<Uv> {
        let slot = usize::from(mono);
        if self.replacement[slot].is_none() {
            let a = self.uv(GLYPH_REFERENCES[0], mono);
            let b = self.uv(GLYPH_REFERENCES[1], mono);
            self.replacement[slot] = Some(if a.is_some() && a == b { a } else { None });
        }
        self.replacement[slot].expect("just filled")
    }

    /// `Some(true)` drawn, `Some(false)` a hollow box, `None` this family cannot be measured.
    pub fn drawable(&mut self, c: char, mono: bool) -> Option<bool> {
        if let Some(known) = self.seen.get(&(c, mono)) {
            return *known;
        }
        let answer = match (self.replacement(mono), self.uv(c, mono)) {
            (Some(repl), Some(uv)) => Some(uv != repl),
            // No replacement rectangle: unmeasurable. No glyph at all: the layout produced nothing to
            // look at, which is not evidence of a box either.
            _ => None,
        };
        self.seen.insert((c, mono), answer);
        answer
    }
}

/// **The whole snapshot, in the order a reader meets it**: the window title the desktop drew, then the top
/// bar the player drew.
///
/// `probe` answers *can the window draw this character*, for a run's own family — and it answers in **three
/// states, not two**: `Some(true)` drawable, `Some(false)` a hollow box, `None` **this family cannot be
/// measured on this build**. See [`Glyphs`] for the third state, which is not hypothetical: it is what
/// happens when the two [`GLYPH_REFERENCES`] disagree and the measurement has quietly stopped measuring.
///
/// A parameter rather than a call, so this module's tests can drive all three answers and nothing here needs
/// an `egui::Context` that exists only inside a frame.
///
/// **Only `Some(false)` reaches `unrenderable`.** That is the one direction it is safe to be weak in: a
/// missed box is a defect this readout does not name, whereas an *invented* box is this readout claiming a
/// defect the window does not have — a wrong answer, which is worse than no answer. `None` is folded into
/// the same empty list as "nothing missing", which the wire cannot distinguish; `oracle-frontend`'s
/// `titleBar` already makes that exact compromise for that exact reason, and the limitation is registered as
/// `F-PLAYER-SCREENTEXT-GLYPHS` rather than left for a reader to infer.
///
/// # `rendered` equals `text`, and that is an admission rather than a shortcut
///
/// `oracle-frontend` fills `rendered` from its own `fit`, because it lays out 5×7 glyphs into pixel columns
/// itself and therefore *knows* what it cut. This window does not: egui measures and clips inside its
/// painting loop, and a top bar wider than the window is clipped by the backend with nothing reported back.
/// So `rendered == text` for every surface here and `truncated` derives to `false` — which is exactly the
/// argument `Surface::window_manager` makes for the frontend's own title bar (*"whatever elision the window
/// manager applies … is invisible to this process"*), applied to a second surface for the same reason.
/// **Registered as `F-PLAYER-SCREENTEXT-CLIP`**: a `Galley`-width measurement against the bar's own
/// `available_width` would make the flag real, and it needs the layout the bar has already discarded by the
/// time this runs.
pub fn snapshot(
    title: &str,
    runs: &[Run],
    probe: &mut dyn FnMut(char, bool) -> Option<bool>,
) -> Vec<ScreenSurface> {
    let mut line = String::new();
    let mut unrenderable: Vec<String> = Vec::new();
    for (i, run) in runs.iter().enumerate() {
        if i > 0 {
            line.push_str(if run.sep_before { SEP } else { GAP });
        }
        line.push_str(&run.text);
        for c in run.text.chars() {
            // Asked of the family that DREW this run — see `Run::mono`. First appearance order,
            // de-duplicated: a bar with four undrawable middle dots has one defect, not four.
            if probe(c, run.mono) == Some(false) {
                let s = c.to_string();
                if !unrenderable.contains(&s) {
                    unrenderable.push(s);
                }
            }
        }
    }
    vec![
        ScreenSurface {
            kind: ScreenSurfaceKind::TitleBar,
            rendered: title.to_string(),
            text: title.to_string(),
            // Empty **by construction, and that is the honest answer rather than a gap**: the desktop
            // paints this string with the desktop's font, so neither egui family says anything about what
            // a reader sees there. `oracle-frontend`'s `Surface::window_manager` makes the identical call.
            unrenderable: Vec::new(),
        },
        ScreenSurface {
            kind: ScreenSurfaceKind::StatusLine,
            rendered: line.clone(),
            text: line,
            unrenderable,
        },
    ]
}

// ---------------------------------------------------------------------------------------------------------
// Panels (§11.50, CR-W): what each drawn body painted, read off its layer's paint list
// ---------------------------------------------------------------------------------------------------------

use oracle_aether::engine::PanelName;
use std::sync::Arc;

/// **One drawn panel body's slice of its layer's paint list**: the shapes at `[start, end)` of `layer`'s
/// list are exactly the shapes that body painted (CR-W Q3, form 1).
///
/// Recorded by `TabViewer::ui` through [`PanelMark`], returned by `build_ui`, read by [`panels`] in the
/// same pass. Never kept across passes: the indices name a list `end_pass` drains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelSpan {
    /// The title the panel's own tab bar draws (`Tab::title`), which is what `panel` reports.
    pub name: &'static str,
    /// The layer the body drew into — the main surface's, or a floating window's own `Middle` layer.
    pub layer: egui::LayerId,
    pub start: usize,
    pub end: usize,
}

/// **The half of a [`PanelSpan`] known before the body runs.** Taken at the top of `TabViewer::ui` and
/// closed by [`PanelMark::leave`] at the bottom; the body's `match` sits between the two.
#[derive(Clone, Copy, Debug)]
pub struct PanelMark {
    layer: egui::LayerId,
    start: usize,
}

impl PanelMark {
    /// Note the body's layer and the index the body's first shape will take.
    ///
    /// ⚑ `Context::graphics` takes the context's **write** lock (`egui-0.36.1/src/context.rs:1044`), so
    /// this must not be called from inside another context closure. `TabViewer::ui` is a `Ui` callback,
    /// not a context closure, which is why it is safe there.
    pub fn enter(ui: &egui::Ui) -> Self {
        let layer = ui.layer_id();
        let start = ui
            .ctx()
            .graphics(|g| g.get(layer).map_or(0, |l| l.next_idx().0));
        Self { layer, start }
    }

    /// The layer the body draws into.
    pub fn layer(&self) -> egui::LayerId {
        self.layer
    }

    /// The paint-list index the body's first shape took.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Close the span: the body painted everything between `start` and the list's next index now.
    pub fn leave(self, ui: &egui::Ui, name: &'static str) -> PanelSpan {
        let end = ui
            .ctx()
            .graphics(|g| g.get(self.layer).map_or(self.start, |l| l.next_idx().0));
        PanelSpan {
            name,
            layer: self.layer,
            start: self.start,
            end,
        }
    }
}

/// **One `Shape::Text` a body painted**, with what is needed to decide which of its glyphs reached the
/// glass: where it was painted and the clip it was painted under.
#[derive(Clone, Debug)]
pub struct Painted {
    pub galley: Arc<egui::Galley>,
    pub pos: egui::Pos2,
    pub clip: egui::Rect,
}

/// **Every text shape in each span, read from each span's own layer**, in paint order, one `Vec` per span.
///
/// One `Context::graphics` call for all spans, and nothing inside it but cloning `Arc`s: that call holds
/// the context's write lock, and [`Glyphs`] needs the fonts afterwards. Must run **in the pass** that
/// recorded the spans — after `end_pass` the lists are empty (measured by the Q3 spike) and a span that
/// indexes past a list's end reads nothing rather than panicking.
pub fn painted(ctx: &egui::Context, spans: &[PanelSpan]) -> Vec<Vec<Painted>> {
    fn walk(shape: &egui::Shape, clip: egui::Rect, out: &mut Vec<Painted>) {
        match shape {
            egui::Shape::Text(t) => out.push(Painted {
                galley: t.galley.clone(),
                pos: t.pos,
                clip,
            }),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    ctx.graphics(|g| {
        spans
            .iter()
            .map(|span| {
                let mut out = Vec::new();
                if let Some(list) = g.get(span.layer) {
                    for c in list
                        .all_entries()
                        .skip(span.start)
                        .take(span.end.saturating_sub(span.start))
                    {
                        walk(&c.shape, c.clip_rect, &mut out);
                    }
                }
                out
            })
            .collect()
    })
}

/// A run's TAB and LF, folded to a space (§11.50, Q7 adopted): without the fold, a label carrying its own
/// line break would shift every row index after it, and row/run alignment would hold only "usually".
fn fold(c: char) -> char {
    if c == '\t' || c == '\n' {
        ' '
    } else {
        c
    }
}

/// Whether `r` meets `clip` — **on the glass, however little of it**. A zero-width rectangle (an empty
/// row, a zero-advance glyph) meets the clip when its edge lies inside it, so it is not lost for having no
/// area.
fn meets(r: egui::Rect, clip: egui::Rect) -> bool {
    let x = if r.width() > 0.0 {
        r.min.x < clip.max.x && r.max.x > clip.min.x
    } else {
        r.min.x >= clip.min.x && r.min.x <= clip.max.x
    };
    let y = if r.height() > 0.0 {
        r.min.y < clip.max.y && r.max.y > clip.min.y
    } else {
        r.min.y >= clip.min.y && r.min.y <= clip.max.y
    };
    x && y
}

/// **One run as the glass has it**: the source, the glyphs that reached the glass, and the vertical band
/// and left edge of the glyph row that places it among the panel's visual rows.
///
/// The band is **layout**, never visibility: it is read off the galley's own row rectangles, which the
/// toolkit computes for every row whether or not the clip kept any of it. That is what lets a run whose
/// glyphs the clip ate entirely still be placed on its row (§11.50 as amended 2026-09-18) instead of
/// needing a source no invisible run has.
#[derive(Clone, Debug, PartialEq)]
pub struct GlassRun {
    pub text: String,
    pub rendered: String,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    /// `Galley::elided`, the toolkit's own answer. Carried for tests and for the record; the wire's
    /// `truncated` is still derived from `rendered != text` by the handler.
    pub elided: bool,
}

/// **The run one painted galley contributes, or `None` when the toolkit laid out no glyph at all** — a
/// TextEdit's empty galley. §11.50's own parenthetical: *an empty text SHAPE is not a run*.
///
/// ⚑ **Visibility decides what lands in `rendered`, and nothing else.** §11.50 as amended
/// 2026-09-18 (`F-PANEL-CLIP-TOTAL-LOSS-UNSIGNALLED`) substitutes the clause that decides this: `text` is
/// every run **"that the toolkit laid out on a reported row"**, where it used to read *"with at least one
/// glyph on the glass"*. So a run the clip ate entirely is still a run — present in `text` with an EMPTY
/// `rendered`, from which `truncated` derives TRUE through the existing comparison — and total loss stops
/// being the single cut a client cannot see. This predicate is the whole of that change: it does not know
/// the wholly-clipped case as a case, it takes a run's PLACEMENT from the layout (below) and leaves
/// visibility to `rendered`. The old `None` swallowed a run the clip ate, which is not what its own
/// justification covered.
///
/// **Placement comes from the galley's rows, which survive having no visible glyph.** Two candidates are
/// read off the same source: the first row that put a glyph on the glass, and — for a run that put none
/// anywhere — the first row the toolkit laid glyphs out on. Both give the band the same way, so an
/// invisible run is placed by the same rule as a visible one rather than by an approximation of it.
/// `left` is the row's first LAID-OUT glyph (`PlacedRow::rect_without_leading_space`), which is the
/// layout-side reading of the same edge the first visible glyph used to give.
///
/// The other half of the amended clause — *on a reported row* — is [`reported_rows`]'s, because whether a
/// row is reported is a fact about the row and not about any one run. That is where the scroll rule lives.
///
/// `rendered` walks the glyph rows the toolkit laid out. A `\n` in the source ends a row and is not a
/// glyph (`PlacedRow::ends_with_newline`), so its folded space is put back between the visible glyphs on
/// either side of it — only when the row after it meets the clip, since a line cut off below the view is
/// not on the glass. A row the toolkit WRAPPED inserts nothing: wrapping is not truncation, and a wrapped
/// label that loses nothing has `rendered == text`.
pub fn glass_run(p: &Painted) -> Option<GlassRun> {
    let mut rendered = String::new();
    let mut on_glass: Option<(f32, f32, f32)> = None;
    let mut laid_out: Option<(f32, f32, f32)> = None;
    let mut newline_pending = false;
    let origin = p.pos.to_vec2();
    for row in &p.galley.rows {
        let row_rect = row.rect().translate(origin);
        let band = (
            row_rect.min.y,
            row_rect.max.y,
            row.rect_without_leading_space().translate(origin).min.x,
        );
        if laid_out.is_none() && !row.glyphs.is_empty() {
            laid_out = Some(band);
        }
        if newline_pending && !rendered.is_empty() && meets(row_rect, p.clip) {
            rendered.push(' ');
        }
        newline_pending = false;
        for g in &row.glyphs {
            let r = g.logical_rect().translate(origin + row.pos.to_vec2());
            if meets(r, p.clip) {
                rendered.push(fold(g.chr));
                if on_glass.is_none() {
                    on_glass = Some(band);
                }
            }
        }
        if row.ends_with_newline {
            newline_pending = true;
        }
    }
    let (top, bottom, left) = on_glass.or(laid_out)?;
    Some(GlassRun {
        text: p.galley.text().chars().map(fold).collect(),
        rendered,
        top,
        bottom,
        left,
        elided: p.galley.elided,
    })
}

/// **The visual rows this surface REPORTS**, top to bottom, each ordered left to right — the grouping both
/// strings are then written from, and the other half of §11.50's clause *"laid out on a reported row"*.
///
/// A run belongs to the current row when the vertical centre of its own band lies inside the band of the
/// row's topmost run — so a small label centred beside a large one is on its row, and a table row whose top
/// touches the previous row's bottom is not.
///
/// **A row is reported when something on it reached the glass**, i.e. when one of its runs rendered a
/// glyph. That is a fact about the row, which is why it is decided here and not in [`glass_run`]: it is
/// what makes the empty run of a wholly-clipped cell land *beside the cells that were shown* rather than
/// anywhere else, and it is the same sentence that keeps the scroll rule (`F-PANEL-SCROLL-UNSTATED`) exactly
/// where it was — a row nothing on the panel showed is in neither string, so rows off the view are still
/// not truncation. Nothing that used to be reported can be dropped by it: a run only ever had a band when
/// a glyph of it was on the glass, so every row that existed before this rule had one.
///
/// `T` is whatever the caller needs carried alongside each run ([`panel_surface`] carries the shape's index,
/// so `unrenderable` can be taken from the runs the surface reports and no others).
pub fn reported_rows<T>(mut runs: Vec<(GlassRun, T)>) -> Vec<Vec<(GlassRun, T)>> {
    runs.sort_by(|a, b| {
        a.0.top
            .total_cmp(&b.0.top)
            .then(a.0.left.total_cmp(&b.0.left))
    });
    let mut rows: Vec<Vec<(GlassRun, T)>> = Vec::new();
    for run in runs {
        let centre = (run.0.top + run.0.bottom) / 2.0;
        match rows.last_mut() {
            Some(row) if centre >= row[0].0.top && centre < row[0].0.bottom => row.push(run),
            _ => rows.push(vec![run]),
        }
    }
    rows.retain(|row| row.iter().any(|(run, _)| !run.rendered.is_empty()));
    for row in &mut rows {
        row.sort_by(|a, b| a.0.left.total_cmp(&b.0.left));
    }
    rows
}

/// **Write the reported rows into the two strings**, identically: rows top to bottom joined by LF, runs
/// left to right within a row joined by TAB.
///
/// The joins are this function's, not text on the glass, and the same joins go into both strings from the
/// same rows in the same order — which is the whole of the alignment guarantee, and why an empty
/// `rendered` cannot slide row *k* run *j* of one string off row *k* run *j* of the other.
pub fn write_rows<T>(rows: &[Vec<(GlassRun, T)>]) -> (String, String) {
    let (mut text, mut rendered) = (String::new(), String::new());
    for (k, row) in rows.iter().enumerate() {
        if k > 0 {
            text.push('\n');
            rendered.push('\n');
        }
        for (j, (run, _)) in row.iter().enumerate() {
            if j > 0 {
                text.push('\t');
                rendered.push('\t');
            }
            text.push_str(&run.text);
            rendered.push_str(&run.rendered);
        }
    }
    (text, rendered)
}

/// [`reported_rows`] then [`write_rows`], for a caller that carries nothing alongside its runs.
///
/// **Test-only, and the reason is worth stating rather than hiding behind an `allow`:** the serve goes
/// through the two functions directly because it carries each shape's index alongside its run, so
/// `unrenderable` can be read from the runs the ROWS kept (see [`panel_surface`]). Before that it called
/// this, and clippy's `dead_code` named the change the moment it stopped. This spelling stays because the
/// unit rows below read the two strings and nothing else.
#[cfg(test)]
pub fn join(runs: Vec<GlassRun>) -> (String, String) {
    write_rows(&reported_rows(
        runs.into_iter().map(|r| (r, ())).collect::<Vec<_>>(),
    ))
}

/// **The characters of a galley's source this window draws as a hollow box**, each asked of the family its
/// section was laid out in, through the same three-state `probe` [`snapshot`] takes (only `Some(false)`
/// counts). Over the SOURCE, so a box in an elided tail is still named — `oracle-frontend`'s choice
/// (CR-W §6.2). A family other than proportional or monospace cannot be asked and is skipped, which is the
/// `None` arm: nothing invented.
fn boxes(
    galley: &egui::Galley,
    probe: &mut dyn FnMut(char, bool) -> Option<bool>,
    out: &mut Vec<String>,
) {
    let job = &galley.job;
    for section in &job.sections {
        let mono = match section.format.font_id.family {
            egui::FontFamily::Proportional => false,
            egui::FontFamily::Monospace => true,
            egui::FontFamily::Name(_) => continue,
        };
        let Some(slice) = job
            .text
            .get(section.byte_range.start.0..section.byte_range.end.0)
        else {
            continue;
        };
        for c in slice.chars() {
            if probe(c, mono) == Some(false) {
                let s = c.to_string();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
}

/// **One `panel` surface per drawn body, in the order the window drew them** (§11.50). Called after
/// `build_ui` and before the pass ends; see the module doc for why each step is where it is.
pub fn panels(
    ctx: &egui::Context,
    spans: &[PanelSpan],
    probe: &mut dyn FnMut(char, bool) -> Option<bool>,
) -> Vec<ScreenSurface> {
    let shapes = painted(ctx, spans);
    spans
        .iter()
        .zip(shapes)
        .map(|(span, painted)| panel_surface(span.name, &painted, probe))
        .collect()
}

/// [`panels`] for one span's shapes: the testable half, over shapes rather than a live paint list.
pub fn panel_surface(
    name: &'static str,
    painted: &[Painted],
    probe: &mut dyn FnMut(char, bool) -> Option<bool>,
) -> ScreenSurface {
    let mut runs = Vec::new();
    for (i, p) in painted.iter().enumerate() {
        if let Some(run) = glass_run(p) {
            runs.push((run, i));
        }
    }
    let rows = reported_rows(runs);
    let (text, rendered) = write_rows(&rows);
    // `unrenderable` names the boxes in the SOURCE of the runs this surface reports — taken from the rows
    // rather than from `glass_run`, so a run on a row the surface does not report (a label scrolled out of
    // view) stays out of this field exactly as it stays out of both strings. Read back in paint order, the
    // order it was collected in before the rows existed.
    let mut reported: Vec<usize> = rows.iter().flatten().map(|(_, i)| *i).collect();
    reported.sort_unstable();
    let mut unrenderable = Vec::new();
    for i in reported {
        boxes(&painted[i].galley, probe, &mut unrenderable);
    }
    ScreenSurface {
        kind: ScreenSurfaceKind::Panel(
            PanelName::new(name).expect("every tab bar draws a non-empty title (Tab::title)"),
        ),
        text,
        rendered,
        unrenderable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe that can draw anything — the shape of a broken measurement, used deliberately below so the
    /// control that catches it is exercised.
    fn all_drawable(_: char, _: bool) -> Option<bool> {
        Some(true)
    }

    /// The probe is asked *"can this glyph be drawn"*, but every `only_X_lacks_Y` probe below is
    /// **named for what it LACKS** — and the name is the whole point of those rows. This flips the
    /// sense once, here, so each probe body can state its rule the way its name reads
    /// (`mono && c == 'b'`) instead of wrapping it in a `!` the reader has to undo.
    fn drawable_unless(lacks: bool) -> Option<bool> {
        Some(!lacks)
    }

    /// **The join groups the way the bar groups, and the separator is not silently dropped.**
    ///
    /// The third assertion is the `assert_ne!`: without it this row would pass against a `snapshot` that
    /// concatenated the runs with nothing between them *and* against one that returned the first run
    /// unchanged, because both still "contain" every piece.
    #[test]
    fn runs_join_with_the_rule_the_bar_drew_and_the_gap_it_did_not() {
        let runs = vec![
            Run::label("oracle-player"),
            Run::after_sep("PAUSE"),
            Run::label("STEP"),
            Run::mono_after_sep("status"),
        ];
        let v = snapshot("title", &runs, &mut all_drawable);
        assert_eq!(v.len(), 2, "a title bar and a status line, always");
        assert_eq!(
            v[1].text,
            format!("oracle-player{SEP}PAUSE{GAP}STEP{SEP}status"),
            "separated runs take the rule, adjacent ones take the gap"
        );
        assert_ne!(
            v[1].text, "oracle-player",
            "the agreement above is two copies of the same untouched value: the line is the whole bar, \
             not its first run"
        );
        assert_ne!(
            v[1].text,
            runs.iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join(""),
            "the drawn separators vanished from the readback"
        );
    }

    /// **`unrenderable` is asked of the family that DREW the run**, not of one family for the whole line.
    ///
    /// The control comes first: a probe that answers `true` for everything reports no defect, so a
    /// `snapshot` that never called the probe at all would pass the positive half below just as green.
    #[test]
    fn a_missing_glyph_is_attributed_to_the_font_that_would_have_drawn_it() {
        let runs = vec![Run::label("ab"), Run::mono_after_sep("ab")];
        assert!(
            snapshot("t", &runs, &mut all_drawable)[1]
                .unrenderable
                .is_empty(),
            "control: nothing is missing when every glyph exists"
        );

        // Only the MONOSPACE family lacks `b`. A probe asked with the wrong `mono` flag — or asked once
        // for the whole line — cannot produce this answer.
        let mut only_mono_lacks_b = |c: char, mono: bool| drawable_unless(mono && c == 'b');
        let v = snapshot("t", &runs, &mut only_mono_lacks_b);
        assert_eq!(
            v[1].unrenderable,
            vec!["b".to_string()],
            "the proportional run's `b` draws; the monospace run's does not"
        );

        // …and the reverse, so the flag is not simply being ignored in one direction.
        let mut only_prop_lacks_a = |c: char, mono: bool| drawable_unless(!mono && c == 'a');
        assert_eq!(
            snapshot("t", &runs, &mut only_prop_lacks_a)[1].unrenderable,
            vec!["a".to_string()]
        );
    }

    /// ⚑ **An UNMEASURABLE family invents nothing.** `None` is not `Some(false)`, and the distance between
    /// them is 25 fabricated defects on a window that draws its whole status line correctly — see
    /// [`Glyphs`] for the measurement that made this arm necessary rather than defensive.
    ///
    /// The control is the same runs under a probe that *can* measure and says the same characters are
    /// missing: without it, a `snapshot` that had simply stopped calling the probe would pass the
    /// `is_empty()` below.
    #[test]
    fn a_family_that_cannot_be_measured_reports_no_boxes_rather_than_all_of_them() {
        let runs = vec![Run::label("ab"), Run::mono_after_sep("cd")];

        let mut mono_unmeasurable = |_: char, mono: bool| (!mono).then_some(false);
        let v = snapshot("t", &runs, &mut mono_unmeasurable);
        assert_eq!(
            v[1].unrenderable,
            vec!["a".to_string(), "b".to_string()],
            "the measurable family still reports its boxes"
        );
        assert!(
            !v[1].unrenderable.contains(&"c".to_string()),
            "an unmeasurable family must not be reported as a window full of hollow boxes"
        );

        // The control: measurable, and answering `false` for exactly the same characters.
        let mut both_measurable = |_: char, _: bool| Some(false);
        assert_eq!(
            snapshot("t", &runs, &mut both_measurable)[1].unrenderable,
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ],
            "control: with the same runs and a probe that CAN measure, `c` and `d` are reported — so \
             their absence above is the `None` arm and not a probe that stopped being called"
        );
    }

    /// Repeats are named once each, in first-appearance order — a bar with four bad dots has one defect.
    #[test]
    fn repeated_missing_glyphs_are_reported_once_each_in_order() {
        let runs = vec![Run::label("x?y!x?"), Run::after_sep("!?")];
        let mut lacks_punct = |c: char, _: bool| Some(!matches!(c, '?' | '!'));
        assert_eq!(
            snapshot("t", &runs, &mut lacks_punct)[1].unrenderable,
            vec!["?".to_string(), "!".to_string()]
        );
    }

    /// **The title bar is not measured against a font that never draws it**, and its `rendered` claims no
    /// truncation this process cannot observe.
    #[test]
    fn the_title_bar_carries_the_desktops_string_and_no_claim_about_our_fonts() {
        let mut nothing_draws = |_: char, _: bool| Some(false);
        let v = snapshot("oracle-player", &[Run::label("x")], &mut nothing_draws);
        assert_eq!(v[0].kind, ScreenSurfaceKind::TitleBar);
        assert_eq!(v[0].text, "oracle-player");
        assert_eq!(v[0].rendered, v[0].text, "no elision we can see");
        assert!(
            v[0].unrenderable.is_empty(),
            "the desktop draws this, not egui: {:?}",
            v[0].unrenderable
        );
        assert_eq!(
            v[1].unrenderable,
            vec!["x".to_string()],
            "…while the run the player DID draw is measured, so the emptiness above is a decision \
             rather than a probe that is never called"
        );
    }

    // -----------------------------------------------------------------------------------------------
    // Panels (§11.50, CR-W): the reading rule over hand-laid galleys. The live gates, against the real
    // bodies and the real dock, are `crate::panel_attribution`'s.
    // -----------------------------------------------------------------------------------------------

    /// A context that has run one frame, so its fonts exist.
    fn fonts_ctx() -> egui::Context {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label("x");
        });
        out.textures_delta.clear();
        ctx
    }

    /// One galley painted at `pos` under `clip`, laid out from `job`.
    fn painted_job(
        ctx: &egui::Context,
        job: egui::text::LayoutJob,
        pos: egui::Pos2,
        clip: egui::Rect,
    ) -> Painted {
        Painted {
            galley: ctx.fonts_mut(|f| f.layout_job(job)),
            pos,
            clip,
        }
    }

    fn simple(text: &str, size: f32, wrap: f32) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::simple(
            text.to_owned(),
            egui::FontId::proportional(size),
            egui::Color32::WHITE,
            wrap,
        );
        job.wrap.max_width = wrap;
        job
    }

    const WIDE: egui::Rect = egui::Rect {
        min: egui::pos2(-1000.0, -1000.0),
        max: egui::pos2(1000.0, 1000.0),
    };

    /// **A source LF and TAB are folded to a space in BOTH strings**, and a LF — which the toolkit lays
    /// out as a row break, not a glyph — is put back in `rendered` exactly where `text` has it, trailing
    /// LF included. Without that, every multi-line label would read as truncated.
    #[test]
    fn a_runs_own_line_breaks_and_tabs_are_folded_identically_in_both_strings() {
        let ctx = fonts_ctx();
        for src in ["one\ntwo", "a\tb", "trailing\n", "two\n\nbreaks"] {
            let p = painted_job(
                &ctx,
                simple(src, 14.0, f32::INFINITY),
                egui::Pos2::ZERO,
                WIDE,
            );
            let run = glass_run(&p).expect("on the glass");
            let folded: String = src.replace(['\n', '\t'], " ");
            assert_eq!(run.text, folded, "{src:?}");
            assert_eq!(
                run.rendered, folded,
                "{src:?}: a whole run renders its whole source"
            );
        }
    }

    /// **A run cut by its clip is whole in `text` and cut in `rendered`; a run the clip ate entirely is
    /// whole in `text` with an EMPTY `rendered`, placed by its layout; an empty SHAPE is not a run.**
    /// Control: the same galley under a wide clip renders whole.
    ///
    /// The third case used to be `None` — the defect the §11.50 amendment names
    /// (`F-PANEL-CLIP-TOTAL-LOSS-UNSIGNALLED`): a run the clip ate was in neither string, both compared
    /// equal, and `truncated` derived false for the cut that loses the most. What stays `None` is a galley
    /// the toolkit laid no glyph out in, which is the case the old return's own justification covered.
    /// Whether such a run reaches the strings is [`reported_rows`]'s question, asserted below it.
    #[test]
    fn a_clip_decides_which_glyphs_are_on_the_glass() {
        let ctx = fonts_ctx();
        let job = || simple("left right", 14.0, f32::INFINITY);
        let whole = glass_run(&painted_job(&ctx, job(), egui::Pos2::ZERO, WIDE)).unwrap();
        assert_eq!(whole.rendered, "left right", "control");
        let right_edge = ctx
            .fonts_mut(|f| f.layout_job(simple("left", 14.0, f32::INFINITY)))
            .rect
            .width();
        let cut = egui::Rect::from_min_max(
            egui::pos2(-10.0, -10.0),
            egui::pos2(right_edge - 0.5, 100.0),
        );
        let run = glass_run(&painted_job(&ctx, job(), egui::Pos2::ZERO, cut)).unwrap();
        assert_eq!(run.text, "left right");
        assert_eq!(run.rendered, "left");
        let below = egui::Rect::from_min_max(egui::pos2(-10.0, 500.0), egui::pos2(500.0, 600.0));
        let eaten = glass_run(&painted_job(&ctx, job(), egui::Pos2::ZERO, below))
            .expect("the toolkit laid this run out, so it is a run whatever the clip kept");
        assert_eq!(eaten.text, "left right", "the source is whole");
        assert_eq!(eaten.rendered, "", "and nothing of it reached the glass");
        assert_eq!(
            (eaten.top, eaten.left),
            (whole.top, whole.left),
            "placed by the same layout as the visible reading of the same galley, not by a fallback of \
             its own"
        );
        assert_eq!(
            join(vec![eaten.clone()]),
            (String::new(), String::new()),
            "alone it is on a row the surface does not report, which is the scroll rule untouched"
        );
        let shown = GlassRun {
            left: whole.left - 50.0,
            ..whole.clone()
        };
        assert_eq!(
            join(vec![eaten, shown]),
            (
                "left right\tleft right".to_owned(),
                "left right\t".to_owned()
            ),
            "beside a run that WAS shown, it is on a reported row: run 1 of row 0 is the same run in both \
             strings, and rendered's is empty"
        );

        let empty = simple("", 14.0, f32::INFINITY);
        assert_eq!(
            glass_run(&painted_job(&ctx, empty, egui::Pos2::ZERO, WIDE)),
            None,
            "an empty text SHAPE is not a run (§11.50)"
        );
    }

    /// **The joins.** A small label centred beside a large one is on its row; the next table row, whose
    /// top touches this row's bottom, is not; runs are left to right within a row whatever order they were
    /// painted in; and both strings carry the same joins.
    #[test]
    fn runs_join_into_visual_rows_left_to_right_identically_in_both_strings() {
        let run = |t: &str, top: f32, bottom: f32, left: f32| GlassRun {
            text: t.into(),
            rendered: format!("{t}!"),
            top,
            bottom,
            left,
            elided: false,
        };
        let (text, rendered) = join(vec![
            run("small", 4.0, 14.0, 200.0),
            run("next", 18.0, 36.0, 0.0),
            run("big", 0.0, 18.0, 0.0),
            run("right", 18.0, 36.0, 90.0),
        ]);
        assert_eq!(text, "big\tsmall\nnext\tright");
        assert_eq!(rendered, "big!\tsmall!\nnext!\tright!");
        assert_eq!(
            join(Vec::new()),
            (String::new(), String::new()),
            "a blank panel is two empty strings"
        );
    }

    /// **`unrenderable` is asked of the family each SECTION was laid out in**, over the source.
    /// Control: an all-drawable probe names nothing.
    #[test]
    fn a_panel_box_is_attributed_to_the_family_of_its_section() {
        let ctx = fonts_ctx();
        let mut job = egui::text::LayoutJob::default();
        job.append(
            "ab",
            0.0,
            egui::TextFormat::simple(egui::FontId::proportional(12.0), egui::Color32::WHITE),
        );
        job.append(
            "ab",
            0.0,
            egui::TextFormat::simple(egui::FontId::monospace(12.0), egui::Color32::WHITE),
        );
        let p = painted_job(&ctx, job, egui::Pos2::ZERO, WIDE);
        let s = panel_surface("Registers", std::slice::from_ref(&p), &mut all_drawable);
        assert!(s.unrenderable.is_empty(), "control");
        assert_eq!(s.kind.panel(), Some("Registers"));
        let mut only_mono_lacks_b = |c: char, mono: bool| drawable_unless(mono && c == 'b');
        let s = panel_surface(
            "Registers",
            std::slice::from_ref(&p),
            &mut only_mono_lacks_b,
        );
        assert_eq!(s.unrenderable, vec!["b".to_string()]);
        let mut only_prop_lacks_a = |c: char, mono: bool| drawable_unless(!mono && c == 'a');
        let s = panel_surface("Registers", &[p], &mut only_prop_lacks_a);
        assert_eq!(s.unrenderable, vec!["a".to_string()]);
    }

    /// ★ **The instrument, against the live toolkit** — and the upstream defect it exists instead of.
    ///
    /// This is the only row here that measures egui rather than this module's arithmetic, and it asserts
    /// three things in the order that makes each one mean something:
    ///
    /// 1. **The positive control.** `A` is drawn in BOTH families. Without it every claim below would hold
    ///    just as well against a toolkit with no fonts loaded at all.
    /// 2. **The instrument finds a real box.** A Han ideograph is a hollow box in both families — which is
    ///    also the check that [`GLYPH_REFERENCES`] is still a reference and not a drawable character.
    /// 3. ⚑ **`Fonts::has_glyph` disagrees, and is wrong.** It calls `A` undrawable in monospace and `▶`
    ///    undrawable in proportional, on a build that draws both. That is the measurement behind
    ///    [`Glyphs`]'s doc, pinned here so it is a fact this repo re-checks rather than a claim in a
    ///    comment. **When this half goes red, epaint has been fixed** — a good day; re-measure and simplify,
    ///    do not delete.
    #[test]
    fn the_glyph_probe_reads_the_atlas_because_the_toolkits_own_predicate_is_wrong() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        // A real frame, so the fonts exist: `Context::fonts*` panics before the first `run`.
        let mut out = ctx.run_ui(raw, |ui| {
            ui.monospace("GOVERNOR");
            ui.strong("oracle-player");
        });
        out.textures_delta.clear();

        let mut g = Glyphs::new(&ctx);
        for mono in [false, true] {
            assert_eq!(
                g.drawable('A', mono),
                Some(true),
                "positive control (mono={mono}): this build cannot draw the letter A, so nothing below \
                 is a measurement"
            );
            assert_eq!(
                g.drawable(GLYPH_REFERENCES[0], mono),
                Some(false),
                "(mono={mono}) the reference character is no longer a hollow box — the font set has \
                 gained it, and GLYPH_REFERENCES needs re-choosing before this probe means anything"
            );
        }
        // Every character the player's own bar can contain draws whole, in the family that draws it.
        //
        // ⚑ The last two were MISSING from this list until `F-AETHER-BIND-FAILURE-SILENT`, and the
        // omission is worth naming: this row claims to cover "every character the player's own bar can
        // contain", and `⏹`/`⚠` are `crate::stopping::Halting::headline`'s — drawn on that bar since
        // `ARMED-STATE-VISIBLE`, never probed. `⚠` is now also the bind-failure alarm's. A hollow box on
        // an alarm is the specific way an alarm fails quietly, so the alarm characters are the ones this
        // list least afforded to be missing.
        for (c, mono) in [
            ('\u{25B6}', false),
            ('\u{23F8}', false),
            ('\u{23ED}', false),
            ('\u{00B7}', true),
            ('|', false),
            ('\u{23F9}', false),
            ('\u{26A0}', false),
        ] {
            assert_eq!(
                g.drawable(c, mono),
                Some(true),
                "the player draws U+{:04X} on its top bar and this build shows a hollow box there",
                c as u32
            );
        }

        // …and the predicate this module refuses to use, pinned wrong on both counts.
        let ask = |mono: bool, c: char| {
            let family = if mono {
                egui::FontFamily::Monospace
            } else {
                egui::FontFamily::Proportional
            };
            ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::new(12.0, family), c))
        };
        assert!(
            !ask(true, 'A'),
            "epaint's `has_glyph` now admits the monospace family draws `A`: the defect `Glyphs` was \
             built around is FIXED. Re-measure and simplify; do not delete this row."
        );
        assert!(
            !ask(false, '\u{25B6}'),
            "epaint's `has_glyph` now admits the proportional family draws `▶`: the second half of the \
             same defect is FIXED. Re-measure; do not delete."
        );
        assert!(
            ask(false, 'A'),
            "control on the WRONG predicate: it does not answer `false` for everything, which is what \
             makes the two assertions above a disagreement rather than a dead API"
        );
    }

    /// ★ **Every character this window can put on the glass draws whole — no hollow boxes.**
    ///
    /// The row above proves the *instrument*; this one turns it on the crate. It is the player's answer to
    /// `oracle-frontend`'s `every_string_literal_the_frontend_can_show_is_drawable`, and it exists because
    /// the hand-written list in the row above is a **list**: it covers the top bar, and the top bar is not
    /// where the defect was.
    ///
    /// ⚑ **The defect this row was written for.** The Breakpoints and Watchpoints tabs drew their delete
    /// button as `\u{2715}`, a sensible delete icon that **no face in egui's bundled set carries**, so the
    /// whole label rendered as the replacement box — and in Breakpoints it landed 36 px right of a real
    /// tick-box, where an unticked box is exactly what it looks like. A UX seat destroyed a breakpoint with
    /// it while trying to re-enable one, before it knew the control existed. A list-shaped guard could not
    /// have caught that, because nobody would have thought to put a delete button's glyph on the list.
    ///
    /// ⚑ **And it immediately found a second one nobody had reported**: `rom_open.rs`'s `"\u{2191} up"`.
    /// `\u{2191}` is drawable in the **monospace** family and NOT in the proportional one, and that button
    /// is proportional — which is the whole reason this row asks about **both** families rather than
    /// guessing which one a literal will be drawn in. A literal in this crate reaches the glass through
    /// `ui.label`, `ui.button`, `ui.monospace`, a hover tooltip, or as a `format!` argument to any of them,
    /// and nothing at the literal says which. Asking both is the only rule that is a fact about the string
    /// rather than a guess about its call site.
    ///
    /// **Derived, not listed.** The modules come from `main.rs`'s own `mod` declarations, so a file that is
    /// not compiled in cannot smuggle a literal into the measurement or hide one from it; the literals come
    /// from a lexer over each module's production region (everything before its first `#[cfg(test)]`), so a
    /// label added next week is measured without anyone remembering to add it here. Format placeholders are
    /// stripped, because what reaches the glass is the substituted value.
    ///
    /// **Red-first, on the pristine tree** — no mutation was needed, because the defect was live:
    ///
    /// ```text
    /// 5 undrawable literal(s), of 1792 lexed across 27 modules:
    ///   ui.rs: "\u{2715}" -> U+2715 is a hollow box in the proportional family
    ///   ui.rs: "\u{2715}" -> U+2715 is a hollow box in the monospace family
    ///   ui.rs: "\u{2715}" -> U+2715 is a hollow box in the proportional family
    ///   ui.rs: "\u{2715}" -> U+2715 is a hollow box in the monospace family
    ///   rom_open.rs: "\u{2191} up" -> U+2191 is a hollow box in the proportional family
    /// ```
    ///
    /// Five rows for three sites, because each character is asked of both families and `\u{2715}` fails in
    /// both. The obvious repair for the third one was also a box: `\u{25B2}` is undrawable in the
    /// proportional family on a build where `\u{25B6}` draws, which is how this row earned its second
    /// finding — the substitute was measured rather than assumed.
    #[test]
    fn every_string_literal_the_player_can_show_is_drawable() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        // The module tree from the crate's own `mod` declarations, the way rustc resolves them: `mod x;`
        // is `x.rs` or `x/mod.rs`, and the walk follows declarations into subdirectories. An inline
        // `mod x { .. }` has no `;` and is skipped here, because its body is already inside the file.
        let mut modules: Vec<std::path::PathBuf> = Vec::new();
        let mut pending: Vec<(std::path::PathBuf, std::path::PathBuf)> =
            vec![(root.join("main.rs"), root.clone())];
        while let Some((file, dir)) = pending.pop() {
            let src = std::fs::read_to_string(&file).unwrap_or_else(|e| {
                panic!(
                    "cannot read {}: {e} (a `mod` with a #[path]?)",
                    file.display()
                )
            });
            for line in src.lines() {
                let Some(rest) = line.trim().strip_prefix("mod ") else {
                    continue;
                };
                let Some(name) = rest.strip_suffix(';') else {
                    continue;
                };
                let name = name.trim();
                let flat = dir.join(format!("{name}.rs"));
                let nested = dir.join(name).join("mod.rs");
                if flat.is_file() {
                    pending.push((flat, dir.clone()));
                } else if nested.is_file() {
                    pending.push((nested, dir.join(name)));
                } else {
                    panic!(
                        "`mod {name};` in {} resolves to neither {} nor {}",
                        file.display(),
                        flat.display(),
                        nested.display()
                    );
                }
            }
            modules.push(file);
        }
        assert!(
            modules.len() > 10,
            "COULD NOT MEASURE: only {} modules found, so the `mod` scan is broken and not the font",
            modules.len()
        );

        // The lexer, proven on planted samples before it is trusted on the crate. Both halves earn their
        // lines: the first is the literal this row exists for, the second is a line whose lifetime, char
        // literal and trailing comment are what break a scanner that treats every quote as a boundary.
        assert_eq!(
            string_literals("if ui.small_button(\"\u{2715}\").clicked() {"),
            vec!["\u{2715}".to_string()],
            "the lexer does not recover a plain literal"
        );
        assert_eq!(
            string_literals("fn f(s: &'static str) { let q = '\"'; g(\"kept\"); } // \"dropped\""),
            vec!["kept".to_string()],
            "the lexer is confused by a lifetime, a quote char literal, or a line comment"
        );

        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        // A real frame, so the fonts exist: `Context::fonts*` panics before the first `run`.
        let mut out = ctx.run_ui(raw, |ui| {
            ui.monospace("GOVERNOR");
            ui.strong("oracle-player");
        });
        out.textures_delta.clear();
        let mut g = Glyphs::new(&ctx);

        // ⚑ **The instrument's own controls, run here rather than borrowed from the row above.** A context
        // that draws nothing would report every character below as a box, and a context whose references
        // have become drawable would report every character as fine. Both are failures of the measurement
        // that look like results, in opposite directions.
        for mono in [false, true] {
            assert_eq!(
                g.drawable('A', mono),
                Some(true),
                "positive control (mono={mono}): this build cannot draw the letter A, so nothing below is \
                 a measurement"
            );
            assert_eq!(
                g.drawable(GLYPH_REFERENCES[0], mono),
                Some(false),
                "negative control (mono={mono}): the reference character is no longer a hollow box, so \
                 this row can no longer tell a box from a glyph"
            );
        }

        let mut checked = 0usize;
        let mut defects: Vec<String> = Vec::new();
        for path in &modules {
            let file = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            let src = std::fs::read_to_string(path).expect("read a module the walk already opened");
            // Production only: everything up to the module's first `#[cfg(test)]`. An assertion message is
            // read by a developer in a terminal, not by a person in the window.
            let prod = src.split("#[cfg(test)]").next().unwrap_or("");
            for lit in string_literals(prod) {
                checked += 1;
                for c in lit.chars() {
                    for mono in [false, true] {
                        let family = if mono { "monospace" } else { "proportional" };
                        match g.drawable(c, mono) {
                            Some(true) => {}
                            Some(false) => defects.push(format!(
                                "  {file}: {lit:?} -> U+{:04X} is a hollow box in the {family} family",
                                c as u32
                            )),
                            // Loud on unmeasurable rather than silent: a family this build cannot measure
                            // is not a family this build has been shown to draw.
                            None => defects.push(format!(
                                "  {file}: {lit:?} -> U+{:04X} is UNMEASURABLE in the {family} family",
                                c as u32
                            )),
                        }
                    }
                }
            }
        }
        assert!(
            checked > 400,
            "COULD NOT MEASURE: only {checked} literals lexed across {} modules, so the lexer is broken \
             and not the font",
            modules.len()
        );
        assert!(
            defects.is_empty(),
            "{} undrawable literal(s), of {checked} lexed across {} modules:\n{}",
            defects.len(),
            modules.len(),
            defects.join("\n")
        );
    }

    /// The string literals in a chunk of Rust source, unescaped as the compiler would (`\"`, `\\`,
    /// `\u{..}`, and the backslash-newline continuation), with `{...}` format placeholders removed and
    /// `\n`/`\t` dropped as line structure rather than glyphs. Line comments are skipped; char literals
    /// are stepped over so their quotes cannot open a string.
    ///
    /// Deliberately small: this is a test aid over one crate's own style, not a Rust lexer, and its caller
    /// asserts both a planted sample and a floor on what it finds, so a silent miss cannot pass as clean.
    /// It is a sibling of `oracle-frontend`'s function of the same name, and the duplication is deliberate:
    /// that crate measures its own 5x7 bitmap table and this one measures a live `egui::Context`, and
    /// neither crate exposes a library the other could reach.
    fn string_literals(src: &str) -> Vec<String> {
        let chars: Vec<char> = src.chars().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '/' && chars.get(i + 1) == Some(&'/') {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            } else if c == '\'' {
                // A char literal, or a lifetime. `'\..'` and `'x'` are chars; anything else is a lifetime
                // and only the quote itself is consumed.
                if chars.get(i + 1) == Some(&'\\') {
                    i += 2;
                    while i < chars.len() && chars[i] != '\'' {
                        i += 1;
                    }
                    i += 1;
                } else if chars.get(i + 2) == Some(&'\'') {
                    i += 3;
                } else {
                    i += 1;
                }
            } else if c == '"' {
                i += 1;
                let mut lit = String::new();
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                        match chars.get(i) {
                            Some('n') | Some('t') | Some('r') | Some('0') => i += 1,
                            Some('\n') => {
                                // Continuation: the newline and the next line's indent both vanish.
                                i += 1;
                                while i < chars.len() && chars[i].is_whitespace() {
                                    i += 1;
                                }
                            }
                            Some('u') => {
                                let start = i + 2;
                                let mut end = start;
                                while end < chars.len() && chars[end] != '}' {
                                    end += 1;
                                }
                                let hex: String = chars[start..end].iter().collect();
                                let cp =
                                    u32::from_str_radix(&hex, 16).expect("\\u{..} escape is hex");
                                lit.push(char::from_u32(cp).expect("\\u{..} escape is a scalar"));
                                i = end + 1;
                            }
                            Some(&e) => {
                                lit.push(e);
                                i += 1;
                            }
                            None => {}
                        }
                    } else if chars[i] == '{' {
                        // A format placeholder: what reaches the glass is the substituted value, not this.
                        // `{{` is a literal brace and is kept.
                        if chars.get(i + 1) == Some(&'{') {
                            lit.push('{');
                            i += 2;
                        } else {
                            while i < chars.len() && chars[i] != '}' && chars[i] != '"' {
                                i += 1;
                            }
                            if chars.get(i) == Some(&'}') {
                                i += 1;
                            }
                        }
                    } else {
                        lit.push(chars[i]);
                        i += 1;
                    }
                }
                i += 1;
                if !lit.is_empty() {
                    out.push(lit);
                }
            } else {
                i += 1;
            }
        }
        out
    }
}
