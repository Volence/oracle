//! The command palette — the modal control surface (spec §3–§4). Pure state machine: input is
//! [`PaletteKey`], output is [`PaletteAction`]; no window, no I/O, fully testable headless.
//! The main loop feeds it keys while open and swallows game input; the game keeps RUNNING
//! behind it (dev-first: the watch ticker stays live while you type).

use crate::commands::{self, Cmd, CommandInfo, Group};
use crate::font::{self, Canvas};
use crate::overlay::{self, ACCENT, INFO};
use crate::present::Rect;

/// Keys the palette understands, already translated from minifb by the caller
/// (`commands::key_char` for the typable set).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaletteKey {
    Char(char),
    Backspace,
    Up,
    Down,
    Enter,
    Esc,
}

/// What the main loop should do after a key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaletteAction {
    None,
    /// Run this command (palette has closed itself).
    Run(Cmd),
}

/// One visible row: a group header or an index into the registry slice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Header(&'static str),
    Item(usize),
}

/// A secondary pick list ("Select save slot..."), opened by the main loop with concrete items.
pub struct Picker {
    pub title: String,
    /// (label, command to run when chosen)
    pub items: Vec<(String, Cmd)>,
    pub sel: usize,
}

pub struct Palette {
    open: bool,
    query: String,
    /// Selection as an index into the CURRENT `rows()` output, always on an `Item` row.
    sel: usize,
    /// Most-recently-used commands, newest first, capped at MRU_CAP, visible-only.
    recents: Vec<Cmd>,
    picker: Option<Picker>,
}

/// The selection highlight bar behind the current row, shared by the item list and the picker
/// list. `inner_w` must already be the saturating `panel_w - 2 * margin` computed once in
/// `draw` — never recompute that subtraction here (a narrow panel can make it underflow).
fn draw_selected_bar(canvas: &mut Canvas, text_x: i32, y: i32, inner_w: usize, line_h: usize) {
    canvas.fill_rect(text_x - 2, y - 1, inner_w, line_h, 0x00123A46, 255);
}

pub const MRU_CAP: usize = 3;

impl Palette {
    pub fn new() -> Self {
        Palette {
            open: false,
            query: String::new(),
            sel: 0,
            recents: Vec::new(),
            picker: None,
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    /// Opens the palette on the full grouped list, selection landing on the first `Item` row
    /// (the invariant "sel is always on an Item row" holds from this call onward, not just
    /// after the first key).
    pub fn open(&mut self, reg: &[CommandInfo]) {
        self.open = true;
        self.query.clear();
        self.picker = None;
        self.sel = 0;
        self.clamp_sel(reg);
    }
    pub fn close(&mut self) {
        self.open = false;
        self.picker = None;
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn picker(&self) -> Option<&Picker> {
        self.picker.as_ref()
    }
    /// Open a secondary pick list (the main loop builds the items — occupancy etc. lives there).
    /// Clears the query too, mirroring `open()`: otherwise Esc-ing back out of the picker would
    /// reveal the main list still filtered by whatever the user had typed before opening it.
    pub fn open_picker(&mut self, title: String, items: Vec<(String, Cmd)>, reg: &[CommandInfo]) {
        self.query.clear();
        self.picker = Some(Picker {
            title,
            items,
            sel: 0,
        });
        self.open = true;
        self.clamp_sel(reg);
    }

    /// The rows the palette shows for its current query. Empty query = grouped full list with
    /// an optional RECENT section on top (spec §4: the empty palette IS the menu). Non-empty
    /// query = flat filtered list, no headers. Hidden registry rows never appear.
    pub fn rows(&self, reg: &[CommandInfo]) -> Vec<Row> {
        let mut out = Vec::new();
        if self.query.is_empty() {
            // RECENT section first (visible commands only, newest first).
            let recent_idx: Vec<usize> = self
                .recents
                .iter()
                .filter_map(|cmd| reg.iter().position(|c| c.cmd == *cmd && !c.hidden))
                .collect();
            if !recent_idx.is_empty() {
                out.push(Row::Header("RECENT"));
                out.extend(recent_idx.into_iter().map(Row::Item));
            }
            for g in Group::ALL {
                let members: Vec<usize> = reg
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.group == g && !c.hidden)
                    .map(|(i, _)| i)
                    .collect();
                if !members.is_empty() {
                    out.push(Row::Header(g.title()));
                    out.extend(members.into_iter().map(Row::Item));
                }
            }
        } else {
            out.extend(
                reg.iter()
                    .enumerate()
                    .filter(|(_, c)| !c.hidden && commands::subseq_match(&self.query, c.title))
                    .map(|(i, _)| Row::Item(i)),
            );
        }
        out
    }

    /// Feed one key. Returns what the caller should do. Selection is clamped to Item rows;
    /// Enter on an Item runs it (recording MRU) and closes; Esc closes (picker first).
    pub fn handle(&mut self, key: PaletteKey, reg: &[CommandInfo]) -> PaletteAction {
        // Picker mode intercepts everything.
        if let Some(pk) = self.picker.as_mut() {
            match key {
                PaletteKey::Up => pk.sel = pk.sel.saturating_sub(1),
                PaletteKey::Down => pk.sel = (pk.sel + 1).min(pk.items.len().saturating_sub(1)),
                PaletteKey::Enter => {
                    if let Some((_, cmd)) = pk.items.get(pk.sel) {
                        let cmd = *cmd;
                        self.close();
                        return PaletteAction::Run(cmd);
                    }
                }
                PaletteKey::Esc => self.picker = None, // back to the list, palette stays open
                PaletteKey::Char(_) | PaletteKey::Backspace => {}
            }
            return PaletteAction::None;
        }
        match key {
            PaletteKey::Char(c) => {
                self.query.push(c);
                self.sel = 0;
            }
            PaletteKey::Backspace => {
                self.query.pop();
                self.sel = 0;
            }
            PaletteKey::Up => self.move_sel(reg, -1),
            PaletteKey::Down => self.move_sel(reg, 1),
            PaletteKey::Esc => self.close(),
            PaletteKey::Enter => {
                let rows = self.rows(reg);
                if let Some(Row::Item(i)) = rows.get(self.sel) {
                    let cmd = reg[*i].cmd;
                    self.record_recent(cmd);
                    self.close();
                    return PaletteAction::Run(cmd);
                }
            }
        }
        self.clamp_sel(reg);
        PaletteAction::None
    }

    /// Keep `sel` on an `Item` row (the list may have changed under it, or this may be the
    /// first frame after `open`/`open_picker`, before any key has landed on an Item at all).
    fn clamp_sel(&mut self, reg: &[CommandInfo]) {
        let rows = self.rows(reg);
        if !rows.is_empty() {
            self.sel = self.sel.min(rows.len() - 1);
            if matches!(rows[self.sel], Row::Header(_)) {
                self.move_sel(reg, 1);
            }
        } else {
            self.sel = 0;
        }
    }

    /// Move the selection to the next/previous `Item` row, skipping headers, clamped.
    fn move_sel(&mut self, reg: &[CommandInfo], dir: isize) {
        let rows = self.rows(reg);
        if rows.is_empty() {
            self.sel = 0;
            return;
        }
        let mut i = self.sel as isize;
        loop {
            i += dir;
            if i < 0 || i as usize >= rows.len() {
                // Clamp: stay where we were if there is no further Item in this direction.
                if !matches!(rows.get(self.sel), Some(Row::Item(_))) {
                    // Initial position may sit on a header (fresh open): find the first Item.
                    if let Some(first) = rows.iter().position(|r| matches!(r, Row::Item(_))) {
                        self.sel = first;
                    }
                }
                return;
            }
            if matches!(rows[i as usize], Row::Item(_)) {
                self.sel = i as usize;
                return;
            }
        }
    }

    fn record_recent(&mut self, cmd: Cmd) {
        self.recents.retain(|c| *c != cmd);
        self.recents.insert(0, cmd);
        self.recents.truncate(MRU_CAP);
    }

    pub fn sel(&self) -> usize {
        self.sel
    }

    /// Paint the palette into the presentation buffer, inside the picture rect only (the same
    /// rule the overlay obeys — never the retained native framebuffer, spec §10). Scale follows
    /// the overlay's: `Overlay::font_scale`.
    pub fn draw(&self, buf: &mut [u32], w: usize, h: usize, area: Rect, reg: &[CommandInfo]) {
        if !self.open || area.w == 0 || area.h == 0 {
            return;
        }
        // Deliberately scales off the window height `h`, not `area.h` (unlike `Overlay::draw`) —
        // the palette is a UI surface anchored to the picture but not obliged to shrink with a
        // small picture in a large window. That decouples panel size from font size, so every
        // width computed below (panel_w, inner_w, the hotkey column, ...) must saturate: a tall
        // window with a narrow picture can make the font bigger than the panel is wide.
        let px = overlay::Overlay::font_scale(h);
        let line_h = font::LINE_H * px;
        let margin = 4 * px;
        // Panel: inset from the picture rect, top-anchored, tall enough for the query line
        // plus what fits.
        let panel_x = area.x + area.w / 10;
        let panel_w = area.w - 2 * (area.w / 10);
        let panel_y = area.y + area.h / 12;
        let panel_h = (area.h - 2 * (area.h / 12)).min(area.h);
        let mut canvas = Canvas::new(buf, w, h);
        canvas.fill_rect(
            panel_x as i32,
            panel_y as i32,
            panel_w,
            panel_h,
            0x000A1418,
            font::PANEL_ALPHA,
        );

        let text_x = (panel_x + margin) as i32;
        let mut y = (panel_y + margin) as i32;
        // Every text run below is clipped to this inner width so nothing can paint past the
        // panel's right edge (and therefore past `area`) — Canvas only clips at buffer edges,
        // not at an arbitrary rect. `overlay::fit` truncates on whole-glyph boundaries.
        let inner_w = panel_w.saturating_sub(2 * margin);

        if let Some(pk) = &self.picker {
            canvas.text(text_x, y, px, ACCENT, overlay::fit(&pk.title, inner_w, px));
            y += (line_h + margin / 2) as i32;
            for (i, (label, _)) in pk.items.iter().enumerate() {
                if (y as usize + line_h) > panel_y + panel_h {
                    break;
                }
                if i == pk.sel {
                    draw_selected_bar(&mut canvas, text_x, y, inner_w, line_h);
                }
                canvas.text(text_x, y, px, INFO, overlay::fit(label, inner_w, px));
                y += line_h as i32;
            }
            return;
        }

        // Query line: "> query_" (static underscore cursor; append-only editing needs no more).
        let q = format!("> {}_", self.query);
        canvas.text(text_x, y, px, ACCENT, overlay::fit(&q, inner_w, px));
        y += (line_h + margin / 2) as i32;

        for (ri, row) in self.rows(reg).iter().enumerate() {
            if (y as usize + line_h) > panel_y + panel_h {
                break; // capped rows; scrolling arrives with a taller list than fits (none yet)
            }
            match row {
                Row::Header(hdr) => {
                    canvas.text(text_x, y, px, ACCENT, overlay::fit(hdr, inner_w, px));
                }
                Row::Item(i) => {
                    let c = &reg[*i];
                    if ri == self.sel {
                        draw_selected_bar(&mut canvas, text_x, y, inner_w, line_h);
                    }
                    let indent = 2 * font::ADVANCE * px;
                    // Reserve room for the hotkey column (its width plus a margin-wide gap) so
                    // a long title can never run into or past it.
                    let hotkey_reserved = c
                        .hotkey
                        .map(|k| font::text_width(commands::key_name(k)) * px + margin)
                        .unwrap_or(0);
                    let title_avail = inner_w
                        .saturating_sub(indent)
                        .saturating_sub(hotkey_reserved);
                    canvas.text(
                        text_x + indent as i32,
                        y,
                        px,
                        INFO,
                        overlay::fit(c.title, title_avail, px),
                    );
                    if let Some(k) = c.hotkey {
                        let name = commands::key_name(k);
                        let kw = font::text_width(name) * px;
                        // Right-aligned to the panel's inner edge. Only draw it if it actually
                        // fits `inner_w` — a saturating subtraction alone would still let a
                        // too-wide name collapse to kx=0 and paint past the panel (and area) on
                        // a panel too narrow for its own margins.
                        if kw <= inner_w {
                            let kx = (panel_x + panel_w)
                                .saturating_sub(margin)
                                .saturating_sub(kw) as i32;
                            canvas.text(kx, y, px, 0x007AA0BB, name);
                        }
                    }
                }
            }
            y += line_h as i32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::registry;

    fn open_palette() -> (Palette, Vec<CommandInfo>) {
        let mut p = Palette::new();
        let reg = registry();
        p.open(&reg);
        (p, reg)
    }

    /// Empty query: every group header present in order, every visible command present,
    /// no hidden command present.
    #[test]
    fn empty_query_lists_everything_grouped() {
        let (p, reg) = open_palette();
        let rows = p.rows(&reg);
        let headers: Vec<&str> = rows
            .iter()
            .filter_map(|r| {
                if let Row::Header(h) = r {
                    Some(*h)
                } else {
                    None
                }
            })
            .collect();
        for g in Group::ALL {
            assert!(headers.contains(&g.title()), "missing header {}", g.title());
        }
        let visible = reg.iter().filter(|c| !c.hidden).count();
        let items = rows.iter().filter(|r| matches!(r, Row::Item(_))).count();
        assert_eq!(items, visible, "every visible command listed exactly once");
        for r in &rows {
            if let Row::Item(i) = r {
                assert!(!reg[*i].hidden, "hidden command leaked into the list");
            }
        }
    }

    /// Typing filters; headers disappear; the filtered set is exactly the matching titles.
    #[test]
    fn typing_filters() {
        let (mut p, reg) = open_palette();
        for c in "watch".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        let rows = p.rows(&reg);
        assert!(
            rows.iter().all(|r| matches!(r, Row::Item(_))),
            "no headers while filtering"
        );
        assert!(!rows.is_empty());
        for r in &rows {
            if let Row::Item(i) = r {
                assert!(
                    commands::subseq_match("watch", reg[*i].title),
                    "non-matching row {}",
                    reg[*i].title
                );
            }
        }
    }

    /// A query that subsequence-matches a HIDDEN title must never surface that row. Self-
    /// validating: it asserts its own fixture actually matches a hidden title via
    /// `subseq_match`, so it cannot pass vacuously if the registry's hidden titles change.
    #[test]
    fn hidden_never_matches_query() {
        let (mut p, reg) = open_palette();
        let query = "alias";
        let hidden_hit = reg
            .iter()
            .find(|c| c.hidden && commands::subseq_match(query, c.title))
            .unwrap_or_else(|| {
                panic!("fixture query {query:?} must subsequence-match some hidden title")
            });
        assert_eq!(hidden_hit.title, "Soft reset (F1 alias)");

        for c in query.chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        let rows = p.rows(&reg);
        for r in &rows {
            if let Row::Item(i) = r {
                assert!(
                    !reg[*i].hidden,
                    "hidden command {:?} leaked into query results",
                    reg[*i].title
                );
            }
        }
        // Today's registry has no VISIBLE title matching "alias" either, so the row list
        // should be empty outright.
        let visible_hit = reg
            .iter()
            .any(|c| !c.hidden && commands::subseq_match(query, c.title));
        if !visible_hit {
            assert!(
                rows.is_empty(),
                "expected no visible matches for {query:?}, got {rows:?}"
            );
        }
    }

    /// Enter runs the selected command, closes the palette, and records it in MRU; reopening
    /// shows it under RECENT.
    #[test]
    fn enter_runs_and_records_mru() {
        let (mut p, reg) = open_palette();
        // Filter down to exactly one row to make the selection deterministic.
        for c in "dump".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        let rows = p.rows(&reg);
        assert_eq!(
            rows.len(),
            1,
            "'dump' should match exactly the dump-hits command"
        );
        let act = p.handle(PaletteKey::Enter, &reg);
        assert_eq!(act, PaletteAction::Run(Cmd::DumpHits));
        assert!(!p.is_open(), "palette closes after running");
        p.open(&reg);
        let rows = p.rows(&reg);
        assert_eq!(rows[0], Row::Header("RECENT"));
        match rows[1] {
            Row::Item(i) => assert_eq!(reg[i].cmd, Cmd::DumpHits),
            _ => panic!("first recent row is not an item"),
        }
    }

    /// The RECENT list never exceeds MRU_CAP even after more distinct commands have been run,
    /// and re-running a command already present does not duplicate it.
    #[test]
    fn mru_respects_cap_and_dedup() {
        let (mut p, reg) = open_palette();
        let recent_cmds = |p: &Palette, reg: &[CommandInfo]| -> Vec<Cmd> {
            let rows = p.rows(reg);
            if rows.first() != Some(&Row::Header("RECENT")) {
                return Vec::new();
            }
            rows[1..]
                .iter()
                .take_while(|r| matches!(r, Row::Item(_)))
                .map(|r| match r {
                    Row::Item(i) => reg[*i].cmd,
                    _ => unreachable!(),
                })
                .collect()
        };
        // Run 4 distinct commands (more than MRU_CAP = 3).
        for query in ["pause", "quit", "clear", "dump"] {
            for c in query.chars() {
                p.handle(PaletteKey::Char(c), &reg);
            }
            let rows = p.rows(&reg);
            assert_eq!(rows.len(), 1, "'{query}' should match exactly one command");
            p.handle(PaletteKey::Enter, &reg);
            p.open(&reg);
        }
        let recents = recent_cmds(&p, &reg);
        assert!(
            recents.len() <= MRU_CAP,
            "RECENT exceeded MRU_CAP: {:?}",
            recents
        );
        assert_eq!(
            recents.len(),
            MRU_CAP,
            "RECENT should be at the cap after 4 distinct runs"
        );

        // Re-run a command already present in RECENT; it must not be duplicated.
        assert!(recents.contains(&Cmd::Quit));
        for c in "quit".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        p.handle(PaletteKey::Enter, &reg);
        p.open(&reg);
        let recents = recent_cmds(&p, &reg);
        assert!(
            recents.len() <= MRU_CAP,
            "RECENT exceeded MRU_CAP after re-run: {:?}",
            recents
        );
        let quit_count = recents.iter().filter(|c| **c == Cmd::Quit).count();
        assert_eq!(
            quit_count, 1,
            "re-running a command must not duplicate it in RECENT"
        );
    }

    /// Up/Down move the selection over Item rows only (headers are skipped) and clamp at the
    /// ends; backspace un-filters; Esc closes without running.
    #[test]
    fn navigation_and_esc() {
        let (mut p, reg) = open_palette();
        assert_eq!(p.handle(PaletteKey::Down, &reg), PaletteAction::None);
        let rows = p.rows(&reg);
        assert!(
            matches!(rows[p.sel()], Row::Item(_)),
            "selection sits on an item"
        );
        p.handle(PaletteKey::Up, &reg);
        p.handle(PaletteKey::Up, &reg); // clamp at top, no panic
        for c in "zzzz".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        assert!(p.rows(&reg).is_empty(), "nothing matches zzzz");
        assert_eq!(
            p.handle(PaletteKey::Enter, &reg),
            PaletteAction::None,
            "enter on empty is a no-op"
        );
        for _ in 0..4 {
            p.handle(PaletteKey::Backspace, &reg);
        }
        assert!(!p.rows(&reg).is_empty(), "backspace restored the list");
        assert_eq!(p.handle(PaletteKey::Esc, &reg), PaletteAction::None);
        assert!(!p.is_open());
    }

    /// The picker: arrows move, Enter yields the picked command, Esc falls back to the main
    /// list (not a full close).
    #[test]
    fn picker_flow() {
        let (mut p, reg) = open_palette();
        p.open_picker(
            "SELECT SLOT".into(),
            vec![
                ("slot 0".into(), Cmd::SlotSelect(0)),
                ("slot 1".into(), Cmd::SlotSelect(1)),
            ],
            &reg,
        );
        p.handle(PaletteKey::Down, &reg);
        let act = p.handle(PaletteKey::Enter, &reg);
        assert_eq!(act, PaletteAction::Run(Cmd::SlotSelect(1)));
        assert!(!p.is_open());
        // Esc inside a picker returns to the list, palette stays open.
        p.open_picker(
            "SELECT SLOT".into(),
            vec![("slot 0".into(), Cmd::SlotSelect(0))],
            &reg,
        );
        p.handle(PaletteKey::Esc, &reg);
        assert!(p.is_open(), "esc closes the picker, not the palette");
        assert!(p.picker().is_none());

        // Fix 2: open_picker clears a stale query. The main loop can invoke it directly
        // (SlotPicker is intercepted before it ever reaches `handle`), including while the
        // user had typed a filter — Esc-ing back out must show the FULL grouped list, not the
        // filtered one that was on screen before the picker opened.
        for c in "quit".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        assert_eq!(p.query(), "quit");
        p.open_picker(
            "SELECT SLOT".into(),
            vec![("slot 0".into(), Cmd::SlotSelect(0))],
            &reg,
        );
        p.handle(PaletteKey::Esc, &reg);
        assert!(p.is_open());
        assert!(p.picker().is_none());
        assert!(
            p.query().is_empty(),
            "open_picker must clear the stale query"
        );
        assert!(
            p.rows(&reg).iter().any(|r| matches!(r, Row::Header(_))),
            "esc from the picker shows the full grouped list, not a filtered one"
        );
    }

    /// `sel` sits on an Item row at every checkpoint: immediately after open() (pins fix 1,
    /// before any key has been fed), after Down runs past the last row (clamp, stay put),
    /// after Up clamps at the top (twice — not just "no panic"), and after backspace restores
    /// a previously-emptied list.
    #[test]
    fn sel_always_on_item_row() {
        let (mut p, reg) = open_palette();
        assert!(
            matches!(p.rows(&reg)[p.sel()], Row::Item(_)),
            "sel must sit on an Item row immediately after open(), before any key"
        );

        // Down past the last row: clamp, stay on an Item.
        let row_count = p.rows(&reg).len();
        for _ in 0..(row_count + 2) {
            p.handle(PaletteKey::Down, &reg);
        }
        assert!(
            matches!(p.rows(&reg)[p.sel()], Row::Item(_)),
            "clamped-at-bottom sel is an Item"
        );

        // Up past the top: clamp, stay on an Item.
        for _ in 0..(row_count + 2) {
            p.handle(PaletteKey::Up, &reg);
        }
        assert!(
            matches!(p.rows(&reg)[p.sel()], Row::Item(_)),
            "clamped-at-top sel is an Item"
        );

        // Filter to nothing, then backspace back out: sel lands on an Item again.
        for c in "zzzz".chars() {
            p.handle(PaletteKey::Char(c), &reg);
        }
        assert!(p.rows(&reg).is_empty());
        for _ in 0..4 {
            p.handle(PaletteKey::Backspace, &reg);
        }
        assert!(!p.rows(&reg).is_empty(), "backspace restored the list");
        assert!(
            matches!(p.rows(&reg)[p.sel()], Row::Item(_)),
            "sel sits on an Item after backspace restores the list"
        );
    }

    /// Rendering smoke: the palette paints its panel into the buffer (some pixels change) and
    /// stays inside the given area. Pixel-exactness is not asserted — layout is free to evolve;
    /// what must hold is "drew something, only inside the picture rect".
    #[test]
    fn draw_paints_inside_area_only() {
        let (mut p, reg) = open_palette();
        p.handle(PaletteKey::Down, &reg);
        let (w, h) = (320usize, 224usize);
        let mut buf = vec![0u32; w * h];
        let area = crate::present::Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        p.draw(&mut buf, w, h, area, &reg);
        let painted = buf.iter().filter(|px| **px != 0).count();
        assert!(painted > 0, "draw painted nothing");
        for (i, px) in buf.iter().enumerate() {
            if *px != 0 {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
    }

    /// Regression for a real containment escape a spec-review probe found: a long enough query
    /// pushed the query line's text past the panel's right edge and out of `area` — `Canvas`
    /// only clips at buffer edges, never at an arbitrary rect, so an unclipped text run can
    /// paint anywhere in the whole buffer. Every text run `draw` emits must be clipped to the
    /// panel's inner width via `overlay::fit`.
    #[test]
    fn draw_contains_long_query() {
        let (mut p, reg) = open_palette();
        for _ in 0..60 {
            p.handle(PaletteKey::Char('a'), &reg);
        }
        let (w, h) = (320usize, 224usize);
        let mut buf = vec![0u32; w * h];
        let area = crate::present::Rect {
            x: 40,
            y: 20,
            w: 240,
            h: 180,
        };
        p.draw(&mut buf, w, h, area, &reg);
        for (i, px) in buf.iter().enumerate() {
            if *px != 0 {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
    }

    /// Regression for a reproducible panic a quality review found: a tall window against a very
    /// narrow picture (`h=1000` -> `px=4` -> `margin=16`, `area.w=34` -> `panel_w=28`) made the
    /// selection-highlight bar's old unguarded `panel_w - 2 * margin` underflow — a debug panic
    /// ("attempt to subtract with overflow") and a release wraparound to ~`usize::MAX` (a
    /// hanging fill loop). `draw` must not panic here, and containment must still hold.
    #[test]
    fn draw_narrow_panel_does_not_underflow() {
        let (mut p, reg) = open_palette();
        p.handle(PaletteKey::Down, &reg); // a selection is on-screen, so the highlight bar draws
        let (w, h) = (100usize, 1000usize);
        let mut buf = vec![0u32; w * h];
        let area = crate::present::Rect {
            x: 0,
            y: 0,
            w: 34,
            h: 1000,
        };
        p.draw(&mut buf, w, h, area, &reg);
        for (i, px) in buf.iter().enumerate() {
            if *px != 0 {
                let (x, y) = (i % w, i / w);
                assert!(
                    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h,
                    "painted outside area at ({x},{y})"
                );
            }
        }
    }

    /// Closed palette draws nothing.
    #[test]
    fn draw_noop_when_closed() {
        let p = Palette::new();
        let reg = registry();
        let mut buf = vec![0u32; 320 * 224];
        p.draw(
            &mut buf,
            320,
            224,
            crate::present::Rect {
                x: 0,
                y: 0,
                w: 320,
                h: 224,
            },
            &reg,
        );
        assert!(buf.iter().all(|px| *px == 0));
    }
}
