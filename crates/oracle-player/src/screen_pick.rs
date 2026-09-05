//! **The Screen tab's pointer half** — click a dot to arm a watch on whatever draws it, or, in spawn mode,
//! to put an object there.
//!
//! This is the migration's S1 (`docs/2026-09-05-frontend-migration-recon.md` §3.1), and it closes
//! `F-SPAWN-PICKER-PANEL-SURFACE` by dissolving the question that row was blocked on. That row asked *which*
//! window the owner's *"clicking a spot in the Screen panel"* meant, because there were two; after the
//! migration there is one window and it is the panels window with the game picture in a tab, so both answers
//! became the same answer.
//!
//! ## ⚑ S1 SHIPS MASKED-OFF-ONLY, AND THIS IS THE PLACE THAT SAYS SO
//!
//! §3.2a of the plan — *the display mask must reach the picture* — is **not in this parcel**, and §8 states
//! the consequence exactly: *"S1's picking is only correct once S2a lands, so if they are separated, S1 must
//! be masked-off-only and say so."* So it is, and here is the whole argument.
//!
//! `oracle-frontend` has two pixel paths: `blit_capture` normally, and `blit_masked` — a post-hoc re-render
//! through `render_line_masked` — whenever a display layer is hidden. **`oracle-player` has only the first.**
//! A mask set through `emulator/set_layer_enabled` therefore changes the bus's answers (`screenshot`,
//! `pixel_attribution`) and does **not** change this window's picture.
//!
//! [`pick::resolve`](oracle_frontend::pick::resolve)'s standing invariant is *"the panel describes the
//! picture"*, and under a mask this window cannot satisfy it either way round:
//!
//! * resolve **with** the bus's mask and the answer describes a picture nobody is looking at — plus a
//!   `planeA hidden, so this is the masked picture` clause that is false of this window's glass;
//! * resolve **without** it and the answer is right about the picture but disagrees, silently, with what
//!   `emulator/pixel_attribution` would tell a socket client about the same dot in the same instant. That is
//!   precisely the drift `pick.rs`'s `bus_parity` guard exists to prevent, arriving through the one path the
//!   guard cannot see.
//!
//! Neither is acceptable, so **while any layer is hidden a click is REFUSED**, in a sentence that names the
//! hidden layers, says the picture does not honour a mask yet, and names both ways out. That is
//! loud-on-unmeasurable applied to a gesture: a picker that quietly answered would be right most days and
//! confidently wrong on the day it mattered. See [`Panel::masked_off_only`].
//!
//! **What S2a deletes:** that gate and its test, plus the `LayerMask::ALL` note in [`Panel::pick`]. Nothing
//! else here changes — the resolve call already takes the bus's own mask.
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

/// One line the tab shows about the last gesture, and whether it was a refusal.
///
/// **`refused` is a field, never a shape of the text.** The tab colours on it. A refusal that reads like a
/// success is the one rendering mistake a debug surface cannot afford, and matching on a `"REFUSED"` prefix
/// would be a second encoding of the same fact — the rule [`crate::ui::Echo`] already states one file over.
pub struct Readout {
    pub text: String,
    pub refused: bool,
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
                match self.mode.arm(a.names) {
                    Ok(name) => {
                        let mut s = format!("spawn mode armed — a click places {name}");
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
    }

    /// Turn spawn mode off. A click picks again.
    pub fn disarm_spawn(&mut self) {
        self.mode.disarm();
        self.last = Some(Readout::ok(
            "spawn mode off — a click arms a watch again".into(),
        ));
    }

    /// Select the next archetype, wrapping. A key that silently does nothing is indistinguishable from a
    /// broken one, so the `None` arm reports rather than swallowing.
    pub fn cycle_spawn(&mut self) {
        match self.mode.cycle() {
            Some(name) => self.last = Some(Readout::ok(format!("a click now places {name}"))),
            None => {
                self.last = Some(Readout::refused(
                    "spawn mode is not armed, so there is nothing to cycle through".into(),
                ))
            }
        }
    }

    /// **The click.** Spawn mode takes it if armed, otherwise it is a watch pick.
    ///
    /// The branch is here, in front of the pick, exactly as `oracle-frontend`'s run loop puts it there:
    /// the two are the same gesture and only one of them can have it — which is precisely why the mode owes
    /// a standing statement that it is on ([`Panel::badge`]).
    pub fn click(&mut self, machine: &mut Machine, bus: &mut Bus, dot: (u16, u16)) {
        match self.mode.selected().map(str::to_string) {
            Some(archetype) => self.place(machine, bus, &archetype, dot),
            None => self.pick(machine, bus, dot),
        }
    }

    /// The refusal a click gets while a display layer is hidden. See this module's doc for the argument.
    ///
    /// **The layers are read off the mask**, never listed here, so this cannot name a layer the bus does not
    /// have — the same derivation `pick::resolve`'s own mask clause and the frontend's layer badge read.
    fn masked_off_only(mask: LayerMask) -> String {
        let hidden = mask.hidden().join(" + ");
        format!(
            "{hidden} hidden — but this window's picture does not honour a display mask yet (migration \
             S2a), so what you clicked is the UNMASKED machine. An answer here would describe neither the \
             picture in front of you nor what emulator/pixel_attribution would tell a client about the same \
             dot, so nothing was armed. Show every layer again (emulator/set_layer_enabled), or use the \
             minifb window, whose picture does honour the mask."
        )
    }

    fn pick(&mut self, machine: &mut Machine, bus: &mut Bus, dot: (u16, u16)) {
        let (x, y) = dot;
        // ⚑ THE MASKED-OFF-ONLY GATE. Read before anything is resolved or retired, so a refused click
        // leaves the previously armed watches exactly where they were rather than half-clearing them.
        let mask = bus.layers();
        if !mask.hidden().is_empty() {
            self.last = Some(Readout::refused(Self::masked_off_only(mask)));
            return;
        }

        let sys = machine.system_mut();
        // The machine's **now**, which §11.27's colour-staleness rule compares a CRAM write stamp against.
        // `sys.scheduler().now()` — the same instant `emulator/pixel_attribution` stamps its verdict with —
        // and deliberately NOT `Vdp::now_mclk`, which is the instant the VDP last did guest-driven work and
        // on a paused machine can be arbitrarily stale.
        let now = sys.scheduler().now();
        // The mask is the bus's own, not `LayerMask::ALL`: the gate above has already established it hides
        // nothing, so the two are equal here, and passing the real one is what makes this line correct
        // unchanged the moment S2a lands.
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
                        "the pick resolved but arming it was refused — {} {}{reason}",
                        e.code, e.message
                    ));
                    break;
                }
            }
        }

        self.last = Some(match refusal {
            Some(r) => Readout {
                text: format!("{}\n{r}", p.description),
                refused: true,
            },
            None => Readout {
                // `p.description` is the sentence a person reads, composed by `pick` and carrying §11.27's
                // colour caveat when it applies. The armed count is appended rather than substituted,
                // because "nothing was armed" is a real outcome (a backdrop with no writable entry) and a
                // description alone would not show it.
                text: format!(
                    "{}\n{} watch{} armed by this click",
                    p.description,
                    self.armed.len(),
                    if self.armed.len() == 1 { "" } else { "es" }
                ),
                refused: self.armed.is_empty(),
            },
        });
    }

    fn place(&mut self, machine: &mut Machine, bus: &mut Bus, archetype: &str, dot: (u16, u16)) {
        let sys = machine.system_mut();
        let remedy = pause_remedy();
        match spawn::place(&mut PlayerCaller { bus, sys }, archetype, dot) {
            Ok(p) => self.last = Some(Readout::ok(p.terminal(archetype))),
            Err(e) => {
                self.last = Some(Readout::refused(e.terminal(archetype, Some(&remedy))));
            }
        }
    }
}

impl Readout {
    fn ok(text: String) -> Self {
        Readout {
            text,
            refused: false,
        }
    }
    fn refused(text: String) -> Self {
        Readout {
            text,
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
    // The masked-off-only gate — what S2a deletes
    // ---------------------------------------------------------------------------------------------

    /// The refusal **names the hidden layers, and it reads them off the mask**.
    ///
    /// A sentence with the layer names written into it would pass a "contains planeA" check while being a
    /// claim this window made rather than one the bus supports, which is the whole failure the mask clause
    /// exists to prevent. So the expectation is derived from `LayerMask::hidden()` itself, and the
    /// all-shown control is asserted beside it — an absence needs its control.
    #[test]
    fn the_masked_off_only_refusal_names_the_layers_the_mask_hides() {
        assert!(
            LayerMask::ALL.hidden().is_empty(),
            "the all-shown control: nothing hidden, so no click is refused"
        );
        for (target_name, target) in LayerMask::targets() {
            let mut m = LayerMask::ALL;
            assert!(m.set(target, false), "{target_name} must be a mask target");
            let hidden = m.hidden();
            assert!(
                !hidden.is_empty(),
                "hiding {target_name} must hide something"
            );
            let s = Panel::masked_off_only(m);
            for name in &hidden {
                assert!(s.contains(name), "the refusal must name {name}: {s:?}");
            }
            // …and it must say what it is refusing and how to get out, or it is a shrug with a layer name
            // in it.
            assert!(
                s.contains("S2a"),
                "it must name the slice that fixes it: {s:?}"
            );
            assert!(
                s.contains("emulator/set_layer_enabled"),
                "it must name the way out: {s:?}"
            );
        }
    }

    /// The refusal is **impossible to reach** while every layer is shown — the gate is a gate, not a wall.
    #[test]
    fn nothing_is_refused_while_every_layer_is_shown() {
        assert!(LayerMask::default().hidden().is_empty());
        assert!(LayerMask::ALL.hidden().is_empty());
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

        // Nothing is armed before the first click — the control, taken while it is still unambiguous.
        assert_eq!(
            armed_on_the_machine(&mut machine, &mut bus),
            Vec::new(),
            "the instrument must start empty, or every assertion below is about somebody else's watch"
        );

        // --- Click the one opaque plane-A cell. ---
        panel.click(&mut machine, &mut bus, (2, 2));
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
            panel.readout().map(|r| r.text.clone())
        );

        // --- Click the backdrop. The prior watch is retired and a CRAM entry takes its place. ---
        panel.click(&mut machine, &mut bus, (200, 100));
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

    /// **A refused click leaves the previously armed watch exactly where it was.**
    ///
    /// The masked-off-only gate is read before anything is resolved or retired, and this is why that
    /// ordering is not incidental: a gate that fired after the retire would answer "nothing is armed"
    /// while telling the person it had refused to change anything.
    #[test]
    fn a_masked_off_refusal_changes_nothing_on_the_machine() {
        let (mut machine, mut bus) = rig();
        let mut panel = Panel::default();
        panel.click(&mut machine, &mut bus, (2, 2));
        let before = armed_on_the_machine(&mut machine, &mut bus);
        assert_eq!(before.len(), 1, "the precondition: one watch is armed");

        // Hide a layer through the tool — the same call a socket client would make.
        let hidden = {
            let sys = machine.system_mut();
            match bus.call(
                sys,
                "emulator/set_layer_enabled",
                &json!({"layer": "planeA", "enabled": false}),
            ) {
                Answer::Ok(v) => v,
                Answer::Err(e) => panic!("set_layer_enabled refused: {} {}", e.code, e.message),
            }
        };
        assert!(
            !bus.layers().hidden().is_empty(),
            "the mask must actually be set, or this row measures nothing: {hidden}"
        );

        panel.click(&mut machine, &mut bus, (200, 100));
        assert_eq!(
            armed_on_the_machine(&mut machine, &mut bus),
            before,
            "a refused click must leave the instrument untouched"
        );
        let r = panel
            .readout()
            .expect("a refusal is a sentence, never silence");
        assert!(r.refused, "and it must be marked as one: {:?}", r.text);
        assert!(
            r.text.contains("planeA"),
            "and it must name what is hidden: {:?}",
            r.text
        );
    }
}
