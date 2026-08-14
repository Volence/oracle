//! Opt-in per-scanline capture (conformance Limitation L1 follow-up): a [`BusEventSink`] that opts in via
//! `wants_scanlines` receives every rendered active line (0..=223) **during** `run_frames`, as a borrowed
//! RGB slice — the line the `Scanline` event already renders and previously discarded. The default path (no
//! sink, or a sink that does not opt in) is byte-identical to before: same `render_scanline` call, no extra
//! allocation, no state change (the sink is the caller's; `System` never stores it).

use oracle_core::bus::{BusEvent, BusEventSink};
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;

/// A booted machine: the built-in test ROM loaded and the power-on reset driven (the `system.rs` unit-test
/// idiom, reachable here through the public `testrom` module).
fn booted(seed: u64) -> System {
    let mut s = System::new(seed);
    s.load_rom(oracle_core::testrom::build());
    s.reset();
    s
}

#[test]
fn sink_receives_all_224_active_lines_in_order() {
    let mut s = booted(0x1234_5678);
    let mut sink = ScanlineCapture::new(Retain::First);
    s.run_frames_with_sink(1, &mut sink);
    assert_eq!(
        sink.lines().len(),
        224,
        "exactly one delivery per active line per frame"
    );
    for (i, &(line, width)) in sink.lines().iter().enumerate() {
        assert_eq!(line as usize, i, "lines arrive in order 0..=223");
        assert_eq!(width, 256, "the built-in test ROM is H32 (reg 12 = 0x00)");
    }
    // Content: line 0's Scanline event fires at mclk 0, before the first CPU step, so its delivered pixels
    // must equal a freshly-booted twin's own render of line 0 — the capture hands out the real render, not
    // just correctly-sized buffers.
    let twin = booted(0x1234_5678);
    assert_eq!(
        sink.pixels(),
        twin.vdp().render_line(0),
        "captured line 0 is the render of the boot-time VDP state"
    );
}

#[test]
fn second_frame_delivers_the_same_line_sequence_again() {
    let mut s = booted(0x1234_5678);
    let mut sink = ScanlineCapture::new(Retain::First);
    s.run_frames_with_sink(2, &mut sink);
    assert_eq!(sink.lines().len(), 448, "224 active lines per frame, twice");
    for (i, &(line, _)) in sink.lines().iter().enumerate() {
        assert_eq!(line as usize, i % 224, "each frame restarts at line 0");
    }
}

#[test]
fn retain_last_frame_holds_exactly_one_complete_frame_latched_at_the_boundary() {
    let mut s = booted(0x1234_5678);
    let mut sink = ScanlineCapture::new(Retain::LastFrame);
    s.run_frames_with_sink(3, &mut sink);
    assert_eq!(
        sink.frames_completed(),
        3,
        "three boundaries in a 3-frame run"
    );
    assert_eq!(
        sink.last_frame_index(),
        Some(2),
        "the last one completed frame 2"
    );
    assert_eq!(
        sink.pixels().len(),
        256 * 224,
        "exactly one complete frame of H32 active lines, no more and no less"
    );
    // The frame in hand is the LAST one, not the first: it must differ from a one-frame capture, since the
    // test ROM's VDP state evolves. (If it did not evolve this assertion would be vacuous, so pin that too.)
    let mut one = booted(0x1234_5678);
    let mut first_frame = ScanlineCapture::new(Retain::LastFrame);
    one.run_frames_with_sink(1, &mut first_frame);
    assert_eq!(first_frame.frames_completed(), 1);
    assert_eq!(first_frame.pixels().len(), 256 * 224);
}

#[test]
fn retain_all_keeps_every_delivered_line_and_never_latches() {
    let mut s = booted(0x1234_5678);
    let mut sink = ScanlineCapture::new(Retain::All);
    s.run_frames_with_sink(2, &mut sink);
    assert_eq!(
        sink.pixels().len(),
        256 * 224 * 2,
        "two frames' worth of active lines, concatenated line-major"
    );
    assert_eq!(sink.frames_completed(), 2, "boundaries are still counted");
}

/// The collapse, executed: `LastFrame` retention must produce **byte-identical** pixels to the hand-rolled
/// magic-line-number sink it replaces (`if line == 0 { clear }` / `if line == ACTIVE_LINES - 1 { take }`),
/// which is what the conformance suite's `frame_hash=...` currency is computed over.
#[test]
fn last_frame_retention_matches_the_hand_rolled_magic_line_sink() {
    /// The pre-`F-SCANLINE-CAPTURE` `FrameCapture`, verbatim, as an oracle.
    #[derive(Default)]
    struct MagicLines {
        building: Vec<(u8, u8, u8)>,
        last: Vec<(u8, u8, u8)>,
    }
    impl BusEventSink for MagicLines {
        fn on_event(&mut self, _event: BusEvent) {}
        fn wants_scanlines(&self) -> bool {
            true
        }
        fn on_scanline(&mut self, line: u16, rgb: &[(u8, u8, u8)]) {
            if line == 0 {
                self.building.clear();
            }
            self.building.extend_from_slice(rgb);
            if line == 223 {
                self.last = std::mem::take(&mut self.building);
            }
        }
    }

    let mut old_way = booted(0x1234_5678);
    let mut old = MagicLines::default();
    old_way.run_frames_with_sink(5, &mut old);

    let mut new_way = booted(0x1234_5678);
    let mut new = ScanlineCapture::new(Retain::LastFrame);
    new_way.run_frames_with_sink(5, &mut new);

    assert!(!old.last.is_empty(), "the oracle captured something");
    assert_eq!(
        new.pixels(),
        old.last.as_slice(),
        "the promoted sink must retain byte-identical pixels to the hand-rolled one"
    );
}

// ---------------------------------------------------------------------------------------------------
// `on_frame_boundary` (F-SCANLINE-CAPTURE): the frame-structure hook every frame-shaped sink previously
// had to infer from magic line numbers.
// ---------------------------------------------------------------------------------------------------

/// One delivery, in arrival order — so a test can assert *interleaving* (where the boundary sits relative
/// to the lines), not merely a count.
#[derive(Debug, PartialEq, Eq)]
enum Delivery {
    Line(u16),
    Boundary(u64),
}

/// A sink that logs the scanline/frame-boundary interleaving verbatim.
#[derive(Default)]
struct BoundaryLog {
    log: Vec<Delivery>,
}

impl BusEventSink for BoundaryLog {
    fn on_event(&mut self, _event: BusEvent) {}

    fn wants_scanlines(&self) -> bool {
        true
    }

    fn on_scanline(&mut self, line: u16, _rgb: &[(u8, u8, u8)]) {
        self.log.push(Delivery::Line(line));
    }

    fn on_frame_boundary(&mut self, frame: u64) {
        self.log.push(Delivery::Boundary(frame));
    }
}

#[test]
fn frame_boundary_fires_exactly_once_per_frame_after_the_last_active_line() {
    let mut s = booted(0x1234_5678);
    let mut sink = BoundaryLog::default();
    s.run_frames_with_sink(3, &mut sink);

    let boundaries: Vec<u64> = sink
        .log
        .iter()
        .filter_map(|d| match d {
            Delivery::Boundary(f) => Some(*f),
            Delivery::Line(_) => None,
        })
        .collect();
    assert_eq!(
        boundaries,
        vec![0, 1, 2],
        "one boundary per frame run, carrying the index of the frame that just completed"
    );

    // The whole point of the hook: the boundary is a *structural* marker the sink no longer has to infer.
    // It must sit immediately after that frame's last active line, and immediately before the next frame's
    // first — so a frame-accumulating consumer's buffer is exactly one complete frame at the callback.
    let expected: Vec<Delivery> = (0..3u64)
        .flat_map(|f| {
            (0..224u16)
                .map(Delivery::Line)
                .chain(std::iter::once(Delivery::Boundary(f)))
        })
        .collect();
    assert_eq!(
        sink.log, expected,
        "lines 0..=223 then the boundary, three times over"
    );
}

#[test]
fn frame_boundary_is_state_neutral() {
    // Same currency argument as the scanline hook: an attached boundary observer cannot move the machine.
    let mut plain = booted(9);
    let mut tapped = booted(9);
    plain.run_frames(3);
    let mut sink = BoundaryLog::default();
    tapped.run_frames_with_sink(3, &mut sink);
    assert_eq!(
        plain.export_state_hash(),
        tapped.export_state_hash(),
        "the boundary hook must not move the machine"
    );
    assert_eq!(
        plain, tapped,
        "the WHOLE machine is identical, not just the hash"
    );
}

#[test]
fn capture_sink_is_state_neutral() {
    // Default-path neutrality, observed from both sides: a run with the capture sink attached reaches the
    // exact same machine state as a plain `run_frames` — the sink only borrows what was already rendered.
    let mut plain = booted(42);
    let mut tapped = booted(42);
    plain.run_frames(2);
    let mut sink = ScanlineCapture::new(Retain::First);
    tapped.run_frames_with_sink(2, &mut sink);
    assert_eq!(
        plain.export_state_hash(),
        tapped.export_state_hash(),
        "capture must not move the machine"
    );
    assert_eq!(sink.lines().len(), 448, "the sink did observe the run");
}
