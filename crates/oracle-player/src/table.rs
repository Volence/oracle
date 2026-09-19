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
//!
//! [`render`] joined them 2026-09-19 for the same reason and from the same cause: it is how a served
//! value becomes the text in a cell, it says nothing about any one panel, and it had drifted into two
//! spellings — the guarded one in [`crate::ui`] and an unguarded raw catch-all in
//! [`crate::objects::Row::cell`].

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

/// A served value as a panel prints it in one cell. **Exhaustive by construction, and that is the point.**
///
/// This used to be two arms: `String(s) => s.clone()` and `other => other.to_string()`. It was correct
/// for every value the server actually sends today, because
/// [`DecodedRecord::to_json`](oracle_core::decoders) emits only scalars and the one composite key
/// (`"fields"`) is skipped by its caller before it ever gets here. It was correct **by luck about the
/// wire**, not by anything this function does: the day a served key becomes an object or an array, that
/// catch-all `to_string()` puts `{"a":1,"b":[2,3]}` on the owner's screen, which is exactly the raw JSON
/// the style page's **P1** forbids, and no test in the crate would have gone red.
///
/// So the catch-all is gone and every `serde_json::Value` variant is spelled out. The two composite arms
/// **state what arrived** instead of dumping it: a nested value is a fact about the served shape, and
/// telling the reader that a shape they cannot see has appeared is useful, whereas printing its
/// punctuation at them is not. If that ever becomes the wrong answer it will be because a real composite
/// key is worth drawing, and drawing it is a panel decision with a layout attached, not a fallthrough.
///
/// `Null` is a stated absence rather than the four characters `null`, per **P6**: an absent fact is a
/// line that says so.
///
/// ⚑ **It lives here, and not in [`crate::ui`], because it had TWO spellings and only one of them was
/// guarded.** `crate::objects::Row::cell` carried the identical two-arm shape — the raw catch-all
/// included — for as long as this one did, and closing it by copying these seven arms into `objects.rs`
/// would have left the next reader two functions to keep in step. There is one now, in the module that
/// already owns the rest of a table cell, and both panels compose it.
///
/// Guarded by [`crate::ui`]'s `json_tests::no_served_value_can_put_raw_json_on_the_screen`, which walks
/// every variant through every renderer in the crate that turns a served value into display text.
pub fn render(v: &serde_json::Value) -> String {
    match v {
        // A string is the payload without its quotes. This is the overwhelmingly common case.
        serde_json::Value::String(s) => s.clone(),
        // Numbers and booleans spell identically in JSON and in prose, so there is no punctuation to leak.
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        // P6: `null` on the wire means the server had nothing, and a reader is owed that in words.
        serde_json::Value::Null => NO_VALUE.to_owned(),
        serde_json::Value::Array(a) => format!(
            "{} value{} in a list. This panel draws single values, so the list itself is not shown.",
            a.len(),
            if a.len() == 1 { "" } else { "s" }
        ),
        serde_json::Value::Object(m) => format!(
            "{} key{} in a nested record. This panel draws single values, so the record is not shown.",
            m.len(),
            if m.len() == 1 { "" } else { "s" }
        ),
    }
}

/// What [`render`] prints for a served `null`. A stated absence, never the token `null` and never a zero.
pub const NO_VALUE: &str = "no value (the server sent nothing here)";
