//! **The spawn picker's model** — which archetype a click places, chosen from a list rather than cycled
//! to, and what this window did to the machine's run state to make that click legal.
//!
//! Two halves of one parcel, in one module because they are one gesture. The owner asked for both in the
//! same breath:
//!
//! > *"for this I'm thinking a panel where you can select the item instead of having to click a button to
//! > kind of pseudo scroll through them"*
//!
//! > *"can the click of the object pause for 1 ms or whatever is needed and spawn it? like
//! > programaticallyy instead of me needing to manually pause."*
//!
//! The design was ruled before it was built, in `docs/2026-09-05-spawn-autopause-design.md`, and the two
//! rulings this module exists to carry are: **capture the prior run state and restore it, resuming only if
//! the machine was running when the click arrived**, and **the window says it did this**.
//!
//! # ⚑ Nothing here holds an egui type, and that is the point rather than the tidiness
//!
//! `docs/2026-09-05-debug-window-audit.md` §3's first lesson, from the Pacing exemplar: this window cannot
//! be opened from an agent seat, so a panel whose correctness lives in its draw calls is a panel nothing
//! can check. The projection is here, testable against values a test chooses; [`crate::ui`] lays the
//! result out and decides nothing.
//!
//! # The two numbers a picker owes a reader, and why neither is a big-number readout
//!
//! The audit's §1 offers three shapes. A list of symbol names is **rows of like-shaped data**, so it is a
//! **column table** with selection carried by fill, exactly as the Objects pool table already carries it.
//! It is deliberately *not* a big-number readout: nobody opens a picker to read a count. The counts are a
//! single small line above the rows, and the two facts that are *not* counts get sentences, per P6:
//!
//! * **the filter matched nothing** is a stated line, never an empty box, and it says how many archetypes
//!   are there to be matched, because "no results" and "no archetypes" are different findings;
//! * **the listing was cut short** is [`spawn::Archetypes::truncation_note`], reused rather than
//!   re-spelled, because a partial measurement rendered as a whole one is the failure the frontend wrote
//!   that note for.
//!
//! # Where spawn mode lives, and why this is not a tab
//!
//! `crate::palette`'s rule, already on disk and not re-litigated here: *things you look at are tabs; things
//! you DO are controls*, and it names spawn as one of the things you do. So the picker is drawn in the
//! Screen tab's control strip, above the picture it places into, rather than in a dock tab of its own,
//! and a person can see the list and the spot at the same time.

use oracle_frontend::spawn;

/// One archetype as the picker draws it.
///
/// The name is carried whole, in the listing's own spelling, because it is the string
/// `emulator/object_spawn`'s `defSymbol` is sent as: a picker that showed a prettified name would be
/// offering a choice it cannot then make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The symbol, exactly as the loaded listing spells it. Drawn in the monospace face, which is P3's
    /// carve-out for *symbol names as they appear in a listing* and not a licence for the prose beside it.
    pub name: String,
    /// Whether a click on the picture places **this** one right now.
    pub selected: bool,
}

/// The picker's whole surface, as facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The rows to draw, already filtered. Empty **iff** [`Listing::absence`] is `Some`, so a renderer
    /// cannot draw an empty box with nothing said in it.
    pub rows: Vec<Row>,
    /// How many rows are drawn out of how many the mode holds, in one small line.
    pub count: String,
    /// The stated line drawn **instead of** rows when there are none (P6: an absent fact is a sentence,
    /// never a blank and never a zero).
    pub absence: Option<String>,
    /// The bus's bounded symbol search was cut short, so the mode is holding fewer archetypes than the
    /// listing has. [`spawn::Archetypes::truncation_note`]'s words, not a second wording of them.
    pub truncation: Option<String>,
}

/// **The picker, projected.**
///
/// `names` is the mode's own list, `selected` the mode's own selection, `total` what the bounded search
/// said the listing holds, and `filter` whatever is in the box. Case-insensitive substring, trimmed: a
/// filter is a way of finding `ObjDef_Ring` by typing `ring`, and requiring the prefix and the case would
/// make it a second way of failing to find it.
pub fn listing(names: &[String], selected: Option<&str>, total: usize, filter: &str) -> Listing {
    let needle = filter.trim().to_ascii_lowercase();
    let rows: Vec<Row> = names
        .iter()
        .filter(|n| needle.is_empty() || n.to_ascii_lowercase().contains(&needle))
        .map(|n| Row {
            name: n.clone(),
            selected: selected == Some(n.as_str()),
        })
        .collect();

    // Reused rather than re-spelled: the frontend composed this sentence for the arm message and the two
    // must not drift into two accounts of one cut-short search.
    let truncation = spawn::Archetypes {
        names: names.to_vec(),
        total,
    }
    .truncation_note();

    let absence = if !rows.is_empty() {
        None
    } else if names.is_empty() {
        // Reachable only through a disarmed mode, which the caller should not be drawing at all. Stated
        // anyway, because the alternative is a renderer drawing an empty frame and a reader deciding for
        // themselves what it means.
        Some("spawn mode is off, so there is no list and a click arms a watch instead.".to_string())
    } else {
        Some(format!(
            "no archetype here contains {:?}. This build offers {}; clear the box to see them all.",
            filter.trim(),
            plural(names.len(), "archetype", "archetypes"),
        ))
    };

    Listing {
        count: format!(
            "{} of {} shown",
            rows.len(),
            plural(names.len(), "archetype", "archetypes")
        ),
        rows,
        absence,
        truncation,
    }
}

/// `1 frame` against `2 frames`, in one place, because a count that says `1 frames` reads as a bug in the
/// number rather than in the sentence.
fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// -------------------------------------------------------------------------------------------------------
// ⚑ What the window did to the run state, and the standing statement that says so
// -------------------------------------------------------------------------------------------------------

/// **What this window did to the machine's run state for one click**, so a resume nobody performed is
/// never a mystery.
///
/// # The hazard is the resume, not the hitch
///
/// `emulator/object_spawn` needs a paused machine, so the click pauses one. That costs at most two frames
/// of emulated time (`OBJREQ_DEFAULT_MAX_FRAMES`, `crates/oracle-aether/src/engine.rs`), which is roughly
/// three to six milliseconds of wall time at this window's measured pace. Nobody minds the hitch.
///
/// **A blind resume is the part that hurts**, and the design page's two cases are why: the owner pauses to
/// line up a placement and the window starts the game under him; or a socket client paused the machine to
/// read it and the window resumes under the client mid read. The second is worse because nothing
/// announces it and it breaks another actor's invariant rather than a person's expectation.
///
/// So the prior state is captured from the server's own `wasRunning` and put back, and a machine that was
/// already paused is left exactly as it was found: no pause, no resume, nothing for a client to notice.
///
/// # Why an enum rather than a pair of booleans
///
/// The four outcomes are four different things to say, and two of them mean **the machine is not in the
/// state the person left it in**. A `paused: bool, resumed: bool` would let a renderer draw the dangerous
/// pair as the ordinary one. This is P6's discipline applied to a state rather than to an absence: make
/// the case that must be loud unrepresentable as the case that is quiet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunState {
    /// The machine was already paused when the click arrived. Nothing was paused and nothing was resumed.
    AlreadyPaused,
    /// It was running, so the window paused it, placed, and resumed it. `frames` is the server's own
    /// `framesAdvanced` when the spawn succeeded, and `None` when it was refused before any frame ran.
    Restored { frames: Option<u64> },
    /// It was running, the window paused it, and **the resume was refused**. The machine is paused and
    /// nobody at this window paused it.
    Stranded { why: String },
    /// The pause was accepted but did not say what it had changed, so the window did **not** resume:
    /// leaving a machine paused is recoverable by a person who can see this line, and starting one
    /// somebody deliberately stopped is not.
    PriorStateUnknown,
}

impl RunState {
    /// **The standing statement.** Drawn for as long as it is the last thing that happened, never as a
    /// toast: a toast expires and the fact that this window paused a machine and could not restart it
    /// does not.
    pub fn sentence(&self) -> String {
        match self {
            Self::AlreadyPaused => "This machine was already paused when you clicked, so the window \
                                    neither paused nor resumed it."
                .to_string(),
            Self::Restored { frames } => {
                let mut s = "This window paused the machine to place the object and resumed it, \
                             because it was running when you clicked."
                    .to_string();
                if let Some(n) = frames {
                    s.push_str(&format!(
                        " {} of emulated time passed while it was paused.",
                        plural(*n as usize, "frame", "frames")
                    ));
                }
                s
            }
            Self::Stranded { why } => format!(
                "This window paused the machine to place the object and the resume was REFUSED, so it \
                 is still paused and nothing here will restart it on its own: {why}"
            ),
            Self::PriorStateUnknown => "This window paused the machine to place the object and could \
                                        not read back whether it had been running, so it left the \
                                        machine paused rather than starting one you may have stopped \
                                        on purpose."
                .to_string(),
        }
    }

    /// Whether the machine's run state is **not** what the person left it in. The renderer colours on
    /// this and never on the shape of the sentence, which is P5's rule generalised off refusals.
    pub fn alarming(&self) -> bool {
        matches!(self, Self::Stranded { .. } | Self::PriorStateUnknown)
    }

    /// The clause that answers the question a person asks once and then never again: *the window ran
    /// frames while I was holding right, was that a race?*
    ///
    /// It is not, and saying so costs a hover. The hold and pad merge is unchanged during those frames,
    /// so held buttons apply to frames that were going to run anyway.
    pub const HELD_INPUT_HOVER: &'static str =
        "Buttons you are holding keep being merged into the machine's pads while this happens. Those are \
         frames that were going to run anyway, so your input applies to them exactly as it always would.";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec![
            "ObjDef_Ring".to_string(),
            "ObjDef_Spring".to_string(),
            "ObjDef_Monitor".to_string(),
        ]
    }

    /// Every string this panel can draw, for the sweeps below. **Assembled from the projection**, not
    /// from a list of literals a later edit would not appear in.
    fn every_string(l: &Listing) -> Vec<String> {
        let mut v: Vec<String> = l.rows.iter().map(|r| r.name.clone()).collect();
        v.push(l.count.clone());
        v.extend(l.absence.clone());
        v.extend(l.truncation.clone());
        v
    }

    /// The four run states, so a sweep cannot pass by only visiting the quiet ones.
    fn every_run_state() -> Vec<RunState> {
        vec![
            RunState::AlreadyPaused,
            RunState::Restored { frames: Some(1) },
            RunState::Restored { frames: None },
            RunState::Stranded {
                why: "-32005 the machine is held by a breakpoint".to_string(),
            },
            RunState::PriorStateUnknown,
        ]
    }

    /// **The selection is the mode's, and exactly one row carries it.**
    #[test]
    fn the_filter_narrows_the_rows_and_the_selection_survives_it() {
        let n = names();
        let all = listing(&n, Some("ObjDef_Spring"), 3, "");
        assert_eq!(all.rows.len(), 3);
        assert_eq!(all.rows.iter().filter(|r| r.selected).count(), 1);
        assert!(all.rows[1].selected, "the selected row is the mode's");
        assert_eq!(all.absence, None, "three rows is not an absence");

        // Case-insensitive and a substring, because the point of a filter is finding `ObjDef_Ring` by
        // typing `ring`.
        let some = listing(&n, Some("ObjDef_Spring"), 3, "  RIN  ");
        assert_eq!(
            some.rows
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["ObjDef_Ring", "ObjDef_Spring"],
            "a trimmed, case-folded substring is what the box means"
        );
        assert!(
            some.rows.iter().any(|r| r.selected),
            "a selection that survives the filter must still be marked"
        );

        // And a selection filtered OUT is still the mode's selection: the picker narrows what is drawn,
        // never what a click would place. A filter that silently changed the armed archetype would place
        // something other than the badge names.
        let hidden = listing(&n, Some("ObjDef_Spring"), 3, "monitor");
        assert_eq!(hidden.rows.len(), 1);
        assert!(
            !hidden.rows[0].selected,
            "the drawn row is not the selected one and must not claim to be"
        );
    }

    /// **An empty result is a sentence, not an empty box** (P6), and it says how many there were to match.
    #[test]
    fn a_filter_that_matches_nothing_says_so_and_says_how_many_there_were() {
        let n = names();
        let l = listing(&n, Some("ObjDef_Ring"), 3, "zzz");
        assert!(l.rows.is_empty());
        let a = l.absence.expect("an empty list owes the reader a line");
        assert!(
            a.contains("zzz") && a.contains('3'),
            "the line must name what was typed and how many archetypes it was matched against: {a:?}"
        );
        assert_eq!(l.count, "0 of 3 archetypes shown");

        // The other absence is a different finding and must not read like this one.
        let none = listing(&[], None, 0, "");
        let a = none.absence.expect("a disarmed mode owes a line too");
        assert!(a.contains("spawn mode is off"), "{a:?}");
    }

    /// **A cut-short search is stated, in the frontend's words rather than in a second wording.**
    ///
    /// The failure this guards is the quiet one: arming over the first 20 of 137 archetypes and drawing
    /// a list that looks complete, which is the window deciding on the reader's behalf that the other 117
    /// do not exist.
    #[test]
    fn a_bounded_search_says_it_was_cut_short_and_a_complete_one_claims_nothing() {
        let n = names();
        assert_eq!(
            listing(&n, None, 3, "").truncation,
            None,
            "a complete listing must not carry a truncation note"
        );
        let cut = listing(&n, None, 137, "")
            .truncation
            .expect("3 of 137 is a partial measurement and must say so");
        assert!(
            cut.contains("137") && cut.contains('3'),
            "the note must carry both numbers: {cut:?}"
        );
        assert_eq!(
            cut,
            spawn::Archetypes {
                names: n,
                total: 137
            }
            .truncation_note()
            .unwrap(),
            "this must BE the frontend's sentence, not a copy that can drift from it"
        );
    }

    /// **The dangerous outcomes are the loud ones, and the ordinary one is quiet.**
    ///
    /// This is the ruling's requirement in the type: the window is allowed to pause and restore silently
    /// in the ordinary case, and is not allowed to leave a machine paused without saying so.
    #[test]
    fn only_the_states_that_leave_the_machine_moved_are_alarming() {
        assert!(!RunState::AlreadyPaused.alarming());
        assert!(!RunState::Restored { frames: Some(2) }.alarming());
        assert!(RunState::Stranded {
            why: "x".to_string()
        }
        .alarming());
        assert!(RunState::PriorStateUnknown.alarming());
    }

    /// **Every run state says what happened to the machine, and the refused resume carries the server's
    /// own words.**
    #[test]
    fn every_run_state_states_what_it_did_and_a_refusal_carries_the_servers_words() {
        for s in every_run_state() {
            let t = s.sentence();
            assert!(
                t.len() > 40 && t.contains("machine"),
                "every state must say what became of the machine, or it is the mystery this exists to \
                 prevent: {t:?}"
            );
        }
        assert!(
            RunState::AlreadyPaused
                .sentence()
                .contains("neither paused nor resumed"),
            "a client-paused machine must be told it was left alone"
        );
        assert!(
            RunState::Restored { frames: Some(1) }
                .sentence()
                .contains("1 frame of"),
            "one frame is not `1 frames`"
        );
        assert!(RunState::Restored { frames: Some(2) }
            .sentence()
            .contains("2 frames of"),);
        assert!(
            !RunState::Restored { frames: None }
                .sentence()
                .contains("frames of emulated time"),
            "a refused spawn advanced nothing, so it must not claim a frame count it never read"
        );
        let why = "-32005 the machine is held by a breakpoint";
        assert!(
            RunState::Stranded {
                why: why.to_string()
            }
            .sentence()
            .contains(why),
            "the refusal is the server's and is carried whole"
        );
    }

    /// **P2, on the rendered value rather than on the source.**
    ///
    /// The style page states P2's check as "no width-padded format specifier", and that check returns zero
    /// on a string whose padding is hand-counted spaces inside the literal. This is the widened form, the
    /// one `pacing.rs` proved out.
    #[test]
    fn no_string_this_panel_draws_pads_itself_into_a_column() {
        let n = names();
        let mut all: Vec<String> = Vec::new();
        for f in ["", "zzz", "ring"] {
            all.extend(every_string(&listing(&n, Some("ObjDef_Ring"), 137, f)));
        }
        all.extend(every_string(&listing(&[], None, 0, "")));
        all.extend(every_run_state().iter().map(RunState::sentence));
        all.push(RunState::HELD_INPUT_HOVER.to_string());
        for s in all {
            assert!(
                !s.contains("  "),
                "a run of spaces is a column drawn inside a string, which is the pseudo-table P2 \
                 outlaws: {s:?}"
            );
            assert!(!s.contains('\t'), "a tab is the same defect: {s:?}");
        }
    }

    /// **P10** (no em or en dashes) and **P9** (no specification citations) over everything this panel can
    /// draw, in every arm.
    #[test]
    fn nothing_this_panel_draws_carries_a_dash_or_cites_the_specification() {
        let n = names();
        let mut all: Vec<String> = Vec::new();
        for f in ["", "zzz", "ring"] {
            all.extend(every_string(&listing(&n, Some("ObjDef_Ring"), 137, f)));
        }
        all.extend(every_string(&listing(&[], None, 0, "")));
        all.extend(every_run_state().iter().map(RunState::sentence));
        all.push(RunState::HELD_INPUT_HOVER.to_string());
        for s in all {
            for bad in ['\u{2014}', '\u{2013}'] {
                assert!(
                    !s.contains(bad),
                    "user-facing text carries {bad:?}, which the owner's 2026-09-05 ruling bars: {s:?}"
                );
            }
            assert!(
                !s.contains('§') && !s.contains("protocol.md"),
                "the person at this window is not holding the specification: {s:?}"
            );
        }
    }
}
