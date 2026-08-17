//! Host **gamepad** input (feature `gamepad`) — the second live input source alongside the keyboard.
//!
//! Like the `audio` module this is strictly frontend-only: `gilrs` never enters `oracle-core`, whose charter
//! is "deterministic, no I/O". The core is still driven solely through [`System::set_pad`], so a gamepad is
//! just another producer of the same injected [`Pad`] bytes a keyboard produces.
//!
//! ## Shape
//!
//! * [`map_gamepad`] is a **pure** function from "is this host button pressed / what is this axis worth" to a
//!   [`Pad`]. It is the whole mapping policy and it is unit-testable with no hardware attached (this machine
//!   has no controller and likely no `/dev/input`; see the tests below).
//! * [`merge_pads`] ORs two [`Pad`]s per button, so keyboard and gamepad are simultaneous, never either/or —
//!   plugging a controller in must not make the keyboard go dead.
//! * [`Gamepads`] is the only part that touches hardware. It owns the `gilrs` context, assigns physical
//!   controllers to emulated ports 0/1 (P1/P2) and pumps hotplug events. Construction returns [`Option`]:
//!   **every** failure path (unsupported platform, backend error) logs one line and degrades to keyboard-only
//!   rather than panicking — the same contract `build_audio` keeps for a missing sound device.
//!
//! ## Remapping
//!
//! Everything a remap needs is in [`BUTTON_MAP`], [`AXIS_MAP`] and [`STICK_DEADZONE`]. No host button constant
//! appears anywhere else in this crate.

use gilrs::{Axis, Button, Event, EventType, Gilrs};
use oracle_core::io::Pad;

/// A Genesis 3-button pad control. The Mega Drive pad has exactly these eight inputs — there is deliberately
/// no room here for host buttons the hardware does not have (no Select, no shoulders, no Mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadButton {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    C,
    Start,
}

/// **THE DEFAULT BUTTON MAPPING TABLE** — the single source of truth for host-button → Genesis-button. Edit
/// this (and [`AXIS_MAP`]) to remap; nothing else in the crate names a [`Button`].
///
/// Face buttons follow the layout convention every Mega Drive-on-a-modern-pad setup uses: the Genesis A/B/C
/// row runs left→right across the bottom of the pad, so it lands on the modern diamond's left/bottom/right —
/// `West`(X/□) = A, `South`(A/✕) = B, `East`(B/○) = C. `North` (Y/△) is intentionally unmapped: the 3-button
/// pad has no fourth face button and this slice does not invent one.
pub const BUTTON_MAP: &[(Button, PadButton)] = &[
    // D-pad (devices whose hat is reported as buttons; hat-as-axis devices are covered by `AXIS_MAP`).
    (Button::DPadUp, PadButton::Up),
    (Button::DPadDown, PadButton::Down),
    (Button::DPadLeft, PadButton::Left),
    (Button::DPadRight, PadButton::Right),
    // Face buttons.
    (Button::West, PadButton::A),
    (Button::South, PadButton::B),
    (Button::East, PadButton::C),
    // Menu.
    (Button::Start, PadButton::Start),
];

/// **THE DEFAULT AXIS MAPPING TABLE** — `(axis, button at the negative end, button at the positive end)`.
///
/// Covers the left analog stick and the D-pad-reported-as-a-hat-axis case (`gilrs` surfaces a hat either as
/// `DPad*` buttons or as `DPadX`/`DPadY`, depending on the device and its SDL mapping; handling both is why
/// the d-pad appears in *both* tables). `gilrs` normalises stick axes to `[-1.0, 1.0]` with **+Y = up**, so
/// the Y rows read "negative = Down, positive = Up".
pub const AXIS_MAP: &[(Axis, PadButton, PadButton)] = &[
    (Axis::LeftStickX, PadButton::Left, PadButton::Right),
    (Axis::LeftStickY, PadButton::Down, PadButton::Up),
    (Axis::DPadX, PadButton::Left, PadButton::Right),
    (Axis::DPadY, PadButton::Down, PadButton::Up),
];

/// Analog deadzone **default**: an axis counts as a direction press only at or past this magnitude. 0.5 is a
/// deliberately generous half-throw — a digital d-pad press must be unambiguous for a platformer, and it also
/// makes a diagonal require real intent on both axes. A hat axis reports exactly ±1.0, so it clears this
/// trivially. The live value is per-[`Gamepads`] instance (config-fed, see `gamepad_default_deadzone` in
/// `main.rs`); this const is only where that default comes from.
pub const STICK_DEADZONE: f32 = 0.5;

/// Set one [`PadButton`] on `pad`. The only place the [`PadButton`] enum meets [`Pad`]'s fields.
fn press(pad: &mut Pad, which: PadButton) {
    match which {
        PadButton::Up => pad.up = true,
        PadButton::Down => pad.down = true,
        PadButton::Left => pad.left = true,
        PadButton::Right => pad.right = true,
        PadButton::A => pad.a = true,
        PadButton::B => pad.b = true,
        PadButton::C => pad.c = true,
        PadButton::Start => pad.start = true,
    }
}

/// Build a [`Pad`] from one controller's state by applying [`BUTTON_MAP`] and [`AXIS_MAP`].
///
/// Pure and hardware-free: callers pass closures reading a live `gilrs::Gamepad`, tests pass canned ones. A
/// control that is absent on the physical device simply never reports pressed (`gilrs` returns `false` / `0.0`
/// for an unmapped element), so an unmapped d-pad or a stick-less pad degrades to "that direction is not
/// held" rather than misbehaving. Both tables contribute by OR, so a stick and a d-pad can be used
/// interchangeably. `deadzone` is the caller's live value (see [`STICK_DEADZONE`] for the default it starts
/// from) — kept as a parameter rather than a const read so it is configurable per [`Gamepads`] instance.
pub fn map_gamepad<P, A>(is_pressed: P, axis_value: A, deadzone: f32) -> Pad
where
    P: Fn(Button) -> bool,
    A: Fn(Axis) -> f32,
{
    let mut pad = Pad::default();
    for &(host, target) in BUTTON_MAP {
        if is_pressed(host) {
            press(&mut pad, target);
        }
    }
    for &(axis, negative, positive) in AXIS_MAP {
        let v = axis_value(axis);
        if v <= -deadzone {
            press(&mut pad, negative);
        } else if v >= deadzone {
            press(&mut pad, positive);
        }
    }
    pad
}

/// Combine two input sources for the same port by logical OR per button — a button is held if **either**
/// source holds it. This is what keeps the keyboard alive while a controller is plugged in (requirement: the
/// two sources merge, they never replace each other).
pub fn merge_pads(a: Pad, b: Pad) -> Pad {
    Pad {
        up: a.up || b.up,
        down: a.down || b.down,
        left: a.left || b.left,
        right: a.right || b.right,
        a: a.a || b.a,
        b: a.b || b.b,
        c: a.c || b.c,
        start: a.start || b.start,
    }
}

/// Emulated controller ports a physical gamepad can occupy: 0 = Player 1, 1 = Player 2. The core has a third
/// I/O port (EXP) but it has no pad, so it is not a target here.
const PORTS: usize = 2;

/// Claim the lowest free port for `id`, returning the port index. `None` = nothing changed: `id` already owns
/// a port, or both ports are taken (a third controller is simply ignored — never a panic).
///
/// Generic over the id type purely for testability: `gilrs::GamepadId` cannot be constructed outside `gilrs`,
/// so the hotplug bookkeeping is tested with `usize` ids.
fn assign_port<T: Copy + PartialEq>(ports: &mut [Option<T>; PORTS], id: T) -> Option<usize> {
    if ports.iter().any(|p| p.as_ref() == Some(&id)) {
        return None;
    }
    let free = ports.iter().position(Option::is_none)?;
    ports[free] = Some(id);
    Some(free)
}

/// Release the port held by `id`, returning the freed index, or `None` if `id` held none. The freed slot is
/// reusable immediately, so unplugging P1 and plugging a different controller back in restores Player 1.
fn release_port<T: Copy + PartialEq>(ports: &mut [Option<T>; PORTS], id: T) -> Option<usize> {
    let at = ports.iter().position(|p| p.as_ref() == Some(&id))?;
    ports[at] = None;
    Some(at)
}

/// Live host-gamepad state: the `gilrs` context plus the physical-controller → emulated-port assignment.
///
/// Held for the whole run, polled once per frame. Only constructed via [`Gamepads::new`], which returns `None`
/// (→ keyboard-only) instead of failing hard.
pub struct Gamepads {
    gilrs: Gilrs,
    ports: [Option<gilrs::GamepadId>; PORTS],
    /// Analog deadzone applied by [`Gamepads::poll`] — config-fed; [`STICK_DEADZONE`] is only its default.
    deadzone: f32,
}

impl Gamepads {
    /// Initialise the gamepad backend and seat any already-connected controllers, printing one line per
    /// detected controller (matching `build_audio`'s startup-log style).
    ///
    /// Returns `None` — **run keyboard-only** — on ANY initialisation failure (platform unsupported, backend
    /// error such as no `/dev/input` access), after printing a one-line warning. It **never panics**. Having
    /// no controller attached is not a failure: that returns `Some` with both ports empty, because a
    /// controller plugged in later is picked up by [`Gamepads::poll`]'s hotplug handling.
    ///
    /// `deadzone` is the live analog deadzone this instance polls with (config-fed; callers with no config
    /// value yet should pass [`STICK_DEADZONE`] or `gamepad_default_deadzone()` from `main.rs`).
    pub fn new(deadzone: f32) -> Option<Self> {
        let gilrs = match Gilrs::new() {
            Ok(g) => g,
            // `NotImplemented` carries a usable dummy context, but a dummy reports no gamepads forever — the
            // honest report is "no gamepad support here", on the same keyboard-only path as a hard error.
            Err(gilrs::Error::NotImplemented(_)) => {
                eprintln!("gamepad: not supported on this platform — keyboard only");
                return None;
            }
            Err(e) => {
                eprintln!("gamepad: init failed ({e}) — keyboard only");
                return None;
            }
        };

        let mut ports = [None; PORTS];
        for (id, gp) in gilrs.gamepads() {
            match assign_port(&mut ports, id) {
                Some(port) => println!("gamepad: port {} = {} — connected", port + 1, gp.name()),
                None => println!(
                    "gamepad: ignoring extra controller {} (both ports in use)",
                    gp.name()
                ),
            }
        }
        if ports.iter().all(Option::is_none) {
            println!(
                "gamepad: no controllers detected — keyboard only (hotplug is picked up live)"
            );
        }
        Some(Self {
            gilrs,
            ports,
            deadzone,
        })
    }

    /// Pump hotplug events and sample both ports, returning `[player1, player2]`.
    ///
    /// Draining `next_event` is what keeps `gilrs`' cached button state current *and* is where connect /
    /// disconnect are noticed, so this must run every frame. A port with no controller — never had one, or
    /// just lost one — yields `Pad::default()` (all released), which OR-merges to "keyboard only" for that
    /// player. Nothing here can panic: an id whose device vanished between the event pump and the sample
    /// resolves through `connected_gamepad` to `None` and is skipped.
    pub fn poll(&mut self) -> [Pad; PORTS] {
        while let Some(Event { id, event, .. }) = self.gilrs.next_event() {
            match event {
                EventType::Connected => {
                    let name = self.gilrs.gamepad(id).name().to_string();
                    match assign_port(&mut self.ports, id) {
                        Some(port) => println!("gamepad: port {} = {name} — connected", port + 1),
                        None => println!(
                            "gamepad: ignoring extra controller {name} (both ports in use)"
                        ),
                    }
                }
                EventType::Disconnected => {
                    if let Some(port) = release_port(&mut self.ports, id) {
                        println!(
                            "gamepad: port {} disconnected — keyboard only for that player",
                            port + 1
                        );
                    }
                }
                _ => {}
            }
        }

        let mut pads = [Pad::default(); PORTS];
        for (port, slot) in self.ports.iter().enumerate() {
            if let Some(gp) = slot.and_then(|id| self.gilrs.connected_gamepad(id)) {
                pads[port] = map_gamepad(|b| gp.is_pressed(b), |a| gp.value(a), self.deadzone);
            }
        }
        pads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sample the mapping with a canned set of pressed buttons and no stick deflection.
    fn pad_from_buttons(pressed: &[Button]) -> Pad {
        map_gamepad(|b| pressed.contains(&b), |_| 0.0, STICK_DEADZONE)
    }

    /// Sample the mapping with a canned axis value and no buttons pressed, at the default deadzone.
    fn pad_from_axis(target: Axis, value: f32) -> Pad {
        pad_from_axis_deadzone(target, value, STICK_DEADZONE)
    }

    /// Sample the mapping with a canned axis value, no buttons pressed, and an explicit deadzone —
    /// what [`deadzone_is_configurable_per_instance`] drives to prove the deadzone is a parameter,
    /// not a hardcoded const, in the pure mapping helper.
    fn pad_from_axis_deadzone(target: Axis, value: f32, deadzone: f32) -> Pad {
        map_gamepad(
            |_| false,
            |a| if a == target { value } else { 0.0 },
            deadzone,
        )
    }

    /// The face-button and Start rows of [`BUTTON_MAP`]: West/South/East → A/B/C, Start → Start, and the
    /// unmapped North (Y/△) presses nothing (the 3-button pad has no fourth face button).
    #[test]
    fn face_buttons_map_to_a_b_c_and_start() {
        assert_eq!(
            pad_from_buttons(&[Button::West]),
            Pad {
                a: true,
                ..Pad::default()
            }
        );
        assert_eq!(
            pad_from_buttons(&[Button::South]),
            Pad {
                b: true,
                ..Pad::default()
            }
        );
        assert_eq!(
            pad_from_buttons(&[Button::East]),
            Pad {
                c: true,
                ..Pad::default()
            }
        );
        assert_eq!(
            pad_from_buttons(&[Button::Start]),
            Pad {
                start: true,
                ..Pad::default()
            }
        );
        // Buttons the Mega Drive pad does not have must do nothing at all.
        assert_eq!(
            pad_from_buttons(&[
                Button::North,
                Button::Select,
                Button::Mode,
                Button::LeftTrigger
            ]),
            Pad::default(),
            "unmapped host buttons press nothing"
        );
    }

    /// The digital d-pad rows, including a simultaneous diagonal.
    #[test]
    fn dpad_buttons_map_to_directions() {
        assert!(pad_from_buttons(&[Button::DPadUp]).up);
        assert!(pad_from_buttons(&[Button::DPadDown]).down);
        assert!(pad_from_buttons(&[Button::DPadLeft]).left);
        assert!(pad_from_buttons(&[Button::DPadRight]).right);
        let diag = pad_from_buttons(&[Button::DPadUp, Button::DPadRight]);
        assert!(diag.up && diag.right && !diag.down && !diag.left);
    }

    /// The analog stick: past the deadzone in either direction presses the matching direction, and `gilrs`'
    /// +Y = up sign convention is honoured (a common way to ship an upside-down stick).
    #[test]
    fn analog_stick_past_deadzone_presses_directions() {
        assert!(pad_from_axis(Axis::LeftStickX, 1.0).right);
        assert!(pad_from_axis(Axis::LeftStickX, -1.0).left);
        assert!(
            pad_from_axis(Axis::LeftStickY, 1.0).up,
            "gilrs +Y is up, so a pushed-forward stick is Up"
        );
        assert!(pad_from_axis(Axis::LeftStickY, -1.0).down);
        // A hat reported as an axis reaches the same directions.
        assert!(pad_from_axis(Axis::DPadX, 1.0).right);
        assert!(pad_from_axis(Axis::DPadY, -1.0).down);
    }

    /// Deadzone behaviour: resting/small deflections press nothing; exactly at the threshold counts as
    /// pressed. This is the test that fails if the deadzone comparison is dropped or inverted.
    #[test]
    fn analog_stick_inside_deadzone_is_neutral() {
        for v in [0.0, 0.1, STICK_DEADZONE - 0.01, -0.49, -0.0] {
            assert_eq!(
                pad_from_axis(Axis::LeftStickX, v),
                Pad::default(),
                "stick at {v} is inside the deadzone and must press nothing"
            );
            assert_eq!(pad_from_axis(Axis::LeftStickY, v), Pad::default());
        }
        assert!(
            pad_from_axis(Axis::LeftStickX, STICK_DEADZONE).right,
            "exactly at the deadzone counts as pressed"
        );
    }

    /// The deadzone is per-instance now (config-fed); the const is only the default. A tighter
    /// deadzone turns the same axis magnitude into a press that the default would ignore.
    #[test]
    fn deadzone_is_configurable_per_instance() {
        // At the default deadzone (0.5), a 0.4 deflection is inside it — not a press.
        assert_eq!(
            pad_from_axis_deadzone(Axis::LeftStickX, 0.4, 0.5),
            Pad::default()
        );
        assert_eq!(
            pad_from_axis_deadzone(Axis::LeftStickX, -0.4, 0.5),
            Pad::default()
        );
        // At a tighter instance deadzone (0.3), the same 0.4 deflection clears it — a press.
        assert!(pad_from_axis_deadzone(Axis::LeftStickX, 0.4, 0.3).right);
        assert!(pad_from_axis_deadzone(Axis::LeftStickX, -0.4, 0.3).left);
    }

    /// Merging is a per-button OR, and merging with an all-released pad is the identity — i.e. an idle
    /// controller can never suppress a held key, which is the "keyboard must keep working" requirement.
    #[test]
    fn merge_is_per_button_or_and_never_suppresses() {
        let keyboard = Pad {
            up: true,
            a: true,
            ..Pad::default()
        };
        let stick = Pad {
            right: true,
            a: true,
            start: true,
            ..Pad::default()
        };
        let merged = merge_pads(keyboard, stick);
        assert_eq!(
            merged,
            Pad {
                up: true,
                right: true,
                a: true,
                start: true,
                ..Pad::default()
            }
        );
        // An unplugged / idle gamepad contributes Pad::default() → the keyboard passes through untouched.
        assert_eq!(merge_pads(keyboard, Pad::default()), keyboard);
        assert_eq!(merge_pads(Pad::default(), stick), stick);
    }

    /// Port bookkeeping, including the mid-session hotplug cases: first controller takes P1, second takes P2,
    /// a third is ignored, re-announcing a seated controller is a no-op, and a disconnect frees exactly its
    /// own slot for the next connect.
    #[test]
    fn port_assignment_handles_hotplug() {
        let mut ports: [Option<usize>; PORTS] = [None; PORTS];
        assert_eq!(assign_port(&mut ports, 10), Some(0), "first → Player 1");
        assert_eq!(assign_port(&mut ports, 11), Some(1), "second → Player 2");
        assert_eq!(assign_port(&mut ports, 12), None, "third is ignored");
        assert_eq!(assign_port(&mut ports, 10), None, "already seated");
        assert_eq!(ports, [Some(10), Some(11)]);

        // Player 1 unplugs mid-session: only port 0 is freed, Player 2 keeps playing.
        assert_eq!(release_port(&mut ports, 10), Some(0));
        assert_eq!(ports, [None, Some(11)]);
        assert_eq!(release_port(&mut ports, 10), None, "already released");
        assert_eq!(release_port(&mut ports, 99), None, "unknown id");

        // A new controller plugged in takes the freed Player-1 slot.
        assert_eq!(assign_port(&mut ports, 12), Some(0));
        assert_eq!(ports, [Some(12), Some(11)]);
    }

    /// The graceful-degradation contract, mirroring `start_audio_never_panics`. There is no controller (and
    /// likely no `/dev/input`) in this build environment, so this normally exercises the "no controllers
    /// detected" / init-failure path; on a machine with a pad it may return `Some`. Either way the contract
    /// under test is that initialisation never panics and always yields a usable decision.
    #[test]
    fn gamepads_new_never_panics() {
        let pads = Gamepads::new(STICK_DEADZONE);
        // And polling a live context (if we got one) is likewise panic-free with nothing plugged in.
        if let Some(mut g) = pads {
            let [p1, p2] = g.poll();
            let _ = (p1, p2);
        }
    }
}
