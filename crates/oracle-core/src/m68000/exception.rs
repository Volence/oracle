//! Exception-entry substructure — the shared supervisor-entry / frame-push / vector-fetch / handler-reload
//! tail every 68000 exception runs over the micro-op framework.
//!
//! Two builders compose the standard exception entry: [`push_standard_frame`] stacks the standard 6-byte
//! frame (SR + the saved 32-bit PC) on the supervisor stack, and [`vector_fetch_and_reload`] reads the
//! handler address from the exception vector table and reloads the prefetch queue at it. They are shared by
//! the **planned** entries (decode emits them directly — `TRAP`/`TRAPV`-trap/`CHK`-trap, Shape A) and, from
//! E3 on, by the **execution-time abort** (a faulting micro-op rewrites its own `MicroState` into the
//! 14-byte group-0 frame, Shape B). Keeping the tail in one place is what makes both drivers — and the
//! decode-emitted vs. abort-installed paths — bit-identical by construction.
//!
//! Every cycle placement and bus-transaction order here is TDD-pinned against the vendored SingleStepTests
//! stream (the `TRAP` anchors `4e40`/`4e41`/`4e47`/`4e4f`, all length 34): the standard frame writes in the
//! on-bus order **`PCL @ B+4`, `SR @ B+0`, `PCH @ B+2`** (the real 68000 microcode order), the two vector
//! reads are **FC=5 (supervisor-data)**, and the handler reload is two FC=6 (supervisor-program) prefetches
//! with an `n2` idle between.

use super::ea::RecipeBuf;
use super::microop::{Fc, MicroOp, Operand, Size, Slot};

/// Scratch slot holding the **stacked PC** of the address-error frame (the live `regs.pc` at the fault),
/// seeded by [`install_address_error`](super::microop::MicroState) and stacked as `PCL @ B+12` / `PCH @
/// B+10`. Slot 0.
pub(crate) const AERR_STACKED_PC_SLOT: Slot = 0;

/// Scratch slot holding the **SR captured at the fault** by the frame's [`MicroOp::EnterException`], stacked
/// as `SR @ B+8`. Slot 1 (matches the standard frame's save-SR convention).
pub(crate) const AERR_SAVE_SR_SLOT: Slot = 1;

/// Scratch slot holding the **full 32-bit faulting access address**, seeded by `install_address_error` and
/// stacked as `aLo @ B+4` / `aHi @ B+2`. Slot 2.
pub(crate) const AERR_FAULT_ADDR_SLOT: Slot = 2;

/// Scratch slot holding the **instruction register** (the latched original opcode), stacked as `IR @ B+6`.
/// Slot 8 — **above** the shared [`vector_fetch_and_reload`] slots 3..=7 so it never aliases the vector
/// fetch (the IR write precedes the fetch, but a disjoint slot keeps the frame trivially correct).
pub(crate) const AERR_IR_SLOT: Slot = 8;

/// Scratch slot holding the **special status word** (`(opcode & 0xFFE0) | low5`), stacked as `SSW @ B+0`.
/// Slot 9 — also disjoint from the vector-fetch slots.
pub(crate) const AERR_SSW_SLOT: Slot = 9;

/// Build the group-0 **14-byte address-error frame** recipe (vector 3, `0x0C`) — the execution-time abort's
/// Shape-B tail, installed in place by
/// [`install_address_error`](super::microop::MicroState). Pinned to the vendored address-error stream
/// scattered across all 13 families (`d850`/`d06c`/`dd56`/`3c82`/`6d25`/`d8b9`, lengths 50–58):
///
/// - **Leading `n4` idle**, then [`MicroOp::EnterException`] (capture the live SR + enter supervisor /
///   clear T), then `AdjustAddr(SP,−14)` so A7 = `B` (the post-push stack top).
/// - The **seven frame writes**, in the on-bus order the 68000 microcode uses (NOT the layout order):
///   `PCL @ B+12`, `SR @ B+8`, `PCH @ B+10`, `IR @ B+6`, `aLo @ B+4`, `SSW @ B+0`, `aHi @ B+2` — each a
///   single FC=5 word [`MicroOp::Write`] at an [`Operand::SpPlus`] address (no per-write `EaCalc`, keeping
///   the recipe ≤ `MAX_OPS`). The access address and the PC are stacked as full 32-bit longs (`aHi`/`PCH`
///   the high word via [`Operand::ScratchHi16`], `aLo`/`PCL` the low word, `Write` truncating to 16).
/// - The shared [`vector_fetch_and_reload`] at vector `3*4 = 0x0C`: two FC=5 vector reads, then the FC=6
///   handler reload with the `n2` idle between.
///
/// The seeded scratch (`AERR_*` slots) and `EnterException`'s `AERR_SAVE_SR_SLOT` capture are the only
/// per-fault inputs; the recipe itself is fixed (the abort vector is always 3), so it is rebuilt identically
/// on every fault — both drivers and the snapshot/restore-across-the-abort path stay bit-identical.
pub(crate) fn build_address_error_frame(buf: &mut RecipeBuf) {
    // The leading n4 idle (the abort's only leading cost — the faulting micro-op was free).
    buf.push(MicroOp::Internal { cycles: 4 });
    // Capture the live SR + enter supervisor (set S, clear T).
    buf.push(MicroOp::EnterException {
        save_sr: AERR_SAVE_SR_SLOT,
    });
    // SSP -= 14 — A7 now points at the new stack top B (routed to the supervisor stack via the S bit).
    buf.push(MicroOp::AdjustAddr { reg: 7, delta: -14 });
    // The seven frame writes in the 68000 on-bus order.
    // PCL @ B+12 (written FIRST) — low 16 of the stacked PC.
    buf.push(MicroOp::Write {
        addr: Operand::SpPlus(12),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(AERR_STACKED_PC_SLOT),
    });
    // SR @ B+8 (SECOND) — the SR captured at the fault.
    buf.push(MicroOp::Write {
        addr: Operand::SpPlus(8),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(AERR_SAVE_SR_SLOT),
    });
    // PCH @ B+10 (THIRD) — high 16 of the stacked PC.
    buf.push(MicroOp::Write {
        addr: Operand::SpPlus(10),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::ScratchHi16(AERR_STACKED_PC_SLOT),
    });
    // IR @ B+6 (FOURTH) — the latched original opcode.
    buf.push(MicroOp::Write {
        addr: Operand::SpPlus(6),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(AERR_IR_SLOT),
    });
    // aLo @ B+4 (FIFTH) — low 16 of the full 32-bit access address.
    buf.push(MicroOp::Write {
        addr: Operand::SpPlus(4),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(AERR_FAULT_ADDR_SLOT),
    });
    // SSW @ B+0 (SIXTH) — (opcode & 0xFFE0) | low5.
    buf.push(MicroOp::Write {
        addr: Operand::SpPlus(0),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(AERR_SSW_SLOT),
    });
    // aHi @ B+2 (SEVENTH) — high 16 of the full 32-bit access address.
    buf.push(MicroOp::Write {
        addr: Operand::SpPlus(2),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::ScratchHi16(AERR_FAULT_ADDR_SLOT),
    });
    // The shared vector-3 fetch (FC=5) + handler reload (FC=6, n2 between).
    vector_fetch_and_reload(buf, 3 * 4);
}

/// Scratch slot holding the `CHK` exception's stacked return PC (the live `regs.pc`, seeded by
/// [`install_chk_trap`](super::microop::MicroState)). Slot 0 — the standard-frame saved-PC convention (matches
/// `TRAP`/`TRAPV`), consumed by the frame's `PCL`/`PCH` writes.
pub(crate) const CHK_SAVED_PC_SLOT: Slot = 0;

/// Scratch slot holding the `CHK` exception's saved SR (the live SR at the trap — with CHK's just-set N),
/// captured by the frame's [`MicroOp::EnterException`]. Slot 1 — distinct from the saved-PC slot so both
/// survive until the push.
const CHK_SAVE_SR_SLOT: Slot = 1;

/// Build the standard **6-byte `CHK` frame** recipe (vector 6, `0x18`) — the Shape-B tail an out-of-bounds
/// [`MicroOp::ChkTrap`] installs in place via [`install_chk_trap`](super::microop::MicroState). Pinned to the
/// vendored `CHK` SST stream (`4d91` Dn<0 → n6 len 44; `4396` Dn>bound → n4 len 42; `45bc` `#imm` Dn>bound →
/// n4 len 42):
///
/// - **Leading idle** of `idle` cycles (`n4` when `Dn>bound`, else `n6`), then the frame's
///   [`MicroOp::EnterException`] (capture the live SR — already carrying CHK's N — and enter supervisor / clear
///   T).
/// - The shared [`push_standard_frame`] (`SSP -= 6`; the on-bus write order `PCL @ B+4`, `SR @ B+0`,
///   `PCH @ B+2`, FC=5). The stacked PC is the live `regs.pc` (seeded by the install — `ChkTrap` runs after the
///   source read + prefetch(es), so `regs.pc` already equals the saved return PC).
/// - The shared [`vector_fetch_and_reload`] at vector `6*4 = 0x18`: two FC=5 vector reads, then the FC=6
///   handler reload with the `n2` idle between.
///
/// Unlike `TRAP`/`TRAPV` (Shape-A, decode-emitted), the saved PC is NOT computed by a leading `TargetCalc` —
/// it is seeded into [`CHK_SAVED_PC_SLOT`] by the execution-time install, so this builder takes only the idle
/// width. Total 17 micro-ops (≤ `MAX_OPS`).
pub(crate) fn build_chk_frame(buf: &mut RecipeBuf, idle: u8) {
    // The leading idle (n4 if Dn>bound, else n6) — the only per-trap parameter.
    buf.push(MicroOp::Internal {
        cycles: idle as u16,
    });
    // Capture the live SR (with CHK's N) + enter supervisor (set S, clear T).
    buf.push(MicroOp::EnterException {
        save_sr: CHK_SAVE_SR_SLOT,
    });
    push_standard_frame(buf, CHK_SAVED_PC_SLOT, CHK_SAVE_SR_SLOT);
    // The standard CHK vector is 6 (address 6*4 = 0x18).
    vector_fetch_and_reload(buf, 6 * 4);
}

/// Scratch slot holding the DIVU/DIVS divide-by-zero exception's stacked return PC (the live `regs.pc`, seeded
/// by [`install_div0_trap`](super::microop::MicroState)). Slot 0 — the standard-frame saved-PC convention
/// (same as CHK/TRAP), consumed by the frame's `PCL`/`PCH` writes. For a memory divisor this aliases the
/// operand-read slot, but the [`AluOp::Divu`](super::microop::AluOp::Divu) arm resolves the divisor BEFORE the
/// install seeds this, so the value is read first, written second.
pub(crate) const DIV0_SAVED_PC_SLOT: Slot = 0;

/// Scratch slot holding the div0 exception's saved SR (the live SR at the trap — with the div0 CCR already set
/// by the `Divu` arm), captured by the frame's [`MicroOp::EnterException`]. Slot 1 — distinct from the saved-PC
/// slot so both survive until the push (the same convention as CHK).
const DIV0_SAVE_SR_SLOT: Slot = 1;

/// Build the standard **6-byte divide-by-zero frame** recipe (vector 5, `0x14`) — the Shape-B tail a
/// divide-by-zero [`AluOp::Divu`](super::microop::AluOp::Divu) installs in place via
/// [`install_div0_trap`](super::microop::MicroState). The vector-5 twin of [`build_chk_frame`] (vector 6),
/// pinned to the sole vendored `op=0x80ef` div0 sample (mode 5 `d16(A7)`, leading idle `n8`, pushed CCR
/// `0b10000`, saved PC `0xc00`, len 46):
///
/// - **Leading idle** of `idle` cycles (`n8` for the vendored sample), then the frame's
///   [`MicroOp::EnterException`] (capture the live SR — already carrying the div0 CCR N=Z=V=C=0/X-kept — and
///   enter supervisor / clear T).
/// - The shared [`push_standard_frame`] (`SSP -= 6`; the on-bus write order `PCL @ B+4`, `SR @ B+0`,
///   `PCH @ B+2`, FC=5). The stacked PC is the live `regs.pc` (seeded by the install — the `Divu` Alu runs
///   after the source read + prefetch(es), so `regs.pc` already equals the saved return PC).
/// - The shared [`vector_fetch_and_reload`] at vector `5*4 = 0x14`: two FC=5 vector reads, then the FC=6
///   handler reload with the `n2` idle between.
///
/// Total 17 micro-ops (≤ `MAX_OPS`), identical in shape to the CHK frame — only the vector differs.
pub(crate) fn build_div0_frame(buf: &mut RecipeBuf, idle: u8) {
    // The leading idle (the divide-by-zero detection cost; n8 for the vendored sample).
    buf.push(MicroOp::Internal {
        cycles: idle as u16,
    });
    // Capture the live SR (with the div0 CCR) + enter supervisor (set S, clear T).
    buf.push(MicroOp::EnterException {
        save_sr: DIV0_SAVE_SR_SLOT,
    });
    push_standard_frame(buf, DIV0_SAVED_PC_SLOT, DIV0_SAVE_SR_SLOT);
    // The divide-by-zero vector is 5 (address 5*4 = 0x14).
    vector_fetch_and_reload(buf, 5 * 4);
}

/// Scratch slot holding a transient standard-frame write address (`B+4` for `PCL`, then reused for `B+2` for
/// `PCH`). Each address is consumed by its `Write` before the next `EaCalc` overwrites it. Distinct from a
/// caller's `saved_pc`/`save_sr` slots so the frame values survive until they are pushed.
const FRAME_ADDR_SLOT: Slot = 2;

/// Scratch slot holding the exception **vector address** (`(vector)*4`), materialized by a
/// [`MicroOp::LoadImm`] so the handler-hi `Read` can address it. Reused after the frame push (the saved
/// PC/SR slots are dead by then), so these vector-fetch slots sit above [`FRAME_ADDR_SLOT`].
const VECTOR_ADDR_SLOT: Slot = 3;

/// Scratch slot holding the HIGH word of the handler address (read at the vector address).
const HANDLER_HI_SLOT: Slot = 4;

/// Scratch slot holding the address of the handler's LOW word (`vector_addr + 2`).
const VECTOR_LO_ADDR_SLOT: Slot = 5;

/// Scratch slot holding the LOW word of the handler address (read at `vector_addr + 2`).
const HANDLER_LO_SLOT: Slot = 6;

/// Scratch slot holding the assembled 32-bit handler address (the `SetPc` source).
const HANDLER_SLOT: Slot = 7;

/// Push the standard 6-byte exception frame to the supervisor stack: `SSP -= 6`, then stack the saved PC and
/// SR. At the post-push `B = SSP_after`: `SR @ B+0`, `PC-high @ B+2`, `PC-low @ B+4` (big-endian). The
/// **on-bus write order is `PCL @ B+4` first, `SR @ B+0` second, `PCH @ B+2` third** — the 68000 microcode
/// order, pinned to the vendored `TRAP` stream. `saved_pc_slot` is a scratch slot already holding the
/// (UNMASKED) 32-bit return PC; `save_sr_slot` a scratch slot holding the SR captured at entry (by
/// [`MicroOp::EnterException`]). All three writes are FC=5 (supervisor-data) word accesses; the `AdjustAddr`
/// and the two `EaCalc`s are 0-cycle, non-bus internal steps.
pub fn push_standard_frame(buf: &mut RecipeBuf, saved_pc_slot: Slot, save_sr_slot: Slot) {
    // SSP -= 6 — A7 now points at the new stack top (B), routed to the supervisor stack via the S bit.
    buf.push(MicroOp::AdjustAddr { reg: 7, delta: -6 });
    // PCL @ B+4 (written FIRST). B+4 = A7 + 2 + 2 (WordStep + WordStep).
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::WordStep,
        disp: Operand::WordStep,
        dst: FRAME_ADDR_SLOT,
    });
    buf.push(MicroOp::Write {
        addr: Operand::Scratch(FRAME_ADDR_SLOT),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(saved_pc_slot), // low 16 of the saved PC (Write truncates)
    });
    // SR @ B+0 (written SECOND) — the SR captured at entry.
    buf.push(MicroOp::Write {
        addr: Operand::AddrReg(7),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(save_sr_slot),
    });
    // PCH @ B+2 (written THIRD).
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: FRAME_ADDR_SLOT,
    });
    buf.push(MicroOp::Write {
        addr: Operand::Scratch(FRAME_ADDR_SLOT),
        fc: Fc::Data,
        size: Size::Word,
        value: Operand::ScratchHi16(saved_pc_slot), // high 16 of the saved PC
    });
}

/// Fetch the handler address from the exception vector at `vector_addr` and reload the prefetch queue at it.
/// The two vector reads (`handler-hi @ vector_addr`, `handler-lo @ vector_addr + 2`) are **FC=5
/// (supervisor-data)**; the assembled 32-bit handler address is UNMASKED (a vector may point into high
/// memory — only the bus reload address masks). The reload is the universal taken-branch tail: `SetPc`
/// primes `pc = handler - 4`, then two `Prefetch`s read `handler` / `handler + 2` (FC=6 supervisor-program)
/// with an `n2` idle between (pinned to the `TRAP` stream).
pub fn vector_fetch_and_reload(buf: &mut RecipeBuf, vector_addr: u32) {
    // Stage the vector address into scratch so a plain Read can address it.
    buf.push(MicroOp::LoadImm {
        value: vector_addr,
        dst: VECTOR_ADDR_SLOT,
    });
    // handler-hi @ vector_addr (FC=5).
    buf.push(MicroOp::Read {
        addr: Operand::Scratch(VECTOR_ADDR_SLOT),
        fc: Fc::Data,
        size: Size::Word,
        dst: HANDLER_HI_SLOT,
    });
    // handler-lo @ vector_addr + 2 (FC=5).
    buf.push(MicroOp::EaCalc {
        base: Operand::Scratch(VECTOR_ADDR_SLOT),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: VECTOR_LO_ADDR_SLOT,
    });
    buf.push(MicroOp::Read {
        addr: Operand::Scratch(VECTOR_LO_ADDR_SLOT),
        fc: Fc::Data,
        size: Size::Word,
        dst: HANDLER_LO_SLOT,
    });
    // Assemble the UNMASKED 32-bit handler address (no mask — only the bus reload masks).
    buf.push(MicroOp::Combine32 {
        hi: HANDLER_HI_SLOT,
        lo: Operand::Scratch(HANDLER_LO_SLOT),
        dst: HANDLER_SLOT,
    });
    // The universal queue reload at the handler: SetPc primes pc = handler - 4, the two Prefetch ops read
    // handler / handler+2 (FC=6), with the n2 idle between (pinned to the TRAP stream).
    buf.push(MicroOp::SetPc {
        value: Operand::Scratch(HANDLER_SLOT),
    });
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Internal { cycles: 2 });
    buf.push(MicroOp::Prefetch);
}
