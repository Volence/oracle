//! **What the player's window says, as a snapshot a tool can read** — the frontend half of
//! `emulator/screen_text` (contract §11.29, CR-H).
//!
//! Today the only instrument for "what is the player telling me right now?" is a human looking at the
//! window. This module is the model that removes that: once per present, the run loop asks the surfaces
//! that just finished drawing what they put on the glass, and hands the result to the bus.
//!
//! # Three rules this module exists to keep
//!
//! 1. **It never composes.** Every string here was built by the drawing code for its own purposes and is
//!    read back afterwards. Composing on demand — a handler asking the frontend to *build* the text when a
//!    caller asks — would run UI composition at an arbitrary point in the frame, and it is the one version
//!    of this feature that could perturb anything. It is refused by design, not by care.
//! 2. **It never restates layout arithmetic.** Every `rendered` string below comes from the same function
//!    that painted it ([`Overlay::status_line_layout`](crate::overlay::Overlay), `visible_toasts`), because
//!    a restated copy agrees with itself while drifting from the drawing code.
//! 3. **Source and rendered, both.** Rendered-only reports the message's shadow; source-only is
//!    structurally blind to the whole truncation defect class. See [`Surface`].
//!
//! # What is deliberately NOT here yet, said out loud
//!
//! The contract's `kind` enum has five values. This module produces **three** — `titleBar`, `statusLine`,
//! `toast` — and does **not** yet produce `palette` or `lens`. That is a scope decision, recorded here
//! rather than left for a reader to infer from an absent match arm:
//!
//! * The **lens** panels (CPU chip, watch ticker, profiler, hover callout) are renderings of data this bus
//!   already serves structurally — `emulator/registers`, `emulator/watchpoint_hits`,
//!   `emulator/get_profiler`. Reading them as *text* would be the worst available way to get them, and the
//!   surface `screen_text` exists for is the text that has **no other reader**.
//! * The **palette** is a modal UI a human opened; its rows come from a static command registry. Serving it
//!   honestly means lifting each panel's visible-row selection out of its draw loop first, because the fold
//!   cutoff and the scroll offset are computed inside the painting loop — and a `rendered` string that
//!   restated them would be exactly the drift rule 2 forbids.
//!
//! Neither omission needs a contract change to fix later: the fragment already names both kinds, and adding
//! them is additive. **Registered as `F-SCREEN-TEXT-PALETTE-LENS`.**

use crate::font;
use crate::overlay::{Overlay, Status};
use crate::present::Rect;

/// Which surface a [`Surface`] came from. Mirrors the contract's closed `kind` enum, minus the two values
/// this build does not yet produce (see the module note) — a variant nothing constructs would be dead code,
/// and dead code that *looks* like coverage is worse than an absence that is written down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    TitleBar,
    StatusLine,
    Toast,
}

/// One text surface: the string the player composed, and the string that reached the glass.
///
/// **Why both.** A caller checking *"did the player say why the ROM failed to open"* against `rendered`
/// alone sees `…/LOCKED (PE` and cannot tell that `Permission denied` was lost. A caller reading `text`
/// alone is told about characters that are not on screen, which makes the readout useless for the one
/// question it exists to answer — *is this window lying to me?*
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    pub kind: Kind,
    /// The SOURCE string the player composed.
    pub text: String,
    /// What is actually on the glass, after this surface's own truncation. A prefix of `text` today.
    pub rendered: String,
    /// Characters in `text` the player has **no glyph for** — it draws a hollow box where they should be.
    /// Empty when none.
    pub unrenderable: Vec<String>,
}

impl Surface {
    /// A surface **this font drew**, so its missing glyphs are computed from [`font::has_glyph`] — the
    /// drawing path's own predicate.
    ///
    /// Computed over `text` and not over `rendered`, which is what the fragment specifies and is also the
    /// more useful of the two: a caller wants to know the message contains a character the player cannot
    /// draw, whether or not truncation happened to cut it off this frame.
    ///
    /// First appearance order, de-duplicated: a toast with four em dashes has one defect, not four.
    pub(crate) fn drawn(kind: Kind, text: String, rendered: String) -> Self {
        let mut unrenderable: Vec<String> = Vec::new();
        for c in text.chars() {
            if !font::has_glyph(c) {
                let s = c.to_string();
                if !unrenderable.contains(&s) {
                    unrenderable.push(s);
                }
            }
        }
        Self {
            kind,
            text,
            rendered,
            unrenderable,
        }
    }

    /// A surface the **window manager** drew — the title bar, and nothing else.
    ///
    /// `unrenderable` is empty by construction and that is the honest answer, not a shortcut: the title bar
    /// is painted by the desktop with the desktop's font, so this build's 5×7 table says nothing about what
    /// a reader sees there. Running `has_glyph` over it would report the em dash in `Oracle — frame N` as a
    /// hollow box, which is a claim about the wrong font.
    ///
    /// `rendered` equals `text` for the same reason: whatever elision the window manager applies to a title
    /// too long for its bar is invisible to this process.
    fn window_manager(text: String) -> Self {
        Self {
            kind: Kind::TitleBar,
            rendered: text.clone(),
            text,
            unrenderable: Vec::new(),
        }
    }
}

/// **The whole snapshot, in Z order: back to front.**
///
/// The title bar first — it is behind everything in the sense that matters, being outside the client area
/// altogether — then the overlay's own surfaces in the order [`Overlay::draw`] paints them.
///
/// Called at the bottom of the present block, after every surface has finished drawing, so what it reports
/// is the frame that is actually on the glass rather than one being composed.
pub fn snapshot(title: &str, ov: &Overlay, area: Rect, st: &Status) -> Vec<Surface> {
    let mut out = vec![Surface::window_manager(title.to_string())];
    out.extend(ov.text_surfaces(area, st));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::INFO;

    /// **The field neither string could carry.** `text` and `rendered` both report the characters the
    /// player was *given*; neither can show that the glass has a hollow box where one of them should be,
    /// because [`crate::overlay::fit`] truncates and never transliterates.
    ///
    /// The literal here is the player's own first toast (`main.rs`: `PRESS ` FOR COMMANDS`), so this is a
    /// live defect rather than a constructed one — and it is asserted **against `font::has_glyph`**, the
    /// drawing path's own predicate, rather than against a hand-written list of characters the table lacks.
    /// A guard that restates its own input is the failure `font.rs`'s existing glyph test already has.
    #[test]
    fn a_character_the_font_cannot_draw_is_named_rather_than_silently_boxed() {
        let text = "PRESS ` FOR COMMANDS";
        // The control, first: if this font ever gains a backtick the row below stops meaning anything, and
        // it must say so rather than going green on a vacuous expectation.
        assert!(
            !font::has_glyph('`'),
            "this font now has a backtick glyph — the premise of this test is gone, re-measure it \
             rather than deleting it"
        );
        assert!(
            font::has_glyph('A'),
            "positive control: an ordinary letter is drawable"
        );

        let s = Surface::drawn(Kind::Toast, text.into(), text.into());
        assert_eq!(
            s.unrenderable,
            vec!["`".to_string()],
            "the one character the player draws as a hollow box, named"
        );
        assert_eq!(s.text, text, "the source is untouched");
        assert_eq!(
            s.rendered, text,
            "nothing was cut here; only a glyph is missing"
        );
    }

    /// De-duplicated, in first-appearance order: a message with four em dashes has one defect, not four.
    #[test]
    fn repeated_missing_glyphs_are_reported_once_each_in_the_order_they_appear() {
        assert!(
            !font::has_glyph('\u{2014}'),
            "premise: the em dash has no glyph"
        );
        let text = "WATCH CLEARED \u{2014} NO LONGER RECORDING \u{2014} SEE `LOG`";
        let s = Surface::drawn(Kind::Toast, text.into(), text.into());
        assert_eq!(
            s.unrenderable,
            vec!["\u{2014}".to_string(), "`".to_string()]
        );
    }

    /// A message with nothing missing carries an **empty list**, never an absent one — the same rule
    /// `truncated` follows, and for the same reason: absence and "none" must not be one artifact.
    #[test]
    fn a_fully_drawable_message_carries_an_empty_list_rather_than_no_list() {
        let s = Surface::drawn(Kind::Toast, "ALL FINE".into(), "ALL FINE".into());
        assert!(s.unrenderable.is_empty());
    }

    /// **The title bar is the window manager's, and this build's font table says nothing about it.**
    ///
    /// The em dash in `Oracle — frame N` has no 5×7 glyph, and the desktop draws it perfectly. Running the
    /// font's predicate over a string the font never sees would report a defect that does not exist — a
    /// claim about the wrong font, and a wrong answer is worse than no answer here.
    #[test]
    fn the_title_bar_is_not_measured_against_a_font_that_never_draws_it() {
        let title = "Oracle \u{2014} frame 12720 [PAUSED]";
        assert!(
            !font::has_glyph('\u{2014}'),
            "premise: the overlay font has no em dash, which is exactly why this row exists"
        );
        let s = Surface::window_manager(title.into());
        assert_eq!(s.kind, Kind::TitleBar);
        assert!(
            s.unrenderable.is_empty(),
            "the desktop draws this string, not our 5x7 table: {:?}",
            s.unrenderable
        );
        assert_eq!(s.rendered, s.text, "no truncation this process can observe");
    }

    /// **Z order, and the title bar is in it.** It is the one surface always visible regardless of F3,
    /// lenses or toasts, and the one invisible to a screenshot of the client area — omitting it would leave
    /// a caller asking "is it paused", getting an empty overlay, and being told nothing while the title bar
    /// said `[PAUSED]` the whole time.
    #[test]
    fn the_snapshot_leads_with_the_window_title_and_then_the_overlays_own_surfaces() {
        let mut ov = Overlay::new();
        ov.status_line = true;
        ov.push("HELLO", INFO);
        let area = Rect {
            x: 0,
            y: 0,
            w: 896,
            h: 672,
        };
        let st = Status {
            aspect: "4:3",
            native: (320, 224),
            ..Status::default()
        };
        let v = snapshot("Oracle \u{2014} frame 7", &ov, area, &st);

        let kinds: Vec<Kind> = v.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![Kind::TitleBar, Kind::StatusLine, Kind::Toast],
            "back to front: the title bar, the status line, then the toasts"
        );
        assert_eq!(v[0].text, "Oracle \u{2014} frame 7");
    }
}
