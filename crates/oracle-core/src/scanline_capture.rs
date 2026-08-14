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

/// What a [`ScanlineCapture`] keeps out of the line stream. The variants differ only in how many *pixels*
/// they hold: `First` one line, `LastFrame` at most two frames (one building, one held), `All` the entire
/// run. They do **not** differ in the per-delivery `lines` bookkeeping, which every policy pays — see
/// [`ScanlineCapture`]'s memory note for the actual numbers, which are not small.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retain {
    /// Keep the **first** delivered line's pixels and drop the rest — the "did the capture hand out the real
    /// render?" probe.
    First,
    /// Keep the most recently **completed** frame's active lines, line-major, latched at
    /// [`BusEventSink::on_frame_boundary`]. This is the per-scanline analogue of an end-of-frame framebuffer
    /// read, and the one that makes mid-frame CRAM effects visible.
    ///
    /// Self-healing: if a run ends mid-frame and the caller then resets, loads a savestate, or attaches the
    /// capture to a different `System`, the orphaned partial frame is discarded when the line stream
    /// restarts — the latched frame is never longer than one frame.
    LastFrame,
    /// Keep **every** delivered line for the whole run, concatenated line-major, and never latch.
    All,
}

/// A [`BusEventSink`] that consumes no bus events and records rendered scanlines under a [`Retain`] policy.
///
/// # Memory — this grows without bound, under every policy
///
/// Nothing here is capped or ring-buffered; the type is sized for the runs it was built for (the conformance
/// harness's tens-to-hundreds of frames) and a caller who attaches one to an open-ended run must call
/// [`clear`](ScanlineCapture::clear) periodically. At NTSC rates (224 active lines x 59.92 frames/s = ~13.4k
/// deliveries/s):
///
/// | what grows | per second | per emulated hour |
/// |---|---|---|
/// | `lines` bookkeeping — **all three policies**, 16 B/entry | ~215 KB | ~774 MB |
/// | pixels, [`Retain::All`], H40 (320 px x 3 B/px) | ~12.9 MB | ~46 GB |
/// | pixels, [`Retain::All`], H32 (256 px x 3 B/px) | ~10.3 MB | ~37 GB |
/// | pixels, [`Retain::First`] / [`Retain::LastFrame`] | bounded (1 line / 2 frames) | bounded |
///
/// So [`Retain::First`] is *not* a free observer: it is ~774 MB/hour of `(line, width)` pairs. The log is
/// kept under every policy on purpose — it is what lets a caller assert line ordering and geometry without
/// paying for pixels — but the honest description is "bounded by run length", not "cheap".
#[derive(Clone, Debug)]
pub struct ScanlineCapture {
    retain: Retain,
    lines: Vec<(u16, usize)>,
    building: Vec<(u8, u8, u8)>,
    pixels: Vec<(u8, u8, u8)>,
    frames: u64,
    last_frame_index: Option<u64>,
    last_line: Option<u16>,
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
            last_line: None,
        }
    }

    /// The configured retention policy.
    pub fn retain_policy(&self) -> Retain {
        self.retain
    }

    /// Drop everything recorded so far and return to the just-constructed state, keeping only the retention
    /// policy. Two uses: reusing one capture across unrelated runs (a reset, a savestate load, a different
    /// `System`) without carrying the previous run's partial frame or line log into it, and releasing the
    /// unbounded `lines` bookkeeping in a long-lived capture — see the type's memory note.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.building.clear();
        self.pixels.clear();
        self.frames = 0;
        self.last_frame_index = None;
        self.last_line = None;
    }

    /// Every delivery in arrival order as `(line number, width in pixels)` — logged under all policies, so a
    /// caller can assert line ordering and geometry without also paying to keep the pixels. Grows for the
    /// whole life of the capture (~215 KB/emulated second); see the type's memory note and
    /// [`clear`](ScanlineCapture::clear).
    pub fn lines(&self) -> &[(u16, usize)] {
        &self.lines
    }

    /// The retained pixels, line-major (`r`,`g`,`b` per pixel), as the active [`Retain`] policy defines them.
    /// Empty until the policy has something to hand back — notably `LastFrame` is empty until the first
    /// [`BusEventSink::on_frame_boundary`].
    ///
    /// Under `LastFrame`, this is the last **completed** frame, which is not necessarily the last frame the
    /// run drew: a run can end after all 224 of a frame's lines but before its boundary (see
    /// [`BusEventSink::on_frame_boundary`]'s sharp-edge note), in which case the frame just drawn is still in
    /// the internal buffer and the *previous* one is returned here. `run_frames(n >= 1)` always ends on a
    /// boundary, so the harness path never sees this.
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
        // **Resync.** A boundary is what normally empties `building`, but a run can end mid-frame
        // (`run_until`) and the caller can then reset the machine, load a savestate, or point the same
        // capture at a different `System` — in which case the next line stream restarts with a torn partial
        // frame still buffered, and without this the "frame" handed back at the next boundary is longer than
        // a frame. The deleted `FrameCapture` self-healed via `if line == 0 { clear }`; the generalisation is
        // that a line number which does not ADVANCE means a new frame has begun (line numbers are strictly
        // ascending within a frame). No-op on the normal path, where the boundary already emptied `building`.
        let restarted = self.last_line.is_some_and(|prev| line <= prev);
        self.last_line = Some(line);
        if restarted {
            self.building.clear();
        }
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

    /// The type's memory note quotes 16 bytes per `lines` entry (hence ~215 KB per emulated second under
    /// every policy). Pin it so the documented cost cannot rot silently.
    #[test]
    fn the_documented_per_delivery_bookkeeping_cost_is_16_bytes() {
        assert_eq!(std::mem::size_of::<(u16, usize)>(), 16);
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

    /// A run can end mid-frame (`run_until`), and the machine can be reset or savestate-loaded under a
    /// capture that is reused across runs. The half-frame buffered by the first run must NOT be prepended to
    /// the next completed frame — the deleted `FrameCapture` self-healed here via `if line == 0 { clear }`
    /// and this type must not regress it.
    #[test]
    fn last_frame_resyncs_when_the_line_stream_restarts_at_zero_without_a_boundary() {
        let mut s = ScanlineCapture::new(Retain::LastFrame);
        for line in 0..3u16 {
            s.on_scanline(line, &[(9, 9, 9)]); // a torn partial frame, never completed
        }
        for line in 0..2u16 {
            s.on_scanline(line, &[(1, line as u8, 0)]); // the stream restarts: a new frame
        }
        s.on_frame_boundary(7);
        assert_eq!(
            s.pixels(),
            [(1, 0, 0), (1, 1, 0)],
            "the torn partial frame must not be prepended to the frame that did complete"
        );
    }

    /// The resync is not "line == 0" specifically: any line that does not advance means a new frame has
    /// begun (a run resumed mid-frame, a reset landing on a different line, a savestate load).
    #[test]
    fn last_frame_resyncs_when_the_line_stream_goes_backwards_mid_frame() {
        let mut s = ScanlineCapture::new(Retain::LastFrame);
        for line in 0..100u16 {
            s.on_scanline(line, &[(9, 9, 9)]);
        }
        for line in 50..52u16 {
            s.on_scanline(line, &[(2, line as u8, 0)]);
        }
        s.on_frame_boundary(3);
        assert_eq!(
            s.pixels(),
            [(2, 50, 0), (2, 51, 0)],
            "a non-advancing line number restarts the frame under construction"
        );
    }

    /// `clear` puts the capture back to `new`, so one instance can be reused across runs without the caller
    /// having to reason about what the previous run left buffered — and so the unbounded `lines` log has an
    /// explicit release point.
    #[test]
    fn clear_returns_the_capture_to_its_initial_state() {
        for r in [Retain::First, Retain::LastFrame, Retain::All] {
            let mut s = ScanlineCapture::new(r);
            feed(&mut s, 2, 3, 1);
            s.on_scanline(0, &[(7, 7, 7)]); // plus a torn partial frame
            s.clear();
            assert!(s.pixels().is_empty(), "{r:?}: pixels released");
            assert!(s.lines().is_empty(), "{r:?}: the line log is released");
            assert_eq!(s.frames_completed(), 0, "{r:?}: frame count reset");
            assert_eq!(s.last_frame_index(), None, "{r:?}: frame index reset");
            assert_eq!(s.retain_policy(), r, "{r:?}: the policy survives");
            // and the buffered partial frame is gone, not merely hidden
            feed(&mut s, 1, 2, 1);
            let expect: &[(u8, u8, u8)] = match r {
                Retain::First => &[(0, 0, 0)],
                Retain::LastFrame | Retain::All => &[(0, 0, 0), (0, 1, 0)],
            };
            assert_eq!(s.pixels(), expect, "{r:?}: a clean two-line frame");
        }
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
