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
const TH_PULLUP_ROM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/blastem-differential/th_pullup.bin"
));

/// Work-RAM offset of the ROM's observable block (`$FF8000`), and of its done marker (`$FF8010`).
const OBS: usize = 0x8000;
const DONE: usize = 0x8010;

/// The bytes BlastEm 0.6.2 wrote for observables +0..+11, recorded 2026-09-18 by
/// `python3 tools/blastem-differential/run_th_pullup.py` with nothing held on the pad. Bit 7 is a separate
/// cell, settled 2026-09-25 for the latch (see `data_bit_7_follows_the_latch_through_the_bus`); bits 6-0
/// are the pull question and its controls.
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

// ---------------------------------------------------------------------------------------------------
// F-IO-DATA-BIT7 — Data-register bit 7 reads back the Data latch's own bit 7. SETTLED 2026-09-25.
// ---------------------------------------------------------------------------------------------------
//
// The port has seven I/O pins (PA0-PA6 / PB0-PB6 / PC0-PC6 on the YM6046 pinout), so no pin corresponds to
// Data bit 7. What the chip hands back for it was booked as unresolved on 2026-09-19 (ours forced `1`,
// BlastEm returned the latch). `docs/2026-09-25-io-data-bit7.md` settles it for THE LATCH, on a mechanism:
//
//   * DIE NETLIST (YM6046 / 315-5309, `emu-russia/SEGAChips` `IOChip/IO.v` @ 6ec064e): the 68000-mode
//     read mux for register 1 ($A10003) takes bit 7 straight from the Q of flip-flop `g_87`, whose D is
//     internal data-bus bit 7 and whose clock `w142` is the Port A Data write strobe shared with the other
//     seven bits of that register. No pin, no pull, no constant on that path.
//   * DIE-DERIVED EMULATOR (Nuked-MD `iochip.c` @ 9c219b3, FC1004 decap): `if (port_a.p_data.q & 128)
//     read_data |= 128;` — likewise for ports B and C.
//   * HARDWARE MEASUREMENT (Charles MacDonald, "Sega Genesis hardware notes" v0.8, section 3.1, "checked on
//     the real thing"): "Bit 7 isn't connected to any pin on the I/O port. It will latch a value written to
//     it", with the listing `$7F` → write `$80` → `$FF` → write `$00` → `$7F`.
//
// THE EXPECTATIONS BELOW ARE THOSE SOURCES' BYTES, not our core's output. MacDonald's listing is asserted
// verbatim; the netlist's rule (bit 7 = latch bit 7 whatever Control says) is asserted across Control values.

/// MacDonald's listing through the real bus, on the committed `th_pullup.bin`: its observables +0 (pristine
/// read), +11 (`$80` latched, Control `$00`) and +8 (`$00` latched, Control `$00`) are exactly his three reads,
/// `$7F` / `$FF` / `$7F`. And with bit 7 settled, **every** observable now agrees with BlastEm 0.6.2 byte for
/// byte — the whole-byte form of `undriven_th_reads_high_in_both_models`.
#[test]
fn data_bit_7_follows_the_latch_through_the_bus() {
    let ours = th_pullup_observables();
    assert_eq!(
        (ours[0], ours[11], ours[8]),
        (0x7F, 0xFF, 0x7F),
        "MacDonald gen-hw.txt 3.1: `move.b $A10003,d0 ; D0 = $7F` / after `move.b #$80` `D0 = $FF` / after \
         `move.b #$00` `D0 = $7F`. Bit 7 is the Data latch's bit 7 (YM6046 netlist g_87; Nuked-MD p_data.q & 128)."
    );
    for i in 0..12 {
        assert_eq!(
            ours[i], BLASTEM_OBSERVED[i],
            "observable +{i}: whole byte ${:02X} here vs ${:02X} in BlastEm 0.6.2 — F-IO-DATA-BIT7 closed the \
             one cell where they differed, so any disagreement now is new.",
            ours[i], BLASTEM_OBSERVED[i]
        );
    }
}

/// The netlist's rule directly on the register block, on every port: Data bit 7 is the last value written
/// there, **independently of Control** (Control bit 7 is the TH-interrupt enable, not a direction bit, and the
/// read path for Data bit 7 does not pass through any Control flip-flop), and independently of the pad.
#[test]
fn data_bit_7_is_the_latch_on_every_port_whatever_control_says() {
    use oracle_core::io::{Io, Pad, PadPort, Port};
    for port in Port::ALL {
        // MacDonald's listing verbatim, Control pristine at $00, nothing plugged/pressed.
        let mut io = Io::default();
        assert_eq!(
            io.read_data(port),
            0x7F,
            "{port:?}: pristine read is $7F (gen-hw.txt 3.0/3.1)"
        );
        io.write_data(port, 0x80);
        assert_eq!(io.read_data(port), 0xFF, "{port:?}: after writing $80, $FF");
        io.write_data(port, 0x00);
        assert_eq!(io.read_data(port), 0x7F, "{port:?}: after writing $00, $7F");

        // Bit 7 = latch bit 7 for every Control value, including Control bit 7 (TH-int enable) set and
        // clear, TH an input or an output, and all seven pins outputs.
        for ctrl in [0x00u8, 0x40, 0x80, 0xC0, 0x7F, 0xFF] {
            for latch in [0x00u8, 0x40, 0x80, 0xC0, 0x7F, 0xFF] {
                let mut io = Io::default();
                if let Some(p) = port.pad_port() {
                    // A pressed pad must not reach bit 7 either: the pad drives seven pins at most.
                    io.set_pad(
                        p,
                        Pad {
                            start: true,
                            c: true,
                            up: true,
                            ..Pad::default()
                        },
                    );
                }
                io.write_ctrl(port, ctrl);
                io.write_data(port, latch);
                assert_eq!(
                    io.read_data(port) & 0x80,
                    latch & 0x80,
                    "{port:?} ctrl=${ctrl:02X} latch=${latch:02X}: Data bit 7 must be the latch's bit 7"
                );
            }
        }
    }
    // The normal game read: Control $40, TH driven high then low, nothing pressed. Bits 6-0 are recon IO4's
    // nibbles; bit 7 is the `0` the game latched. `$7F` / `$33`, as BlastEm recorded at +6 / +7.
    let mut io = Io::default();
    io.set_pad(PadPort::P1, Pad::default());
    io.write_ctrl(Port::P1, 0x40);
    io.write_data(Port::P1, 0x40);
    assert_eq!(
        io.read_data(Port::P1),
        0x7F,
        "TH high, released: ?1CBRLDU with ? = latched 0"
    );
    io.write_data(Port::P1, 0x00);
    assert_eq!(
        io.read_data(Port::P1),
        0x33,
        "TH low, released: ?0SA00DU with ? = latched 0"
    );
}
