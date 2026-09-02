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
/// alone saw `…/LOCKED (PE` and could not tell that `Permission denied` was lost (the toast has since been
/// reordered reason-first and is cut with a visible `…` — F-TOAST-TRUNCATES — but the readout still has to
/// show the cut, not paper over it). A caller reading `text` alone is told about characters that are not on
/// screen, which makes the readout useless for the one question it exists to answer — *is this window lying
/// to me?*
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    pub kind: Kind,
    /// The SOURCE string the player composed.
    pub text: String,
    /// What is actually on the glass, after this surface's own truncation. For the status line a prefix of
    /// `text`; for a toast, the whole of `text` or a prefix of it followed by [`crate::overlay::TRUNCATION_MARK`]
    /// (`…`), so `rendered != text` is still exactly "it was cut" and the mark is on the glass too.
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
    /// a reader sees there. Running `has_glyph` over it would report the em dash in `Oracle — draws N` as a
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
    /// **Re-measured 2026-09-01 (F-FONT-BACKTICK).** This row was written against the player's own first
    /// toast, `PRESS ` FOR COMMANDS`, when the backtick had no glyph — a live defect. The glyph has since
    /// been added, so the live literal is now the *positive* half below, and the mechanism is exercised with
    /// a character the frontend has no message for (an arrow, U+2192). It is still asserted **against
    /// `font::has_glyph`**, the drawing path's own predicate, rather than against a hand-written list of
    /// characters the table lacks: a guard that restates its own input is the failure `font.rs`'s existing
    /// glyph test already has.
    #[test]
    fn a_character_the_font_cannot_draw_is_named_rather_than_silently_boxed() {
        // The control, first: if this font ever gains an arrow the row below stops meaning anything, and it
        // must say so rather than going green on a vacuous expectation.
        assert!(
            !font::has_glyph('\u{2192}'),
            "this font now has an arrow glyph — the premise of this test is gone, re-measure it \
             rather than deleting it"
        );
        assert!(
            font::has_glyph('A'),
            "positive control: an ordinary letter is drawable"
        );
        // The former defect, closed: the first toast the player shows now draws every character it holds.
        let first_toast = "PRESS ` FOR COMMANDS";
        assert!(
            font::has_glyph('`'),
            "F-FONT-BACKTICK regressed: the palette's own key has lost its glyph again"
        );
        let live = Surface::drawn(Kind::Toast, first_toast.into(), first_toast.into());
        assert_eq!(
            live.unrenderable,
            Vec::<String>::new(),
            "the player's first toast draws whole"
        );

        let text = "PRESS \u{2192} FOR COMMANDS";
        let s = Surface::drawn(Kind::Toast, text.into(), text.into());
        assert_eq!(
            s.unrenderable,
            vec!["\u{2192}".to_string()],
            "the one character the player draws as a hollow box, named"
        );
        assert_eq!(s.text, text, "the source is untouched");
        assert_eq!(
            s.rendered, text,
            "nothing was cut here; only a glyph is missing"
        );
    }

    /// De-duplicated, in first-appearance order: a message with four arrows has one defect, not four.
    ///
    /// **Re-measured 2026-09-01 (F-FONT-EMDASH).** The fixture used to be the em dash and the backtick, both
    /// then missing; both are drawable now, so the same shape is run over two characters that still are not,
    /// and the old pair is asserted drawable rather than deleted — the premise moved, the row did not go.
    #[test]
    fn repeated_missing_glyphs_are_reported_once_each_in_the_order_they_appear() {
        assert!(
            font::has_glyph('\u{2014}') && font::has_glyph('`'),
            "F-FONT-EMDASH / F-FONT-BACKTICK regressed: the separator or the palette key lost its glyph"
        );
        assert!(
            !font::has_glyph('\u{2192}') && !font::has_glyph('\u{00B7}'),
            "premise: the arrow and the middle dot have no glyph"
        );
        let text = "WATCH CLEARED \u{2192} NO LONGER RECORDING \u{2192} SEE \u{00B7}LOG\u{00B7} \u{2014} `OK`";
        let s = Surface::drawn(Kind::Toast, text.into(), text.into());
        assert_eq!(
            s.unrenderable,
            vec!["\u{2192}".to_string(), "\u{00B7}".to_string()],
            "each missing glyph once, in first-appearance order, and the drawable ones absent"
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
    /// Running the font's predicate over a string the font never sees would report a defect that does not
    /// exist — a claim about the wrong font, and a wrong answer is worse than no answer here.
    ///
    /// **Re-measured 2026-09-01.** The em dash in `Oracle — draws N` used to be the character with no 5×7
    /// glyph that made the point; it has one now (F-FONT-EMDASH), so the title carries a character the
    /// overlay font still cannot draw — the desktop's font can — and the premise is asserted on that one.
    #[test]
    fn the_title_bar_is_not_measured_against_a_font_that_never_draws_it() {
        let title = "Oracle \u{2192} draws 12720 [PAUSED]";
        assert!(
            !font::has_glyph('\u{2192}'),
            "premise: the overlay font has no arrow, which is exactly why this row exists"
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

    /// **Every message this crate can put on the glass draws whole — no hollow boxes.**
    ///
    /// The toasts are `format!` literals scattered across the frontend (`notify`, `notify_err`, `ov.push`,
    /// the config loader's `warnings.push`, the lens and watch messages), and a hand-copied list of them here
    /// would be stale by the next parcel. So the enumeration is **derived from the source**: every string
    /// literal in every non-test region of every module `main.rs` declares, read back through the crate's own
    /// `mod` lines at test time. That is a strict superset of the live toasts (it also covers `println!`-only
    /// text, which costs nothing once the glyphs exist), and it was cross-checked against the hand grep that
    /// found the toast sites in the first place:
    ///
    /// ```text
    /// grep -n 'notify\|ov\.push\|toast' crates/oracle-frontend/src/main.rs
    /// grep -n -P '"[^"]*[^\x00-\x7F][^"]*"' crates/oracle-frontend/src/*.rs | grep -v '///'
    /// ```
    ///
    /// which is how the characters the table lacked were found: the backtick (`PRESS ` FOR COMMANDS`), the
    /// em dash (41 literals) and the ellipsis (`config::kept_warning`) — and, once this row ran over every
    /// literal instead of the grep's toast sites, the `~` in the usage text. Format placeholders (`{e}`,
    /// `{:?}`) are stripped before measuring, because what reaches the glass is the substituted value, and
    /// `\n`/`\t` escapes are skipped as line structure rather than glyphs. `unrenderable` is computed by
    /// [`Surface::drawn`] — the same predicate the wire readout uses — so a literal failing here is exactly a
    /// literal that would report a hollow box over `emulator/screen_text`.
    ///
    /// Red-first (2026-09-01): with the em-dash arm removed from `font::glyph`, this row failed with `62
    /// undrawable literal(s):` starting `main.rs: ":  — cannot read " lacks glyphs for ["—"]`.
    #[test]
    fn every_string_literal_the_frontend_can_show_is_drawable() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        // The module tree, from the crate's own `mod` declarations rather than a directory walk, so a file
        // that is not compiled in cannot smuggle a literal into the measurement or hide one from it. A `mod
        // x;` resolves to `x.rs` or `x/mod.rs` (the crate uses both — `lens/mod.rs` declares its own
        // submodules), and the walk follows declarations into subdirectories the same way rustc does.
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
            "COULD NOT MEASURE: only {} modules found — the `mod` scan is broken, not the font",
            modules.len()
        );

        let mut checked = 0usize;
        let mut defects: Vec<String> = Vec::new();
        for path in &modules {
            let file = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            let src = std::fs::read_to_string(path).expect("read a module the walk already opened");
            // Production code only: everything up to the module's first `#[cfg(test)]`.
            let prod = src.split("#[cfg(test)]").next().unwrap_or("");
            for lit in string_literals(prod) {
                checked += 1;
                let s = Surface::drawn(Kind::Toast, lit.clone(), lit.clone());
                if !s.unrenderable.is_empty() {
                    defects.push(format!(
                        "{file}: {:?} lacks glyphs for {:?}",
                        lit, s.unrenderable
                    ));
                }
            }
        }
        assert!(
            checked > 200,
            "COULD NOT MEASURE: only {checked} literals lexed across {} files — the lexer is broken, not the font",
            modules.len()
        );
        // Sanity on the lexer against the one literal this parcel exists for.
        assert!(
            string_literals(r#"ov.push("PRESS ` FOR COMMANDS", INFO);"#)
                == vec!["PRESS ` FOR COMMANDS"],
            "the lexer does not recover a plain literal"
        );
        assert!(
            defects.is_empty(),
            "{} undrawable literal(s):\n{}",
            defects.len(),
            defects.join("\n")
        );
    }

    /// The string literals in a chunk of Rust source, unescaped as the compiler would (`\"`, `\\`, `\u{..}`,
    /// and the backslash-newline continuation), with `{...}` format placeholders removed and `\n`/`\t`
    /// dropped. Line comments are skipped; char literals (`'"'`, `'\''`) are stepped over so their quotes
    /// cannot open a string. Deliberately small: this is a test aid over one crate's own style, not a Rust
    /// lexer, and the caller asserts a floor on what it finds so a silent miss cannot pass as "clean".
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
                // A char literal, or a lifetime. `'\...'` is a char; `'x'` is a char; anything else is a
                // lifetime and only the quote itself is consumed.
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
                                // Continuation: the newline and the next line's leading whitespace vanish.
                                i += 1;
                                while i < chars.len() && chars[i].is_whitespace() {
                                    i += 1;
                                }
                            }
                            Some('u') => {
                                // \u{XXXX}
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
                        if chars.get(i + 1) == Some(&'{') {
                            lit.push('{');
                            i += 2;
                        } else {
                            // A format placeholder: skip to its close, allowing nested `{}` in width/precision.
                            let mut depth = 0usize;
                            while i < chars.len() {
                                if chars[i] == '{' {
                                    depth += 1;
                                } else if chars[i] == '}' {
                                    depth -= 1;
                                    if depth == 0 {
                                        i += 1;
                                        break;
                                    }
                                }
                                i += 1;
                            }
                        }
                    } else if chars[i] == '}' && chars.get(i + 1) == Some(&'}') {
                        lit.push('}');
                        i += 2;
                    } else {
                        lit.push(chars[i]);
                        i += 1;
                    }
                }
                i += 1; // closing quote
                out.push(lit);
            } else {
                i += 1;
            }
        }
        out
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
        let v = snapshot("Oracle \u{2014} draws 7", &ov, area, &st);

        let kinds: Vec<Kind> = v.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![Kind::TitleBar, Kind::StatusLine, Kind::Toast],
            "back to front: the title bar, the status line, then the toasts"
        );
        assert_eq!(v[0].text, "Oracle \u{2014} draws 7");
    }
}
