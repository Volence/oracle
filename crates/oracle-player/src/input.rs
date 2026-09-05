//! Host keyboard → [`Pad`], feeding the **same** surface the minifb player feeds:
//! `System::set_pad(port, pad)`, which `crates/oracle-core/src/io.rs` documents as the sole, deterministic
//! input path into the core. Nothing here reaches around it.
//!
//! # The binding is the incumbent's, deliberately
//!
//! `crates/oracle-frontend/src/main.rs::poll_pad` binds arrows to the D-pad, `A`/`S`/`D` to Genesis
//! A/B/C and `Enter` to Start. That is the binding the owner's hands already know, so it is reproduced
//! exactly rather than improved on; [`pad_from_keys`] and its test are the statement of it. A configurable
//! binding is a later parcel's job, not something to slip in under a rebuild.
//!
//! # The one thing that had to be redesigned
//!
//! The minifb player has a `swallow_keys_until_release` latch because its command palette closes
//! *mid-iteration*, leaving the key that closed it physically down when the pad is polled further down the
//! same iteration — so `Enter` (which ran the command) read straight through as Start and paused the game.
//! This player has no palette yet, but it has something the minifb player never had: **egui widgets that
//! want the keyboard**. A focused text field, a search box in a future memory panel, a rename-this-tab
//! edit — all of them are "the user is typing, not playing", and all of them can lose focus mid-frame with
//! the keys still held.
//!
//! So the same latch is kept, driven by `egui::Context::egui_wants_keyboard_input()` instead of by a palette
//! flag. [`release_latch`] is the incumbent's function unchanged; what feeds it is different.

use oracle_core::io::Pad;

/// Build the Player-1 [`Pad`] from a "is this key down right now" oracle.
///
/// It takes a closure rather than an `egui::InputState` so the binding itself is testable without
/// constructing a toolkit context — the mapping is the part that can be wrong, and it is the part a test
/// can pin.
pub fn pad_from_keys(down: impl Fn(egui::Key) -> bool) -> Pad {
    Pad {
        up: down(egui::Key::ArrowUp),
        down: down(egui::Key::ArrowDown),
        left: down(egui::Key::ArrowLeft),
        right: down(egui::Key::ArrowRight),
        a: down(egui::Key::A),
        b: down(egui::Key::S),
        c: down(egui::Key::D),
        start: down(egui::Key::Enter),
    }
}

/// Read Player 1 off a live egui context.
pub fn poll_pad(ctx: &egui::Context) -> Pad {
    ctx.input(|i| pad_from_keys(|k| i.key_down(k)))
}

/// Next value of the "these keys were typed at a widget, not at the game" latch.
///
/// Verbatim from `crates/oracle-frontend/src/main.rs::release_latch`: the keyboard half of Player 1 stays
/// released until the user has let go of *every* game key, so a press that began as text can never finish
/// as gameplay.
pub fn release_latch(latch: bool, any_game_key_down: bool) -> bool {
    latch && any_game_key_down
}

/// The whole per-iteration input decision, as one pure function so it can be tested without a window.
///
/// * `keys` — what the keyboard says right now ([`pad_from_keys`]).
/// * `wants_text` — `Context::egui_wants_keyboard_input()`: a widget has focus and is consuming typing.
/// * `latch` — the carried latch; updated in place.
///
/// Returns the pad the core is actually given.
pub fn decide(keys: Pad, wants_text: bool, latch: &mut bool) -> Pad {
    if wants_text {
        // Arm the latch while a widget is typing, so that when focus is dropped mid-iteration the keys
        // that dropped it do not read through as gameplay.
        *latch = true;
    } else {
        *latch = release_latch(*latch, keys != Pad::default());
    }
    if wants_text || *latch {
        Pad::default()
    } else {
        keys
    }
}

// ---------------------------------------------------------------------------------------------------
// ⚑ Machine keys — S3
// ---------------------------------------------------------------------------------------------------

/// **The keys that are not the pad**: reset, ROM reload, and the save-state slots.
///
/// A value rather than an action, so the binding is a pure function this crate can test without a window
/// and the *doing* stays where the doing belongs — two of these are a call to a served method and the
/// rest are [`crate::states`]. `oracle-frontend` binds the same five keys to the same five things
/// (`main.rs`'s controls table), and they are reproduced rather than improved on for
/// [`pad_from_keys`]'s reason: the owner's hands already know them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineKey {
    /// `F1` **and `Tab`** — soft reset. `emulator/reset`; SRAM contents preserved, as on real hardware.
    ///
    /// ⚑ **`Tab` was deliberately unbound here until 2026-09-05, and the owner overruled it.**
    ///
    /// The original reasoning stands as a description of the cost: `egui` uses `Tab` for focus traversal
    /// between widgets, and this window's docked panels have text boxes in them, so one keystroke now
    /// means both "move to the next field" and "reset the console". The owner was told that and took the
    /// trade: *"And I don't care aobut walking focus right now."* So the mechanism was never in doubt,
    /// only which of the two behaviours is worth more, and `oracle-frontend` binds both keys
    /// (`main.rs:38`, `main.rs:1919`) so this is parity with the window his hands already know.
    ///
    /// **Reversible, and the comment says so because he said "right now".** He dismissed the cost as it
    /// stands today, not for all time; a window that later grows a form worth tabbing through is a reason
    /// to raise it again, not a contradiction of this.
    ///
    /// What keeps it safe is not that `Tab` is rare but that [`poll_machine_keys`] returns nothing at all
    /// while `Context::egui_wants_keyboard_input()`, so no machine key fires while a widget has the
    /// keyboard. That is a property of the caller rather than of this binding, which is exactly why
    /// [`tab_resets_only_when_no_widget_wants_the_keyboard`](tests::tab_resets_only_when_no_widget_wants_the_keyboard)
    /// asserts it here instead of trusting it upstream.
    Reset,
    /// `F5` — re-read the ROM file and reset. `emulator/reload_rom`.
    ReloadRom,
    /// `F2` — write the machine to the selected slot.
    SaveState,
    /// `F4` — restore the machine from the selected slot.
    LoadState,
    /// `F6` / `F7` — previous / next slot, wrapping.
    SlotStep(isize),
    /// `0`-`9` — select that slot directly.
    SlotSelect(usize),
}

/// The binding, as a pure function of "was this key pressed this frame".
///
/// **Pressed, not down**: every one of these is an event, and a held `F2` must not write the slot sixty
/// times a second. That is the one place this differs in kind from [`pad_from_keys`], where "down" is
/// exactly right.
///
/// It returns a `Vec` rather than an `Option` because two of these can genuinely arrive in one frame (a
/// slot key and `F2`, from a fast hand), and dropping the second would make the save go to the wrong
/// slot — silently, and only sometimes.
///
/// The digits are **generated from [`oracle_frontend::save_state::SLOT_COUNT`]**, and the array they index
/// is length-checked against it below, so a slot count that grew past the keys is a test failure rather
/// than a slot nobody can reach.
pub fn machine_keys(pressed: impl Fn(egui::Key) -> bool) -> Vec<MachineKey> {
    let mut out = Vec::new();
    // Both keys, one action, and pushed once however many of them arrived: a hand on `F1` and `Tab` in
    // the same frame asked for one reset, not two.
    if pressed(egui::Key::F1) || pressed(egui::Key::Tab) {
        out.push(MachineKey::Reset);
    }
    if pressed(egui::Key::F5) {
        out.push(MachineKey::ReloadRom);
    }
    if pressed(egui::Key::F2) {
        out.push(MachineKey::SaveState);
    }
    if pressed(egui::Key::F4) {
        out.push(MachineKey::LoadState);
    }
    if pressed(egui::Key::F6) {
        out.push(MachineKey::SlotStep(-1));
    }
    if pressed(egui::Key::F7) {
        out.push(MachineKey::SlotStep(1));
    }
    for (slot, key) in SLOT_KEYS.iter().enumerate() {
        if pressed(*key) {
            out.push(MachineKey::SlotSelect(slot));
        }
    }
    out
}

/// The digit key for each slot, in slot order. Typed here because `egui::Key` has no arithmetic; held to
/// [`oracle_frontend::save_state::SLOT_COUNT`] by
/// [`every_slot_has_a_key`](tests::every_slot_has_a_key) rather than by a comment.
const SLOT_KEYS: [egui::Key; 10] = [
    egui::Key::Num0,
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
    egui::Key::Num8,
    egui::Key::Num9,
];

/// Read the machine keys off a live egui context.
///
/// **Silent while a widget has the keyboard**, for [`decide`]'s reason and then one more: the palette's
/// method box is a text field, and a person typing `emulator/reload_rom` into it types a `0` on the way
/// to `z80_read`. A digit that moved the save slot while it was being typed would be a slot change
/// nobody made.
pub fn poll_machine_keys(ctx: &egui::Context) -> Vec<MachineKey> {
    if ctx.egui_wants_keyboard_input() {
        return Vec::new();
    }
    ctx.input(|i| machine_keys(|k| i.key_pressed(k)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚑ **`Tab` resets the console, and only while nothing else wants the keyboard.**
    ///
    /// The owner overruled the decision that kept `Tab` unbound (2026-09-05), accepting the loss of focus
    /// traversal: *"And I don't care aobut walking focus right now."* What makes that safe is
    /// [`poll_machine_keys`]'s latch, not the rarity of the key, so **the latch is what is asserted** and
    /// it is asserted here rather than trusted upstream: a binding whose safety lives in its caller has
    /// no gate at all if only the binding is tested.
    ///
    /// A **discriminating pair**, because either leg alone is satisfied by a defect. "Tab does nothing
    /// while a widget has the keyboard" passes on a `Tab` that was never bound; "Tab resets" passes on a
    /// latch that has stopped working. Only the two together say what shipped.
    ///
    /// The focused-widget leg asserts its own precondition. `egui_wants_keyboard_input()` is
    /// `memory.focused().is_some()`, and a fixture that failed to focus anything would render this leg
    /// vacuous while reading exactly like a pass.
    #[test]
    fn tab_resets_only_when_no_widget_wants_the_keyboard() {
        // The binding itself: two keys, one action, and one action even when both arrive together.
        assert_eq!(
            machine_keys(|k| k == egui::Key::Tab),
            vec![MachineKey::Reset],
            "Tab is bound to Reset, as `oracle-frontend` binds it"
        );
        assert_eq!(
            machine_keys(|k| k == egui::Key::F1),
            vec![MachineKey::Reset]
        );
        assert_eq!(
            machine_keys(|k| matches!(k, egui::Key::Tab | egui::Key::F1)),
            vec![MachineKey::Reset],
            "a hand on both keys in one frame asked for one reset, not two"
        );

        let tab_press = || {
            let mut raw = egui::RawInput::default();
            raw.events.push(egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
            raw
        };

        // (a) Nothing wants the keyboard, so the key reaches the machine.
        let ctx = egui::Context::default();
        let mut got = Vec::new();
        let mut out = ctx.run_ui(tab_press(), |ui| {
            let ctx = ui.ctx();
            assert!(
                !ctx.egui_wants_keyboard_input(),
                "the control leg must start with nothing focused, or it measures the latch instead"
            );
            got = poll_machine_keys(ctx);
        });
        // `FullOutput` panics on drop with unapplied texture deltas: no backend is going to apply them
        // here, so they are discarded explicitly rather than left to be a confusing failure.
        out.textures_delta.clear();
        assert_eq!(
            got,
            vec![MachineKey::Reset],
            "Tab must reset when no widget has the keyboard"
        );

        // (b) A widget has the keyboard, so the same key reaches nothing.
        let ctx = egui::Context::default();
        let mut got = Vec::new();
        let mut out = ctx.run_ui(tab_press(), |ui| {
            let ctx = ui.ctx();
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("a text box")));
            assert!(
                ctx.egui_wants_keyboard_input(),
                "the fixture failed to focus anything, so this leg would pass without testing the latch"
            );
            got = poll_machine_keys(ctx);
        });
        out.textures_delta.clear();
        assert!(
            got.is_empty(),
            "Tab reset the console while somebody was typing: {got:?}"
        );
    }

    /// The slot keys and the slot count are one set, derived from the container's own constant. A tenth
    /// slot with no key would be a save nobody can reach; an eleventh key would index off the end.
    #[test]
    fn every_slot_has_a_key() {
        assert_eq!(
            SLOT_KEYS.len(),
            oracle_frontend::save_state::SLOT_COUNT,
            "the digit keys and the container's slot count have drifted"
        );
        for (slot, key) in SLOT_KEYS.iter().enumerate() {
            assert_eq!(
                machine_keys(|k| k == *key),
                vec![MachineKey::SlotSelect(slot)],
                "slot {slot}'s key selects slot {slot} and nothing else"
            );
        }
    }

    /// Each machine key means one thing, and pressing nothing means nothing. The `assert_eq!` on the whole
    /// vector is the anti-vacuity half: a binding that fired *and* dragged a second command along would
    /// pass a `contains` check.
    #[test]
    fn the_machine_keys_are_the_minifb_players_machine_keys() {
        let cases = [
            (egui::Key::F1, MachineKey::Reset),
            (egui::Key::F5, MachineKey::ReloadRom),
            (egui::Key::F2, MachineKey::SaveState),
            (egui::Key::F4, MachineKey::LoadState),
            (egui::Key::F6, MachineKey::SlotStep(-1)),
            (egui::Key::F7, MachineKey::SlotStep(1)),
        ];
        for (key, want) in cases {
            assert_eq!(machine_keys(|k| k == key), vec![want], "{key:?}");
        }
        assert!(
            machine_keys(|_| false).is_empty(),
            "nothing pressed is nothing done"
        );
        // ⚑ And no machine key is a pad key: a binding that overlapped would make one press mean two
        // things, and the one that is not the game would win silently.
        for (key, _) in cases {
            assert_eq!(
                pad_from_keys(|k| k == key),
                Pad::default(),
                "{key:?} is bound to the machine AND to the pad"
            );
        }
        for key in SLOT_KEYS {
            assert_eq!(
                pad_from_keys(|k| k == key),
                Pad::default(),
                "{key:?} is bound to a save slot AND to the pad"
            );
        }
    }

    /// Every binding, one key at a time, against the minifb player's `poll_pad`. If this test and
    /// `crates/oracle-frontend/src/main.rs::poll_pad` ever disagree, the owner's hands are the ones that
    /// find out.
    #[test]
    fn the_binding_is_the_minifb_players_binding() {
        /// key, the pad field it must set, and a name for the failure message.
        type Case = (egui::Key, fn(&Pad) -> bool, &'static str);
        let cases: [Case; 8] = [
            (egui::Key::ArrowUp, |p| p.up, "Up"),
            (egui::Key::ArrowDown, |p| p.down, "Down"),
            (egui::Key::ArrowLeft, |p| p.left, "Left"),
            (egui::Key::ArrowRight, |p| p.right, "Right"),
            (egui::Key::A, |p| p.a, "A -> Genesis A"),
            (egui::Key::S, |p| p.b, "S -> Genesis B"),
            (egui::Key::D, |p| p.c, "D -> Genesis C"),
            (egui::Key::Enter, |p| p.start, "Enter -> Start"),
        ];
        for (key, field, name) in cases {
            let pad = pad_from_keys(|k| k == key);
            assert!(field(&pad), "{name}: the bound key did not set its button");
            // ...and it set *only* that button.
            let lit = [
                pad.up, pad.down, pad.left, pad.right, pad.a, pad.b, pad.c, pad.start,
            ]
            .iter()
            .filter(|b| **b)
            .count();
            assert_eq!(lit, 1, "{name}: set {lit} buttons, expected exactly 1");
        }
        assert_eq!(
            pad_from_keys(|_| false),
            Pad::default(),
            "nothing down = nothing pressed"
        );
    }

    /// `S` is Genesis **B** and `D` is Genesis **C**. Getting this pair the wrong way round is the single
    /// most plausible transcription error in the whole mapping, and it would be invisible in a rebuild
    /// until somebody tried to jump.
    #[test]
    fn s_is_b_and_d_is_c_not_the_other_way_round() {
        let s = pad_from_keys(|k| k == egui::Key::S);
        assert!(s.b && !s.c, "S must be B");
        let d = pad_from_keys(|k| k == egui::Key::D);
        assert!(d.c && !d.b, "D must be C");
    }

    /// While a widget holds the keyboard the game sees nothing...
    #[test]
    fn a_focused_widget_swallows_the_pad() {
        let mut latch = false;
        let typing = pad_from_keys(|k| k == egui::Key::Enter);
        assert_eq!(decide(typing, true, &mut latch), Pad::default());
        assert!(latch, "the latch must arm while typing");
    }

    /// ...and the key that dismissed it does not read through as Start on the way out. This is the exact
    /// bug the minifb player's latch exists to stop (Enter running a command, then pausing the game).
    #[test]
    fn the_key_that_dismissed_a_widget_is_not_a_button_press() {
        let mut latch = false;
        let enter = pad_from_keys(|k| k == egui::Key::Enter);

        // Frame 1: typing, Enter down.
        assert_eq!(decide(enter, true, &mut latch), Pad::default());
        // Frame 2: the widget dropped focus, Enter still physically held.
        assert_eq!(
            decide(enter, false, &mut latch),
            Pad::default(),
            "Enter must not reach the game while it is still held from the widget"
        );
        assert!(latch, "the latch is still holding");
        // Frame 3: let go.
        assert_eq!(decide(Pad::default(), false, &mut latch), Pad::default());
        assert!(!latch, "the latch releases once every game key is up");
        // Frame 4: a fresh press is gameplay again.
        assert_eq!(decide(enter, false, &mut latch), enter);
    }

    /// The latch must not be able to swallow input forever: with no widget and no keys held it clears on
    /// the very next iteration.
    #[test]
    fn the_latch_cannot_stick() {
        let mut latch = true;
        assert_eq!(decide(Pad::default(), false, &mut latch), Pad::default());
        assert!(!latch);
    }

    /// Feed a **real** `egui::Context` real key events and read the pad back off it.
    ///
    /// Every test above pins the *mapping*, through a closure. None of them touches the seam that can
    /// actually be wrong in a toolkit swap: whether [`poll_pad`]'s `InputState::key_down` reports what
    /// egui stores when a key event arrives, and whether it stays true across the following frame (a
    /// player polls once per frame, and a key held down produces **no** further events). Nothing else in
    /// this crate exercises that, and it is exactly the kind of thing an egui bump changes.
    #[test]
    fn a_real_egui_context_holds_a_key_down_across_frames() {
        fn key(key: egui::Key, pressed: bool) -> egui::Event {
            egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            }
        }
        fn frame(ctx: &egui::Context, events: Vec<egui::Event>) -> Pad {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(640.0, 480.0),
                )),
                events,
                ..Default::default()
            };
            let mut seen = Pad::default();
            let mut out = ctx.run_ui(raw, |ui| seen = poll_pad(ui.ctx()));
            // See `main.rs`: dropping unapplied texture deltas panics in debug builds.
            out.textures_delta.clear();
            seen
        }

        let ctx = egui::Context::default();
        assert_eq!(frame(&ctx, vec![]), Pad::default(), "nothing pressed yet");

        // Right + S (Genesis B) go down together: run and jump.
        let pressed = frame(
            &ctx,
            vec![key(egui::Key::ArrowRight, true), key(egui::Key::S, true)],
        );
        assert!(pressed.right, "ArrowRight did not reach the pad");
        assert!(pressed.b, "S did not reach the pad as Genesis B");
        assert!(
            !pressed.c && !pressed.a && !pressed.left,
            "spurious buttons"
        );

        // A HELD key emits no further events. If `key_down` did not latch, the very next frame would
        // report the button released and the player would be unplayable in the least obvious way.
        let still = frame(&ctx, vec![]);
        assert_eq!(
            still, pressed,
            "a held key must stay down on a frame that carries no events"
        );

        // ...and releasing one releases exactly that one.
        let after = frame(&ctx, vec![key(egui::Key::S, false)]);
        assert!(after.right, "ArrowRight is still held");
        assert!(!after.b, "S was released and must not still read as B");

        let none = frame(&ctx, vec![key(egui::Key::ArrowRight, false)]);
        assert_eq!(none, Pad::default(), "everything released");
    }
}
