//! End-to-end proof for the controller/I/O push: a real 68000 fixture ROM polls Player 1 through the actual
//! 3-button TH protocol (recon IO4) and the injected pad state visibly changes the rendered frame.
//!
//! The fixture (`testrom::build_pad_poll`) zeroes VRAM so the whole screen is the backdrop, then loops
//! reading the TH=0 nibble and setting the backdrop colour register from **Start**. This test injects input
//! through the only input path there is — [`System::set_pad`] — runs frames, and asserts a rendered pixel
//! flipped. Nothing here reads `Io` state directly; the assertion is on the pixels the guest produced.

use oracle_core::io::Pad;
use oracle_core::system::System;
use oracle_core::testrom::build_pad_poll;

/// Boot the pad-poll fixture with `pad` injected on Player 1, run a few frames, and return a backdrop pixel.
fn backdrop_pixel_with(pad: Pad) -> (u8, u8, u8) {
    let mut sys = System::new(0x5EED);
    sys.load_rom(build_pad_poll());
    sys.reset(); // reset re-powers-on (clears Io) — inject AFTER it, as a frontend would.
    sys.set_pad(oracle_core::io::PadPort::P1, pad);
    sys.run_frames(3);
    // Every pixel is the backdrop (VRAM is zeroed → all planes/sprites transparent); sample the centre.
    let line = sys.vdp().render_line(112);
    line[line.len() / 2]
}

#[test]
fn holding_start_changes_the_backdrop_via_the_real_pad_protocol() {
    let released = backdrop_pixel_with(Pad::default());
    let held = backdrop_pixel_with(Pad {
        start: true,
        ..Default::default()
    });
    assert_ne!(
        released, held,
        "holding Start must flip the backdrop the ROM selected (released={released:?}, held={held:?})"
    );
    // The ROM maps released → CRAM 1 (white $0EEE) and held → CRAM 2 (red $000E). Assert the actual colours
    // so a regression that merely *changes* the backdrop (rather than flipping it correctly) is caught too.
    // CRAM 1 = $0EEE (all channels level 7) and CRAM 2 = $000E (red level 7); a level-7 channel renders to
    // 255 under the Normal ramp (14 × 255 / 14), S/H disabled.
    assert_eq!(
        released,
        (255, 255, 255),
        "released → white backdrop (CRAM 1)"
    );
    assert_eq!(held, (255, 0, 0), "Start held → red backdrop (CRAM 2)");
}

#[test]
fn an_unrelated_button_does_not_flip_the_start_backdrop() {
    // Injecting a different button (C) must NOT read as Start — the TH-nibble decode is real, not a
    // "any button pressed" shortcut.
    let released = backdrop_pixel_with(Pad::default());
    let c_held = backdrop_pixel_with(Pad {
        c: true,
        ..Default::default()
    });
    assert_eq!(
        released, c_held,
        "C is on the TH=1 nibble; the ROM tests Start on TH=0, so the backdrop stays released"
    );
}

// ---------------------------------------------------------------------------------------------------
// F-TH-PULLUP-UNDISCRIMINATED — the undriven-TH pull direction, as a cross-model differential.
// ---------------------------------------------------------------------------------------------------
//
// `docs/2026-07-17-io-recon.md` IO3 pins that a port pin configured as an input with nothing driving it
// reads HIGH. Every pad-detection argument in this tree stands on it — including the 2026-09-18 answer to
// `docs/2026-07-25-testrom-conformance.md`'s Q1, which concluded that `vdp_sprite_masking`'s on-screen
// "press Start" text is stale because the ROM never makes TH an output, so bit 5 is `C`. If the pull were
// the other way, bit 5 WOULD be `Start` and the ROM's text would have been right.
//
// WHY THE EXPECTATIONS BELOW ARE NOT OUR OWN OUTPUT. Our core implements the rule under test, so asking it
// and asserting what it says cannot fail: that is a consistency check, not evidence. The bytes asserted
// here were recorded from **BlastEm 0.6.2**, an independently written model, running the SAME ROM IMAGE
// this test loads — `tools/blastem-differential/th_pullup.bin`, driven by `run_th_pullup.py` over the
// GDB-remote stub. One instrument, two models. Full evidence chain and the documented third leg (the
// 3-button pad's own discrete pull-up on the select line) in `docs/2026-09-19-th-pullup.md`.
//
// WHAT WOULD MAKE THIS TEST WRONG, stated so it is not mistaken for proof of hardware:
//   * BlastEm could be wrong. It has already been caught with a blind spot in this very rig (STOP × trace,
//     see `tools/blastem-differential/README.md`), and two models can inherit one documentation error.
//     Real hardware is what this test cannot substitute for; it is what remains open on the row.
//   * If BlastEm's pad model ever stops presenting an all-released 3-button pad, the recorded table stops
//     describing the released case and the controls below are what would catch it.
// Either way the test is a DIFFERENTIAL: it goes red when our core stops agreeing with the other model,
// which is the only thing an in-tree gate can honestly police.

/// The ROM image both arms run. Built by `tools/blastem-differential/build_th_pullup.py`; a wrong path is a
/// compile error rather than a silently skipped test.
const TH_PULLUP_ROM: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/blastem-differential/th_pullup.bin"));

/// Work-RAM offset of the ROM's observable block (`$FF8000`), and of its done marker (`$FF8010`).
const OBS: usize = 0x8000;
const DONE: usize = 0x8010;

/// The bytes BlastEm 0.6.2 wrote for observables +0..+11, recorded 2026-09-18 by
/// `python3 tools/blastem-differential/run_th_pullup.py` with nothing held on the pad. Bit 7 is a separate,
/// unresolved cell (see `bit_7_of_the_data_register_is_a_recorded_divergence`); bits 6-0 are the pull
/// question and its controls.
///
/// +0  `$A10003` pristine        `$7F`  bits 6-0 all high  → TH READS HIGH while an input
/// +1  `$A10009` pristine        `$00`  premise: Control powers on at `$00`, observed not assumed
/// +2  `$A10005` pristine        `$7F`  the same answer on Port 2
/// +3  `$A1000B` pristine        `$00`  premise, Port 2
/// +4  `$A10001` version         `$A0`  the block answers, and not with `$FF`
/// +5  latch `$40`, ctrl `$00`   `$7F`  with +8, rules out an input pin echoing its own latch
/// +6  ctrl `$40`, latch `$40`   `$7F`  CONTROL: TH driven high
/// +7  ctrl `$40`, latch `$00`   `$33`  CONTROL: TH driven low — bit 6 clear, bits 3-2 forced low
/// +8  back to ctrl `$00`        `$7F`  TH an input again
/// +9  `$A10009` read back       `$00`
/// +10 ctrl `$40`, latch `$C0`   `$FF`  bit 7 = the latch, in BlastEm's model
/// +11 ctrl `$00`, latch `$80`   `$FF`  likewise, with TH an input
const BLASTEM_OBSERVED: [u8; 12] = [
    0x7F, 0x00, 0x7F, 0x00, 0xA0, 0x7F, 0x7F, 0x33, 0x7F, 0x00, 0xFF, 0xFF,
];

/// Run `th_pullup.bin` in our core to its done marker and return its twelve observables.
///
/// Loud on unmeasurable: an absent or wrong done marker panics naming what did not happen, so a run that
/// never reached the end cannot be read as agreement.
fn th_pullup_observables() -> [u8; 12] {
    let mut sys = System::new(0x5EED);
    sys.load_rom(TH_PULLUP_ROM.to_vec());
    sys.reset();
    // No pad injected: `reset` leaves an all-released 3-button pad on both ports, which is the state
    // BlastEm was in (headless, no key held). The ROM touches no VDP, so instruction stepping suffices.
    let budget = 4_000;
    let mut steps = 0;
    while steps < budget {
        sys.step_instruction();
        steps += 1;
        if u16::from_be_bytes([sys.ram()[DONE], sys.ram()[DONE + 1]]) == 0xC0DE {
            break;
        }
    }
    let marker = u16::from_be_bytes([sys.ram()[DONE], sys.ram()[DONE + 1]]);
    assert_eq!(
        marker, 0xC0DE,
        "DID NOT MEASURE: th_pullup.bin never reached its done marker in {budget} instructions \
         (marker=${marker:04X}; $DEAD means an exception vector was taken). This is an absence, not a \
         finding — nothing below may be read as agreement."
    );
    let mut out = [0u8; 12];
    out.copy_from_slice(&sys.ram()[OBS..OBS + 12]);
    out
}

/// **Bits 6-0 of every observable agree with BlastEm**, which is what makes IO3's pull direction a
/// cross-model result rather than a restatement of our own code.
///
/// The two controls are what give the main cells meaning. At +6 TH is genuinely an output driven high and
/// at +7 genuinely an output driven low: +7 reading `$33` (bit 6 clear, bits 3-2 forced low, Start/A/Down/Up
/// released) proves this ROM and this readback path CAN carry a low TH, so +0/+5/+8 reading `$7F` is a
/// measurement of a high TH rather than a stuck value. +5 (latch `$40`) and +8 (latch `$00`), both with
/// Control at `$00`, agreeing at `$7F` is what separates a real pull-up from a model that hands an input
/// pin back its own latch — under that hypothesis they would differ.
#[test]
fn undriven_th_reads_high_in_both_models() {
    let ours = th_pullup_observables();
    for i in 0..12 {
        assert_eq!(
            ours[i] & 0x7F,
            BLASTEM_OBSERVED[i] & 0x7F,
            "observable +{i}: bits 6-0 are ${:02X} here and ${:02X} in BlastEm 0.6.2 (whole bytes \
             ${:02X} vs ${:02X}). A disagreement here is a core-behaviour question, not a harness one: \
             it reopens `docs/2026-07-25-testrom-conformance.md` Q1, whose answer needs an \
             input-configured TH to read high.",
            ours[i] & 0x7F,
            BLASTEM_OBSERVED[i] & 0x7F,
            ours[i],
            BLASTEM_OBSERVED[i],
        );
    }
    // Say the three-way discriminator out loud, so the pattern is asserted and not merely implied.
    assert_eq!(
        (ours[0] & 0x7F, ours[5] & 0x7F, ours[8] & 0x7F),
        (0x7F, 0x7F, 0x7F),
        "TH reads HIGH while an input. `$33` at all three would be a pull-down; `($33, $7F, $33)` would \
         be an input pin echoing its latch and would say nothing about the pull."
    );
    assert_eq!(ours[1], 0x00, "premise: P1 Control powers on at $00");
    assert_eq!(ours[3], 0x00, "premise: P2 Control powers on at $00");
    assert_eq!(
        ours[7] & 0x7F,
        0x33,
        "detector-positive control: with TH DRIVEN low, bit 6 is clear and bits 3-2 are forced low. \
         Without this, `$7F` above would not be distinguishable from a stuck read."
    );
}

/// **Bit 7 of the Data register is a recorded divergence, not a settled cell** — and this test pins the
/// disagreement rather than either answer, so that changing our side is loud.
///
/// The port has seven I/O pins: Control is bits 6-0 and its bit 7 is the TH-interrupt enable, so no pin
/// corresponds to Data bit 7 at all. Recon IO4's table asserts it reads `1` ("pull-up, undriven") and
/// `pad_device_byte` implements that unconditionally. BlastEm 0.6.2 instead hands back **the Data latch's
/// own bit 7**: `0` at every observable until the ROM writes `$C0`/`$80`, then `1` — and independently of
/// the Control register, which has no direction bit for it.
///
/// Neither side is corroborated by a permitted source: Plutiedev's I/O-ports and Controllers pages do not
/// mention bit 7 of the register at all. So this is booked as **F-IO-DATA-BIT7** rather than fixed, with
/// the same standing as `tools/blastem-differential/known_differences.py`'s STOP×trace entry.
///
/// If you are here because this test went red: you have changed our bit-7 behaviour. That is a
/// core-behaviour decision that needs the row closed on evidence first — re-run `run_th_pullup.py`, record
/// the new table, and amend IO4 — not a table edit.
#[test]
fn bit_7_of_the_data_register_is_a_recorded_divergence() {
    let ours = th_pullup_observables();
    for i in [0usize, 5, 6, 7, 8, 10, 11] {
        assert_eq!(
            ours[i] & 0x80,
            0x80,
            "observable +{i}: our bit 7 is 0. Ours is documented (recon IO4) as always 1; changing it is \
             F-IO-DATA-BIT7's to decide, and closing that row means re-recording BLASTEM_OBSERVED too."
        );
    }
    // And the divergence itself: BlastEm read bit 7 low exactly where the ROM had not latched a 1 there.
    for i in [0usize, 5, 6, 7, 8] {
        assert_eq!(
            BLASTEM_OBSERVED[i] & 0x80,
            0x00,
            "the recorded BlastEm table must still carry the divergence this test names at +{i}; if it no \
             longer does, the table was edited without re-running the rig"
        );
    }
    for i in [10usize, 11] {
        assert_eq!(
            BLASTEM_OBSERVED[i] & 0x80,
            0x80,
            "+{i}: BlastEm's bit 7 follows the latch, so it must read 1 once the ROM has latched one there"
        );
    }
}
