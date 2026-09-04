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
//! [`crate::overlay::Overlay::spawn_badge`]). This is a correctness requirement in this crate and not
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
                "world ({x}, {y}) is outside this act, whose extent is {} x {} pixels — the valid box \
                 is [0, {}) x [0, {}), read just now from `{LEVEL_WIDTH_SYMBOL}` and \
                 `{LEVEL_HEIGHT_SYMBOL}`. An object placed there is culled by the engine on camera \
                 distance with no error and nothing on screen, so the click is refused here rather than \
                 acked and thrown away",
                self.width, self.height, self.width, self.height
            ),
            Some(format!(
                "click inside the act — it is {} x {} pixels",
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
                 nothing on screen, and a click sent unchecked would be acked as placed and then vanish — \
                 so this refuses rather than guessing the act is infinite"
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
                 an act has initialised — there is no act for an object to be inside of, and anything \
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
                "the listing holds {} — this is the first {}, which is where the bus's bounded \
                 symbol search stops",
                self.total,
                self.names.len()
            )
        })
    }
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
    /// `pub(crate)` rather than `pub`: [`crate::bus`] builds the server-side refusal as a literal and must
    /// be able to say `None` here, but nothing outside this crate may set a remedy — a remedy is a
    /// statement in *this window's* vocabulary about *this window's* keys.
    pub(crate) remedy: Option<String>,
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
    pub fn terminal(&self, archetype: &str, pause_key: Option<&str>) -> String {
        let who = match (self.code, self.reason.as_deref()) {
            (Some(c), Some(r)) => format!("aether {c} {r}"),
            (Some(c), None) => format!("aether {c}"),
            (None, _) => "the window".to_string(),
        };
        let mut s = format!(
            "spawn refused by {who} — nothing was placed, {archetype} is not on screen: {}",
            self.message
        );
        if let Some(r) = self.remedy(pause_key) {
            s.push_str(&format!(" — {r}"));
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
        format!("SPAWN REFUSED — {body}")
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
            "spawned {archetype} at world ({}, {}) — it is {slot}, handle {}, addr {}. \
             Its record reads ({}, {}) after {} frame(s): that is where the object is NOW, not a \
             confirmation of where you clicked — anything with velocity has already moved.",
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
/// Disarmed by default and disarmed after every `reset` / ROM swap, because the archetype list was read
/// out of a listing that may no longer describe the machine — and a stale archetype address is precisely
/// the silent-corruption shape §11.32 §8 exists to refuse.
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
                 nothing a click could place — spawn mode is left off"
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
            assert!(toast.starts_with("SPAWN REFUSED — "), "{toast:?}");
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
            toast, "SPAWN REFUSED — press Space to pause this window, then click the spot again",
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
}
