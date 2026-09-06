//! **The Screen tab's pointer half** — click a dot to arm a watch on whatever draws it, or, in spawn mode,
//! to put an object there.
//!
//! This is the migration's S1 (`docs/2026-09-05-frontend-migration-recon.md` §3.1), and it closes
//! `F-SPAWN-PICKER-PANEL-SURFACE` by dissolving the question that row was blocked on. That row asked *which*
//! window the owner's *"clicking a spot in the Screen panel"* meant, because there were two; after the
//! migration there is one window and it is the panels window with the game picture in a tab, so both answers
//! became the same answer.
//!
//! ## ⚑ S2a: THE MASK REACHES THE PICTURE, SO THE PANEL DESCRIBES THE PICTURE
//!
//! S1 shipped **masked-off-only** — while any layer was hidden a click was refused outright — because
//! `oracle-player` had one pixel path and `oracle-frontend` had two. That is over.
//! [`Machine::render_masked`](crate::machine::Machine::render_masked) is this window's `blit_masked`, and
//! [`crate::bus::drain`] applies it on every iteration a mask is set, to the loop's own frames and to a
//! client-driven one alike. The blanket refusal and its two rows are deleted.
//!
//! [`pick::resolve`](oracle_frontend::pick::resolve)'s standing invariant is *"the panel describes the
//! picture"*, and it is now satisfiable: resolve under **the mask the picture on the glass was actually
//! drawn with**, which is also the bus's mask, so the answer describes what the person is looking at *and*
//! agrees with what `emulator/pixel_attribution` would tell a socket client about the same dot in the same
//! instant.
//!
//! ### The residual, and why it is a refusal rather than a caveat
//!
//! "The mask the glass was drawn with" and "the mask the bus holds" are two different facts, and they can
//! separate: the mask can move *after* the picture was made — the palette can call
//! `emulator/set_layer_enabled` during the same `build_ui` that draws the picture — or a masked re-render
//! can fail to produce a picture at all. In that window the glass is one picture and the bus is describing
//! another, and there is no honest answer to give about a dot.
//!
//! So the gate is now **narrow and exact**: a click is refused only while [`Panel::pick`]'s `glass`
//! argument disagrees with `bus.layers()`, and the refusal says which is which. That is
//! loud-on-unmeasurable applied to a gesture — *"COULD NOT MEASURE"* beats a plausible answer — and it
//! costs a person nothing on any ordinary frame, because on an ordinary frame the two agree.
//!
//! ### And the mask says it is on, continuously, where a person is looking
//!
//! [`mask_statement`] is a standing line drawn on **every** frame a mask is set, above the picture, naming
//! the hidden layers. It is a correctness requirement rather than decoration, and the reasoning is the
//! consumer's, banked in `docs/OVERSEER.md`'s GUI-LAYERS entry: *the author will forget, and then read a
//! masked picture as the real one.* A toast cannot carry it, because a toast expires and the mask does not.
//! It also has to say the second thing a mask does to this window's picture — the masked path is a post-hoc
//! re-render, so mid-frame palette effects are gone — or the picture silently changes in a way the toggle
//! did not ask for.
//!
//! ## Points, pixels, and the one thing that is invisible at 1.0 scaling
//!
//! egui works in **points**; the picture, the blit and `present::window_to_native` work in **device
//! pixels**, and the two differ by `Context::pixels_per_point`. The owner's display is not at 1.0, so a
//! conversion that is merely forgotten produces a picking offset that every test on a 1.0 harness would
//! call correct. [`dot_at`] takes `ppp` explicitly for that reason and is tested at 1.0, 1.5 and 2.0.
//!
//! ## Why the inverse reads the drawn rect rather than re-deriving the fit
//!
//! `oracle-frontend` computes its destination rectangle and then inverts *that same rectangle*, because
//! minifb never tells the caller where it put the image. egui does: `Response::rect` is the rect the image
//! was actually laid out in. So [`dot_at`] inverts **what was drawn**, not a second derivation of what
//! should have been drawn, and the recon's §3.2 hazard — *"change the fit and the inverse must change with
//! it"* — cannot arise. S2 changed the fit from a float square scale to `present::dest_rect` under three
//! `Aspect` modes and this function needed **no edit at all**; the identity row below passes at every mode
//! unchanged, which is the assertion rather than the anecdote.
//!
//! ## The two routes, and why a click uses both
//!
//! `protocol.md` D15: an in-process GUI *"reads the method registry directly, in-process; it does not open a
//! socket to itself."* That cuts two ways and this module uses both edges of it.
//!
//! * **Resolving the dot is a read**, and it goes straight to the core — `pick::resolve` over the VDP the
//!   loop owns. `pick.rs`'s own module doc argues this out and `bus_parity` holds it to it.
//! * **Arming the watch, and spawning, are per-gesture commands**, and they go through
//!   [`Bus::call`](crate::bus::Bus::call) → `Host::call`. Synchronous, in-process, no socket — and the
//!   point of going through the server is that a click gets the tool's exact reply *and its exact refusal*
//!   rather than a sentence this module composed about a server it lives inside. The watch cap
//!   (`watchCapReached`) and every spawn refusal §11.32 §6 defines arrive here whole and are shown whole.

use egui::{Pos2, Rect as ERect, Vec2};
use oracle_core::render::LayerMask;
use oracle_frontend::present::{self, Aspect};
use oracle_frontend::{pick, spawn};
use serde_json::{json, Value};

use crate::bus::{Answer, Bus};
use crate::machine::Machine;
use crate::spawn_picker;

/// How the player's transport control is named to a person, for the one refusal that has a remedy.
///
/// `spawn::Refusal::remedy` keys off the machine-readable `reason` and formats *"press {k} to pause this
/// window"*. `oracle-frontend` passes the key its command registry actually bound; this window has a button
/// rather than a key, so it passes the button's own label constant. **Derived, not transcribed** — that is
/// the rule the frontend's version states, and a literal `"pause"` here would go stale the moment the label
/// changed.
fn pause_remedy() -> String {
    format!("the {} button on the top bar", crate::ui::PAUSE_LABEL)
}

// -------------------------------------------------------------------------------------------------------
// Geometry
// -------------------------------------------------------------------------------------------------------

/// The picture's size **in egui points**, for a `src_w x src_h` native frame in an `avail`-sized panel,
/// under `aspect`.
///
/// ⚑ **S2. This used to be four lines of float square-pixel fit, and that was a geometrically wrong
/// picture by `oracle-frontend`'s own standard.** A Mega Drive does not have square pixels: H40 puts 320
/// dots and H32 puts 256 dots across the *same* physical width of a 4:3 television, so the correct picture
/// is the active area letterboxed to 4:3 in both modes — H32 stretched wider, not pillarboxed. That is
/// [`Aspect::Tv`], it is the **default**, and the player did not have it.
///
/// The body is [`present::dest_rect`] — the frontend's own fit, the same integer arithmetic and the same
/// *exact* reduced-ratio derivation, rather than a second implementation of 4:3 in this file. Only the
/// rect's size is used: egui centres the picture itself and the origin `dest_rect` computes is for a
/// caller that blits into a window-sized buffer.
///
/// **`ppp` is taken even though the result is in points**, and that is not a rounding nicety.
/// [`Aspect::Integer`] is a claim about the **pixel** grid — "the largest whole scale at which no row or
/// column is duplicated unevenly" — and it is meaningless in points: computed in points at a non-integer
/// `pixels_per_point`, "integer mode" would duplicate rows while calling itself sharp. So the fit is
/// computed in device pixels and converted back at the end.
///
/// Returns a zero `Vec2` for a degenerate panel (a zero dimension anywhere), so a window manager handing
/// out a 0-height panel mid-resize produces no picture rather than a panic or a NaN.
pub fn fit(avail: Vec2, src_w: usize, src_h: usize, ppp: f32, aspect: Aspect) -> Vec2 {
    if !ppp.is_finite() || ppp <= 0.0 || !avail.x.is_finite() || !avail.y.is_finite() {
        return Vec2::ZERO;
    }
    let w_px = (avail.x * ppp).floor().max(0.0) as usize;
    let h_px = (avail.y * ppp).floor().max(0.0) as usize;
    let r = present::dest_rect(w_px, h_px, src_w, src_h, aspect);
    if r.w == 0 || r.h == 0 {
        return Vec2::ZERO;
    }
    Vec2::new(r.w as f32 / ppp, r.h as f32 / ppp)
}

/// **The inverse of the blit** — the native dot under `pos`, or `None` when the pointer is off the picture.
///
/// `image` is the rect egui **actually drew the image in** (`Response::rect`), in points and in screen
/// space. The offset within it is converted to device pixels by `ppp` and handed to
/// [`present::window_to_native`](oracle_frontend::present::window_to_native), which is the exact inverse of
/// `oracle-frontend`'s own blit rather than a re-derivation of it — the property that makes click-to-watch
/// survive an arbitrary window size, reused here rather than rewritten.
///
/// The rect passed down is anchored at the origin and sized in pixels, because the offset has already had
/// the origin subtracted; `window_to_native`'s letterbox rejection is then the same test as "the pointer is
/// outside the picture", which is what it is being asked.
pub fn dot_at(image: ERect, pos: Pos2, ppp: f32, src_w: usize, src_h: usize) -> Option<(u16, u16)> {
    // NaN-safe on purpose: `is_finite` rejects a NaN scale, which a bare `<= 0.0` would let through.
    if !ppp.is_finite() || ppp <= 0.0 {
        return None;
    }
    let w = (image.width() * ppp).round();
    let h = (image.height() * ppp).round();
    if !w.is_finite() || !h.is_finite() || w < 1.0 || h < 1.0 {
        return None;
    }
    let dx = (pos.x - image.min.x) * ppp;
    let dy = (pos.y - image.min.y) * ppp;
    present::window_to_native(
        dx,
        dy,
        present::Rect {
            x: 0,
            y: 0,
            w: w as usize,
            h: h as usize,
        },
        src_w,
        src_h,
    )
}

// -------------------------------------------------------------------------------------------------------
// The panel's state
// -------------------------------------------------------------------------------------------------------

/// What the tab shows about the last gesture, and whether it was a refusal.
///
/// **`refused` is a field, never a shape of the text.** The tab colours on it. A refusal that reads like a
/// success is the one rendering mistake a debug surface cannot afford, and matching on a `"REFUSED"` prefix
/// would be a second encoding of the same fact: the rule [`crate::ui::Echo`] already states one file over.
///
/// # ⚑ Three parts rather than one paragraph, and the seam is the composer's
///
/// The readout used to be a single `String` rendered as one wrapped block, which is the shape that makes a
/// correct answer hard to read: the sentence naming what was clicked, the addressing that backs it up, and
/// the count of what got armed are three different kinds of statement and they all arrived at the same
/// weight.
///
/// The split is **not** made by looking for punctuation in a finished string. `pick::Pick` already carries
/// `headline` and `detail` as separate fields because `describe` composed them separately, and the armed
/// count is this panel's own number. Every part here is handed over by whoever made it, which is the same
/// discipline that keeps `refused` a field: a panel that recovered structure by parsing prose would break
/// the first time the prose changed, silently and in the reader's favour.
pub struct Readout {
    /// The sentence a person reads: what was clicked, in words, with any clauses the dot earned.
    pub head: String,
    /// The addressing behind it, or `None` for a gesture that has none (a mode change, a refusal).
    pub detail: Option<String>,
    /// What the gesture did to this panel's watches. `None` when the gesture was not a pick.
    pub outcome: Option<String>,
    pub refused: bool,
}

impl Readout {
    /// The whole readout as one string, **for tests only**.
    ///
    /// The tab draws the three parts at three weights and never joins them, so this is not what any
    /// reader sees; it is the seam an assertion works across, because a panel this crate cannot
    /// screenshot is only checkable where it becomes text. Deliberately not offered to the renderer:
    /// a joined string beside a laid-out one is two spellings of one answer, and the flat one is the
    /// shape this parcel exists to get rid of.
    #[cfg(test)]
    pub fn text(&self) -> String {
        let mut s = self.head.clone();
        for part in [self.detail.as_ref(), self.outcome.as_ref()]
            .into_iter()
            .flatten()
        {
            s.push('\n');
            s.push_str(part);
        }
        s
    }
}

/// The Screen tab's own state between repaints.
///
/// **The watches themselves are not in here — only their handles are.** The instrument is the `Host`'s one
/// `Watchpoints`, which the Watchpoints tab reads afresh and `emulator/watchpoint_hits` answers from; a
/// second one on this side would be two answers to one question. What this panel keeps is the list of
/// handles *it* issued, because retiring only its own is a correctness requirement: a
/// `watchpoint_clear {all: true}` would take a socket client's watches with it, which is the
/// shared-instrument hazard `oracle-frontend` learned the hard way.
#[derive(Default)]
pub struct Panel {
    /// **Which of the three fits the picture is drawn with**, and [`Aspect::Tv`] by default because
    /// `Aspect`'s own `Default` is — read, not restated, so this window and the game window cannot default
    /// differently. `Square` and `Integer` preserve the pixel grid instead, which is what you want when you
    /// are counting pixels rather than playing.
    ///
    /// Not persisted, deliberately: this is a *looking at it* choice like a zoom, and the dock layout store
    /// is `player.conf`'s open question (recon §3.4). Deciding where it lives before that slice decides the
    /// split is how a setting ends up written by one store and read by the other.
    pub aspect: Aspect,
    /// The standing readout of the last click. **Standing, not a toast**: a toast expires and the fact that
    /// a click armed nothing does not.
    last: Option<Readout>,
    /// Handles of the watches this panel armed, in the order they were armed, so the next click retires
    /// exactly them.
    armed: Vec<String>,
    /// Spawn mode: whether a click places instead of picks, and what it places.
    mode: spawn::Mode,
    /// **The subtypes the selected archetype offers**, read out of the listing's equate table.
    ///
    /// Re-read on every archetype change rather than kept for the whole arm, because it is a fact about
    /// one archetype and holding a set past its own selection is how a picker offers `ObjDef_Ring` the
    /// spring's strengths. Empty for an archetype the listing names no subtypes for, which is an answer
    /// and gets a sentence rather than an empty box.
    subtypes: spawn::Subtypes,
    /// **The armed subtype's whole equate name**, or `None` when there is nothing armable.
    ///
    /// The name and not the value, for `spawn::Mode`'s reason one level up: a re-read under a new listing
    /// must not silently arm whichever subtype inherited a number. The byte a click carries is looked up
    /// from this name each time ([`Panel::subtype_byte`]), so it is always the value the listing
    /// published rather than one this window remembered.
    subtype: Option<String>,
    /// Why the subtype list could not be read, when it could not be.
    ///
    /// P4, kept apart from an empty [`Panel::subtypes`] on purpose: *the listing names none* and *the
    /// window could not ask* are two different findings, and an empty set standing in for a refusal is
    /// the true-sounding sentence about the wrong thing that `bus_stub`'s twin refuses for.
    subtype_refusal: Option<String>,
    /// What the bounded symbol search said the listing holds, from the arm that filled [`Panel::mode`].
    ///
    /// Kept beside the mode rather than inside it because it is a fact about the *search*, not about the
    /// mode: `total > mode.names().len()` is the cut-short case, and a picker that did not carry it would
    /// draw the first 20 of 137 archetypes as though they were all of them.
    total: usize,
    /// What is in the picker's filter box. Panel state rather than model state for the reason the aspect
    /// selector is: it is a way of looking at the list, not a fact about the machine.
    filter: String,
    /// ⚑ **What this window last did to the machine's run state**, or `None` when it has not touched it.
    ///
    /// Standing, and deliberately **not** cleared by disarming the mode: "this window paused your machine
    /// and the resume was refused" is exactly the fact that must not disappear because you turned
    /// something off. Replaced by the next click that places.
    run: Option<spawn_picker::RunState>,
    /// **Which gesture [`Panel::run`] is about.** Two of them pause and restore now, and a run state that
    /// did not carry its own occasion would print an account of a placement after a selection change.
    run_deed: spawn_picker::Deed,
    /// **The picture of the archetype a click would place**, or the stated reason there is not one.
    ///
    /// Retaken once per selection change ([`Panel::take_preview`]), never per frame: it costs a checkpoint
    /// round trip and a handful of emulated frames. See [`crate::preview`] for the whole measurement.
    preview: Option<crate::preview::Outcome>,
}

impl Panel {
    /// The standing spawn badge, or `None` when the mode is off.
    ///
    /// `oracle-frontend`'s rule, carried over verbatim because it is a correctness requirement rather than
    /// decoration: a mode that changes what a left-click **does** must say so for as long as it is on, and
    /// it must name the archetype rather than merely admit to a mode.
    pub fn badge(&self) -> Option<String> {
        self.mode.badge()
    }

    pub fn is_armed(&self) -> bool {
        self.mode.is_armed()
    }

    pub fn readout(&self) -> Option<&Readout> {
        self.last.as_ref()
    }

    /// How many watches this panel currently holds. Shown, so "the click armed nothing" is visible rather
    /// than inferred from a silent tab.
    pub fn armed_count(&self) -> usize {
        self.armed.len()
    }

    /// Arm spawn mode: list the archetypes this build offers and select the first.
    ///
    /// The list is read **now** rather than cached, because `emulator/load_symbols` can replace the listing
    /// at any point and a stale archetype name spawns the wrong thing rather than failing to spawn. Every
    /// failure is the server's own words — `-32012` *you forgot to load symbols* and `-32013` *this build
    /// has no such name* are a distinction a person hits here, and §8.2 keeps them apart on purpose.
    pub fn arm_spawn(&mut self, machine: &mut Machine, bus: &mut Bus) {
        let sys = machine.system_mut();
        let listed = spawn::archetypes(&mut PlayerCaller { bus, sys });
        match listed {
            Ok(a) => {
                let note = a.truncation_note();
                // Kept for the picker, which draws `n of m` and the cut-short note standing rather than
                // once in an arm message that scrolls away.
                self.total = a.total;
                match self.mode.arm(a.names) {
                    Ok(name) => {
                        let mut s = format!("spawn mode armed: a click places {name}");
                        if let Some(n) = note {
                            s.push_str(&format!(" ({n})"));
                        }
                        self.last = Some(Readout::ok(s));
                    }
                    Err(e) => self.last = Some(Readout::refused(e.terminal("(none)", None))),
                }
            }
            Err(e) => self.last = Some(Readout::refused(e.terminal("(none)", None))),
        }
        // A fresh arm re-read the listing, so a filter left over from the last one would hide rows of a
        // list the reader has not seen yet.
        self.filter.clear();
        // ⚑ **Before the picture, not after.** The arm selects the first archetype, so it is a selection
        // change: it owes both the subtypes that archetype offers and a picture of what a click now
        // places. The subtypes come first because the picture is keyed on the armed one.
        self.refresh_subtypes(machine, bus);
        self.take_preview(machine, bus);
    }

    /// Turn spawn mode off. A click picks again.
    ///
    /// [`Panel::run`] is deliberately **not** cleared: what this window did to the machine's run state is
    /// not undone by turning a mode off, and it is the one statement that must outlive the gesture.
    pub fn disarm_spawn(&mut self) {
        self.mode.disarm();
        self.total = 0;
        self.filter.clear();
        // The subtypes go with the mode for the picture's reason: they are the forms of an archetype a
        // click would place, and a click places nothing now.
        self.subtypes = spawn::Subtypes::default();
        self.subtype = None;
        self.subtype_refusal = None;
        // The picture goes with the mode: it is a picture of what a click would place, and a click places
        // nothing now. [`Panel::run`] deliberately does not, and the difference is that one is an answer
        // and the other is what this window did to somebody's machine.
        self.preview = None;
        self.last = Some(Readout::ok(
            "spawn mode off: a click arms a watch again".into(),
        ));
    }

    /// **Select the archetype a click places**, by name, from the picker's rows.
    ///
    /// By name rather than by row number because the rows are a *filtered* view: the row a person clicks
    /// is the `n`th match, not the `n`th archetype. The `None` arm reports rather than swallowing, for
    /// the reason the cycle key it replaces did: a control that silently does nothing is
    /// indistinguishable from a broken one, and here it would also mean the picker is drawing a name the
    /// mode no longer holds.
    pub fn select_archetype(&mut self, machine: &mut Machine, bus: &mut Bus, name: &str) {
        match self.mode.select(name) {
            Some(sel) => {
                let sel = sel.to_string();
                self.last = Some(Readout::ok(format!("a click now places {sel}")));
                // ⚑ The subtypes belong to the archetype, so they are re-read here and the armed one is
                // reset: a subtype name carried across an archetype change would put one object's form
                // on another, and the byte would still be a real byte.
                self.refresh_subtypes(machine, bus);
                // ⚑ **Once per selection change**, which is what makes the cost bearable: a checkpoint
                // round trip and a few emulated frames on a gesture a person makes by hand, never on a
                // frame the loop draws by itself.
                self.take_preview(machine, bus);
            }
            None => {
                self.last = Some(Readout::refused(format!(
                    "{name} is not one of the archetypes spawn mode is holding, so nothing was \
                     selected. Re-arm spawn mode to read the listing again."
                )))
            }
        }
    }

    /// **Re-read the selected archetype's subtypes**, and arm the lowest valued one.
    ///
    /// Called on an arm and on every archetype change, never per frame: it is one bus call, and the thing
    /// it must not do is go stale. The armed subtype is reset here rather than carried, because a name
    /// that survived an archetype change would be this window placing one object's form on another.
    ///
    /// ⚑ **The default is armed and named rather than left unchosen**, which is the safer of the two.
    /// `emulator/object_spawn` composes a subtype byte whether or not one is sent, so a picker with an
    /// unchosen state would be arming a subtype in silence while showing an empty selection. Arming the
    /// lowest and printing which it is places exactly what a click placed before this list existed.
    fn refresh_subtypes(&mut self, machine: &mut Machine, bus: &mut Bus) {
        let Some(archetype) = self.mode.selected().map(str::to_string) else {
            self.subtypes = spawn::Subtypes::default();
            self.subtype = None;
            self.subtype_refusal = None;
            return;
        };
        let all = self.mode.names().to_vec();
        let sys = machine.system_mut();
        match spawn::subtypes(&mut PlayerCaller { bus, sys }, &archetype, &all) {
            Ok(s) => {
                self.subtype = s.default_choice().map(|e| e.name.clone());
                self.subtypes = s;
                self.subtype_refusal = None;
            }
            Err(e) => {
                self.subtype = None;
                self.subtypes = spawn::Subtypes::none_for(&archetype);
                self.subtype_refusal = Some(e.terminal(&archetype, None));
            }
        }
    }

    /// **Arm one subtype**, by its whole equate name, from the rows the picker drew.
    ///
    /// The `None` arm reports rather than swallowing, for [`Panel::select_archetype`]'s reason: a control
    /// that silently does nothing is indistinguishable from a broken one, and here it would also mean the
    /// picker is drawing a name the set no longer holds.
    pub fn select_subtype(&mut self, machine: &mut Machine, bus: &mut Bus, name: &str) {
        let Some(entry) = self.subtypes.get(name) else {
            self.last = Some(Readout::refused(format!(
                "{name} is not one of the subtypes read for {}, so nothing was armed. Choose the \
                 archetype again to read its subtypes afresh.",
                self.subtypes.archetype
            )));
            return;
        };
        let Some(byte) = entry.byte() else {
            self.last = Some(Readout::refused(format!(
                "{name} was not armed: this listing gives it the value {}, and a placement carries \
                 one byte of subtype. Sending it cut down to a byte would place a different form \
                 than the row names.",
                entry.value
            )));
            return;
        };
        let short = entry.short(self.subtypes.prefix.as_deref().unwrap_or_default());
        self.last = Some(Readout::ok(format!(
            "a click now places {} as {short} (${byte:02X})",
            self.subtypes.archetype
        )));
        self.subtype = Some(name.to_string());
        // ⚑ A subtype change is a selection change and owes a picture of what it now places: the whole
        // point of choosing a strength is that it looks different, and a preview left over from the
        // previous one would be a wrong answer with the panel's authority behind it. The key carries the
        // subtype, so the guard in [`Panel::preview`] catches the stale one either way.
        self.take_preview(machine, bus);
    }

    /// **The byte a click carries**, looked up from the armed name every time rather than remembered.
    pub fn subtype_byte(&self) -> Option<u8> {
        self.subtype
            .as_deref()
            .and_then(|n| self.subtypes.get(n))
            .and_then(spawn::Subtype::byte)
    }

    /// **The subtype picker's surface**, or the stated reason it could not be read (P4).
    pub fn subtype_listing(&self) -> Result<spawn_picker::SubtypeListing, &str> {
        match &self.subtype_refusal {
            Some(why) => Err(why.as_str()),
            None => Ok(spawn_picker::subtype_listing(
                &self.subtypes,
                self.subtype.as_deref(),
            )),
        }
    }

    /// **The picker's rows, filtered**, for [`crate::ui`] to lay out and for nothing else to decide.
    pub fn listing(&self) -> spawn_picker::Listing {
        spawn_picker::listing(
            self.mode.names(),
            self.mode.selected(),
            self.total,
            &self.filter,
        )
    }

    /// The filter box's own text. `&mut` because egui's `TextEdit` writes it in place.
    pub fn filter_mut(&mut self) -> &mut String {
        &mut self.filter
    }

    /// ⚑ **What this window last did to the machine's run state**, or `None` when it has not touched it.
    pub fn run_state(&self) -> Option<&spawn_picker::RunState> {
        self.run.as_ref()
    }

    /// **The standing run-state line and whether it is the alarming kind**, for the gesture that caused
    /// it.
    ///
    /// The renderer takes both from here rather than composing the sentence itself, because the deed is
    /// this panel's fact: it knows whether the last pause was a placement or a preview, and a strip that
    /// picked one would be right half the time.
    pub fn run_line(&self) -> Option<(String, bool)> {
        self.run_state()
            .map(|r| (r.sentence_of(self.run_deed), r.alarming()))
    }

    /// **The picture of the archetype a click would place**, or the stated reason there is not one.
    ///
    /// ⚑ **A picture of some other archetype is not returned at all.** The selection can move while a
    /// picture is being kept, and a picture of the previous one drawn under a badge naming the current one
    /// is a wrong answer with the panel's whole authority behind it. The guard is here rather than at the
    /// two draw sites so neither can be the one that forgets it.
    pub fn preview(&self) -> Option<&crate::preview::Outcome> {
        let sel = self.mode.selected()?;
        let out = self.preview.as_ref()?;
        match out {
            crate::preview::Outcome::Ready(p) | crate::preview::Outcome::Stale(p)
                if !p.is_of(sel, self.subtype_byte().map(u32::from)) =>
            {
                None
            }
            _ => Some(out),
        }
    }

    /// ⚑ **Retire the picture the moment the art under it is replaced.**
    ///
    /// Cheap and per frame: [`crate::preview::fingerprint`] over the colour table and the handful of tiles
    /// the picture was drawn from. An act change moves both, so the picture is retired the frame the new
    /// act's art lands, and the panel says so instead of drawing a real picture of the wrong tiles.
    ///
    /// **Retired rather than retaken.** Retaking here would put a checkpoint round trip inside a draw pass
    /// and, worse, would run one on **every** frame of a load, because the fingerprint is moving the whole
    /// time the art is arriving. So this only ever moves a picture to
    /// [`Outcome::Stale`](crate::preview::Outcome::Stale), which is not drawable, and the person takes it
    /// again with the button the panel offers.
    pub fn expire_preview(&mut self, vdp: &oracle_core::vdp::Vdp) {
        let stale = matches!(
            &self.preview,
            Some(crate::preview::Outcome::Ready(p)) if !p.still_current(vdp)
        );
        if stale {
            if let Some(crate::preview::Outcome::Ready(p)) = self.preview.take() {
                self.preview = Some(crate::preview::Outcome::Stale(p));
            }
        }
    }

    /// **Take a picture of the archetype a click would place**, by putting one into the machine, reading
    /// the sprites it drew, and putting the machine back.
    ///
    /// The whole measurement is [`crate::preview`]'s; this is the choreography and the run-state
    /// accounting. It is called on an arm and on a selection change, and by the panel's own button, and
    /// never per frame.
    ///
    /// # ⚑ How the measurement's own frames are kept off the glass
    ///
    /// They cannot reach it, and that is structural rather than a promise this function makes. Two paths
    /// write the picture the window shows: [`Machine::step`], which is not running inside a draw pass, and
    /// [`crate::bus::drain`]'s `adopt_frame`, which is gated on [`Bus::framebuffer`] holding a whole frame.
    /// `emulator/restore` calls the engine's `invalidate_screen`, which drops the latched frame outright,
    /// so by the time the drain looks there is nothing to adopt and the retained picture stays up exactly
    /// as it does for an iteration that emulated nothing. **The last thing this function does to the
    /// machine is a restore**, in every path including every refusal, which is what makes that hold.
    pub fn take_preview(&mut self, machine: &mut Machine, bus: &mut Bus) {
        let Some(archetype) = self.mode.selected().map(str::to_string) else {
            self.preview = None;
            return;
        };
        let dot = preview_dot(machine);
        let subtype = self.subtype_byte();
        match paused_for(machine, bus, |m, b| measure(m, b, &archetype, subtype, dot)) {
            Ok(((out, not_put_back), mut run)) => {
                // ⚑ **`Some(0)`, and it is the whole point rather than a placeholder.** Frames really did
                // run, and the restore put every one of them back, so the emulated time this cost the
                // machine is zero. `None` would say "a number this window did not read", which is the one
                // thing that is not true here.
                if let spawn_picker::RunState::Restored { frames } = &mut run {
                    *frames = Some(0);
                }
                self.run = Some(run);
                self.run_deed = spawn_picker::PREVIEWING;
                // ⚑ **A refused restore is the loud one.** The machine is then carrying a probe object and
                // the frames the probe ran, neither of which the person asked for, and nothing else in
                // this window will mention it. It goes in the standing readout card, coloured on the
                // field, so it survives the gesture that caused it.
                if let Some(alarm) = not_put_back {
                    self.last = Some(Readout::refused(alarm));
                }
                self.preview = Some(out);
            }
            Err(why) => {
                self.run = None;
                self.preview = Some(crate::preview::Outcome::Absent(format!(
                    "the window could not pause the machine to look at {archetype}, so no picture \
                     of it was taken. {why}"
                )));
            }
        }
    }

    /// Show or hide one display layer, **through the served method** `emulator/set_layer_enabled`.
    ///
    /// The four checkboxes this backs are generated from the core's own [`LayerMask::targets`], so this
    /// window cannot offer a layer the bus lacks and cannot spell one differently — the derivation
    /// `oracle-frontend`'s four `ToggleLayer` palette rows already use, which is what lets those four close
    /// with this slice rather than being re-typed here.
    ///
    /// It goes through [`Bus::call`] rather than through `Host::set_layer_enabled` for D15's reason and for
    /// one more: there is exactly one mask, it lives on the engine, and a window that moved it by any other
    /// door would be a second writer to a field a socket client also writes. The tool's own refusal is what
    /// is shown when it refuses.
    pub fn set_layer(&mut self, machine: &mut Machine, bus: &mut Bus, layer: &str, enabled: bool) {
        let sys = machine.system_mut();
        match bus.call(
            sys,
            "emulator/set_layer_enabled",
            &json!({"layer": layer, "enabled": enabled}),
        ) {
            Answer::Ok(_) => {
                self.last = Some(Readout::ok(format!(
                    "{layer} is now {}",
                    if enabled { "shown" } else { "HIDDEN" }
                )));
            }
            // The server's own words, whole — this window's second opinion about a server it lives inside
            // is the one thing a refusal must not become.
            Answer::Err(e) => {
                self.last = Some(Readout::refused(format!(
                    "hiding {layer} was refused. {} {}",
                    e.code, e.message
                )))
            }
        }
    }

    /// **The click.** Spawn mode takes it if armed, otherwise it is a watch pick.
    ///
    /// The branch is here, in front of the pick, exactly as `oracle-frontend`'s run loop puts it there:
    /// the two are the same gesture and only one of them can have it — which is precisely why the mode owes
    /// a standing statement that it is on ([`Panel::badge`]).
    ///
    /// `glass` is **the mask the picture on screen was drawn with** — see [`Panel::pick`].
    pub fn click(
        &mut self,
        machine: &mut Machine,
        bus: &mut Bus,
        glass: Option<LayerMask>,
        dot: (u16, u16),
    ) {
        match self.mode.selected().map(str::to_string) {
            Some(archetype) => self.place(machine, bus, &archetype, dot),
            None => self.pick(machine, bus, glass, dot),
        }
    }

    /// The refusal a click gets when the picture on the glass and the bus's mask are not the same mask.
    ///
    /// **Both masks are read off the values themselves**, never listed here, so this cannot name a layer the
    /// bus does not have — the same derivation `pick::resolve`'s own mask clause and the frontend's layer
    /// badge read. `None` for the glass is the honest *"there is no picture yet"* case rather than a fourth
    /// spelling of "unmasked".
    fn glass_disagrees(glass: Option<LayerMask>, bus_mask: LayerMask) -> String {
        let drawn = match glass {
            Some(m) => format!("the picture on screen was drawn with {}", describe_mask(m)),
            None => "there is no picture on screen yet".to_string(),
        };
        format!(
            "{drawn}, but the machine's mask is now {}, so nothing on this glass is the picture that \
             answer would be about. Nothing was armed. This clears itself on the next frame; if it does \
             not, the masked re-render is failing and the picture you are looking at is not the one the \
             bus is describing.",
            describe_mask(bus_mask)
        )
    }

    /// Resolve the dot, retire this panel's watches, and arm what the dot names.
    ///
    /// ⚑ **`glass` is the mask the picture was drawn with, and it is a parameter for the same reason
    /// `pick::resolve`'s `mask` and `now_mclk` are: this panel describes a picture it did not make.** The
    /// caller reads it off the uploaded texture rather than off the bus, because *what is on the glass* and
    /// *what the machine has been told* are two different facts and the gap between them is exactly what
    /// must be refused on rather than papered over. On every ordinary frame they are equal, and this reads
    /// as it always did.
    fn pick(
        &mut self,
        machine: &mut Machine,
        bus: &mut Bus,
        glass: Option<LayerMask>,
        dot: (u16, u16),
    ) {
        let (x, y) = dot;
        // ⚑ THE GATE, read before anything is resolved or retired, so a refused click leaves the
        // previously armed watches exactly where they were rather than half-clearing them.
        let bus_mask = bus.layers();
        if glass != Some(bus_mask) {
            self.last = Some(Readout::refused(Self::glass_disagrees(glass, bus_mask)));
            return;
        }
        let mask = bus_mask;

        let sys = machine.system_mut();
        // The machine's **now**, which §11.27's colour-staleness rule compares a CRAM write stamp against.
        // `sys.scheduler().now()` — the same instant `emulator/pixel_attribution` stamps its verdict with —
        // and deliberately NOT `Vdp::now_mclk`, which is the instant the VDP last did guest-driven work and
        // on a paused machine can be arbitrarily stale.
        let now = sys.scheduler().now();
        // The mask is the engine's own — never `LayerMask::ALL`, and never a second one assembled here.
        // The gate above has established that it is also the mask the picture on the glass was drawn with,
        // which is what makes "the panel describes the picture" an assertion rather than a hope.
        let p = pick::resolve(sys.vdp(), x, y, mask, now);

        // Retire only what THIS panel armed. `{all: true}` would take a socket client's watches with it —
        // the shared-instrument hazard, and the reason "a click replaces the prior watch" needs a list
        // rather than a reset.
        //
        // ⚑ **The one place in this module where a refusal is deliberately not a sentence**, and it is
        // worth saying why given the rule everywhere else. The only way this refuses is a handle the engine
        // no longer holds — which happens when a socket client cleared it first, and that is exactly the
        // outcome being asked for. Reporting "could not retire a watch that is already gone" would be noise
        // on the one line the person is reading for the pick's answer. The handle leaves our list either
        // way, which is the state that matters.
        for handle in std::mem::take(&mut self.armed) {
            let _ = bus.call(sys, "emulator/watchpoint_clear", &json!({"watch": handle}));
        }

        let mut refusal: Option<String> = None;
        for t in &p.targets {
            let space = match t.space {
                pick::Space::Vram => "vram",
                pick::Space::Cram => "cram",
            };
            let params = json!({
                "space": space,
                "addr": format!("0x{:08X}", t.lo),
                "len": u64::from(t.hi - t.lo) + 1,
                "write": true,
                "label": t.label,
            });
            match bus.call(sys, "emulator/watchpoint_add", &params) {
                Answer::Ok(v) => {
                    if let Some(h) = v["watch"].as_str() {
                        self.armed.push(h.to_string());
                    }
                }
                Answer::Err(e) => {
                    // **The tool's own words, whole.** The cap refusal (`watchCapReached`) is the one this
                    // will actually hit, and it already names the number and the way out; a sentence
                    // composed here would be this window's second opinion about a server it lives inside.
                    let reason = match e.data.as_ref().and_then(|d| d["reason"].as_str()) {
                        Some(r) => format!(" [{r}]"),
                        None => String::new(),
                    };
                    refusal = Some(format!(
                        "the pick resolved but arming it was refused. {} {}{reason}",
                        e.code, e.message
                    ));
                    break;
                }
            }
        }

        // `p.headline` is the sentence a person reads, composed by `pick` and carrying the colour
        // caveat when it applies; `p.detail` is the addressing behind it. Both come from `pick` already
        // separated, so this panel is laying out parts rather than cutting up a paragraph.
        //
        // The outcome is a part of its own rather than a substitution, because "nothing was armed" is a
        // real result (a backdrop with no writable entry) and a description alone would not show it.
        self.last = Some(Readout {
            head: p.headline.clone(),
            detail: Some(p.detail.clone()),
            outcome: Some(match &refusal {
                Some(r) => r.clone(),
                None => format!(
                    "{} watch{} armed by this click",
                    self.armed.len(),
                    if self.armed.len() == 1 { "" } else { "es" }
                ),
            }),
            refused: refusal.is_some() || self.armed.is_empty(),
        });
    }

    /// **The click that places, and the pause it takes to make that legal.**
    ///
    /// Ruled in `docs/2026-09-05-spawn-autopause-design.md` before it was built, from the owner's ask:
    /// *"can the click of the object pause for 1 ms or whatever is needed and spawn it? like
    /// programaticallyy instead of me needing to manually pause."* The measured answer is at most two
    /// frames of emulated time (`OBJREQ_DEFAULT_MAX_FRAMES`), roughly three to six milliseconds of wall
    /// time. His instinct was right and the figure is a couple of frames rather than a millisecond.
    ///
    /// # The three steps, and the middle one is not the interesting one
    ///
    /// 1. **`emulator/pause`, explicitly, through the same served method a socket client would call**, and
    ///    the reply's `wasRunning` is the capture. It is the server's own account of the state it just
    ///    changed, which is a stronger source than anything this window could infer afterwards.
    /// 2. The choreography, unchanged, on a machine that genuinely is paused. It is `oracle-frontend`'s
    ///    one implementation and nothing about it is copied here.
    /// 3. **`emulator/resume`, but only if step 1 found the machine running.**
    ///
    /// # ⚑ The hazard is the resume, not the hitch
    ///
    /// A blind resume starts a machine somebody deliberately stopped. Two real cases: the owner pauses to
    /// line up a placement and the window starts the game under him; or an attached client paused the
    /// machine to read it and the window resumes under the client mid read. **The second is worse because
    /// nothing announces it** and it breaks another actor's invariant rather than a person's expectation.
    /// Built this way, a client-paused machine needs no pause and no resume at all, which also answers
    /// what a mid-read client sees: nothing.
    ///
    /// # What the run-control rule actually binds
    ///
    /// `protocol.md` §6: the named methods, *called while free-running*, MUST fail with `-32005` and never
    /// pause implicitly. **It binds what a method does when called.** A window that pauses explicitly,
    /// calls the method on a machine that genuinely is paused, and then restores what it found satisfies
    /// it literally: no method paused implicitly, and the machine really was paused for the spawn.
    ///
    /// # A side effect worth having rather than tolerating
    ///
    /// The pause now wraps the **whole** choreography rather than only the mailbox write, so the world
    /// join and the act-bounds read happen on a machine that is not moving under them. Before this, the
    /// camera and the level extent were read off a running machine and the placement happened on a paused
    /// one, which is two frames' worth of "the coordinates were for a picture that had already changed".
    ///
    /// Every path through here ends in a [`Readout`] **and** in a [`spawn_picker::RunState`], because a
    /// window that moves the run state without saying so is read as the machine's own state, which is the
    /// lesson the mask statement and the lens episode both paid for.
    fn place(&mut self, machine: &mut Machine, bus: &mut Bus, archetype: &str, dot: (u16, u16)) {
        let remedy = pause_remedy();
        // ⚑ **The byte the listing gave**, read back out of the armed name rather than derived from it.
        // Nothing in this window computes a subtype from a spelling.
        let subtype = self.subtype_byte();
        let (placed, mut run) = match paused_for(machine, bus, |machine, bus| {
            let sys = machine.system_mut();
            spawn::place(&mut PlayerCaller { bus, sys }, archetype, subtype, dot)
        }) {
            Ok(both) => both,
            // The window never got as far as touching the run state, so there is nothing to restore and
            // nothing to say about it. The server's own words are the whole of the answer.
            Err(why) => {
                self.run = None;
                self.last = Some(Readout::refused(format!(
                    "the window could not pause the machine to place {archetype}, so nothing was \
                     placed. {why}"
                )));
                return;
            }
        };
        // The server's own count of the frames its handshake took, filled in here because
        // [`paused_for`] does not know what its body did. `None` when the spawn was refused before any
        // frame ran: a number this window did not read is not a zero.
        if let spawn_picker::RunState::Restored { frames } = &mut run {
            *frames = placed.as_ref().ok().map(|p| p.frames_advanced);
        }
        self.run = Some(run);
        // ⚑ **The deed comes back with the run state.** Arming and selecting also pause and restore now,
        // so leaving this alone would make a placement print the preview's account of itself. The pair is
        // written in one place for the reason `Machine::adopt_system` is one method: two lines that must
        // not be separated should not be separable.
        self.run_deed = spawn_picker::PLACING;

        let outcome = self.run.as_ref().map(spawn_picker::RunState::sentence);
        self.last = Some(match placed {
            Ok(p) => Readout {
                head: p.terminal(archetype),
                detail: None,
                outcome,
                refused: false,
            },
            Err(e) => Readout {
                head: e.terminal(archetype, Some(&remedy)),
                detail: None,
                outcome,
                refused: true,
            },
        });
    }
}

// -------------------------------------------------------------------------------------------------------
// ⚑ The preview measurement: two runs from one checkpoint, and the machine put back after both
// -------------------------------------------------------------------------------------------------------

/// The label the probe's checkpoint carries, so a person listing checkpoints on the socket sees whose it is
/// rather than an anonymous slot that appeared while they were not looking.
///
/// It is dropped in every path, including every refusal, so it should never be listable at all. The label
/// is for the window in which it is.
const PREVIEW_LABEL: &str = "oracle window: taking an object preview";

/// **Where the probe object is put**, as a screen dot.
///
/// It has to be **on camera**, which is the one thing the obvious answer gets wrong: an object placed
/// outside the view does not put sprites on the table, so a probe hidden off screen measures nothing at
/// all. It does not have to avoid anything on screen, because the difference isolates the object's own
/// sprites whether or not they overlap something, and the picture is assembled from the tiles rather than
/// cropped out of the frame.
///
/// A quarter of the way down the middle, because that is usually air: the camera keeps the player near the
/// middle of the picture, so a probe there is least likely to be picked up, sprung or squashed inside the
/// one or two frames it lives for. When it is, the measurement says nothing appeared rather than guessing.
///
/// Read off the machine's own active display, so an H32 act gets an H32 dot instead of a constant that is
/// off the right edge of it.
fn preview_dot(machine: &Machine) -> (u16, u16) {
    let (w, h) = machine.system().vdp().active_display();
    (w / 2, h / 4)
}

/// The sprites the video chip would walk, off this machine, now.
fn live_sprites(machine: &Machine) -> Vec<oracle_core::render::SpriteDecoded> {
    let vdp = machine.system().vdp();
    crate::preview::walk(&vdp.sprites_decoded(), vdp.parsed_sprite_max())
}

/// Put the machine back on the checkpoint. `Some` is the server's own refusal, and it means the machine is
/// **not** where the person left it.
fn restore_to(machine: &mut Machine, bus: &mut Bus, id: &str) -> Option<String> {
    match bus.call(machine.system_mut(), "emulator/restore", &json!({"id": id})) {
        Answer::Ok(_) => None,
        Answer::Err(e) => Some(format!(
            "THE MACHINE WAS NOT PUT BACK. This window checkpointed it to take a picture of an \
             object, ran it forward and could not restore it, so it is carrying an object nobody \
             placed and a few frames nobody asked for: {} {}",
            e.code, e.message
        )),
    }
}

/// Drop the probe's checkpoint. The answer is deliberately not read: a slot that will not drop is a leak
/// the person can clear from the socket, and it is not worth a second sentence in front of the one above.
fn drop_checkpoint(machine: &mut Machine, bus: &mut Bus, id: &str) {
    let _ = bus.call(
        machine.system_mut(),
        "emulator/checkpoint_drop",
        &json!({"id": id}),
    );
}

/// Advance `n` whole frames through the served method, so the probe and the control run the identical path.
fn run_frames(machine: &mut Machine, bus: &mut Bus, n: u64) -> Option<String> {
    match bus.call(
        machine.system_mut(),
        "emulator/run_frames",
        &json!({"frames": n}),
    ) {
        Answer::Ok(_) => None,
        Answer::Err(e) => Some(format!("{} {}", e.code, e.message)),
    }
}

/// **The measurement.** See [`crate::preview`]'s header for why it needs a control and why the difference
/// is a multiset of shapes.
///
/// Returns the outcome and, separately, the one thing that is not an outcome: **the machine was not put
/// back**. That is louder than "no picture" and belongs where a refusal goes, not where a picture goes.
///
/// The picture is composed from the machine **after** the restore, which is the machine the person is
/// looking at. Composing it from the probe would answer a different question: whether the art was resident
/// in a machine that no longer exists.
///
/// # ⚑ Why the rewind is a wrapper and not four call sites
///
/// The property this rests on is *"the machine is where it was afterwards, on every path"*, and inside one
/// function with five early returns it is **unobservable**: on a fixture whose spawn is refused before a
/// frame runs the machine does not move, so every rewind can be deleted with the suite still green. That
/// is not a hypothetical, it is what the first version of this measured. [`checkpointed`] makes it a
/// property a test can pose directly, with a body that deliberately moves the machine, on the real bus and
/// with no game at all. It is exactly why [`paused_for`] was extracted one function down, and it is the
/// same argument for the same reason.
fn measure(
    machine: &mut Machine,
    bus: &mut Bus,
    archetype: &str,
    subtype: Option<u8>,
    dot: (u16, u16),
) -> (crate::preview::Outcome, Option<String>) {
    use crate::preview::{Key, Outcome};

    let taken = checkpointed(machine, bus, |machine, bus, id| {
        // --- the probe --------------------------------------------------------------------------
        let placed = {
            let sys = machine.system_mut();
            // The probe is the object a click would place, subtype and all: a picture taken of some
            // other form is a picture of the wrong thing, drawn with the panel's whole authority.
            spawn::place(&mut PlayerCaller { bus, sys }, archetype, subtype, dot)
        };
        let advanced = match placed {
            Ok(p) => p.frames_advanced,
            Err(e) => {
                return Reading::Refused(
                    format!(
                        "no picture of {archetype} could be taken, because putting one into the \
                         machine to look at was refused. {}",
                        e.terminal(archetype, Some(&pause_remedy()))
                    ),
                    None,
                )
            }
        };
        if let Some(why) = run_frames(machine, bus, crate::preview::EXTRA_FRAMES) {
            return Reading::Refused(
                format!(
                    "one {archetype} was put into the machine, and the frame it needed to draw \
                     itself was refused, so there was nothing to read. {why}"
                ),
                None,
            );
        }
        let probe = live_sprites(machine);

        // --- back, then the control over the identical number of frames -------------------------
        //
        // ⚑ This rewind is the body's own and not the wrapper's: the control has to start from the same
        // instant the probe did, so it happens in the middle rather than at the end.
        if let Some(alarm) = restore_to(machine, bus, id) {
            return Reading::Refused(
                format!(
                    "no picture of {archetype} was produced, because the machine could not be put \
                     back and this window will not keep running one it has lost its place in."
                ),
                Some(alarm),
            );
        }
        if let Some(why) = run_frames(machine, bus, advanced + crate::preview::EXTRA_FRAMES) {
            return Reading::Refused(
                format!(
                    "the control run this picture is measured against was refused, so what the \
                     object drew cannot be told apart from what the rest of the game drew. {why}"
                ),
                None,
            );
        }
        Reading::Took {
            control: live_sprites(machine),
            probe,
        }
    });

    let (reading, tail_alarm) = match taken {
        Ok(both) => both,
        Err(why) => {
            return (
                Outcome::Absent(format!(
                    "no picture of {archetype} could be taken, because the window could not \
                     checkpoint the machine first and it will not run one forward it cannot put \
                     back. {why}"
                )),
                None,
            )
        }
    };
    let (reading, body_alarm) = reading.split();
    // ⚑ **The machine is not where it was, and that outranks everything else this function has to say.**
    let alarm = body_alarm.or(tail_alarm);
    if alarm.is_some() {
        return (
            Outcome::Absent(format!(
                "no picture of {archetype} is shown, because the machine is not where it was and \
                 that is the thing to deal with first."
            )),
            alarm,
        );
    }
    let (probe, control) = match reading {
        Ok(pair) => pair,
        Err(why) => return (Outcome::Absent(why), None),
    };

    // The machine has been put back by now, on every path, so the picture is composed from the machine the
    // person is looking at. Composing it from the probe would answer a different question: whether the art
    // was resident in a machine that no longer exists.
    let key = Key {
        archetype: archetype.to_string(),
        subtype: subtype.map(u32::from),
    };
    let out = match crate::preview::appeared(&control, &probe) {
        Err(why) => Outcome::Absent(why),
        Ok(entries) => match crate::preview::compose(&entries, dot, machine.system().vdp(), key) {
            Ok(p) => Outcome::Ready(Box::new(p)),
            Err(why) => Outcome::Absent(why),
        },
    };
    (out, None)
}

/// What [`checkpointed`]'s body came back with: the two sprite lists a picture is the difference of, or
/// the stated reason there are not two.
///
/// The refusal arm carries its own not-put-back alarm, because one of the ways the body gives up **is** a
/// refused rewind and that is a different kind of fact from "no picture".
enum Reading {
    Took {
        probe: Vec<oracle_core::render::SpriteDecoded>,
        control: Vec<oracle_core::render::SpriteDecoded>,
    },
    Refused(String, Option<String>),
}

type Lists = Vec<oracle_core::render::SpriteDecoded>;

impl Reading {
    /// The reading and the alarm, apart, so the caller handles the two facts in the order they matter in.
    #[allow(clippy::type_complexity)]
    fn split(self) -> (Result<(Lists, Lists), String>, Option<String>) {
        match self {
            Self::Took { probe, control } => (Ok((probe, control)), None),
            Self::Refused(why, alarm) => (Err(why), alarm),
        }
    }
}

/// **Take a checkpoint, run `body`, and put the machine back on it whatever `body` did or said.**
///
/// The wrapper exists so *"the machine is where it was afterwards"* is a property rather than a habit. It
/// is [`paused_for`]'s twin one layer down: that one owns the run state, this one owns the timeline, and
/// both were extracted for the same stated reason, which is that a discipline spread over five early
/// returns is a discipline nothing can witness.
///
/// `Err` is the checkpoint itself being refused, carrying the server's own words: **nothing was tried on
/// the machine**, because this window will not run one forward it cannot put back. The `Option<String>` in
/// the success arm is the restore being refused, which means the machine is **not** where it was and is
/// the loudest thing this whole surface can report.
///
/// The checkpoint is dropped on every path. A slot left behind is a whole machine's worth of memory per
/// selection change, and it would eventually fill the server's own cap and start refusing a client's.
fn checkpointed<T>(
    machine: &mut Machine,
    bus: &mut Bus,
    body: impl FnOnce(&mut Machine, &mut Bus, &str) -> T,
) -> Result<(T, Option<String>), String> {
    let id = match bus.call(
        machine.system_mut(),
        "emulator/checkpoint",
        &json!({"label": PREVIEW_LABEL}),
    ) {
        Answer::Ok(v) => match v["id"].as_str() {
            Some(s) => s.to_string(),
            None => {
                return Err("the answer carried no checkpoint id, so nothing was tried.".to_string())
            }
        },
        Answer::Err(e) => return Err(format!("{} {}", e.code, e.message)),
    };
    let value = body(machine, bus, &id);
    let alarm = restore_to(machine, bus, &id);
    drop_checkpoint(machine, bus, &id);
    Ok((value, alarm))
}

/// **Run `body` on a machine that genuinely is paused, and put the run state back the way it was found.**
///
/// Separated from [`Panel::place`] for one reason and it is not tidiness: *"the machine really was paused
/// while the body ran"* is the property the whole design rests on, and inside a single function it is
/// unobservable. As a function taking a closure it is a property a test can assert directly, on the real
/// bus, without a listing, a mailbox or a game
/// (`the_body_runs_on_a_machine_that_is_genuinely_paused_and_the_run_state_is_put_back`).
///
/// `Err` is the pause itself being refused, carrying the server's own `code` and `message` and nothing of
/// ours: the body never ran and the run state was never touched, so there is nothing to restore and
/// nothing to say about it.
///
/// The returned [`spawn_picker::RunState`] is complete except for `Restored { frames }`, which is left
/// `None` because only the caller knows what its body found out.
fn paused_for<T>(
    machine: &mut Machine,
    bus: &mut Bus,
    body: impl FnOnce(&mut Machine, &mut Bus) -> T,
) -> Result<(T, spawn_picker::RunState), String> {
    // ⚑ **The capture is the server's own `wasRunning`**, from the same served method a socket client
    // would call. It is the server's account of the state it just changed, which is a stronger source
    // than anything this window could infer afterwards, and `None` (a reply with no such flag) is
    // treated as "do not resume" rather than guessed either way.
    let was_running = match bus.call(machine.system_mut(), crate::ui::PAUSE, &json!({})) {
        Answer::Ok(v) => v["wasRunning"].as_bool(),
        Answer::Err(e) => return Err(format!("{} {}", e.code, e.message)),
    };

    let value = body(machine, bus);

    // ⚑ **Resume ONLY if it was running when the click arrived.** A blind resume starts a machine
    // somebody deliberately stopped: the owner pausing to line up a placement, or an attached client
    // pausing to read. The second is worse because nothing announces it.
    let run = match was_running {
        Some(true) => match bus.call(machine.system_mut(), crate::ui::RESUME, &json!({})) {
            Answer::Ok(_) => spawn_picker::RunState::Restored { frames: None },
            Answer::Err(e) => spawn_picker::RunState::Stranded {
                why: format!("{} {}", e.code, e.message),
            },
        },
        Some(false) => spawn_picker::RunState::AlreadyPaused,
        None => spawn_picker::RunState::PriorStateUnknown,
    };
    Ok((value, run))
}

/// ⚑ **The standing alarm that the picture on the glass is not the machine's masked picture**, or `None`
/// when there is nothing to be wrong about.
///
/// The tab's half of the same fact [`Panel::pick`] refuses on, and it is a separate function so the two
/// cannot answer differently about one frame — the alarm and the refusal must appear together or a person
/// gets one without the other.
///
/// **`glass == None` is `None` here and a REFUSAL there, and that asymmetry is deliberate.** With no
/// picture uploaded the tab draws *"no frame yet"* and there is no picture below the alarm for it to be
/// about; an alarm there would fire on a freshly launched player, before its first frame, with an empty
/// mask in it — a false alarm on the loudest surface this tab has, which is the fastest way to teach a
/// reader to ignore it. A *click* in that state is a different question and still has no honest answer,
/// so it is still refused.
pub fn glass_alarm(glass: Option<LayerMask>, bus_mask: LayerMask) -> Option<String> {
    let drawn = glass?;
    if drawn == bus_mask {
        return None;
    }
    Some(format!(
        "THE PICTURE BELOW IS NOT THE MACHINE'S PICTURE. It was drawn with {}, and the machine's mask \
         is now {}. Nothing here can be read as the machine's view until the next frame.",
        describe_mask(drawn),
        describe_mask(bus_mask)
    ))
}

/// A mask in one short phrase, for a sentence that has to name two of them without a reader having to
/// diff two lists. One derivation, so a refusal cannot describe the same mask two ways.
fn describe_mask(m: LayerMask) -> String {
    let hidden = m.hidden();
    if hidden.is_empty() {
        "every layer shown".to_string()
    } else {
        format!("{} hidden", hidden.join(" + "))
    }
}

/// ⚑ **The standing statement that a display mask is on**, or `None` when every layer is drawn.
///
/// This is drawn on **every** frame the mask is non-default and on none where it is not. It is a
/// correctness requirement rather than decoration, and the argument is `oracle-frontend`'s layer badge's,
/// carried over word for word because the failure it prevents is the same one in a different window:
///
/// * **A mask changes what the picture *is*.** With no standing statement, the person who set it will
///   forget, and then read a masked picture as the machine's — which is worse than not having the toggle,
///   because a wrong picture that looks right is indistinguishable from a right one.
/// * **A toast cannot carry this.** Toasts expire; the mask does not.
/// * **It names the hidden layers rather than admitting to a mask**, because *"something is hidden"* sends
///   a reader hunting and *"planeB is hidden"* does not. The names are [`LayerMask::hidden`]'s, which is
///   the same derivation the wire's caveat and `pick`'s clause use — so this cannot name a layer the mask
///   does not hide, and cannot miss one.
/// * **And it names the second thing the mask does**, which nobody asked for and which is invisible until
///   it bites: the masked picture is a post-hoc re-render of current VDP state, so mid-frame palette
///   effects are gone from it. `emulator/screenshot` announces the same trade as `source: "stateRender"`.
pub fn mask_statement(mask: LayerMask) -> Option<String> {
    let hidden = mask.hidden();
    if hidden.is_empty() {
        return None;
    }
    Some(format!(
        "HIDDEN: {}. This picture is re-rendered from current VDP state, so mid-frame palette effects \
         are not in it",
        hidden.join(" ")
    ))
}

impl Readout {
    /// A one-part answer: a mode change, or anything with no addressing behind it.
    fn ok(head: String) -> Self {
        Readout {
            head,
            detail: None,
            outcome: None,
            refused: false,
        }
    }
    fn refused(head: String) -> Self {
        Readout {
            head,
            detail: None,
            outcome: None,
            refused: true,
        }
    }
}

/// This window supplying [`spawn::Caller`] — the same adapter shape `oracle-frontend`'s `bus.rs` has, over
/// a `Host` this crate hosts differently.
///
/// The choreography itself is not here and must never be copied here: it lives in
/// `oracle-frontend/src/spawn.rs` and both windows call it, so the act-bounds gate and the world join cannot
/// answer one person differently in one window.
struct PlayerCaller<'a> {
    bus: &'a mut Bus,
    sys: &'a mut oracle_core::system::System,
}

impl spawn::Caller for PlayerCaller<'_> {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, spawn::Refusal> {
        match self.bus.call(self.sys, method, &params) {
            Answer::Ok(v) => Ok(v),
            // The server's message is **moved, never rewritten**, and `remedy` is left `None` so
            // `Refusal::remedy`'s reason-keyed table is the only thing that adds one.
            Answer::Err(e) => {
                let reason = e
                    .data
                    .as_ref()
                    .and_then(|d| d["reason"].as_str())
                    .map(str::to_string);
                Err(spawn::Refusal {
                    code: Some(e.code),
                    reason,
                    message: e.message,
                    remedy: None,
                })
            }
        }
    }

    /// Resolved off the engine's own listing every call — `Bus::symbols` reads the `Host`'s table rather
    /// than a copy, so this cannot answer from a listing a `load_symbols` has already replaced.
    fn address_of(&mut self, symbol: &str) -> Option<u32> {
        self.bus.symbols().and_then(|t| t.address_of(symbol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::state_hash::fnv1a_bytes;

    const W: usize = 320;
    const H: usize = 224;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> ERect {
        ERect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    // ---------------------------------------------------------------------------------------------
    // Points vs pixels — §3.1's one named risk
    // ---------------------------------------------------------------------------------------------

    /// **The conversion is invisible at 1.0 and wrong everywhere else**, which is why this row sweeps three
    /// scale factors rather than testing the one a headless harness happens to use.
    ///
    /// The expectations are *derived*: at a picture `k` times the native size in pixels, the dot under a
    /// pointer `n` points from the left edge is `floor(n * ppp / k)`. Nothing here is a measurement copied
    /// back from a run.
    #[test]
    fn the_click_inverse_goes_through_pixels_per_point_at_every_scale() {
        for ppp in [1.0f32, 1.5, 2.0] {
            // A picture drawn at exactly 2 device pixels per game pixel, expressed in points.
            let k = 2.0f32;
            let size_points = Vec2::new(W as f32 * k / ppp, H as f32 * k / ppp);
            let r = ERect::from_min_size(Pos2::new(37.0, 11.0), size_points);
            for n_points in [0.0f32, 5.0, 40.0, 100.0] {
                let pos = Pos2::new(r.min.x + n_points, r.min.y);
                let want = ((n_points * ppp / k).floor() as usize).min(W - 1) as u16;
                assert_eq!(
                    dot_at(r, pos, ppp, W, H),
                    Some((want, 0)),
                    "ppp={ppp} at {n_points} points from the left edge"
                );
            }
        }
    }

    /// A pointer outside the picture is `None`, not a clamped edge dot. A clamp here would arm a watch on
    /// the corner tile every time somebody clicked the letterbox.
    #[test]
    fn a_pointer_off_the_picture_resolves_to_nothing() {
        let r = rect(100.0, 50.0, 320.0, 224.0);
        assert_eq!(dot_at(r, Pos2::new(99.0, 60.0), 1.0, W, H), None, "left");
        assert_eq!(dot_at(r, Pos2::new(110.0, 49.0), 1.0, W, H), None, "above");
        assert_eq!(dot_at(r, Pos2::new(420.0, 60.0), 1.0, W, H), None, "right");
        assert_eq!(dot_at(r, Pos2::new(110.0, 274.0), 1.0, W, H), None, "below");
        // …and the first dot inside each edge still answers, so the row above is a boundary and not a
        // blanket refusal.
        assert_eq!(dot_at(r, Pos2::new(100.0, 50.0), 1.0, W, H), Some((0, 0)));
        assert_eq!(
            dot_at(r, Pos2::new(419.0, 273.0), 1.0, W, H),
            Some((319, 223))
        );
    }

    /// A degenerate `pixels_per_point` answers `None` rather than dividing by zero or naming a dot.
    #[test]
    fn a_degenerate_scale_answers_nothing() {
        let r = rect(0.0, 0.0, 320.0, 224.0);
        assert_eq!(dot_at(r, Pos2::new(10.0, 10.0), 0.0, W, H), None);
        assert_eq!(dot_at(r, Pos2::new(10.0, 10.0), -1.0, W, H), None);
        assert_eq!(
            dot_at(rect(0.0, 0.0, 0.0, 0.0), Pos2::ZERO, 1.0, W, H),
            None
        );
    }

    // ---------------------------------------------------------------------------------------------
    // The fit, and the round trip through it — the property S1's correctness rests on
    // ---------------------------------------------------------------------------------------------

    /// **The identity S2 must not break.** At both native widths, at three scale factors: the centre of
    /// the window span showing game dot `(gx, gy)` inverts back to `(gx, gy)`.
    ///
    /// The forward direction is [`present::native_rect_to_window`], the frontend's own forward map, so this
    /// asserts the two halves of one blit against each other rather than against arithmetic restated here.
    /// The round trip is an **upscale** property (`present.rs` states why), so the panel sizes below are all
    /// comfortably above 1:1.
    ///
    /// S2 extends this row over the three `Aspect` modes. The body is a helper taking the fitted size, so
    /// that extension is a new caller rather than a rewrite of the assertion.
    #[test]
    fn the_fit_and_the_click_inverse_are_inverses() {
        for aspect in [Aspect::Tv, Aspect::Square, Aspect::Integer] {
            for (sw, sh) in [(320usize, 224usize), (256, 224)] {
                for ppp in [1.0f32, 1.5, 2.0] {
                    let avail = Vec2::new(1280.0 / ppp, 900.0 / ppp);
                    let size = fit(avail, sw, sh, ppp, aspect);
                    assert!(
                        size.x > 0.0 && size.y > 0.0,
                        "{aspect:?} {sw}x{sh} ppp={ppp}"
                    );
                    assert_round_trip(size, ppp, sw, sh, &format!("{aspect:?}"));
                }
            }
        }
    }

    /// The body of the identity above, so S2 can run it over three fits without restating it.
    fn assert_round_trip(size: Vec2, ppp: f32, sw: usize, sh: usize, what: &str) {
        let image = ERect::from_min_size(Pos2::new(64.0, 32.0), size);
        // The picture in pixels, exactly as `dot_at` reconstructs it.
        let px = present::Rect {
            x: 0,
            y: 0,
            w: (size.x * ppp).round() as usize,
            h: (size.y * ppp).round() as usize,
        };
        for gx in [0usize, 1, 7, sw / 2, sw - 2, sw - 1] {
            for gy in [0usize, 1, sh / 2, sh - 1] {
                let span = present::native_rect_to_window(
                    present::Rect {
                        x: gx,
                        y: gy,
                        w: 1,
                        h: 1,
                    },
                    px,
                    sw,
                    sh,
                )
                .expect("a dot inside the picture has a span");
                // Centre of the span, back in points and back in screen space.
                let cx = image.min.x + (span.x as f32 + span.w as f32 / 2.0) / ppp;
                let cy = image.min.y + (span.y as f32 + span.h as f32 / 2.0) / ppp;
                assert_eq!(
                    dot_at(image, Pos2::new(cx, cy), ppp, sw, sh),
                    Some((gx as u16, gy as u16)),
                    "{what} {sw}x{sh} ppp={ppp}: game dot ({gx},{gy}) drawn at {span:?}"
                );
            }
        }
    }

    /// A degenerate panel yields no picture rather than a panic — a window manager really can hand out a
    /// zero-height panel mid-resize, and `dest_rect` carries the same rule.
    #[test]
    fn a_degenerate_panel_yields_no_picture() {
        let tv = Aspect::Tv;
        assert_eq!(fit(Vec2::new(0.0, 700.0), W, H, 1.0, tv), Vec2::ZERO);
        assert_eq!(fit(Vec2::new(900.0, 0.0), W, H, 1.0, tv), Vec2::ZERO);
        assert_eq!(fit(Vec2::new(900.0, 700.0), 0, H, 1.0, tv), Vec2::ZERO);
        assert_eq!(fit(Vec2::new(900.0, 700.0), W, 0, 1.0, tv), Vec2::ZERO);
        assert_eq!(fit(Vec2::new(f32::NAN, 700.0), W, H, 1.0, tv), Vec2::ZERO);
        assert_eq!(fit(Vec2::new(900.0, 700.0), W, H, 0.0, tv), Vec2::ZERO);
        assert_eq!(fit(Vec2::new(900.0, 700.0), W, H, f32::NAN, tv), Vec2::ZERO);
    }

    // ---------------------------------------------------------------------------------------------
    // S2 — the three aspect modes
    // ---------------------------------------------------------------------------------------------

    /// **The default is the television one, and the three modes are actually three.**
    ///
    /// The player was showing a square-pixel picture, which is geometrically wrong by the frontend's own
    /// standard for a *player*. The default is read off `Aspect::default()` rather than written as a
    /// literal here, so this window and the game window cannot default differently — the assertion is that
    /// they agree, not that both happen to say `Tv`.
    #[test]
    fn the_default_aspect_is_the_television_one_and_the_modes_differ() {
        assert_eq!(Panel::default().aspect, Aspect::default());
        let avail = Vec2::new(1000.0, 700.0);
        let tv = fit(avail, 320, 224, 1.0, Aspect::Tv);
        let sq = fit(avail, 320, 224, 1.0, Aspect::Square);
        let int = fit(avail, 320, 224, 1.0, Aspect::Integer);
        assert_ne!(tv, sq, "4:3 and square must not be the same picture");
        assert_ne!(sq, int, "a fractional square fit is not an integer one");
        // ⚑ **`Tv` is EXACTLY 4:3 and `Square` is exactly the native ratio** — the two claims, stated as
        // integer identities because `dest_rect` builds both out of whole multiples of a reduced fraction.
        // Note which way round this goes for H40: 320x224 reduces to 10:7 ≈ 1.429, which is *wider* than
        // 4:3 ≈ 1.333, so the television picture is NARROWER than square pixels here and wider than them at
        // H32. "Tv is the wide one" is the plausible wrong version of this row, and it is wrong.
        assert_eq!(
            tv.x as usize * 3,
            tv.y as usize * 4,
            "tv={tv:?} must be an exact 4:3 box"
        );
        assert_eq!(
            sq.x as usize * 7,
            sq.y as usize * 10,
            "sq={sq:?} must be the exact native 320:224 = 10:7 ratio"
        );
        // …and the two orderings, both ways, so this is a measurement of the ratio rather than of one box.
        let sq32 = fit(avail, 256, 224, 1.0, Aspect::Square);
        let tv32 = fit(avail, 256, 224, 1.0, Aspect::Tv);
        assert!(tv.x / tv.y < sq.x / sq.y, "H40: 4:3 is narrower than 10:7");
        assert!(
            tv32.x / tv32.y > sq32.x / sq32.y,
            "H32: 4:3 is wider than 8:7"
        );
        // Integer mode duplicates no row: both axes are whole multiples of the native frame.
        assert_eq!(
            int.x as usize % 320,
            0,
            "integer mode must be whole: {int:?}"
        );
        assert_eq!(
            int.y as usize % 224,
            0,
            "integer mode must be whole: {int:?}"
        );
    }

    /// **H32 is stretched wider, not pillarboxed.** 256 and 320 dots occupy the same 4:3 box, which is the
    /// one thing about `Tv` that a reader is most likely to implement backwards.
    #[test]
    fn h32_and_h40_get_the_same_television_box() {
        let avail = Vec2::new(1000.0, 700.0);
        assert_eq!(
            fit(avail, 320, 224, 1.0, Aspect::Tv),
            fit(avail, 256, 224, 1.0, Aspect::Tv)
        );
        // …and under square pixels they do not, which is what makes the row above a measurement rather
        // than a property of the fixture.
        assert_ne!(
            fit(avail, 320, 224, 1.0, Aspect::Square),
            fit(avail, 256, 224, 1.0, Aspect::Square)
        );
    }

    /// ⚑ **`Integer` is a claim about the PIXEL grid, so it must survive a non-integer `ppp`.**
    ///
    /// This is the row that fails if the fit is ever computed in points: at `ppp = 1.5` a "whole" scale in
    /// points is 1.5x in pixels, which duplicates every other row while the mode's name promises it does
    /// not. The assertion is on the pixel size, reconstructed exactly as [`dot_at`] reconstructs it.
    #[test]
    fn integer_mode_is_whole_in_pixels_not_in_points() {
        for ppp in [1.0f32, 1.25, 1.5, 2.0] {
            for (sw, sh) in [(320usize, 224usize), (256, 224)] {
                let size = fit(
                    Vec2::new(1600.0 / ppp, 1000.0 / ppp),
                    sw,
                    sh,
                    ppp,
                    Aspect::Integer,
                );
                let w_px = (size.x * ppp).round() as usize;
                let h_px = (size.y * ppp).round() as usize;
                assert_eq!(
                    w_px % sw,
                    0,
                    "ppp={ppp} {sw}x{sh}: {w_px} px wide is not whole"
                );
                assert_eq!(
                    h_px % sh,
                    0,
                    "ppp={ppp} {sw}x{sh}: {h_px} px tall is not whole"
                );
                assert_eq!(
                    w_px / sw,
                    h_px / sh,
                    "ppp={ppp}: integer mode scales both axes by ONE factor"
                );
            }
        }
    }

    /// The fit is `present::dest_rect`'s, **not a second implementation of it in this file**.
    ///
    /// Asserted by construction rather than by eye: at `ppp = 1.0` the returned points are the rect's own
    /// pixels, for every mode and both widths. If somebody ever inlines a formula here, this is what
    /// notices.
    #[test]
    fn the_fit_is_the_frontends_own_dest_rect() {
        for aspect in [Aspect::Tv, Aspect::Square, Aspect::Integer] {
            for (sw, sh) in [(320usize, 224usize), (256, 224)] {
                for (w, h) in [(1000usize, 700usize), (640, 480), (1920, 1080)] {
                    let r = present::dest_rect(w, h, sw, sh, aspect);
                    assert_eq!(
                        fit(Vec2::new(w as f32, h as f32), sw, sh, 1.0, aspect),
                        Vec2::new(r.w as f32, r.h as f32),
                        "{aspect:?} {sw}x{sh} in {w}x{h}"
                    );
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // S2a — the standing statement, the layer toggles, and the one thing still refused
    // ---------------------------------------------------------------------------------------------

    /// **The standing statement names the hidden layers, and it reads them off the mask.**
    ///
    /// A sentence with the layer names written into it would pass a "contains planeA" check while being a
    /// claim this window made rather than one the bus supports, which is the whole failure the mask clause
    /// exists to prevent. So the expectation is derived from `LayerMask::hidden()` itself, and the
    /// all-shown control is asserted beside it — an absence needs its control.
    ///
    /// It also has to say the **second** thing a mask does to this window's picture, which nobody asked
    /// for: the masked path is a post-hoc re-render, so mid-frame palette effects are not in it.
    #[test]
    fn the_standing_mask_statement_names_the_layers_the_mask_hides() {
        assert_eq!(
            mask_statement(LayerMask::ALL),
            None,
            "the all-shown control: nothing hidden, so there is nothing to announce"
        );
        assert_eq!(
            mask_statement(LayerMask::default()),
            None,
            "and the default mask is the all-shown one"
        );
        for (target_name, target) in LayerMask::targets() {
            let mut m = LayerMask::ALL;
            assert!(m.set(target, false), "{target_name} must be a mask target");
            let hidden = m.hidden();
            assert!(
                !hidden.is_empty(),
                "hiding {target_name} must hide something"
            );
            let s = mask_statement(m).expect("a set mask must announce itself");
            for name in &hidden {
                assert!(s.contains(name), "the statement must name {name}: {s:?}");
            }
            assert!(
                s.contains("palette"),
                "it must say what the masked path costs, or the picture changes in a second way \
                 nothing on screen explains: {s:?}"
            );
        }
    }

    /// **The standing alarm fires when the glass and the machine hold different masks — and NOT before
    /// the first frame.**
    ///
    /// ⚑ The `None` row is the one this test was written for, and it is a defect the first draft shipped:
    /// with the alarm written as `screen_mask != Some(bus.layers())` it fired on a freshly launched
    /// player, before any picture existed, announcing that a picture that was not there disagreed with an
    /// empty mask. A false alarm on the loudest surface this tab has is how a reader learns to ignore the
    /// real one, so it is asserted absent rather than argued away.
    #[test]
    fn the_standing_alarm_fires_on_a_disagreement_and_never_before_the_first_frame() {
        let mut masked = LayerMask::ALL;
        assert!(masked.set(oracle_core::render::Layer::PlaneA, false));

        // No picture yet: nothing on the glass to be wrong about, under either mask.
        assert_eq!(glass_alarm(None, LayerMask::ALL), None);
        assert_eq!(glass_alarm(None, masked), None);
        // Agreement, masked or not: silence, because silence here is a measurement.
        assert_eq!(glass_alarm(Some(LayerMask::ALL), LayerMask::ALL), None);
        assert_eq!(glass_alarm(Some(masked), masked), None);

        // …and the two disagreements, in both directions.
        for (glass, bus) in [(LayerMask::ALL, masked), (masked, LayerMask::ALL)] {
            let s = glass_alarm(Some(glass), bus).expect("a disagreement must be announced");
            assert!(
                s.contains(&describe_mask(glass)) && s.contains(&describe_mask(bus)),
                "the alarm must name BOTH masks, or a reader cannot tell which is which: {s:?}"
            );
        }
    }

    /// **Every layer toggle goes through the served method and moves the engine's own mask.**
    ///
    /// Swept over [`LayerMask::targets`] rather than over four names written here, which is what makes
    /// this window unable to offer a layer the bus lacks. The read-back is `Bus::layers()`, i.e. the
    /// engine's field — not the panel's memory of what it asked for.
    #[test]
    fn every_layer_toggle_goes_through_the_served_method() {
        for (name, layer) in LayerMask::targets() {
            let (mut machine, mut bus) = rig();
            let mut panel = Panel::default();
            assert!(
                bus.layers().shows(layer),
                "the control: {name} starts shown, or hiding it witnesses nothing"
            );

            panel.set_layer(&mut machine, &mut bus, name, false);
            assert!(
                !bus.layers().shows(layer),
                "the toggle did not reach the engine's mask for {name}"
            );
            assert!(
                bus.layers().hidden().contains(&name),
                "and the mask must name it: {:?}",
                bus.layers().hidden()
            );
            let r = panel.readout().expect("a toggle says what it did");
            assert!(!r.refused, "showing/hiding a real layer is not a refusal");
            assert!(
                r.text().contains(name),
                "it must name the layer: {:?}",
                r.text()
            );

            // …and back, so this is a control rather than a one-way door.
            panel.set_layer(&mut machine, &mut bus, name, true);
            assert!(bus.layers().shows(layer));
            assert!(
                bus.layers().is_all(),
                "restoring one layer must restore the whole mask in this fixture"
            );
        }
    }

    /// ⚑ **The armed subtype is looked up by name every time, and a refused choice does not disarm.**
    ///
    /// The wiring rather than the model. Three things go wrong here and nowhere else:
    ///
    /// * a byte **remembered** instead of looked up would survive a listing change and place a form under
    ///   a name that no longer carries it, which is the stale-archetype hazard one level down;
    /// * a refusal that also cleared the armed subtype would leave the badge and the machine disagreeing
    ///   about what a click does, silently, after a gesture the reader thought did nothing;
    /// * and a subtype the set is not holding must be a **sentence**, for the reason the archetype
    ///   picker's own miss arm is one: a control that quietly does nothing is indistinguishable from a
    ///   broken one.
    #[test]
    fn a_subtype_is_resolved_by_name_and_a_refused_choice_leaves_the_armed_one_alone() {
        let (mut machine, mut bus) = rig();
        let mut panel = Panel {
            subtypes: spawn::Subtypes {
                archetype: "ObjDef_Spring".into(),
                prefix: Some("ObjSub_Spring__".into()),
                entries: vec![
                    spawn::Subtype {
                        name: "ObjSub_Spring__Up_Yellow".into(),
                        value: 0x02,
                    },
                    // The value that cannot be sent, which is the rail no real build exercises today.
                    spawn::Subtype {
                        name: "ObjSub_Spring__Wide_Huge".into(),
                        value: 0x140,
                    },
                ],
                truncated: false,
                collisions: Vec::new(),
            },
            ..Default::default()
        };

        assert_eq!(
            panel.subtype_byte(),
            None,
            "nothing is armed until something is chosen"
        );

        panel.select_subtype(&mut machine, &mut bus, "ObjSub_Spring__Up_Yellow");
        assert_eq!(
            panel.subtype_byte(),
            Some(0x02),
            "the armed byte is the listing's, resolved from the name the row carried"
        );

        // A value that cannot be sent is refused, and the armed one survives the refusal.
        panel.select_subtype(&mut machine, &mut bus, "ObjSub_Spring__Wide_Huge");
        let r = panel.readout().expect("a refused choice owes a sentence");
        assert!(
            r.refused,
            "and it is coloured on the field, never on the prose"
        );
        assert!(
            r.head.contains("320"),
            "the refusal quotes the value the listing gave: {:?}",
            r.head
        );
        assert_eq!(
            panel.subtype_byte(),
            Some(0x02),
            "a refused choice must not disarm the one that was working"
        );

        // A name the set is not holding is the other sentence.
        panel.select_subtype(&mut machine, &mut bus, "ObjSub_Spring__Sideways");
        let r = panel.readout().expect("a miss owes a sentence too");
        assert!(
            r.refused && r.head.contains("ObjSub_Spring__Sideways"),
            "{r:?}",
            r = r.head
        );
        assert_eq!(panel.subtype_byte(), Some(0x02));

        // ⚑ The lookup is by NAME and is redone every time: swap the set for one where the same name
        // carries a different value, and the armed byte moves with the listing rather than with memory.
        panel.subtypes.entries[0].value = 0x52;
        assert_eq!(
            panel.subtype_byte(),
            Some(0x52),
            "the byte is read out of the set on every call, so a rebuilt listing is followed rather \
             than a remembered number being placed"
        );

        // And the projection agrees with what a click would carry.
        let l = panel
            .subtype_listing()
            .expect("nothing refused the read, so there are rows");
        assert!(l.armed.contains("Up_Yellow") && l.armed.contains("$52"));
        assert!(
            l.rows.iter().filter(|r| r.selected).count() == 1,
            "exactly one row is armed"
        );

        // Disarming spawn mode takes the subtypes with it: a click places nothing, so there is no form
        // for it to place. The run state deliberately does not go, and that is a different field.
        panel.disarm_spawn();
        assert_eq!(panel.subtype_byte(), None);
        assert!(panel.subtypes.entries.is_empty());
    }

    /// ⚑ **The readout cites nothing at the reader, carries no dash, and arrives in three parts.**
    ///
    /// P9 and P10 of the style page, plus the reason this parcel touched the readout at all.
    ///
    /// The P9 before-case was **not in this file**, and finding that out is most of what this test is
    /// worth writing down: the `protocol.md §11.3` clause the rule was written from lives in
    /// `oracle_core::render::cram_divergence_caveat`, which composes the colour caveat that
    /// `pick::resolve` folds into the headline. So it is fixed at its source, where
    /// `emulator/pixel_attribution` reads the same sentence, and `render.rs`'s own
    /// `the_caveat_cites_nothing_at_the_reader_and_carries_no_dash` is the gate on the string itself.
    /// **This one is the gate on the string arriving here**, which is the half that would go unnoticed if
    /// the two ever separated.
    ///
    /// Every gesture this panel can produce a readout from is driven, not just the pick: a rule checked on
    /// one path is checked on one path.
    #[test]
    fn every_readout_this_panel_can_show_is_free_of_citations_and_dashes() {
        let (mut machine, mut bus) = rig();
        let mask = bus.layers();
        let mut panel = Panel::default();

        let mut seen: Vec<(&str, String)> = Vec::new();
        let mut take = |what: &'static str, p: &Panel| {
            let r = p.readout().expect("the gesture left a readout");
            seen.push((what, r.text()));
        };

        // 1. A pick that resolves. The fixture's opaque plane-A cell is at (2,2).
        panel.click(&mut machine, &mut bus, Some(mask), (2, 2));
        take("a resolved pick", &panel);
        // …and the pick's parts are three, which is what the tab lays out.
        {
            let r = panel.readout().expect("the pick left a readout");
            assert!(
                r.detail.is_some() && r.outcome.is_some(),
                "a pick's answer is the sentence, the addressing and the outcome, and the tab draws \
                 them at three weights: {r:?}",
                r = (&r.head, &r.detail, &r.outcome)
            );
            assert!(
                !r.head.contains('\n'),
                "the headline is one line; the parts are fields, not a paragraph split later: {:?}",
                r.head
            );
        }

        // 2. A pick on the backdrop, which is the arm that carries the colour caveat's wording.
        panel.click(&mut machine, &mut bus, Some(mask), (60, 60));
        take("a backdrop pick", &panel);

        // 3. A click while the glass and the machine disagree: the refusal path.
        panel.click(&mut machine, &mut bus, None, (2, 2));
        take("a refused click", &panel);

        // 4. A layer toggle, through the served method.
        panel.set_layer(&mut machine, &mut bus, "planeA", false);
        take("a layer toggle", &panel);
        panel.set_layer(&mut machine, &mut bus, "planeA", true);

        // 5. Spawn mode, armed and disarmed.
        panel.arm_spawn(&mut machine, &mut bus);
        take("spawn armed", &panel);
        // The picker's refusal arm: a name the mode is not holding. On this fixture nothing armed, so
        // this is also the "selected while disarmed" case, and it must be a sentence rather than a
        // silent no-op for the reason the cycle key it replaces was.
        panel.select_archetype(&mut machine, &mut bus, "ObjDef_Ring");
        take("an archetype the mode does not hold", &panel);
        panel.disarm_spawn();
        take("spawn off", &panel);

        // 6. The two standing statements the tab draws above the picture.
        let hidden = {
            // Built through the core's own vocabulary rather than from a name typed here, the same
            // derivation the four checkboxes use.
            let (_, layer) = LayerMask::targets()
                .into_iter()
                .find(|(n, _)| *n == "planeA")
                .expect("the core publishes a planeA target");
            let mut m = LayerMask::ALL;
            m.set(layer, false);
            m
        };
        seen.push((
            "the mask statement",
            mask_statement(hidden).expect("a hidden layer produces a statement"),
        ));
        seen.push((
            "the glass alarm",
            glass_alarm(Some(hidden), mask).expect("disagreeing masks produce an alarm"),
        ));

        // 7. ⚑ **Every spawn refusal, and the spawn success line.**
        //
        // `Panel::place` shows `spawn::Refusal::terminal` and `spawn::Placed::terminal` verbatim, so those
        // strings are this readout's whatever crate composed them. Reached by construction rather than by
        // driving a click, because five of the seven are engine refusals a fixture machine will not
        // produce, and a rule checked only on the reachable ones is checked on the reachable ones. This
        // is the gap the first version of this test had: it drove `arm_spawn` on a listing-less fixture,
        // got the one refusal that path yields, and never saw the other six.
        let bounds = spawn::Bounds {
            width: 1024,
            height: 768,
        };
        let remedy = pause_remedy();
        for (what, r) in [
            ("spawn outside the act", bounds.outside(9999, 9999)),
            ("spawn with no act extent", spawn::Bounds::unmeasurable()),
            ("spawn with no act loaded", spawn::Bounds::no_act()),
            (
                "a served spawn refusal",
                spawn::Refusal {
                    code: Some(-32005),
                    reason: Some("machineRunning".into()),
                    message: "emulator/object_spawn needs the machine paused; call emulator/pause \
                              first"
                        .into(),
                    remedy: None,
                },
            ),
        ] {
            seen.push((what, r.terminal("ObjDef_Ring", Some(&remedy))));
        }
        // …and the success line, which is the longest sentence this readout ever shows.
        seen.push((
            "a placed object",
            spawn::Placed {
                handle: "0x8123".into(),
                addr: "0x00FF8123".into(),
                slot: Some(7),
                asked: (100, 200),
                now: (104, 200),
                frames_advanced: 1,
                caveat: None,
            }
            .terminal("ObjDef_Ring"),
        ));

        // The anti-vacuity clause: every gesture above must actually have said something.
        assert_eq!(seen.len(), 14, "a gesture produced no readout: {seen:?}");
        assert!(
            seen.iter().any(|(_, t)| t.contains("planeA")),
            "the collected readouts must include real resolved content: {seen:?}"
        );

        for (what, text) in &seen {
            for bad in ["§", "protocol.md"] {
                assert!(
                    !text.contains(bad),
                    "{what} quotes the specification at the reader ({bad:?}). A section reference is \
                     addressed to somebody holding the contract, and the person at this window is \
                     not:\n{text}"
                );
            }
            for bad in ['\u{2014}', '\u{2013}'] {
                assert!(
                    !text.contains(bad),
                    "{what} carries {bad:?}. Use a comma, a colon, a period or parentheses:\n{text}"
                );
            }
        }
    }

    /// **A click resolves under the mask the picture was drawn with, and it is no longer refused.**
    ///
    /// This is the row that replaces S1's blanket masked-off-only refusal. The fixture's one opaque
    /// plane-A cell is at (2,2); hide plane A, re-derive the picture the way [`crate::bus::drain`] does,
    /// and the same click must now resolve to the **backdrop** — because that is what is on the glass —
    /// and arm a CRAM entry instead of a VRAM pattern.
    ///
    /// The anti-vacuity clause is the unmasked control taken first: without it, "the click armed a CRAM
    /// entry" would be satisfied by a panel that had always armed one.
    #[test]
    fn a_click_resolves_under_the_mask_the_picture_was_drawn_with() {
        let (mut machine, mut bus) = rig();
        let mut panel = Panel::default();

        // The control: unmasked, (2,2) is plane A and the click arms its pattern in VRAM.
        assert!(machine.render_masked(LayerMask::ALL));
        let glass = machine.image_mask();
        panel.click(&mut machine, &mut bus, glass, (2, 2));
        let unmasked = armed_on_the_machine(&mut machine, &mut bus);
        assert_eq!(
            unmasked,
            vec![(
                "vram".to_string(),
                format!("0x{:08X}", u32::from(A_TILE) * 32)
            )],
            "the unmasked control: (2,2) is plane A"
        );

        // Hide plane A through the tool, and re-derive the picture exactly as the drain does.
        {
            let sys = machine.system_mut();
            match bus.call(
                sys,
                "emulator/set_layer_enabled",
                &json!({"layer": "planeA", "enabled": false}),
            ) {
                Answer::Ok(_) => {}
                Answer::Err(e) => panic!("set_layer_enabled refused: {} {}", e.code, e.message),
            }
        }
        let mask = bus.layers();
        assert!(machine.render_masked(mask), "the masked picture must exist");
        assert_eq!(
            machine.image_mask(),
            Some(mask),
            "the glass and the bus must agree, or the click below is refused for the other reason"
        );

        let glass = machine.image_mask();
        panel.click(&mut machine, &mut bus, glass, (2, 2));
        let r = panel.readout().expect("a standing readout");
        assert!(
            !r.refused,
            "a click on a masked picture this window actually drew is answerable: {:?}",
            r.text()
        );
        assert!(
            r.text().contains("backdrop"),
            "with plane A hidden the dot IS the backdrop — the panel must describe the picture: {:?}",
            r.text()
        );
        assert!(
            r.text().contains("planeA"),
            "and it must say the picture is a masked one, naming what is hidden: {:?}",
            r.text()
        );

        let masked = armed_on_the_machine(&mut machine, &mut bus);
        assert_eq!(
            masked,
            vec![(
                "cram".to_string(),
                format!("0x{:08X}", u32::from(BACKDROP_ENTRY) * 2)
            )],
            "the click must arm what the MASKED picture draws that dot from"
        );
        // ⚑ ANTI-VACUITY: a panel that ignored the mask would have armed the plane pattern again.
        assert_ne!(
            unmasked, masked,
            "the mask did not change what the click resolved to"
        );
    }

    /// **The one thing still refused: the glass and the machine holding different masks.**
    ///
    /// Narrow, and reachable — the palette can call `emulator/set_layer_enabled` during the same
    /// `build_ui` that drew the picture, and a masked re-render can fail outright. In that window there is
    /// no honest answer about a dot, so the panel says so rather than describing a picture that is not
    /// there, and it leaves the previously armed watch exactly where it was: the gate is read before
    /// anything is resolved *or retired*.
    #[test]
    fn a_click_is_refused_while_the_glass_and_the_machine_disagree_about_the_mask() {
        let (mut machine, mut bus) = rig();
        let mut panel = Panel::default();
        assert!(machine.render_masked(LayerMask::ALL));
        let glass = machine.image_mask();
        panel.click(&mut machine, &mut bus, glass, (2, 2));
        let before = armed_on_the_machine(&mut machine, &mut bus);
        assert_eq!(before.len(), 1, "the precondition: one watch is armed");

        // The mask moves on the machine; the picture is deliberately NOT re-derived, which is the state
        // between a mid-frame `set_layer_enabled` and the next drain.
        {
            let sys = machine.system_mut();
            match bus.call(
                sys,
                "emulator/set_layer_enabled",
                &json!({"layer": "planeA", "enabled": false}),
            ) {
                Answer::Ok(_) => {}
                Answer::Err(e) => panic!("set_layer_enabled refused: {} {}", e.code, e.message),
            }
        }
        assert_eq!(
            machine.image_mask(),
            Some(LayerMask::ALL),
            "the glass must still be the unmasked picture, or this row measures nothing"
        );
        assert_ne!(machine.image_mask(), Some(bus.layers()));

        let glass = machine.image_mask();
        panel.click(&mut machine, &mut bus, glass, (200, 100));
        assert_eq!(
            armed_on_the_machine(&mut machine, &mut bus),
            before,
            "a refused click must leave the instrument untouched"
        );
        let r = panel
            .readout()
            .expect("a refusal is a sentence, never silence");
        assert!(r.refused, "and it must be marked as one: {:?}", r.text());
        assert!(
            r.text().contains("planeA"),
            "it must name what the machine now hides: {:?}",
            r.text()
        );
        assert!(
            r.text().contains("every layer shown"),
            "…and what the glass was drawn with, or a reader cannot tell which is which: {:?}",
            r.text()
        );

        // …and once the picture catches up, the same click answers. The gate is a gate, not a wall.
        assert!(machine.render_masked(bus.layers()));
        let glass = machine.image_mask();
        panel.click(&mut machine, &mut bus, glass, (200, 100));
        assert!(
            !panel.readout().expect("a readout").refused,
            "with the glass and the machine back in step the click must answer: {:?}",
            panel.readout().map(|r| r.text())
        );
    }

    /// **A click before there is any picture is refused, and it says that rather than naming a mask.**
    ///
    /// `None` is a different fact from "unmasked", and a refusal that spelled it as one would send a
    /// reader looking for a mask that is not set.
    #[test]
    fn a_click_with_no_picture_yet_says_so() {
        let (mut machine, mut bus) = rig();
        let mut panel = Panel::default();
        assert_eq!(machine.image_mask(), None, "the precondition: no picture");
        panel.click(&mut machine, &mut bus, None, (2, 2));
        let r = panel.readout().expect("a refusal is a sentence");
        assert!(r.refused, "{:?}", r.text());
        assert!(
            r.text().contains("no picture on screen yet"),
            "it must say there is no picture rather than describe a mask: {:?}",
            r.text()
        );
        assert_eq!(
            armed_on_the_machine(&mut machine, &mut bus),
            Vec::new(),
            "and it must have armed nothing"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // ★ The load-bearing row: the assertion is on the MACHINE, not on the reply
    // ---------------------------------------------------------------------------------------------

    fn set_reg(v: &mut oracle_core::vdp::Vdp, reg: u8, val: u8) {
        v.control_write(0x8000 | (u16::from(reg) << 8) | u16::from(val), 0);
    }

    fn write_vram(v: &mut oracle_core::vdp::Vdp, addr: u16, words: &[u16]) {
        v.control_write(0x4000 | (addr & 0x3FFF), 0);
        v.control_write(addr >> 14, 0);
        for w in words {
            v.data_write(*w);
        }
    }

    /// A machine showing one opaque plane-A tile in the top-left corner and a non-zero backdrop everywhere
    /// else, so a click at `(2,2)` and a click at `(200,100)` have **different right answers in different
    /// VDP memories** — which is what makes the anti-vacuity clause below able to fail.
    const A_TILE: u16 = 0x055;
    const BACKDROP_ENTRY: u8 = 0x25;

    fn rig() -> (Machine, Bus) {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let bus = Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo::default(),
            false,
            None,
        );
        let v = machine.system_mut().vdp_mut();
        v.vram_mut().fill(0);
        set_reg(v, 0x01, 0x74); // display on, mode 5
        set_reg(v, 0x0C, 0x81); // H40
        set_reg(v, 0x02, 0x30); // plane A nametable @ $C000
        set_reg(v, 0x04, 0x07); // plane B nametable @ $E000
        set_reg(v, 0x05, 0x58); // SAT @ $B000, empty
        set_reg(v, 0x07, BACKDROP_ENTRY);
        set_reg(v, 0x0F, 0x02);
        set_reg(v, 0x10, 0x00);
        write_vram(v, 0xC000, &[(1 << 13) | A_TILE]);
        write_vram(v, A_TILE * 32, &[0x3333; 16]);
        (machine, bus)
    }

    /// Read the armed watches back **through the tool**, as `{space, addr}` pairs.
    ///
    /// Through `emulator/watchpoint_list` rather than off a field, because the claim is that the click
    /// changed the instrument a *client* can see — the one shared `Watchpoints` the `Host` owns and the
    /// Watchpoints tab reads. A test that peeked at `Panel::armed` would assert that this module remembered
    /// what it did, which it obviously does.
    fn armed_on_the_machine(machine: &mut Machine, bus: &mut Bus) -> Vec<(String, String)> {
        let sys = machine.system_mut();
        let v = match bus.call(sys, "emulator/watchpoint_list", &json!({})) {
            Answer::Ok(v) => v,
            Answer::Err(e) => panic!("watchpoint_list refused: {} {}", e.code, e.message),
        };
        v["watches"]
            .as_array()
            .expect("a watches array")
            .iter()
            .map(|w| {
                (
                    w["space"].as_str().unwrap_or_default().to_string(),
                    w["addr"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    }

    /// ★ **A click leaves a watch on the machine, over the range the clicked tile actually occupies — and a
    /// click somewhere else leaves a different one, in a different memory.**
    ///
    /// This is the row that cannot pass by accident. Every other test in this file is about geometry or
    /// wording; this one drives [`Panel::click`] end to end — resolve through `pick`, retire through
    /// `emulator/watchpoint_clear`, arm through `emulator/watchpoint_add` — and then reads the result back
    /// **off the shared instrument through the tool**, so a `Panel` that composed a lovely sentence and
    /// armed nothing fails here.
    ///
    /// **The expectations are derived, not measured.** `A_TILE * 32` is the fixture's own tile index times
    /// the pattern size; the CRAM address is the backdrop register's own entry times 2. Neither was read
    /// out of a first run and pasted back.
    ///
    /// **The anti-vacuity clause is the second half.** A first click arms a VRAM pattern; a second click on
    /// the backdrop must leave a CRAM entry *and no VRAM watch at all* — `assert_ne!` against the first
    /// answer, because a `Panel::click` that silently did nothing would satisfy "there is a watch" forever
    /// after the first one and satisfy nothing here.
    #[test]
    fn a_click_arms_the_clicked_tile_on_the_machine_and_the_next_click_replaces_it() {
        let (mut machine, mut bus) = rig();
        let mut panel = Panel::default();
        // S2a: a click is answered against *the picture on the glass*, so there has to be one. Nothing is
        // masked here, so this is the ordinary frame every other assertion in this row is about.
        assert!(machine.render_masked(LayerMask::ALL));

        // Nothing is armed before the first click — the control, taken while it is still unambiguous.
        assert_eq!(
            armed_on_the_machine(&mut machine, &mut bus),
            Vec::new(),
            "the instrument must start empty, or every assertion below is about somebody else's watch"
        );

        // --- Click the one opaque plane-A cell. ---
        let glass = machine.image_mask();
        panel.click(&mut machine, &mut bus, glass, (2, 2));
        let after_plane = armed_on_the_machine(&mut machine, &mut bus);
        let want_vram = format!("0x{:08X}", u32::from(A_TILE) * 32);
        assert_eq!(
            after_plane,
            vec![("vram".to_string(), want_vram.clone())],
            "a plane click must arm exactly the 32-byte pattern of tile ${A_TILE:03X} in VRAM"
        );
        assert_eq!(panel.armed_count(), 1);
        assert!(
            !panel.readout().expect("a standing readout").refused,
            "a click that armed a watch is not a refusal: {:?}",
            panel.readout().map(|r| r.text())
        );

        // --- Click the backdrop. The prior watch is retired and a CRAM entry takes its place. ---
        let glass = machine.image_mask();
        panel.click(&mut machine, &mut bus, glass, (200, 100));
        let after_backdrop = armed_on_the_machine(&mut machine, &mut bus);
        let want_cram = format!("0x{:08X}", u32::from(BACKDROP_ENTRY) * 2);
        assert_eq!(
            after_backdrop,
            vec![("cram".to_string(), want_cram)],
            "a backdrop click must arm the CRAM word its register selects, and nothing else"
        );
        // ⚑ ANTI-VACUITY. If `click` were a no-op after the first, both reads would be equal and every
        // assertion above would still hold.
        assert_ne!(
            after_plane, after_backdrop,
            "the second click must have changed the instrument — equal reads mean click did nothing"
        );
        assert_eq!(
            panel.armed_count(),
            1,
            "the panel retires what IT armed; it must not accumulate"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ The spawn picker, and the pause the click takes to make itself legal
    // ---------------------------------------------------------------------------------------------

    /// A listing whose only symbols are three `ObjDef_` archetypes, so spawn mode can arm **for real**
    /// against the same bounded prefix search the shipped path uses.
    ///
    /// Deliberately no `Camera_X`, so the choreography refuses at the world join. That is not a weakness
    /// of these tests: the run state is restored on every path, and a refusal is the path where a window
    /// that only restored on success would still look correct.
    /// ⚑ **The `Equate Table` is not decoration here.** Without it this fixture answers the empty set for
    /// every archetype, so every subtype path in this file would be the absent one and the populated
    /// paths would be unreachable while looking tested. That is the shape that has hidden a real defect
    /// in this repo three times, so the section is present **and** built to disagree with the live
    /// listing in the two dimensions under test:
    ///
    /// * **value order is not name order.** By name these run `Angled_Blue`, `Down_Red`, `Up_Red`,
    ///   `Up_Yellow`, `Wide_Huge`; by value they run `Up_Red`, `Up_Yellow`, `Angled_Blue`, `Down_Red`,
    ///   `Wide_Huge`. The parser's own map is name ordered, so an implementation that forgot to sort
    ///   would look right on a corpus where the two agree.
    /// * **one value does not fit in a byte** (`$140`). Nothing aeon publishes today exceeds `$52`, so
    ///   the rail that refuses to cut a value down is unexercised by every real build.
    ///
    /// `ObjDef_Ring` and `ObjDef_Monitor` deliberately publish **none**, so the stated-absence path and
    /// the populated path both have a subject in the same listing and a selection change between them is
    /// a real re-read rather than a repeat.
    const ARCHETYPE_LST: &str = "\
  Symbol Table (* = unused):
  --------------------------

 ObjDef_Ring : 1000 C |
 ObjDef_Spring : 1010 C |
 ObjDef_Monitor : 1020 C |

    3 symbols
    0 unused symbols

  Equate Table (name = value; values, not addresses):
  ---------------------------------------------------

EQU ObjSub_Spring__Angled_Blue = $00000011
EQU ObjSub_Spring__Down_Red = $00000020
EQU ObjSub_Spring__Up_Red = $00000000
EQU ObjSub_Spring__Up_Yellow = $00000002
EQU ObjSub_Spring__Wide_Huge = $00000140

    5 equates
";

    /// A running machine whose listing names three archetypes, with spawn mode already armed.
    fn armed_rig() -> (Machine, Bus, Panel) {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let table = oracle_core::symbols::SymbolTable::parse(ARCHETYPE_LST)
            .expect("the fixture listing parses");
        let mut bus = Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo {
                rom_path: Some("testrom".into()),
                symbols: Some(table),
                symbols_path: Some("testrom.lst".into()),
            },
            false,
            None,
        );
        let mut panel = Panel::default();
        panel.arm_spawn(&mut machine, &mut bus);
        assert!(
            panel.is_armed(),
            "the fixture must arm, or every test below is about a disarmed mode: {:?}",
            panel.readout().map(Readout::text)
        );
        (machine, bus, panel)
    }

    /// ★ **The body runs on a machine that is genuinely paused, and the run state is put back.**
    ///
    /// The first half is the whole reason `emulator/object_spawn` can be reached from a click at all: it
    /// refuses `-32005 machineRunning` against a free-running bus, so a window that dispatched it without
    /// pausing would refuse every click, and one whose "pause" did not land would look identical.
    /// Asserted **inside** the body, which is what [`paused_for`] exists as a closure-taker for.
    ///
    /// The second half is the ruling's: the machine was running when the click arrived, so it is running
    /// when the click is done.
    ///
    /// **The alternative green paths, each ruled out by a named assertion:**
    /// 1. *The fixture was paused all along*, which would make the restore vacuous. Asserted running
    ///    before anything happens.
    /// 2. *The body never ran.* It returns a token that is asserted, so a `paused_for` that skipped it
    ///    could not produce this value.
    #[test]
    fn the_body_runs_on_a_machine_that_is_genuinely_paused_and_the_run_state_is_put_back() {
        let (mut machine, mut bus, _) = armed_rig();
        assert!(
            !bus.is_paused(),
            "the control: this fixture must start RUNNING or a restore witnesses nothing"
        );

        let mut saw: Option<bool> = None;
        let (token, run) = paused_for(&mut machine, &mut bus, |_, bus| {
            saw = Some(bus.is_paused());
            "the body ran"
        })
        .expect("the pause was accepted");

        assert_eq!(token, "the body ran");
        assert_eq!(
            saw,
            Some(true),
            "the body ran against a free-running bus, so `emulator/object_spawn` would answer \
             -32005 machineRunning and the click would place nothing"
        );
        assert_eq!(run, spawn_picker::RunState::Restored { frames: None });
        assert!(
            !bus.is_paused(),
            "the window paused this machine and left it paused: the person did not stop it and \
             nothing here would restart it"
        );
    }

    /// ★ **A machine somebody else paused is left exactly as it was found.**
    ///
    /// The hazard the design page names as the worse of the two, because nothing announces it: an
    /// attached client pauses the machine to read it, and the window resumes under the client mid read.
    /// A client-paused machine needs no pause and no resume at all.
    #[test]
    fn a_machine_somebody_else_paused_is_left_paused_and_nothing_here_resumes_it() {
        let (mut machine, mut bus, _) = armed_rig();
        // Through the served method, which is the door a socket client uses.
        assert!(
            !bus.call(machine.system_mut(), crate::ui::PAUSE, &json!({}))
                .is_err(),
            "the fixture could not be paused"
        );
        assert!(bus.is_paused(), "the control: it must be paused now");

        let mut saw: Option<bool> = None;
        let (_, run) = paused_for(&mut machine, &mut bus, |_, bus| {
            saw = Some(bus.is_paused());
        })
        .expect("pausing an already-paused machine is not a refusal");

        assert_eq!(saw, Some(true), "the body must still see a paused machine");
        assert_eq!(
            run,
            spawn_picker::RunState::AlreadyPaused,
            "a machine that was already paused was neither paused nor resumed by this window"
        );
        assert!(
            bus.is_paused(),
            "the window RESUMED a machine somebody else stopped, which is the exact hazard the \
             capture-and-restore exists to prevent"
        );
        assert!(
            !run.alarming(),
            "leaving a paused machine paused is the quiet case, not an alarm"
        );
    }

    /// ★ **The whole gesture, through [`Panel::click`]**: spawn mode armed, a click on the picture, the
    /// window's run state where it started, and a standing sentence saying what it did.
    ///
    /// The spawn itself is refused here (this listing has no `Camera_X`), which is deliberate: the run
    /// state must be restored on the refusal path too, and the readout must carry both halves.
    #[test]
    fn a_spawn_click_restores_the_run_state_and_says_what_it_did() {
        let (mut machine, mut bus, mut panel) = armed_rig();
        let mask = bus.layers();
        assert!(!bus.is_paused(), "the control: the fixture starts running");
        // ⚑ **Arming already paused and restored once**, because the arm is a selection change and a
        // selection change takes a picture ([`Panel::take_preview`]). The run state is therefore already
        // set, and it is set under the OTHER deed, which is what the last assertion in this test is about.
        let (armed_line, _) = panel
            .run_line()
            .expect("arming takes a picture, and taking one moves the run state");
        assert!(
            armed_line.contains("take a picture"),
            "the arm's account must be the preview's and not a placement's: {armed_line:?}"
        );
        assert!(!bus.is_paused(), "the picture put the run state back");

        panel.click(&mut machine, &mut bus, Some(mask), (2, 2));

        assert!(
            !bus.is_paused(),
            "a click that placed nothing still left the machine paused"
        );
        let run = panel
            .run_state()
            .expect("a placing click says what it did to the run state");
        assert_eq!(*run, spawn_picker::RunState::Restored { frames: None });
        let r = panel.readout().expect("a click leaves a readout");
        assert!(
            r.refused,
            "this listing cannot join a dot to a world position, so the spawn is refused: {:?}",
            r.text()
        );
        assert_eq!(
            r.outcome.as_deref(),
            Some(run.sentence().as_str()),
            "the readout's outcome is the run-state sentence, so the tab cannot show one without the \
             other"
        );
        // ⚑ **And the deed came back with it.** A placement that printed the preview's sentence would
        // send a reader hunting for a picture nobody asked for, on the loudest standing line this tab has.
        let (placed_line, _) = panel.run_line().expect("a placing click sets the line");
        assert!(
            placed_line.contains("place the object") && !placed_line.contains("take a picture"),
            "the click's account must be the placement's: {placed_line:?}"
        );
    }

    /// ★★ **A body that moves the machine is rewound, and the checkpoint does not outlive it.**
    ///
    /// This is the property the whole preview rests on: taking a picture spawns an object into the machine
    /// somebody is playing and runs it forward twice. If any of that leaks, the window has silently
    /// altered a game in progress, and the symptom is an object appearing out of nowhere seconds later.
    ///
    /// # ⚑ Why this is posed on [`checkpointed`] rather than on [`Panel::take_preview`]
    ///
    /// **Because the same assertion on `take_preview` was green with every rewind deleted, and that was
    /// measured rather than reasoned.** This fixture has no `Camera_X`, so the spawn inside the
    /// measurement is refused by `emulator/object_at` *before a single frame runs*, and a machine that
    /// never moved is byte-identical whether or not anything put it back. The row proved that the
    /// measurement leaves the machine alone; it proved nothing at all about the rewind, and it would have
    /// gone on passing while the feature corrupted a running game.
    ///
    /// So the property is posed where it can fail: a body that **deliberately** advances the machine, with
    /// its own control asserting that it did, on the real bus and with no game.
    #[test]
    fn a_body_that_moves_the_machine_is_rewound_and_the_checkpoint_goes_with_it() {
        let (mut machine, mut bus, _panel) = armed_rig();
        let before = machine.system().snapshot();

        let (moved, alarm) = checkpointed(&mut machine, &mut bus, |m, b, _id| {
            let _ = b.call(m.system_mut(), crate::ui::PAUSE, &json!({}));
            assert!(
                matches!(
                    b.call(m.system_mut(), "emulator/run_frames", &json!({"frames": 2})),
                    Answer::Ok(_)
                ),
                "the body has to be able to run frames or it cannot move anything"
            );
            // ⚑ **The control, taken inside the body**: the machine really is somewhere else by now, so
            // the assertion after the wrapper returns is about the rewind and not about a body that did
            // nothing.
            m.system().snapshot() != before
        })
        .expect("the checkpoint");

        assert!(
            moved,
            "the body must actually move the machine, or the rewind below witnesses nothing"
        );
        assert_eq!(alarm, None, "the restore was not refused on this fixture");
        // Compared as a fingerprint rather than as the bytes themselves, because a failure here prints
        // its operands: two 1.4 MB machine images in a test log is a red row nobody can read, and the
        // question being asked is only whether they are the same.
        assert_eq!(
            fnv1a_bytes(&machine.system().snapshot()),
            fnv1a_bytes(&before),
            "the machine must come back byte for byte: an object placed to look at is an object \
             nobody asked for"
        );

        // The probe's checkpoint is dropped on every path. A slot left behind is a whole machine's worth
        // of memory per selection change, and it would fill the server's own cap.
        match bus.call(machine.system_mut(), "emulator/checkpoint_list", &json!({})) {
            Answer::Ok(v) => assert_eq!(
                v["checkpoints"].as_array().map(Vec::len),
                Some(0),
                "the rewind's checkpoint must not outlive it: {v:?}"
            ),
            Answer::Err(e) => panic!("listing checkpoints was refused: {} {}", e.code, e.message),
        }
    }

    /// ★ **The whole gesture leaves the machine alone**, end to end through [`Panel::take_preview`].
    ///
    /// ⚑ **On its own this row is weak, and it is kept for what it does cover rather than for what it
    /// looks like it covers.** On this fixture the spawn is refused before a frame runs, so the machine
    /// never moves and the comparison cannot fail. What it does pin is that the gesture is wired up: the
    /// run state comes back, the checkpoint is dropped, and the panel is left holding a stated reason. The
    /// rewind itself is pinned by the row above, where it can fail.
    #[test]
    fn the_whole_gesture_leaves_the_machine_and_the_run_state_alone() {
        let (mut machine, mut bus, mut panel) = armed_rig();
        let before = machine.system().snapshot();

        panel.take_preview(&mut machine, &mut bus);

        assert_eq!(
            fnv1a_bytes(&machine.system().snapshot()),
            fnv1a_bytes(&before),
            "taking a picture must leave the machine where it was"
        );
        assert!(
            !bus.is_paused(),
            "the machine was running when the picture was taken, so it must be running after it"
        );
        assert!(
            panel.preview().is_some(),
            "a selection always leaves the panel holding an outcome, drawable or stated"
        );
        match bus.call(machine.system_mut(), "emulator/checkpoint_list", &json!({})) {
            Answer::Ok(v) => assert_eq!(
                v["checkpoints"].as_array().map(Vec::len),
                Some(0),
                "the preview's checkpoint must not outlive the preview: {v:?}"
            ),
            Answer::Err(e) => panic!("listing checkpoints was refused: {} {}", e.code, e.message),
        }
    }

    /// ★ **Every way a picture cannot be taken is a sentence, and the fixture takes one of them.**
    ///
    /// This listing has no `Camera_X`, so the spawn inside the measurement is refused, which is the arm
    /// this rig can reach without a game. What it pins is P6 on the real path: the panel holds a stated
    /// reason rather than an empty frame, the reason names the archetype, and nothing drawable survives.
    #[test]
    fn a_picture_that_could_not_be_taken_is_a_stated_reason_and_not_an_empty_frame() {
        let (mut machine, mut bus, mut panel) = armed_rig();
        panel.take_preview(&mut machine, &mut bus);
        let out = panel
            .preview()
            .expect("a selection always leaves an outcome");
        assert!(
            out.drawable().is_none(),
            "this fixture cannot place anything, so nothing may be drawable"
        );
        let why = out.sentence();
        assert!(
            why.len() > 40,
            "the reason must be a sentence rather than a label: {why:?}"
        );
        assert!(
            why.contains("ObjDef_"),
            "and it must name what could not be pictured: {why:?}"
        );
        for bad in ['\u{2014}', '\u{2013}'] {
            assert!(!why.contains(bad), "P10, on the real path: {why:?}");
        }
    }

    /// ★ **A picture of one archetype is never shown under another one's name.**
    ///
    /// The selection can move while a picture is held, and the badge names the selection. A picture of the
    /// previous archetype drawn under it would be a wrong answer carrying the panel's whole authority, and
    /// the guard is on the accessor so neither draw site can be the one that forgets it.
    #[test]
    fn a_picture_of_another_archetype_is_not_offered_for_this_one() {
        use crate::preview::{Art, Cell, Key, Outcome, Preview, Shot};
        let (mut machine, mut bus, mut panel) = armed_rig();
        let selected = panel
            .listing()
            .rows
            .iter()
            .find(|r| r.selected)
            .map(|r| r.name.clone())
            .expect("the arm selects one");

        let made = |name: &str| {
            Outcome::Ready(Box::new(Preview {
                key: Key {
                    archetype: name.to_string(),
                    subtype: None,
                },
                w: 8,
                h: 8,
                anchor: (4, 4),
                cells: vec![Cell {
                    x: 0,
                    y: 0,
                    w: 8,
                    h: 8,
                }],
                art: Art::Captured(Shot {
                    w: 8,
                    h: 8,
                    px: vec![Some((1, 2, 3)); 64],
                }),
                tiles: vec![1],
                art_print: 0,
            }))
        };

        panel.preview = Some(made(&selected));
        assert!(
            panel.preview().and_then(Outcome::drawable).is_some(),
            "the control: a picture of the selected archetype IS offered"
        );
        panel.preview = Some(made("ObjDef_SomethingElse"));
        assert!(
            panel.preview().is_none(),
            "a picture of another archetype must not reach a draw site at all"
        );
        let _ = (&mut machine, &mut bus);
    }

    /// ★ **Arming reads the subtypes out of the listing, and changing the archetype re-reads them.**
    ///
    /// The wiring end to end, on a rig whose listing genuinely publishes an `Equate Table`. Three things
    /// only fail here:
    ///
    /// * **the arm reads them at all**, rather than the picker sitting empty until something else asks;
    /// * **the armed default is the lowest valued one that fits**, named rather than left implicit,
    ///   because a placement carries a subtype byte whether or not one was chosen and a picker showing no
    ///   selection would be arming one in silence;
    /// * ⚑ **the set belongs to the archetype.** Selecting an archetype the listing names no subtypes for
    ///   must clear the previous one's, or the picker offers the spring's strengths for a monitor and the
    ///   byte it sends is a real byte for the wrong object. Going back re-reads them, so this is a
    ///   re-read and not a one-way clear.
    #[test]
    fn arming_reads_the_subtypes_and_changing_the_archetype_reads_them_again() {
        let (mut machine, mut bus, mut panel) = armed_rig();

        // The arm selects the first archetype by name order, which is `ObjDef_Monitor` here, and it
        // publishes no subtypes. So the control comes first: point the panel at the one that does.
        panel.select_archetype(&mut machine, &mut bus, "ObjDef_Spring");
        let l = panel
            .subtype_listing()
            .expect("the listing is loaded, so nothing refused the read");
        assert_eq!(
            l.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            ["Up_Red", "Up_Yellow", "Angled_Blue", "Down_Red", "Wide_Huge"],
            "the rows are read out of the listing's equate table and ordered by value, which is not \
             the order the parser's own map holds them in"
        );
        assert_eq!(
            panel.subtype_byte(),
            Some(0x00),
            "the arm carries the lowest valued subtype that fits a byte"
        );
        assert!(
            l.armed.contains("Up_Red") && l.armed.contains("$00"),
            "and it is named in words rather than left to a fill colour: {:?}",
            l.armed
        );
        assert_eq!(l.absence, None);
        assert_eq!(
            l.truncation, None,
            "five subtypes is well under the search's cap, so nothing may claim it was cut short"
        );

        // Choosing one moves the armed byte to the listing's own value.
        panel.select_subtype(&mut machine, &mut bus, "ObjSub_Spring__Angled_Blue");
        assert_eq!(panel.subtype_byte(), Some(0x11));

        // ⚑ The set belongs to the archetype. An archetype the listing names none for clears it and says
        // so, rather than keeping a byte that means something else entirely.
        panel.select_archetype(&mut machine, &mut bus, "ObjDef_Monitor");
        assert_eq!(
            panel.subtype_byte(),
            None,
            "a subtype must not survive the archetype it belongs to"
        );
        let l = panel.subtype_listing().expect("still readable");
        assert!(l.rows.is_empty());
        let line = l.absence.expect("P6: an empty list is a stated line");
        assert!(
            line.contains("ObjSub_Monitor__") && line.contains("ObjDef_Monitor"),
            "the line names the archetype and the namespace searched: {line:?}"
        );

        // …and going back re-reads them, so the clear above was a re-read and not a one-way door.
        panel.select_archetype(&mut machine, &mut bus, "ObjDef_Spring");
        assert_eq!(panel.subtype_byte(), Some(0x00));
        assert_eq!(panel.subtype_listing().expect("readable").rows.len(), 5);
    }

    /// ⚑ **A picture of one form is never drawn under another form's badge.**
    ///
    /// The archetype half of this guard already exists one test up; this is the half the filled key
    /// bought. Two subtypes of one archetype are usually two different pictures, so a preview kept under
    /// the archetype's name alone would come back after a subtype change **looking exactly like the
    /// feature working**, which is the failure the whole cache key exists against.
    #[test]
    fn a_picture_of_one_subtype_is_not_offered_under_another() {
        use crate::preview::{Art, Cell, Key, Outcome, Preview, Shot};
        let (mut machine, mut bus, mut panel) = armed_rig();
        panel.select_archetype(&mut machine, &mut bus, "ObjDef_Spring");
        panel.select_subtype(&mut machine, &mut bus, "ObjSub_Spring__Up_Yellow");
        assert_eq!(panel.subtype_byte(), Some(0x02));

        let made = |subtype: Option<u32>| {
            Outcome::Ready(Box::new(Preview {
                key: Key {
                    archetype: "ObjDef_Spring".to_string(),
                    subtype,
                },
                w: 8,
                h: 8,
                anchor: (4, 4),
                cells: vec![Cell {
                    x: 0,
                    y: 0,
                    w: 8,
                    h: 8,
                }],
                art: Art::Captured(Shot {
                    w: 8,
                    h: 8,
                    px: vec![Some((1, 2, 3)); 64],
                }),
                tiles: vec![1],
                art_print: 0,
            }))
        };

        // The control: a picture of the armed form IS offered, so the rejections below are about the
        // subtype and not about the guard rejecting everything.
        panel.preview = Some(made(Some(0x02)));
        assert!(
            panel.preview().and_then(Outcome::drawable).is_some(),
            "a picture of the armed subtype must reach a draw site"
        );

        // A picture of a different form of the same archetype must not.
        panel.preview = Some(made(Some(0x00)));
        assert!(
            panel.preview().is_none(),
            "a picture of the red spring must not be drawn under the yellow spring's badge"
        );
        // Nor one that named no subtype at all: that is a third thing, not a wildcard.
        panel.preview = Some(made(None));
        assert!(
            panel.preview().is_none(),
            "a placement that named no subtype is not a stand-in for every subtype"
        );
    }

    /// ★ **The picker: the rows are the mode's, a click on one selects it, and the filter narrows what is
    /// drawn without changing what a click places.**
    #[test]
    fn the_picker_lists_the_modes_archetypes_and_selecting_one_moves_the_badge() {
        let (mut machine, mut bus, mut panel) = armed_rig();

        let l = panel.listing();
        let first = l.rows.first().expect("three archetypes armed").name.clone();
        assert_eq!(
            l.rows.len(),
            3,
            "the picker draws the mode's own whole list"
        );
        assert_eq!(l.rows.iter().filter(|r| r.selected).count(), 1);
        assert!(
            panel.badge().expect("armed").contains(&first),
            "the arm selects the first row, and the badge is what says which"
        );

        panel.select_archetype(&mut machine, &mut bus, "ObjDef_Spring");
        assert!(
            panel.badge().expect("armed").contains("ObjDef_Spring"),
            "selecting a row must move the badge, which is what a click reads"
        );
        assert!(
            panel
                .listing()
                .rows
                .iter()
                .any(|r| r.selected && r.name == "ObjDef_Spring"),
            "and the list must mark it"
        );

        // The filter narrows the DRAWING and never the selection.
        *panel.filter_mut() = "ring".to_string();
        let l = panel.listing();
        assert_eq!(
            l.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["ObjDef_Ring", "ObjDef_Spring"],
            "the filter is a case-folded substring over the whole name"
        );
        assert!(
            panel.badge().expect("armed").contains("ObjDef_Spring"),
            "a filter must not change what a click places"
        );

        // Disarming clears the list and the filter with it. The run state is NOT cleared, because what
        // this window did to a machine is not undone by turning a mode off.
        panel.disarm_spawn();
        assert!(!panel.is_armed());
        let off = panel.listing();
        assert!(off.rows.is_empty());
        assert!(
            off.absence.is_some(),
            "an empty list owes the reader a line"
        );
    }
}
