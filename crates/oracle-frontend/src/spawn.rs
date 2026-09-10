//! **Spawn mode** — click a spot on the picture and put an object there.
//!
//! ## What this is, and what it deliberately is not
//!
//! The bus half of this landed first: `emulator/object_spawn` / `_move` / `_delete` write aeon's
//! `Obj_Req_*` mailbox and hand back either a placed object or one of the engine's five named refusals
//! (`docs/2026-09-02-cr-spawn-mode.md`, adopted as `protocol.md` §11.32). Until this module there was **no
//! way to reach any of it from the window** — the only `spawn` in this crate was `thread::spawn`.
//!
//! It is **debug/throwaway** by the owner's own scope (`empyrean:contract/projects.json`, `LIVE-OBJECTS`:
//! *"tbh the click to place is just for debug/throwaway, wasn't planned for permanent"*). Nothing here
//! writes back into level placements and nothing survives a reset, a warp or a ROM swap. That half is
//! another lane's and is not quietly half-implemented here.
//!
//! ## Why the click goes through the server and not around it
//!
//! Per-frame panel bodies in this crate read the core directly, in-process — [`crate::pick`] argues that
//! out and D15 backs it. **A per-gesture command is the other case**, and it goes through
//! [`Host::call`](oracle_aether::host::Host::call): synchronous, in-process, no socket
//! (`contract/protocol.md` D15: an in-process GUI *"reads the method registry directly, in-process; it
//! does not open a socket to itself"*).
//!
//! The reason is this module's whole point. §11.32 §6 defines five engine refusals and two server ones,
//! each with a code, a `data.reason` discriminant and **a message written for a person**. A picker that
//! reimplemented the mailbox — or that mapped statuses to its own words — would throw all of that away and
//! replace it with a second opinion. So the window asks the same handler a socket client asks, and prints
//! what it gets back. [`Refusal::terminal`] carries the server's `message` **verbatim**; the only thing
//! this module adds is a *remedy*, keyed on the machine-readable `reason` and never on the message text,
//! because "call `emulator/pause` first" is the right sentence for a socket client and a useless one for
//! somebody holding a keyboard.
//!
//! ## Two things the reply is not allowed to be read as
//!
//! * **Success is not "it is where you clicked."** §11.32's 2026-09-03 addendum rules that `x`/`y` on a
//!   spawn reply are **re-read from the record after `framesAdvanced` frames, not an echo of the accepted
//!   request** — an object with velocity has already moved. So [`Placed::terminal`] names the moment out
//!   loud, exactly as the addendum requires of the wire, rather than printing a bare coordinate the reader
//!   will take for a confirmation. (⚑ Same defect as `ATTR-RGB-LATCH`: if either is reworded, reword both.)
//! * **A refusal is not a no-op.** Every path here ends in a sentence. There is no branch that returns
//!   quietly, and [`Refusal`] has no constructor that can produce an empty message.
//!
//! ## The mode says it is on, for as long as it is on
//!
//! [`Mode::badge`] is a **standing** statement, drawn on every frame the mode is armed (see
//! the frontend's `overlay::Overlay::spawn_badge`, the player's Screen-tab badge). This is a correctness requirement in this crate and not
//! decoration: the layer mask earned the identical rule one file over — *"the person who set it will
//! forget, and then read a masked picture as the machine's"* — and a mode that changes what a left-click
//! **does** is the same hazard with a bigger blast radius. A toast cannot carry it, because toasts expire
//! and the mode does not.
//!
//! It names the archetype rather than merely admitting to a mode, for the reason the layer badge names the
//! layers: *"something is armed" sends you hunting and "SPAWN: ObjDef_Ring" does not.* That is also the
//! adopted rule out of aurora's measured failure — a lens that highlighted 1,244 cells perfectly and drew
//! the reaction *"what are the purple boxes"* — applied here before it can be paid for a second time.

/// The symbol prefix a click's archetype is discovered under.
///
/// **Discovered, never listed here.** §11.32 §9.1 declines to propose an archetype-catalogue row on the
/// grounds that `emulator/lookup_symbol`'s bounded prefix search over `ObjDef_` *already is one*, and this
/// module takes that at its word: the names the window offers come out of the listing that is loaded, so a
/// build with different archetypes offers different archetypes and a build with none refuses to arm. A
/// hard-coded `ObjDef_Ring` would be this crate asserting a fact about somebody else's game.
pub const ARCHETYPE_PREFIX: &str = "ObjDef_";

/// **What a window says when a listing change disarms spawn mode** — one sentence, and now two windows
/// say it (H20).
///
/// The repair it announces is a correctness one and not a courtesy. A [`Mode`]'s archetype list is a set
/// of *names* read out of whichever listing was loaded when it armed, and `emulator/object_spawn
/// {defSymbol}` re-resolves each one at call time. A replaced listing therefore does **not** make a click
/// fail — it makes the click succeed **at a different address**. Of the symbols `s4.lst` and
/// `s4.debug.lst` share, 92.6 % name a different address (`Engine::load_symbols`'s own measurement), so
/// that is the common case rather than a corner.
///
/// It lives here rather than at either call site because the alternative is what H20 actually found: the
/// player had copied the frontend's *signal* for this repair and not the repair, and a second window
/// spelling the sentence for itself would be the same mistake one layer down. Both windows now disarm
/// through their own `disarm` and quote this.
pub const DISARMED_BY_LISTING_CHANGE: &str =
    "spawn mode disarmed: the symbol listing changed, so its archetypes may now name different addresses";

/// The two derived RAM words that carry **the act's true pixel extent**, and the only two this crate will
/// accept as the answer to "is this click inside the level".
///
/// aeon published them for exactly this join, and their own declaration states the box: *"the act's TRUE
/// pixel extent — the valid world box is `[0, Level_Width) × [0, Level_Height)`"*
/// (`games/sonic4/config/ram.emp`). They are written by `Player_BoundsInit` from the values it holds
/// **before** it subtracts its margins, and they exist in both the release and the DEBUG shape.
///
/// ⚠ **`Player_Bound_Right` / `Player_Bound_Bottom` are NOT these**, and reading them as the extent is the
/// dangerous mistake rather than the obvious one. They are the *player's* clamp edges, inset by
/// `PBOUND_RIGHT_MARGIN` and `SCREEN_HEIGHT`; objects are deliberately unclamped, so **an object placed
/// between `Player_Bound_Right` and `Level_Width` is legal and renders**. A window that refused there would
/// reject real placements *and look right doing it*, because a refusal near an edge is half-expected — the
/// failure shaped like correctness, which is precisely why aeon wrote these two words rather than pointing
/// us at the ones a grep finds first. There is no `Player_Bound_Left`/`_Top` at all: the low edge of the
/// box is a literal `0`.
///
/// ⚑ **Resolved by name, per call, never cached** — the same rule §11.26 was amended to impose on
/// `Camera_X`. Measured on this box: `Level_Width` is `$FFFFBABE` in `s4.lst` and `$FFFFE95C` in
/// `s4.debug.lst`, ~11 KB apart, so a cached address is silently wrong in the other shape and yields a
/// number rather than a fault.
pub const LEVEL_WIDTH_SYMBOL: &str = "Level_Width";
/// See [`LEVEL_WIDTH_SYMBOL`].
pub const LEVEL_HEIGHT_SYMBOL: &str = "Level_Height";

/// **The act's pixel extent, as read out of the machine just now.**
///
/// Never a constant. The one act measured on this box reads `$1800 × $1800` (6144, which is aeon's
/// `GRID_W = 3 << SECTION_SIZE_SHIFT = 11`), and that is *one act's value*, not the engine's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// `Level_Width`, in world pixels. The box is half-open: `width` itself is **outside**.
    pub width: u32,
    /// `Level_Height`, in world pixels.
    pub height: u32,
}

impl Bounds {
    /// Whether a world pixel is inside `[0, width) × [0, height)` — aeon's own words for the box.
    ///
    /// Half-open, and that is load-bearing at exactly one pixel per axis: `x == width` is the first column
    /// that is not in the act. Unsigned, so the low edge needs no test — a negative world pixel cannot be
    /// represented, and `Camera_X + dot` cannot produce one.
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height
    }

    /// Whether these are the boot-cleared zeroes rather than a measurement.
    ///
    /// aeon's declaration: both words are *"boot-cleared with all Work RAM, so both read 0 until an act
    /// init has run"*. A `0 × 0` box is therefore **not an act of no size** — it is the absence of an act,
    /// and it gets its own sentence, because "your click is outside the level" is a confusing thing to read
    /// on a title screen.
    pub fn no_act_loaded(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// **The refusal for a click that landed outside the act.**
    ///
    /// It says what the engine would have done, because that is the whole reason this refusal exists: aeon's
    /// `RunObjects` culls an out-of-act object by camera distance and *does nothing* — no error, no refusal,
    /// nothing on screen. Before this check the window acked such a click as placed. A reader who is only
    /// told "refused" will reasonably assume the window is being fussy; a reader who is told the object
    /// would have been silently culled knows the refusal is the useful half.
    pub fn outside(&self, x: u32, y: u32) -> Refusal {
        Refusal::window(
            "outsideAct",
            format!(
                "world ({x}, {y}) is outside this act, whose extent is {} x {} pixels. The valid \
                 box is [0, {}) x [0, {}), read just now from `{LEVEL_WIDTH_SYMBOL}` and \
                 `{LEVEL_HEIGHT_SYMBOL}`. An object placed there is culled by the engine on camera \
                 distance with no error and nothing on screen, so the click is refused here rather than \
                 acked and thrown away",
                self.width, self.height, self.width, self.height
            ),
            Some(format!(
                "click inside the act, which is {} x {} pixels",
                self.width, self.height
            )),
        )
    }

    /// **The refusal for a build whose listing cannot answer where the act ends.**
    ///
    /// The third option, and the one this lane takes: not silently permitting (which is the defect itself,
    /// restored), not silently refusing (the feature works perfectly inside a level and a silent refusal
    /// would read as a broken click), but **saying the check could not be made**. The shipped precedent is
    /// this module's own: no archetypes means [`Mode::arm`] refuses rather than arming a mode that can
    /// place nothing.
    pub fn unmeasurable() -> Refusal {
        Refusal::window(
            "actExtentUnknown",
            format!(
                "the window cannot tell whether this click is inside the act: `{LEVEL_WIDTH_SYMBOL}` and \
                 `{LEVEL_HEIGHT_SYMBOL}` are not both in the loaded listing, so there is no measurement to \
                 check against. An object placed outside the act is culled by the engine with no error and \
                 nothing on screen, and a click sent unchecked would be acked as placed and then \
                 vanish, so this refuses rather than guessing the act is infinite"
            ),
            Some("load a listing that names Level_Width and Level_Height".to_string()),
        )
    }

    /// **The refusal for `0 × 0`** — the boot-cleared reading, which is the absence of an act.
    pub fn no_act() -> Refusal {
        Refusal::window(
            "noActLoaded",
            format!(
                "`{LEVEL_WIDTH_SYMBOL}` and `{LEVEL_HEIGHT_SYMBOL}` read 0, which is what they hold until \
                 an act has initialised. There is no act for an object to be inside of, and anything \
                 placed now would be culled the moment objects run"
            ),
            Some("start an act first, then click".to_string()),
        )
    }
}

/// **What `emulator/lookup_symbol`'s bounded prefix search found**, and how much of the listing it
/// actually looked at.
///
/// `total` is carried because that search is bounded by the engine's `max_symbol_matches` and a truncated
/// answer is a different fact from a complete one: a window that armed over the first 20 of 137
/// archetypes and said "armed" would be quietly deciding, on the reader's behalf, that the other 117 do
/// not exist. `loud-on-unmeasurable`, applied to a partial measurement rather than an absent one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archetypes {
    /// The names, in the order the search returned them.
    pub names: Vec<String>,
    /// How many the listing holds. `names.len()` when nothing was cut.
    pub total: usize,
}

impl Archetypes {
    /// The clause the arm message carries when the search was cut short, or `None` when it was not.
    pub fn truncation_note(&self) -> Option<String> {
        (self.total > self.names.len()).then(|| {
            format!(
                "the listing holds {}; this is the first {}, which is where the bus's bounded \
                 symbol search stops",
                self.total,
                self.names.len()
            )
        })
    }
}

// ---------------------------------------------------------------------------------------------------
// Subtypes — the strengths, colours and directions one archetype comes in
// ---------------------------------------------------------------------------------------------------

/// The namespace an archetype's **subtypes** are published under.
///
/// The owner's ask is *"remember I have to be able to choose subtypes for spawn"*: springs come in
/// strengths and directions, and he wants to pick one before placing it. aeon publishes them as equates
/// in the game's own listing, so this crate **reads** them on [`ARCHETYPE_PREFIX`]'s standing rule rather
/// than typing them, because a table of subtypes written here would be this crate asserting facts about
/// somebody else's game, and it would be wrong the first time that game changed.
///
/// ⚑ **Read with `emulator/lookup_equate`, never `emulator/lookup_symbol`.** The two read **disjoint
/// sections** of the listing, and a symbol query against an equate refuses in a way that reads exactly
/// like the name not existing. That distinction cost the engine lane an hour and this lane a wrong design
/// decision on the same evening; it is written down in `docs/2026-09-06-ring-placement-rules.md`
/// precisely because the channel between the two repos cannot carry it.
pub const SUBTYPE_PREFIX: &str = "ObjSub_";

/// The **double** underscore at the object-to-subtype boundary, and it is a safety property rather than a
/// spelling.
///
/// Single underscores inside a subtype's own name are free: `Up_Red` is one subtype name, not two words
/// to be split on. The picker therefore **prefix-searches and never splits a name on an underscore**, and
/// the doubled boundary is what makes that safe: an object called `Spring_Board` publishes under
/// `ObjSub_Spring_Board__`, which does not begin with `ObjSub_Spring__`, so its subtypes cannot be filed
/// under `Spring`.
///
/// ⚑ [`swallowed_by`] exists anyway, and the engine lane's own argument is why: *"a rule that depends on
/// every future author remembering a convention is not a rule."*
pub const SUBTYPE_BOUNDARY: &str = "__";

/// **The equate prefix `archetype`'s subtypes are published under**, by substitution on its own name:
/// `ObjDef_Spring` becomes `ObjSub_Spring__`.
///
/// `None` when no prefix can be derived, which is exactly two cases and both are stated rather than
/// guessed at: a name that is not an archetype name at all, and the degenerate symbol literally called
/// `ObjDef_` (which [`archetypes`] handles as an exact hit) whose stem is empty and which therefore names
/// no object to hang a namespace on.
pub fn subtype_prefix(archetype: &str) -> Option<String> {
    let stem = archetype.strip_prefix(ARCHETYPE_PREFIX)?;
    if stem.is_empty() {
        return None;
    }
    Some(format!("{SUBTYPE_PREFIX}{stem}{SUBTYPE_BOUNDARY}"))
}

/// **Every other archetype whose subtypes would be swept up by `archetype`'s prefix search**, which is
/// the collision the doubled boundary is meant to prevent and this function refuses to assume it did.
///
/// The test is the mechanism itself rather than a remembered convention: `b`'s subtypes land in `a`'s
/// search exactly when `b`'s own prefix **starts with** `a`'s, so that is what is asked, over the same
/// list the picker is drawing. Stated rather than filtered: this crate can see that two namespaces
/// overlap and cannot know which rows belong to which object, so it names the other archetype and lets
/// the person reading decide, instead of silently filing subtypes under the wrong one.
///
/// ⚑ It catches **two** shapes and not one. `b == a + "_"` is the obvious one (`ObjDef_Spring` against
/// `ObjDef_Spring_`), and `b` starting with `a + "__"` is the other (`ObjDef_Spring` against
/// `ObjDef_Spring__Tall`), which a rule written as "another name plus an underscore" misses. Asking the
/// prefixes directly gets both without enumerating either.
pub fn swallowed_by(archetype: &str, all: &[String]) -> Vec<String> {
    let Some(mine) = subtype_prefix(archetype) else {
        return Vec::new();
    };
    all.iter()
        .filter(|b| b.as_str() != archetype)
        .filter(|b| subtype_prefix(b).is_some_and(|theirs| theirs.starts_with(&mine)))
        .cloned()
        .collect()
}

/// **The largest subtype `emulator/object_spawn` will take**, because it is composed into the low byte of
/// the placement word.
///
/// A listing is free to publish a larger number under the subtype namespace, and this crate does not mask
/// it: the value is carried whole and the row is drawn and **not offered**, with the reason said out loud.
/// Masking would place a different object than the row names, and hiding the row would be this window
/// deciding the listing is wrong.
pub const MAX_SUBTYPE: u64 = 255;

/// **One subtype, exactly as the listing published it.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subtype {
    /// The equate's whole name, in the listing's own spelling.
    pub name: String,
    /// The equate's value, **unmodified**. Not masked, not range-checked, and never an address: the
    /// bus's own ruling on `lookup_equate` is that the value is a plain integer, and this carries it.
    pub value: u64,
}

impl Subtype {
    /// **The byte a placement carries**, or `None` when the listing's value does not fit in one.
    pub fn byte(&self) -> Option<u8> {
        u8::try_from(self.value).ok()
    }

    /// The subtype's own name with the object's half taken off: `ObjSub_Spring__Up_Red` under
    /// `ObjSub_Spring__` reads as `Up_Red`.
    ///
    /// The whole name is what is carried and compared; this is only what a row is **labelled**, and it
    /// falls back to the whole name rather than to an empty label if the prefix does not match.
    pub fn short<'a>(&'a self, prefix: &str) -> &'a str {
        let short = self.name.strip_prefix(prefix).unwrap_or(&self.name);
        if short.is_empty() {
            &self.name
        } else {
            short
        }
    }
}

/// **What one archetype's subtype search found**, and everything about it that is not a row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subtypes {
    /// The archetype these belong to, so a stale set cannot be drawn under a new selection.
    pub archetype: String,
    /// The prefix that was searched, or `None` when none could be derived ([`subtype_prefix`]).
    pub prefix: Option<String>,
    /// The subtypes, **ordered by value**. The value arrives beside the name, so the ordering costs
    /// nothing and it is the order the numbers are in rather than the order the letters are in: the
    /// listing's own map is name ordered, which files a spring's directions alphabetically and puts the
    /// `$00` one wherever its spelling happens to fall.
    ///
    /// ⚑ The sentence here used to name the positions two spellings took **in aeon's build on the
    /// evening this was written**, and that build changed the same night: the side and underside springs
    /// landed, and the set this was measured against went from four rows to eight. Nothing about the
    /// ordering broke, because it is derived from the values that arrive; but the *measurement* in this
    /// comment was stale within hours of being committed, and a doc comment is where a perishable claim
    /// goes to be read by nobody. State the rule, never the peer's current row order.
    pub entries: Vec<Subtype>,
    /// ⚑ **The search was cut short**, straight off the reply's own `truncated`.
    ///
    /// There is no `total` beside it and none is invented: the prefix reply carries `query`, `matches`
    /// and `truncated`, and a count of what was left behind is not among them.
    pub truncated: bool,
    /// Other archetypes whose namespaces overlap this one's ([`swallowed_by`]).
    pub collisions: Vec<String>,
}

impl Subtypes {
    /// The set nothing was read for, so a panel drawn before any search has a shape to hold.
    pub fn none_for(archetype: &str) -> Self {
        Self {
            archetype: archetype.to_string(),
            prefix: subtype_prefix(archetype),
            ..Self::default()
        }
    }

    /// **The subtype a click carries when the person has not chosen one**: the lowest valued one that
    /// fits in a byte, or `None` when there is nothing to arm.
    ///
    /// A default rather than an unchosen state, and that is the safer of the two. `emulator/object_spawn`
    /// composes a subtype byte whether or not one is sent, so an "unchosen" picker would be arming a
    /// subtype silently while claiming it had armed none. Arming the lowest and **naming it** places
    /// exactly what a click placed before this list existed, and says which one that is.
    pub fn default_choice(&self) -> Option<&Subtype> {
        self.entries.iter().find(|s| s.byte().is_some())
    }

    /// The subtype with this name, or `None` when this set is not holding it.
    ///
    /// By name and not by row, for [`Mode::select`]'s reason: the row a person clicks is a row of
    /// whatever is drawn, and a set that has been re-read under a new listing must not silently hand back
    /// whichever subtype took that position.
    pub fn get(&self, name: &str) -> Option<&Subtype> {
        self.entries.iter().find(|s| s.name == name)
    }

    /// ⚑ **The line a truncated list owes the reader, in words that say what is wrong with it.**
    ///
    /// This is the one absence with no visible symptom. A cut-short subtype list **does not look cut
    /// short**: it looks like an object with fewer subtypes, and the reader picks from three when there
    /// are twelve with nothing on the glass to argue with. The cap is high enough that this should not
    /// happen, which is a reason to say it loudly when it does rather than a reason to leave it unsaid.
    pub fn truncation_note(&self) -> Option<String> {
        self.truncated.then(|| {
            format!(
                "This list is CUT SHORT. {} names more subtypes for {} than the search would return, \
                 and the ones missing are not marked because a shortened list looks exactly like an \
                 object with fewer subtypes. Do not read the rows below as all of them.",
                self.prefix.as_deref().unwrap_or("this build"),
                self.archetype,
            )
        })
    }

    /// The line an overlapping namespace owes the reader, or `None` when nothing overlaps.
    pub fn collision_note(&self) -> Option<String> {
        if self.collisions.is_empty() {
            return None;
        }
        Some(format!(
            "NAME CLASH: {} publishes subtypes under a name that also begins with {}, so rows below \
             may belong to it rather than to {}. Nothing has been hidden or reassigned, because this \
             window cannot tell which object a clashing name was written for.",
            self.collisions.join(", "),
            self.prefix.as_deref().unwrap_or(&self.archetype),
            self.archetype,
        ))
    }

    /// **The stated line drawn instead of rows**, or `None` when there are rows to draw.
    ///
    /// P6, and the two reasons there are no rows are two different findings that must not read alike: a
    /// name this crate cannot build a namespace out of, against a listing that publishes nothing under a
    /// namespace it could. Neither is a refusal and neither is an empty box.
    pub fn absence(&self) -> Option<String> {
        if !self.entries.is_empty() {
            return None;
        }
        Some(match &self.prefix {
            Some(p) => format!(
                "This build's listing names no subtypes under {p}, so {} has one form and a click \
                 places it. Subtypes are read from the listing, so a build that adds some will show \
                 them here without anything changing in this window.",
                self.archetype,
            ),
            None => format!(
                "{} carries no object name after {ARCHETYPE_PREFIX}, so there is no name to look its \
                 subtypes up under and none were searched for.",
                self.archetype,
            ),
        })
    }
}

/// **The subtypes `archetype` offers in the listing loaded right now**, discovered rather than listed.
///
/// One call, on the equate door. `all` is the archetype list the picker is already holding, and it is
/// passed rather than re-read so the clash check is made against the same names the person is looking at.
///
/// ⚑ **The reply's shape is `{query, matches, truncated}` and there is no `total` in it.** A count of
/// what a cut-short search left behind is a fact the equate door does not publish, so
/// [`Subtypes::truncation_note`] says the list is short without claiming a number, and nothing here
/// reaches for `total` and quietly gets a zero.
///
/// An empty `matches` is an **answer**, not a refusal: the equate door was built that way on purpose, so
/// an archetype with no subtypes reaches [`Subtypes::absence`] and gets a sentence.
#[cfg(feature = "aether")]
pub fn subtypes(c: &mut impl Caller, archetype: &str, all: &[String]) -> Result<Subtypes, Refusal> {
    let collisions = swallowed_by(archetype, all);
    let Some(prefix) = subtype_prefix(archetype) else {
        return Ok(Subtypes {
            collisions,
            ..Subtypes::none_for(archetype)
        });
    };
    let v = c.call(
        "emulator/lookup_equate",
        serde_json::json!({"prefix": prefix}),
    )?;
    let mut entries: Vec<Subtype> = v["matches"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(Subtype {
                        name: m["name"].as_str()?.to_string(),
                        value: m["value"].as_u64()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // Ordered by value, with the name breaking a tie so the order is total and a redraw cannot shuffle
    // two equal-valued rows past each other.
    entries.sort_by(|a, b| a.value.cmp(&b.value).then_with(|| a.name.cmp(&b.name)));
    Ok(Subtypes {
        archetype: archetype.to_string(),
        prefix: Some(prefix),
        entries,
        truncated: v["truncated"] == serde_json::json!(true),
        collisions,
    })
}

/// **A refusal, on its way to a person.** Never a silent failure and never rendered as a success.
///
/// `code` and `reason` are the server's own discriminants (§11.32 §6). They are carried rather than
/// consumed because the *machine-readable* half is what [`Self::remedy`] may branch on — branching on
/// `message` text would be this crate parsing prose it does not own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The JSON-RPC error code, or `None` when the refusal is the **window's own** — a precondition the
    /// server never got the chance to fail. Both kinds exist and a reader is told which this is.
    pub code: Option<i64>,
    /// `error.data.reason`, where the server sent one.
    pub reason: Option<String>,
    /// **The server's `message`, verbatim.** The one field nothing in this module is allowed to reword.
    pub message: String,
    /// The remedy a **window-side** refusal carries with it, or `None` to fall back to
    /// [`Self::remedy`]'s reason-keyed table.
    ///
    /// It is a field rather than another arm of that `match` for a reason that is not tidiness: the
    /// remedies for these refusals **quote a measurement** — *"click inside the act — it is 1024 x 768
    /// pixels"* — and a `match` on `reason` alone has no access to the numbers that were read. Keying it
    /// off the constructor is *stricter* than keying it off `reason`, not looser: nothing here branches on
    /// message text, which is the rule the reason-keyed table exists to hold.
    ///
    /// **`pub` as of the S0 lib target**, and the widening is deliberate rather than incidental. It was
    /// `pub(crate)` so that the frontend's `bus` module could build the server-side refusal as a literal
    /// and say `None` here while nothing outside the crate could set a remedy. This module now lives in
    /// `oracle-frontend`'s lib and its second consumer — `oracle-player`'s Screen tab — is a window in
    /// exactly the same sense, with its own keys to name. The rule the old visibility encoded still
    /// stands and is now a rule rather than a type: **a remedy is a statement in the holding window's
    /// vocabulary about the holding window's keys**, so a consumer that has no such key writes `None`.
    pub remedy: Option<String>,
}

impl Refusal {
    /// A refusal the **window** raised, before or instead of a call. Coded `None` so the terminal line can
    /// say whose refusal it is rather than inventing an RPC code for something that never went to RPC.
    pub fn local(message: impl Into<String>) -> Self {
        Self {
            code: None,
            reason: None,
            message: message.into(),
            remedy: None,
        }
    }

    /// A window-side refusal that carries **its own machine-readable discriminant and its own remedy**.
    ///
    /// The `reason` is not decoration: `Refusal::local`'s refusals are all one undifferentiated `None`
    /// today, and the three the act-bounds check raises are three different facts a caller may want to tell
    /// apart (*the listing cannot answer*, *no act is loaded*, *you clicked outside it*). Giving them
    /// discriminants is the same move §11.32 §6 makes on the wire, applied to the refusals we own — and
    /// `code` stays `None`, so [`Self::terminal`] still says "the window" rather than claiming an RPC code
    /// for a call that never happened.
    pub fn window(
        reason: impl Into<String>,
        message: impl Into<String>,
        remedy: Option<String>,
    ) -> Self {
        Self {
            code: None,
            reason: Some(reason.into()),
            message: message.into(),
            remedy,
        }
    }

    /// **What the window can do about this refusal**, in the window's own vocabulary, or `None`.
    ///
    /// Keyed on `reason` — the machine-readable discriminant §11.32 §6 defines for exactly this purpose —
    /// and never on the message. `pause_key` comes from the command registry (see `main.rs`), so the key
    /// named here is the key that is actually bound; a transcribed "Space" would go stale the moment
    /// somebody rebound it.
    ///
    /// Only `machineRunning` has one today, and that is the honest list rather than a thin one: the other
    /// four engine refusals and the two server refusals are answered by the machine's state or by choosing
    /// a different archetype, neither of which is a keystroke this window owns.
    pub fn remedy(&self, pause_key: Option<&str>) -> Option<String> {
        // A refusal the window built already knows its own next action, including the numbers it measured
        // to reach it. Nothing below can reconstruct those.
        if self.remedy.is_some() {
            return self.remedy.clone();
        }
        match (self.reason.as_deref(), pause_key) {
            (Some("machineRunning"), Some(k)) => Some(format!(
                "press {k} to pause this window, then click the spot again"
            )),
            _ => None,
        }
    }

    /// The full line for the terminal: whose refusal, its discriminants, and **the server's own words**.
    ///
    /// `archetype` is named because a refusal with no subject reads as "something went wrong" — the
    /// failure this module's own docs open with.
    ///
    /// ⚑ **This one is user-facing in `oracle-player` too**, which is why its two em dashes went and
    /// [`Self::toast`]'s did not: the player's pick readout shows this line verbatim when a spawn is
    /// refused, so it is inside the panel parcel that applied the owner's 2026-09-05 no-dash ruling.
    /// The toast is drawn only by this crate's own window and is left for that lane's own pass, because
    /// a rule applied where nobody asked for it is a rule applied to somebody else's tests.
    pub fn terminal(&self, archetype: &str, pause_key: Option<&str>) -> String {
        let who = match (self.code, self.reason.as_deref()) {
            (Some(c), Some(r)) => format!("aether {c} {r}"),
            (Some(c), None) => format!("aether {c}"),
            (None, _) => "the window".to_string(),
        };
        let mut s = format!(
            "spawn refused by {who}, nothing was placed, {archetype} is not on screen: {}",
            self.message
        );
        if let Some(r) = self.remedy(pause_key) {
            s.push_str(&format!(". {r}"));
        }
        s
    }

    /// The short form for the toast. The remedy wins the glass when there is one, because a person looking
    /// at the window wants the next action; the server's exact words are two feet away in the terminal, on
    /// the line [`Self::terminal`] just printed. Same split [`crate::pick`] already uses.
    pub fn toast(&self, pause_key: Option<&str>) -> String {
        let body = self
            .remedy(pause_key)
            .unwrap_or_else(|| self.message.clone());
        format!("SPAWN REFUSED: {body}")
    }
}

/// **A placed object, as the server described it after the frame advance.**
///
/// The fields are the reply's, renamed to this crate's spelling and nothing else. `x`/`y` are the
/// **re-read**, not the request: see [`Self::terminal`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    /// `handle` — the low word of the object's SST address, as a hex string.
    pub handle: String,
    /// `addr` — the full SST address, as a hex string.
    pub addr: String,
    /// `slot` — the pool index, **present iff the server's layout resolved it**. Never fabricated here for
    /// the same reason it is never fabricated there.
    pub slot: Option<i64>,
    /// The world position the window **asked for**, from the click.
    pub asked: (u32, u32),
    /// `x`/`y` — the record's position **now**, re-read after `frames_advanced`.
    pub now: (i64, i64),
    /// `framesAdvanced`.
    pub frames_advanced: u64,
    /// `caveat`, where the server sent one (the §7.2 mid-frame window, or a slot that went inactive).
    pub caveat: Option<String>,
}

impl Placed {
    /// The full line for the terminal.
    ///
    /// **It names the moment `now` is for.** §11.32's 2026-09-03 addendum rules the spawn reply's `x`/`y`
    /// to be *"as read from the object's record after `framesAdvanced` frames, not an echo of the accepted
    /// request"*, and requires the description to say so — because an unqualified coordinate after an
    /// advance is a plausible wrong answer that a reader takes for a placement confirmation. A window that
    /// printed the pair bare would reintroduce, on the glass, exactly the defect the wire fixed.
    pub fn terminal(&self, archetype: &str) -> String {
        let slot = match self.slot {
            Some(s) => format!("slot {s}"),
            // Omitted upstream when the layout cannot supply it, so it is named as absent rather than
            // guessed — the ⚙ group's rule (3), carried through to the sentence.
            None => "a slot this build's layout cannot name".to_string(),
        };
        let mut s = format!(
            "spawned {archetype} at world ({}, {}): it is {slot}, handle {}, addr {}. \
             Its record reads ({}, {}) after {} frame(s), which is where the object is NOW and not a \
             confirmation of where you clicked (anything with velocity has already moved).",
            self.asked.0,
            self.asked.1,
            self.handle,
            self.addr,
            self.now.0,
            self.now.1,
            self.frames_advanced,
        );
        if let Some(c) = &self.caveat {
            s.push_str(&format!(" Caveat: {c}"));
        }
        s
    }

    /// The short form for the toast. `NOW AT` rather than `AT`, for [`Self::terminal`]'s reason in the
    /// space a toast has.
    pub fn toast(&self, archetype: &str) -> String {
        match self.slot {
            Some(s) => format!(
                "SPAWNED {archetype} SLOT {s} NOW AT ({}, {})",
                self.now.0, self.now.1
            ),
            None => format!(
                "SPAWNED {archetype} {} NOW AT ({}, {})",
                self.handle, self.now.0, self.now.1
            ),
        }
    }
}

/// **Whether a left-click places an object, and which one.**
///
/// Disarmed by default, and disarmed by **every event that can replace the listing**, because the
/// archetype list was read out of a listing that may no longer describe the machine — and a stale
/// archetype address is precisely the silent-corruption shape §11.32 §8 exists to refuse.
///
/// ⚑ **This type cannot enforce that on its own, so the sites are named here rather than asserted.**
/// `Mode` holds names, not the generation they were read against; disarming is therefore something each
/// listing-replacing path must *do*, and the exhaustive list of them is:
///
/// | site | event |
/// |---|---|
/// | `main.rs`, `Cmd::ToggleSpawnMode` | the person turns it off |
/// | `drain.rs`, the `symbols` drain | a hosted client called `emulator/load_symbols` over the bus |
/// | `main.rs`, the `pending_rom` swap block | F5 reload / open — the window replaced its own cartridge |
///
/// The third row was missing until the lens sweep found this doc claiming it (finding H21). The gap was
/// not an oversight of the drain's: the engine cannot cover it, because *"a window that swaps its own
/// cartridge (the frontend's F5) therefore does not get told about its own listing"*
/// (`Host::pump`'s `symbols_generation` snapshot, in `oracle-aether`). A `reset` alone (Tab / F1) does
/// **not** disarm and does not need to — it re-runs the vector fetch and keeps the cartridge, so the
/// listing still describes the machine.
///
/// **The shape that would make this a property rather than a checklist** — derive armed-ness from the
/// listing generation it was armed against, so no swap path can forget — is the right one and is not built
/// here; it needs a generation counter the frontend does not yet carry. Until then a fourth
/// listing-replacing path added without a disarm reintroduces the defect silently, which is why the table
/// above is exhaustive rather than illustrative.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mode {
    /// The archetypes discovered at arm time. Empty **iff** disarmed: arming with nothing to place is
    /// refused rather than armed-and-useless, so `armed == !archetypes.is_empty()` is an invariant rather
    /// than a coincidence and there is no second flag that could disagree with it.
    archetypes: Vec<String>,
    index: usize,
}

impl Mode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a click places an object right now.
    pub fn is_armed(&self) -> bool {
        !self.archetypes.is_empty()
    }

    /// The archetype a click would place, or `None` when disarmed.
    pub fn selected(&self) -> Option<&str> {
        self.archetypes.get(self.index).map(String::as_str)
    }

    /// **The standing statement**, or `None` when the mode is off. See the module docs for why this is a
    /// correctness requirement rather than polish.
    ///
    /// It carries the position in the list because the cycle key is otherwise a mystery — `2/9` is what
    /// makes "there are more of these" visible without a second surface.
    pub fn badge(&self) -> Option<String> {
        let name = self.selected()?;
        Some(format!(
            "SPAWN: {name} ({}/{})",
            self.index + 1,
            self.archetypes.len()
        ))
    }

    /// Arm the mode over `archetypes`.
    ///
    /// **An empty list is a refusal, not an empty mode.** A mode that is on and can place nothing is the
    /// worse of the two failures: the badge would claim a click does something it cannot do, which is the
    /// "works perfectly, communicates nothing" shape one step past. The refusal names the prefix so the
    /// reader can tell "this build has no archetypes" from "I forgot to load symbols" — the same
    /// distinction `-32012` vs `-32013` draws on the wire.
    pub fn arm(&mut self, archetypes: Vec<String>) -> Result<&str, Refusal> {
        if archetypes.is_empty() {
            return Err(Refusal::local(format!(
                "this build's symbol listing names no `{ARCHETYPE_PREFIX}` archetype, so there is \
                 nothing a click could place. Spawn mode is left off"
            )));
        }
        self.archetypes = archetypes;
        self.index = 0;
        Ok(self.archetypes[0].as_str())
    }

    /// Turn the mode off. Idempotent; a click goes back to arming a watch.
    pub fn disarm(&mut self) {
        self.archetypes.clear();
        self.index = 0;
    }

    /// Select the next archetype, wrapping. `None` when the mode is off — which the caller reports rather
    /// than swallowing, because a key that silently does nothing is indistinguishable from a broken one.
    pub fn cycle(&mut self) -> Option<&str> {
        if self.archetypes.is_empty() {
            return None;
        }
        self.index = (self.index + 1) % self.archetypes.len();
        self.selected()
    }

    /// **Every archetype this mode is holding**, in the order the search returned them.
    ///
    /// Added for the window that offers a *choice* rather than a cycle key: a picker has to draw the
    /// list, and it draws **this** list rather than keeping one of its own, so the rows a person reads
    /// and the `n/m` in [`Mode::badge`] cannot be counting two different things.
    pub fn names(&self) -> &[String] {
        &self.archetypes
    }

    /// **Select `name`**, or report that this mode is not holding it.
    ///
    /// By name and not by index, because the caller drawing the list may be drawing a *filtered* view of
    /// it whose row numbers are not this mode's. `None` is a real answer that a caller reports rather
    /// than swallows, for [`Mode::cycle`]'s reason and one more of its own: an archetype that has left
    /// the listing under a `reload_rom` or a `load_symbols` must not silently select whichever name took
    /// its position, which is the stale-archetype hazard this whole module is written against.
    pub fn select(&mut self, name: &str) -> Option<&str> {
        let i = self.archetypes.iter().position(|a| a == name)?;
        self.index = i;
        self.selected()
    }
}

// ---------------------------------------------------------------------------------------------------
// The choreography — one implementation, two windows
// ---------------------------------------------------------------------------------------------------
//
// ⚑ **Why this is here rather than in either window's `bus` module.** It was in
// `oracle-frontend/src/bus.rs` (`Bus::archetypes` / `act_bounds` / `spawn_at`) until the migration's S1,
// which is when a *second* window needed it. Copying it would have put two implementations of the act-bounds
// gate and the world join on disk, and the migration's own standing lesson is the one CR-K banked:
// `reload_rom` was deliberately re-pointed at one implementation so *"the two cannot answer one client
// differently about one file in one millisecond."* The same argument applies with more force here, because
// this choreography is what stands between a click and an **acked-then-silently-culled** spawn.
//
// The seam is [`Caller`]: one method that dispatches a served method synchronously and gives back the
// tool's own reply or the tool's own refusal, and one that resolves a symbol. Both windows hold a `Host`
// and can supply both; neither window's `Bus` type is nameable from here, and it does not need to be.

/// What the choreography below needs from whichever window is holding the click.
///
/// **Deliberately two methods and no more.** A trait that took the window's `Bus` would tie this to one
/// crate; a trait that took a `Host` would tie it to `oracle-aether`'s exact hosting arrangement, which the
/// two windows do not share (`oracle-frontend` pumps, `oracle-player` does not). What is genuinely common is
/// "dispatch one method and hand me the answer" and "resolve one symbol", and that is the whole surface.
///
/// `call` returns `Result<Value, Refusal>` rather than the raw `RpcError` for the reason
/// [`Refusal::terminal`] exists: the server's `message` is carried **verbatim** and the *remedy* is the
/// holding window's to add, keyed on `reason`. An implementor builds the `Refusal` with `code: Some(..)`,
/// the reply's `reason`, the server's `message`, and `remedy: None`.
#[cfg(feature = "aether")]
pub trait Caller {
    /// Dispatch one served method synchronously, in-process. No socket — `protocol.md` D15.
    fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Refusal>;

    /// The 68000 address of `symbol` in the listing the machine is running with, **resolved fresh every
    /// time**. See [`act_bounds`] for why a cache here is a correctness bug rather than an optimisation.
    fn address_of(&mut self, symbol: &str) -> Option<u32>;
}

/// **The archetypes this build offers**, discovered under [`ARCHETYPE_PREFIX`] rather than listed.
///
/// The list is read at the moment the mode is armed and the mode is disarmed whenever the machine's listing
/// changes, because a stale archetype name is a spawn of the wrong thing rather than a failed spawn.
///
/// Every failure comes back as the server's own words (`-32012` *you forgot to load symbols* against
/// `-32013` *this build has no such name* is exactly the distinction a person hits here, and §8.2 keeps them
/// apart on purpose).
#[cfg(feature = "aether")]
pub fn archetypes(c: &mut impl Caller) -> Result<Archetypes, Refusal> {
    let v = c.call(
        "emulator/lookup_symbol",
        serde_json::json!({"name": ARCHETYPE_PREFIX}),
    )?;
    // The exact branch: a symbol literally named `ObjDef_`. Vanishingly unlikely and handled anyway,
    // because the alternative is an empty list from a reply that found something.
    if v["exact"] == serde_json::json!(true) {
        let name = v["name"].as_str().unwrap_or_default().to_string();
        return Ok(Archetypes {
            total: 1,
            names: vec![name],
        });
    }
    let page = &v["otherMatches"];
    let names: Vec<String> = page["items"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    // `total` from the envelope, not from `names.len()` — the two differ exactly when the search was cut,
    // which is the case the note exists for.
    let total = page["total"].as_u64().unwrap_or(names.len() as u64) as usize;
    Ok(Archetypes { names, total })
}

/// **The act's pixel extent, read out of the machine right now.**
///
/// ⚑ **Both addresses are resolved BY NAME on every call and never cached**, which is the rule §11.26 was
/// amended to impose on `Camera_X` after it was found to *move between build shapes*. These move too, and
/// further: measured on this box, `Level_Width` is `$FFFFBABE` in `s4.lst` and `$FFFFE95C` in
/// `s4.debug.lst`. A cached address does not fault in the other shape — it returns a number, and a number is
/// what this check compares against.
///
/// The two symbols are resolved **independently**, not as one 4-byte read off the first. They are adjacent
/// in every listing seen so far and that is a fact about a declaration order this crate does not own; an
/// implementation that assumed it would keep working right up until aeon inserted a field.
///
/// Every failure is the **window's own** refusal (`code: None`), matching the sibling case in [`place`]: a
/// build without `Camera_X` already gets a local sentence rather than a coordinate, and a build without
/// `Level_Width` is the same shape of unmeasurable.
#[cfg(feature = "aether")]
pub fn act_bounds(c: &mut impl Caller) -> Result<Bounds, Refusal> {
    let addrs = c
        .address_of(LEVEL_WIDTH_SYMBOL)
        .zip(c.address_of(LEVEL_HEIGHT_SYMBOL));
    // Both or neither. Half an extent is not a smaller measurement, it is no measurement — and the half
    // that resolved would be the more dangerous of the two, because a check on one axis looks like a check.
    let (wa, ha) = match addrs {
        Some(p) => p,
        None => return Err(Bounds::unmeasurable()),
    };
    Ok(Bounds {
        width: u32::from(read_u16(c, wa)?),
        height: u32::from(read_u16(c, ha)?),
    })
}

/// One word out of work RAM, through the same handler a socket client reads with.
#[cfg(feature = "aether")]
fn read_u16(c: &mut impl Caller, addr: u32) -> Result<u16, Refusal> {
    let r = c.call(
        "emulator/read_memory",
        serde_json::json!({"addr": format!("0x{addr:08X}"), "len": 2}),
    )?;
    // `bytes` is the reply's `0xXXXX`. A reply that cannot be parsed is refused rather than defaulted to
    // zero: a silent `0` here would read as "no act loaded" and send the person hunting for an act that is
    // already running.
    r["bytes"]
        .as_str()
        .and_then(|s| u16::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16).ok())
        .ok_or_else(|| {
            Refusal::local(format!(
                "the window could not read the act extent at {addr:#010X}: \
                 emulator/read_memory answered {:?}, which is not a word",
                r["bytes"]
            ))
        })
}

/// **Place `archetype` where the window was clicked.**
///
/// Three calls, because the click is in *screen* dots, the mailbox wants *world* pixels, and the act has an
/// edge that the mailbox will not defend:
///
/// 1. `emulator/object_at`, whose `world{x,y}` is `Camera_X`/`Camera_Y` plus the dot (§11.26 M3, and the
///    join §11.32 §11 names as the GUI's one extra dependency). It is a pure read and needs no pause.
/// 2. [`act_bounds`], which reads `Level_Width`/`Level_Height` by name and is what makes a click outside
///    the level a **sentence** instead of an ack followed by a silent cull (`F-SPAWN-OUTSIDE-ACT`).
/// 3. `emulator/object_spawn { defSymbol, x, y }`, which is where the pause requirement, the mailbox
///    handshake and all five engine refusals live.
///
/// **The `world` half is refused rather than guessed.** §11.26 makes `worldSource` a field precisely so its
/// absence is not inferred from a missing `world`, and a build without the camera symbols gets a sentence
/// instead of a coordinate — a spawn at the raw dot would land somewhere plausible and wrong, which is the
/// failure class this whole row is written against.
///
/// **UNMEASURED, and named rather than assumed** (§11.32 §11's own flag): that `object_at`'s world space is
/// the same flat world-pixel space `Obj_Req_X`/`Y` want. Aeon states it is *"the same convention as
/// `Warp_Req_X/Y`"*; nothing in this repo has confirmed the two agree against a running game, and if they do
/// not, this needs a conversion that no CR has specified.
/// **`subtype` is the byte the listing gave**, or `None` for a placement that names none.
///
/// It is a parameter rather than something derived here, and the derivation it replaces is the one that
/// must never exist: a subtype worked out from a name would be this crate inventing a number for somebody
/// else's game. What travels is [`Subtype::byte`] of an entry [`subtypes`] read out of the listing, and
/// nothing else can reach this argument.
/// **The world pixel a screen dot names**, through `emulator/object_at`, or the window's own refusal.
///
/// ⚑ **Extracted so that everything this window can place shares ONE world join.** Ring placement
/// ([`crate::rings`]) needs the identical answer for the identical click, and the standing lesson CR-K
/// banked is that a second implementation lets the two *"answer one client differently about one file in
/// one millisecond"*. There is one, and both call it.
///
/// **The `world` half is refused rather than guessed.** §11.26 makes `worldSource` a field precisely so
/// its absence is not inferred from a missing `world`, and a build without the camera symbols gets a
/// sentence instead of a coordinate: a placement at the raw dot would land somewhere plausible and wrong,
/// which is the failure class this whole module is written against.
#[cfg(feature = "aether")]
pub fn world_at(c: &mut impl Caller, dot: (u16, u16)) -> Result<(u32, u32), Refusal> {
    let (dx, dy) = dot;
    let at = c.call("emulator/object_at", serde_json::json!({"x": dx, "y": dy}))?;
    let source = at["worldSource"].as_str().unwrap_or("unavailable");
    match (source, at["world"]["x"].as_u64(), at["world"]["y"].as_u64()) {
        ("camera", Some(x), Some(y)) => Ok((x as u32, y as u32)),
        _ => Err(Refusal::local(format!(
            "this build cannot turn a click into a world position (object_at answered \
             worldSource={source:?}): `Camera_X` and `Camera_Y` are not both in the loaded \
             listing, and spawning at the raw screen dot ({dx},{dy}) would place the object \
             somewhere plausible and wrong"
        ))),
    }
}

/// **The act-bounds gate**, as one call: measured, then checked, with each of the three failures its own
/// sentence.
///
/// Extracted alongside [`world_at`] and for its reason. It is the same gate [`place`] has always run, and
/// [`crate::rings`] runs it too, because a ring placed outside the act is culled by the same engine on the
/// same camera distance with the same silence.
#[cfg(feature = "aether")]
pub fn in_act(c: &mut impl Caller, world: (u32, u32)) -> Result<Bounds, Refusal> {
    let bounds = act_bounds(c)?;
    if bounds.no_act_loaded() {
        return Err(Bounds::no_act());
    }
    if !bounds.contains(world.0, world.1) {
        return Err(bounds.outside(world.0, world.1));
    }
    Ok(bounds)
}

#[cfg(feature = "aether")]
pub fn place(
    c: &mut impl Caller,
    archetype: &str,
    subtype: Option<u8>,
    dot: (u16, u16),
) -> Result<Placed, Refusal> {
    let world = world_at(c, dot)?;
    // --- THE ACT-BOUNDS GATE (`F-SPAWN-OUTSIDE-ACT`) --------------------------------------------
    //
    // A click outside the level used to be **acked as placed and then silently culled**: aeon's
    // `RunObjects` drops an out-of-act object on camera distance and does nothing — no error, no refusal,
    // nothing on screen. That is the exact failure class this whole module is written against, arriving
    // through the one path that returns success, so it is caught here, on the side that holds the click.
    //
    // **It is ours and it is window-side on purpose.** aeon deliberately added no clamp and no refusal to
    // the mailbox rather than pre-empt this design by making the engine quietly do half of it, so
    // `emulator/object_spawn` will accept this request without complaint. Nothing below this line is
    // allowed to be the thing that stops it.
    //
    // **Refused, not clamped.** The booking allowed either. A clamp moves the object away from where the
    // person clicked and then reports success with coordinates — which, given §11.32's ruling that the
    // reply's `x`/`y` are a *re-read*, would print a perfectly plausible line about an object sitting at the
    // level edge for reasons nothing on the glass explains. That is a smaller lie of the same family as the
    // defect, and there is no rule for which edge to snap to that preserves what the click meant. A refusal
    // costs one gesture and says why.
    //
    // **Ordering.** This sits after the world join and before the mailbox, so an unresolvable click is
    // still refused for *that* reason first. It does run before the server's `paused` precondition, so an
    // out-of-act click on a running machine reads "outside the act" rather than "press Space" — both true,
    // and the one the person can act on without pausing first.
    in_act(c, world)?;

    let mut req = serde_json::json!({"defSymbol": archetype, "x": world.0, "y": world.1});
    // Sent only when one was chosen, so a build with no subtypes puts no key on the wire rather than
    // asserting a zero it never read. The value is the listing's own byte, carried, never computed.
    if let Some(s) = subtype {
        req["subtype"] = serde_json::json!(s);
    }
    let placed = c.call("emulator/object_spawn", req)?;
    Ok(Placed {
        handle: placed["handle"].as_str().unwrap_or_default().to_string(),
        addr: placed["addr"].as_str().unwrap_or_default().to_string(),
        slot: placed["slot"].as_i64(),
        asked: world,
        now: (
            placed["x"].as_i64().unwrap_or_default(),
            placed["y"].as_i64().unwrap_or_default(),
        ),
        frames_advanced: placed["framesAdvanced"].as_u64().unwrap_or_default(),
        caveat: placed["caveat"].as_str().map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------------------------
    // The mode, and its standing statement
    // ---------------------------------------------------------------------------------------------

    /// **The badge exists exactly while the mode does, and it names the archetype.**
    ///
    /// The `None`-when-off half is as load-bearing as the `Some`-when-on half: a badge that stood on a
    /// disarmed window would be a permanent claim that a click places something, which is the mask
    /// badge's defect inverted and just as unreadable.
    #[test]
    fn the_badge_stands_iff_the_mode_is_armed_and_names_what_a_click_places() {
        let mut m = Mode::new();
        assert_eq!(m.badge(), None, "a disarmed mode must claim nothing");
        assert!(!m.is_armed());

        m.arm(vec!["ObjDef_Ring".into(), "ObjDef_Spring".into()])
            .expect("two archetypes are enough to arm");
        let badge = m.badge().expect("an armed mode must say so");
        assert!(
            badge.contains("ObjDef_Ring"),
            "the badge must name the subject, not merely admit to a mode: {badge:?}"
        );
        assert!(
            badge.contains("1/2"),
            "the badge must show there are others to cycle to: {badge:?}"
        );

        m.disarm();
        assert_eq!(m.badge(), None, "disarming must retract the claim");
    }

    /// Cycling wraps, and reports nothing to cycle when the mode is off.
    #[test]
    fn cycling_wraps_and_refuses_to_pretend_when_disarmed() {
        let mut m = Mode::new();
        assert_eq!(m.cycle(), None, "nothing to cycle through when off");

        m.arm(vec!["ObjDef_A".into(), "ObjDef_B".into()]).unwrap();
        assert_eq!(m.selected(), Some("ObjDef_A"));
        assert_eq!(m.cycle(), Some("ObjDef_B"));
        assert_eq!(m.cycle(), Some("ObjDef_A"), "the cycle wraps");
    }

    /// **Selecting by name moves the badge, and a name the mode is not holding selects nothing.**
    ///
    /// The second half is the one worth a gate. A `select` that fell back to an index, or that silently
    /// did nothing while the badge went on naming the old archetype, would place something other than
    /// the row that was clicked, which is the stale-archetype failure wearing a picker's clothes.
    #[test]
    fn selecting_by_name_moves_the_badge_and_a_name_it_does_not_hold_selects_nothing() {
        let mut m = Mode::new();
        assert_eq!(m.select("ObjDef_A"), None, "a disarmed mode holds no name");

        m.arm(vec![
            "ObjDef_A".into(),
            "ObjDef_B".into(),
            "ObjDef_C".into(),
        ])
        .unwrap();
        assert_eq!(m.select("ObjDef_C"), Some("ObjDef_C"));
        let badge = m.badge().expect("armed");
        assert!(
            badge.contains("ObjDef_C") && badge.contains("3/3"),
            "the badge must follow the selection, position included: {badge:?}"
        );

        assert_eq!(
            m.select("ObjDef_Gone"),
            None,
            "a name that is not in the listing must select nothing"
        );
        assert_eq!(
            m.selected(),
            Some("ObjDef_C"),
            "a refused selection must leave the previous one exactly where it was"
        );
        assert_eq!(
            m.names().len(),
            3,
            "the list itself is what the picker draws"
        );
    }

    /// **Arming over an empty list refuses**, and the refusal names the prefix that came up empty.
    #[test]
    fn arming_with_no_archetypes_refuses_rather_than_arming_a_mode_that_can_place_nothing() {
        let mut m = Mode::new();
        let e = m.arm(Vec::new()).expect_err("an empty list cannot arm");
        assert!(
            e.message.contains(ARCHETYPE_PREFIX),
            "the refusal must name the prefix that found nothing: {:?}",
            e.message
        );
        assert!(!m.is_armed(), "a refused arm must leave the mode off");
        assert_eq!(m.badge(), None, "…and must not leave a badge standing");
    }

    /// A truncated archetype search says so; a complete one adds no noise.
    #[test]
    fn a_bounded_archetype_search_admits_what_it_did_not_see() {
        let whole = Archetypes {
            names: vec!["ObjDef_A".into(), "ObjDef_B".into()],
            total: 2,
        };
        assert_eq!(whole.truncation_note(), None);

        let cut = Archetypes {
            names: vec!["ObjDef_A".into()],
            total: 137,
        };
        let note = cut.truncation_note().expect("a cut search must say so");
        assert!(note.contains("137"), "{note:?}");
    }

    // ---------------------------------------------------------------------------------------------
    // The act's box (`F-SPAWN-OUTSIDE-ACT`)
    // ---------------------------------------------------------------------------------------------

    /// **The box is half-open**, which matters at exactly one pixel per axis.
    ///
    /// aeon's own words are `[0, Level_Width) × [0, Level_Height)`, so `Level_Width` itself is the first
    /// column that is *not* in the act. An inclusive test would accept one illegal column and one illegal
    /// row — the object would be acked and then culled, which is the defect, surviving at the one place
    /// nobody clicks twice.
    #[test]
    fn the_act_box_is_half_open_so_the_extent_itself_is_outside_it() {
        let b = Bounds {
            width: 6144,
            height: 4096,
        };
        assert!(
            b.contains(0, 0),
            "the low edge is a literal 0 and is inside"
        );
        assert!(b.contains(6143, 4095), "the last legal pixel is inside");
        assert!(!b.contains(6144, 0), "`Level_Width` itself is outside");
        assert!(!b.contains(0, 4096), "`Level_Height` itself is outside");
        assert!(!b.contains(6144, 4096));
    }

    /// The three refusals this gate can raise are **three different facts**, and each says which.
    #[test]
    fn each_act_bounds_refusal_is_its_own_fact_with_its_own_next_action() {
        let b = Bounds {
            width: 1024,
            height: 768,
        };
        let outside = b.outside(2000, 10);
        let unknown = Bounds::unmeasurable();
        let no_act = Bounds::no_act();

        // Machine-readable and distinct, so a caller never has to read prose to tell them apart.
        let reasons: Vec<_> = [&outside, &unknown, &no_act]
            .iter()
            .map(|r| r.reason.clone())
            .collect();
        assert_eq!(
            reasons,
            vec![
                Some("outsideAct".to_string()),
                Some("actExtentUnknown".to_string()),
                Some("noActLoaded".to_string())
            ]
        );

        for r in [&outside, &unknown, &no_act] {
            // All three are the WINDOW's: no RPC happened, so no RPC code may be claimed for one.
            assert_eq!(r.code, None, "{r:?}");
            assert!(
                r.terminal("ObjDef_Ring", Some("Space"))
                    .contains("the window"),
                "the reader must be told whose refusal this is: {r:?}"
            );
            // Every one of them ends in a sentence with a next action, and the toast carries the action
            // rather than the essay — a toast is cut from the right and these messages are long.
            let toast = r.toast(Some("Space"));
            assert!(toast.starts_with("SPAWN REFUSED: "), "{toast:?}");
            assert!(
                toast.len() < r.message.len(),
                "the glass must get the remedy, not the whole explanation: {toast:?}"
            );
            // …and the remedy is the window's own, not the pause key's — a rebind must not touch these.
            assert_eq!(
                r.remedy(Some("Space")),
                r.remedy(Some("F8")),
                "these remedies name no key, so rebinding must not change them: {r:?}"
            );
        }

        // The one that measured something quotes the measurement, on both surfaces.
        assert!(outside.message.contains("1024") && outside.message.contains("768"));
        assert!(
            outside.toast(None).contains("1024 x 768"),
            "{:?}",
            outside.toast(None)
        );
    }

    // ---------------------------------------------------------------------------------------------
    // Refusals reach the person
    // ---------------------------------------------------------------------------------------------

    /// The server's `message` survives to the terminal **verbatim**, for every refusal shape.
    ///
    /// This is the property the whole `Host::call` arrangement is for: the five engine refusals in
    /// §11.32 §6 are the valuable part, and a picker that summarised them would have thrown away the
    /// reason it goes through the server at all.
    #[test]
    fn every_refusal_reaches_the_terminal_with_the_servers_own_words() {
        let cases = [
            Refusal {
                code: Some(-32005),
                reason: Some("machineRunning".into()),
                message:
                    "emulator/object_spawn needs the machine paused; call emulator/pause first"
                        .into(),
                remedy: None,
            },
            Refusal {
                code: Some(-32005),
                reason: Some("objectPoolFull".into()),
                message: "the dynamic object pool is full; nothing was evicted".into(),
                remedy: None,
            },
            Refusal {
                code: Some(-32013),
                reason: None,
                message: "this build has no live-object mailbox".into(),
                remedy: None,
            },
            Refusal::local("the window could not turn this click into a world position"),
        ];
        for r in &cases {
            let line = r.terminal("ObjDef_Ring", Some("Space"));
            assert!(
                line.contains(&r.message),
                "the server's words must survive verbatim: {line:?}"
            );
            assert!(
                line.contains("ObjDef_Ring"),
                "a refusal with no subject reads as 'something went wrong': {line:?}"
            );
            assert!(
                line.contains("nothing was placed"),
                "a refusal must never be readable as a success: {line:?}"
            );
            assert!(
                !r.toast(Some("Space")).is_empty(),
                "no refusal may reach the glass as silence"
            );
            assert!(
                r.toast(Some("Space")).contains("REFUSED"),
                "the toast must read as a refusal: {:?}",
                r.toast(Some("Space"))
            );
        }
    }

    /// **The paused requirement is made actionable**, and it is keyed on the discriminant rather than on
    /// prose.
    ///
    /// §11.32 §7.1 makes `paused` a precondition of all three rows and the server's remedy is *"call
    /// emulator/pause first"* — correct for a socket client and unusable for somebody holding a keyboard.
    /// The key named here comes from the command registry (`main.rs`), so it cannot go stale against a
    /// rebind.
    #[test]
    fn a_click_on_a_running_machine_is_told_which_key_pauses_it() {
        let r = Refusal {
            code: Some(-32005),
            reason: Some("machineRunning".into()),
            message: "emulator/object_spawn needs the machine paused; call emulator/pause first"
                .into(),
            remedy: None,
        };
        let toast = r.toast(Some("Space"));
        assert_eq!(
            toast, "SPAWN REFUSED: press Space to pause this window, then click the spot again",
            "the glass must carry the next action"
        );
        // Rebinding the key rebinds the sentence: nothing here transcribes "Space".
        assert!(r.toast(Some("F8")).contains("press F8 to pause"));
        // A different reason gets no invented remedy — the server's message stands alone.
        let other = Refusal {
            reason: Some("objectPoolFull".into()),
            ..r.clone()
        };
        assert_eq!(other.remedy(Some("Space")), None);
        assert!(other.toast(Some("Space")).contains(&other.message));
    }

    // ---------------------------------------------------------------------------------------------
    // A success is not allowed to read as a placement confirmation
    // ---------------------------------------------------------------------------------------------

    /// **The reply's `x`/`y` name the moment they are for.**
    ///
    /// §11.32's 2026-09-03 addendum rules them a re-read after `framesAdvanced`, not an echo, and
    /// requires the description to say so — the `ATTR-RGB-LATCH` defect one surface over. A window that
    /// printed the pair bare would put that defect back on the glass, so this row pins the disclosure
    /// rather than the numbers.
    #[test]
    fn a_success_says_the_position_is_a_re_read_and_not_where_you_clicked() {
        let p = Placed {
            handle: "0x8E62".into(),
            addr: "0xFFFF8E62".into(),
            slot: Some(12),
            asked: (1111, 2222),
            now: (7, 9),
            frames_advanced: 2,
            caveat: None,
        };
        let line = p.terminal("ObjDef_Ring");
        assert!(line.contains("ObjDef_Ring"), "{line:?}");
        assert!(
            line.contains("(1111, 2222)"),
            "what was asked for: {line:?}"
        );
        assert!(
            line.contains("(7, 9)"),
            "what the record reads now: {line:?}"
        );
        assert!(
            line.contains("NOW") || line.contains("now"),
            "the moment must be named: {line:?}"
        );
        assert!(
            line.contains("not a confirmation of where you clicked"),
            "the addendum's disclosure must be on the line: {line:?}"
        );
        assert!(
            p.toast("ObjDef_Ring").contains("NOW AT"),
            "the glass must name the moment too: {:?}",
            p.toast("ObjDef_Ring")
        );
    }

    /// A `slot` the layout could not supply is **named as absent**, never printed as `0`.
    #[test]
    fn an_unresolvable_slot_is_named_absent_rather_than_defaulted() {
        let p = Placed {
            handle: "0x8E62".into(),
            addr: "0xFFFF8E62".into(),
            slot: None,
            asked: (10, 20),
            now: (10, 20),
            frames_advanced: 1,
            caveat: Some("the slot is no longer active".into()),
        };
        let line = p.terminal("ObjDef_Ring");
        assert!(
            line.contains("cannot name"),
            "an absent slot must say so: {line:?}"
        );
        assert!(!line.contains("slot 0"), "…and never invent one: {line:?}");
        assert!(
            line.contains("the slot is no longer active"),
            "a caveat the server sent must reach the reader: {line:?}"
        );
        assert!(
            p.toast("ObjDef_Ring").contains("0x8E62"),
            "with no slot the toast falls back to the handle"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // Subtypes: the prefix, the clash detector, and the sentences
    // ---------------------------------------------------------------------------------------------

    /// **The prefix is a substitution on the archetype's own name, doubled at the boundary.**
    ///
    /// The single underscores inside a subtype's own name are the half that matters: the picker
    /// prefix-searches and never splits a name, so `Up_Red` has to survive as one name. A row that only
    /// checked `ObjDef_Spring` would pass against an implementation that split on the first underscore
    /// and offered the subtypes of an object called `Up`.
    #[test]
    fn the_subtype_prefix_is_a_substitution_and_the_boundary_is_doubled() {
        assert_eq!(
            subtype_prefix("ObjDef_Spring").as_deref(),
            Some("ObjSub_Spring__")
        );
        // An object name that already carries an underscore keeps it whole, which is the case the
        // doubled boundary exists for.
        assert_eq!(
            subtype_prefix("ObjDef_Spring_Board").as_deref(),
            Some("ObjSub_Spring_Board__")
        );
        // ⚑ And the two do not overlap, which is the property the whole convention buys.
        let spring = subtype_prefix("ObjDef_Spring").unwrap();
        let board = subtype_prefix("ObjDef_Spring_Board").unwrap();
        assert!(
            !board.starts_with(&spring),
            "a longer object name must not file its subtypes under a shorter one: {board} against \
             {spring}"
        );

        // Nothing to derive a namespace from is `None`, never a guess.
        assert_eq!(
            subtype_prefix("ObjDef_"),
            None,
            "an empty stem names no object"
        );
        assert_eq!(
            subtype_prefix("Player_1"),
            None,
            "not an archetype name at all"
        );
    }

    /// **The clash detector catches both shapes, and stays quiet on the safe one.**
    ///
    /// It exists in spite of the convention, on the engine lane's own argument: *a rule that depends on
    /// every future author remembering a convention is not a rule.* The second shape is the one a rule
    /// phrased as "another name plus an underscore" misses, and it is here because the mechanism is
    /// asked directly rather than the phrasing being trusted.
    #[test]
    fn the_clash_detector_catches_both_shapes_and_leaves_the_safe_one_alone() {
        let all = vec![
            "ObjDef_Spring".to_string(),
            // Shape one: another name plus a single underscore.
            "ObjDef_Spring_".to_string(),
            // Shape two: another name plus the doubled boundary. Its own namespace begins with
            // `ObjSub_Spring__`, so its subtypes land in the spring's search.
            "ObjDef_Spring__Tall".to_string(),
            // The safe one. `ObjSub_Spring_Board__` does not begin with `ObjSub_Spring__`.
            "ObjDef_Spring_Board".to_string(),
            "ObjDef_Ring".to_string(),
        ];
        let mut hit = swallowed_by("ObjDef_Spring", &all);
        hit.sort();
        assert_eq!(
            hit,
            ["ObjDef_Spring_", "ObjDef_Spring__Tall"],
            "both swallowing shapes must be named, and the safe neighbour must not be"
        );
        // An ordinary build says nothing, so the note is never standing decoration.
        assert!(swallowed_by(
            "ObjDef_Spring",
            &["ObjDef_Spring".to_string(), "ObjDef_Ring".to_string()]
        )
        .is_empty());
    }

    /// **A value past a byte is carried whole and offered as nothing.**
    #[test]
    fn a_subtype_too_large_for_a_byte_is_carried_and_not_cut_down() {
        let big = Subtype {
            name: "ObjSub_Spring__Wide_Huge".into(),
            value: MAX_SUBTYPE + 1,
        };
        assert_eq!(big.byte(), None, "256 must not become 0");
        assert_eq!(
            Subtype {
                name: "x".into(),
                value: MAX_SUBTYPE
            }
            .byte(),
            Some(255),
            "the boundary itself fits"
        );
        assert_eq!(big.short("ObjSub_Spring__"), "Wide_Huge");
        assert_eq!(
            big.short("ObjSub_Ring__"),
            "ObjSub_Spring__Wide_Huge",
            "a prefix that does not match leaves the whole name rather than an empty label"
        );

        // And the armed default steps over it rather than arming something unsendable.
        let s = Subtypes {
            archetype: "ObjDef_Spring".into(),
            prefix: Some("ObjSub_Spring__".into()),
            entries: vec![
                big.clone(),
                Subtype {
                    name: "ObjSub_Spring__Up_Red".into(),
                    value: 0,
                },
            ],
            truncated: false,
            collisions: Vec::new(),
        };
        assert_eq!(
            s.default_choice().map(|e| e.name.as_str()),
            Some("ObjSub_Spring__Up_Red"),
            "the default must be armable, so an oversized first entry is stepped over"
        );
        // Nothing armable at all is `None`, never a byte this crate invented.
        let none = Subtypes {
            entries: vec![big],
            ..s
        };
        assert_eq!(none.default_choice(), None);
    }

    /// ⚑ **A cut-short list says it is cut short, in words that name the failure.**
    ///
    /// This is the absence with no visible symptom: a shortened subtype list looks exactly like an object
    /// with fewer subtypes, so a reader picks from three when there are twelve and nothing on the glass
    /// argues with them. The sentence therefore has to say the list is incomplete rather than merely
    /// mention that a limit exists.
    #[test]
    fn a_cut_short_subtype_list_says_so_and_a_complete_one_claims_nothing() {
        let mut s = Subtypes::none_for("ObjDef_Spring");
        s.entries.push(Subtype {
            name: "ObjSub_Spring__Up_Red".into(),
            value: 0,
        });
        assert_eq!(
            s.truncation_note(),
            None,
            "a complete list must not carry a note, or the note means nothing when it appears"
        );

        s.truncated = true;
        let note = s
            .truncation_note()
            .expect("a cut list owes the reader a line");
        assert!(
            note.contains("CUT SHORT") && note.contains("ObjDef_Spring"),
            "the note must say the list is incomplete and name whose it is: {note:?}"
        );
        assert!(
            note.contains("not to read the rows below as all of them")
                || note.contains("all of them"),
            "it must tell the reader what NOT to conclude, which is the whole hazard: {note:?}"
        );
    }

    /// **The two absences are two different findings and must not read alike** (P6).
    #[test]
    fn the_two_reasons_for_no_subtypes_are_two_different_sentences() {
        let named = Subtypes::none_for("ObjDef_Ring");
        let a = named.absence().expect("an empty set owes a line");
        assert!(
            a.contains("ObjSub_Ring__"),
            "a listing that publishes nothing under a namespace names the namespace: {a:?}"
        );

        let unnameable = Subtypes::none_for("ObjDef_");
        assert_eq!(unnameable.prefix, None);
        let b = unnameable.absence().expect("this owes a line too");
        assert_ne!(a, b, "two different findings must not be one sentence");
        assert!(
            b.contains("no object name"),
            "a name with no object in it says that, rather than claiming the listing is empty: {b:?}"
        );

        // A set with rows owes nothing.
        let mut full = named.clone();
        full.entries.push(Subtype {
            name: "ObjSub_Ring__A".into(),
            value: 1,
        });
        assert_eq!(full.absence(), None);
    }

    /// **A clash is stated and nothing is hidden or reassigned.**
    #[test]
    fn a_namespace_clash_is_stated_and_names_the_other_archetype() {
        let mut s = Subtypes::none_for("ObjDef_Spring");
        assert_eq!(s.collision_note(), None, "an ordinary build says nothing");
        s.collisions.push("ObjDef_Spring_".into());
        let note = s.collision_note().expect("a clash owes the reader a line");
        assert!(
            note.contains("ObjDef_Spring_") && note.contains("ObjSub_Spring__"),
            "the note must name the other archetype and the namespace they share: {note:?}"
        );
        assert!(
            note.contains("Nothing has been hidden"),
            "the reader must be told the rows were left alone, since this window cannot know which \
             object a clashing name was written for: {note:?}"
        );
    }

    /// **No sentence this module composes for a person carries a dash or cites the specification**
    /// (P10, P9), and none pads itself into a column (P2).
    #[test]
    fn no_subtype_sentence_carries_a_dash_or_a_citation_or_a_drawn_column() {
        let mut all: Vec<String> = Vec::new();
        let visit = |s: &Subtypes, all: &mut Vec<String>| {
            all.extend(s.absence());
            all.extend(s.truncation_note());
            all.extend(s.collision_note());
        };
        let populated = Subtypes {
            archetype: "ObjDef_Spring".into(),
            prefix: Some("ObjSub_Spring__".into()),
            entries: Vec::new(),
            truncated: true,
            collisions: vec!["ObjDef_Spring_".into()],
        };
        visit(&populated, &mut all);
        // The other absence arm, which is a different sentence and must be swept too.
        let mut unnameable = Subtypes::none_for("ObjDef_");
        unnameable.truncated = true;
        visit(&unnameable, &mut all);
        assert!(all.len() >= 5, "the sweep must reach every arm: {all:?}");
        for t in all {
            for bad in ['\u{2014}', '\u{2013}'] {
                assert!(!t.contains(bad), "user-facing text carries {bad:?}: {t:?}");
            }
            assert!(
                !t.contains('\u{a7}') && !t.contains("protocol.md"),
                "the person at this window is not holding the specification: {t:?}"
            );
            assert!(
                !t.contains("  ") && !t.contains('\t'),
                "a column drawn inside a string: {t:?}"
            );
        }
    }

    // ---------------------------------------------------------------------------------------------
    // The wire: what the equate reply is read for, and what the placement carries
    // ---------------------------------------------------------------------------------------------

    /// A [`Caller`] that answers from a script and records what it was asked.
    ///
    /// It is here for the two properties a real machine cannot show cheaply: that a reply saying it was
    /// truncated is believed, and that a request carries the subtype key only when one was named.
    #[cfg(feature = "aether")]
    struct Scripted {
        reply: serde_json::Value,
        seen: Vec<(String, serde_json::Value)>,
    }

    #[cfg(feature = "aether")]
    impl Caller for Scripted {
        fn call(
            &mut self,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, Refusal> {
            self.seen.push((method.to_string(), params.clone()));
            match method {
                "emulator/lookup_equate" => Ok(self.reply.clone()),
                "emulator/object_at" => Ok(serde_json::json!({
                    "worldSource": "camera", "world": {"x": 10, "y": 20}
                })),
                "emulator/read_memory" => Ok(serde_json::json!({"bytes": "0x0400"})),
                "emulator/object_spawn" => Ok(serde_json::json!({
                    "handle": "0x1", "addr": "0x2", "x": 1, "y": 2, "framesAdvanced": 1
                })),
                other => panic!("unscripted call to {other}"),
            }
        }
        fn address_of(&mut self, _symbol: &str) -> Option<u32> {
            Some(0x00FF_0000)
        }
    }

    /// ⚑ **`truncated` is read off the reply's own flag, and there is no `total` to reach for.**
    ///
    /// The prefix reply carries `query`, `matches` and `truncated`, and **no count of what was left
    /// behind**. So this reply deliberately has no `total` key: an implementation that reconstructed
    /// truncation as `total > matches.len()` would read a missing key as zero, conclude the list was
    /// whole, and draw a short list that looks complete. That is the exact failure the note exists for,
    /// arriving through the field that does not exist.
    #[cfg(feature = "aether")]
    #[test]
    fn a_truncated_equate_reply_is_believed_and_no_total_is_reached_for() {
        let mut c = Scripted {
            reply: serde_json::json!({
                "query": "ObjSub_Spring__",
                "matches": [
                    {"name": "ObjSub_Spring__Down_Red", "value": 0x20},
                    {"name": "ObjSub_Spring__Up_Red", "value": 0x00}
                ],
                "truncated": true
            }),
            seen: Vec::new(),
        };
        let s = subtypes(&mut c, "ObjDef_Spring", &["ObjDef_Spring".to_string()]).unwrap();
        assert!(
            s.truncated,
            "the reply said it was cut short and nothing here may decide otherwise"
        );
        assert!(s.truncation_note().is_some());
        assert_eq!(
            s.entries.iter().map(|e| e.value).collect::<Vec<_>>(),
            [0x00, 0x20],
            "and the rows are in value order, not the order they arrived in"
        );
        // The door that was knocked on is the equate door, which is the whole point of the row.
        assert_eq!(c.seen[0].0, "emulator/lookup_equate");
        assert_eq!(c.seen[0].1["prefix"], serde_json::json!("ObjSub_Spring__"));

        // The other direction: a reply that says it is whole is taken at its word too.
        let mut c = Scripted {
            reply: serde_json::json!({"query": "x", "matches": [], "truncated": false}),
            seen: Vec::new(),
        };
        let s = subtypes(&mut c, "ObjDef_Spring", &[]).unwrap();
        assert!(!s.truncated);
        assert_eq!(s.truncation_note(), None);
    }

    /// **The placement carries the subtype key only when one was named**, and carries the byte it was
    /// given rather than one derived from anything.
    #[cfg(feature = "aether")]
    #[test]
    fn the_placement_sends_the_byte_it_was_given_and_omits_the_key_when_given_none() {
        let mut c = Scripted {
            reply: serde_json::json!({}),
            seen: Vec::new(),
        };
        place(&mut c, "ObjDef_Spring", Some(0x02), (1, 2)).expect("the scripted spawn succeeds");
        let (_, params) = c
            .seen
            .iter()
            .find(|(m, _)| m == "emulator/object_spawn")
            .expect("a spawn was requested");
        assert_eq!(params["subtype"], serde_json::json!(2));
        assert_eq!(params["defSymbol"], serde_json::json!("ObjDef_Spring"));

        let mut c = Scripted {
            reply: serde_json::json!({}),
            seen: Vec::new(),
        };
        place(&mut c, "ObjDef_Spring", None, (1, 2)).expect("the scripted spawn succeeds");
        let (_, params) = c
            .seen
            .iter()
            .find(|(m, _)| m == "emulator/object_spawn")
            .expect("a spawn was requested");
        assert_eq!(
            params.get("subtype"),
            None,
            "no subtype was named, so no key may appear: sending a zero would assert a value this \
             window never read"
        );
    }
}
