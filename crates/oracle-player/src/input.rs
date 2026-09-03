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

#[cfg(test)]
mod tests {
    use super::*;

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
}
