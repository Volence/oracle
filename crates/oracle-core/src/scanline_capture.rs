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

    /// **The completed-frame reader** — the most recently finished `height`-line frame, or `None` when
    /// this capture is not holding one right now (nothing completed yet, or a run that ended mid-frame).
    ///
    /// # Why this lives here rather than in each window
    ///
    /// It existed **four times** — `oracle-frontend`'s `blit_capture`, `oracle-player`'s
    /// `capture_to_image`, `oracle-aether`'s `store_from_capture`, and the panels spike — with identical
    /// selection logic and four different pixel types on the far end. Only one of the four was tested,
    /// and that one was the copy scheduled for deletion; two of the untested three back
    /// `emulator/screenshot` and the player's own window. Two of their docs stated the coupling in prose
    /// and one of those noted that **getting it wrong is silent**: the visible symptom is a frame sheared
    /// mid-screen or skewed by 64 px per line, and neither throws.
    ///
    /// So the *selection* is here, once, under a test, and each consumer keeps only its own packing loop
    /// (`u32` ARGB, `egui::Color32`, `(u8,u8,u8)`), which is the part that genuinely differs. `height`
    /// stays a parameter rather than a constant: this type deliberately knows nothing about how tall a
    /// frame is — see [`BusEventSink::on_frame_boundary`] and the module doc.
    ///
    /// # The two non-obvious rules, and they are load-bearing
    ///
    /// * **The completed frame is the last `height` deliveries, and the sum check is what proves it.** A
    ///   run that ended mid-frame leaves a *previous* frame in [`pixels`](Self::pixels) whose lines are
    ///   no longer the tail of the delivery log; without the check a torn run hands back a frame stitched
    ///   from two different geometries.
    /// * **A frame is not guaranteed rectangular.** A game can switch H32↔H40 part-way down, and S3K does
    ///   exactly that on the first frame after a soft reset (two 256-px lines, then 222 at 320). The
    ///   width is the width the frame **ended** on — what the VDP is actually scanning out by V-Blank —
    ///   and shorter lines are padded with black to reach it by
    ///   [`CompletedFrame::pixels`]. Rejecting such frames instead would blank the window for as long as
    ///   a game kept switching.
    ///
    /// Nothing is copied here: the return borrows the capture, so a caller who only wants the width pays
    /// for no pixels at all.
    pub fn completed_frame(&self, height: usize) -> Option<CompletedFrame<'_>> {
        let px = self.pixels();
        let log = self.lines();
        // `height == 0` is not reachable from any caller today (all four pass a 224 constant), and it is
        // refused rather than trusted because this is a public parameter: with an empty `widths` the
        // `widths[height - 1]` below would panic inside a read that has a `None` for every other way of
        // not having a frame.
        if height == 0 || px.is_empty() || log.len() < height {
            return None;
        }
        let widths = &log[log.len() - height..];
        if widths.iter().map(|&(_, w)| w).sum::<usize>() != px.len() {
            return None;
        }
        let width = widths[height - 1].1;
        if width == 0 {
            return None;
        }
        Some(CompletedFrame {
            width,
            height,
            px,
            widths,
        })
    }
}

/// One completed frame, borrowed out of a [`ScanlineCapture`] by
/// [`completed_frame`](ScanlineCapture::completed_frame).
///
/// Deliberately not a pixel buffer: the four consumers want four different pixel types, and the thing
/// they must agree on is *which* pixels and *what shape*, not what a pixel is spelled as.
#[derive(Clone, Copy, Debug)]
pub struct CompletedFrame<'a> {
    width: usize,
    height: usize,
    px: &'a [(u8, u8, u8)],
    widths: &'a [(u16, usize)],
}

impl<'a> CompletedFrame<'a> {
    /// The display width: the width the frame **ended** on, never a re-query of the VDP — a post-hoc
    /// query answers for whatever mode the chip is in *now*, which after an H32↔H40 switch is the next
    /// frame's. Always non-zero.
    pub fn width(&self) -> usize {
        self.width
    }

    /// The height this frame was read at — the `height` that was asked for, restated so a caller sizing
    /// an image does not have to carry the constant twice.
    pub fn height(&self) -> usize {
        self.height
    }

    /// The frame's lines exactly as they were delivered, **ragged**: a line may be shorter or longer
    /// than [`width`](Self::width). For drawing, prefer [`pixels`](Self::pixels), which applies the
    /// padding rule; this is for a caller that needs to see the geometry itself.
    pub fn rows(&self) -> impl Iterator<Item = &'a [(u8, u8, u8)]> + '_ {
        let px = self.px;
        let mut at = 0usize;
        self.widths.iter().map(move |&(_, w)| {
            let line = &px[at..at + w];
            at += w;
            line
        })
    }

    /// **Every pixel of the `width` × `height` rectangle, line-major**, with the padding rule applied:
    /// a line shorter than [`width`](Self::width) is filled out with black, and a longer one is cut.
    /// Exactly `width * height` items, always.
    pub fn pixels(&self) -> impl Iterator<Item = (u8, u8, u8)> + '_ {
        let width = self.width;
        self.rows().flat_map(move |line| {
            (0..width).map(move |x| line.get(x).copied().unwrap_or((0, 0, 0)))
        })
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

    // -----------------------------------------------------------------------------------------------
    // `completed_frame` — the reader that used to exist four times (H25)
    //
    // The four copies agreed exactly and **nothing asserted that they must**; the only tested one was
    // `oracle-frontend`'s `blit_capture`, which is the copy scheduled for deletion. These rows are that
    // copy's four edge assertions, moved to the one implementation they now all run, plus the two rules
    // its prose stated and no test held: the ragged-frame padding, and the width the frame ended on.
    // -----------------------------------------------------------------------------------------------

    /// Feed `lines` deliveries of the given per-line widths, then a boundary.
    fn feed_ragged(sink: &mut ScanlineCapture, frame: u8, widths: &[usize]) {
        for (line, &w) in widths.iter().enumerate() {
            let px: Vec<(u8, u8, u8)> = (0..w).map(|x| (frame, line as u8, x as u8)).collect();
            sink.on_scanline(line as u16, &px);
        }
    }

    /// **The four `None`s**, each reached by a different route and each asserted separately, because
    /// "returns `None`" is satisfied by an implementation that never returns anything else.
    #[test]
    fn completed_frame_is_none_until_a_whole_frame_is_actually_held() {
        const H: usize = 4;

        // 1. Nothing delivered at all.
        let empty = ScanlineCapture::new(Retain::LastFrame);
        assert!(empty.completed_frame(H).is_none(), "no deliveries");

        // 2. Fewer than `height` deliveries in the log.
        let mut partial = ScanlineCapture::new(Retain::LastFrame);
        feed_ragged(&mut partial, 0, &[2, 2, 2]);
        partial.on_frame_boundary(0);
        assert!(
            partial.completed_frame(H).is_none(),
            "3 lines cannot be a 4-line frame"
        );

        // 3. ⚑ THE SUM CHECK. A run that ends mid-frame leaves the PREVIOUS frame in `pixels()` while
        //    the tail of the log describes the torn one. Without the check the reader stitches a frame
        //    out of two different geometries and nothing throws.
        //
        //    ⚑ The torn lines are deliberately WIDER. This check compares a total, so it can only see a
        //    tear that changed the geometry — a torn run at the same width leaves the tail summing to
        //    exactly what is held, and the reader hands back the previous completed frame. That is not a
        //    hole: re-presenting the last good picture is what every caller does with a `None` anyway.
        //    The tear that matters is the one that would be *stitched*, and that is this one.
        let mut stale = ScanlineCapture::new(Retain::LastFrame);
        feed_ragged(&mut stale, 0, &[2; H]);
        stale.on_frame_boundary(0);
        assert!(
            stale.completed_frame(H).is_some(),
            "the control: frame 0 completed and is readable"
        );
        feed_ragged(&mut stale, 1, &[5, 5]); // torn: two lines of a wider frame, no boundary
        assert!(
            stale.completed_frame(H).is_none(),
            "the log's tail is two wide torn lines plus two of frame 0, and the held pixels are frame \
             0's eight; stitching those is a picture sheared mid-screen"
        );

        // 4. A zero-width final line: a frame with no display width is not a picture.
        let mut zero = ScanlineCapture::new(Retain::LastFrame);
        feed_ragged(&mut zero, 0, &[2, 2, 2, 0]);
        zero.on_frame_boundary(0);
        assert!(
            zero.completed_frame(H).is_none(),
            "the frame ended on a zero-width line"
        );

        // 5. And the public parameter's own edge: `height == 0` refuses rather than indexing `[-1]`.
        let mut fine = ScanlineCapture::new(Retain::LastFrame);
        feed_ragged(&mut fine, 0, &[2; H]);
        fine.on_frame_boundary(0);
        assert!(fine.completed_frame(H).is_some(), "the control");
        assert!(
            fine.completed_frame(0).is_none(),
            "a zero-line frame is refused, not panicked over"
        );
    }

    /// **The ragged-frame rule, which every copy stated in prose and none of them asserted.**
    ///
    /// A game can switch H32↔H40 part-way down and S3K does on the first frame after a soft reset. The
    /// width is the one the frame **ended** on, short lines are padded with black, and a long line is
    /// cut. Getting this wrong is the silent 64-px-per-line skew, so the pixels are checked by position
    /// and not merely counted.
    #[test]
    fn a_ragged_frame_takes_the_width_it_ended_on_and_pads_the_short_lines() {
        const H: usize = 3;
        let mut s = ScanlineCapture::new(Retain::LastFrame);
        // Two narrow lines, then a wide one — S3K's shape, shrunk.
        feed_ragged(&mut s, 7, &[2, 5, 4]);
        s.on_frame_boundary(0);

        let f = s.completed_frame(H).expect("a whole frame was delivered");
        assert_eq!(
            f.width(),
            4,
            "the width the frame ENDED on, not the first or the widest"
        );
        assert_eq!(f.height(), H);

        // `rows` is the delivered geometry, ragged and unpadded.
        assert_eq!(
            f.rows().map(<[_]>::len).collect::<Vec<_>>(),
            vec![2, 5, 4],
            "rows() must not pad — that is pixels()' job"
        );

        // `pixels` is the rectangle. Exactly width*height, black where a line ran short, cut where it
        // ran long.
        let got: Vec<(u8, u8, u8)> = f.pixels().collect();
        assert_eq!(got.len(), 4 * H, "exactly width * height pixels");
        assert_eq!(
            got,
            vec![
                // line 0 delivered 2 of 4: two real, two black.
                (7, 0, 0),
                (7, 0, 1),
                (0, 0, 0),
                (0, 0, 0),
                // line 1 delivered 5 of 4: the fifth is dropped, never wrapped onto the next line.
                (7, 1, 0),
                (7, 1, 1),
                (7, 1, 2),
                (7, 1, 3),
                // line 2 is exact.
                (7, 2, 0),
                (7, 2, 1),
                (7, 2, 2),
                (7, 2, 3),
            ],
            "a wrong pad or a wrong cut is a sheared picture and nothing throws"
        );
    }

    /// The frame handed back is the one the **last** boundary completed, not an earlier one — the other
    /// half of what the sum check buys, and the assertion a reader that took the FIRST `height` lines
    /// would fail while passing every row above.
    #[test]
    fn completed_frame_reads_the_most_recent_frame_not_the_first() {
        const H: usize = 2;
        let mut s = ScanlineCapture::new(Retain::LastFrame);
        for frame in 0..3u8 {
            feed_ragged(&mut s, frame, &[3; H]);
            s.on_frame_boundary(u64::from(frame));
        }
        let f = s.completed_frame(H).expect("three frames completed");
        let got: Vec<(u8, u8, u8)> = f.pixels().collect();
        assert!(
            got.iter().all(|&(frame, _, _)| frame == 2),
            "every pixel must come from frame 2: {got:?}"
        );
    }
}
