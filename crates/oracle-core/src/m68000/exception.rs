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
