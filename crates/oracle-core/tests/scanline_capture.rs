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

const VENDOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/TestRoms");

/// The vendored ROM this file's retention oracle runs on, and the budget `conformance_roms.rs` already runs
/// it under — so the frame compared below is the very frame the pinned `frame_hash` currency is computed
/// over. `color_1536` rewrites CRAM *mid-scanline*, so its captured picture keeps changing frame to frame
/// (measured: frame 0 is all black; frames 0/59/119/120 are pairwise different).
///
/// The built-in `testrom::build()` picture cannot serve here: it renders a **constant all-black frame**
/// (measured: frame 0 and frame 4 byte-identical, every pixel `(0,0,0)`), so a capture that retained the
/// FIRST frame instead of the last would compare equal and the oracle would pass vacuously.
const EVOLVING_ROM: &str = "color_1536";
const EVOLVING_FRAMES: u64 = 120;

/// Boot a vendored conformance ROM, or `None` with a `SKIP:` note if it has not been fetched — the
/// `conformance_roms.rs` idiom. Under CI a missing ROM is a hard failure instead of a silent skip: a skip
/// here re-creates exactly the vacuity this ROM was brought in to remove.
fn boot_vendor(name: &str) -> Option<System> {
    let path = format!("{VENDOR_DIR}/{name}.bin");
    let Ok(rom) = std::fs::read(&path) else {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI: vendored test ROM {path} is missing — tools/fetch-testroms.sh must run before the test \
             job. Skipping it would make the retention oracle compare a constant picture to itself."
        );
        eprintln!("SKIP: {path} not found — run tools/fetch-testroms.sh");
        return None;
    };
    let mut sys = System::new(0x1234_5678);
    sys.load_rom(rom);
    sys.reset();
    Some(sys)
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
    // Deliberately NOT asserted here: that the frame in hand differs from a one-frame capture. The built-in
    // test ROM renders a constant all-black picture (measured: frame 0 and frame 4 byte-identical, every
    // pixel `(0,0,0)`), so LAST-vs-FIRST retention is indistinguishable on this ROM and any such assertion
    // would be vacuous — which is what the previous version of this comment claimed to guard against while
    // never writing the comparison. The last-vs-first distinction is pinned on a ROM whose picture actually
    // evolves, in `last_frame_retention_matches_the_hand_rolled_magic_line_sink`. What this test pins is the
    // geometry and the boundary bookkeeping.
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
///
/// Run on [`EVOLVING_ROM`], not the built-in test ROM: an oracle is only worth its runtime if the two sides
/// can actually disagree, and on a constant all-black picture they cannot. The oracle keeps the FIRST
/// completed frame as well as the last, and asserts the two differ, so the comparison below is proven
/// non-vacuous *by the oracle itself* — the guard uses only magic line numbers and never
/// `on_frame_boundary`, so it cannot be satisfied by the hook under test.
#[test]
fn last_frame_retention_matches_the_hand_rolled_magic_line_sink() {
    /// The pre-`F-SCANLINE-CAPTURE` `FrameCapture`, verbatim, plus a latch on the first completed frame.
    #[derive(Default)]
    struct MagicLines {
        building: Vec<(u8, u8, u8)>,
        first: Vec<(u8, u8, u8)>,
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
                if self.first.is_empty() {
                    self.first.clone_from(&self.last);
                }
            }
        }
    }

    let Some(mut old_way) = boot_vendor(EVOLVING_ROM) else {
        return;
    };
    let mut old = MagicLines::default();
    old_way.run_frames_with_sink(EVOLVING_FRAMES, &mut old);

    let mut new_way =
        boot_vendor(EVOLVING_ROM).expect("the oracle run above already read this ROM");
    let mut new = ScanlineCapture::new(Retain::LastFrame);
    new_way.run_frames_with_sink(EVOLVING_FRAMES, &mut new);

    assert!(!old.last.is_empty(), "the oracle captured something");
    assert_ne!(
        old.first, old.last,
        "NON-VACUITY GUARD: {EVOLVING_ROM}'s first and last completed frames must differ, or the \
         comparison below cannot tell last-frame retention from first-frame retention"
    );
    assert_eq!(
        new.pixels(),
        old.last.as_slice(),
        "the promoted sink must retain byte-identical pixels to the hand-rolled one"
    );
    assert_ne!(
        new.pixels(),
        old.first.as_slice(),
        "the retained frame is the LAST completed one, not the first"
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
    // A neutrality control is only a control if the thing whose neutrality it is asserting actually
    // happened. Without this, deleting the `on_frame_boundary` call from `System::deliver_event` outright
    // leaves this test green — it would then be comparing two runs of the same never-firing hook.
    assert_eq!(
        sink.log
            .iter()
            .filter(|d| matches!(d, Delivery::Boundary(_)))
            .count(),
        3,
        "the sink did observe three boundaries — a hook that never fires cannot pass this test"
    );
    assert_eq!(
        sink.log.len(),
        3 * (224 + 1),
        "224 lines plus a boundary, three times over"
    );
}

/// The `LastFrame` resync (`I1`). A boundary is what normally empties the frame under construction, so a run
/// that ends mid-frame leaves a torn partial frame buffered; if the caller then resets the machine (or loads
/// a savestate, or points the same capture at a different `System`) the next boundary must hand back one
/// frame, not one frame plus the remnant. `ScanlineCapture` is public core API, so this is a contract, not a
/// test-local convenience — the deleted `FrameCapture` self-healed here via `if line == 0 { clear }`.
#[test]
fn last_frame_resyncs_after_a_reset_that_interrupts_a_frame() {
    const PARTIAL_LINES: u64 = 100;
    let mut s = booted(0x1234_5678);
    let mut sink = ScanlineCapture::new(Retain::LastFrame);
    s.run_until_with_sink(PARTIAL_LINES * oracle_core::vdp::MCLK_PER_LINE, &mut sink);
    assert_eq!(
        sink.lines().len(),
        PARTIAL_LINES as usize - 1,
        "the first run ended mid-frame, with a torn partial frame buffered. `- 1`: the run saw \
         {PARTIAL_LINES} line events, but under deferred emission (`F-SCANLINE-SUBLINE` slice 3) each row \
         is handed to the sink at the NEXT line's event, so the last line resolved is still retained in the \
         machine. Same audited consequence as \
         `a_run_ending_between_the_last_active_line_and_the_boundary_defers_it_to_the_next_run` \
         (`docs/2026-08-19-subline-recon.md` §D); the resync contract this test is about is untouched"
    );
    assert!(
        sink.pixels().is_empty(),
        "no boundary was reached, so nothing is handed back yet"
    );

    s.reset();
    s.run_frames_with_sink(1, &mut sink);
    assert_eq!(
        sink.pixels().len(),
        256 * 224,
        "exactly one complete frame — NOT one frame plus the 99 orphaned lines"
    );
    assert_eq!(sink.frames_completed(), 1);

    // The explicit release valve does the same thing, deliberately, and also drops the unbounded line log.
    sink.clear();
    assert!(sink.pixels().is_empty() && sink.lines().is_empty());
    s.reset();
    s.run_frames_with_sink(1, &mut sink);
    assert_eq!(sink.pixels().len(), 256 * 224);
    assert_eq!(sink.lines().len(), 224, "the line log restarted from empty");
}

/// Documented sharp edge 1: "exactly once per frame" is a **lifetime** invariant, not a per-run one. A run
/// that ends inside the ~3420-mclk window between line 223's render and the line-224 event delivers ZERO
/// boundaries; the boundary is deferred into the next run. A caller that reads `pixels()` right after such a
/// run gets the PREVIOUS frame, with no signal that it did.
#[test]
fn a_run_ending_between_the_last_active_line_and_the_boundary_defers_it_to_the_next_run() {
    let mut s = booted(0x1234_5678);
    let mut sink = BoundaryLog::default();
    s.run_until_with_sink(224 * oracle_core::vdp::MCLK_PER_LINE - 1, &mut sink);
    assert_eq!(
        sink.log
            .iter()
            .filter(|d| matches!(d, Delivery::Line(_)))
            .count(),
        223,
        "223, not 224: under deferred emission (`F-SCANLINE-SUBLINE` slice 3) a row is handed to the sink at \
         the NEXT line's Scanline event, so line 223's row is still retained when a run stops one mclk short \
         of the line-224 event. The deferral this test is about is therefore one row deeper than it was — \
         the thesis (a whole frame drawn, no boundary, completed by the next run) is unchanged. Audited \
         consequence, `docs/2026-08-19-subline-recon.md` §D"
    );
    assert!(
        !sink.log.iter().any(|d| matches!(d, Delivery::Boundary(_))),
        "and NOT the boundary — a whole frame of lines with no frame boundary is reachable"
    );

    s.run_until_with_sink(225 * oracle_core::vdp::MCLK_PER_LINE, &mut sink);
    assert_eq!(
        sink.log
            .iter()
            .filter_map(|d| match d {
                Delivery::Boundary(f) => Some(*f),
                Delivery::Line(_) => None,
            })
            .collect::<Vec<_>>(),
        vec![0],
        "the deferred boundary arrives in the NEXT run, still carrying frame 0"
    );
}

/// The deferred-emission rule itself (`F-SCANLINE-SUBLINE` slice 3), stated once rather than inferred from
/// the two run-length tests that happen to observe it: **row N is handed to the sink at line N+1's
/// `Scanline` event**, never at its own. So a run stopped just short of line K's event has delivered rows
/// `0..=K-2`, i.e. `K - 1` of them.
///
/// The row *index* is unchanged — `on_scanline(N, …)` still means line N — and so is the delivery order;
/// only the instant moves, which is what buys the emitter a whole line's worth of CRAM writes to place
/// inside the row (slice 4). A line-atomic emitter delivers `K` rows here and fails every row of the table.
///
/// The exact counts assume no single instruction of `testrom::build()` spans a whole scanline (3420 mclk);
/// one that did would let several `Scanline` events drain in one burst and shift them. True today by a wide
/// margin — recorded so a future test-ROM change reads as a fixture change, not a regression.
#[test]
fn a_row_is_emitted_at_the_next_lines_event_not_at_its_own() {
    for stop_line in [1u64, 2, 57, 100, 224] {
        let mut s = booted(0x1234_5678);
        let mut sink = BoundaryLog::default();
        // One mclk short of line `stop_line`'s event, so exactly the events for lines `0..stop_line` fired.
        s.run_until_with_sink(stop_line * oracle_core::vdp::MCLK_PER_LINE - 1, &mut sink);
        let lines: Vec<u16> = sink
            .log
            .iter()
            .filter_map(|d| match d {
                Delivery::Line(l) => Some(*l),
                Delivery::Boundary(_) => None,
            })
            .collect();
        let last_resolved = stop_line - 1; // the highest line whose event fired
        assert_eq!(
            lines,
            (0..last_resolved as u16).collect::<Vec<_>>(),
            "{stop_line} line events fired (lines 0..={last_resolved}), so every row EXCEPT {last_resolved} \
             has been emitted — row {last_resolved} is still retained, waiting for the next line's event"
        );
    }
}

/// **The whole slice-4 plumbing, end to end, without a server** (`F-SCANLINE-SUBLINE`): boot the mid-frame
/// CRAM fixture through the ordinary run loop and check the row it writes during actually splits.
///
/// `crates/oracle-aether/tests/scanlines.rs`'s a2 gate asserts the same shape over the wire, which is the
/// right place for the *contract*; this asserts it at the seam where the mechanism lives, so a break in the
/// mclk reduction, the CRAM-target filter, or which row a landing is filed against fails here — with a core
/// stack trace and no spawned process to read it through.
#[test]
fn the_row_a_mid_line_cram_write_lands_on_is_the_row_that_splits() {
    const LINE: usize = 50;
    let mut s = System::new(0x1234_5678);
    s.load_rom(oracle_core::testrom::build_cram_midframe(LINE as u8));
    s.reset();
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    s.run_frames_with_sink(6, &mut cap); // frame 0 is wholly colour A; read a later one

    let width = 256usize; // the fixture programs H32
    let px = cap.pixels();
    assert_eq!(px.len(), width * 224, "one complete H32 frame");
    let row = |n: usize| &px[n * width..(n + 1) * width];
    let transitions = |n: usize| row(n).windows(2).filter(|w| w[0] != w[1]).count();

    assert_eq!(
        transitions(LINE),
        1,
        "line {LINE} carries the write, so it splits — EXACTLY once. Zero means the emitter is still \
         line-atomic (or the write never reached the journal); more than one means it was applied out of \
         order or more than once."
    );
    for n in [LINE - 1, LINE + 1] {
        assert_eq!(
            transitions(n),
            0,
            "line {n} is uniform — the landing is filed against line {LINE} alone, not smeared across \
             its neighbours"
        );
    }
    assert_eq!(
        row(LINE)[0],
        row(LINE - 1)[0],
        "the split row opens on the colour its predecessor is wholly painted in"
    );
    assert_eq!(
        row(LINE)[width - 1],
        row(LINE + 1)[0],
        "and closes on the colour its successor is wholly painted in"
    );
    assert_ne!(
        row(LINE - 1)[0],
        row(LINE + 1)[0],
        "which are two different colours"
    );
}

/// Documented sharp edge 2: the frame index is `mclk / MCLK_PER_FRAME`, and `System::reset` zeroes mclk, so
/// the index REPEATS across a reset while `frames_completed` keeps climbing. Consumers must not treat it as
/// monotonic.
#[test]
fn the_frame_index_repeats_across_a_reset_while_the_count_keeps_climbing() {
    let mut s = booted(0x1234_5678);
    let mut sink = BoundaryLog::default();
    s.run_frames_with_sink(2, &mut sink);
    s.reset();
    s.run_frames_with_sink(2, &mut sink);
    assert_eq!(
        sink.log
            .iter()
            .filter_map(|d| match d {
                Delivery::Boundary(f) => Some(*f),
                Delivery::Line(_) => None,
            })
            .collect::<Vec<_>>(),
        vec![0, 1, 0, 1],
        "the index is derived from mclk, which reset zeroes — it is not a monotonic frame counter"
    );

    let mut s2 = booted(0x1234_5678);
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    s2.run_frames_with_sink(2, &mut cap);
    s2.reset();
    s2.run_frames_with_sink(2, &mut cap);
    assert_eq!(cap.frames_completed(), 4, "the COUNT is monotonic");
    assert_eq!(cap.last_frame_index(), Some(1), "the INDEX is not");
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
