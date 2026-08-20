//! The profiler panel — the CPU accountant's own readout, on the glass.
//!
//! **This is the profiler's fourth surface** (D15). The instrument has three already: the MCP tool rows, the
//! plain Aether methods, and the core's Rust API. Each of those answers a client that asked a question. This
//! one answers the person who is *watching the game run* — which is the case where "what is expensive right
//! now" is a question you have while looking at motion, not one you stop to compose a JSON-RPC call about.
//!
//! It reads and never arms. Arming is a bus operation (`emulator/set_profiler`), so a build with no bus
//! shows a permanently-off header over an empty sample rather than a control that cannot work.
//!
//! # What is shown, and why each part of it
//!
//! The header carries the two facts that are not derivable from each other: whether the instrument is
//! **armed**, and how many whole frames the sample **covers**. Disarming retains the sample (§11.16), so
//! rows can exist with nothing recording, and a panel showing only rows could not tell those apart.
//!
//! Then the top rows by cycles, the two interrupt buckets, and — **only when they are non-zero** — the
//! escape hatch and the two caveat counters. A counter that is always on screen reading `0` is noise a
//! reader learns to skip; one that appears only when it has something to say is a signal. The rule cuts the
//! other way too: `unattributedCycles`, `abandonedFrames` and `depthExceeded` each mean some row is
//! understating, so hiding them when they *are* non-zero would leave a reader trusting numbers they should
//! not.
//!
//! `perFrameExact` is marked in the header when false, because every divided figure below it is then
//! floored, one-sided low — a reader comparing one against a constant needs to know before, not after.

use crate::font;
use crate::overlay::{self, ACCENT, INFO};
use crate::present::Rect;
use oracle_core::profiler::{Counts, Profiler, LEVEL_HINT, LEVEL_VINT};
use oracle_core::symbols::SymbolTable;

/// How many routine rows the panel shows. The rows are ordered by cycles descending, so the head is where
/// the answer to "what is expensive" lives; a longer list is what `emulator/get_profiler_frames` is for.
pub const ROWS: usize = 8;

/// How far past a symbol's address a routine entry may sit and still be named by it. The run loop's own
/// constant for the CPU chip and the watch ticker is `0x1000`; a profiler row is an entry address, which
/// lands *on* a label far more often than a PC does, so the same bound is generous rather than tight.
const MAX_SYMBOL_DISPLACEMENT: u32 = 0x1000;

/// The instrument, and whether a client has it armed — what [`Bus::read_instruments`] hands the run loop,
/// carried into the model as one thing so a caller cannot pass the flag of one profiler with the rows of
/// another.
///
/// [`Bus::read_instruments`]: crate::bus::Bus::read_instruments
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub prof: &'a Profiler,
    /// Whether the profiler is *recording*. Not derivable from `prof`: disarming retains the sample.
    pub armed: bool,
}

/// The panel's model: the lines to draw, and the pause state the layout needs.
///
/// Lines rather than a struct-of-figures for the same reason [`watch::Ticker`](crate::lens::watch::Ticker)
/// is: the formatting is where the decisions are (which counters to suppress, how a nameless address is
/// spelled), and keeping them in the model is what lets a test assert on them without a window.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Panel {
    pub lines: Vec<String>,
    /// The run loop's `paused`. The panel steps clear of the `PAUSED` banner, and only when the banner is
    /// actually there — see [`top_of`].
    pub paused: bool,
    /// Whether the instrument is recording, carried into the draw so the header can say so in colour as
    /// well as in words. A reader glancing at the corner should be able to tell "these numbers are moving"
    /// from "these numbers are a record" without reading a line of text.
    pub armed: bool,
}

/// A routine's display name: the symbol that covers its entry address, or the bare address.
///
/// The bare address is spelled `$00XXXX` — the canonical 24-bit form the rest of this frontend uses — so a
/// row with no symbol is still something a reader can look up, rather than a decimal number they have to
/// convert first.
fn routine_name(addr: u32, symbols: Option<&SymbolTable>) -> String {
    symbols
        .and_then(|t| t.resolve_within(addr, MAX_SYMBOL_DISPLACEMENT))
        .map(|r| r.to_string())
        .unwrap_or_else(|| format!("${addr:06X}"))
}

/// One row: per-frame cycles, calls, then the name.
///
/// **The name goes last on purpose.** `overlay::fit` truncates the tail, and a truncated *name* is still an
/// approximation of where you are, while a truncated *number* is a plausible-looking wrong figure — the
/// same rule the CPU chip's `fit_tail` follows for its PC line, applied the other way round because here it
/// is the numbers that must survive.
fn row_line(addr: u32, c: &Counts, symbols: Option<&SymbolTable>) -> String {
    format!(
        "{:>8}c x{:<4}{}",
        c.cycles,
        c.calls,
        routine_name(addr, symbols)
    )
}

/// Turn the live instrument into the lines to draw. Pure: it reads the profiler and nothing else, and it
/// cannot arm, clear or otherwise move it.
pub fn model(view: View<'_>, symbols: Option<&SymbolTable>, paused: bool, rows: usize) -> Panel {
    // `report()` allocates two maps, which is why `models()` builds this only when the lens is on — the
    // same gate `cram_decoded` and `sprites_decoded` are behind.
    let r = view.prof.report();
    let mut lines = Vec::new();
    lines.push(format!(
        "PROFILER {}  frames {}{}",
        if view.armed { "ARMED" } else { "OFF" },
        r.frame_count,
        // Every divided figure below is floored when this is false. Said in the header, before the numbers
        // it qualifies, rather than in a footnote after them.
        if r.per_frame_exact { "" } else { "  FLOORED" },
    ));
    if r.frame_count == 0 {
        // A sample of no whole frames is not an error and must not be dressed as one: the instrument opens
        // its sample at the first frame boundary after arming, so "armed a moment ago" lands here honestly.
        lines.push("(no whole frames sampled yet)".to_string());
        return Panel {
            lines,
            paused,
            armed: view.armed,
        };
    }
    // Ordered by cycles descending, ties broken by address so the panel does not shuffle between frames
    // when two routines cost the same — a list that reorders under a steady state is unreadable.
    let mut by_cost: Vec<(u32, Counts)> = r.routines.iter().map(|(&a, &c)| (a, c)).collect();
    by_cost.sort_by(|a, b| b.1.cycles.cmp(&a.1.cycles).then(a.0.cmp(&b.0)));
    for (addr, c) in by_cost.iter().take(rows) {
        lines.push(row_line(*addr, c, symbols));
    }
    // Both buckets on one line, and both keyed by the acknowledged CAUSE rather than by where a handler
    // lives. Shown even at zero, unlike the caveats below: a zero here is a real measurement ("no HBlanks
    // were taken this sample"), while a zero caveat is the absence of a problem.
    let hint = r.interrupts.get(&LEVEL_HINT).copied().unwrap_or_default();
    let vint = r.interrupts.get(&LEVEL_VINT).copied().unwrap_or_default();
    lines.push(format!(
        "VINT {}c x{}   HINT {}c x{}",
        vint.cycles, vint.calls, hint.cycles, hint.calls
    ));
    // The escape hatch. Non-zero is the ordinary phase for a ROM whose VBlank handler straddles the opening
    // boundary, so it is information rather than an alarm — but it is the term that keeps the reconciliation
    // identity closed, and a reader adding the rows up needs it.
    if r.unattributed_cycles > 0 {
        lines.push(format!("UNATTRIBUTED {}c", r.unattributed_cycles));
    }
    // The two ways a row can understate. Either one means the accountant lost the thread of the program's
    // stack, which is a fact about the measurement rather than about the program.
    if r.abandoned_frames > 0 || r.depth_exceeded > 0 {
        lines.push(format!(
            "ABANDONED {}  DEPTH-EXCEEDED {}",
            r.abandoned_frames, r.depth_exceeded
        ));
    }
    Panel {
        lines,
        paused,
        armed: view.armed,
    }
}

/// The widest line, in device pixels.
fn content_width(lines: &[String], px: usize) -> usize {
    lines
        .iter()
        .map(|l| font::text_width(l) * px)
        .max()
        .unwrap_or(0)
}

/// **The lowest row anything above the panel occupies** — the floor the panel must sit below.
///
/// The F3 status band always, and the `PAUSED` banner while the machine is stopped. Both are
/// [`overlay`]'s to report rather than this module's to re-derive: a second copy of a geometry is correct
/// only on the day it is written, which is exactly how the CPU chip once ended up drawing *under* the
/// status line. One definition, several readers.
///
/// The banner's contribution ignores columns, unlike the CPU chip's, and that is because this panel is
/// anchored to the **bottom** rather than to the banner's own corner: it is already far below the banner at
/// every ordinary size, so this is a bail condition for pictures too short to hold the panel at all, not a
/// layout that shifts under one.
fn floor_of(area: Rect, px: usize, paused: bool) -> usize {
    let band = overlay::status_band(area, px);
    let mut floor = band.y + band.h;
    if paused {
        if let Some(b) = overlay::paused_banner_rect(area, px) {
            floor = floor.max(b.y + b.h);
        }
    }
    floor
}

/// **Left of `area`, stacked directly above the watch ticker's strip.**
///
/// Bottom-anchored on purpose. The three corners are taken — the ticker owns the bottom edge, the CPU chip
/// the top right, the CRAM strip the top left — and the top of the picture is also where everything the
/// *overlay* draws lives. A readout of this height has nowhere to go up there that does not end in the
/// arbitration `cpu::top_of` exists for, so it goes where the overlay is not: above the ticker, below
/// everything else, clear by distance rather than by negotiation. The offset past the ticker is
/// [`watch::strip_height`](crate::lens::watch::strip_height)'s to report, and it is unconditional — a panel
/// that jumped five rows whenever the ticker was toggled would be worse than one sitting five rows higher
/// than it strictly needs to.
///
/// A picture that cannot hold the whole panel between [`floor_of`] and the ticker draws **nothing**. There
/// is no degrading form the way the CPU chip has one, and it is the CRAM strip's call for the strip's
/// reason: a clipped profile is not a shorter profile. The rows are ordered by cost, so the part that falls
/// off is as likely to be the head — the expensive routines, the only part anyone reads it for — as any
/// other. The bail is also what keeps the `usize` geometry below from underflowing on a tiny area (the
/// `draw_narrow_panel_does_not_underflow` class).
pub fn draw(c: &mut font::Canvas, area: Rect, px: usize, p: &Panel) {
    if p.lines.is_empty() {
        return;
    }
    let pad = 2 * px;
    let margin = (2 * px).max(4);
    let line_h = font::LINE_H * px;
    let panel_w = (content_width(&p.lines, px) + 2 * pad).min(area.w.saturating_sub(2 * margin));
    let panel_h = p.lines.len() * line_h + 2 * pad;
    let floor = floor_of(area, px, p.paused);
    // Saturating throughout, and the comparison happens before any subtraction, so a picture shorter than
    // the things stacked in it bails instead of wrapping into an enormous `top`.
    let bottom = (area.y + area.h).saturating_sub(margin + crate::lens::watch::strip_height(px));
    if panel_w < 16 * px || bottom < floor + panel_h {
        return;
    }
    let left = (area.x + margin) as i32;
    let top = (bottom - panel_h) as i32;
    c.fill_rect(left, top, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);

    let avail = panel_w.saturating_sub(2 * pad);
    for (i, l) in p.lines.iter().enumerate() {
        c.text(
            left + pad as i32,
            top + pad as i32 + (i * line_h) as i32,
            px,
            // The header is amber while the instrument is RECORDING, so "these numbers are still
            // moving" is legible at a glance rather than only by reading the word ARMED.
            if i == 0 && p.armed { ACCENT } else { INFO },
            overlay::fit(l, avail, px),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::ink_bounds;
    use oracle_core::bus::{BusEvent, BusEventSink, BusOp, Size, StepRetire};
    use oracle_core::system::System;
    use oracle_core::testrom::{self, ProfilerShape};

    /// The buffer fill every draw test starts from, and **the reason it is not `0`**: the panel is
    /// `fill_rect(..., 0x0000_0000, PANEL_ALPHA)` — black, alpha-blended — which over a zero buffer is a
    /// no-op. A `!= 0` assertion would be blind to the largest thing `draw` paints. Same value, same
    /// reason, as every other lens module's.
    const BG: u32 = 0x0012_3456;

    /// A profiler carrying a real sample. `ModeSwitch` calls a leaf in **user** mode with VBlank live, so
    /// the sample holds several routine rows, a handler row and a VBlank bucket — the shape the panel is
    /// for, produced by running the machine rather than by hand-filling a struct.
    fn sampled() -> Profiler {
        let mut sys = System::new(0x1234_5678);
        sys.load_rom(testrom::build_profiler(ProfilerShape::ModeSwitch));
        sys.reset();
        let mut prof = Profiler::new();
        sys.run_frames_with_sink(4, &mut prof);
        prof
    }

    fn model_of(prof: &Profiler, armed: bool) -> Panel {
        model(View { prof, armed }, None, false, ROWS)
    }

    // --- The header -------------------------------------------------------------------------------

    /// **Armed and sampled are two facts, and the header carries both.** Disarming RETAINS the sample
    /// (§11.16), so an instrument holding rows says nothing about whether it is still recording — a panel
    /// that showed only the rows would report a frozen record as a live measurement. Both directions are
    /// asserted over the SAME sample, so the difference can only be the flag.
    #[test]
    fn the_header_reports_arming_and_the_sample_separately() {
        let prof = sampled();
        let armed = model_of(&prof, true);
        let disarmed = model_of(&prof, false);
        assert!(
            armed.lines[0].contains("ARMED"),
            "an armed instrument must say so: {:?}",
            armed.lines[0]
        );
        assert!(
            disarmed.lines[0].contains("OFF"),
            "and a disarmed one must not: {:?}",
            disarmed.lines[0]
        );
        assert_eq!(
            armed.lines[1..],
            disarmed.lines[1..],
            "the flag changes the header and nothing else — the retained sample is the same sample"
        );
        assert!(
            armed.armed && !disarmed.armed,
            "and it reaches the draw, which colours the header by it"
        );
        let frames = prof.report().frame_count;
        assert!(
            frames > 0,
            "the fixture must actually have sampled something"
        );
        assert!(
            armed.lines[0].contains(&format!("frames {frames}")),
            "the header reports the frames the sample covers ({frames}): {:?}",
            armed.lines[0]
        );
    }

    /// A sample of no whole frames is the ordinary state one instant after arming — the instrument opens
    /// its sample at the first frame boundary — so it is answered, not dressed as an error, and above all
    /// not answered with a table of zeroes that looks like a measurement.
    #[test]
    fn an_empty_sample_says_so_instead_of_reporting_zeroes() {
        let p = model_of(&Profiler::new(), true);
        assert_eq!(
            p.lines.len(),
            2,
            "header plus the note, and no rows at all: {:?}",
            p.lines
        );
        assert!(p.lines[0].contains("frames 0"), "{:?}", p.lines[0]);
        assert!(p.lines[1].contains("no whole frames"), "{:?}", p.lines[1]);
        assert!(
            !p.lines.iter().any(|l| l.starts_with("VINT")),
            "a bucket line here would read as 'the interrupts cost nothing', which is not what \
             'nothing has been measured' means: {:?}",
            p.lines
        );
    }

    // --- The rows ---------------------------------------------------------------------------------

    /// Ordered by cycles descending and bounded by the row cap. Descending because the head is the answer
    /// to "what is expensive"; bounded because the panel is a corner of a game window and the full list is
    /// what `emulator/get_profiler_frames` is for.
    #[test]
    fn rows_are_the_costliest_first_and_no_more_than_the_cap() {
        let prof = sampled();
        let report = prof.report();
        assert!(
            report.routines.len() >= 2,
            "the fixture must have at least two rows or ordering proves nothing ({} rows)",
            report.routines.len()
        );
        let full = model_of(&prof, true);
        // Header, rows, bucket line, then any caveats. The rows are the slice between.
        let cycles: Vec<u64> = full.lines[1..]
            .iter()
            .take_while(|l| !l.starts_with("VINT"))
            .map(|l| {
                l.split('c')
                    .next()
                    .expect("a row starts with its cycle count")
                    .trim()
                    .parse()
                    .expect("the cycle field parses")
            })
            .collect();
        assert_eq!(
            cycles.len(),
            report.routines.len().min(ROWS),
            "every row up to the cap, and no more"
        );
        assert!(
            cycles.windows(2).all(|w| w[0] >= w[1]),
            "rows must be ordered by cycles descending: {cycles:?}"
        );
        // The cap really caps. Asked for one row, given one — otherwise "no more than the cap" above is
        // satisfied by a fixture that simply has fewer rows than the cap.
        let capped = model(
            View {
                prof: &prof,
                armed: true,
            },
            None,
            false,
            1,
        );
        assert_eq!(
            capped.lines[1..]
                .iter()
                .take_while(|l| !l.starts_with("VINT"))
                .count(),
            1,
            "the row cap is honoured: {:?}",
            capped.lines
        );
        assert_eq!(
            capped.lines[1], full.lines[1],
            "and the one it keeps is the costliest, not an arbitrary one"
        );
    }

    /// A row is named when a symbol covers its entry address and addressed when none does. Both halves,
    /// over the same row: an implementation that never resolved anything would pass the second alone.
    #[test]
    fn a_row_is_named_by_its_symbol_and_addressed_without_one() {
        let prof = sampled();
        let addr = *prof
            .sample_routines()
            .keys()
            .next()
            .expect("the fixture has rows");
        let table = oracle_core::symbols::SymbolTable::parse(&format!(
            "  Symbol Table (* = unused):\n  --------------------------\n\n \
             PROF_FIXTURE_ROUTINE : {addr:X} C |\n\n    1 symbols\n    0 unused symbols\n"
        ))
        .expect("the fixture listing parses");
        assert_eq!(
            routine_name(addr, Some(&table)),
            "PROF_FIXTURE_ROUTINE",
            "a symbol covering the entry address names the row"
        );
        assert_eq!(
            routine_name(addr, None),
            format!("${addr:06X}"),
            "and with no table the row is the canonical 24-bit address, not a decimal number"
        );
    }

    // --- The caveats, both ways --------------------------------------------------------------------

    /// A profiler driven straight through its sink, arranged to produce **both** things the panel is
    /// supposed to confess: a suppressed pre-sample bucket whose own time becomes `unattributedCycles`,
    /// and a return the accountant could not match, which tears a frame off the stack.
    ///
    /// Driven synthetically because it must be: an interrupt has to straddle the opening boundary and a
    /// return has to land where no frame was entered, and timing a real machine onto both is a coin flip.
    fn profiler_with_caveats() -> Profiler {
        const S: u32 = 0x00FF_FF00;
        const OP_NOP: u16 = 0x4E71;
        const OP_JSR: u16 = 0x4EB8;
        const OP_RTE: u16 = 0x4E73;
        const OP_RTS: u16 = 0x4E75;
        let step = |pc: u32, opcode: u16, sp: u32, ssp: u32| StepRetire {
            pc,
            opcode,
            sp,
            ssp,
            cycles: 10,
            stall_cycles: 0,
            executed: true,
            supervisor: true,
        };
        let entry = |pc: u32, sp: u32, ssp: u32| StepRetire {
            executed: false,
            ..step(pc, OP_NOP, sp, ssp)
        };
        let mut p = Profiler::new();
        // A VBlank taken BEFORE the sample opens, whose handler returns, leaving the bucket on top.
        p.on_event(BusEvent {
            op: BusOp::Read,
            fc: 7,
            addr: 0x00FF_FFF1 | (6 << 1),
            size: Size::Word,
            value: 0,
        });
        p.on_step_retire(entry(0x2000, S - 6, S - 6));
        p.on_step_retire(step(0x3000, OP_NOP, S - 6, S - 6));
        p.on_step_retire(step(0x3002, OP_RTS, S - 2, S - 2));
        p.on_frame_boundary(0); // the sample opens: that bucket is suppressed
        p.on_step_retire(step(0x3004, OP_NOP, S - 2, S - 2)); // its own time -> unattributed
        p.on_step_retire(step(0x3006, OP_RTE, S, S));
        // And a wedge: INNER leaves by a route no frame was entered at, so OUTER's return finds it
        // stranded on top and unwinds it as abandoned.
        p.on_step_retire(step(0x1000, OP_NOP, S, S));
        p.on_step_retire(step(0x1002, OP_JSR, S - 4, S - 4));
        p.on_step_retire(step(0x2000, OP_NOP, S - 4, S - 4));
        p.on_step_retire(step(0x2002, OP_JSR, S - 8, S - 8));
        p.on_step_retire(step(0x3000, OP_NOP, S - 8, S - 8));
        p.on_step_retire(step(0x3002, OP_RTS, S - 6, S - 6));
        p.on_step_retire(step(0x2004, OP_RTS, S, S));
        p.on_frame_boundary(1);
        p
    }

    /// **The suppression rule, proven in both directions.** A counter always on screen reading `0` is
    /// noise a reader learns to skip; one that appears only when it has something to say is a signal. And
    /// each of these means some row is understating, so hiding a NON-zero one would leave a reader
    /// trusting numbers they should not.
    #[test]
    fn the_escape_hatch_and_the_caveats_appear_exactly_when_they_are_non_zero() {
        let clean = sampled();
        let clean_report = clean.report();
        assert_eq!(
            (
                clean_report.abandoned_frames,
                clean_report.depth_exceeded,
                clean_report.unattributed_cycles
            ),
            (0, 0, 0),
            "the clean fixture must really be clean or the absence below proves nothing"
        );
        let quiet = model_of(&clean, true);
        assert!(
            !quiet.lines.iter().any(|l| l.contains("ABANDONED")),
            "no caveat line when there is nothing to confess: {:?}",
            quiet.lines
        );
        assert!(
            !quiet.lines.iter().any(|l| l.contains("UNATTRIBUTED")),
            "nor an escape hatch reading zero: {:?}",
            quiet.lines
        );

        let noisy = profiler_with_caveats();
        let noisy_report = noisy.report();
        assert_eq!(
            (
                noisy_report.abandoned_frames,
                noisy_report.unattributed_cycles
            ),
            // 20, not 10: two steps retire inside the suppressed bucket — the `nop` and the `rte`
            // itself, which is charged before it is classified — and both are its own time.
            (1, 20),
            "the synthetic stream must produce both, or the presence below is untested"
        );
        let loud = model_of(&noisy, true);
        assert!(
            loud.lines.iter().any(|l| l == "UNATTRIBUTED 20c"),
            "the escape hatch is reported, exactly: {:?}",
            loud.lines
        );
        assert!(
            loud.lines
                .iter()
                .any(|l| l == "ABANDONED 1  DEPTH-EXCEEDED 0"),
            "and so are the two counters that say a row understates: {:?}",
            loud.lines
        );
    }

    /// Both buckets are shown even at zero, unlike the caveats — a zero here is a **measurement** ("no
    /// HBlanks were taken this sample"), not the absence of a problem, and a reader asking where the
    /// frame went needs to see that it did not go there.
    #[test]
    fn both_interrupt_buckets_are_shown_even_when_one_is_empty() {
        let prof = sampled();
        let p = model_of(&prof, true);
        let bucket = p
            .lines
            .iter()
            .find(|l| l.starts_with("VINT"))
            .expect("a bucket line");
        assert!(
            bucket.contains("HINT"),
            "both causes on one line: {bucket:?}"
        );
        let report = prof.report();
        assert!(
            !report.interrupts.contains_key(&LEVEL_HINT),
            "the fixture arms no HBlank, so the HINT half really is the zero case"
        );
        assert!(
            report.interrupts.contains_key(&LEVEL_VINT),
            "and the VBlank half really is not"
        );
        assert!(
            bucket.contains("HINT 0c x0"),
            "the empty cause is reported as zero rather than omitted: {bucket:?}"
        );
    }

    // --- Pixels -----------------------------------------------------------------------------------

    fn panel(lines: usize, paused: bool) -> Panel {
        Panel {
            lines: (0..lines)
                .map(|_| "ABCDEFGHIJKLMNOPQRST".to_string())
                .collect(),
            paused,
            armed: true,
        }
    }

    /// The panel leaves a mark and every pixel of it is inside the picture.
    ///
    /// The lower bound is the **panel's own area**, which text alone could never reach: if this ever
    /// fails the panel has gone invisible against `BG` again and every other pixel assertion here is
    /// blind. The magic numbers are written out rather than re-derived from production — a bound computed
    /// by the code under test moves with the bug.
    #[test]
    fn draw_paints_inside_the_picture_and_leaves_its_whole_panel_behind() {
        let (w, h) = (320usize, 224usize);
        let area = Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        let px = 1;
        let p = panel(3, false);
        let mut buf = vec![BG; w * h];
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(&mut c, area, px, &p);
        }
        // px 1: margin `(2*px).max(4)` = 4, pad = 2, line box 8. Twenty glyphs are `20*6 - 1` = 119 wide,
        // so the panel is 123 x (3*8 + 4) = 123 x 28.
        let (panel_w, panel_h) = (123usize, 28usize);
        let painted = buf.iter().filter(|q| **q != BG).count();
        assert!(
            painted >= panel_w * panel_h,
            "the panel left no mark: {painted} changed, panel is {panel_w}x{panel_h}"
        );
        for (i, q) in buf.iter().enumerate() {
            if *q != BG {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
        // And it sits where it says it does: bottom edge one ticker-strip clear of the picture's, left
        // edge one margin in. `strip_height(1)` is `5 * 8 + 4` = 44, written out.
        let (top, bottom, left, right) =
            ink_bounds(&buf, w, BG).expect("the panel painted something");
        let expect_bottom = area.y + area.h - 4 - 44 - 1;
        assert_eq!(
            (top, bottom, left, right),
            (
                expect_bottom + 1 - panel_h,
                expect_bottom,
                area.x + 4,
                area.x + 4 + panel_w - 1
            ),
            "the panel's four edges"
        );
    }

    /// **At every font scale**, because `margin = (2 * px).max(4)` is 4 at both px 1 and px 2 — the floor
    /// still binds, so a test-local `margin = 4` is the identity everywhere px ≤ 2 looks and would ship
    /// green. It first diverges at px 4, where the correct margin is 8. `video.rs` closed this exact gap
    /// in `1407d07`; this module gets it from the start.
    #[test]
    fn the_panel_clears_the_ticker_strip_at_every_font_scale() {
        let (w, h) = (896usize, 672usize);
        let area = Rect { x: 0, y: 0, w, h };
        for px in [1usize, 2, 4] {
            let p = panel(3, false);
            let mut buf = vec![BG; w * h];
            {
                let mut c = font::Canvas::new(&mut buf, w, h);
                draw(&mut c, area, px, &p);
            }
            let margin = (2 * px).max(4);
            // Five rows of `LINE_H` plus two pads: `(4 + 1) * 8 * px + 2 * (2 * px)` = `44 * px`.
            let strip = 44 * px;
            let panel_h = 3 * 8 * px + 2 * (2 * px);
            let panel_w = (20 * 6 - 1) * px + 2 * (2 * px);
            let (top, bottom, left, right) =
                ink_bounds(&buf, w, BG).unwrap_or_else(|| panic!("px {px}: nothing painted"));
            let expect_bottom = area.y + area.h - margin - strip - 1;
            assert_eq!(
                (top, bottom, left, right),
                (
                    expect_bottom + 1 - panel_h,
                    expect_bottom,
                    area.x + margin,
                    area.x + margin + panel_w - 1
                ),
                "px {px}: the panel's four edges"
            );
        }
    }

    /// **The panel never lands under the `PAUSED` banner** — it clears it, or it does not draw.
    ///
    /// The overlay is drawn after every lens and its panels are only `PANEL_ALPHA` opaque, so a glyph
    /// beneath one is dimmed to about a quarter and reads as *absent* beside its bright neighbours: a
    /// `12345c` with a glyph extinguished is a plausible wrong number, not a visibly damaged one.
    ///
    /// **Swept over panel heights past today's, deliberately.** At [`ROWS`] the panel is at most twelve
    /// lines and sits comfortably below the banner at every geometry here, so a sweep at that height
    /// passes with the banner guard *deleted* — measured, not assumed. The guard exists for the panel
    /// this one grows into, and the taller probes are what hold it: at 320x224 a seventeen-line panel
    /// pushed up past the banner without it. Either answer is correct for such a picture — draw clear of
    /// the banner, or draw nothing — and the assertion is over both.
    ///
    /// Geometries are `the_overlay_never_extinguishes_a_lens_glyph`'s, including a letterboxed picture
    /// with a non-zero origin so an offset that ignored `area` cannot pass.
    #[test]
    fn a_paused_panel_stays_clear_of_the_paused_banner() {
        let mut checked = 0usize;
        let mut ever_painted = 0usize;
        for (label, w, h, area) in [
            (
                "the default window",
                320usize,
                224usize,
                Rect {
                    x: 0,
                    y: 0,
                    w: 320,
                    h: 224,
                },
            ),
            (
                "the owner's window",
                896,
                672,
                Rect {
                    x: 0,
                    y: 0,
                    w: 896,
                    h: 672,
                },
            ),
            (
                "a letterboxed picture with a non-zero origin",
                700,
                520,
                crate::present::dest_rect(700, 520, 320, 224, crate::present::Aspect::Integer),
            ),
        ] {
            let px = crate::overlay::Overlay::font_scale(area.h.max(1));
            let banner = overlay::paused_banner_rect(area, px)
                .unwrap_or_else(|| panic!("{label}: no banner, so nothing is being cleared"));
            // Twelve is the tallest the panel gets today (header + ROWS + the bucket line + the two
            // conditional caveat lines); the rest are the growth this guard is for.
            for lines in [2usize, 12, 17, 24] {
                let mut buf = vec![BG; w * h];
                {
                    let mut c = font::Canvas::new(&mut buf, w, h);
                    draw(&mut c, area, px, &panel(lines, true));
                }
                let mut painted = 0usize;
                for (i, q) in buf.iter().enumerate() {
                    if *q != BG {
                        painted += 1;
                        let (x, y) = (i % w, i / w);
                        assert!(
                            !(x >= banner.x
                                && x < banner.x + banner.w
                                && y >= banner.y
                                && y < banner.y + banner.h),
                            "{label}, {lines} lines: the panel painted inside the PAUSED banner at \
                             ({x},{y}) — a dimmed glyph beside bright ones reads as missing"
                        );
                    }
                }
                if painted > 0 {
                    ever_painted += 1;
                }
                checked += 1;
            }
        }
        assert_eq!(checked, 3 * 4, "the sweep did not cover every case");
        // Vacuity: "clear of the banner" is trivially true of a panel that never drew, so at least some
        // of the sweep must have put ink on the glass.
        assert!(
            ever_painted >= 3,
            "the panel drew in only {ever_painted} of the swept cases, so the clearance above is \
             mostly a statement about an empty buffer"
        );
    }

    /// A picture too short to hold the panel between the status band and the ticker draws **nothing** —
    /// and, just as importantly, does not underflow the `usize` geometry into a top somewhere off the
    /// world (the `draw_narrow_panel_does_not_underflow` hazard class).
    #[test]
    fn a_short_area_draws_nothing_and_does_not_underflow() {
        let (w, h) = (320usize, 224usize);
        for area_h in [1usize, 8, 40, 60] {
            let area = Rect {
                x: 0,
                y: 0,
                w: 320,
                h: area_h,
            };
            let mut buf = vec![BG; w * h];
            {
                let mut c = font::Canvas::new(&mut buf, w, h);
                draw(&mut c, area, 1, &panel(10, false));
            }
            assert!(
                buf.iter().all(|q| *q == BG),
                "h {area_h}: a picture with no room for the panel must draw nothing"
            );
        }
    }

    /// Likewise too narrow: a panel that cannot hold sixteen glyph cells says nothing honestly, so it
    /// says nothing.
    #[test]
    fn a_narrow_area_draws_nothing() {
        let (w, h) = (320usize, 224usize);
        for area_w in [1usize, 10, 20] {
            let area = Rect {
                x: 0,
                y: 0,
                w: area_w,
                h: 224,
            };
            let mut buf = vec![BG; w * h];
            {
                let mut c = font::Canvas::new(&mut buf, w, h);
                draw(&mut c, area, 1, &panel(3, false));
            }
            assert!(
                buf.iter().all(|q| *q == BG),
                "w {area_w}: a picture too narrow for the panel must draw nothing"
            );
        }
    }

    /// An empty model paints nothing — the guard that keeps the `lines.len() * line_h` panel from being a
    /// zero-row rectangle with padding and no content.
    #[test]
    fn an_empty_model_paints_nothing() {
        let (w, h) = (320usize, 224usize);
        let mut buf = vec![BG; w * h];
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(&mut c, Rect { x: 0, y: 0, w, h }, 1, &Panel::default());
        }
        assert!(buf.iter().all(|q| *q == BG), "no lines, no panel");
    }
}
