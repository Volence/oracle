//! **SPIKE (M24-NEEDS-A-CURRENCY), NOT PROPOSED FOR MERGE.** Removed from the tip in the parcel's final
//! `spike:` commit; kept in history as the instrument behind `docs/2026-09-13-z80-timing-currency-design.md`.
//!
//! Two things live here, both thread-local so parallel test threads never mix:
//!
//! - **A probe.** Counters and FNV-1a digests fed from the Z80 core, `Z80Bus` and `catch_up_z80`: how many
//!   Z80 instructions ran gated-on, how many bus grants (and grants that cut an instruction), `EI`s,
//!   interrupt acceptances (and the two kinds each M24 half can change), plus four candidate currencies
//!   computed in place: a timestamped and an untimed digest of every Z80 bus access, a digest of the
//!   Z80-side FM/PSG write tap, and a digest of the instants the Z80 accepts `/INT`.
//! - **Env-gated mutations** (`M24_MUT`), so one build serves every leg and no leg depends on cargo's mtime
//!   fingerprint noticing an edit. Each mutation counts its own firings (`mut_fired`), which is the
//!   runtime proof that it applied.
//!
//! If `M24_PROBE_LOG` names a file, every test thread that ran the Z80 (gated-on or granted) appends one
//! line with its counters when the thread exits. This does file I/O inside a crate whose charter is
//! "no I/O"; that is why this is a spike and not a proposal.

use std::cell::RefCell;
use std::io::Write;
use std::sync::OnceLock;

/// The active mutation, read once from `M24_MUT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mutation {
    /// `M24_MUT` unset or empty: the committed behaviour.
    None,
    /// `m21`: every bus grant costs the Z80 an extra [`M21_EXTRA_MCLK`] (the M21 corpus measurement's
    /// mutation G, added once per grant at the grant edge and carried like the tail).
    M21,
    /// `ei`: UM0080's one-instruction `EI` delay — a pending `/INT` is not accepted at the boundary
    /// immediately after an `EI`.
    EiDelay,
    /// `int1`: `/INT` deasserted one line (`MCLK_PER_LINE`) after it was asserted, instead of at line 0.
    IntOneLine,
    /// `int1lvl`: `int1`, and the line is a LEVEL — acceptance no longer consumes it, so a handler that
    /// re-enables while it is still asserted takes it again (R6: "re-triggered ... within 228 Z80 clock
    /// cycles").
    IntOneLineLevel,
    /// `render`: timing-neutral, output-only. Flips bit 0 of every decoded red channel.
    NeutralRender,
    /// `r`: timing-neutral state change. The Z80 refresh register advances by 2 per M1 instead of 1.
    NeutralR,
}

/// The active mutation. Panics on an unknown value, so a typo cannot silently run the baseline.
pub fn mutation() -> Mutation {
    static M: OnceLock<Mutation> = OnceLock::new();
    *M.get_or_init(|| match std::env::var("M24_MUT").as_deref() {
        Ok("m21") => Mutation::M21,
        Ok("ei") => Mutation::EiDelay,
        Ok("int1") => Mutation::IntOneLine,
        Ok("int1lvl") => Mutation::IntOneLineLevel,
        Ok("render") => Mutation::NeutralRender,
        Ok("r") => Mutation::NeutralR,
        Ok("") | Err(_) => Mutation::None,
        Ok(other) => panic!("M24_MUT={other}: unknown mutation"),
    })
}

/// Either one-line `/INT` mutation (the width half of M24, with or without the level half).
pub fn int_one_line() -> bool {
    matches!(mutation(), Mutation::IntOneLine | Mutation::IntOneLineLevel)
}

/// The M21 mutation's per-grant cost: eight lines, `8 * MCLK_PER_LINE` = 8 * 3420 = 27,360 mclk.
pub const M21_EXTRA_MCLK: u64 = 8 * crate::vdp::MCLK_PER_LINE;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a-64 over `bytes`, folded into `h`.
pub fn fnv(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(FNV_PRIME);
    }
}

/// The FNV-1a-64 offset basis, for callers folding their own digests.
pub const FNV_BASIS: u64 = FNV_OFFSET;

/// One thread's probe.
#[derive(Clone, Debug)]
pub struct Probe {
    /// Z80 instructions stepped in `catch_up_z80`'s gated-on arm.
    pub gated_on_steps: u64,
    /// Bus grants (entries into the bus-granted arm from another arm).
    pub grants: u64,
    /// Grants whose first call found a tail (`frontier > step_start`): the arm M21 changed.
    pub grants_with_tail: u64,
    /// Calls into the held-in-reset arm.
    pub reset_calls: u64,
    /// `EI` executions.
    pub ei: u64,
    /// Maskable interrupts accepted.
    pub accepts: u64,
    /// Acceptances at the boundary immediately after an `EI`: exactly the ones an `EI` delay defers.
    pub ei_then_accept: u64,
    /// Acceptances at a Z80 frontier at least one line after the assert: exactly the ones a one-line
    /// `/INT` would lose.
    pub accept_late: u64,
    /// `/INT` asserts (VInt events).
    pub int_asserts: u64,
    /// Late acceptances where the bus was granted at some point after the assert (the Z80 was stopped
    /// through part of the window), and those where it never was (the driver held interrupts off).
    pub late_grant: u64,
    pub late_masked: u64,
    /// Acceptances beyond the first for one assert (only possible with a level `/INT`).
    pub retriggers: u64,
    accepts_this_assert: u64,
    granted_since_assert: bool,
    /// Times the active mutation took effect.
    pub mut_fired: u64,
    /// Candidate (a): every Z80 bus access, `(op, z80 addr, value, mclk)`.
    pub access_count: u64,
    pub access_timed: u64,
    /// Candidate (a'): the same stream without the clock, `(op, z80 addr, value)`.
    pub access_order: u64,
    /// Candidate (c) as the Z80 sees it: the FM/PSG write tap, `(addr, value, mclk)`.
    pub fmpsg_count: u64,
    pub fmpsg_timed: u64,
    /// The same tap bucketed by frame (`mclk / MCLK_PER_FRAME`), the default `VgmLogger`'s resolution.
    pub fmpsg_frame: u64,
    /// Candidate (d): the Z80 frontier at each acceptance.
    pub accept_log: u64,
    last_ei: bool,
    frontier: u64,
    int_assert_mclk: u64,
    in_grant: bool,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            gated_on_steps: 0,
            grants: 0,
            grants_with_tail: 0,
            reset_calls: 0,
            ei: 0,
            accepts: 0,
            ei_then_accept: 0,
            accept_late: 0,
            int_asserts: 0,
            late_grant: 0,
            late_masked: 0,
            retriggers: 0,
            accepts_this_assert: 0,
            granted_since_assert: false,
            mut_fired: 0,
            access_count: 0,
            access_timed: FNV_OFFSET,
            access_order: FNV_OFFSET,
            fmpsg_count: 0,
            fmpsg_timed: FNV_OFFSET,
            fmpsg_frame: FNV_OFFSET,
            accept_log: FNV_OFFSET,
            last_ei: false,
            frontier: 0,
            int_assert_mclk: 0,
            in_grant: false,
        }
    }
}

impl Probe {
    /// One `key=value` line, stable field order.
    pub fn line(&self) -> String {
        format!(
            "gated_on_steps={} grants={} grants_with_tail={} reset_calls={} ei={} accepts={} \
             ei_then_accept={} accept_late={} late_grant={} late_masked={} retriggers={} int_asserts={} \
             mut_fired={} access_count={} \
             access_timed={:016x} access_order={:016x} fmpsg_count={} fmpsg_timed={:016x} \
             fmpsg_frame={:016x} accept_log={:016x}",
            self.gated_on_steps,
            self.grants,
            self.grants_with_tail,
            self.reset_calls,
            self.ei,
            self.accepts,
            self.ei_then_accept,
            self.accept_late,
            self.late_grant,
            self.late_masked,
            self.retriggers,
            self.int_asserts,
            self.mut_fired,
            self.access_count,
            self.access_timed,
            self.access_order,
            self.fmpsg_count,
            self.fmpsg_timed,
            self.fmpsg_frame,
            self.accept_log,
        )
    }
}

struct Slot(RefCell<Probe>);

impl Drop for Slot {
    fn drop(&mut self) {
        let Ok(path) = std::env::var("M24_PROBE_LOG") else {
            return;
        };
        let p = self.0.borrow();
        if p.gated_on_steps == 0 && p.grants == 0 {
            return;
        }
        let exe = std::env::current_exe()
            .ok()
            .and_then(|e| e.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "?".into());
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        let line = format!(
            "PROBE exe={exe} thread={thread} mut={:?} {}\n",
            mutation(),
            p.line()
        );
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

thread_local! {
    static P: Slot = Slot(RefCell::new(Probe::default()));
}

fn with<R: Default>(f: impl FnOnce(&mut Probe) -> R) -> R {
    P.try_with(|s| f(&mut s.0.borrow_mut()))
        .unwrap_or_default()
}

/// This thread's probe, copied.
pub fn snapshot() -> Probe {
    P.try_with(|s| s.0.borrow().clone()).unwrap_or_default()
}

/// Zero this thread's probe (the start of a leg).
pub fn reset() {
    with(|p| *p = Probe::default());
}

pub(crate) fn note_mut_fired() {
    with(|p| p.mut_fired += 1);
}

pub(crate) fn note_ei() {
    with(|p| {
        p.ei += 1;
        p.last_ei = true;
    });
}

/// Was the instruction the Z80 just finished an `EI`? Clears the latch.
pub(crate) fn take_last_ei() -> bool {
    with(|p| std::mem::take(&mut p.last_ei))
}

pub(crate) fn note_accept(after_ei: bool) {
    with(|p| {
        p.accepts += 1;
        if after_ei {
            p.ei_then_accept += 1;
        }
        if p.int_asserts > 0 && p.frontier >= p.int_assert_mclk + crate::vdp::MCLK_PER_LINE {
            p.accept_late += 1;
            if p.granted_since_assert {
                p.late_grant += 1;
            } else {
                p.late_masked += 1;
            }
        }
        if p.accepts_this_assert > 0 {
            p.retriggers += 1;
        }
        p.accepts_this_assert += 1;
        let f = p.frontier;
        fnv(&mut p.accept_log, &f.to_le_bytes());
    });
}

pub(crate) fn set_frontier(mclk: u64) {
    with(|p| p.frontier = mclk);
}

pub(crate) fn note_int_assert(mclk: u64) {
    with(|p| {
        p.int_asserts += 1;
        p.int_assert_mclk = mclk;
        p.accepts_this_assert = 0;
        p.granted_since_assert = false;
    });
}

pub(crate) fn int_assert_mclk() -> u64 {
    with(|p| p.int_assert_mclk)
}

pub(crate) fn note_gated_on_entry() {
    with(|p| p.in_grant = false);
}

pub(crate) fn note_gated_on_step() {
    with(|p| p.gated_on_steps += 1);
}

/// A bus-granted call. Returns `true` on the grant's first call (the grant edge).
pub(crate) fn note_grant(tail: u64) -> bool {
    with(|p| {
        p.granted_since_assert = true;
        if p.in_grant {
            return false;
        }
        p.in_grant = true;
        p.grants += 1;
        if tail > 0 {
            p.grants_with_tail += 1;
        }
        true
    })
}

pub(crate) fn note_reset() {
    with(|p| {
        p.reset_calls += 1;
        p.in_grant = false;
    });
}

pub(crate) fn access(write: bool, addr: u16, value: u8, mclk: u64) {
    with(|p| {
        p.access_count += 1;
        let a = addr.to_le_bytes();
        fnv(&mut p.access_order, &[u8::from(write), a[0], a[1], value]);
        fnv(&mut p.access_timed, &[u8::from(write), a[0], a[1], value]);
        fnv(&mut p.access_timed, &mclk.to_le_bytes());
    });
}

pub(crate) fn fmpsg(addr: u16, value: u8, mclk: u64) {
    with(|p| {
        p.fmpsg_count += 1;
        let a = addr.to_le_bytes();
        fnv(&mut p.fmpsg_timed, &[a[0], a[1], value]);
        fnv(&mut p.fmpsg_timed, &mclk.to_le_bytes());
        fnv(&mut p.fmpsg_frame, &[a[0], a[1], value]);
        fnv(
            &mut p.fmpsg_frame,
            &(mclk / crate::system::MCLK_PER_FRAME).to_le_bytes(),
        );
    });
}
