//! **The Z80 `/INT` census** (listening-kit instrument for card d-51, M24 parcel 4). Feature `z80-census`,
//! default OFF: without the feature this module does not exist and its four call sites compile away, so
//! the default build, every currency and every gate are untouched by construction.
//!
//! It answers one question over a run: for each once-a-frame `/INT` assert, did the Z80 take it, and
//! when? The model under test decides *whether* an acceptance happens; this census only watches and
//! classifies, against a fixed ruler that does not depend on the model: the documented one-line pulse
//! (`docs/2026-07-16-vdp-recon.md` R6, `MCLK_PER_LINE` mclk from the assert instant).
//!
//! Per assert, on the Z80's own clock (its frontier at the acceptance):
//!
//! - **in window**: the first acceptance inside `[assert, assert + one line)`;
//! - **late**: the first acceptance at or past one line after the assert (only a "held until taken"
//!   model can produce one); split by whether a 68000 bus grant overlapped the window (`late_grant`) or
//!   not (`late_masked`: the Z80 simply had interrupts off);
//! - **missed**: no acceptance at all before the next assert, split the same way;
//! - **re-trigger**: a second or later acceptance for the same assert (only a level model can produce
//!   one).
//!
//! Thread-local, so parallel test threads never mix counts; the listening kit's render tool runs one
//! machine on one thread.

use std::cell::RefCell;

use crate::vdp::{MCLK_PER_FRAME, MCLK_PER_LINE};

/// What happened to one assert, for the per-event log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Taken inside the one-line window.
    InWindow,
    /// Taken at or past one line after the assert.
    Late,
    /// Never taken before the next assert.
    Missed,
}

/// One assert that was not taken in its window, for the "where to listen" timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// The assert instant, absolute mclk.
    pub assert_mclk: u64,
    /// `assert_mclk / MCLK_PER_FRAME`.
    pub frame: u64,
    /// Late or missed (in-window asserts are not logged).
    pub outcome: Outcome,
    /// Mclk from the assert to the acceptance (late only; 0 for missed).
    pub delay_mclk: u64,
    /// A 68000 bus grant overlapped the one-line window.
    pub grant_in_window: bool,
}

/// The running totals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Asserts delivered to the Z80 (finalised ones: the one still open at the end is excluded).
    pub asserts: u64,
    /// Every acceptance, whatever it is attributed to.
    pub acceptances: u64,
    /// First acceptance inside the window.
    pub in_window: u64,
    /// First acceptance past the window, with a grant in the window.
    pub late_grant: u64,
    /// First acceptance past the window, no grant in the window.
    pub late_masked: u64,
    /// No acceptance, with a grant in the window.
    pub missed_grant: u64,
    /// No acceptance, no grant in the window.
    pub missed_masked: u64,
    /// Second and later acceptances of one assert.
    pub retriggers: u64,
    /// Acceptances before the first assert of the run (none are expected).
    pub orphan_acceptances: u64,
    /// Every late/missed assert, in order.
    pub events: Vec<Event>,
}

#[derive(Default)]
struct State {
    totals: Totals,
    frontier: u64,
    open: Option<Open>,
}

#[derive(Clone, Copy)]
struct Open {
    assert: u64,
    accepts: u64,
    first_delay: u64,
    grant: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn finalise(st: &mut State) {
    let Some(o) = st.open.take() else { return };
    let t = &mut st.totals;
    t.asserts += 1;
    let outcome = if o.accepts == 0 {
        if o.grant {
            t.missed_grant += 1;
        } else {
            t.missed_masked += 1;
        }
        Outcome::Missed
    } else if o.first_delay < MCLK_PER_LINE {
        t.in_window += 1;
        Outcome::InWindow
    } else {
        if o.grant {
            t.late_grant += 1;
        } else {
            t.late_masked += 1;
        }
        Outcome::Late
    };
    if outcome != Outcome::InWindow {
        t.events.push(Event {
            assert_mclk: o.assert,
            frame: o.assert / MCLK_PER_FRAME,
            outcome,
            delay_mclk: if outcome == Outcome::Late {
                o.first_delay
            } else {
                0
            },
            grant_in_window: o.grant,
        });
    }
}

/// `System`'s `VInt` arm: the Z80's `/INT` was asserted at `deadline`.
pub(crate) fn note_assert(deadline: u64) {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        finalise(&mut st);
        st.open = Some(Open {
            assert: deadline,
            accepts: 0,
            first_delay: 0,
            grant: false,
        });
    });
}

/// `catch_up_z80`, before each Z80 step: the Z80's clock at the boundary it is about to sample.
pub(crate) fn set_frontier(mclk: u64) {
    STATE.with(|s| s.borrow_mut().frontier = mclk);
}

/// `Z80::accept_interrupt`: an acceptance at the last frontier set.
pub(crate) fn note_accept() {
    STATE.with(|s| {
        let mut guard = s.borrow_mut();
        let st = &mut *guard;
        st.totals.acceptances += 1;
        match st.open.as_mut() {
            None => st.totals.orphan_acceptances += 1,
            Some(o) => {
                if o.accepts == 0 {
                    o.first_delay = st.frontier.saturating_sub(o.assert);
                } else {
                    st.totals.retriggers += 1;
                }
                o.accepts += 1;
            }
        }
    });
}

/// `catch_up_z80`'s bus-granted arm, covering `[from, to)` of the 68000's clock.
pub(crate) fn note_grant(from: u64, to: u64) {
    STATE.with(|s| {
        if let Some(o) = s.borrow_mut().open.as_mut() {
            if o.accepts == 0 && to > o.assert && from < o.assert + MCLK_PER_LINE {
                o.grant = true;
            }
        }
    });
}

/// Take this thread's totals and start over. The assert still open (its window may not have ended) is
/// not counted.
pub fn take() -> Totals {
    STATE.with(|s| std::mem::take(&mut *s.borrow_mut()).totals)
}
