//! [`ScanlineCapture`] — the one per-scanline capture sink (`F-SCANLINE-CAPTURE`).
//!
//! Two near-duplicate sinks previously existed in the test tree, written nine hours apart on the same day
//! against the same seam: `LineCollector` (`tests/scanline_capture.rs`, first-wins) and `FrameCapture`
//! (`tests/conformance_roms.rs`, last-complete-frame). Three of their four method bodies were byte-identical
//! and they differed only in **retention policy**, so retention is what this type takes as configuration —
//! [`Retain`] — and everything else is written once.
//!
//! The `LastFrame` policy is also the reason [`BusEventSink::on_frame_boundary`] exists. `FrameCapture`
//! hand-detected frame structure from two magic line comparisons (`if line == 0 { clear }` /
//! `if line == ACTIVE_LINES - 1 { take }`) with 224 hard-coded on both sides; here the frame boundary is
//! delivered by the run loop, so this sink knows nothing about how tall a frame is.
//!
//! **Currency**: caller-owned, like every sink. `System` never stores it, it never writes to the machine,
//! and it opts in via `wants_scanlines` — a run without it is byte-for-byte the discard-the-render hot path.

use crate::bus::{BusEvent, BusEventSink};

/// What a [`ScanlineCapture`] keeps out of the line stream. The variants are cheap to switch between and
/// differ only in memory: `First` holds one line, `LastFrame` at most two frames (one building, one held),
/// `All` holds the entire run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retain {
    /// Keep the **first** delivered line's pixels and drop the rest — the "did the capture hand out the real
    /// render?" probe.
    First,
    /// Keep the most recently **completed** frame's active lines, line-major, latched at
    /// [`BusEventSink::on_frame_boundary`]. This is the per-scanline analogue of an end-of-frame framebuffer
    /// read, and the one that makes mid-frame CRAM effects visible.
    LastFrame,
    /// Keep **every** delivered line for the whole run, concatenated line-major, and never latch.
    All,
}

/// A [`BusEventSink`] that consumes no bus events and records rendered scanlines under a [`Retain`] policy.
#[derive(Clone, Debug)]
pub struct ScanlineCapture {
    retain: Retain,
    lines: Vec<(u16, usize)>,
    building: Vec<(u8, u8, u8)>,
    pixels: Vec<(u8, u8, u8)>,
    frames: u64,
    last_frame_index: Option<u64>,
}

impl ScanlineCapture {
    /// A capture with the given retention policy.
    pub fn new(retain: Retain) -> Self {
        ScanlineCapture {
            retain,
            lines: Vec::new(),
            building: Vec::new(),
            pixels: Vec::new(),
            frames: 0,
            last_frame_index: None,
        }
    }

    /// The configured retention policy.
    pub fn retain_policy(&self) -> Retain {
        self.retain
    }

    /// Every delivery in arrival order as `(line number, width in pixels)` — logged under all policies, so a
    /// caller can assert line ordering and geometry without also paying to keep the pixels.
    pub fn lines(&self) -> &[(u16, usize)] {
        &self.lines
    }

    /// The retained pixels, line-major (`r`,`g`,`b` per pixel), as the active [`Retain`] policy defines them.
    /// Empty until the policy has something to hand back — notably `LastFrame` is empty until the first
    /// [`BusEventSink::on_frame_boundary`].
    pub fn pixels(&self) -> &[(u8, u8, u8)] {
        &self.pixels
    }

    /// How many frame boundaries the run delivered — counted under all policies.
    pub fn frames_completed(&self) -> u64 {
        self.frames
    }

    /// The index of the frame the last boundary completed, or `None` if no boundary has been seen.
    pub fn last_frame_index(&self) -> Option<u64> {
        self.last_frame_index
    }
}

impl BusEventSink for ScanlineCapture {
    fn on_event(&mut self, _event: BusEvent) {}

    fn wants_scanlines(&self) -> bool {
        true
    }

    fn on_scanline(&mut self, line: u16, rgb: &[(u8, u8, u8)]) {
        self.lines.push((line, rgb.len()));
        match self.retain {
            Retain::First => {
                if self.pixels.is_empty() {
                    self.pixels.extend_from_slice(rgb);
                }
            }
            Retain::LastFrame => self.building.extend_from_slice(rgb),
            Retain::All => self.pixels.extend_from_slice(rgb),
        }
    }

    fn on_frame_boundary(&mut self, frame: u64) {
        self.frames += 1;
        self.last_frame_index = Some(frame);
        if self.retain == Retain::LastFrame {
            // Active display just ended, so `building` is exactly one complete frame. No line arithmetic: the
            // run loop knows the frame geometry, this sink does not.
            self.pixels = std::mem::take(&mut self.building);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the sink's hooks directly, with no machine — the retention policies are pure functions of the
    /// delivery sequence, so they are testable without booting anything.
    fn feed(sink: &mut ScanlineCapture, frames: u64, lines_per_frame: u16, width: usize) {
        for f in 0..frames {
            for line in 0..lines_per_frame {
                // A per-(frame,line) pixel value, so a policy that keeps the wrong lines is caught.
                let px = vec![(f as u8, line as u8, 0u8); width];
                sink.on_scanline(line, &px);
            }
            sink.on_frame_boundary(f);
        }
    }

    #[test]
    fn first_keeps_only_the_first_line() {
        let mut s = ScanlineCapture::new(Retain::First);
        feed(&mut s, 3, 4, 2);
        assert_eq!(s.pixels(), [(0, 0, 0), (0, 0, 0)]);
        assert_eq!(s.lines().len(), 12, "every delivery is still logged");
    }

    #[test]
    fn last_frame_keeps_the_most_recently_completed_frame() {
        let mut s = ScanlineCapture::new(Retain::LastFrame);
        feed(&mut s, 3, 2, 1);
        assert_eq!(
            s.pixels(),
            [(2, 0, 0), (2, 1, 0)],
            "frame 2's two lines, and nothing from frames 0/1"
        );
        assert_eq!(s.frames_completed(), 3);
        assert_eq!(s.last_frame_index(), Some(2));
    }

    #[test]
    fn last_frame_is_empty_until_the_first_boundary() {
        let mut s = ScanlineCapture::new(Retain::LastFrame);
        s.on_scanline(0, &[(1, 2, 3)]);
        assert!(
            s.pixels().is_empty(),
            "a partial frame is not a frame — nothing is handed back before the boundary"
        );
        s.on_frame_boundary(0);
        assert_eq!(s.pixels(), [(1, 2, 3)]);
    }

    #[test]
    fn all_keeps_everything_and_the_boundary_does_not_truncate_it() {
        let mut s = ScanlineCapture::new(Retain::All);
        feed(&mut s, 2, 2, 1);
        assert_eq!(s.pixels(), [(0, 0, 0), (0, 1, 0), (1, 0, 0), (1, 1, 0)]);
        assert_eq!(s.frames_completed(), 2);
    }

    #[test]
    fn every_policy_opts_into_scanlines_and_ignores_bus_events() {
        for r in [Retain::First, Retain::LastFrame, Retain::All] {
            let mut s = ScanlineCapture::new(r);
            assert!(s.wants_scanlines());
            assert_eq!(s.retain_policy(), r);
            assert!(
                !s.wants_vdp_writes(),
                "the capture arms no VDP write capture"
            );
            assert!(!s.stop_requested(), "the capture never ends a run");
            s.on_event(BusEvent {
                op: crate::bus::BusOp::Read,
                fc: 5,
                addr: 0,
                size: crate::bus::Size::Word,
                value: 0,
            });
            assert!(s.lines().is_empty(), "bus events are not lines");
        }
    }
}
