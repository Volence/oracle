//! Opt-in per-scanline capture (conformance Limitation L1 follow-up): a [`BusEventSink`] that opts in via
//! `wants_scanlines` receives every rendered active line (0..=223) **during** `run_frames`, as a borrowed
//! RGB slice — the line the `Scanline` event already renders and previously discarded. The default path (no
//! sink, or a sink that does not opt in) is byte-identical to before: same `render_scanline` call, no extra
//! allocation, no state change (the sink is the caller's; `System` never stores it).

use oracle_core::bus::{BusEvent, BusEventSink};
use oracle_core::system::System;

/// Collecting sink: records each delivered line's number and width, and keeps the first delivered line's
/// pixels so a test can compare the capture's content against an independent render.
struct LineCollector {
    lines: Vec<(u16, usize)>,
    first_line_rgb: Option<Vec<(u8, u8, u8)>>,
}

impl LineCollector {
    fn new() -> Self {
        LineCollector {
            lines: Vec::new(),
            first_line_rgb: None,
        }
    }
}

impl BusEventSink for LineCollector {
    fn on_event(&mut self, _event: BusEvent) {}

    fn wants_scanlines(&self) -> bool {
        true
    }

    fn on_scanline(&mut self, line: u16, rgb: &[(u8, u8, u8)]) {
        if self.first_line_rgb.is_none() {
            self.first_line_rgb = Some(rgb.to_vec());
        }
        self.lines.push((line, rgb.len()));
    }
}

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
    let mut sink = LineCollector::new();
    s.run_frames_with_sink(1, &mut sink);
    assert_eq!(
        sink.lines.len(),
        224,
        "exactly one delivery per active line per frame"
    );
    for (i, &(line, width)) in sink.lines.iter().enumerate() {
        assert_eq!(line as usize, i, "lines arrive in order 0..=223");
        assert_eq!(width, 256, "the built-in test ROM is H32 (reg 12 = 0x00)");
    }
    // Content: line 0's Scanline event fires at mclk 0, before the first CPU step, so its delivered pixels
    // must equal a freshly-booted twin's own render of line 0 — the capture hands out the real render, not
    // just correctly-sized buffers.
    let first = sink.first_line_rgb.as_ref().expect("line 0 was delivered");
    let twin = booted(0x1234_5678);
    assert_eq!(
        *first,
        twin.vdp().render_line(0),
        "captured line 0 is the render of the boot-time VDP state"
    );
}

#[test]
fn second_frame_delivers_the_same_line_sequence_again() {
    let mut s = booted(0x1234_5678);
    let mut sink = LineCollector::new();
    s.run_frames_with_sink(2, &mut sink);
    assert_eq!(sink.lines.len(), 448, "224 active lines per frame, twice");
    for (i, &(line, _)) in sink.lines.iter().enumerate() {
        assert_eq!(line as usize, i % 224, "each frame restarts at line 0");
    }
}

#[test]
fn capture_sink_is_state_neutral() {
    // Default-path neutrality, observed from both sides: a run with the capture sink attached reaches the
    // exact same machine state as a plain `run_frames` — the sink only borrows what was already rendered.
    let mut plain = booted(42);
    let mut tapped = booted(42);
    plain.run_frames(2);
    let mut sink = LineCollector::new();
    tapped.run_frames_with_sink(2, &mut sink);
    assert_eq!(
        plain.export_state_hash(),
        tapped.export_state_hash(),
        "capture must not move the machine"
    );
    assert_eq!(sink.lines.len(), 448, "the sink did observe the run");
}
