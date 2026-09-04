//! **What the player's window says, as a snapshot a client can read** — the player half of
//! `emulator/screen_text` (contract §11.29, CR-H), booked as unwired by design §5.8.2 and closed here.
//!
//! # What is on the glass, and what is not
//!
//! `oracle-frontend` composes its snapshot from a title, an overlay and a status line, and this module
//! does the same job for a window whose chrome is a different shape: a top bar and **eight dockable
//! panels** that are mostly text. The tempting answer is *all eight panels' contents*. It is the wrong
//! answer, and not marginally:
//!
//! * ⚑ **`egui_dock` draws only the ACTIVE tab of a leaf.** [`crate::ui::initial_dock`] puts
//!   Registers/Memory/Objects in one pane and Breakpoints/Watchpoints/Profiler in another, so ~~six~~
//!   **four** of the eight panel bodies do not run on a given frame — that fact is load-bearing enough
//!   that this crate grew `--dock every-tab` ([`crate::ui::every_tab_dock`]) to make a cost measurement
//!   mean anything. *(Corrected by `PANELS-NAV`, and it had been copied out of here twice before anyone
//!   counted: `egui_dock` draws one body per **leaf**, and the default layout has four leaves. The six
//!   counts every tab that shares a pane, but two of those — `Registers` and `Breakpoints` — are their
//!   own leaf's active tab and do run. Measured in
//!   `nav::tests::the_default_layout_hides_one_body_per_shared_pane_and_the_count_is_measured`, which
//!   derives it from the leaf count rather than restating a figure. The argument below is unchanged: a
//!   snapshot of all eight would still report text nobody can see.)* A
//!   snapshot listing all eight would report text **nobody can see**, which is the exact class of wrong
//!   answer `screen_text` exists to avoid: a caller reading it would be told the window says something it
//!   does not say.
//! * **The active tab is no better, only less obviously wrong.** What a panel body actually reveals
//!   depends on the pane's pixel height and its scroll offset, and both are computed *inside* egui's
//!   painting loop. Restating them here is precisely the drift `oracle-frontend`'s rule 2 forbids: a
//!   restated copy agrees with itself while diverging from the drawing code. `oracle-frontend` refused
//!   `palette` and `lens` for this reason in so many words, and its argument transfers unchanged.
//! * **Six of the eight panels have another reader anyway.** Registers, Memory, Objects, Breakpoints,
//!   Watchpoints and Profiler are renderings of `emulator/registers`, `emulator/read_memory`,
//!   `emulator/object_list`, `emulator/breakpoint_list`, `emulator/watchpoint_hits` and
//!   `emulator/get_profiler`. Reading them back as *text* is the worst available way to get them, and the
//!   surface this method exists for is the text with **no other reader**.
//!
//! So the answer is the **top bar and the window title**, and it is smaller than the ambitious version on
//! purpose. Both are drawn unconditionally, outside the dock, on every frame: no tab can hide them, no
//! scroll offset can cut them, and nothing else on the bus reports either one.
//!
//! # One derivation, two consumers — enforced by the return type
//!
//! The bar does not get *read back*; it **hands over what it drew**. [`crate::ui::Transport::bar`] and
//! [`crate::main`]'s `build_ui` return the [`Run`]s they just painted, and `Loop::iterate` pushes them.
//! That is stronger than a helper both sides happen to call: there is no second expression to drift, and
//! the snapshot **cannot be composed before the bar draws it**, which is the ordering
//! [`oracle_aether::host::Host::set_screen_text`] requires. Pushing text that describes a frame not yet
//! presented is the trap that method's own doc names; here it is a type error rather than a rule.
//!
//! # Kinds this module does not produce, said out loud
//!
//! The contract's `kind` enum has five values; this module produces **two**, `titleBar` and `statusLine`.
//!
//! * **`toast`** — the player has none. The nearest thing is the transport bar's [`crate::ui::Echo`], the
//!   bus's verbatim answer to the last button click; it is persistent chrome inside the top bar rather
//!   than a transient overlay, so it is reported as part of the `statusLine` run it is drawn in, not as a
//!   toast it is not.
//! * **`palette`**, **`lens`** — `oracle-frontend`'s reasons (`F-SCREEN-TEXT-PALETTE-LENS`), plus the dock
//!   argument above. The player's lenses are its eight panels.

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

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe that can draw anything — the shape of a broken measurement, used deliberately below so the
    /// control that catches it is exercised.
    fn all_drawable(_: char, _: bool) -> Option<bool> {
        Some(true)
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
        let mut only_mono_lacks_b = |c: char, mono: bool| Some(!(mono && c == 'b'));
        let v = snapshot("t", &runs, &mut only_mono_lacks_b);
        assert_eq!(
            v[1].unrenderable,
            vec!["b".to_string()],
            "the proportional run's `b` draws; the monospace run's does not"
        );

        // …and the reverse, so the flag is not simply being ignored in one direction.
        let mut only_prop_lacks_a = |c: char, mono: bool| Some(!(!mono && c == 'a'));
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
        for (c, mono) in [
            ('\u{25B6}', false),
            ('\u{23F8}', false),
            ('\u{23ED}', false),
            ('\u{00B7}', true),
            ('|', false),
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
}
