//! **A line a pane cuts says so** (`PANEL-CLIP-MARK`, the whole-widget half; owner card `d-54`, ruled
//! `mark-only` by the hub 2026-09-25).
//!
//! # The defect
//!
//! [`crate::ui::fitted_label`] marks a cut when the TOOLKIT is the one truncating: a label that was told a
//! width, and ran out of it, ends in `…`. What it cannot reach is text whose whole CONTAINER is wider than
//! the pane: a table, a fact grid or a control strip wider than its pane widens the tab's scroll content,
//! every paragraph after it then wraps at that wider width, and the pane's clip cuts the right-hand side
//! off with nothing on the glass to say so. The first parcel measured 67 such rows over 28 arrangements (60
//! whose layout box lies past the pane, 7 control captions laid out at their natural width), and held them
//! under a ceiling for the owner, because no truncation WIDTH can mark them without changing the layout: the
//! width they were laid out at is the container's, and the container is his.
//!
//! # The ruling, and what it rules out
//!
//! `d-54`, ruled `mark-only`: **a cut line must announce it was cut, with the full text on hover. No line
//! is wrapped onto a second row, and no pane width or default dock layout changes.** So nothing here is
//! allowed to change what any widget ALLOCATES. It changes what the widget PAINTED, after the fact.
//!
//! # The treatment: the painted glyphs, not the layout
//!
//! [`mark_cut_rows`] runs once per drawn panel body, after the body has painted, over exactly the shapes it
//! painted (the same `[start, end)` slice of the layer's paint list the CR-W harvest reads, see
//! [`crate::screen::PanelMark`]). For every text shape with a glyph row that runs past its clip's right edge
//! at the pane's edge, the row is re-laid out by the toolkit itself — the row's own characters in their own
//! formats, told the width that is really visible, one row, `…` as the overflow character — and that row is
//! spliced into the painted galley in place of the cut one. Everything else about the shape stays:
//!
//! * **the layout box and every allocation** — they were decided before this runs, and nothing reads the
//!   painted galley back, so a column is exactly as wide as it was and nothing wraps. This is also why the
//!   `Grid` ratchet `fitted_label` warns about cannot happen here: the ratchet is truncation feeding back
//!   into next frame's column width, and this truncation never reaches an allocation;
//! * **the galley's job**, so [`crate::screen::glass_run`]'s `text` is still the whole source (§11.50: `text`
//!   is every run the toolkit laid out on a reported row), `rendered` is still exactly the glyphs on the
//!   glass (which now include the mark), and row k / run j alignment is untouched: no run is added, removed
//!   or moved to another row;
//! * **the row's position** — the new row is placed so its first glyph lands where the old row's first
//!   glyph was, and on the same baseline.
//!
//! A wrapped paragraph gets the mark on EVERY row the pane cuts, because each such row lost text and a
//! reader of any one of them cannot tell. The galley is flagged `elided`, which is what it now is.
//!
//! # The hover
//!
//! The whole text, on hover, **once**. The first parcel's second finding was a DOUBLE hover: egui's `Label`
//! already shows its whole text on hover when the galley it drew is elided (`show_tooltip_when_elided`),
//! and a second `on_hover_text` stacked a second copy. This pass runs after the widget decided its hover,
//! so the rule is exact: **a galley the toolkit had already elided keeps the toolkit's hover and gets none
//! from here; a galley this pass is the first to cut gets one, over its visible part.** The hover is a
//! `Sense::hover()` region, so it never takes a click or a drag from the control underneath it (egui hovers
//! a non-interactive widget on top of an interactive one alongside it, `egui-0.36.1/src/interaction.rs:263`).
//!
//! # What is deliberately NOT marked
//!
//! A row cut by a clip that is not the pane's own right edge: a `TextEdit` clips its text to its own frame
//! and scrolls it under the caret, and an input field whose typed text grew an `…` would be showing the
//! reader characters that are not in it. "The pane's edge" is the body's clip, less the width a nested
//! scroll area's bar takes from it (`ScrollStyle::allocated_width`). The gate
//! (`no_drawn_run_is_cut_at_a_pane_edge_without_the_mark`) checks every clip, so a cut at an inner clip is
//! still found; it just is not treated here.
//!
//! # Alternatives rejected
//!
//! * **Give each table cell a truncation width from the pane's visible clip.** Marks single-line cells
//!   only; a paragraph that wraps at the widened width needs a narrower WRAP, which is the layout change the
//!   ruling forbids. And a width that depends on the clip feeds straight back into the cell's allocation,
//!   which is the `Grid` ratchet again: a column truncated to what was visible last frame is narrower this
//!   frame, so less of it is visible, so it is truncated further.
//! * **Paint a separate `…` shape at the clip edge.** It would be a run of its own in `screen_text` (text
//!   `…`, attached to no source), it would sit on top of a half-drawn glyph rather than after a whole one,
//!   and the reader's row would not end in the mark — a second glyph beside it would.
//! * **Rewrap the paragraphs to the visible width.** The best reading experience, and precisely what the
//!   owner kept for himself.
//! * **Mutate the row's mesh by hand** (drop the cut glyphs' quads, add an ellipsis quad). Exact, but it
//!   re-implements the toolkit's own elision against a mesh layout epaint does not promise; laying the row
//!   out through the toolkit gets the mark, the kerning and the atlas entry from the one place that owns
//!   them.

use std::sync::Arc;

/// The overflow mark, the same character egui's own elision writes.
pub const MARK: char = '\u{2026}';

#[cfg(test)]
thread_local! {
    static OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// **Test-only: run `f` with this pass switched off**, the control arm of the gate. The sweep has to be
/// able to see the cuts this pass removes, or its green would be an absence nobody measured.
#[cfg(test)]
pub fn with_marking_off<R>(f: impl FnOnce() -> R) -> R {
    OFF.with(|o| o.set(true));
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            OFF.with(|o| o.set(false));
        }
    }
    let _restore = Restore;
    f()
}

/// Where a shape lives in the paint list: its index, and the path through any nested `Shape::Vec`.
type Path = (usize, Vec<usize>);

/// One text shape that needs marking, read out under the graphics lock and treated outside it (laying out
/// text needs the fonts, which take the same context lock).
struct Cut {
    path: Path,
    clip: egui::Rect,
    pos: egui::Pos2,
    galley: Arc<egui::Galley>,
}

/// **Mark every row the pane cut in the shapes `[start, end)` of `layer`**, and hang the whole text on the
/// hover of each run this pass is the first to cut. See the module docs for the rules and why.
///
/// `ui` is the panel body's own `Ui`: its clip is the pane, and its id scopes the hover regions.
pub fn mark_cut_rows(ui: &mut egui::Ui, layer: egui::LayerId, start: usize, end: usize) {
    #[cfg(test)]
    if OFF.with(std::cell::Cell::get) {
        return;
    }
    if end <= start {
        return;
    }
    let pane_right = ui.clip_rect().max.x - ui.spacing().scroll.allocated_width() - 0.5;
    let cuts = ui.ctx().graphics(|g| {
        let mut out = Vec::new();
        if let Some(list) = g.get(layer) {
            for (i, c) in list.all_entries().enumerate().take(end).skip(start) {
                find(&c.shape, c.clip_rect, (i, Vec::new()), pane_right, &mut out);
            }
        }
        out
    });
    if cuts.is_empty() {
        return;
    }
    let mut marked: Vec<(Path, Arc<egui::Galley>, Option<(egui::Rect, String)>)> = Vec::new();
    for cut in cuts {
        let Some(galley) = remark(ui, &cut) else {
            continue;
        };
        // The toolkit's hover already carries an elided galley's whole text; one from here would be the
        // second copy the first parcel measured.
        let hover = (!cut.galley.elided)
            .then(|| visible_rect(&galley, cut.pos, cut.clip))
            .flatten()
            .map(|r| (r, cut.galley.text().to_owned()));
        marked.push((cut.path, galley, hover));
    }
    ui.ctx().graphics_mut(|g| {
        let list = g.entry(layer);
        for (path, galley, _) in &marked {
            list.mutate_shape(egui::layers::ShapeIdx(path.0), |c| {
                replace(&mut c.shape, &path.1, galley.clone());
            });
        }
    });
    for (path, _, hover) in marked {
        if let Some((rect, text)) = hover {
            let id = ui.id().with(("cut_mark", path.0, path.1));
            ui.interact(rect, id, egui::Sense::hover())
                .on_hover_text(text);
        }
    }
}

/// Collect every text shape under `shape` with a row the pane cut and did not mark.
fn find(shape: &egui::Shape, clip: egui::Rect, path: Path, pane_right: f32, out: &mut Vec<Cut>) {
    match shape {
        egui::Shape::Text(t) => {
            // A clip short of the pane's edge is a widget's own (a TextEdit's): not this pass's business.
            if clip.max.x < pane_right || t.angle != 0.0 {
                return;
            }
            let right = t.pos.x + t.galley.mesh_bounds.max.x;
            if right <= clip.max.x {
                return;
            }
            if t.galley.rows.iter().any(|row| row_is_cut(row, t.pos, clip)) {
                out.push(Cut {
                    path,
                    clip,
                    pos: t.pos,
                    galley: t.galley.clone(),
                });
            }
        }
        egui::Shape::Vec(v) => {
            for (j, s) in v.iter().enumerate() {
                let mut p = path.clone();
                p.1.push(j);
                find(s, clip, p, pane_right, out);
            }
        }
        _ => {}
    }
}

/// Whether `row` put a non-blank glyph on the glass, ran a non-blank glyph past the clip's right edge, and
/// does not already end on the glass in the mark. The same geometry the gate reads (`logical_rect`), so this
/// pass and the gate agree on what "cut" means.
fn row_is_cut(row: &egui::epaint::text::PlacedRow, pos: egui::Pos2, clip: egui::Rect) -> bool {
    let o = pos.to_vec2() + row.pos.to_vec2();
    let mut last_visible: Option<char> = None;
    let mut over = false;
    for g in &row.glyphs {
        let r = g.logical_rect().translate(o);
        let on = r.min.x < clip.max.x
            && r.max.x > clip.min.x
            && r.min.y < clip.max.y
            && r.max.y > clip.min.y;
        if on && !g.chr.is_whitespace() {
            last_visible = Some(g.chr);
        }
        if !g.chr.is_whitespace() && r.max.x > clip.max.x {
            over = true;
        }
    }
    over && last_visible.is_some_and(|c| c != MARK)
}

/// The galley `cut` painted, with every cut row replaced by the toolkit's own truncation of it to the width
/// that is visible. `None` when no row could be marked (nothing is changed then).
fn remark(ui: &egui::Ui, cut: &Cut) -> Option<Arc<egui::Galley>> {
    let src = &cut.galley;
    let job = &src.job;
    // Byte offset of every char, plus the end: rows are counted in chars, sections in bytes.
    let bytes: Vec<usize> = job
        .text
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(job.text.len()))
        .collect();
    let n_chars = bytes.len() - 1;
    let mut rows = src.rows.clone();
    let mut changed = false;
    let mut at = 0usize; // char index of the current row's first glyph
    let last = src.rows.len().saturating_sub(1);
    for (k, placed) in src.rows.iter().enumerate() {
        let row_start = at;
        at += placed.glyphs.len() + usize::from(placed.ends_with_newline);
        if !row_is_cut(placed, cut.pos, cut.clip) {
            continue;
        }
        let Some(first) = placed.glyphs.first() else {
            continue;
        };
        // The row's own characters. The last row of a galley the toolkit already elided ends in ITS mark,
        // which is not in the source; that row takes the rest of the source instead, so the new layout is
        // certain to overflow and write the mark where it can be seen.
        let from = row_start.min(n_chars);
        let to = if src.elided && k == last {
            n_chars
        } else {
            (row_start + placed.glyphs.len()).min(n_chars)
        };
        if to <= from {
            continue;
        }
        let (b0, b1) = (bytes[from], bytes[to]);
        let sections: Vec<egui::text::LayoutSection> = job
            .sections
            .iter()
            .filter(|s| s.byte_range.start.0 < b1 && s.byte_range.end.0 > b0)
            .map(|s| egui::text::LayoutSection {
                leading_space: 0.0,
                byte_range: egui::epaint::text::ByteIndex(s.byte_range.start.0.max(b0) - b0)
                    ..egui::epaint::text::ByteIndex(s.byte_range.end.0.min(b1) - b0),
                format: s.format.clone(),
            })
            .collect();
        if sections.is_empty() {
            continue;
        }
        let origin_x = cut.pos.x + placed.pos.x + first.pos.x;
        let width = cut.clip.max.x - origin_x;
        if width <= 0.0 {
            continue;
        }
        let sub = egui::text::LayoutJob {
            text: job.text[b0..b1].to_owned(),
            sections,
            wrap: egui::text::TextWrapping {
                max_width: width,
                max_rows: 1,
                break_anywhere: true,
                overflow_character: Some(MARK),
            },
            round_output_to_gui: false,
            ..Default::default()
        };
        let laid = ui.painter().layout_job(sub);
        let Some(new) = laid.rows.first() else {
            continue;
        };
        // Only a row that now ends in the mark is an improvement; anything else leaves the row as it was.
        if !laid.elided || new.glyphs.last().map(|g| g.chr) != Some(MARK) {
            continue;
        }
        let Some(nfirst) = new.glyphs.first() else {
            continue;
        };
        rows[k] = egui::epaint::text::PlacedRow {
            pos: placed.pos + egui::vec2(first.pos.x - nfirst.pos.x, first.pos.y - nfirst.pos.y),
            row: new.row.clone(),
            ends_with_newline: placed.ends_with_newline,
        };
        changed = true;
    }
    if !changed {
        return None;
    }
    let mut g = (**src).clone();
    g.num_vertices = rows.iter().map(|r| r.visuals.mesh.vertices.len()).sum();
    g.num_indices = rows.iter().map(|r| r.visuals.mesh.indices.len()).sum();
    g.mesh_bounds = rows.iter().fold(egui::Rect::NOTHING, |b, r| {
        b.union(r.visuals.mesh_bounds.translate(r.pos.to_vec2()))
    });
    g.rows = rows;
    g.elided = true;
    Some(Arc::new(g))
}

/// The part of `galley`, painted at `pos`, that lies inside `clip`: where a pointer has to be to hover it.
fn visible_rect(galley: &egui::Galley, pos: egui::Pos2, clip: egui::Rect) -> Option<egui::Rect> {
    let r = galley
        .rows
        .iter()
        .fold(egui::Rect::NOTHING, |b, row| b.union(row.rect()))
        .translate(pos.to_vec2())
        .intersect(clip);
    r.is_positive().then_some(r)
}

/// Put `galley` into the text shape at `path` under `shape`.
fn replace(shape: &mut egui::Shape, path: &[usize], galley: Arc<egui::Galley>) {
    match (shape, path.split_first()) {
        (egui::Shape::Text(t), None) => t.galley = galley,
        (egui::Shape::Vec(v), Some((j, rest))) => {
            if let Some(s) = v.get_mut(*j) {
                replace(s, rest, galley);
            }
        }
        _ => {}
    }
}
