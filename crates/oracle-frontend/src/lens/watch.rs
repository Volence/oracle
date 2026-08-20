//! The watch ticker (spec §5.1) — a bottom strip streaming the newest watch hits plus the armed
//! count.
//!
//! Reads the hit ring through the **non-destructive** `hits()`. Never `take_hits()`: the ring is
//! shared with socket clients, and a lens that consumed it would delete a client's evidence just
//! by being switched on (the rule main.rs:1152-1155 states for the `W` key).

use crate::overlay::{self, ACCENT, INFO};
use crate::present::Rect;
use crate::{font, MAX_SYMBOL_DISPLACEMENT};
use oracle_core::symbols::SymbolTable;
use oracle_core::watchpoints::{WatchHit, WatchSpace, Watchpoints};

/// How many hits the strip shows. Four fits under the picture without eating it; the full log is
/// still one `W` away.
pub const ROWS: usize = 4;

/// What the strip draws: already-formatted lines (newest last, reading order), plus the two
/// numbers that tell you whether to believe them.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Ticker {
    pub lines: Vec<String>,
    pub armed: usize,
    pub dropped: u64,
}

/// The space spelling. Hand-rolled because core has no `Display` for `WatchSpace` and the two
/// existing spellings disagree (`dump_hits` says `Bus`, the bus protocol says `bus`); the bus
/// protocol's lowercase wins here, because that is what a hit looks like everywhere a client
/// sees it.
fn space_name(s: WatchSpace) -> &'static str {
    match s {
        WatchSpace::Bus => "bus",
        WatchSpace::Vram => "vram",
        WatchSpace::Cram => "cram",
        WatchSpace::Vsram => "vsram",
    }
}

/// One hit, compact: `w0 vram $4A00 3F->12 @f811 Sonic_Move+$1C`.
///
/// Deliberately the same field order and the same `$`-hex spellings as `dump_hits`, so the strip
/// and the terminal log read as one instrument. The PC is symbolised through the same
/// `resolve_within(_, MAX_SYMBOL_DISPLACEMENT)` the log uses, and falls back to raw hex — a name
/// 4 KiB past its symbol would be actively misleading.
///
/// Private: `dump_hits` formats its own, wider line (it carries `seq` and `via` too), so this is
/// the strip's own spelling and has no second caller to answer to.
fn line(h: &WatchHit, symbols: Option<&SymbolTable>) -> String {
    let at = symbols
        .and_then(|t| t.resolve_within(h.pc, MAX_SYMBOL_DISPLACEMENT))
        .map(|r| r.to_string())
        .unwrap_or_else(|| format!("${:06X}", h.pc));
    format!(
        "w{} {} ${:04X} {:X}->{:X} @f{} {}",
        h.watch.0,
        space_name(h.space),
        h.addr,
        h.old,
        h.value,
        h.frame,
        at
    )
}

/// The newest `rows` hits, oldest first within the strip. `hits()` is seq-ascending, so this is a
/// tail — no sort, no allocation beyond the strings themselves.
pub fn model(wp: &Watchpoints, symbols: Option<&SymbolTable>, rows: usize) -> Ticker {
    let hits = wp.hits();
    let start = hits.len().saturating_sub(rows);
    Ticker {
        lines: hits[start..].iter().map(|h| line(h, symbols)).collect(),
        armed: wp.watch_count(),
        dropped: wp.dropped(),
    }
}

/// **The height the ticker's strip reserves at the bottom edge**, in device pixels, at its full
/// [`ROWS`] — panel and padding, not just the text.
///
/// Exported for the same reason [`overlay::status_band`] is: the ticker owns the bottom edge, another lens
/// has to stack above it, and the alternative is that lens re-deriving `ROWS`, `LINE_H` and the padding for
/// itself. A second copy of a geometry is correct only on the day it is written — which is exactly how the
/// CPU chip once ended up drawing *under* the status line. One definition, two readers.
///
/// It reports what the strip *would* stand at, unconditionally, whether or not this lens is on and however
/// many hits it currently holds. A neighbour that jumped five rows whenever a watch fired, or whenever the
/// ticker was toggled, would be worse than one sitting five rows higher than it strictly needs to.
pub fn strip_height(px: usize) -> usize {
    (ROWS + 1) * font::LINE_H * px + 2 * (2 * px)
}

/// Bottom strip of `area`. Toasts stack from the same edge and are drawn later, so a burst of
/// toasts covers the ticker briefly — accepted: toasts are transient and the ticker is not.
pub fn draw(c: &mut font::Canvas, area: Rect, px: usize, t: &Ticker) {
    let pad = 2 * px;
    let margin = (2 * px).max(4);
    let line_h = font::LINE_H * px;
    let rows = t.lines.len() + 1; // + the header
    let panel_h = rows * line_h + 2 * pad;
    debug_assert!(
        panel_h <= strip_height(px),
        "the strip stands taller than the height it reserves, so a lens stacking above it overlaps"
    );
    // Too small to say anything honestly — and the `top` below is `usize` arithmetic, so this is
    // also what stops a short area underflowing rather than drawing off the top of the world.
    if area.w < 16 * px || area.h < panel_h + margin {
        return;
    }
    let panel_w = area.w.saturating_sub(2 * margin);
    let left = (area.x + margin) as i32;
    let top = (area.y + area.h - margin - panel_h) as i32;
    c.fill_rect(left, top, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);

    let avail = panel_w.saturating_sub(2 * pad);
    // Amber once anything has been dropped: the lines below are then a sample, not the record.
    let head = format!("WATCH  armed {}  dropped {}", t.armed, t.dropped);
    c.text(
        left + pad as i32,
        top + pad as i32,
        px,
        if t.dropped > 0 { ACCENT } else { INFO },
        overlay::fit(&head, avail, px),
    );
    for (i, l) in t.lines.iter().enumerate() {
        c.text(
            left + pad as i32,
            top + pad as i32 + ((i + 1) * line_h) as i32,
            px,
            INFO,
            overlay::fit(l, avail, px),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::bus::{BusEvent, BusEventSink, BusOp, Size};
    use oracle_core::watchpoints::{WatchId, WatchOp, WatchVia};

    fn hit(seq: u64, addr: u32, old: u32, value: u32) -> WatchHit {
        WatchHit {
            watch: WatchId(0),
            space: WatchSpace::Vram,
            addr,
            old,
            value,
            size: Size::Byte,
            op: BusOp::Write,
            fc: 0,
            via: WatchVia::Direct,
            pc: 0x00_0100,
            frame: 811,
            mclk: 0,
            seq,
        }
    }

    /// A byte write the bus watch below will match. `fc` 5 is supervisor data — the ordinary CPU
    /// store; the watch carries no `fc` filter, so it matches either way.
    fn write_event(addr: u32, value: u32) -> BusEvent {
        BusEvent {
            op: BusOp::Write,
            fc: 5,
            addr,
            size: Size::Byte,
            value,
        }
    }

    /// Feed `n` distinct writes at `$0100..` through the real sink, so the tests below read a ring
    /// the core filled rather than one a test hand-assembled. The PC is deliberately **not**
    /// `$01xx`: it renders as `$00ABCD`, which cannot be confused with an address in an assertion.
    fn armed_with_hits(cap: usize, watches: usize, n: u32) -> Watchpoints {
        let mut wp = Watchpoints::new(cap);
        for i in 0..watches {
            wp.add_watch(0..=0xFFFF, WatchOp::Write, format!("test{i}"));
        }
        wp.on_step_boundary(0x00_ABCD, 811);
        for i in 0..n {
            wp.on_event(write_event(0x0100 + i, i));
        }
        wp
    }

    #[test]
    fn a_line_reads_like_the_terminal_log() {
        let l = line(&hit(1, 0x4A00, 0x3F, 0x12), None);
        assert_eq!(l, "w0 vram $4A00 3F->12 @f811 $000100");
    }

    /// The strip is read by the same people reading `emulator/watchpoint_hits` over the socket, so
    /// the space spellings are the bus protocol's lowercase ones — and each space keeps its own.
    #[test]
    fn every_space_keeps_its_own_lowercase_spelling() {
        let names: Vec<&str> = [
            WatchSpace::Bus,
            WatchSpace::Vram,
            WatchSpace::Cram,
            WatchSpace::Vsram,
        ]
        .iter()
        .map(|s| space_name(*s))
        .collect();
        assert_eq!(names, ["bus", "vram", "cram", "vsram"]);
    }

    /// The ring holds thousands; the strip shows the newest few, in reading order. The ring is
    /// left deliberately roomy so the trimming under test is **the model's**, not the ring's.
    #[test]
    fn the_model_takes_the_newest_rows_oldest_first() {
        let wp = armed_with_hits(64, 1, 6);
        assert_eq!(wp.hits().len(), 6, "the ring kept every hit");

        let t = model(&wp, None, ROWS);
        assert_eq!(t.lines.len(), ROWS);
        assert!(
            t.lines[0].contains("$0102"),
            "the tail starts at the 3rd of 6: {:?}",
            t.lines
        );
        assert!(
            t.lines[ROWS - 1].contains("$0105"),
            "and ends at the newest: {:?}",
            t.lines
        );
        assert!(
            !t.lines.iter().any(|l| l.contains("$0100")),
            "the oldest is off the strip: {:?}",
            t.lines
        );
        assert_eq!(t.armed, 1, "armed is the watch count");
        assert_eq!(t.dropped, 0, "nothing was dropped at cap 64");
    }

    /// The two numbers that say whether to believe the lines: how many watches are armed, and how
    /// many hits the ring threw away. A strip that showed neither would read a gap as quiet.
    #[test]
    fn the_model_carries_the_armed_and_dropped_counts() {
        let wp = armed_with_hits(2, 3, 5);
        let t = model(&wp, None, ROWS);
        assert_eq!(t.armed, 3, "three watches are armed");
        assert_eq!(t.dropped, 3, "cap 2 with 5 hits drops the oldest 3");
        assert_eq!(
            t.lines.len(),
            2,
            "the strip cannot show what the ring dropped"
        );
    }

    /// The buffer fill every draw test below starts from, and **the reason it is not `0`**.
    ///
    /// The panel is `fill_rect(..., 0x0000_0000, PANEL_ALPHA)` — black alpha-blended. Over a black
    /// buffer that is a *no-op*, so a `!= 0` test cannot see the largest thing `draw` paints: a
    /// panel spanning the whole window used to pass containment untouched. `palette.rs:691` looks
    /// like the same test but presses a key first so its **opaque** highlight bar draws; this
    /// module has no opaque element, so the shape alone yields no coverage. Filling with a
    /// distinctive colour and asserting changed-vs-untouched is what makes the panel visible, and
    /// it is the pattern every lens draw test must copy.
    const BG: u32 = 0x0012_3456;

    #[test]
    fn draw_paints_inside_area_only() {
        let (w, h) = (320usize, 224usize);
        let mut buf = vec![BG; w * h];
        let area = Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        let px = 1;
        let t = Ticker {
            lines: vec!["w0 vram $4A00 3F->12 @f811 Sonic_Move+$1C".to_string(); ROWS],
            armed: 2,
            dropped: 7,
        };
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(&mut c, area, px, &t);
        }
        // The panel alone is `panel_w * panel_h` pixels; text could never reach that. If this ever
        // fails, the panel has gone invisible against BG again and every assertion below is blind.
        // `(2 * px).max(4)` is 4 at px 1 — written out, not re-derived from production.
        let margin = 4;
        let panel_w = area.w - 2 * margin;
        let panel_h = (ROWS + 1) * font::LINE_H * px + 2 * (2 * px);
        let painted = buf.iter().filter(|p| **p != BG).count();
        assert!(
            painted >= panel_w * panel_h,
            "the panel left no mark: {painted} changed, panel is {panel_w}x{panel_h}"
        );
        for (i, p) in buf.iter().enumerate() {
            if *p != BG {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
    }

    /// It is a **bottom** strip, hugging the bottom edge of the picture (which is why toasts, which
    /// stack from the same edge, can briefly cover it) — **at every font scale**.
    ///
    /// All four edges by equality against hand-written coordinates, rather than the
    /// `bottom_edge - bottom_row <= margin` inequality this used to carry. That form was doubly
    /// weak: an inequality lets the strip drift *inward* freely, and its bound came from a
    /// test-local copy of the production margin formula, so it moved with the very bug it was
    /// supposed to catch.
    ///
    /// **Three scales, because `margin = (2 * px).max(4)` is 4 at both px 1 and px 2** — the floor
    /// still binds — so hardcoding `margin = 4` was the identity everywhere this module looked, and
    /// shipped green. It first diverges at px 4, where the correct margin is 8. `video.rs` closed
    /// this exact gap in `1407d07`; `watch.rs` and `cpu.rs` never got the same treatment.
    ///
    /// Derivation, once: `margin = (2 * px).max(4)`, `pad = 2 * px`, five rows (four hits plus the
    /// header) of `LINE_H * px` make the panel `44 * px` tall, and it spans the picture's width less
    /// a margin either side.
    #[test]
    fn the_strip_hugs_the_bottom_at_every_scale() {
        let (w, h) = (760usize, 560usize);
        // A non-zero origin, so an anchor that measured from the window corner cannot pass.
        let area = Rect {
            x: 16,
            y: 12,
            w: 700,
            h: 500,
        };
        let t = Ticker {
            lines: vec!["w0 vram $4A00 3F->12 @f811 $00ABCD".to_string(); ROWS],
            armed: 1,
            dropped: 0,
        };
        // (px, top, bottom, left, right) — every number written out by hand.
        for (px, want) in [
            (1usize, (464usize, 507usize, 20usize, 711usize)),
            (2, (420, 507, 20, 711)),
            (4, (328, 503, 24, 707)),
        ] {
            let mut buf = vec![BG; w * h];
            {
                let mut c = font::Canvas::new(&mut buf, w, h);
                draw(&mut c, area, px, &t);
            }
            let got = crate::lens::ink_bounds(&buf, w, BG)
                .unwrap_or_else(|| panic!("px {px}: draw painted nothing"));
            assert_eq!(
                got, want,
                "px {px}: the strip is at (top,bottom,left,right) {got:?}, not {want:?}"
            );
            // The claims the tuple encodes, named so a failure says which one broke. Not how `want`
            // was derived — `want` is hand-written; these are assertions about it.
            let (top, bottom, left, _) = got;
            assert_eq!(
                bottom + 1 + (2 * px).max(4),
                area.y + area.h,
                "px {px}: the bottom gutter is not exactly one margin"
            );
            assert_eq!(
                left,
                area.x + (2 * px).max(4),
                "px {px}: the left gutter is not exactly one margin"
            );
            assert!(
                top >= area.y + area.h / 2,
                "px {px}: ink reached the top half — this is a bottom strip (first ink on row \
                 {top})"
            );
        }
    }

    /// A very long symbol name must be truncated, not allowed to run past the panel — `Canvas`
    /// clips at the buffer edge only, so an unfitted string would paint over the whole window.
    ///
    /// The bound is the **panel's** right edge, not the area's: those differ by `margin`, and text
    /// that stopped at the area edge would still have escaped the panel it is supposed to sit in.
    #[test]
    fn a_long_line_stays_inside_a_narrow_panel() {
        let (w, h) = (200usize, 400usize);
        let mut buf = vec![BG; w * h];
        let area = Rect {
            x: 0,
            y: 0,
            w: 60,
            h: 400,
        };
        let px = 2;
        let t = Ticker {
            lines: vec!["w0 vram $4A00 3F->12 @f811 ".to_string() + &"X".repeat(400)],
            armed: 1,
            dropped: 0,
        };
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(&mut c, area, px, &t);
        }
        // left + panel_w == area.x + margin + (area.w - 2 * margin).
        // px is 2 here, where `(2 * px).max(4)` is still 4.
        let panel_right = area.x + area.w - 4;
        assert!(buf.iter().any(|p| *p != BG), "draw painted nothing");
        for (i, p) in buf.iter().enumerate() {
            if *p != BG {
                assert!(
                    i % w < panel_right,
                    "ink escaped the panel (right edge {panel_right}) at x={}",
                    i % w
                );
            }
        }
    }

    /// An area too short to hold the panel draws **nothing** — and, just as important, does not
    /// underflow computing where the panel's top would be (`area.y + area.h - margin - panel_h` is
    /// `usize` arithmetic; this is the hazard class `draw_narrow_panel_does_not_underflow` pins for
    /// the palette). The area is comfortably **wide** enough, so only the height clause can fire.
    #[test]
    fn a_short_area_draws_nothing_and_does_not_underflow() {
        let (w, h) = (64usize, 64usize);
        let mut buf = vec![BG; w * h];
        let t = Ticker {
            lines: vec!["w0 bus $0100 0->1 @f811 $00ABCD".to_string(); ROWS],
            armed: 1,
            dropped: 0,
        };
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(
                &mut c,
                Rect {
                    x: 0,
                    y: 0,
                    w: 40,
                    h: 8,
                },
                2,
                &t,
            );
        }
        assert!(
            buf.iter().all(|p| *p == BG),
            "a panel was drawn into an area too short to hold it"
        );
    }

    /// The other half of the guard: an area with all the height in the world but too narrow to hold
    /// a legible line draws nothing either. Nothing underflows here — `panel_w` merely saturates to
    /// 0 and `fit` yields nothing — but the guard reads as load-bearing, so it is held to that.
    #[test]
    fn a_narrow_area_draws_nothing() {
        let (w, h) = (64usize, 240usize);
        let mut buf = vec![BG; w * h];
        let t = Ticker {
            lines: vec!["w0 bus $0100 0->1 @f811 $00ABCD".to_string(); ROWS],
            armed: 1,
            dropped: 0,
        };
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(
                &mut c,
                Rect {
                    x: 0,
                    y: 0,
                    w: 20,
                    h: 240,
                },
                2,
                &t,
            );
        }
        assert!(
            buf.iter().all(|p| *p == BG),
            "a panel was drawn into an area too narrow to hold a line"
        );
    }

    /// The header turns amber once the ring has dropped hits: the strip is then knowingly
    /// incomplete, and a reader who cannot see that reads the gap as quiet. The **body** lines stay
    /// `INFO` throughout — the amber means "this list is a sample", so amber on the list itself
    /// would say the opposite. Rendered with a line present so both can be told apart, and asserted
    /// by row order rather than fixed coordinates: the amber must sit *above* the INFO body.
    #[test]
    fn a_dropped_hit_recolours_the_header_but_not_the_lines() {
        let (w, h) = (320usize, 224usize);
        let render = |dropped: u64| {
            let mut buf = vec![BG; w * h];
            let t = Ticker {
                lines: vec!["w0 bus $0100 0->1 @f811 $00ABCD".to_string()],
                armed: 1,
                dropped,
            };
            {
                let mut c = font::Canvas::new(&mut buf, w, h);
                draw(&mut c, Rect { x: 0, y: 0, w, h }, 1, &t);
            }
            buf
        };
        let rows_of = |buf: &[u32], color: u32| -> Vec<usize> {
            buf.iter()
                .enumerate()
                .filter(|(_, p)| **p == color)
                .map(|(i, _)| i / w)
                .collect()
        };
        let clean = render(0);
        assert!(
            rows_of(&clean, ACCENT).is_empty(),
            "nothing is amber while nothing has been dropped"
        );
        assert!(
            !rows_of(&clean, INFO).is_empty(),
            "a clean strip is drawn in INFO"
        );

        let lossy = render(1);
        let amber = rows_of(&lossy, ACCENT);
        let white = rows_of(&lossy, INFO);
        assert!(!amber.is_empty(), "a dropped hit draws the header amber");
        assert!(!white.is_empty(), "and leaves the body line INFO");
        assert!(
            amber.iter().max() < white.iter().min(),
            "the amber must be the header, above the INFO body line (amber rows \
             {:?}..{:?}, INFO rows {:?}..{:?})",
            amber.iter().min(),
            amber.iter().max(),
            white.iter().min(),
            white.iter().max()
        );
    }
}
