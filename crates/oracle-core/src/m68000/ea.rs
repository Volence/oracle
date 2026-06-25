//! Effective-address machinery — a decode-time builder that emits, per EA mode, the shared sub-sequence
//! of micro-ops that fetches (source) or addresses (destination) an operand.
//!
//! The hybrid design (`docs/plans/2026-06-25-m68000-ea-machinery.md`): a [`RecipeBuf`] stages a recipe in
//! a fixed `[MicroOp; MAX_OPS]` array (no `Vec` — the recipe stays a `Copy`-friendly, bounded template),
//! and [`ea_src`]/[`ea_dst`] push the per-mode bus steps interleaved with the opcode's [`MicroOp::Alu`] at
//! the right [`AluPlacement`]. The placement is load-bearing: it pins *when* the ALU samples the prefetch
//! queue relative to the refills that shift it (e.g. `#imm` must read `prefetch[1]` **before** the two
//! refills that shift the immediate out — [`AluPlacement::First`]).
//!
//! C1 re-expresses the already-covered modes (`Dn` / `(An)` / `#imm` as a source, `Dn,(An)` as a
//! destination) through this builder; a regression test pins the builder's output byte-for-byte against
//! the literal recipes it replaces. Later commits add the remaining modes here, not in `decode`.

use super::bus68k::ADDR_MASK;
use super::microop::{MicroOp, MicroState, Operand, Size, MAX_OPS};
use super::registers::Registers;

/// The computed effective address of a **register-file** EaCalc-based memory EA mode (`d16(An)` = 101,
/// `abs.w` = 111/000, `d16(PC)` = 111/010), masked to the 24-bit bus — the **single shared** address
/// computation backing BOTH the framework (a debug-assert in [`MicroOp::EaCalc`]'s exec arm runs the recipe
/// and compares; the per-mode agreement unit test in this module is the hard gate that the recipe's
/// `EaCalc` deposits exactly this) AND the SST runner's
/// [`covered`](../../../tests/singlestep_m68000.rs) parity filter (a word access to an odd EA is an address
/// error — deferred → xfail). Pure: it reads only `pc` / an address register and the displacement word in
/// `regs.prefetch[1]` (live at decode time, before any refill shifts it out).
///
/// `abs.l` (111/001) is deliberately **excluded**: its low address word lives in RAM (at `pc+4`), not the
/// register file, so it cannot be derived from `regs` alone — both `covered()` and its agreement test
/// assemble the two extension words directly.
///
/// `size` is accepted for forward-compatibility (byte EAs may be odd and are never odd-filtered; word/long
/// odd EAs xfail) but does not change the address arithmetic itself. Panics on a non-EaCalc mode — callers
/// must gate on the mode first.
pub fn compute_ea(opcode: u16, regs: &Registers, size: Size) -> u32 {
    let _ = size;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as usize;
    match (mode, reg) {
        // d16(An): An + sign_extend16(disp).
        (5, _) => {
            regs.addr_reg(reg)
                .wrapping_add(sign_extend16(regs.prefetch[1]))
                & ADDR_MASK
        }
        // abs.w: sign_extend16(disp) alone (no base register).
        (7, 0) => sign_extend16(regs.prefetch[1]) & ADDR_MASK,
        // d16(PC): (pc+2) + sign_extend16(disp) — the base is the extension-word address. `regs.pc` is the
        // opcode address (decode time, before any refill advances it), matching the recipe's `PcOfExt`.
        (7, 2) => {
            regs.pc
                .wrapping_add(2)
                .wrapping_add(sign_extend16(regs.prefetch[1]))
                & ADDR_MASK
        }
        // d8(An,Xn): An + index(Xn) + sign_extend8(disp8) — the brief ext word in prefetch[1] carries both
        // the index spec and the disp8. The index decode (D/A reg file, W/L size, sign-extension) is the
        // SAME as the recipe's `Operand::BriefIndex` resolver, replicated here so `covered()` and the
        // framework compute the identical address (the per-mode agreement test is the hard gate).
        (6, _) => {
            regs.addr_reg(reg)
                .wrapping_add(brief_index(regs))
                .wrapping_add(brief_disp8(regs))
                & ADDR_MASK
        }
        // d8(PC,Xn): (pc+2) + index(Xn) + sign_extend8(disp8) — same as d8(An,Xn) but PC-relative base.
        (7, 3) => {
            regs.pc
                .wrapping_add(2)
                .wrapping_add(brief_index(regs))
                .wrapping_add(brief_disp8(regs))
                & ADDR_MASK
        }
        // (abs.l (7/1) is NOT here: its low word lives in RAM, not the register file — its parity filter
        // assembles the two words directly. compute_ea covers only the register-file EaCalc modes.)
        _ => panic!("compute_ea: mode {mode}/{reg} is not an EaCalc-based EA"),
    }
}

/// Sign-extend a 16-bit value to 32 bits — the `d16`/`abs.w` displacement extension.
#[inline]
fn sign_extend16(v: u16) -> u32 {
    v as i16 as i32 as u32
}

/// The `d8(An,Xn)`/`d8(PC,Xn)` brief-extension **index** value, decoded from `regs.prefetch[1]` exactly as
/// the framework's `Operand::BriefIndex` resolver does (bit15 = D/A reg file, A7-aware; bits14-12 = reg;
/// bit11 = W/L size with sign-extension). The shared decode keeps `compute_ea` (the `covered()` parity
/// filter) bit-identical to the recipe's EaCalc.
#[inline]
fn brief_index(regs: &Registers) -> u32 {
    let ext = regs.prefetch[1];
    let reg = ((ext >> 12) & 7) as usize;
    let raw = if ext & 0x8000 != 0 {
        regs.addr_reg(reg)
    } else {
        regs.d[reg]
    };
    if ext & 0x0800 != 0 {
        raw
    } else {
        sign_extend16(raw as u16)
    }
}

/// The `d8(An,Xn)`/`d8(PC,Xn)` brief-extension **disp8**: `sign_extend8(prefetch[1] & 0xFF)`.
#[inline]
fn brief_disp8(regs: &Registers) -> u32 {
    (regs.prefetch[1] & 0xFF) as u8 as i8 as i32 as u32
}

/// Where an opcode's [`MicroOp::Alu`] sits relative to the prefetch refill(s) the EA emits.
///
/// The 68000 overlaps the operand combine with prefetch; *which* refill it straddles fixes which words
/// are live in the queue when the ALU samples it. Verified against the SST stream (see the plan's
/// source-read table):
/// - [`AluPlacement::First`] — `#imm`: the ALU reads the immediate from `prefetch[1]` **before** the two
///   refills shift it out.
/// - [`AluPlacement::AfterPrefetch`] — `Dn`/`An` direct: no operand read; the single refill happens, then
///   the ALU combines a register.
/// - [`AluPlacement::Last`] — memory operand: the operand is READ, the final refill happens, then the ALU
///   combines the just-read scratch value (the read is always the second-to-last bus event).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AluPlacement {
    /// ALU first, then the prefetch refill(s) (`#imm`).
    First,
    /// One prefetch refill, then the ALU (register-direct source).
    AfterPrefetch,
    /// Operand read, prefetch refill(s), then the ALU (memory source).
    Last,
}

/// A fixed-capacity staging buffer for one instruction's micro-op recipe. Holds up to [`MAX_OPS`] ops in
/// a plain array (no heap, no `Vec`) and hands the filled prefix to [`MicroState::from_ops`].
pub struct RecipeBuf {
    ops: [MicroOp; MAX_OPS],
    len: usize,
}

impl RecipeBuf {
    /// An empty buffer (filler slots are inert `Internal { cycles: 0 }`, identical to what
    /// [`MicroState::from_ops`] pads with, so a built recipe compares equal to the literal one).
    pub fn new() -> Self {
        Self {
            ops: [MicroOp::Internal { cycles: 0 }; MAX_OPS],
            len: 0,
        }
    }

    /// Append one micro-op. Panics if the recipe would exceed [`MAX_OPS`].
    pub fn push(&mut self, op: MicroOp) {
        assert!(self.len < MAX_OPS, "recipe exceeds MAX_OPS");
        self.ops[self.len] = op;
        self.len += 1;
    }

    /// The micro-ops pushed so far, in order.
    pub fn as_ops(&self) -> &[MicroOp] {
        &self.ops[..self.len]
    }

    /// Finalize into the resumable [`MicroState`] the drivers execute.
    pub fn finish(&self) -> MicroState {
        MicroState::from_ops(self.as_ops())
    }
}

impl Default for RecipeBuf {
    fn default() -> Self {
        Self::new()
    }
}

/// One source EA mode, decoded into the pieces the shared assembler interleaves: the optional operand
/// READ (its address), how many `Prefetch` refills the instruction emits (= its word count, invariant 1),
/// the operand the ALU combines, and where the ALU sits relative to the refills ([`AluPlacement`]). This
/// is the per-mode row of the plan's verified source-read table, made data so a single emitter places the
/// ALU rather than each mode re-implementing the interleave.
struct SrcSeq {
    /// An [`MicroOp::EaCalc`] emitted **first** (before any refill), or `None` for modes whose address is
    /// just an address register. It deposits the computed effective address into the EA scratch slot
    /// ([`EA_SLOT`]); the operand READ then targets `Scratch(EA_SLOT)`. Emitted first so a displacement leg
    /// (`Operand::DispWord`) is captured from `prefetch[1]` **before** the first `Prefetch` shifts it out.
    ea_calc: Option<MicroOp>,
    /// Steps emitted **before** the operand READ, in order — the `-(An)` pre-decrement: the `AdjustAddr`
    /// (so the read hits the decremented address) followed by its `Internal(2)` idle. Empty for modes with
    /// no pre-read side effect. At most two slots (the worst in-scope case).
    pre_read: [Option<MicroOp>; 2],
    /// Address of the operand READ, or `None` for register-direct / immediate modes (no operand read).
    read_addr: Option<Operand>,
    /// A step emitted **after** the READ but before the refill(s) — the `(An)+` post-increment `AdjustAddr`
    /// (the read still hits the un-incremented address; the read stays the second-to-last bus event).
    post_read: Option<MicroOp>,
    /// Number of `Prefetch` refills (= instruction word count).
    prefetch: u8,
    /// The operand the ALU combines (the source value).
    operand: Operand,
    /// Where the ALU sits relative to the refill(s).
    placement: AluPlacement,
}

/// The auto-(in/de)crement step magnitude (in bytes) for an `(An)+`/`-(An)` access of `size` on register
/// `reg`. Word is 2; byte is 1 — **except** `(A7)+`/`-(A7)` byte, which steps by 2 so the stack pointer
/// stays even (the in-scope A7 byte rule). Long (4) is deferred.
#[inline]
fn step_bytes(size: Size, reg: u8) -> i8 {
    match size {
        Size::Word => 2,
        Size::Byte => {
            if reg == 7 {
                2
            } else {
                1
            }
        }
    }
}

/// Scratch slot holding a computed effective address (the [`MicroOp::EaCalc`] destination). Slots 0/1 are
/// the operand-read value and the ALU result; the EA gets its own slot so a read and a write to a
/// destination EA hit the same materialized address.
const EA_SLOT: u8 = 2;

/// Scratch slot holding the captured HIGH word of an `abs.l` address (the first of its two extension
/// words), deposited before the interleaved `Prefetch` shifts the LOW word into the queue. Distinct from
/// [`EA_SLOT`] so both halves are snapshot-visible mid-assembly.
const HI_SLOT: u8 = 3;

/// Decode a source EA mode into its [`SrcSeq`]. Covers `Dn` (0), `An` (1, word/long), `(An)` (2),
/// `(An)+` (3), `-(An)` (4), `#imm` (7/4). Other modes land in later commits. `size` selects the
/// auto-(in/de)crement step (word 2; byte 1, or 2 for A7 to keep the SP even) for the `(An)+`/`-(An)`
/// `AdjustAddr`s — the bus shape itself is size-independent.
fn src_seq(mode: u16, reg: u8, size: Size) -> SrcSeq {
    match (mode, reg) {
        // Dn — data-register direct: no operand read; one refill, then combine the register.
        (0, _) => SrcSeq {
            ea_calc: None,
            pre_read: [None, None],
            read_addr: None,
            post_read: None,
            prefetch: 1,
            operand: Operand::DataRegLow16(reg),
            placement: AluPlacement::AfterPrefetch,
        },
        // An — address-register direct (word/long only; LEGAL `ADD.w`/`SUB.w An,Dn`, NOT `ADDA`; byte is
        // illegal and never reaches here, and An is not an alterable destination). Same bus shape as Dn —
        // no operand read, one refill, then combine — but the operand is An's low word. A7 source is fine
        // (no memory access, no address error).
        (1, _) => SrcSeq {
            ea_calc: None,
            pre_read: [None, None],
            read_addr: None,
            post_read: None,
            prefetch: 1,
            operand: Operand::AddrRegLow16(reg),
            placement: AluPlacement::AfterPrefetch,
        },
        // (An) — address-register indirect: read the operand (→ scratch 0), refill, then combine it.
        (2, _) => SrcSeq {
            ea_calc: None,
            pre_read: [None, None],
            read_addr: Some(Operand::AddrReg(reg)),
            post_read: None,
            prefetch: 1,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // (An)+ — postincrement: read at An (→ scratch 0), then post-increment An by the word step (a
        // 0-cycle non-bus `AdjustAddr` after the read so the read still hits the un-incremented address),
        // refill, then combine. Same `[READ, PF]` bus stream as `(An)` (the bump is invisible to the bus),
        // and the same 8 cycles.
        (3, _) => SrcSeq {
            ea_calc: None,
            pre_read: [None, None],
            read_addr: Some(Operand::AddrReg(reg)),
            post_read: Some(MicroOp::AdjustAddr {
                reg,
                delta: step_bytes(size, reg),
            }),
            prefetch: 1,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // -(An) — predecrement: decrement An by the word step FIRST (so the read hits An-step), then an
        // internal 2-cycle idle (the predecrement penalty; non-bus, pinned by the SST `n` cycle), then read
        // at the decremented An, refill, combine. `[READ, PF]` bus stream, 10 cycles (8 + the idle 2).
        (4, _) => SrcSeq {
            ea_calc: None,
            pre_read: [
                Some(MicroOp::AdjustAddr {
                    reg,
                    delta: -step_bytes(size, reg),
                }),
                Some(MicroOp::Internal { cycles: 2 }),
            ],
            read_addr: Some(Operand::AddrReg(reg)),
            post_read: None,
            prefetch: 1,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // d16(An) — register indirect with displacement: EaCalc(An + sign_extend(disp)) → EA scratch
        // BEFORE the first refill (the disp is in prefetch[1] now; the refill would shift it out). Then the
        // 2-word stream `[PF, READ, PF]` — one refill, the operand read at the computed EA, the final
        // refill — and combine. 12 cycles (3 word accesses; EaCalc/ALU are 0-cycle).
        (5, _) => SrcSeq {
            ea_calc: Some(MicroOp::EaCalc {
                base: Operand::AddrReg(reg),
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: EA_SLOT,
            }),
            pre_read: [None, None],
            read_addr: Some(Operand::Scratch(EA_SLOT)),
            post_read: None,
            prefetch: 2,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // d8(An,Xn) — register indirect with index + 8-bit displacement: EaCalc(An + index(Xn) +
        // sign_extend8(disp8)) → EA scratch BEFORE the first refill (the brief ext word is in prefetch[1]
        // now; it carries BOTH the index spec and the disp8, and the refill would shift it out). Then the
        // indexed-mode `Internal(2)` idle (non-bus penalty, pinned by the SST `n` cycle), then the 2-word
        // stream `[PF, READ, PF]` and combine. 14 cycles (3 word accesses + the 2-cycle idle). The brief
        // ext word's data/addr-index, word/long-size and sign-extension are resolved entirely inside the
        // `Operand::BriefIndex` resolver — the one isolated runtime branch.
        (6, _) => SrcSeq {
            ea_calc: Some(MicroOp::EaCalc {
                base: Operand::AddrReg(reg),
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: EA_SLOT,
            }),
            pre_read: [Some(MicroOp::Internal { cycles: 2 }), None],
            read_addr: Some(Operand::Scratch(EA_SLOT)),
            post_read: None,
            prefetch: 2,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // abs.w — absolute short: the EA is the sign-extended extension word itself (base/index inert).
        // Same `[PF, READ, PF]` 2-word stream and 12 cycles as d16(An).
        (7, 0) => SrcSeq {
            ea_calc: Some(MicroOp::EaCalc {
                base: Operand::Zero,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: EA_SLOT,
            }),
            pre_read: [None, None],
            read_addr: Some(Operand::Scratch(EA_SLOT)),
            post_read: None,
            prefetch: 2,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // d16(PC) — program-counter indirect with displacement: EaCalc((pc+2) + sign_extend(disp)) → EA
        // scratch BEFORE the first refill (the PC base is the extension-word address `pc+2`, and the disp is
        // in prefetch[1] now; the refill would advance pc and shift the disp out). Same `[PF, READ, PF]`
        // 2-word stream and 12 cycles as d16(An) — only the base leg differs (`PcOfExt` not `AddrReg`).
        // PC-relative is source-only (not alterable); no destination form.
        (7, 2) => SrcSeq {
            ea_calc: Some(MicroOp::EaCalc {
                base: Operand::PcOfExt,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: EA_SLOT,
            }),
            pre_read: [None, None],
            read_addr: Some(Operand::Scratch(EA_SLOT)),
            post_read: None,
            prefetch: 2,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // d8(PC,Xn) — program-counter indirect with index + 8-bit displacement: EaCalc((pc+2) + index(Xn)
        // + sign_extend8(disp8)) → EA scratch BEFORE the first refill (the PC base is the extension-word
        // address `pc+2`; the refill would advance pc and shift the brief ext word out). Same shape and 14
        // cycles as d8(An,Xn) — only the base leg differs (`PcOfExt` not `AddrReg`). Source-only (PC-relative
        // is not alterable; no destination form).
        (7, 3) => SrcSeq {
            ea_calc: Some(MicroOp::EaCalc {
                base: Operand::PcOfExt,
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: EA_SLOT,
            }),
            pre_read: [Some(MicroOp::Internal { cycles: 2 }), None],
            read_addr: Some(Operand::Scratch(EA_SLOT)),
            post_read: None,
            prefetch: 2,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // #imm — the immediate is the queued word; the ALU captures `prefetch[1]` BEFORE the two refills
        // shift it out (placement First), then both refills run (the 2-word instruction's fetch).
        (7, 4) => SrcSeq {
            ea_calc: None,
            pre_read: [None, None],
            read_addr: None,
            post_read: None,
            prefetch: 2,
            operand: Operand::ImmWord,
            placement: AluPlacement::First,
        },
        _ => todo!("ea_src mode {mode}/{reg} not yet covered"),
    }
}

/// Push the `abs.l` two-word address assembly into `buf`, leaving the full 24-bit effective address in
/// [`EA_SLOT`]. The HIGH word is captured from `prefetch[1]` (via [`Operand::ExtWordHi`]) into [`HI_SLOT`]
/// **before** the interleaved `Prefetch` shifts the LOW word into the queue; the LOW word is then read from
/// `prefetch[1]` (via [`Operand::ExtWordRaw`]) **after** that refill — never from the refill's bus-return
/// value (which would double-count the queue). Emits `[EaCalc(HI), Prefetch, EaCalc(ADDR)]` — the first of
/// the instruction's three refills (the second and third bracket the operand access, placed by the caller).
fn push_abs_l_addr(buf: &mut RecipeBuf) {
    buf.push(MicroOp::EaCalc {
        base: Operand::Zero,
        index: Operand::Zero,
        disp: Operand::ExtWordHi,
        dst: HI_SLOT,
    });
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::EaCalc {
        base: Operand::Scratch(HI_SLOT),
        index: Operand::Zero,
        disp: Operand::ExtWordRaw,
        dst: EA_SLOT,
    });
}

/// Push the source-EA sub-sequence for an `<ea>,Dn`-shaped instruction: the bus steps that fetch the
/// source operand, interleaved with the prefetch refill(s) and the opcode's ALU. `make_alu` builds the
/// `MicroOp::Alu` given the operand the ALU combines (a register operand, or `Scratch(0)` for a memory
/// read) — the op/size/destination are the caller's, only the source operand and its placement are the
/// EA's concern. The [`AluPlacement`] from [`src_seq`] is the load-bearing pivot the emitter honors.
///
/// Covers source modes `Dn` (0), `An` (1, word/long), `(An)` (2), `(An)+` (3), `-(An)` (4), `d16(An)` (5),
/// `abs.w` (7/0), `abs.l` (7/1), `d16(PC)` (7/2), `#imm` (7/4). Other modes land in later commits. `size`
/// sizes the operand READ (byte → a `read8`, zero-extended into scratch) and the auto-(in/de)crement step.
pub fn ea_src(
    buf: &mut RecipeBuf,
    mode: u16,
    reg: u8,
    size: Size,
    make_alu: impl FnOnce(Operand) -> MicroOp,
) {
    // abs.l — a 3-word instruction: assemble the two-word address first (HIGH then, after a refill, LOW),
    // then the `[READ, Prefetch]` operand access. The two-EaCalc interleave doesn't fit `SrcSeq`'s single
    // `ea_calc` leg, so it's emitted directly here. Bus: [PF, PF, READ, PF].
    if (mode, reg) == (7, 1) {
        let alu = make_alu(Operand::Scratch(0));
        push_abs_l_addr(buf); // [EaCalc(HI), Prefetch, EaCalc(ADDR)] — the first of three refills
        buf.push(MicroOp::Prefetch); // the second refill, before the operand read (read = 2nd-to-last bus)
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(EA_SLOT),
            fc: super::microop::Fc::Data,
            size,
            dst: 0,
        });
        buf.push(MicroOp::Prefetch); // the third (final) refill, trailing the operand read
        buf.push(alu);
        return;
    }
    let seq = src_seq(mode, reg, size);
    let alu = make_alu(seq.operand);
    // The EA computation (if any) runs FIRST so a displacement leg captures `prefetch[1]` before any refill
    // shifts it out (invariant 2). The operand READ then targets the EA scratch slot it deposited.
    if let Some(ea_calc) = seq.ea_calc {
        buf.push(ea_calc);
    }
    // Pre-read side effects (the `-(An)` predecrement `AdjustAddr` + its `Internal(2)` idle) run next, so
    // the read hits the decremented address.
    for op in seq.pre_read.into_iter().flatten() {
        buf.push(op);
    }
    match seq.placement {
        // ALU first, then every refill (#imm captures prefetch[1] before the refills shift it out).
        AluPlacement::First => {
            buf.push(alu);
            for _ in 0..seq.prefetch {
                buf.push(MicroOp::Prefetch);
            }
        }
        // One refill, then the ALU (register-direct: a single fetch, no operand read).
        AluPlacement::AfterPrefetch => {
            debug_assert_eq!(seq.prefetch, 1, "AfterPrefetch is the 1-word direct form");
            buf.push(MicroOp::Prefetch);
            buf.push(alu);
        }
        // Memory operand: the verified bus stream is (k−1) refills → operand READ → 1 final refill (the
        // read is always the second-to-last bus event, invariant 3). So emit `prefetch − 1` refills, then
        // the read (+ its post-read bump), then the final refill, then the ALU on the just-read scratch.
        AluPlacement::Last => {
            let read = MicroOp::Read {
                addr: seq.read_addr.expect("memory-source mode must read"),
                fc: super::microop::Fc::Data,
                size,
                dst: 0,
            };
            for _ in 0..seq.prefetch.saturating_sub(1) {
                buf.push(MicroOp::Prefetch);
            }
            buf.push(read);
            // The `(An)+` postincrement runs after the read (the read hit the un-incremented address) but
            // before the final refill; the bump is non-bus, so the stream is unchanged.
            if let Some(op) = seq.post_read {
                buf.push(op);
            }
            buf.push(MicroOp::Prefetch);
            buf.push(alu);
        }
    }
}

/// Push the destination-EA sub-sequence for a `Dn,<ea>` (memory-destination) read-modify-write: read the
/// old memory value, refill prefetch, combine via the ALU, write the result back. `make_alu` builds the
/// `MicroOp::Alu` given the memory operand (the minuend) and the scratch destination it writes; the write
/// then stores that scratch slot at the same address.
///
/// Covers the alterable-memory destination modes `(An)` (2), `(An)+` (3), `-(An)` (4), `d16(An)` (5),
/// `abs.w` (7/0) and `abs.l` (7/1); the indexed `d8(An,Xn)` mode lands in a later commit. For `(An)+`/`-(An)`
/// the register side effect is an explicit `AdjustAddr`: predecrement runs **before** the read (so the read
/// and write both hit the decremented address), postincrement runs **after** the write (so both hit the
/// un-incremented address). `size` sizes the read/write (byte → `read8`/`write8`) and the step.
pub fn ea_dst(
    buf: &mut RecipeBuf,
    mode: u16,
    reg: u8,
    size: Size,
    make_alu: impl FnOnce(Operand) -> MicroOp,
) {
    // The alterable-memory destination skeleton: read old value → refill → ALU (memory is the minuend) →
    // write the result back, at the same `(An)` address. `(An)+`/`-(An)` wrap this with an `AdjustAddr`.
    let read = MicroOp::Read {
        addr: Operand::AddrReg(reg),
        fc: super::microop::Fc::Data,
        size,
        dst: 0,
    };
    let write = MicroOp::Write {
        addr: Operand::AddrReg(reg),
        fc: super::microop::Fc::Data,
        size,
        value: Operand::Scratch(1),
    };
    match (mode, reg) {
        // (An): read, refill, ALU → scratch 1, write back. Collapses to today's exact `Dn,(An)` recipe.
        (2, _) => {
            buf.push(read);
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
            buf.push(write);
        }
        // (An)+: same RMW at An, then post-increment An (0-cycle, after the write). Read and write both
        // hit the un-incremented address; the bump is invisible to the bus stream.
        (3, _) => {
            buf.push(read);
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
            buf.push(write);
            buf.push(MicroOp::AdjustAddr {
                reg,
                delta: step_bytes(size, reg),
            });
        }
        // -(An): pre-decrement An (then the internal idle), so the read and write both hit An-step.
        (4, _) => {
            buf.push(MicroOp::AdjustAddr {
                reg,
                delta: -step_bytes(size, reg),
            });
            buf.push(MicroOp::Internal { cycles: 2 });
            buf.push(read);
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
            buf.push(write);
        }
        // d16(An) / abs.w: compute the EA into the EA scratch slot FIRST (the disp is in prefetch[1] now;
        // the refill would shift it out), then the 2-word RMW stream `[EaCalc, PF, READ, PF, Alu, WRITE]`
        // (read and write both target the materialized EA scratch). 16 cycles (4 word accesses).
        (5, _) | (7, 0) => {
            let ea_calc = if mode == 5 {
                MicroOp::EaCalc {
                    base: Operand::AddrReg(reg),
                    index: Operand::Zero,
                    disp: Operand::DispWord,
                    dst: EA_SLOT,
                }
            } else {
                MicroOp::EaCalc {
                    base: Operand::Zero,
                    index: Operand::Zero,
                    disp: Operand::DispWord,
                    dst: EA_SLOT,
                }
            };
            let read_ea = MicroOp::Read {
                addr: Operand::Scratch(EA_SLOT),
                fc: super::microop::Fc::Data,
                size,
                dst: 0,
            };
            let write_ea = MicroOp::Write {
                addr: Operand::Scratch(EA_SLOT),
                fc: super::microop::Fc::Data,
                size,
                value: Operand::Scratch(1),
            };
            buf.push(ea_calc);
            buf.push(MicroOp::Prefetch);
            buf.push(read_ea);
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
            buf.push(write_ea);
        }
        // d8(An,Xn): compute the EA (An + index(Xn) + sign_extend8(disp8)) into the EA scratch slot FIRST
        // (the brief ext word — index spec + disp8 — is in prefetch[1] now; the refill would shift it out),
        // then the indexed-mode `Internal(2)` idle, then the 2-word RMW stream `[PF, READ, PF, WRITE]` at the
        // materialized EA (read and write hit the SAME scratch). 18 cycles (4 word accesses + the idle 2).
        (6, _) => {
            buf.push(MicroOp::EaCalc {
                base: Operand::AddrReg(reg),
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: EA_SLOT,
            });
            buf.push(MicroOp::Internal { cycles: 2 });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Read {
                addr: Operand::Scratch(EA_SLOT),
                fc: super::microop::Fc::Data,
                size,
                dst: 0,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
            buf.push(MicroOp::Write {
                addr: Operand::Scratch(EA_SLOT),
                fc: super::microop::Fc::Data,
                size,
                value: Operand::Scratch(1),
            });
        }
        // abs.l: assemble the two-word address (HIGH, refill, LOW) into the EA scratch slot, then the RMW at
        // that materialized EA. 3-word instruction → 3 Prefetch total; bus `[PF, PF, READ, PF, WRITE]`.
        (7, 1) => {
            push_abs_l_addr(buf); // [EaCalc(HI), Prefetch, EaCalc(ADDR)] — the first of three refills
            buf.push(MicroOp::Prefetch); // the second refill, before the read (read = 2nd-to-last bus)
            buf.push(MicroOp::Read {
                addr: Operand::Scratch(EA_SLOT),
                fc: super::microop::Fc::Data,
                size,
                dst: 0,
            });
            buf.push(MicroOp::Prefetch); // the third (final) refill, trailing the read
            buf.push(make_alu(Operand::Scratch(0)));
            buf.push(MicroOp::Write {
                addr: Operand::Scratch(EA_SLOT),
                fc: super::microop::Fc::Data,
                size,
                value: Operand::Scratch(1),
            });
        }
        _ => todo!("ea_dst mode {mode}/{reg} not yet covered"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::m68000::microop::{AluOp, Dest, Fc, MicroState, Size};

    /// The opcode's ALU as a literal, for the regression fixtures. `<ea>,Dn` form: `Dn` is the minuend
    /// (`a`), the source EA supplies `b`, the result lands back in `Dn`.
    fn ea_dn_alu(dn: u8, b: Operand) -> MicroOp {
        MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Word,
            a: Operand::DataRegLow16(dn),
            b,
            dst: Dest::DataRegLow16(dn),
        }
    }

    /// The opcode's ALU as a literal for the `Dn,(An)` form: the memory operand (`a`) is the minuend,
    /// `Dn` is `b`, the result lands in scratch 1 (then written back to memory).
    fn dn_ea_alu(dn: u8, a: Operand) -> MicroOp {
        MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Word,
            a,
            b: Operand::DataRegLow16(dn),
            dst: Dest::Scratch(1),
        }
    }

    fn build_src(mode: u16, reg: u8, dn: u8) -> MicroState {
        let mut buf = RecipeBuf::new();
        ea_src(&mut buf, mode, reg, Size::Word, |b| ea_dn_alu(dn, b));
        buf.finish()
    }

    fn build_dst(mode: u16, reg: u8, dn: u8) -> MicroState {
        let mut buf = RecipeBuf::new();
        ea_dst(&mut buf, mode, reg, Size::Word, |a| dn_ea_alu(dn, a));
        buf.finish()
    }

    // --- Regression guard: the builder emits byte-for-byte the SAME micro-op sequences as today's
    // literal recipes for the C1 modes (Dn, (An), #imm source; Dn,(An) dest). Any drift fails here. ---

    #[test]
    fn builder_matches_literal_dn_source() {
        // <op>.w Dn,Dn → [Prefetch, Alu(b=DataRegLow16(reg))].
        let literal =
            MicroState::from_ops(&[MicroOp::Prefetch, ea_dn_alu(3, Operand::DataRegLow16(5))]);
        assert_eq!(build_src(0, 5, 3), literal);
    }

    #[test]
    fn builder_matches_literal_an_direct_source() {
        // <op>.w An,Dn (mode 1) → [Prefetch, Alu(b=AddrRegLow16(reg))]: same shape as Dn-direct, but the
        // source operand is An's low word. Word/long only (no byte — ADD.b An,Dn is illegal).
        let literal =
            MicroState::from_ops(&[MicroOp::Prefetch, ea_dn_alu(6, Operand::AddrRegLow16(7))]);
        assert_eq!(build_src(1, 7, 6), literal);
    }

    #[test]
    fn builder_matches_literal_an_indirect_source() {
        // <op>.w (An),Dn → [Read(AddrReg(reg)), Prefetch, Alu(b=Scratch(0))].
        let literal = MicroState::from_ops(&[
            MicroOp::Read {
                addr: Operand::AddrReg(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(4, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(2, 2, 4), literal);
    }

    #[test]
    fn builder_matches_literal_an_postinc_source() {
        // <op>.w (An)+,Dn (mode 3) → [Read(AddrReg), AdjustAddr(+2), Prefetch, Alu(b=Scratch(0))].
        // The operand is read at An, An is then post-incremented by the word step; the read is still the
        // second-to-last bus event (invariant 3); the AdjustAddr is a 0-cycle non-bus step.
        let literal = MicroState::from_ops(&[
            MicroOp::Read {
                addr: Operand::AddrReg(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::AdjustAddr { reg: 2, delta: 2 },
            MicroOp::Prefetch,
            ea_dn_alu(4, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(3, 2, 4), literal);
    }

    #[test]
    fn builder_matches_literal_an_predec_source() {
        // <op>.w -(An),Dn (mode 4) → [AdjustAddr(-2), Internal(2), Read(AddrReg), Prefetch, Alu].
        // An is pre-decremented first (so the read hits An-2), an internal 2-cycle idle precedes the read.
        let literal = MicroState::from_ops(&[
            MicroOp::AdjustAddr { reg: 5, delta: -2 },
            MicroOp::Internal { cycles: 2 },
            MicroOp::Read {
                addr: Operand::AddrReg(5),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(1, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(4, 5, 1), literal);
    }

    #[test]
    fn builder_matches_literal_an_postinc_destination() {
        // <op>.w Dn,(An)+ (mode 3) → [Read(AddrReg), Prefetch, Alu, Write(AddrReg), AdjustAddr(+2)].
        // Read and write hit the same (un-incremented) An; An is post-incremented after the write.
        let literal = MicroState::from_ops(&[
            MicroOp::Read {
                addr: Operand::AddrReg(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            dn_ea_alu(3, Operand::Scratch(0)),
            MicroOp::Write {
                addr: Operand::AddrReg(2),
                fc: Fc::Data,
                size: Size::Word,
                value: Operand::Scratch(1),
            },
            MicroOp::AdjustAddr { reg: 2, delta: 2 },
        ]);
        assert_eq!(build_dst(3, 2, 3), literal);
    }

    #[test]
    fn builder_matches_literal_an_predec_destination() {
        // <op>.w Dn,-(An) (mode 4) → [AdjustAddr(-2), Internal(2), Read(AddrReg), Prefetch, Alu,
        // Write(AddrReg)]. An is pre-decremented before the read so read and write both hit An-2.
        let literal = MicroState::from_ops(&[
            MicroOp::AdjustAddr { reg: 1, delta: -2 },
            MicroOp::Internal { cycles: 2 },
            MicroOp::Read {
                addr: Operand::AddrReg(1),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            dn_ea_alu(6, Operand::Scratch(0)),
            MicroOp::Write {
                addr: Operand::AddrReg(1),
                fc: Fc::Data,
                size: Size::Word,
                value: Operand::Scratch(1),
            },
        ]);
        assert_eq!(build_dst(4, 1, 6), literal);
    }

    #[test]
    fn builder_matches_literal_imm_source() {
        // <op>.w #imm,Dn → [Alu(b=ImmWord), Prefetch, Prefetch] (ALU First).
        let literal = MicroState::from_ops(&[
            ea_dn_alu(7, Operand::ImmWord),
            MicroOp::Prefetch,
            MicroOp::Prefetch,
        ]);
        assert_eq!(build_src(7, 4, 7), literal);
    }

    #[test]
    fn builder_matches_literal_d16_an_source() {
        // <op>.w d16(An),Dn (mode 5) →
        //   [EaCalc(AddrReg,·,DispWord)→scratch 2, Prefetch, Read(Scratch(2))→0, Prefetch, Alu(b=Scratch(0))].
        // The displacement is captured by EaCalc from prefetch[1] BEFORE the first Prefetch shifts it; the
        // operand READ (at the computed EA) is the second-to-last bus event (invariant 3); two prefetches
        // total (a 2-word instruction).
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::AddrReg(3),
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: 2,
            },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(4, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(5, 3, 4), literal);
    }

    #[test]
    fn builder_matches_literal_abs_w_source() {
        // <op>.w (xxx).w,Dn (mode 7/0) →
        //   [EaCalc(·,·,DispWord)→scratch 2, Prefetch, Read(Scratch(2))→0, Prefetch, Alu(b=Scratch(0))].
        // The abs.w address is the sign-extended extension word — base and index are both inert (Zero).
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::Zero,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: 2,
            },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(1, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(7, 0, 1), literal);
    }

    #[test]
    fn builder_matches_literal_d16_pc_source() {
        // <op>.w d16(PC),Dn (mode 7/2) →
        //   [EaCalc(PcOfExt,·,DispWord)→2, Prefetch, Read(Scratch(2))→0, Prefetch, Alu(b=Scratch(0))].
        // PC-relative base = pc+2 (the extension-word address), captured by EaCalc BEFORE the first refill;
        // same `[PF, READ, PF]` 2-word stream as d16(An), only the base differs.
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::PcOfExt,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: 2,
            },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(4, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(7, 2, 4), literal);
    }

    #[test]
    fn builder_matches_literal_d8_an_xn_source() {
        // <op>.w d8(An,Xn),Dn (mode 6) →
        //   [EaCalc(AddrReg,BriefIndex,BriefDisp8)→2, Internal(2), Prefetch, Read(Scratch(2))→0, Prefetch,
        //    Alu(b=Scratch(0))].
        // The brief extension word (in prefetch[1]) supplies BOTH the index leg and the disp8 leg; EaCalc
        // captures them BEFORE the first Prefetch shifts it out. The Internal(2) idle is the indexed-mode
        // penalty (non-bus); the bus stream is [PF, READ, PF] (read = second-to-last bus event), 14 cycles.
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::AddrReg(5),
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: 2,
            },
            MicroOp::Internal { cycles: 2 },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(0, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(6, 5, 0), literal);
    }

    #[test]
    fn builder_matches_literal_d8_pc_xn_source() {
        // <op>.w d8(PC,Xn),Dn (mode 7/3) →
        //   [EaCalc(PcOfExt,BriefIndex,BriefDisp8)→2, Internal(2), Prefetch, Read(Scratch(2))→0, Prefetch,
        //    Alu(b=Scratch(0))].
        // Same shape as d8(An,Xn) but the base is the extension-word address (pc+2), captured BEFORE any
        // refill advances pc. PC-relative is source-only (not alterable). 14 cycles.
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::PcOfExt,
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: 2,
            },
            MicroOp::Internal { cycles: 2 },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(4, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(7, 3, 4), literal);
    }

    #[test]
    fn builder_matches_literal_d8_an_xn_destination() {
        // <op>.w Dn,d8(An,Xn) (mode 6) →
        //   [EaCalc(AddrReg,BriefIndex,BriefDisp8)→2, Internal(2), Prefetch, Read(Scratch(2))→0, Prefetch,
        //    Alu, Write(Scratch(2))].
        // Read and write hit the SAME materialized EA (scratch 2). Bus [PF, READ, PF, WRITE], 18 cycles.
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::AddrReg(6),
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: 2,
            },
            MicroOp::Internal { cycles: 2 },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            dn_ea_alu(2, Operand::Scratch(0)),
            MicroOp::Write {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                value: Operand::Scratch(1),
            },
        ]);
        assert_eq!(build_dst(6, 6, 2), literal);
    }

    #[test]
    fn builder_matches_literal_abs_l_source() {
        // <op>.w (xxx).l,Dn (mode 7/1) — a 3-word instruction (3 Prefetch TOTAL). The address is assembled
        // from two extension words: HIGH captured from prefetch[1] first, the interleaved Prefetch shifts
        // the LOW word in, the second EaCalc adds it. The LOW word comes from prefetch[1] AFTER that refill,
        // NEVER from the refill's bus-return value. Bus: [PF, PF, READ, PF].
        //   [EaCalc(·,·,ExtWordHi)→3, Prefetch, EaCalc(Scratch(3),·,ExtWordRaw)→2, Prefetch,
        //    Read(Scratch(2))→0, Prefetch, Alu(b=Scratch(0))].
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::Zero,
                index: Operand::Zero,
                disp: Operand::ExtWordHi,
                dst: 3,
            },
            MicroOp::Prefetch,
            MicroOp::EaCalc {
                base: Operand::Scratch(3),
                index: Operand::Zero,
                disp: Operand::ExtWordRaw,
                dst: 2,
            },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(5, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(7, 1, 5), literal);
    }

    #[test]
    fn builder_matches_literal_abs_l_destination() {
        // <op>.w Dn,(xxx).l (mode 7/1) → the abs.l two-word EA assembly, then the RMW at the materialized
        // EA: read old → final refill → ALU → write back. Bus: [PF, PF, READ, PF, WRITE].
        //   [EaCalc(·,·,ExtWordHi)→3, Prefetch, EaCalc(Scratch(3),·,ExtWordRaw)→2, Prefetch,
        //    Read(Scratch(2))→0, Prefetch, Alu, Write(Scratch(2))].
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::Zero,
                index: Operand::Zero,
                disp: Operand::ExtWordHi,
                dst: 3,
            },
            MicroOp::Prefetch,
            MicroOp::EaCalc {
                base: Operand::Scratch(3),
                index: Operand::Zero,
                disp: Operand::ExtWordRaw,
                dst: 2,
            },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            dn_ea_alu(6, Operand::Scratch(0)),
            MicroOp::Write {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                value: Operand::Scratch(1),
            },
        ]);
        assert_eq!(build_dst(7, 1, 6), literal);
    }

    #[test]
    fn builder_matches_literal_d16_an_destination() {
        // <op>.w Dn,d16(An) (mode 5) →
        //   [EaCalc(AddrReg,·,DispWord)→2, Prefetch, Read(Scratch(2))→0, Prefetch, Alu, Write(Scratch(2))].
        // Read and write hit the SAME computed EA (scratch 2); the disp is captured before the refills.
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::AddrReg(2),
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: 2,
            },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            dn_ea_alu(2, Operand::Scratch(0)),
            MicroOp::Write {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                value: Operand::Scratch(1),
            },
        ]);
        assert_eq!(build_dst(5, 2, 2), literal);
    }

    #[test]
    fn builder_matches_literal_abs_w_destination() {
        // <op>.w Dn,(xxx).w (mode 7/0) →
        //   [EaCalc(·,·,DispWord)→2, Prefetch, Read(Scratch(2))→0, Prefetch, Alu, Write(Scratch(2))].
        let literal = MicroState::from_ops(&[
            MicroOp::EaCalc {
                base: Operand::Zero,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: 2,
            },
            MicroOp::Prefetch,
            MicroOp::Read {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            dn_ea_alu(5, Operand::Scratch(0)),
            MicroOp::Write {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
                value: Operand::Scratch(1),
            },
        ]);
        assert_eq!(build_dst(7, 0, 5), literal);
    }

    #[test]
    fn builder_matches_literal_dn_an_destination() {
        // <op>.w Dn,(An) → [Read(AddrReg), Prefetch, Alu(a=Scratch(0)→Scratch(1)), Write(AddrReg)].
        let literal = MicroState::from_ops(&[
            MicroOp::Read {
                addr: Operand::AddrReg(6),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            dn_ea_alu(1, Operand::Scratch(0)),
            MicroOp::Write {
                addr: Operand::AddrReg(6),
                fc: Fc::Data,
                size: Size::Word,
                value: Operand::Scratch(1),
            },
        ]);
        assert_eq!(build_dst(2, 6, 1), literal);
    }

    // --- HARD GATE: the recipe's EaCalc deposits exactly `compute_ea` (the shared helper backing both the
    // framework and the SST runner's parity filter). Black-box: build the *real* recipe the decoder
    // produces, run it, and read the operand-READ transaction address off the bus — that IS the EaCalc
    // result. Any drift between the builder's EaCalc legs and `compute_ea` fails here. ---

    use crate::m68000::bus68k::{FlatBus, TxKind};
    use crate::m68000::registers::{Registers, SR_SUPERVISOR};

    fn agreement_regs(disp: u16, an: u32) -> Registers {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            pc: 0x0C00,
            sr: SR_SUPERVISOR,
            // prefetch[0] = opcode (unused by the standalone recipe run); prefetch[1] = the disp word the
            // EaCalc captures.
            prefetch: [0, disp],
        };
        regs.a[3] = an;
        regs
    }

    /// The data-space operand-READ address from running a built source recipe — the materialized EA.
    fn read_addr_of(recipe: &MicroState, regs: &Registers) -> u32 {
        let mut st = recipe.clone();
        let mut bus = FlatBus::new();
        st.run_to_completion(&mut regs.clone(), &mut bus);
        bus.log
            .iter()
            .find(|t| t.kind == TxKind::Read && t.fc == 5) // FC 5 = supervisor data = the operand read
            .expect("a memory-source recipe makes one data read")
            .addr
    }

    #[test]
    fn ea_calc_recipe_agrees_with_compute_ea_d16_an() {
        // d16(A3),D4 — opcode 1101 100 0 01 101 011 = 0xD86B. EA = A3 + sign_extend16(disp).
        let opcode = 0xD86B;
        for (disp, an) in [
            (0x02F6u16, 0x0045_7E36u32), // +758
            (0xFFF8, 0x0010_0010),       // -8
            (0x8000, 0x0080_0000),       // large negative
            (0x7FFF, 0x0000_0001),       // large positive (odd EA — still must agree)
        ] {
            let regs = agreement_regs(disp, an);
            let mut buf = RecipeBuf::new();
            ea_src(&mut buf, 5, 3, Size::Word, |b| ea_dn_alu(4, b));
            let recipe = buf.finish();
            let got = read_addr_of(&recipe, &regs);
            let want = compute_ea(opcode, &regs, Size::Word);
            assert_eq!(got, want, "d16(An) EaCalc vs compute_ea (disp={disp:#06x})");
        }
    }

    #[test]
    fn ea_calc_recipe_agrees_with_compute_ea_abs_w() {
        // (xxx).w,D4 — opcode 1101 100 0 01 111 000 = 0xD878. EA = sign_extend16(disp).
        let opcode = 0xD878;
        for disp in [0x2EA4u16, 0xCC1A, 0x0000, 0xFFFF, 0x8000, 0x7FFF] {
            let regs = agreement_regs(disp, 0);
            let mut buf = RecipeBuf::new();
            ea_src(&mut buf, 7, 0, Size::Word, |b| ea_dn_alu(4, b));
            let recipe = buf.finish();
            let got = read_addr_of(&recipe, &regs);
            let want = compute_ea(opcode, &regs, Size::Word);
            assert_eq!(got, want, "abs.w EaCalc vs compute_ea (disp={disp:#06x})");
        }
    }

    #[test]
    fn ea_calc_dest_recipe_agrees_with_compute_ea() {
        // The destination recipe's read and write hit the SAME materialized EA == compute_ea (d16(An)).
        let opcode = 0xD96B; // ADD.w D4,(d16,A3) = 1101 100 1 01 101 011
        let disp = 0x02F6u16;
        let an = 0x0045_7E36u32;
        let regs = agreement_regs(disp, an);
        let mut buf = RecipeBuf::new();
        ea_dst(&mut buf, 5, 3, Size::Word, |a| dn_ea_alu(4, a));
        let recipe = buf.finish();

        let mut st = recipe.clone();
        let mut bus = FlatBus::new();
        st.run_to_completion(&mut regs.clone(), &mut bus);
        let want = compute_ea(opcode, &regs, Size::Word);
        let read = bus
            .log
            .iter()
            .find(|t| t.kind == TxKind::Read && t.fc == 5)
            .unwrap()
            .addr;
        let write = bus
            .log
            .iter()
            .find(|t| t.kind == TxKind::Write && t.fc == 5)
            .unwrap()
            .addr;
        assert_eq!(read, want, "dest read EA agrees with compute_ea");
        assert_eq!(write, want, "dest write EA agrees with compute_ea");
        assert_eq!(read, write, "dest read and write hit the same EA");
    }

    #[test]
    fn ea_calc_recipe_agrees_with_compute_ea_d16_pc() {
        // d16(PC),D4 — opcode 1101 100 0 01 111 010 = 0xD87A. EA = (pc+2) + sign_extend16(disp). The base
        // is the extension-word address (pc+2), captured by EaCalc BEFORE any refill advances pc.
        let opcode = 0xD87A;
        for (disp, pc) in [
            (0x0010u16, 0x0000_0C00u32), // small positive
            (0xD8E2, 0x0000_0C00),       // large negative (-10014)
            (0x7FFE, 0x0000_0040),       // large positive (even EA)
            (0x8000, 0x0010_0000),       // large negative
        ] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 0,
                pc,
                sr: SR_SUPERVISOR,
                prefetch: [opcode, disp],
            };
            let mut buf = RecipeBuf::new();
            ea_src(&mut buf, 7, 2, Size::Word, |b| ea_dn_alu(4, b));
            let recipe = buf.finish();
            let got = read_addr_of(&recipe, &regs);
            let want = compute_ea(opcode, &regs, Size::Word);
            assert_eq!(got, want, "d16(PC) EaCalc vs compute_ea (disp={disp:#06x})");
        }
    }

    #[test]
    fn ea_calc_recipe_agrees_with_compute_ea_d8_an_xn() {
        // d8(A3,Xn),D4 — opcode 1101 100 0 01 110 011 = 0xD873. EA = A3 + index(Xn) + sign_extend8(disp8).
        // The brief ext word (prefetch[1]) carries BOTH the index spec and disp8; cover all four W/L × D/A
        // corners plus an A7 index. The shared `compute_ea` decodes it identically to the recipe's EaCalc.
        let opcode = 0xD873;
        // (ext, index-reg setup applied below). ext bits: 15=D/A, 14-12=reg, 11=W/L, 7-0=disp8.
        for ext in [
            0x3010u16, // D3, W, disp +0x10
            0x3810,    // D3, L, disp +0x10
            0xD0F0,    // A5, W, disp -16
            0xF8F0,    // A7, L, disp -16
            0x2080,    // D2, W, disp -128
            0xC87F,    // A4, L, disp +127
        ] {
            let mut regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 0x0020_0000,
                pc: 0x0C00,
                sr: SR_SUPERVISOR,
                prefetch: [opcode, ext],
            };
            regs.a[3] = 0x0010_0000; // base A3
            regs.d[2] = 0x0001_8044;
            regs.d[3] = 0x00FF_2002;
            regs.a[4] = 0x0030_F010;
            regs.a[5] = 0x0000_9008;
            let mut buf = RecipeBuf::new();
            ea_src(&mut buf, 6, 3, Size::Word, |b| ea_dn_alu(4, b));
            let recipe = buf.finish();
            let got = read_addr_of(&recipe, &regs);
            let want = compute_ea(opcode, &regs, Size::Word);
            assert_eq!(got, want, "d8(An,Xn) EaCalc vs compute_ea (ext={ext:#06x})");
        }
    }

    #[test]
    fn ea_calc_recipe_agrees_with_compute_ea_d8_pc_xn() {
        // d8(PC,Xn),D4 — opcode 1101 100 0 01 111 011 = 0xD87B. EA = (pc+2) + index(Xn) + sign_extend8(d8).
        let opcode = 0xD87B;
        for ext in [0x3010u16, 0x3810, 0xD0F0, 0xF8F0] {
            let mut regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 0x0020_0000,
                pc: 0x0000_0C00,
                sr: SR_SUPERVISOR,
                prefetch: [opcode, ext],
            };
            regs.d[3] = 0x00FF_2002;
            regs.a[5] = 0x0000_9008;
            let mut buf = RecipeBuf::new();
            ea_src(&mut buf, 7, 3, Size::Word, |b| ea_dn_alu(4, b));
            let recipe = buf.finish();
            let got = read_addr_of(&recipe, &regs);
            let want = compute_ea(opcode, &regs, Size::Word);
            assert_eq!(got, want, "d8(PC,Xn) EaCalc vs compute_ea (ext={ext:#06x})");
        }
    }

    #[test]
    fn ea_calc_d8_dest_recipe_agrees_with_compute_ea() {
        // The d8(An,Xn) destination recipe's read and write hit the SAME materialized EA == compute_ea.
        let opcode = 0xD976; // ADD.w D4,(d8,A6,Xn) = 1101 100 1 01 110 110
        let ext = 0x3010u16; // D3, W, disp +0x10
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            pc: 0x0C00,
            sr: SR_SUPERVISOR,
            prefetch: [opcode, ext],
        };
        regs.a[6] = 0x0040_0000;
        regs.d[3] = 0x0000_1000;
        let mut buf = RecipeBuf::new();
        ea_dst(&mut buf, 6, 6, Size::Word, |a| dn_ea_alu(4, a));
        let recipe = buf.finish();

        let mut st = recipe.clone();
        let mut bus = FlatBus::new();
        st.run_to_completion(&mut regs.clone(), &mut bus);
        let want = compute_ea(opcode, &regs, Size::Word);
        let read = bus
            .log
            .iter()
            .find(|t| t.kind == TxKind::Read && t.fc == 5)
            .unwrap()
            .addr;
        let write = bus
            .log
            .iter()
            .find(|t| t.kind == TxKind::Write && t.fc == 5)
            .unwrap()
            .addr;
        assert_eq!(read, want, "d8 dest read EA agrees with compute_ea");
        assert_eq!(write, want, "d8 dest write EA agrees with compute_ea");
        assert_eq!(read, write, "d8 dest read and write hit the same EA");
    }

    #[test]
    fn abs_l_recipe_addresses_the_two_extension_words() {
        // (xxx).l,D4 — opcode 1101 100 0 01 111 001 = 0xD879. The address is (HIGH << 16) | LOW, where HIGH
        // is prefetch[1] (captured first) and LOW is the word at pc+4 (shifted into prefetch[1] by the first
        // interleaved refill, NOT taken from that refill's bus value). compute_ea cannot reach the LOW word
        // (it lives in RAM, not the register file), so the expected EA is formed directly here from the two
        // words and the recipe's operand-READ address is checked against it.
        for (hi, lo) in [
            (0x00CCu16, 0x9C2Au16), // 0xCC9C2A
            (0x0010, 0x0000),       // 0x100000
            (0x00FF, 0xFFFE),       // 0xFFFFFE (top of bus, even)
            (0x1234, 0x5678),       // 0x345678 after the 24-bit mask
        ] {
            let pc = 0x0000_0C00u32;
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 0,
                pc,
                sr: SR_SUPERVISOR,
                prefetch: [0xD879, hi], // prefetch[1] = HIGH word
            };
            // The LOW word lives at pc+4 (big-endian); the first refill shifts it into prefetch[1].
            let mut bus = FlatBus::new();
            bus.poke(pc + 4, (lo >> 8) as u8);
            bus.poke(pc + 5, (lo & 0xFF) as u8);

            let mut buf = RecipeBuf::new();
            ea_src(&mut buf, 7, 1, Size::Word, |b| ea_dn_alu(4, b));
            let mut st = buf.finish();
            st.run_to_completion(&mut regs.clone(), &mut bus);

            let want = (((hi as u32) << 16) | lo as u32) & ADDR_MASK;
            let got = bus
                .log
                .iter()
                .find(|t| t.kind == TxKind::Read && t.fc == 5)
                .expect("abs.l source makes one data read")
                .addr;
            assert_eq!(
                got, want,
                "abs.l EA = (HIGH << 16 | LOW) masked (hi={hi:#06x})"
            );
        }
    }

    #[test]
    fn abs_l_dest_read_and_write_hit_the_same_assembled_ea() {
        // Dn,(xxx).l — opcode 1101 100 1 01 111 001 = 0xD979. Read and write of the RMW both target the
        // materialized abs.l EA (HIGH << 16 | LOW).
        let hi = 0x0036u16;
        let lo = 0xF50Cu16;
        let pc = 0x0000_0C00u32;
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            pc,
            sr: SR_SUPERVISOR,
            prefetch: [0xD979, hi],
        };
        let mut bus = FlatBus::new();
        bus.poke(pc + 4, (lo >> 8) as u8);
        bus.poke(pc + 5, (lo & 0xFF) as u8);

        let mut buf = RecipeBuf::new();
        ea_dst(&mut buf, 7, 1, Size::Word, |a| dn_ea_alu(4, a));
        let mut st = buf.finish();
        st.run_to_completion(&mut regs.clone(), &mut bus);

        let want = (((hi as u32) << 16) | lo as u32) & ADDR_MASK;
        let read = bus
            .log
            .iter()
            .find(|t| t.kind == TxKind::Read && t.fc == 5)
            .unwrap()
            .addr;
        let write = bus
            .log
            .iter()
            .find(|t| t.kind == TxKind::Write && t.fc == 5)
            .unwrap()
            .addr;
        assert_eq!(read, want, "abs.l dest read EA");
        assert_eq!(write, want, "abs.l dest write EA");
        assert_eq!(read, write, "dest read and write hit the same abs.l EA");
    }
}
