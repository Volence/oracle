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
//! # ⚑ Where spawn mode lives, and why this IS a tab now
//!
//! This section used to argue the opposite, and it was overruled by the person who uses the window. It
//! shipped in the Screen tab's control strip on `crate::palette`'s rule — *things you look at are tabs;
//! things you DO are controls*, and that header names spawn as one of the things you do — and the ruling
//! was flagged at landing as reversible and his. He reversed it after using it:
//!
//! > *"the placement works well it seems! it just takes up a lot of space haha. Maybe it should be its own
//! > debug tool in the right panel instead of part of screens?"*
//!
//! The strip is drawn **above** the picture and `crate::ui::Panels::screen` allocates whatever is left, so
//! a growing list and the game view were spending the same pixels. The rule was not wrong; its scope was.
//! It is about **one-shot gestures** — reset, press, write, a button you hit and forget. A picker is a
//! *standing list you read*, and a list is the one thing a strip above a picture cannot hold. `palette.rs`
//! is amended to say so, so the next reader is not steered by a rule the window no longer follows.
//!
//! So the rows are drawn by `crate::ui::Panels::spawn` in `crate::ui::Tab::Spawn`, and **nothing about
//! arming or placing moved with them**: the click that places is still on the picture, in the Screen tab,
//! and this module's projection is unchanged by the move. What stays in the strip is the pair that must be
//! seen without going to look for it — the badge, and [`RunState`]'s standing statement below.

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
// The subtype list — which one of an archetype's forms a click places
// -------------------------------------------------------------------------------------------------------

/// One subtype as the picker draws it.
///
/// The whole equate name is carried alongside the label because the label is a **shortening for reading**
/// and the name is what a selection is made by: [`spawn::Subtypes::get`] is asked for the name, exactly as
/// `spawn::Mode::select` is asked for the archetype's, and for the same reason. A picker that selected by
/// its own abbreviation would be choosing by a string the listing never published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtypeRow {
    /// The equate's whole name, in the listing's own spelling. What a click sends back.
    pub name: String,
    /// The name with the object's half taken off, which is what the row reads as: `Up_Red`.
    pub label: String,
    /// The listing's value, in the monospace hex form P3 carves out for exactly this.
    pub value: String,
    /// Whether a click on the picture carries **this** one right now.
    pub selected: bool,
    /// Whether it can be chosen at all. `false` only for the case below.
    pub offered: bool,
    /// Why it cannot be chosen, when [`SubtypeRow::offered`] is `false`. P6: the row is drawn with the
    /// reason on it rather than dropped, because a row that vanishes teaches nothing and a masked value
    /// would place a different object than the row names.
    pub note: Option<String>,
}

/// **The subtype picker's whole surface, as facts.**
///
/// Four things that are not rows, and each is a different finding: nothing to choose from, a list that is
/// short without looking short, two namespaces that overlap, and what a click is carrying right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtypeListing {
    /// The rows to draw, ordered by value. Empty **iff** [`SubtypeListing::absence`] is `Some`.
    pub rows: Vec<SubtypeRow>,
    /// How many there are, in one small line.
    pub count: String,
    /// The stated line drawn **instead of** rows (P6).
    pub absence: Option<String>,
    /// ⚑ The search was cut short. [`spawn::Subtypes::truncation_note`]'s words, not a second wording.
    pub truncation: Option<String>,
    /// Two archetypes' namespaces overlap. [`spawn::Subtypes::collision_note`]'s words.
    pub collision: Option<String>,
    /// **The standing statement of what a click carries**, always present.
    ///
    /// Standing rather than implied by which row is filled, on the badge's own rule: the thing that
    /// changes what a click **does** says so in words, so a reader who has scrolled the list or filtered
    /// the archetypes above it is never left inferring the armed subtype from a highlight.
    pub armed: String,
}

/// **The subtype picker, projected.**
///
/// `chosen` is the whole equate name of the armed subtype, or `None` when nothing is armed, which happens
/// only when there is nothing armable: [`spawn::Subtypes::default_choice`] arms the lowest valued one the
/// moment an archetype with subtypes is selected, so that "nothing chosen" and "nothing to choose" are
/// the same state rather than two that look alike.
pub fn subtype_listing(s: &spawn::Subtypes, chosen: Option<&str>) -> SubtypeListing {
    let prefix = s.prefix.clone().unwrap_or_default();
    let rows: Vec<SubtypeRow> = s
        .entries
        .iter()
        .map(|e| {
            let byte = e.byte();
            SubtypeRow {
                name: e.name.clone(),
                label: e.short(&prefix).to_string(),
                value: match byte {
                    Some(b) => format!("${b:02X}"),
                    None => format!("${:X}", e.value),
                },
                selected: chosen == Some(e.name.as_str()),
                offered: byte.is_some(),
                note: byte.is_none().then(|| {
                    format!(
                        "not offered: a placement carries one byte of subtype and this listing gives \
                         {} the value {}, which does not fit in one. Sending it would place a \
                         different form than this row names, so it is shown and left unselectable \
                         rather than quietly cut down.",
                        e.name, e.value
                    )
                }),
            }
        })
        .collect();

    let armed = match chosen.and_then(|c| s.get(c)) {
        Some(e) => format!(
            "A click places {} as {} ({}), and that is the value this build's listing publishes for it.",
            s.archetype,
            e.short(&prefix),
            match e.byte() {
                Some(b) => format!("${b:02X}"),
                None => format!("${:X}", e.value),
            },
        ),
        None if s.entries.is_empty() => format!(
            "A click places {}, which is the only form this build's listing names for it.",
            s.archetype
        ),
        None => format!(
            "No subtype is armed for {}, because none of the {} below fits in the one byte a \
             placement carries. A click places the archetype's own default form.",
            s.archetype,
            plural(s.entries.len(), "subtype", "subtypes"),
        ),
    };

    SubtypeListing {
        count: plural(rows.len(), "subtype", "subtypes"),
        rows,
        absence: s.absence(),
        truncation: s.truncation_note(),
        collision: s.collision_note(),
        armed,
    }
}

// -------------------------------------------------------------------------------------------------------
// ⚑ The ring, which is not in either list above and cannot be
// -------------------------------------------------------------------------------------------------------

/// **The ring section of the Spawn tab, projected.**
///
/// # Why a ring is not a row in the archetype list
///
/// The picker above draws what `emulator/lookup_symbol` finds under `ObjDef_`, and this build publishes
/// six of them: `Spring`, `PathSwap`, `Static`, `Solid`, `Enemy`, `Parent`. **There is no `ObjDef_Ring`
/// and there is not going to be one.** A ring in this engine is not an object: it takes no pool slot, has
/// no definition record, and never reaches the `Obj_Req_` mailbox `emulator/object_spawn` writes. It
/// lives in one flat array that `DrawRings` walks straight into the sprite table.
///
/// So ring placement is a **second thing a click can be**, alongside picking and placing an object, and
/// it gets its own control rather than a row in a list it could never legitimately appear in. Adding a
/// made-up `ObjDef_Ring` row would be this window asserting a name the game does not have, which is the
/// one thing the whole discovered-not-listed design exists to prevent.
///
/// # The three things it draws, and one of them is a condition of the feature
///
/// A toggle, a standing statement of what a click does, and [`oracle_frontend::rings::TEMPORARY`]. The
/// third is not polish: a placed ring is swept out of the buffer once the camera moves away and nothing
/// brings it back, so a person who is not told that watches their ring vanish and reasonably concludes
/// the window is broken. It is drawn for as long as the mode is armed, on the spawn badge's own rule that
/// a toast expires and a design does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingListing {
    /// Whether a left-click on the picture places a ring right now.
    pub armed: bool,
    /// **The standing statement of what a click does**, always present, in both arms. Never carried by a
    /// highlight: a reader who has scrolled either list above is otherwise inferring it.
    pub armed_line: String,
    /// ⚑ **The vanishing rule**, drawn while the mode is armed and `None` while it is not.
    ///
    /// [`oracle_frontend::rings::TEMPORARY`]'s own words rather than a second wording, for the reason
    /// [`SubtypeListing::truncation`] carries the frontend's: two accounts of one design drift until one
    /// of them stops being true.
    pub temporary: Option<String>,
}

/// **The ring section, projected**, from the one fact the panel holds about it.
///
/// `object` is what the archetype picker would place instead, so the disarmed line can say what a click
/// *does* do rather than only what it does not. `None` when nothing is armed there either, which is the
/// third state and reads as neither of the other two.
pub fn ring_listing(armed: bool, object: Option<&str>) -> RingListing {
    let armed_line = if armed {
        "A click on the picture places a ring. The object picker above is off while this is on, \
         because a click can only do one of the two."
            .to_string()
    } else {
        match object {
            Some(a) => format!(
                "A click on the picture places {a}, not a ring. Turn this on to place rings instead."
            ),
            None => "A click on the picture arms a watch. Turn this on to place rings instead."
                .to_string(),
        }
    };
    RingListing {
        armed,
        armed_line,
        temporary: armed.then(|| oracle_frontend::rings::TEMPORARY.to_string()),
    }
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

/// **What the window paused the machine in order to do, and what the person did to start it.**
///
/// Two gestures now pause and restore: the click that places an object, and the selection that takes a
/// picture of one ([`crate::preview`]). The four outcomes are identical for both and the sentences must not
/// be, because *"paused the machine to place the object"* is a false account of a selection change, and a
/// person reading it goes looking for an object that was never placed.
///
/// A two-field constant rather than a fifth enum variant or a second copy of [`RunState::sentence`]: the
/// **states** are what the type is about and they did not change, so what varies is a noun and a clause,
/// and both are supplied by the caller that knows which gesture it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deed {
    /// The verb phrase after "paused the machine to": `place the object`.
    pub doing: &'static str,
    /// The clause naming what started it: `when you clicked`.
    pub occasion: &'static str,
}

/// The click on the picture that places an object.
pub const PLACING: Deed = Deed {
    doing: "place the object",
    occasion: "when you clicked",
};

impl Default for Deed {
    /// [`PLACING`], because a panel that has not paused for anything yet has nothing to say and the one
    /// gesture that predates this type is the click. There is no empty [`Deed`]: two empty strings would
    /// produce a sentence with a hole in it, which is worse than the wrong noun.
    fn default() -> Self {
        PLACING
    }
}

/// The selection change that takes a picture of one ([`crate::preview`]).
pub const PREVIEWING: Deed = Deed {
    doing: "take a picture of the object",
    occasion: "when you chose it",
};

impl RunState {
    /// **The standing statement**, for the gesture that caused it. Drawn for as long as it is the last
    /// thing that happened, never as a toast: a toast expires and the fact that this window paused a
    /// machine and could not restart it does not.
    pub fn sentence_of(&self, deed: Deed) -> String {
        let Deed { doing, occasion } = deed;
        match self {
            Self::AlreadyPaused => format!(
                "This machine was already paused {occasion}, so the window neither paused nor \
                 resumed it."
            ),
            Self::Restored { frames } => {
                let mut s = format!(
                    "This window paused the machine to {doing} and resumed it, because it was \
                     running {occasion}."
                );
                if let Some(n) = frames {
                    s.push_str(&format!(
                        " {} of emulated time passed while it was paused.",
                        plural(*n as usize, "frame", "frames")
                    ));
                }
                s
            }
            Self::Stranded { why } => format!(
                "This window paused the machine to {doing} and the resume was REFUSED, so it \
                 is still paused and nothing here will restart it on its own: {why}"
            ),
            Self::PriorStateUnknown => format!(
                "This window paused the machine to {doing} and could not read back whether it had \
                 been running, so it left the machine paused rather than starting one you may have \
                 stopped on purpose."
            ),
        }
    }

    /// [`RunState::sentence_of`] for the gesture this type was written for.
    pub fn sentence(&self) -> String {
        self.sentence_of(PLACING)
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
        // ⚑ Both deeds, because a second gesture now supplies the noun and the clause: a sweep over one
        // of them would leave the other free to draw a padded or dashed sentence.
        all.extend(
            every_run_state()
                .iter()
                .map(|s| s.sentence_of(super::PREVIEWING)),
        );
        all.push(RunState::HELD_INPUT_HOVER.to_string());
        // ⚑ The subtype picker's strings are swept by the same two rules and in every arm: a rule
        // checked on one path is checked on one path, and this is a second surface with its own
        // sentences rather than more rows of the first one's.
        all.extend(every_subtype_arm());
        // ⚑ And the ring section, which is a third surface with sentences of its own rather than more
        // rows of either list above.
        all.extend(every_ring_arm());
        for s in all {
            assert!(
                !s.contains("  "),
                "a run of spaces is a column drawn inside a string, which is the pseudo-table P2 \
                 outlaws: {s:?}"
            );
            assert!(!s.contains('\t'), "a tab is the same defect: {s:?}");
        }
    }

    // ---------------------------------------------------------------------------------------------
    // The subtype picker
    // ---------------------------------------------------------------------------------------------

    /// The set the subtype rows are projected from.
    ///
    /// ⚑ **Deliberately unlike the live listing in the two dimensions under test.** The real build's eight
    /// spring subtypes run `$00 $02 $10 $12 $20 $22 $50 $52` and none of them exceeds a byte, so a fixture
    /// that copied it would leave the unsendable-value rail dead-but-green and would make a value-ordered
    /// list hard to tell from a name-ordered one. Here the value order (`Up_Red`, `Up_Yellow`,
    /// `Angled_Blue`, `Wide_Huge`) is not the name order, and one value does not fit in a byte.
    fn springs() -> spawn::Subtypes {
        spawn::Subtypes {
            archetype: "ObjDef_Spring".into(),
            prefix: Some("ObjSub_Spring__".into()),
            entries: vec![
                spawn::Subtype {
                    name: "ObjSub_Spring__Up_Red".into(),
                    value: 0x00,
                },
                spawn::Subtype {
                    name: "ObjSub_Spring__Up_Yellow".into(),
                    value: 0x02,
                },
                spawn::Subtype {
                    name: "ObjSub_Spring__Angled_Blue".into(),
                    value: 0x11,
                },
                spawn::Subtype {
                    name: "ObjSub_Spring__Wide_Huge".into(),
                    value: 0x140,
                },
            ],
            truncated: false,
            collisions: Vec::new(),
        }
    }

    /// Every string the subtype picker can draw, over every arm, for the two style sweeps.
    ///
    /// **Assembled from the projection** rather than from a list of literals, so a sentence added later
    /// is swept without anybody remembering to add it here.
    fn every_subtype_arm() -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        // Rows, armed, and both quiet notes.
        v.extend(every_subtype_string(&subtype_listing(
            &springs(),
            Some("ObjSub_Spring__Up_Yellow"),
        )));
        // Both loud notes at once.
        let mut loud = springs();
        loud.truncated = true;
        loud.collisions.push("ObjDef_Spring_".into());
        v.extend(every_subtype_string(&subtype_listing(&loud, None)));
        // The two absences, which are two different sentences.
        v.extend(every_subtype_string(&subtype_listing(
            &spawn::Subtypes::none_for("ObjDef_Ring"),
            None,
        )));
        v.extend(every_subtype_string(&subtype_listing(
            &spawn::Subtypes::none_for("ObjDef_"),
            None,
        )));
        // The armed line's third arm: subtypes exist and none of them can be sent.
        let unsendable = spawn::Subtypes {
            entries: vec![spawn::Subtype {
                name: "ObjSub_Spring__Wide_Huge".into(),
                value: 0x140,
            }],
            ..springs()
        };
        v.extend(every_subtype_string(&subtype_listing(&unsendable, None)));
        v
    }

    /// Every string the ring section can draw, over all three arms.
    fn every_ring_arm() -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for (armed, object) in [
            (true, Some("ObjDef_Spring")),
            (false, Some("ObjDef_Spring")),
            (false, None),
            (true, None),
        ] {
            let l = ring_listing(armed, object);
            v.push(l.armed_line);
            v.extend(l.temporary);
        }
        v
    }

    fn every_subtype_string(l: &SubtypeListing) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for r in &l.rows {
            v.push(r.label.clone());
            v.push(r.value.clone());
            v.extend(r.note.clone());
        }
        v.push(l.count.clone());
        v.push(l.armed.clone());
        v.extend(l.absence.clone());
        v.extend(l.truncation.clone());
        v.extend(l.collision.clone());
        v
    }

    /// **A row reads as the subtype's own name and the listing's own value**, and the one that cannot be
    /// sent is drawn with the reason on it rather than dropped.
    #[test]
    fn a_subtype_row_carries_the_listings_label_and_value_and_says_when_it_cannot_be_sent() {
        let s = springs();
        let l = subtype_listing(&s, Some("ObjSub_Spring__Up_Yellow"));

        assert_eq!(l.rows.len(), 4);
        assert_eq!(l.count, "4 subtypes");
        assert_eq!(
            l.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            ["Up_Red", "Up_Yellow", "Angled_Blue", "Wide_Huge"],
            "the label is the subtype's own half of the name, and the single underscore inside it \
             survives: a picker that split on underscores would offer an object called Up"
        );
        assert_eq!(
            l.rows.iter().map(|r| r.value.as_str()).collect::<Vec<_>>(),
            ["$00", "$02", "$11", "$140"],
            "the value shown is the listing's, in hex, and the oversized one is not cut to a byte"
        );

        // Exactly one row is armed, and it is the one the caller named.
        assert_eq!(l.rows.iter().filter(|r| r.selected).count(), 1);
        assert!(l.rows[1].selected);

        // The unsendable row is drawn, not offered, and says why on itself.
        let huge = &l.rows[3];
        assert!(!huge.offered);
        let note = huge
            .note
            .as_deref()
            .expect("an unofferable row owes a reason");
        assert!(
            note.contains("320") || note.contains(&format!("{}", 0x140)),
            "the reason must quote the value the listing gave: {note:?}"
        );
        assert!(
            l.rows[..3].iter().all(|r| r.offered && r.note.is_none()),
            "every row that fits a byte is offered and carries no excuse"
        );
    }

    /// ⚑ **The armed subtype is stated in words, in all three arms.**
    ///
    /// Standing rather than carried by a fill colour, on the spawn badge's own rule one level up: the
    /// thing that changes what a click **does** says so, because a reader who has scrolled either list is
    /// otherwise inferring it from a highlight they cannot see.
    #[test]
    fn what_a_click_carries_is_stated_in_words_in_every_arm() {
        let s = springs();

        let armed = subtype_listing(&s, Some("ObjSub_Spring__Up_Yellow")).armed;
        assert!(
            armed.contains("Up_Yellow") && armed.contains("$02") && armed.contains("ObjDef_Spring"),
            "the armed line names the archetype, the form and the byte: {armed:?}"
        );

        // An archetype the listing names nothing for: one form, and it says so rather than going quiet.
        let none = subtype_listing(&spawn::Subtypes::none_for("ObjDef_Ring"), None).armed;
        assert!(
            none.contains("ObjDef_Ring") && none.contains("only form"),
            "an archetype with no subtypes still says what a click does: {none:?}"
        );
        assert_ne!(armed, none);

        // The third arm: subtypes exist and not one of them can be sent.
        let unsendable = spawn::Subtypes {
            entries: vec![spawn::Subtype {
                name: "ObjSub_Spring__Wide_Huge".into(),
                value: 0x140,
            }],
            ..springs()
        };
        let line = subtype_listing(&unsendable, None).armed;
        assert!(
            line.contains("No subtype is armed") && line.contains("ObjDef_Spring"),
            "a set with nothing armable must say so rather than reading like an absent list: {line:?}"
        );
    }

    /// **The three lines that are not rows are the model's own sentences**, not second wordings of them.
    ///
    /// The truncation clause is the one that matters: a cut-short subtype list looks exactly like an
    /// object with fewer subtypes, and two accounts of one cut-short search is how the two drift until
    /// one of them stops being true.
    #[test]
    fn the_notes_are_the_models_own_sentences_and_an_empty_set_is_a_line_not_a_box() {
        let quiet = subtype_listing(&springs(), None);
        assert_eq!(quiet.absence, None, "four rows is not an absence");
        assert_eq!(quiet.truncation, None);
        assert_eq!(quiet.collision, None);

        let mut loud = springs();
        loud.truncated = true;
        loud.collisions.push("ObjDef_Spring_".into());
        let l = subtype_listing(&loud, None);
        assert_eq!(
            l.truncation,
            loud.truncation_note(),
            "this must BE the model's sentence, not a copy that can drift from it"
        );
        assert_eq!(l.collision, loud.collision_note());

        // P6: no rows is a stated line, and it is the model's.
        let empty = spawn::Subtypes::none_for("ObjDef_Ring");
        let l = subtype_listing(&empty, None);
        assert!(l.rows.is_empty());
        assert_eq!(l.count, "0 subtypes");
        assert_eq!(
            l.absence,
            empty.absence(),
            "an empty box is never drawn, and the line is the model's"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // The ring section
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **The vanishing rule is on the glass whenever ring placement is armed, and it is the
    /// frontend's own words.**
    ///
    /// A condition of the feature rather than a polish item. Without it a placed ring disappears the
    /// first time the camera moves, with nothing on screen to explain it, and reads as a broken tool
    /// rather than as the engine's design. The `assert_eq!` against the constant is the load-bearing
    /// half: a paraphrase here would be a second account of one design, and the two drift until one of
    /// them stops being true.
    ///
    /// ⚑ **This panel is now the ONLY place the rule is stated whole**, since 2026-09-10. The readout
    /// over the picture used to repeat it on every placement and the arm line used to repeat it again;
    /// both were simultaneously on screen with this panel and with the badge, which is the wall the owner
    /// asked us to cut. So this assertion carries more weight than it did: if it goes, the rule goes.
    #[test]
    fn arming_ring_placement_puts_the_vanishing_rule_on_the_glass_in_the_frontends_words() {
        let on = ring_listing(true, Some("ObjDef_Spring"));
        assert!(on.armed);
        let t = on
            .temporary
            .as_deref()
            .expect("an armed ring mode owes the reader the rule");
        assert_eq!(
            t,
            oracle_frontend::rings::TEMPORARY,
            "this must BE the frontend's sentence, not a copy that can drift from it"
        );
        assert!(
            t.contains("TEMPORARY") && t.contains("camera"),
            "the rule must say what happens and when: {t:?}"
        );

        // Off, there is no ring to be temporary, so the claim is retracted rather than left standing.
        let off = ring_listing(false, Some("ObjDef_Spring"));
        assert!(!off.armed);
        assert_eq!(
            off.temporary, None,
            "a rule about a ring nobody can place is a standing claim about nothing"
        );
    }

    /// **What a click does is stated in words, in all three arms**, and the three do not read alike.
    ///
    /// The disarmed arms are the ones worth a row: *"a click places a spring"* and *"a click arms a
    /// watch"* are different facts, and a toggle whose off state said only "off" would leave a reader
    /// inferring which from a list two panels away.
    #[test]
    fn the_ring_toggle_says_what_a_click_does_in_every_arm() {
        let on = ring_listing(true, Some("ObjDef_Spring")).armed_line;
        let other = ring_listing(false, Some("ObjDef_Spring")).armed_line;
        let watch = ring_listing(false, None).armed_line;

        assert!(on.contains("places a ring"), "{on:?}");
        assert!(
            on.contains("only do one of the two"),
            "the two modes are exclusive and the line must say so rather than leaving a reader to \
             find out by clicking: {on:?}"
        );
        assert!(
            other.contains("ObjDef_Spring") && other.contains("not a ring"),
            "the off state names what a click DOES do: {other:?}"
        );
        assert!(watch.contains("arms a watch"), "{watch:?}");
        assert_ne!(on, other);
        assert_ne!(other, watch);
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
        // ⚑ Both deeds, because a second gesture now supplies the noun and the clause: a sweep over one
        // of them would leave the other free to draw a padded or dashed sentence.
        all.extend(
            every_run_state()
                .iter()
                .map(|s| s.sentence_of(super::PREVIEWING)),
        );
        all.push(RunState::HELD_INPUT_HOVER.to_string());
        // ⚑ The subtype picker's strings are swept by the same two rules and in every arm: a rule
        // checked on one path is checked on one path, and this is a second surface with its own
        // sentences rather than more rows of the first one's.
        all.extend(every_subtype_arm());
        all.extend(every_ring_arm());
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
