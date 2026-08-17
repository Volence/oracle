//! The CPU chip (spec §5.3) — a small top-right readout: PC as a symbol, SR, frame counter.
//! Auto-shows while paused or stepping; `LensId::CpuRegs` expands it to the full D0-D7/A0-A7
//! block. Without a `.lst` the PC is raw hex — the fallback spec §10 names.

use crate::overlay::{self, ACCENT, INFO};
use crate::present::Rect;
use crate::{font, MAX_SYMBOL_DISPLACEMENT};
use oracle_core::m68000::registers::Registers;
use oracle_core::symbols::SymbolTable;
use std::borrow::Cow;

/// The compact head — `PC`, `SR`, frame. Everything [`model`] appends past it is the expanded
/// register block, and [`form`] degrades to exactly this head when the block will not fit.
const COMPACT_LINES: usize = 3;

/// The chip's content width, in **glyph columns**: the register block's own line format,
/// `D0 D0000000  D4 D4000004`, is exactly 24 glyphs wide.
///
/// **Not the widest line.** That is what shipped first, and on the owner's screen it made the chip
/// eat 41% of the picture: the PC line carries a symbol name of unbounded length — their ROM
/// produces `GAMESTATE_OJZSCROLL.UPDATE.SKIP_ENTITY_SCAN+$…`, some fifty glyphs — so the panel grew
/// to fit it, clamped to the full width of the picture, and drew straight over the CRAM strip. The
/// truncation logic was never wrong; it was simply never asked, because the panel expanded before
/// [`overlay::fit`] could shrink anything.
///
/// **The same constant in both forms**, compact and expanded, for two reasons. Toggling `cpu_regs`
/// then changes the panel's height and nothing else, instead of making it jump width — the same
/// don't-shove-things-around rule the panel was already sized by. And the compact chip is the one
/// whose whole purpose is the PC (spec §5.3), so sizing it to `SR $2700 S7` would leave the symbol
/// five readable characters.
const CHIP_COLUMNS: usize = 24;

/// [`CHIP_COLUMNS`] in device pixels of ink. `n` glyphs are `ADVANCE * n - 1` wide: the trailing
/// inter-glyph gap is not ink.
fn chip_width(px: usize) -> usize {
    (CHIP_COLUMNS * font::ADVANCE - 1) * px
}

/// The leading-truncation marker. Three full stops rather than `…`, which the 5x7 font has no
/// glyph for — it would draw as the missing-glyph box (`font.rs:26`).
const ELLIPSIS: &str = "...";

/// Fit `text` into `avail` device pixels by dropping characters from the **front**.
///
/// The opposite of [`overlay::fit`], and only the PC line uses it. When you are watching the PC
/// move, the leading module path is the least informative part of a symbol: every frame it reads
/// `GAMESTATE_OJZSCROLL.UPDATE.…`, and the part that actually changes is the tail. Head-truncation
/// shows you the half that never moves.
///
/// Borrowed and untouched when the whole thing already fits, so the common short-symbol and
/// no-symbol-table cases allocate nothing.
fn fit_tail(text: &str, avail: usize, px: usize) -> Cow<'_, str> {
    if font::text_width(text) * px <= avail {
        return Cow::Borrowed(text);
    }
    // Walk in from the end, keeping the longest suffix that still leaves room for the marker.
    let marker = ELLIPSIS.chars().count();
    let mut start = text.len();
    for (i, _) in text.char_indices().rev() {
        let glyphs = marker + text[i..].chars().count();
        if (font::ADVANCE * glyphs - 1) * px > avail {
            break;
        }
        start = i;
    }
    if start == text.len() {
        // Not even the marker plus one character fits. Say "there is more here" rather than
        // showing a single arbitrary character as if it were the whole answer.
        return Cow::Borrowed(overlay::fit(ELLIPSIS, avail, px));
    }
    Cow::Owned(format!("{ELLIPSIS}{}", &text[start..]))
}

/// The font scale a chip of `n` lines draws at: **the expanded block drops one step.**
///
/// Eleven lines at the owner's `px = 3` stand 276 device pixels tall, which is 41% of a 672-pixel
/// picture before the width is even counted; one scale down that is 184. Dense data at smaller type
/// is ordinary for a debugger, and it is the axis with room to give — the alternative of four
/// registers to a line spends 850 device pixels of width to save height, and width is the scarcer
/// of the two here.
///
/// The compact head keeps the full scale: it is three lines, it costs nothing, and it is what the
/// chip shows when it appears on its own at a pause.
fn scale_for(n: usize, px: usize) -> usize {
    if n > COMPACT_LINES {
        px.saturating_sub(1).max(1)
    } else {
        px
    }
}

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

/// The width the fixed-width lines need in device pixels.
///
/// Line 0 — the PC — is **exempt**: a symbol name has no bound, so truncating it is expected, and
/// `Sonic_Move_Some…` still reads as an approximation of where you are. Every other line is
/// fixed-width, and there truncation is a different thing entirely: `D0 DEADBE` is not a partial
/// answer, it is a plausible-looking wrong 32-bit value. So the register block is only ever drawn
/// at a width that holds it whole.
fn fixed_width(lines: &[String], px: usize) -> usize {
    lines
        .iter()
        .skip(1)
        .map(|l| font::text_width(l) * px)
        .max()
        .unwrap_or(0)
}

/// How many of `lines` to draw: all of them, the compact head, or none.
///
/// **Turning on more information must never remove all of it.** At `px = 1` the expanded block
/// needs 96 rows where the compact chip needs 32, so on a picture between the two, bailing outright
/// would make switching `cpu_regs` *on* hide the chip — and, because the chip auto-shows when the
/// machine stops, it would take the paused readout with it. The same reasoning covers width, where
/// bailing would instead have meant truncated register values. So the form degrades one step, to
/// the three lines that always fit if anything does, and only then gives up.
fn form(lines: &[String], area: Rect, px: usize) -> usize {
    let fits = |n: usize| {
        // Each candidate form is measured at the scale it would actually be drawn at, or the
        // expanded block would be rejected for a height it never asks for.
        let px = scale_for(n, px);
        let pad = 2 * px;
        let margin = (2 * px).max(4);
        n > 0
            && n * font::LINE_H * px + 2 * pad + margin <= area.h
            // `fixed_width`, not `chip_width`: this clause exists to stop a register value being
            // truncated into a plausible-looking wrong number, and that is a question about the
            // lines themselves. A picture too narrow for `chip_width` is fine — the panel simply
            // comes out narrower and the PC's tail gets shorter.
            && fixed_width(&lines[..n], px) + 2 * pad + 2 * margin <= area.w
    };
    let head = COMPACT_LINES.min(lines.len());
    if fits(lines.len()) {
        lines.len()
    } else if fits(head) {
        head
    } else {
        0
    }
}

/// Top-right of `area`, sized to its widest line so the expanded block does not shove the compact
/// one around — and clamped to the picture, so a long symbol name cannot push the panel out into
/// the letterbox.
pub fn draw(c: &mut font::Canvas, area: Rect, px: usize, chip: &Chip) {
    let n = form(&chip.lines, area, px);
    if n == 0 {
        return;
    }
    let px = scale_for(n, px);
    let lines = &chip.lines[..n];
    let pad = 2 * px;
    let margin = (2 * px).max(4);
    let line_h = font::LINE_H * px;
    let panel_h = n * line_h + 2 * pad;
    // **Sized to [`CHIP_COLUMNS`], never to the widest line** — the PC's symbol name is unbounded,
    // and letting it set the width is what made the chip eat the picture. `fixed_width` still has a
    // say so that a hypothetical over-wide fixed line (a 20-digit frame counter) is not truncated
    // into a wrong number; today it never wins, and the test above pins that.
    //
    // `form` returning non-zero is what makes both of these safe: it guarantees
    // `2 * pad + 2 * margin <= area.w`, so the subtraction below cannot underflow, and
    // `panel_w <= area.w - 2 * margin`, so `area.w - margin - panel_w >= margin` (the
    // `draw_narrow_panel_does_not_underflow` hazard class).
    let content = chip_width(px).max(fixed_width(lines, px));
    let panel_w = (content + 2 * pad).min(area.w - 2 * margin);
    let left = (area.x + area.w - margin - panel_w) as i32;
    let top = (area.y + margin) as i32;
    c.fill_rect(left, top, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);

    let avail = panel_w.saturating_sub(2 * pad);
    // Amber while stopped: the chip shows itself when the machine pauses, so it has to say so.
    let color = if chip.paused { ACCENT } else { INFO };
    for (i, l) in lines.iter().enumerate() {
        // Line 0 is the PC and truncates from the **front**; every other line either fits by
        // construction or is short enough that it does.
        let text = if i == 0 {
            fit_tail(l, avail, px)
        } else {
            Cow::Borrowed(overlay::fit(l, avail, px))
        };
        c.text(
            left + pad as i32,
            top + pad as i32 + (i * line_h) as i32,
            px,
            color,
            &text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::ink_bounds;

    /// `Registers` has no `Default` (it derives `Clone, Debug, PartialEq, Eq` and the bincode
    /// pair only), so the fixture is an explicit literal — the same shape `registers.rs`'s own
    /// tests use. `usp` and `ssp` are deliberately *different* and both recognisable, because the
    /// whole point of `addr_reg(7)` is choosing between them.
    ///
    /// **Every register holds a distinct value that names itself** (`D5` holds `D5000005`). That is
    /// not decoration: with sixteen zeros, or with two registers sharing a value, a `model` that
    /// paired every label with the *wrong* register's value would print a set of lines containing
    /// all the right numbers and all the right names, and no assertion over either set alone could
    /// tell. The pairing is the thing under test.
    fn regs() -> Registers {
        Registers {
            d: [
                0xD000_0000,
                0xD100_0001,
                0xD200_0002,
                0xD300_0003,
                0xD400_0004,
                0xD500_0005,
                0xD600_0006,
                0xD700_0007,
            ],
            a: [
                0xA000_0000,
                0xA100_0001,
                0xA200_0002,
                0xA300_0003,
                0xA400_0004,
                0xA500_0005,
                0xA600_0006,
            ],
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
        assert_eq!(c.lines.len(), COMPACT_LINES, "compact is three lines");
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

    /// Every register printed **against its own value**, asserted line by whole line.
    ///
    /// Membership assertions (`joined.contains("D0")`, `joined.contains("DEADBEEF")`) check the
    /// labels as a set and the values as a set and never check the *pairing*, so a `model` that
    /// printed `D0`'s label beside `D4`'s value — every register wrong under the right name, the
    /// worst output a debugger readout can produce — passed all of them. Whole lines are the only
    /// form that pins it, and they subsume the label, value and ordering checks besides.
    ///
    /// A7 comes from `addr_reg(7)`, which picks ssp/usp by the supervisor bit: `regs.a[7]` would
    /// panic (the array is 7 wide), and printing `usp` unconditionally would be wrong in supervisor
    /// mode. **Both** modes are pinned — a hardcoded `ssp` passes the supervisor half on its own,
    /// which is exactly the half-right bug the accessor exists to prevent.
    #[test]
    fn expanded_shows_all_sixteen_registers_and_a7_follows_the_supervisor_bit() {
        let mut r = regs();
        let block = |c: &Chip| -> Vec<String> { c.lines[COMPACT_LINES..].to_vec() };

        let c = model(&r, None, 0, false, true);
        assert_eq!(
            c.lines.len(),
            COMPACT_LINES + 8,
            "3 + 4 D-lines + 4 A-lines"
        );
        assert_eq!(
            block(&c),
            [
                "D0 D0000000  D4 D4000004",
                "D1 D1000001  D5 D5000005",
                "D2 D2000002  D6 D6000006",
                "D3 D3000003  D7 D7000007",
                "A0 A0000000  A4 A4000004",
                "A1 A1000001  A5 A5000005",
                "A2 A2000002  A6 A6000006",
                "A3 A3000003  A7 00FFF000", // supervisor: A7 is the SSP
            ]
        );
        assert!(
            !c.lines.iter().any(|l| l.contains("00000BAD")),
            "the USP leaked into supervisor-mode output: {:?}",
            c.lines
        );

        // The other half. Same registers, S clear: A7 — and only A7 — swaps to the USP.
        r.sr = 0x0000;
        let c = model(&r, None, 0, false, true);
        assert_eq!(
            block(&c),
            [
                "D0 D0000000  D4 D4000004",
                "D1 D1000001  D5 D5000005",
                "D2 D2000002  D6 D6000006",
                "D3 D3000003  D7 D7000007",
                "A0 A0000000  A4 A4000004",
                "A1 A1000001  A5 A5000005",
                "A2 A2000002  A6 A6000006",
                "A3 A3000003  A7 00000BAD", // user: A7 is the USP
            ]
        );
        assert!(
            !c.lines.iter().any(|l| l.contains("00FFF000")),
            "the SSP leaked into user-mode output: {:?}",
            c.lines
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

    /// The degradation ladder, pinned at the layer that decides it. At `px = 1` the compact chip
    /// needs 32 rows and 77 columns; the register block needs 96 and 155. Between the two, the
    /// chip must **shrink, never vanish** — a lens toggle that removes the readout is the opposite
    /// of what turning it on asked for, and it would take the paused auto-show down with it.
    #[test]
    fn the_form_degrades_to_the_compact_head_rather_than_vanishing() {
        let px = 1;
        let lines = model(&regs(), None, 7, false, true).lines;
        assert_eq!(lines.len(), COMPACT_LINES + 8);
        let at = |w: usize, h: usize| form(&lines, Rect { x: 0, y: 0, w, h }, px);

        assert_eq!(at(320, 224), lines.len(), "a whole picture holds the block");
        assert_eq!(
            at(320, 60),
            COMPACT_LINES,
            "too short for the block is not too short for the chip"
        );
        assert_eq!(
            at(120, 224),
            COMPACT_LINES,
            "too narrow for the block is not too narrow for the chip"
        );
        assert_eq!(
            at(120, 60),
            COMPACT_LINES,
            "short and narrow, still the chip"
        );
        assert_eq!(at(320, 12), 0, "too short for even the compact chip");
        assert_eq!(at(40, 224), 0, "too narrow for even the compact chip");
    }

    /// The same ladder in pixels, because `form` being right is no use if `draw` ignores it: on a
    /// picture too small for the register block the chip still appears, and appears as exactly the
    /// three compact lines.
    #[test]
    fn a_picture_too_small_for_the_register_block_still_draws_the_chip() {
        let (w, h) = (400usize, 240usize);
        let px = 1;
        let chip = model(&regs(), None, 7, true, true);
        let compact_h = COMPACT_LINES * font::LINE_H * px + 2 * (2 * px);
        for (label, area) in [
            (
                "too short for the block",
                Rect {
                    x: 0,
                    y: 0,
                    w: 320,
                    h: 60,
                },
            ),
            (
                "too narrow for the block",
                Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 224,
                },
            ),
        ] {
            let buf = render(w, h, area, px, &chip);
            let (top, bottom, _, _) = ink_bounds(&buf, w, BG)
                .unwrap_or_else(|| panic!("{label}: the chip vanished instead of degrading"));
            assert_eq!(
                bottom - top + 1,
                compact_h,
                "{label}: drew something other than the compact three lines"
            );
        }
    }

    /// When the register block *is* drawn, it is drawn whole. `overlay::fit` truncating
    /// `D0 D0000000` to `D0 D00000` would not be a partial answer, it would be a plausible-looking
    /// wrong 32-bit value — which is why width, not just height, chooses the form.
    ///
    /// `REGISTER_LINE` is written out rather than derived from `fixed_width`: a bound computed from
    /// the function under test moves with the bug and cannot catch it.
    ///
    /// Swept across **every** picture width rather than probed at one, which is the difference
    /// between catching this and not. Checked at the single width where the block exactly fits,
    /// a `fixed_width` stubbed to 0 still drew a wide-enough panel and the test passed against
    /// precisely the bug it was written for; the truncation only appears at the widths *below*
    /// that, which are the ones a boundary probe never visits.
    #[test]
    fn the_register_block_is_never_drawn_truncated() {
        let px = 1;
        let pad = 2 * px;
        let (bw, bh) = (280usize, 240usize);
        let chip = model(&regs(), None, 7, false, true);
        // `D0 D0000000  D4 D4000004` — 24 glyphs.
        assert_eq!(chip.lines[COMPACT_LINES].chars().count(), 24);
        const REGISTER_LINE: usize = 24 * font::ADVANCE - 1; // 143 device px at px = 1

        let mut widths_showing_the_block = 0;
        for w in 0..bw - 40 {
            let area = Rect {
                x: 0,
                y: 0,
                w,
                h: 224,
            };
            if form(&chip.lines, area, px) != chip.lines.len() {
                continue; // degraded to the compact head, or absent: nothing to check here
            }
            widths_showing_the_block += 1;
            let buf = render(bw, bh, area, px, &chip);
            let (_, _, left, right) =
                ink_bounds(&buf, bw, BG).expect("the block should have drawn");
            let panel_w = right - left + 1;
            assert!(
                panel_w >= REGISTER_LINE + 2 * pad,
                "at picture width {w} the block drew into a {panel_w}px panel, too narrow to hold \
                 a whole register line ({})",
                REGISTER_LINE + 2 * pad
            );
        }
        assert!(
            widths_showing_the_block > 0,
            "the sweep never drew the block at all, so it checked nothing"
        );
    }

    /// It is a **top-right** chip. Containment alone cannot tell that from a bottom-left one — the
    /// watch ticker's own tests passed for a while with the strip anchored to the wrong edge — so
    /// both axes are pinned from both sides: exactly one margin's gutter from the top-right corner,
    /// and nowhere near the opposite half of the picture.
    ///
    /// The gutters are pinned by **equality**, not `<= margin`: zero satisfies `<=`, so the looser
    /// form let `top = area.y` through, and a chip flush against the picture's edge is the
    /// untidiness anchoring to the picture was meant to avoid.
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
        let (top, bottom, left, right) = ink_bounds(&buf, w, BG).expect("draw painted nothing");

        assert_eq!(
            top,
            area.y + margin,
            "the top gutter is not exactly one margin (first ink on row {top})"
        );
        assert_eq!(
            right + 1,
            area.x + area.w - margin,
            "the right gutter is not exactly one margin (last ink at column {right})"
        );
        assert!(
            bottom < area.y + area.h / 2,
            "ink reached the bottom half — this is a top chip, not a bottom one (last ink on row \
             {bottom})"
        );
        // **Position and width together, by equality.** This used to read
        // `left >= area.x + area.w / 2` — a width claim wearing an anchoring costume, and one the
        // right-gutter equality above already implies the useful half of. It is replaced rather
        // than dropped, and by something strictly stronger: the panel's left edge and its width are
        // both pinned to the pixel, so a chip that grew, shrank or slid fails here.
        //
        // 147 is `CHIP_COLUMNS` at `px = 1`: 24 glyphs of 6 px less the trailing gap, plus 2 px of
        // pad either side. Written out, never computed from `chip_width`.
        const PANEL_W_PX1: usize = 147;
        assert_eq!(
            right - left + 1,
            PANEL_W_PX1,
            "the panel is not {PANEL_W_PX1} px wide (it runs {left}..={right})"
        );
        assert_eq!(
            left,
            area.x + area.w - margin - PANEL_W_PX1,
            "the panel's left edge is not one margin plus its own width in from the right"
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
        let (ct, cb, _, _) = ink_bounds(&compact, w, BG).expect("the compact chip painted nothing");
        let (et, eb, _, _) =
            ink_bounds(&expanded, w, BG).expect("the expanded chip painted nothing");
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
        let (_, _, left, right) = ink_bounds(&buf, w, BG).expect("draw painted nothing");
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

    // --- The chip stops eating the picture -----------------------------------------------------

    /// A symbol from the owner's ROM, near enough — long enough that no sane panel holds it whole.
    const LONG_PC: &str = "GAMESTATE.SKIP_ENTITY_SCAN+$4";

    /// **The truncation keeps the end, not the beginning.** When you are watching the PC move, the
    /// leading module path is the half that never changes; head-truncation shows you exactly that
    /// half and hides the part that does.
    ///
    /// Exact strings, not "contains" or "is shorter than": a `fit_tail` that truncated from the end
    /// like [`overlay::fit`] produces a string of the right length, inside the right width, made of
    /// characters from the right input — and is the bug. The two widths differ by one glyph, so the
    /// boundary is pinned rather than merely landed in, and the `px = 2` row proves the scale is
    /// applied to the budget rather than ignored.
    #[test]
    fn fit_tail_keeps_the_end_of_the_symbol_not_the_beginning() {
        // A glyph is 6 px of advance and the trailing gap is not ink, so `n` glyphs need
        // `6n - 1` px: 17 glyphs need 101, 18 need 107. Written out, never computed from `font`.
        assert_eq!(
            fit_tail(LONG_PC, 101, 1),
            "...ENTITY_SCAN+$4",
            "17 glyphs: the marker plus the last fourteen characters"
        );
        assert_eq!(
            fit_tail(LONG_PC, 106, 1),
            "...ENTITY_SCAN+$4",
            "one px short of the eighteenth glyph, so still seventeen"
        );
        assert_eq!(
            fit_tail(LONG_PC, 107, 1),
            "..._ENTITY_SCAN+$4",
            "eighteen glyphs fit at 107"
        );
        assert_eq!(
            fit_tail(LONG_PC, 202, 2),
            "...ENTITY_SCAN+$4",
            "px 2 doubles the cost of every glyph, so 202 buys the same seventeen"
        );

        // Anything that already fits is handed back untouched and unmarked.
        assert_eq!(fit_tail("PC $00FF00", 1000, 1), "PC $00FF00");
        assert!(
            matches!(fit_tail("PC $00FF00", 1000, 1), Cow::Borrowed(_)),
            "a line that fits must not allocate"
        );

        // The marker plus one character is four glyphs, or 23 px, so 22 is one short of it: then
        // say "there is more here" rather than showing one arbitrary character as the answer.
        assert_eq!(
            fit_tail(LONG_PC, 23, 1),
            "...4",
            "four glyphs is marker plus one"
        );
        assert_eq!(
            fit_tail(LONG_PC, 22, 1),
            "...",
            "one px less leaves only the marker"
        );
        // Below even the marker it clips honestly. `overlay::fit` charges the first glyph 5 px and
        // every later one 6, so 16 buys two dots and 4 buys none at all.
        assert_eq!(fit_tail(LONG_PC, 16, 1), "..");
        assert_eq!(fit_tail(LONG_PC, 4, 1), "");
    }

    /// **The panel's width does not depend on the PC symbol's length.** That dependency is the whole
    /// bug: the owner's chip grew to fit a fifty-glyph symbol, clamped to the full width of the
    /// picture, and covered the CRAM strip.
    ///
    /// Two chips differing *only* in the PC line — one short, one absurd — must produce pixel-
    /// identical panel geometry. An assertion that the panel is merely "not full width" would pass
    /// on a panel that still grew, just less; comparing two renders pins the independence itself,
    /// and it needs no number written out to do it.
    ///
    /// The expanded form is the one under test because that is where the collision happened, and
    /// the `expected` bound is stated separately so the test also fails if *neither* render is the
    /// right size.
    #[test]
    fn the_panel_width_does_not_depend_on_the_pc_symbols_length() {
        let (w, h) = (960usize, 720usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 896,
            h: 672,
        };
        let px = 3; // the owner's window: 672 / 224
        let with = |pc: &str| {
            let mut chip = model(&regs(), None, 7, false, true);
            chip.lines[0] = format!("PC {pc}");
            let buf = render(w, h, area, px, &chip);
            ink_bounds(&buf, w, BG).expect("draw painted nothing")
        };
        let short = with("$00FF00");
        let long = with(&LONG_PC.repeat(3));
        assert_eq!(
            short, long,
            "the panel moved or resized when the PC symbol got longer"
        );

        // And it is the size it should be: 24 glyphs at px 2 (the expanded block drops a scale) is
        // `24 * 6 - 1 = 143`, doubled to 286, plus 2 * (2 * 2) of pad = 294.
        let (_, _, left, right) = short;
        assert_eq!(
            right - left + 1,
            294,
            "the expanded panel is not CHIP_COLUMNS wide"
        );
        assert!(
            right - left + 1 < area.w / 2,
            "the chip still takes more than half the picture's width"
        );
    }

    /// **The drawn PC line really is the symbol's tail**, measured on the glass rather than in
    /// [`fit_tail`] alone.
    ///
    /// `fit_tail`'s own test pins the function; this pins that `draw` *calls* it. Deleting the call
    /// and truncating the PC like every other line — the state this fix replaced — leaves the
    /// panel's geometry pixel-identical, so every other test in this module stays green. (Measured:
    /// it survived `the_panel_width_does_not_depend_on_the_pc_symbols_length` outright.)
    ///
    /// Two pairs, each differing by a single character, and neither needs a glyph shape or a
    /// pixel offset written out. Changing the **last** character must change the render, or the
    /// tail is not what reaches the glass. Changing the **first** must not, or the head is still on
    /// screen and nothing was truncated from the front. Head-truncation fails both, in opposite
    /// directions.
    #[test]
    fn the_drawn_pc_line_is_the_symbols_tail() {
        let (w, h) = (960usize, 720usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 896,
            h: 672,
        };
        let with = |pc: &str| {
            let mut chip = model(&regs(), None, 7, false, true);
            chip.lines[0] = format!("PC {pc}");
            render(w, h, area, 3, &chip)
        };
        assert_ne!(
            with(&format!("{LONG_PC}A")),
            with(&format!("{LONG_PC}B")),
            "the last character of the symbol never reached the glass — the PC is being \
             truncated from the wrong end"
        );
        assert_eq!(
            with(&format!("A{LONG_PC}")),
            with(&format!("B{LONG_PC}")),
            "the first character of the symbol is still on screen, so nothing was dropped from \
             the front"
        );
    }

    /// **The expanded block draws one font scale smaller.** Eleven lines at the owner's `px = 3`
    /// stand 276 px tall in a 672 px picture — 41% of it before width is counted. One scale down
    /// they stand 184.
    ///
    /// Heights are pinned by equality at two scales, and the compact head is pinned alongside to
    /// show the drop is *conditional*: a `scale_for` that shrank everything would halve the compact
    /// chip too, and a `scale_for` that shrank nothing gives 276. Both numbers are written out —
    /// `11 * 8 * 2 + 2 * (2 * 2)` and `3 * 8 * 3 + 2 * (2 * 3)` — never taken from the function.
    #[test]
    fn the_expanded_block_draws_one_font_scale_smaller() {
        let (w, h) = (960usize, 720usize);
        let area = Rect {
            x: 0,
            y: 0,
            w: 896,
            h: 672,
        };
        let tall = |px: usize, expanded: bool| {
            let buf = render(w, h, area, px, &model(&regs(), None, 7, false, expanded));
            let (top, bottom, _, _) = ink_bounds(&buf, w, BG).expect("draw painted nothing");
            bottom - top + 1
        };
        assert_eq!(
            tall(3, true),
            184,
            "the expanded block at px 3 draws at px 2"
        );
        assert_eq!(tall(3, false), 84, "the compact head keeps px 3");
        // px 2 drops to px 1: `11 * 8 * 1 + 2 * (2 * 1)`. Undropped it would be 184 — the same
        // number the px-3 row demands, which is what makes these two rows rule each other's bug out.
        assert_eq!(
            tall(2, true),
            92,
            "the expanded block at px 2 draws at px 1"
        );
        assert_eq!(
            tall(1, true),
            92,
            "px 1 has nowhere to drop to, so it matches px 2's dropped height exactly"
        );
    }
}
