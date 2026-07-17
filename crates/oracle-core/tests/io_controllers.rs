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
    sys.set_pad(0, pad);
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
