//! ⚑ **The shape of a table column, with nothing panel-specific in it.**
//!
//! The window's column table ([`crate::ui`]'s `table`, and the `column_widths` / `table_cell` /
//! `header_cell` / `cell_face` furniture under it) was written for the Objects tab and took
//! [`crate::objects::Col`] everywhere, so the Profiler, the Watch log and the Breakpoint list — three
//! more tables of like-shaped rows, and the three the audit's build order puts next — could not reach it
//! without dragging `objects::Field` in with them.
//!
//! Only one thing in that type was ever object-specific: `field`, which is how an `objects::Row` knows
//! which of its own facts a column holds, and which `cell_colour` matches on. `head`, `numeric` and
//! `mono` are **presentation facts about a column of text** and say nothing about objects at all. So
//! they live here, `objects::Col` composes this rather than restating it, and the renderer takes this.
//!
//! **This module holds no `egui` type**, deliberately and for the reason the Pacing exemplar states: a
//! projection that can only be checked by opening a window is a projection nothing can check, and this
//! window cannot be opened from an agent seat.

/// One column of a table, named once so a header and a body cannot disagree about how many there are or
/// what order they come in.
///
/// `numeric` and `mono` are **presentation facts derived from what the column holds**, kept beside the
/// column rather than in the renderer so two tables cannot align one kind of column two ways: a machine
/// address and a cycle count are monospace because they are read digit by digit against each other, and a
/// count or a coordinate is right-aligned because that is what makes a column of numbers comparable at a
/// glance. A symbol name is neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Col {
    /// The header cell. Lower case: this is a column of a table, not a title.
    pub head: &'static str,
    /// Right-aligned. True for anything whose digits a reader compares down the column.
    pub numeric: bool,
    /// Drawn in the monospace face. Style page P3: addresses, hex and machine numbers only.
    pub mono: bool,
}
