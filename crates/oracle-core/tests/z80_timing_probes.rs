//! **The Z80 timing probes** (M24 parcel 2): the Z80-timing currency.
//!
//! Authority: `docs/2026-09-13-z80-timing-currency-design.md` (§1 candidate (e), §2.1, §7 row 2). Each probe
//! is a tiny Z80 program run on the `testrom::build` machine, whose 68000 stirs work RAM forever and never
//! touches the Z80. The program stores what it observed into Z80 RAM and the test reads it back, so nothing
//! here needs a hook, a hot-path change, a save-format change or the wire. This file is the whole parcel,
//! and it reaches the machine only through `System`'s public API.
//!
//! ## Pinned at TODAY's values, and some of those are wrong
//!
//! The M24 fixes (design §7 parcels 3 and 4) must move these pins by a predicted amount; a fix that moves
//! nothing, or moves a probe it does not name, is wrong by construction. So each pin is today's behaviour,
//! and where the documentation disagrees the pin says **WRONG** in its own comment, with the documented
//! value, the source and the parcel expected to move it beside it. A pin changes only in the commit that
//! changes the behaviour, next to a `cause:` line naming the mechanism and the measured delta (design §7,
//! "Regeneration").
//!
//! | Probe | Observable | Pinned today | Documented (source) | Moved by |
//! |---|---|---|---|---|
//! | C1 | `A` when the interrupt is taken, `EI` then `INC A` | 0, WRONG | 1 (UM0080 p.18) | parcel 3 |
//! | C2a | `V`, `HL` when taken, `EI` about 0.7 line after the assert | `$E0`, 0 | `$E0` (R6), 1 (UM0080 p.18) | HL: parcel 3. Parcel 4 must leave it; a `/INT` shorter than about 0.7 line moves HL to 3309-3311 (the next frame's assert, also at `$E0`) |
//! | C2b | `V`, `HL` when taken, `EI` about 1.2 lines after the assert | `$E1`, 0, WRONG | `$E0`, 3303-3305 (R6) | V and HL: parcel 4. Parcel 3 alone moves HL to 1 |
//! | C3 | passes of a 34-T-state loop across 100 bus grants | the derived count | the same (UM0080 T-states, `MCLK_PER_Z80_CYCLE`) | nothing in M24; it guards M21 |
//! | C4 | handler entries per `/INT` assert, handler re-enables inside the window | 1 per assert, WRONG (medium confidence) | 6, or 5 once parcel 3 lands (R6's level corollary, MEDIUM confidence) | parcel 4 |
//!
//! C2 has two legs where the design had one, with cause. The design's single leg enabled two lines after
//! the assert, so it could only tell "`/INT` is gone within about 2.2 lines" from "it is not". A two-line
//! pulse lands that leg exactly on its documented value, so a parcel-4 fix of the wrong width would have
//! passed it. C2b enables 1.2 lines after the assert and C2a 0.7 line after, so between them the documented
//! one-line pulse is bracketed from both sides.
//!
//! ## Where every number comes from
//!
//! - Z80 T-states: Zilog *Z80 CPU User Manual* UM008011-0816 (UM0080), each instruction's own page. The
//!   IM 1 acceptance is 13 (design §6.1: "two more than normal", on an 11-T restart).
//! - Clocks: [`MCLK_PER_Z80_CYCLE`], [`MCLK_PER_CPU_CYCLE`], [`MCLK_PER_LINE`], [`MCLK_PER_FRAME`].
//! - The `/INT` assert: line [`ACTIVE_LINES`] (`$E0`), H counter `$02` (`docs/2026-07-16-vdp-recon.md` R6),
//!   which is 4 of the 342 positions of an H32 line (recon R2; the test ROM never selects H40).
//! - The V counter a program reads through `$7F08`: the scanline number for lines 0-234 (recon R2).
//!
//! No expected value here is read back from the emulator's own accounting of the thing under test. C3's
//! count is derived from the gated-on spans the test itself drove, measured on the 68000's clock, which no
//! Z80 timing change touches.

use oracle_core::m68000::bus68k::Bus68k;
use oracle_core::system::{System, MCLK_PER_CPU_CYCLE, MCLK_PER_Z80_CYCLE};
use oracle_core::vdp::{ACTIVE_LINES, MCLK_PER_FRAME, MCLK_PER_LINE};

// ---- UM0080 T-states ---------------------------------------------------------------------------------------

const T_DI: u64 = 4;
const T_EI: u64 = 4;
const T_LD_A_IND_NN: u64 = 13; // LD A,(nn)
const T_CP_N: u64 = 7;
const T_JR_TAKEN: u64 = 12; // JR e, and JR cc,e taken
const T_JR_NOT_TAKEN: u64 = 7; // JR cc,e not taken
const T_LD_B_N: u64 = 7;
const T_DJNZ_TAKEN: u64 = 13;
const T_DJNZ_NOT_TAKEN: u64 = 8;
const T_LD_HL_NN: u64 = 10;
const T_INC_HL: u64 = 6; // INC ss
const T_LD_IND_NN_HL: u64 = 16; // LD (nn),HL
const T_INC_IND_HL: u64 = 11; // INC (HL)
const T_RET: u64 = 10;
const T_HALT_SLICE: u64 = 4; // HALT idles in NOP-length slices, sampling /INT each one
const T_ACCEPT_IM1: u64 = 13;

/// T-states to mclk: the one clock ratio every Z80 figure here goes through.
const fn mclk(t: u64) -> u64 {
    t * MCLK_PER_Z80_CYCLE
}

// ---- The /INT assert ---------------------------------------------------------------------------------------

/// The `/INT` assert's offset inside its line: H counter `$02` (R6) is position 4 of the 342 an H32 line
/// sweeps (the readable H is the top 8 bits of a 9-bit counter, recon R2).
const ASSERT_IN_LINE: u64 = 2 * 2 * MCLK_PER_LINE / 342;

/// The `/INT` assert's offset inside its frame: line `$E0`, the first line past the active display (R6).
const ASSERT_IN_FRAME: u64 = ACTIVE_LINES as u64 * MCLK_PER_LINE + ASSERT_IN_LINE;

/// How late the Z80 can see an assert: the run loop delivers the event at the first 68000 instruction
/// boundary at or past it (design §0, the late-view note), and the test ROM's longest instruction is
/// `lea $00FF0000,A0`, 12 cycles (M68000 PRM; `testrom.rs`'s listing). Used only in documented ranges.
const LATE_VIEW_MAX: u64 = 12 * MCLK_PER_CPU_CYCLE;

/// Every `/INT` assert strictly after `from` whose whole one-line window ends before `to`.
fn asserts_between(from: u64, to: u64) -> u64 {
    (0u64..)
        .map(|k| k * MCLK_PER_FRAME + ASSERT_IN_FRAME)
        .take_while(|&a| a + MCLK_PER_LINE <= to)
        .filter(|&a| a > from)
        .count() as u64
}

// ---- The harness -------------------------------------------------------------------------------------------

/// Where every probe stores what it saw.
const OUT: usize = 0x1000;

/// The `testrom::build` machine with `program` in Z80 RAM, the observation bytes zeroed, and the Z80
/// released through `$A11200` as a guest does. Before the release the machine runs briefly with the Z80
/// held in reset, which sets the Z80's frontier to the 68000's clock at every catch-up, so the program
/// starts at the release instant with no backlog. Returns the machine and the release instant.
fn released(program: &[(usize, &[u8])]) -> (System, u64) {
    let mut s = System::new(0x5EED);
    s.load_rom(oracle_core::testrom::build());
    s.reset();
    for (at, bytes) in program {
        s.z80_ram_mut()[*at..*at + bytes.len()].copy_from_slice(bytes);
    }
    s.z80_ram_mut()[OUT..OUT + 8].fill(0);
    let t = s.scheduler().now();
    s.run_until(t + 1_000);
    assert!(
        !s.z80_running(),
        "UNMEASURABLE: the Z80 left reset before the probe released it"
    );
    s.mega_bus(&mut ()).write8(0xA1_1200, 5, 1);
    assert!(
        s.z80_running(),
        "UNMEASURABLE: the reset release did not latch"
    );
    let at = s.scheduler().now();
    (s, at)
}

/// Take (`true`) or give back the Z80's bus through `$A11100`, as a guest does, and check it latched.
fn take_bus(s: &mut System, take: bool) {
    s.mega_bus(&mut ()).write8(0xA1_1100, 5, u8::from(take));
    assert_eq!(
        s.z80_busreq(),
        take,
        "UNMEASURABLE: the bus request did not latch"
    );
}

// ---- C1: the EI delay --------------------------------------------------------------------------------------

/// C1. `DI; IM 1; LD SP,$2000`, poll the V counter until the `/INT` line (V = `$E0`), wait out the assert,
/// then `XOR A; EI; INC A; INC A; INC A`. The request is pending (asserted, masked) when the `EI` runs.
const C1_MAIN: &[u8] = &[
    0xF3, // 0000 DI
    0xED, 0x56, // 0001 IM 1
    0x31, 0x00, 0x20, // 0003 LD SP,$2000
    0x3A, 0x08, 0x7F, // 0006 LD A,($7F08)   the V counter
    0xFE, 0xE0, // 0009 CP $E0
    0x38, 0xF9, // 000B JR C,$0006
    0x06, 0x04, // 000D LD B,4
    0x10, 0xFE, // 000F DJNZ $000F         3 x 13 + 8 T
    0xAF, // 0011 XOR A
    0xFB, // 0012 EI
    0x3C, // 0013 INC A
    0x3C, // 0014 INC A
    0x3C, // 0015 INC A
    0x18, 0xFE, // 0016 JR $0016
];

/// The C1 handler at `$0038`: store `A`, then the V counter, then a sentinel, and halt with IFF1 clear.
const C1_ISR: &[u8] = &[
    0x32, 0x00, 0x10, // LD ($1000),A
    0x3A, 0x08, 0x7F, // LD A,($7F08)
    0x32, 0x02, 0x10, // LD ($1002),A
    0x3E, 0xAA, // LD A,$AA
    0x32, 0x01, 0x10, // LD ($1001),A
    0x76, // HALT
];

/// **WRONG by the documentation, pinned because it is today's behaviour.** The core accepts a pending
/// request at the first instruction boundary after `EI` (`Z80::step` samples `int_pending && iff1` at
/// every boundary and `EI` sets IFF1 at once), so no `INC A` has run: `A = 0`.
const C1_A_TODAY: u8 = 0;
/// UM0080 p.18: "any pending interrupt request is not accepted until after the instruction following EI is
/// executed", so one `INC A` runs first. Parcel 3 (the `EI` delay) moves C1 from 0 to this.
const C1_A_DOCUMENTED: u8 = 1;

#[test]
fn c1_ei_delay_the_instruction_after_ei_runs_before_the_pending_interrupt() {
    let (mut s, _) = released(&[(0, C1_MAIN), (0x38, C1_ISR)]);
    s.run_frames(2);
    let seen = &s.z80_ram()[OUT..OUT + 3];
    let (a, sentinel, v) = (seen[0], seen[1], seen[2]);
    assert_eq!(sentinel, 0xAA, "UNMEASURABLE: the handler never ran");
    // The EI runs 1275-1754 mclk into line $E0 (the poll's slack, then 85 T of LD A, CP, JR, LD B, DJNZ and
    // XOR A), so its following boundary is inside any /INT pulse longer than about 0.52 line (C2a brackets
    // the width from below), and the handler reads V two instructions after acceptance. Any other line
    // means the probe's layout failed, not that the EI delay moved.
    assert_eq!(
        v, 0xE0,
        "UNMEASURABLE: the request was taken at V = {v:02X}, not on the assert's own line"
    );
    // A = 2 or 3 would mean the request was not pending at the EI at all: the probe did not measure.
    assert!(
        a <= C1_A_DOCUMENTED,
        "UNMEASURABLE: A = {a} when taken, so the request was not pending when EI ran"
    );
    assert_eq!(
        a, C1_A_TODAY,
        "C1 moved: A = {a} when taken. Pinned {C1_A_TODAY} (today, WRONG); documented {C1_A_DOCUMENTED} \
         (UM0080 p.18, the EI delay), which parcel 3 must produce with a cause: line"
    );
}

// ---- C2: the /INT width, bracketed ----------------------------------------------------------------------

/// The C2 handler at `$0038`: store `HL`, then the V counter, then a sentinel, and halt with IFF1 clear.
const C2_ISR: &[u8] = &[
    0x22, 0x00, 0x10, // LD ($1000),HL
    0x3A, 0x08, 0x7F, // LD A,($7F08)
    0x32, 0x02, 0x10, // LD ($1002),A
    0x3E, 0xAA, // LD A,$AA
    0x32, 0x04, 0x10, // LD ($1004),A
    0x76, // HALT
];

/// One `INC HL; JR` pass of both C2 programs' counting loop.
const C2_PASS: u64 = mclk(T_INC_HL + T_JR_TAKEN);

/// The poll's slack: its `LD A,($7F08)` samples V once per `LD; CP; JR` pass, so the first read of the
/// target line starts anywhere in the first pass-length of that line.
const POLL_SLACK_MAX: u64 = mclk(T_LD_A_IND_NN + T_CP_N + T_JR_TAKEN) - 1;

/// The HL a C2 program holds when it takes the NEXT frame's assert instead of the current one: the
/// `INC HL; JR` passes it began between its first `INC HL` (right after an `EI` starting `ei_in_line`
/// mclk into line `ei_line` of frame 0, at the poll's least slack) and the instant it sees frame 1's
/// assert. Returned as (least, greatest) over the two slacks the probe cannot pin: where in its pass the
/// poll's V read first saw the target line, and how late the 68000's instruction boundary lets the Z80
/// see the assert.
fn next_frame_hl(ei_line: u64, ei_in_line: u64) -> (u64, u64) {
    let first_inc_at = |slack: u64| ei_line * MCLK_PER_LINE + ei_in_line + slack + mclk(T_EI);
    let assert_seen = |late: u64| MCLK_PER_FRAME + ASSERT_IN_FRAME + late;
    let lo = (assert_seen(0) - first_inc_at(POLL_SLACK_MAX)).div_ceil(C2_PASS);
    let hi = (assert_seen(LATE_VIEW_MAX) - first_inc_at(0)).div_ceil(C2_PASS);
    (lo, hi)
}

/// C2a. Poll until V = `$E0`, wait with `LD B,9; DJNZ`, then `LD HL,0; EI` and count `INC HL; JR` passes.
/// The `EI` runs 2340-2819 mclk into line `$E0`, 0.67-0.81 line after the assert.
const C2A_MAIN: &[u8] = &[
    0xF3, // 0000 DI
    0xED, 0x56, // 0001 IM 1
    0x31, 0x00, 0x20, // 0003 LD SP,$2000
    0x3A, 0x08, 0x7F, // 0006 LD A,($7F08)
    0xFE, 0xE0, // 0009 CP $E0
    0x38, 0xF9, // 000B JR C,$0006
    0x06, 0x09, // 000D LD B,9
    0x10, 0xFE, // 000F DJNZ $000F         8 x 13 + 8 T
    0x21, 0x00, 0x00, // 0011 LD HL,0
    0xFB, // 0014 EI
    0x23, // 0015 INC HL
    0x18, 0xFD, // 0016 JR $0015
];

/// Where C2a's `EI` starts inside line `$E0`, at the poll's least and greatest slack.
const C2A_EI_IN_LINE: (u64, u64) = {
    let after_poll = mclk(
        T_LD_A_IND_NN
            + T_CP_N
            + T_JR_NOT_TAKEN
            + T_LD_B_N
            + 8 * T_DJNZ_TAKEN
            + T_DJNZ_NOT_TAKEN
            + T_LD_HL_NN,
    );
    (after_poll, after_poll + POLL_SLACK_MAX)
};

/// Today: the request is held from the assert to the next frame's line 0 (`System`'s `EventKind::VInt`
/// sets it, line 0 or acceptance clears it), so it is pending at the `EI` and taken at once: V = `$E0`,
/// HL = 0.
const C2A_TODAY: (u8, u16) = (0xE0, 0);
/// R6: the pulse is one line, so it is still asserted 0.67-0.81 line after the assert and is taken on the
/// same line, V = `$E0`. UM0080 p.18 runs the `INC HL` after `EI` first, HL = 1: parcel 3 moves HL 0 -> 1,
/// parcel 4 must leave C2a alone. A `/INT` shorter than about 0.7 line would miss this `EI`, and the next
/// frame's assert would be taken instead: also at V = `$E0`, but with HL a frame of passes
/// ([`next_frame_hl`]).
const C2A_DOCUMENTED: (u8, u16) = (0xE0, 1);

#[test]
fn c2a_int_width_the_request_is_still_held_most_of_a_line_after_the_assert() {
    let (ei_lo, ei_hi) = C2A_EI_IN_LINE;
    // The probe's own premise, from the constants: the EI (and the boundary after it) falls after the
    // assert and before one line past it.
    assert!(
        ei_lo > ASSERT_IN_LINE && ei_hi + mclk(T_EI) < ASSERT_IN_LINE + MCLK_PER_LINE,
        "the C2a layout must enable inside the documented pulse: EI at {ei_lo}-{ei_hi} mclk"
    );
    let (miss_lo, miss_hi) = next_frame_hl(ACTIVE_LINES as u64, ei_lo);
    let (mut s, _) = released(&[(0, C2A_MAIN), (0x38, C2_ISR)]);
    s.run_frames(3);
    let seen = &s.z80_ram()[OUT..OUT + 5];
    assert_eq!(seen[4], 0xAA, "UNMEASURABLE: the handler never ran");
    let got = (seen[2], u16::from_le_bytes([seen[0], seen[1]]));
    assert_eq!(
        got, C2A_TODAY,
        "C2a moved: (V, HL) = ({:02X}, {}) when taken. Pinned ({:02X}, {}) today; documented ({:02X}, {}): \
         R6 holds /INT for one line, so V stays $E0, and UM0080 p.18 makes HL 1 (parcel 3). HL in \
         {miss_lo}-{miss_hi} is the next frame's assert: /INT got shorter than this EI's 0.67-0.81 line",
        got.0, got.1, C2A_TODAY.0, C2A_TODAY.1, C2A_DOCUMENTED.0, C2A_DOCUMENTED.1
    );
}

/// C2b. Poll until V = `$E1`, then `LD HL,0; EI` and count `INC HL; JR` passes. The `EI` runs 555-1034 mclk
/// into line `$E1`, 1.15-1.29 lines after the assert.
const C2B_MAIN: &[u8] = &[
    0xF3, // 0000 DI
    0xED, 0x56, // 0001 IM 1
    0x31, 0x00, 0x20, // 0003 LD SP,$2000
    0x3A, 0x08, 0x7F, // 0006 LD A,($7F08)
    0xFE, 0xE1, // 0009 CP $E1
    0x38, 0xF9, // 000B JR C,$0006
    0x21, 0x00, 0x00, // 000D LD HL,0
    0xFB, // 0010 EI
    0x23, // 0011 INC HL
    0x18, 0xFD, // 0012 JR $0011
];

/// Where C2b's `EI` starts inside line `$E1`, at the poll's least and greatest slack.
const C2B_EI_IN_LINE: (u64, u64) = {
    let after_poll = mclk(T_LD_A_IND_NN + T_CP_N + T_JR_NOT_TAKEN + T_LD_HL_NN);
    (after_poll, after_poll + POLL_SLACK_MAX)
};

/// **WRONG by the documentation, pinned because it is today's behaviour.** The request is held until the
/// next frame's line 0, so it is still pending at this `EI` on line `$E1` and is taken at once: V = `$E1`,
/// HL = 0. R6's value: with the pulse one line wide the request is gone before this `EI`, so the Z80 takes
/// the NEXT frame's assert, at V = `$E0`, with HL a frame of passes ([`next_frame_hl`]).
const C2B_TODAY: (u8, u16) = (0xE1, 0);

#[test]
fn c2b_int_width_the_request_is_gone_a_fifth_of_a_line_after_one_line() {
    let (ei_lo, _) = C2B_EI_IN_LINE;
    // The probe's premise, from the constants: the EI falls after the documented pulse ends, one line past
    // the assert, and within the line after it.
    assert!(
        ei_lo > ASSERT_IN_LINE && ei_lo < MCLK_PER_LINE,
        "the C2b layout must enable after the documented pulse"
    );
    let (hl_lo, hl_hi) = next_frame_hl(ACTIVE_LINES as u64 + 1, ei_lo);
    let (mut s, _) = released(&[(0, C2B_MAIN), (0x38, C2_ISR)]);
    s.run_frames(3);
    let seen = &s.z80_ram()[OUT..OUT + 5];
    assert_eq!(
        seen[4], 0xAA,
        "UNMEASURABLE: the handler never ran within three frames"
    );
    let got = (seen[2], u16::from_le_bytes([seen[0], seen[1]]));
    assert_eq!(
        got, C2B_TODAY,
        "C2b moved: (V, HL) = ({:02X}, {}) when taken. Pinned ({:02X}, {}) today, WRONG. Documented: V = \
         $E0 and HL in {hl_lo}-{hl_hi} (R6, a one-line /INT: the next frame's assert is taken), which \
         parcel 4 must produce with a cause: line; parcel 3 alone moves HL to 1 (UM0080 p.18)",
        got.0, got.1, C2B_TODAY.0, C2B_TODAY.1
    );
}

// ---- C3: grant timing --------------------------------------------------------------------------------------

/// C3. `DI; LD HL,0`, then forever `INC HL; LD ($1000),HL; JR`: one pass is 6 + 16 + 12 = 34 T = 510 mclk,
/// and the count in Z80 RAM is how many passes' stores the Z80 began in its own time.
const C3_MAIN: &[u8] = &[
    0xF3, // 0000 DI
    0x21, 0x00, 0x00, // 0001 LD HL,0
    0x23, // 0004 INC HL
    0x22, 0x00, 0x10, // 0005 LD ($1000),HL
    0x18, 0xFA, // 0008 JR $0004
];

/// Z80 time from the release to the start of the first pass (`DI`, `LD HL,0`).
const C3_PREAMBLE: u64 = mclk(T_DI + T_LD_HL_NN);
/// Z80 time from the release to the start of the first store (the preamble, then one `INC HL`).
const C3_FIRST_STORE: u64 = C3_PREAMBLE + mclk(T_INC_HL);
/// One pass.
const C3_PASS: u64 = mclk(T_INC_HL + T_LD_IND_NN_HL + T_JR_TAKEN);

/// Whether a bus grant at `own` mclk of the Z80's own time cuts an instruction: its instruction boundaries
/// are the release, the ends of `DI` and `LD HL,0`, then each pass's `INC HL`, store and `JR` ends.
fn c3_cuts_an_instruction(own: u64) -> bool {
    if own <= C3_PREAMBLE {
        return ![0, mclk(T_DI), C3_PREAMBLE].contains(&own);
    }
    let in_pass = (own - C3_PREAMBLE) % C3_PASS;
    ![0, mclk(T_INC_HL), mclk(T_INC_HL + T_LD_IND_NN_HL)].contains(&in_pass)
}

/// **C3 is right today and pinned at the derived value, which is also the documented one.** A Z80 whose bus
/// the 68000 takes and gives back runs exactly its gated-on time: its own clock is the concatenation of the
/// gated-on spans (UM0080's BUSREQ "is always recognized at the end of the current machine cycle", so an
/// instruction a grant cuts finishes after the release, lens M21). A store is begun iff its start, in that
/// own time, falls before the total, so the count is `ceil((G - first store) / pass)`. The spans are this
/// test's own, measured as the 68000's clock moved across them; nothing reads the Z80's frontier.
///
/// The grants vary in length and phase so that most of them cut an instruction mid-way: a schedule of
/// uniform grants could land every edge on a boundary and never exercise the tail M21 is about. There are
/// a hundred so that a one-Z80-clock error per grant (1500 mclk, about 3 passes) cannot hide inside one
/// pass's rounding.
#[test]
fn c3_grant_timing_a_z80_loop_counts_exactly_its_gated_on_time() {
    const GRANTS: u64 = 100;
    let (mut s, _) = released(&[(0, C3_MAIN)]);
    let (mut own, mut cut) = (0u64, 0u64);
    for i in 0..=GRANTS {
        let t = s.scheduler().now();
        s.run_until(t + 20_000 + (i * 7_919) % 13_001);
        own += s.scheduler().now() - t;
        if i == GRANTS {
            break;
        }
        cut += u64::from(c3_cuts_an_instruction(own));
        take_bus(&mut s, true);
        let t = s.scheduler().now();
        s.run_until(t + 2_000 + (i * 3_571) % 9_001);
        take_bus(&mut s, false);
    }
    assert!(
        cut * 2 > GRANTS,
        "UNMEASURABLE: only {cut} of {GRANTS} grants cut an instruction, so there is no tail to test"
    );
    assert!(own > C3_FIRST_STORE, "UNMEASURABLE: the Z80 never ran");
    let expected = (own - C3_FIRST_STORE).div_ceil(C3_PASS);
    let count = u64::from(u16::from_le_bytes([s.z80_ram()[OUT], s.z80_ram()[OUT + 1]]));
    assert_eq!(
        count,
        expected,
        "C3 moved: the Z80 was gated on for {own} mclk across {GRANTS} grants ({cut} cut an instruction), \
         room for {expected} stores of {C3_PASS} mclk after a {C3_FIRST_STORE} mclk preamble; it made \
         {count}, i.e. {} mclk of Z80 time gained or lost",
        (i128::from(count) - i128::from(expected)) * i128::from(C3_PASS)
    );
}

// ---- C4: the /INT level ------------------------------------------------------------------------------------

/// C4. `DI; IM 1; LD SP,$2000; EI`, then `HALT; JR` back to it, so the Z80 idles sampling `/INT` every
/// 4-T slice. It enables long before the first assert.
const C4_MAIN: &[u8] = &[
    0xF3, // 0000 DI
    0xED, 0x56, // 0001 IM 1
    0x31, 0x00, 0x20, // 0003 LD SP,$2000
    0xFB, // 0006 EI
    0x76, // 0007 HALT
    0x18, 0xFD, // 0008 JR $0007
];

/// The C4 handler at `$0038`: count the entry, then re-enable at once, so a still-asserted level takes it
/// again: `LD HL,$1000; INC (HL); EI; RET`.
const C4_ISR: &[u8] = &[
    0x21, 0x00, 0x10, // LD HL,$1000
    0x34, // INC (HL)
    0xFB, // EI
    0xC9, // RET
];

/// **WRONG by R6, medium confidence, pinned because it is today's behaviour.** `Z80::accept_interrupt`
/// consumes the request (`int_pending = false`) and `System` raises it once per frame (`EventKind::VInt`),
/// so however soon the handler re-enables there is nothing left to take: one entry per assert.
const C4_ENTRIES_PER_ASSERT_TODAY: u64 = 1;

/// R6's level model's entries per assert for a handler whose entry-to-re-entry cycle is `cycle` mclk:
/// the line stays asserted one line past the assert, the first acceptance comes `first` mclk after the
/// assert, and each further one a cycle later while the line is still up. The (least, greatest) over the
/// unpinned `first`: up to one test-ROM 68000 instruction late (`LATE_VIEW_MAX`) plus one HALT slice.
fn c4_level_entries(cycle: u64) -> (u64, u64) {
    let fit = |first: u64| (MCLK_PER_LINE - first).div_ceil(cycle);
    (fit(LATE_VIEW_MAX + mclk(T_HALT_SLICE)), fit(0))
}

/// Today's handler cycle, and R6's level with no `EI` delay: acceptance, `LD HL`, `INC (HL)`, `EI`, and the
/// next acceptance comes at the boundary right after `EI`.
const C4_CYCLE_NO_EI_DELAY: u64 = mclk(T_ACCEPT_IM1 + T_LD_HL_NN + T_INC_IND_HL + T_EI);
/// With parcel 3's `EI` delay (UM0080 p.18) the `RET` runs before the next acceptance.
const C4_CYCLE_WITH_EI_DELAY: u64 = C4_CYCLE_NO_EI_DELAY + mclk(T_RET);

/// `"6"` for a range whose ends agree, `"5-6"` for one that does not.
fn span(r: (u64, u64)) -> String {
    if r.0 == r.1 {
        r.0.to_string()
    } else {
        format!("{}-{}", r.0, r.1)
    }
}

#[test]
fn c4_level_a_handler_that_re_enables_inside_the_window_is_entered_once_per_assert_today() {
    let (mut s, release) = released(&[(0, C4_MAIN), (0x38, C4_ISR)]);
    s.run_frames(3);
    let asserts = asserts_between(release, s.scheduler().now());
    assert!(
        asserts >= 2,
        "UNMEASURABLE: only {asserts} whole /INT window(s) fell inside the run"
    );
    let entries = u64::from(s.z80_ram()[OUT]);
    let plain = span(c4_level_entries(C4_CYCLE_NO_EI_DELAY));
    let delayed = span(c4_level_entries(C4_CYCLE_WITH_EI_DELAY));
    assert_eq!(
        entries,
        asserts * C4_ENTRIES_PER_ASSERT_TODAY,
        "C4 moved: {entries} handler entries over {asserts} asserts. Pinned {C4_ENTRIES_PER_ASSERT_TODAY} per \
         assert (today, WRONG by R6). Documented, MEDIUM confidence (R6's level corollary, which parcel 4 \
         needs a ruling on): the line is a level for one line, so this handler is re-entered {plain} times \
         per assert with no EI delay, {delayed} with parcel 3's"
    );
}
