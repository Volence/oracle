//! The CPU chip (spec §5.3) — a small top-right readout: PC as a symbol, SR, frame counter.
//! Auto-shows while paused or stepping; `LensId::CpuRegs` expands it to the full D0-D7/A0-A7
//! block. Without a `.lst` the PC is raw hex — the fallback spec §10 names.

use crate::overlay::{self, ACCENT, INFO};
use crate::present::Rect;
use crate::{font, MAX_SYMBOL_DISPLACEMENT};
use oracle_core::m68000::registers::Registers;
use oracle_core::symbols::SymbolTable;

/// What the chip draws: already-formatted lines, top to bottom, plus why it is on screen.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Chip {
    pub lines: Vec<String>,
    /// Drawn in the paused colour when the machine is stopped, so the chip says *why* it is
    /// showing when it appeared on its own.
    pub paused: bool,
}

/// Compact three lines — `PC`, `SR`, frame — or eleven when `expanded` adds the register file.
///
/// The PC is symbolised through the same `resolve_within(_, MAX_SYMBOL_DISPLACEMENT)` the watch
/// log and the ticker use, and falls back to raw hex: a name 4 KiB past its symbol would be
/// actively misleading, and so would a chip that quietly stopped resolving at all.
///
/// A0-A7 go through [`Registers::addr_reg`], never `regs.a[i]`: the register file's `a` array is
/// **7 wide** (A7 lives in `usp`/`ssp`, and which one is live depends on the supervisor bit), so
/// indexing it with 7 would panic — and printing `usp` unconditionally would be wrong in
/// supervisor mode, which is most of the time in a booting Genesis game.
pub fn model(
    regs: &Registers,
    symbols: Option<&SymbolTable>,
    frame: u64,
    paused: bool,
    expanded: bool,
) -> Chip {
    let pc = symbols
        .and_then(|t| t.resolve_within(regs.pc, MAX_SYMBOL_DISPLACEMENT))
        .map(|r| r.to_string())
        .unwrap_or_else(|| format!("${:06X}", regs.pc));
    let mut lines = vec![
        format!("PC {pc}"),
        format!(
            "SR ${:04X} {}{}",
            regs.sr,
            if regs.supervisor() { "S" } else { "U" },
            regs.int_mask()
        ),
        format!("F {frame}"),
    ];
    if expanded {
        // Two registers to a line: sixteen single-register rows would be taller than the picture
        // at any font scale above 1.
        for i in 0..4 {
            lines.push(format!(
                "D{} {:08X}  D{} {:08X}",
                i,
                regs.d[i],
                i + 4,
                regs.d[i + 4]
            ));
        }
        for i in 0..4 {
            lines.push(format!(
                "A{} {:08X}  A{} {:08X}",
                i,
                regs.addr_reg(i),
                i + 4,
                regs.addr_reg(i + 4)
            ));
        }
    }
    Chip { lines, paused }
}

/// Top-right of `area`, sized to its widest line so the expanded block does not shove the compact
/// one around — and clamped to the picture, so a long symbol name cannot push the panel out into
/// the letterbox.
pub fn draw(c: &mut font::Canvas, area: Rect, px: usize, chip: &Chip) {
    let pad = 2 * px;
    let margin = (2 * px).max(4);
    let line_h = font::LINE_H * px;
    let panel_h = chip.lines.len() * line_h + 2 * pad;
    let widest = chip
        .lines
        .iter()
        .map(|l| font::text_width(l) * px)
        .max()
        .unwrap_or(0);
    let panel_w = (widest + 2 * pad).min(area.w.saturating_sub(2 * margin));
    // Too small to say anything honestly — and the `panel_w == 0` clause is also what makes the
    // `left` below safe. An `area.w` under `2 * margin` saturates `panel_w` to 0 and returns here;
    // past that point `panel_w <= area.w - 2 * margin`, so `area.w - margin - panel_w >= margin`
    // and the `usize` arithmetic cannot underflow (the
    // `draw_narrow_panel_does_not_underflow` hazard class).
    if area.w < 16 * px || panel_w == 0 || area.h < panel_h + margin {
        return;
    }
    let left = (area.x + area.w - margin - panel_w) as i32;
    let top = (area.y + margin) as i32;
    c.fill_rect(left, top, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);

    let avail = panel_w.saturating_sub(2 * pad);
    // Amber while stopped: the chip shows itself when the machine pauses, so it has to say so.
    let color = if chip.paused { ACCENT } else { INFO };
    for (i, l) in chip.lines.iter().enumerate() {
        c.text(
            left + pad as i32,
            top + pad as i32 + (i * line_h) as i32,
            px,
            color,
            overlay::fit(l, avail, px),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Registers` has no `Default` (it derives `Clone, Debug, PartialEq, Eq` and the bincode
    /// pair only), so the fixture is an explicit literal — the same shape `registers.rs`'s own
    /// tests use. `usp` and `ssp` are deliberately *different* and both recognisable, because the
    /// whole point of `addr_reg(7)` is choosing between them.
    fn regs() -> Registers {
        Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x0000_0BAD,
            ssp: 0x00FF_F000,
            pc: 0x00_1234,
            sr: 0x2700, // supervisor, interrupt mask 7
            prefetch: [0; 2],
        }
    }

    /// A one-symbol listing covering `$1200..`, so `regs()`'s `pc = $1234` resolves to
    /// `Sonic_Move+$34` and a nearby-but-too-far address does not.
    fn table() -> SymbolTable {
        SymbolTable::parse(
            "  Symbol Table (* = unused):\n  --------------------------\n\n \
             Sonic_Move : 1200 C |\n\n    1 symbols\n    0 unused symbols\n",
        )
        .expect("fixture parses")
    }

    #[test]
    fn without_symbols_the_pc_is_raw_hex() {
        let c = model(&regs(), None, 42, false, false);
        assert_eq!(c.lines.len(), 3, "compact is three lines");
        assert_eq!(c.lines[0], "PC $001234");
        assert_eq!(c.lines[1], "SR $2700 S7", "supervisor, mask 7");
        assert_eq!(c.lines[2], "F 42", "the frame counter is shown");
        assert!(!c.paused);
    }

    /// The reason the lens takes a `SymbolTable` at all: with a `.lst` loaded the PC reads as a
    /// name, through the same `resolve_within(_, MAX_SYMBOL_DISPLACEMENT)` the watch log uses.
    /// Without this the whole symbol path could be deleted and only the hex fallback test above
    /// would notice — which it would not, because it passes `None`.
    #[test]
    fn with_symbols_the_pc_reads_as_a_name_and_displacement() {
        let t = table();
        let c = model(&regs(), Some(&t), 0, false, false);
        assert_eq!(c.lines[0], "PC Sonic_Move+$34");

        // Past MAX_SYMBOL_DISPLACEMENT the name would be actively misleading, so it falls back to
        // hex rather than claiming a symbol 4 KiB away.
        let mut far = regs();
        far.pc = 0x1200 + MAX_SYMBOL_DISPLACEMENT + 1;
        let c = model(&far, Some(&t), 0, false, false);
        assert_eq!(c.lines[0], format!("PC ${:06X}", far.pc));
    }

    /// The SR line must follow the SR, not a constant: user mode with a different mask reads
    /// differently on both halves.
    #[test]
    fn the_sr_line_follows_the_supervisor_bit_and_the_interrupt_mask() {
        let mut r = regs();
        r.sr = 0x0300; // S clear, mask 3
        let c = model(&r, None, 0, false, false);
        assert_eq!(c.lines[1], "SR $0300 U3");
    }

    /// A7 must come from `addr_reg(7)`, which picks ssp/usp by the supervisor bit — `regs.a[7]`
    /// would panic (the array is 7 wide), and printing `usp` unconditionally would be wrong in
    /// supervisor mode. **Both** modes are pinned: a hardcoded `ssp` passes the supervisor half on
    /// its own, which is exactly the half-right bug the accessor exists to prevent.
    #[test]
    fn expanded_shows_all_sixteen_registers_and_a7_follows_the_supervisor_bit() {
        let mut r = regs();
        r.d[0] = 0xDEAD_BEEF;
        r.d[7] = 0x0000_0007;
        r.a[0] = 0x00C0_FFEE;
        r.a[6] = 0x0000_00A6;
        let c = model(&r, None, 0, false, true);
        assert_eq!(c.lines.len(), 11, "3 + 4 D-lines + 4 A-lines");
        let joined = c.lines.join("\n");
        for name in ["D0", "D1", "D2", "D3", "D4", "D5", "D6", "D7"] {
            assert!(joined.contains(name), "{name} missing");
        }
        for name in ["A0", "A1", "A2", "A3", "A4", "A5", "A6", "A7"] {
            assert!(joined.contains(name), "{name} missing");
        }
        assert!(joined.contains("DEADBEEF"), "D0's value is shown");
        assert!(joined.contains("00000007"), "D7's value is shown");
        assert!(joined.contains("00C0FFEE"), "A0's value is shown");
        assert!(joined.contains("000000A6"), "A6's value is shown");
        assert!(
            joined.contains("00FFF000"),
            "A7 is the SSP in supervisor mode: {joined}"
        );
        assert!(
            !joined.contains("00000BAD"),
            "the USP is not A7 in supervisor mode: {joined}"
        );

        // The other half. Same registers, S clear: A7 must swap to the USP.
        r.sr = 0x0000;
        let c = model(&r, None, 0, false, true);
        let joined = c.lines.join("\n");
        assert!(
            joined.contains("00000BAD"),
            "A7 is the USP in user mode: {joined}"
        );
        assert!(
            !joined.contains("00FFF000"),
            "the SSP is not A7 in user mode: {joined}"
        );
    }

    /// The buffer fill every draw test below starts from, and **the reason it is not `0`**: the
    /// panel is `fill_rect(..., 0x0000_0000, PANEL_ALPHA)`, black alpha-blended, which over a black
    /// buffer is a *no-op*. A `!= 0` test cannot see the largest thing `draw` paints. Same reason,
    /// same constant, as `lens/watch.rs`.
    const BG: u32 = 0x0012_3456;

    /// The house margin idiom, re-derived so the assertions below can name the panel's own edges
    /// rather than the area's — the two differ by exactly this, and that difference is the bug
    /// class (ink in the letterbox) these tests exist to catch.
    fn margin_of(px: usize) -> usize {
        (2 * px).max(4)
    }

    /// Render `chip` into a `w * h` buffer over [`BG`] and hand back the buffer.
    fn render(w: usize, h: usize, area: Rect, px: usize, chip: &Chip) -> Vec<u32> {
        let mut buf = vec![BG; w * h];
        {
            let mut c = font::Canvas::new(&mut buf, w, h);
            draw(&mut c, area, px, chip);
        }
        buf
    }

    /// The (row, column) extremes of everything `draw` changed.
    fn ink_bounds(buf: &[u32], w: usize) -> Option<(usize, usize, usize, usize)> {
        let mut b: Option<(usize, usize, usize, usize)> = None;
        for (i, p) in buf.iter().enumerate() {
            if *p == BG {
                continue;
            }
            let (x, y) = (i % w, i / w);
            b = Some(match b {
                None => (y, y, x, x),
                Some((t, bo, l, r)) => (t.min(y), bo.max(y), l.min(x), r.max(x)),
            });
        }
        b
    }

    #[test]
    fn draw_paints_inside_area_only() {
        let (w, h) = (320usize, 224usize);
        let area = Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        let px = 1;
        let chip = model(&regs(), None, 7, true, true);
        let buf = render(w, h, area, px, &chip);

        // The panel alone is `panel_w * panel_h` pixels; text could never reach that. If this ever
        // fails, the panel has gone invisible against BG again and every assertion below is blind.
        let pad = 2 * px;
        let panel_h = chip.lines.len() * font::LINE_H * px + 2 * pad;
        let widest = chip
            .lines
            .iter()
            .map(|l| font::text_width(l) * px)
            .max()
            .expect("the chip always has lines");
        let panel_w = (widest + 2 * pad).min(area.w - 2 * margin_of(px));
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

    /// It is a **top-right** chip. Containment alone cannot tell that from a bottom-left one — the
    /// watch ticker's own tests passed for a while with the strip anchored to the wrong edge — so
    /// both axes are pinned from both sides: hard against the top-right corner (within one margin)
    /// and nowhere near the opposite half of the picture.
    #[test]
    fn the_chip_hugs_the_top_right_of_the_area() {
        let (w, h) = (320usize, 224usize);
        let area = Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        let px = 1;
        let margin = margin_of(px);
        let chip = model(&regs(), None, 7, false, false);
        let buf = render(w, h, area, px, &chip);
        let (top, bottom, left, right) = ink_bounds(&buf, w).expect("draw painted nothing");

        assert!(
            top - area.y <= margin,
            "not anchored to the top: first ink on row {top}, area starts at {}",
            area.y
        );
        assert!(
            (area.x + area.w) - (right + 1) <= margin,
            "not anchored to the right: last ink at column {right}, area ends at {}",
            area.x + area.w
        );
        assert!(
            bottom < area.y + area.h / 2,
            "ink reached the bottom half — this is a top chip, not a bottom one (last ink on row \
             {bottom})"
        );
        assert!(
            left >= area.x + area.w / 2,
            "ink reached the left half — this is a right-hand chip (first ink at column {left})"
        );
    }

    /// `CpuRegs` has to *do* something visible: the expanded block is eight rows taller than the
    /// compact chip and grows downward from the same top edge. A `draw` that ignored the extra
    /// lines, or a `model` that never added them, would still pass every containment test above.
    #[test]
    fn the_expanded_chip_is_taller_than_the_compact_one_from_the_same_top() {
        let (w, h) = (320usize, 224usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 320,
            h: 224,
        };
        let px = 1;
        let compact = render(w, h, area, px, &model(&regs(), None, 7, false, false));
        let expanded = render(w, h, area, px, &model(&regs(), None, 7, false, true));
        let (ct, cb, _, _) = ink_bounds(&compact, w).expect("the compact chip painted nothing");
        let (et, eb, _, _) = ink_bounds(&expanded, w).expect("the expanded chip painted nothing");
        assert_eq!(ct, et, "both forms hang from the same top edge");
        assert!(
            eb > cb + 7 * font::LINE_H * px,
            "the expanded block is not eight rows taller (compact ends {cb}, expanded {eb})"
        );
    }

    /// A very long symbol name must be truncated, not allowed to run past the panel — `Canvas`
    /// clips at the buffer edge only, so an unfitted string would paint over the whole window. The
    /// bound is the **panel's** edge, not the area's: the two differ by `margin`, and text that
    /// stopped at the area edge would still have escaped the panel it is supposed to sit in.
    #[test]
    fn a_long_line_stays_inside_a_narrow_panel() {
        let (w, h) = (400usize, 200usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 200,
        };
        let px = 2;
        let chip = Chip {
            lines: vec!["PC ".to_string() + &"X".repeat(400)],
            paused: false,
        };
        let buf = render(w, h, area, px, &chip);
        let margin = margin_of(px);
        let (_, _, left, right) = ink_bounds(&buf, w).expect("draw painted nothing");
        assert!(
            left >= area.x + margin,
            "ink escaped the panel's left edge ({}) at x={left}",
            area.x + margin
        );
        assert!(
            right < area.x + area.w - margin,
            "ink escaped the panel's right edge ({}) at x={right}",
            area.x + area.w - margin
        );
    }

    /// An area too short to hold the panel draws **nothing**. The area is comfortably *wide*
    /// enough, so only the height clause can fire.
    #[test]
    fn a_short_area_draws_nothing() {
        let (w, h) = (200usize, 200usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 160,
            h: 12,
        };
        let chip = model(&regs(), None, 7, false, true);
        let buf = render(w, h, area, 2, &chip);
        assert!(
            buf.iter().all(|p| *p == BG),
            "a panel was drawn into an area too short to hold it"
        );
    }

    /// The other half of the guard: all the height in the world but too narrow to hold a legible
    /// line draws nothing either, rather than a sliver of panel with no text in it.
    #[test]
    fn a_narrow_area_draws_nothing() {
        let (w, h) = (200usize, 240usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 240,
        };
        let chip = model(&regs(), None, 7, false, false);
        let buf = render(w, h, area, 2, &chip);
        assert!(
            buf.iter().all(|p| *p == BG),
            "a panel was drawn into an area too narrow to hold a line"
        );
    }

    /// The chip auto-shows while paused, so it has to say *why* it appeared: amber while stopped,
    /// white while running. Without this the `paused` flag could be threaded all the way through
    /// the model and then dropped on the floor by `draw`.
    #[test]
    fn a_paused_machine_draws_the_chip_amber() {
        let (w, h) = (320usize, 224usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 320,
            h: 224,
        };
        let has = |buf: &[u32], c: u32| buf.contains(&c);

        let running = render(w, h, area, 1, &model(&regs(), None, 7, false, false));
        assert!(has(&running, INFO), "a running chip is drawn in INFO");
        assert!(!has(&running, ACCENT), "and nothing about it is amber");

        let stopped = render(w, h, area, 1, &model(&regs(), None, 7, true, false));
        assert!(has(&stopped, ACCENT), "a paused chip is drawn amber");
        assert!(!has(&stopped, INFO), "and none of it stays INFO");
    }
}
