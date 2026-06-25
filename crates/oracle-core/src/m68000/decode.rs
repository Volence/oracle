//! Opcode → micro-op recipe (decode), and the instruction-level [`Cpu68000`] entry points.
//!
//! `decode` maps the opcode in the prefetch queue to its [`MicroState`] recipe; the two `Cpu68000`
//! methods tie decode to the framework's two drivers (run-to-completion fast path / step-one-micro-op
//! quiesce). Decodes the `ADD`/`SUB` families in word, byte and long sizes so far (the shared `arith_ea_dn`
//! / `arith_dn_ea` builders are parameterized by `AluOp` and `Size`); the full 65536-entry dispatch (one
//! builder per instruction family) lands with full coverage.

use super::bus68k::Bus68k;
use super::ea::{ea_dst, ea_move, ea_movea, ea_src, RecipeBuf};
use super::microop::{condition_true, AluOp, Cpu68000, Dest, MicroOp, MicroState, Operand, Size};
use super::registers::Registers;

/// Scratch slot holding a `JMP`'s computed 32-bit branch target (the `SetPc` source). Slot 0 — the same
/// slot a `Bcc`'s `TargetCalc` deposits its target into.
const JMP_TARGET_SLOT: u8 = 0;

/// Scratch slot parking the HIGH word of a `JMP abs.l` target between the two extension-word captures, so the
/// LOW-word refill does not clobber it. Slot 3 — matching the EA machinery's `abs.l`-HI convention, distinct
/// from the target slot 0 so both halves are snapshot-visible mid-assembly.
const ABS_L_HI_SLOT: u8 = 3;

/// Whether an EA `mode`/`reg` pair is an alterable-memory destination the builder currently covers:
/// `(An)` (010), `(An)+` (011), `-(An)` (100), `d16(An)` (101), `d8(An,Xn)` (110), `abs.w` (111/000),
/// `abs.l` (111/001) — all seven alterable-memory modes. (PC-relative and `#imm` are not alterable, so
/// never destinations.)
#[inline]
fn is_dst_mem_mode(mode: u16, reg: u16) -> bool {
    matches!(mode, 2..=6) || (mode == 7 && (reg == 0 || reg == 1))
}

/// Whether `opcode` is a `MOVE.w` (NOT `MOVEA`). MOVE layout: `00 SS RRR MMM mmm rrr` — bits 15-14 = 00,
/// the size field (bits 13-12) is `11` for word, the destination mode (bits 8-6, the SWAPPED field) is the
/// EA mode. `dst_mode == 1` (`An`) is `MOVEA` (a separate decode arm, M4) and is excluded here. So a word
/// MOVE has bits 15-12 == `0b0011` and `dst_mode != 1`.
#[inline]
fn is_move_word(opcode: u16) -> bool {
    (opcode >> 12) & 0xF == 0b0011 && ((opcode >> 6) & 7) != 1
}

/// Whether `opcode` is a `MOVE.b` (NOT `MOVEA` — byte MOVEA is illegal). Same layout as [`is_move_word`]
/// but the size field (bits 13-12) is `01` for byte, so bits 15-12 == `0b0001`. `dst_mode == 1` (`An`) is
/// excluded (byte MOVEA does not exist).
#[inline]
fn is_move_byte(opcode: u16) -> bool {
    (opcode >> 12) & 0xF == 0b0001 && ((opcode >> 6) & 7) != 1
}

/// Whether `opcode` is a `MOVE.l` (NOT `MOVEA.l`). Same layout as [`is_move_word`] but the size field
/// (bits 13-12) is `10` for long, so bits 15-12 == `0b0010`. `dst_mode == 1` (`An`) is `MOVEA.l` (a separate
/// decode arm, M4) and is excluded here.
#[inline]
fn is_move_long(opcode: u16) -> bool {
    (opcode >> 12) & 0xF == 0b0010 && ((opcode >> 6) & 7) != 1
}

/// Whether `opcode` is a `MOVEA` (MOVE with `dst_mode == 1`, An). MOVEA exists only for **word** (size
/// field 11) and **long** (size field 10) — byte MOVEA is ILLEGAL (size field 01 with `dst_mode == 1` is
/// not decoded). So bits 15-14 == 00, `dst_mode` (bits 8-6) == 1, and the size field is 11 or 10. MOVEA
/// affects no flags (`.w` sign-extends the source word to 32 bits; `.l` writes full 32). The source is any
/// of the 12 EA modes (An-direct included).
#[inline]
fn is_movea(opcode: u16) -> bool {
    (opcode >> 14) == 0 && ((opcode >> 6) & 7) == 1 && matches!((opcode >> 12) & 3, 0b11 | 0b10)
}

/// Decode the opcode currently in `regs.prefetch[0]` into its micro-op recipe.
#[inline]
pub fn decode(regs: &Registers) -> MicroState {
    let opcode = regs.prefetch[0];
    // MOVE.w (`00 11 RRR MMM mmm rrr`, dst_mode != 1) — the EA→EA composition. Decoded before the ADD/SUB
    // arms (the opcode spaces 0x3xxx and 0xD/0x9xxx are disjoint).
    if is_move_word(opcode) {
        return move_recipe(opcode, Size::Word);
    }
    // MOVE.b (`00 01 RRR MMM mmm rrr`, dst_mode != 1) — the byte EA→EA composition. Same shape as MOVE.w
    // through the size-aware `ea_move`; the opcode space 0x1xxx is disjoint from MOVE.w/ADD/SUB.
    if is_move_byte(opcode) {
        return move_recipe(opcode, Size::Byte);
    }
    // MOVE.l (`00 10 RRR MMM mmm rrr`, dst_mode != 1) — the long EA→EA composition (the heaviest recipes:
    // a long source = two reads + Combine32; a long memory dest = two writes). Same size-aware `ea_move`;
    // the opcode space 0x2xxx (long, dst_mode != 1) is disjoint from MOVE.w/MOVE.b/ADD/SUB.
    if is_move_long(opcode) {
        return move_recipe(opcode, Size::Long);
    }
    // MOVEA.w / MOVEA.l (`00 SS RRR 001 mmm rrr`, dst_mode == 1 == An). No flags; `.w` sign-extends the
    // source word to 32 bits, `.l` writes full 32. Byte MOVEA is illegal (not matched). The opcode space
    // (0x3xxx/0x2xxx with dst_mode == 1) is disjoint from plain MOVE (dst_mode != 1) and from ADD/SUB.
    if is_movea(opcode) {
        let size = if (opcode >> 12) & 3 == 0b11 {
            Size::Word
        } else {
            Size::Long
        };
        return movea_recipe(opcode, size);
    }
    // ADD.w and SUB.w share recipe shapes — they differ only in the `AluOp` (operand order is arranged so
    // the destination is the minuend, which matters for the non-commutative SUB).
    // `<op>.w Dn,<ea>` (memory destination, `1xx1 ddd 1 01 mmm rrr`). The destination-EA builder handles
    // all seven alterable-memory modes: `(An)`/`(An)+`/`-(An)`/`d16(An)`/`d8(An,Xn)`/`abs.w`/`abs.l`.
    if opcode & 0xF1C0 == 0xD140 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Add, Size::Word); // ADD.w Dn,<ea>
    }
    if opcode & 0xF1C0 == 0xD040 {
        return arith_ea_dn(opcode, AluOp::Add, Size::Word); // ADD.w <ea>,Dn
    }
    if opcode & 0xF1C0 == 0x9140 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Sub, Size::Word); // SUB.w Dn,<ea>
    }
    if opcode & 0xF1C0 == 0x9040 {
        return arith_ea_dn(opcode, AluOp::Sub, Size::Word); // SUB.w <ea>,Dn
    }
    // ADD.b / SUB.b — same opcode shapes, the size field `00` (`<op>.b`). `Dn,<ea>` (memory dest, bit8 = 1)
    // and `<ea>,Dn` (register dest, bit8 = 0). Byte excludes `An`-direct as a source (`ADD.b An,Dn` is
    // illegal) — that is handled by the source builder / the `covered()` filter, not here.
    if opcode & 0xF1C0 == 0xD100 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Add, Size::Byte); // ADD.b Dn,<ea>
    }
    if opcode & 0xF1C0 == 0xD000 {
        return arith_ea_dn(opcode, AluOp::Add, Size::Byte); // ADD.b <ea>,Dn
    }
    if opcode & 0xF1C0 == 0x9100 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Sub, Size::Byte); // SUB.b Dn,<ea>
    }
    if opcode & 0xF1C0 == 0x9000 {
        return arith_ea_dn(opcode, AluOp::Sub, Size::Byte); // SUB.b <ea>,Dn
    }
    // ADD.l / SUB.l — the size field `10` (`<op>.l`). `<ea>,Dn` (opmode 010): ADD=0xD080, SUB=0x9080.
    // `Dn,<ea>` (opmode 110, alterable-memory dest): ADD=0xD180, SUB=0x9180. A `.l` operand is two word
    // bus accesses, threaded through the same `ea_src`/`ea_dst` builders.
    if opcode & 0xF1C0 == 0xD180 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Add, Size::Long); // ADD.l Dn,<ea>
    }
    if opcode & 0xF1C0 == 0xD080 {
        return arith_ea_dn(opcode, AluOp::Add, Size::Long); // ADD.l <ea>,Dn
    }
    if opcode & 0xF1C0 == 0x9180 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Sub, Size::Long); // SUB.l Dn,<ea>
    }
    if opcode & 0xF1C0 == 0x9080 {
        return arith_ea_dn(opcode, AluOp::Sub, Size::Long); // SUB.l <ea>,Dn
    }
    // Bcc / BRA (`0110 cccc dddddddd`, 0x6xxx) — conditional branch. cc = bits 11-8 (cc == 0 is BRA, always
    // taken); cc == 1 is BSR (a separate decode arm, NOT this commit) and is excluded. The condition is
    // evaluated at DECODE time against the live CCR, emitting the taken or not-taken linear recipe directly.
    if opcode >> 12 == 0b0110 && (opcode >> 8) & 0xF != 1 {
        return bcc_recipe(opcode, regs);
    }
    // BSR (`0110 0001 dddddddd`, 0x61xx) — branch to subroutine: an UNCONDITIONAL PC-relative branch (cc == 1
    // == the BSR encoding) that first PUSHES the 32-bit return address onto the stack, then branches. byte /
    // word displacement like Bcc (`disp8 == 0` → word form). `disp8 == 0xFF` is the 68020 long-displacement
    // form — an address-error trap on the 68000 (xfail), never decoded here. The opcode space 0x61xx is
    // disjoint from Bcc (which excludes cc == 1) and from every arm above.
    if opcode & 0xFF00 == 0x6100 {
        return bsr_recipe(opcode);
    }
    // JMP `<control ea>` (`0100 1110 11 mmm rrr`, 0x4EC0 | ea) — compute the UNMASKED branch target for the
    // control addressing mode, write the PC, and reload the prefetch queue. No push (that is JSR). Control
    // modes only: `(An)` 010, `(d16,An)` 101, `(d8,An,Xn)` 110, `abs.w` 111/0, `abs.l` 111/1, `(d16,PC)`
    // 111/2, `(d8,PC,Xn)` 111/3. The opcode space 0x4EC0..=0x4EFF is disjoint from every arm above.
    if opcode & 0xFFC0 == 0x4EC0 {
        return jmp_recipe(opcode);
    }
    // JSR `<control ea>` (`0100 1110 10 mmm rrr`, 0x4E80 | ea) — jump to subroutine: compute the UNMASKED
    // branch target for the control addressing mode (the same seven modes as JMP), PUSH the 32-bit return
    // address, and reload the prefetch queue at the target. The reload **splits around the push** (read
    // target → push → read target+2). The opcode space 0x4E80..=0x4EBF is disjoint from JMP (0x4EC0) and
    // every arm above.
    if opcode & 0xFFC0 == 0x4E80 {
        return jsr_recipe(opcode);
    }
    // RTS (`0x4E75`) — return from subroutine: POP the 32-bit return address off the supervisor/user stack
    // (hi @ SP, lo @ SP+2, FC=Data) and reload the prefetch queue at it. The inverse of the BSR/JSR push: a
    // long pop (`AdjustAddr(SP, +4)`) then the universal `SetPc` + two-`Prefetch` queue reload. No flags. The
    // opcode `0x4E75` is a single point in the 0x4Exx space, disjoint from JMP (0x4EC0) / JSR (0x4E80) above.
    if opcode == 0x4E75 {
        return rts_recipe();
    }
    todo!("opcode {opcode:#06X} not yet decoded")
}

impl Cpu68000 {
    /// Decode the next instruction (from the prefetch queue) and begin it (the quiesce path then drives
    /// it one micro-op at a time via [`Cpu68000::step_micro_op`]).
    pub fn start_instruction(&mut self) {
        let recipe = decode(&self.regs);
        self.begin(recipe);
    }

    /// Decode and run the next instruction to completion — the default fast path. Returns its cycles.
    #[inline]
    pub fn run_instruction(&mut self, bus: &mut impl Bus68k) -> u32 {
        let mut recipe = decode(&self.regs);
        recipe.run_to_completion(&mut self.regs, bus)
    }
}

/// The `Dn` operand the ALU samples at `size` (the low word for `.w`, the low byte for `.b`).
#[inline]
fn dn_operand(dn: u8, size: Size) -> Operand {
    match size {
        Size::Long => Operand::DataRegFull(dn),
        Size::Word => Operand::DataRegLow16(dn),
        Size::Byte => Operand::DataRegLow8(dn),
    }
}

/// The `Dn` write-back destination at `size` (low word for `.w` preserving the high word; low byte for
/// `.b` preserving the upper 24 bits).
#[inline]
fn dn_dest(dn: u8, size: Size) -> Dest {
    match size {
        Size::Long => Dest::DataReg(dn),
        Size::Word => Dest::DataRegLow16(dn),
        Size::Byte => Dest::DataRegLow8(dn),
    }
}

/// `<op>.{b,w} Dn,<ea>` (`1xx1 ddd s 0 mmm rrr` with `s` the size field, memory destination): read the
/// memory operand at the dest EA, refill prefetch, combine it with `Dn`, write the result back to the same
/// address. The **memory operand is the minuend** (`a`) so `SUB` computes `<ea> - Dn`; `ADD` is commutative
/// so the same order is correct. The ALU is an overlapped internal step; the `(An)+`/`-(An)` register adjust
/// is a 0-cycle `AdjustAddr` (sized — byte `(A7)`±/`-(A7)` steps by 2 to keep the SP even). Expressed
/// through the shared destination-EA builder ([`ea_dst`]) — the read/refill/ALU/write skeleton is the
/// mode's, only the ALU operands and size are the opcode's.
fn arith_dn_ea(opcode: u16, op: AluOp, size: Size) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    ea_dst(&mut buf, mode, reg, size, |a| MicroOp::Alu {
        op,
        size,
        a,
        b: dn_operand(dn, size),
        dst: Dest::Scratch(1),
    });
    buf.finish()
}

/// `MOVE.{b,w} <ea>,<ea>` (`00 SS RRR MMM mmm rrr`, `dst_mode != 1`): copy the source operand to the
/// destination at `size`, setting N/Z, clearing V/C, preserving X. The destination field is **swapped** —
/// `dst_reg` is bits 11-9, `dst_mode` is bits 8-6 — and the source is the usual `mode/reg` in bits 5-0.
/// Delegates the whole EA→EA composition (source read → flag-ALU → destination write, with the MOVE prefetch
/// interleave) to the size-aware [`ea_move`]. Byte and word share the recipe shape (the byte path uses
/// byte-granular `Read`/`Write` and the byte flag/operand widths); the byte size field never selects `An`
/// (byte MOVEA is illegal).
fn move_recipe(opcode: u16, size: Size) -> MicroState {
    let dst_reg = ((opcode >> 9) & 7) as u8;
    let dst_mode = (opcode >> 6) & 7;
    let src_mode = (opcode >> 3) & 7;
    let src_reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    ea_move(&mut buf, dst_mode, dst_reg, src_mode, src_reg, size);
    buf.finish()
}

/// `MOVEA.{w,l} <ea>,An` (`00 SS RRR 001 mmm rrr`, `dst_mode == 1`): copy the source operand into address
/// register `An`, affecting **no flags**. The `.w` form sign-extends the source word to 32 bits; the `.l`
/// form writes the full 32. The destination is `An` (bits 11-9, the SWAPPED reg field); the source is the
/// usual `mode/reg` in bits 5-0 (all 12 modes are legal — An-direct included). There is no destination
/// memory access (An is a register) and no trailing operand idle. Delegates to [`ea_movea`].
fn movea_recipe(opcode: u16, size: Size) -> MicroState {
    let dst_reg = ((opcode >> 9) & 7) as u8;
    let src_mode = (opcode >> 3) & 7;
    let src_reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    ea_movea(&mut buf, dst_reg, src_mode, src_reg, size);
    buf.finish()
}

/// `<op>.{b,w} <ea>,Dn` (`1xx1 ddd s 0 mmm rrr`, register destination): `Dn = Dn <op> <ea>` — **Dn is the
/// minuend** (`a`). The source-EA builder ([`ea_src`]) covers the source modes; the ALU combines `Dn` (the
/// minuend) with the source operand at `size` and writes back to `Dn`'s low byte/word (upper bits preserved).
fn arith_ea_dn(opcode: u16, op: AluOp, size: Size) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    ea_src(&mut buf, mode, reg, size, |b| MicroOp::Alu {
        op,
        size,
        a: dn_operand(dn, size),
        b,
        dst: dn_dest(dn, size),
    });
    buf.finish()
}

/// `Bcc`/`BRA` (`0110 cccc dddddddd`, 0x6xxx; cc != 1 — cc == 1 is BSR, a separate arm): a conditional
/// program-counter-relative branch. The condition (cc = bits 11-8) is resolved at **decode time** against the
/// live CCR via [`condition_true`], so the interpreter stays a flat linear recipe (a different-length recipe
/// emerges per path, which is how the variable cycle count arises). `disp8 = opcode & 0xFF`; `disp8 == 0`
/// selects the **word** form (the 16-bit displacement is the extension word `prefetch[1]`), else the **byte**
/// form (the displacement is `disp8` itself, from the opcode word). The target is `pc + 2 +
/// sign_extend(disp)` — the displacement is relative to the extension-word address (`pc + 2`), exactly the
/// `PcOfExt` base. All cycle counts/orderings are pinned against the vendored `Bcc` SST stream.
///
/// - **Not taken** — the sequential fall-through: `[Internal(4), Prefetch×k]` (k = 1 word form, 2 word form),
///   advancing `pc` by `2k` with no `SetPc`. Byte = 8 cyc (`62b6`), word = 12 cyc (`6400`).
/// - **Taken** — `[TargetCalc(PcOfExt, ·, disp), Internal(2), SetPc(target), Prefetch, Prefetch]`: compute the
///   full 32-bit (UNMASKED) target FIRST (capturing `prefetch[1]`/the opcode disp before any refill), then
///   `SetPc` primes the queue reload. 10 cyc both forms (`636a` byte, `6700` word).
fn bcc_recipe(opcode: u16, regs: &Registers) -> MicroState {
    let cc = ((opcode >> 8) & 0xF) as u8;
    let byte_form = (opcode & 0xFF) != 0;
    let taken = condition_true(cc, regs.sr);
    let mut buf = RecipeBuf::new();
    if taken {
        // The displacement leg: the opcode's low byte (byte form) or the extension word (word form). The
        // TargetCalc captures it BEFORE any Prefetch shifts/advances, using the original pc (`PcOfExt` = pc+2).
        let disp = if byte_form {
            Operand::BranchDisp8
        } else {
            Operand::DispWord
        };
        buf.push(MicroOp::TargetCalc {
            base: Operand::PcOfExt,
            index: Operand::Zero,
            disp,
            dst: 0,
        });
        buf.push(MicroOp::Internal { cycles: 2 });
        buf.push(MicroOp::SetPc {
            value: Operand::Scratch(0),
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Prefetch);
    } else {
        // Not taken: the leading idle (4) makes the total, then the sequential queue advance — one refill for
        // the byte form (1-word instruction), two for the word form (the displacement word is skipped).
        buf.push(MicroOp::Internal { cycles: 4 });
        buf.push(MicroOp::Prefetch);
        if !byte_form {
            buf.push(MicroOp::Prefetch);
        }
    }
    buf.finish()
}

/// Scratch slot holding a `BSR`'s 32-bit return address (`pc + N`), parked by a [`MicroOp::TargetCalc`] and
/// consumed by the two return-address `Write`s (hi via [`Operand::ScratchHi16`], lo via [`Operand::Scratch`]).
/// Slot 0 — the slot a `Bcc`/`JMP` `TargetCalc` also uses, but a BSR's TARGET lives in a separate slot so the
/// return address survives until both push writes complete.
const BSR_RETURN_SLOT: u8 = 0;

/// Scratch slot holding a `BSR`'s 32-bit branch target (the `SetPc` source), distinct from
/// [`BSR_RETURN_SLOT`] so the return address and the target are both snapshot-visible mid-push.
const BSR_TARGET_SLOT: u8 = 1;

/// Scratch slot holding the **address** of a `BSR` return-address push's LOW half (`SP + 2`), materialized
/// once by an [`MicroOp::EaCalc`] (masked to the 24-bit bus — a real even bus address, distinct from the
/// UNMASKED target) so the low-word `Write` hits exactly `SP + 2`. Slot 2 — distinct from the return / target
/// slots so every value is snapshot-visible mid-push.
const BSR_LO_ADDR_SLOT: u8 = 2;

/// `BSR` (`0110 0001 dddddddd`, 0x61xx): branch to subroutine — an unconditional PC-relative branch that
/// first **pushes the 32-bit return address** (`pc + N`, `N` = the instruction's byte length) onto the
/// supervisor/user stack, then jumps to the target and reloads the prefetch queue. `disp8 = opcode & 0xFF`;
/// `disp8 == 0` selects the **word** form (the 16-bit displacement is the extension word `prefetch[1]`,
/// `N = 4`), else the **byte** form (`disp8` itself, `N = 2`). (`disp8 == 0xFF` is the 68020 long-disp form,
/// an address-error trap on the 68000 — never decoded.) The target is `pc + 2 + sign_extend(disp)` (relative
/// to the extension-word address `pc + 2`, the `PcOfExt` base), UNMASKED — a backward BSR.w can land `pc`
/// with high bits set (e.g. `0xFFFF_DB42`), and only the bus reload address masks.
///
/// Recipe (pinned to the vendored `BSR` SST stream — `617c` byte / `6100` word, both **18 cyc**):
/// `[TargetCalc(return = PcPlus(N)), TargetCalc(target = PcOfExt + disp), Internal(2), AdjustAddr(SP, −4),
/// Write(hi @ SP−4), Write(lo @ SP−2), SetPc(target), Prefetch, Prefetch]`. The two `TargetCalc`s run FIRST
/// (capturing the original `pc` / `prefetch[1]` before any refill); the push is **hi @ SP−4 then lo @ SP−2**
/// (the order pinned to the data — the return address is stored big-endian across the two halves); then the
/// `SetPc` + two `Prefetch`s reload the queue at the target. The `n2` idle is NOT in the asserted transaction
/// stream (only the two writes + two reads are) — it only contributes to the 18-cycle total.
fn bsr_recipe(opcode: u16) -> MicroState {
    let byte_form = (opcode & 0xFF) != 0;
    // The instruction byte length: a byte-form BSR is 2 bytes (return = pc+2), a word-form BSR is 4 (pc+4).
    let n: u8 = if byte_form { 2 } else { 4 };
    // The displacement leg: the opcode's low byte (byte form) or the extension word (word form).
    let disp = if byte_form {
        Operand::BranchDisp8
    } else {
        Operand::DispWord
    };
    let mut buf = RecipeBuf::new();
    // Compute the return address (pc + N) and the branch target FIRST — both capture the original pc (and the
    // word-form disp from prefetch[1]) before any Prefetch advances/shifts the queue. UNMASKED (TargetCalc).
    buf.push(MicroOp::TargetCalc {
        base: Operand::PcPlus(n),
        index: Operand::Zero,
        disp: Operand::Zero,
        dst: BSR_RETURN_SLOT,
    });
    buf.push(MicroOp::TargetCalc {
        base: Operand::PcOfExt,
        index: Operand::Zero,
        disp,
        dst: BSR_TARGET_SLOT,
    });
    // The internal idle (n2; not in the asserted transaction stream — only the total cycle count).
    buf.push(MicroOp::Internal { cycles: 2 });
    // Pre-decrement the stack pointer by 4 (the long push). A7 now points at SP−4 (the new stack top).
    buf.push(MicroOp::AdjustAddr { reg: 7, delta: -4 });
    // Materialize the LOW half's address (SP−4 + 2 = SP−2) once, masked to the 24-bit bus (a real even bus
    // address), so the low-word Write hits exactly SP−2.
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: BSR_LO_ADDR_SLOT,
    });
    // Write the return address as two words — hi @ SP−4 (AddrReg(7), the new SP) FIRST, lo @ SP−2 second
    // (the order pinned to the data; the return address is stored big-endian across the two halves).
    buf.push(MicroOp::Write {
        addr: Operand::AddrReg(7),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        value: Operand::ScratchHi16(BSR_RETURN_SLOT),
    });
    buf.push(MicroOp::Write {
        addr: Operand::Scratch(BSR_LO_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(BSR_RETURN_SLOT),
    });
    // SetPc primes the queue reload at the target; the two Prefetch ops read at target / target+2 (FC 6).
    buf.push(MicroOp::SetPc {
        value: Operand::Scratch(BSR_TARGET_SLOT),
    });
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// `JMP <control ea>` (`0100 1110 11 mmm rrr`, 0x4EC0 | ea): an unconditional jump — set the PC to the
/// computed effective address (no return-address push; that is JSR) and reload the prefetch queue at the
/// target. The seven 68000 control addressing modes:
///
/// - **`(An)` 010** — the target is the address register itself: `SetPc(AddrReg(n))` directly (no compute).
///   **8 cyc**, no idle. Bus: `[r@target, r@target+2]`.
/// - **`(d16,An)` 101 / `abs.w` 111/0 / `(d16,PC)` 111/2** — `TargetCalc(base, ·, DispWord)` (base `AddrReg`
///   / `Zero` / `PcOfExt`), then the 2-cyc idle. **10 cyc**. Bus: `[r@target, r@target+2]`.
/// - **`(d8,An,Xn)` 110 / `(d8,PC,Xn)` 111/3** — `TargetCalc(base, BriefIndex, BriefDisp8)` (base `AddrReg`
///   / `PcOfExt`), then the 6-cyc index idle. **14 cyc**. Bus: `[r@target, r@target+2]`.
/// - **`abs.l` 111/1** — assemble the two extension words **UNMASKED** (the target keeps its full 32 bits;
///   139/140 of the clean SST cases land a target with the upper byte set, which an `EaCalc` mask would
///   wrongly clear): park the HIGH word (`prefetch[1]`, captured into [`ABS_L_HI_SLOT`] before the refill
///   shifts it out via the `(0 << 16) | prefetch[1]` `Combine32` idiom), `Prefetch` (reads the LOW word at
///   `pc+4` into the queue), then `Combine32(HI, ExtWordRaw)` (= `(hi << 16) | lo`, no mask). **12 cyc**, no
///   extra idle (the LOW-word refill is the 3rd word access). Bus: `[r@pc+4, r@target, r@target+2]`.
///
/// Every target is UNMASKED — the PC stays full 32-bit (`SetPc`/`TargetCalc` do no `ADDR_MASK`); only the
/// bus reload address masks. The two trailing `Prefetch`s reload the queue at `target`/`target+2` (FC 6
/// program). All cycle counts/orderings are pinned against the vendored `JMP` SST stream.
fn jmp_recipe(opcode: u16) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    match (mode, reg) {
        // (An) — the target is An itself; no compute, no idle.
        (2, _) => {
            buf.push(MicroOp::SetPc {
                value: Operand::AddrReg(reg),
            });
        }
        // abs.l — assemble the two extension words into the UNMASKED 32-bit target. The HIGH word
        // (prefetch[1]) is parked into ABS_L_HI_SLOT via the `(0 << 16) | prefetch[1]` Combine32 (scratch
        // slot 0 is still 0 in a fresh recipe) BEFORE the refill shifts it out; the refill reads the LOW
        // word (at pc+4) into prefetch[1]; the second Combine32 assembles `(hi << 16) | lo` (no mask) into
        // the target slot. The LOW-word refill IS one of the instruction's bus reads, so no extra idle.
        (7, 1) => {
            buf.push(MicroOp::Combine32 {
                hi: JMP_TARGET_SLOT,
                lo: Operand::ExtWordRaw,
                dst: ABS_L_HI_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Combine32 {
                hi: ABS_L_HI_SLOT,
                lo: Operand::ExtWordRaw,
                dst: JMP_TARGET_SLOT,
            });
            buf.push(MicroOp::SetPc {
                value: Operand::Scratch(JMP_TARGET_SLOT),
            });
        }
        // The TargetCalc register-file modes: compute the UNMASKED target FIRST (capturing the displacement
        // / brief ext word from prefetch[1] and the original pc via PcOfExt, before any refill), then the
        // mode's idle, then SetPc.
        _ => {
            let (base, index, disp, idle) = match (mode, reg) {
                // (d16,An) — An + sign_extend16(disp); 2-cyc idle.
                (5, _) => (Operand::AddrReg(reg), Operand::Zero, Operand::DispWord, 2),
                // (d8,An,Xn) — An + index(Xn) + sign_extend8(disp8); 6-cyc index idle.
                (6, _) => (
                    Operand::AddrReg(reg),
                    Operand::BriefIndex,
                    Operand::BriefDisp8,
                    6,
                ),
                // abs.w — sign_extend16(disp) alone; 2-cyc idle.
                (7, 0) => (Operand::Zero, Operand::Zero, Operand::DispWord, 2),
                // (d16,PC) — (pc+2) + sign_extend16(disp); 2-cyc idle.
                (7, 2) => (Operand::PcOfExt, Operand::Zero, Operand::DispWord, 2),
                // (d8,PC,Xn) — (pc+2) + index(Xn) + sign_extend8(disp8); 6-cyc index idle.
                (7, 3) => (
                    Operand::PcOfExt,
                    Operand::BriefIndex,
                    Operand::BriefDisp8,
                    6,
                ),
                _ => unreachable!("jmp_recipe: non-control EA mode {mode}/{reg}"),
            };
            buf.push(MicroOp::TargetCalc {
                base,
                index,
                disp,
                dst: JMP_TARGET_SLOT,
            });
            buf.push(MicroOp::Internal { cycles: idle });
            buf.push(MicroOp::SetPc {
                value: Operand::Scratch(JMP_TARGET_SLOT),
            });
        }
    }
    // Every JMP ends with the two-word queue reload at target / target+2 (the universal taken-branch tail).
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// Scratch slot holding a `JSR`'s 32-bit branch target (the `SetPc` source). Slot 0 — the same slot a `JMP`'s
/// target lives in, so the `abs.l` HI-word capture can use the fresh-recipe `(0 << 16) | prefetch[1]` idiom
/// (slot 0 is still 0 when the HI capture runs, BEFORE the target is assembled into it).
const JSR_TARGET_SLOT: u8 = 0;

/// Scratch slot holding a `JSR`'s 32-bit return address (`pc + N`), parked by a [`MicroOp::TargetCalc`] and
/// consumed by the two return-address `Write`s (hi via [`Operand::ScratchHi16`], lo via [`Operand::Scratch`]).
/// Slot 1 — distinct from the target slot (slot 0) so the return address survives the whole reload-interleave
/// (it is pushed AFTER the first reload `Prefetch`, while the target is still needed by `SetPc`).
const JSR_RETURN_SLOT: u8 = 1;

/// Scratch slot holding the **address** of a `JSR` return-address push's LOW half (`SP + 2`), materialized
/// once by an [`MicroOp::EaCalc`] (masked to the 24-bit bus — a real even bus address) so the low-word `Write`
/// hits exactly `SP + 2`. Slot 2 — distinct from the target / return slots so every value is snapshot-visible
/// mid-push.
const JSR_LO_ADDR_SLOT: u8 = 2;

/// Scratch slot parking the captured HIGH word of a `JSR abs.l` target between the two extension-word captures,
/// so the LOW-word refill does not clobber it. Slot 3 — matching the EA / `JMP` `abs.l`-HI convention, distinct
/// from the target / return / lo-addr slots so all are snapshot-visible mid-assembly.
const JSR_ABS_L_HI_SLOT: u8 = 3;

/// `JSR <control ea>` (`0100 1110 10 mmm rrr`, 0x4E80 | ea): jump to subroutine — compute the UNMASKED branch
/// target for the control addressing mode (the **same seven modes as JMP**), push the 32-bit return address
/// (`pc + N`, `N` = the instruction's byte length), and reload the prefetch queue at the target. Unlike `BSR`
/// (which pushes first, then reloads both queue words), `JSR`'s **reload SPLITS around the push**: the first
/// reload `Prefetch` reads the target into `prefetch[0]`, then the two push `Write`s run, then the second
/// reload `Prefetch` reads `target+2` into `prefetch[1]`. (Pinned to the vendored `JSR` SST stream — the bus
/// order is `[…compute…, r@target, w@SP−4(hi), w@SP−2(lo), r@target+2]`, the `n` idle never in the stream.)
///
/// The return address `pc + N` is computed FIRST (capturing the original opcode `pc`, before any `Prefetch`
/// advances it), UNMASKED. `N` is **VERIFIED against the pushed value in the data** (the data wins): `(An)` 2,
/// `(d16,An)`/`abs.w`/`(d16,PC)` 4, indexed `(d8,An,Xn)`/`(d8,PC,Xn)` 4, and **`abs.l` 6** (the data shows the
/// pushed return is `pc + 6` — the recon prose said `pc + 4`; the DATA WINS).
///
/// Cycle counts (pinned to the data): `(An)` 16, `(d16,An)`/`abs.w`/`(d16,PC)` 18, `abs.l` 20, indexed
/// `(d8,An,Xn)`/`(d8,PC,Xn)` 22. The target arithmetic per mode mirrors [`jmp_recipe`] (every target is
/// UNMASKED — the PC stays full 32-bit; only the bus reload address masks). `abs.l` assembles its two
/// extension words BEFORE the push (the LOW-word refill `Prefetch` is the `r@pc+4` bus event).
fn jsr_recipe(opcode: u16) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    // The instruction byte length N (the return address is pc + N). VERIFIED against the pushed value in the
    // data: abs.l is 6 (NOT 4 — the data wins), every other control mode is 2 ((An)) or 4.
    let n: u8 = match (mode, reg) {
        (2, _) => 2, // (An) — 1-word instruction
        (7, 1) => 6, // abs.l — 3-word instruction (DATA: pushed return = pc + 6)
        _ => 4,      // (d16,An)/abs.w/(d16,PC)/(d8,An,Xn)/(d8,PC,Xn) — 2-word instructions
    };
    let mut buf = RecipeBuf::new();
    // The return address (pc + N) FIRST — captures the original pc before any Prefetch advances it. UNMASKED.
    buf.push(MicroOp::TargetCalc {
        base: Operand::PcPlus(n),
        index: Operand::Zero,
        disp: Operand::Zero,
        dst: JSR_RETURN_SLOT,
    });
    // Compute the target into JSR_TARGET_SLOT (slot 0) per mode — identical arithmetic to JMP, only the
    // post-target tail (the push splitting the reload) differs.
    match (mode, reg) {
        // (An) — the target is An itself: no compute, no idle.
        (2, _) => {
            buf.push(MicroOp::SetPc {
                value: Operand::AddrReg(reg),
            });
        }
        // abs.l — assemble the two extension words into the UNMASKED 32-bit target. The HIGH word
        // (prefetch[1]) is parked into JSR_ABS_L_HI_SLOT via the `(0 << 16) | prefetch[1]` Combine32 (slot 0
        // is still 0 here — the target has not been assembled yet) BEFORE the refill shifts it out; the refill
        // reads the LOW word (at pc+4) into prefetch[1] (the `r@pc+4` bus event); the second Combine32
        // assembles `(hi << 16) | lo` (no mask) into the target slot. No idle.
        (7, 1) => {
            buf.push(MicroOp::Combine32 {
                hi: JSR_TARGET_SLOT,
                lo: Operand::ExtWordRaw,
                dst: JSR_ABS_L_HI_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Combine32 {
                hi: JSR_ABS_L_HI_SLOT,
                lo: Operand::ExtWordRaw,
                dst: JSR_TARGET_SLOT,
            });
            buf.push(MicroOp::SetPc {
                value: Operand::Scratch(JSR_TARGET_SLOT),
            });
        }
        // The TargetCalc register-file modes: compute the UNMASKED target FIRST (capturing the displacement /
        // brief ext word from prefetch[1] and the original pc via PcOfExt, before any refill), then the mode's
        // idle (n2 / n6; NOT in the asserted transaction stream — only the total cycle count), then SetPc.
        _ => {
            let (base, index, disp, idle) = match (mode, reg) {
                // (d16,An) — An + sign_extend16(disp); 2-cyc idle.
                (5, _) => (Operand::AddrReg(reg), Operand::Zero, Operand::DispWord, 2),
                // (d8,An,Xn) — An + index(Xn) + sign_extend8(disp8); 6-cyc index idle.
                (6, _) => (
                    Operand::AddrReg(reg),
                    Operand::BriefIndex,
                    Operand::BriefDisp8,
                    6,
                ),
                // abs.w — sign_extend16(disp) alone; 2-cyc idle.
                (7, 0) => (Operand::Zero, Operand::Zero, Operand::DispWord, 2),
                // (d16,PC) — (pc+2) + sign_extend16(disp); 2-cyc idle.
                (7, 2) => (Operand::PcOfExt, Operand::Zero, Operand::DispWord, 2),
                // (d8,PC,Xn) — (pc+2) + index(Xn) + sign_extend8(disp8); 6-cyc index idle.
                (7, 3) => (
                    Operand::PcOfExt,
                    Operand::BriefIndex,
                    Operand::BriefDisp8,
                    6,
                ),
                _ => unreachable!("jsr_recipe: non-control EA mode {mode}/{reg}"),
            };
            buf.push(MicroOp::TargetCalc {
                base,
                index,
                disp,
                dst: JSR_TARGET_SLOT,
            });
            buf.push(MicroOp::Internal { cycles: idle });
            buf.push(MicroOp::SetPc {
                value: Operand::Scratch(JSR_TARGET_SLOT),
            });
        }
    }
    // The JSR reload-interleave: SetPc has primed pc = target − 4. The FIRST reload Prefetch reads pc+4 =
    // target into prefetch[0] (and advances pc to target − 2). THEN the return-address push runs: pre-decrement
    // SP by 4, materialize the LOW-half address (SP−4 + 2 = SP−2), and write hi @ SP−4 then lo @ SP−2 (the
    // big-endian return address, the same order as BSR). The SECOND reload Prefetch reads (target − 2) + 4 =
    // target + 2 into prefetch[1] (leaving pc = target). Bus order: [r@target, w@SP−4, w@SP−2, r@target+2].
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::AdjustAddr { reg: 7, delta: -4 });
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: JSR_LO_ADDR_SLOT,
    });
    buf.push(MicroOp::Write {
        addr: Operand::AddrReg(7),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        value: Operand::ScratchHi16(JSR_RETURN_SLOT),
    });
    buf.push(MicroOp::Write {
        addr: Operand::Scratch(JSR_LO_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(JSR_RETURN_SLOT),
    });
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// Scratch slot holding the HIGH word of an `RTS` popped return address (read from `SP`), assembled with the
/// LOW word by [`MicroOp::Combine32`]. Slot 0 — the conventional read-value slot.
const RTS_HI_SLOT: u8 = 0;

/// Scratch slot holding the **address** of an `RTS` pop's LOW half (`SP + 2`), materialized once by an
/// [`MicroOp::EaCalc`] (masked to the 24-bit bus — a real even bus address) so the low-word `Read` hits
/// exactly `SP + 2`. Slot 2 — distinct from the hi / lo / target slots so every value is snapshot-visible.
const RTS_LO_ADDR_SLOT: u8 = 2;

/// Scratch slot holding the LOW word of an `RTS` popped return address (read from `SP + 2`). Slot 4 — distinct
/// from the hi-word / lo-addr / target slots so every half is snapshot-visible mid-pop.
const RTS_LO_SLOT: u8 = 4;

/// Scratch slot holding the assembled 32-bit `RTS` return address (the `SetPc` source). Slot 1 — distinct from
/// the hi/lo read slots so the popped target is snapshot-visible while the queue reloads.
const RTS_TARGET_SLOT: u8 = 1;

/// `RTS` (`0x4E75`): return from subroutine — POP the 32-bit return address off the stack and reload the
/// prefetch queue at it. The inverse of the `BSR`/`JSR` return-address push, reusing the same long stack
/// machinery: a long pop is **two word reads** (hi @ `SP`, lo @ `SP + 2`, FC=Data) assembled by
/// [`MicroOp::Combine32`] into the UNMASKED 32-bit target, the stack pointer post-incremented by 4
/// ([`MicroOp::AdjustAddr`], A7-aware), then the universal taken-branch tail — [`MicroOp::SetPc`] primes the
/// queue reload and the two `Prefetch` ops read at `target` / `target + 2` (FC=6 program), leaving
/// `pc == target`.
///
/// Recipe (pinned to the vendored `RTS` SST stream — `4e75`, **16 cyc**): `[Read(hi @ SP), EaCalc(SP+2),
/// Read(lo @ SP+2), AdjustAddr(SP, +4), Combine32(hi,lo → target), SetPc(target), Prefetch, Prefetch]`. The
/// popped return address is the FULL 32-bit pc (UNMASKED — a return into high memory keeps its high bits;
/// `Combine32` does no mask), and only the bus reload address masks. The `SP+2` low-half address is
/// materialized once (masked, a real even bus address) so the low read hits exactly `SP + 2`. The bus stream
/// is `[r@SP, r@SP+2, r@target, r@target+2]` (4 word reads = 16 cycles, no idle).
fn rts_recipe() -> MicroState {
    let mut buf = RecipeBuf::new();
    // Pop the return address — HI word @ SP first.
    buf.push(MicroOp::Read {
        addr: Operand::AddrReg(7),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: RTS_HI_SLOT,
    });
    // Materialize the LOW half's address (SP + 2) once, masked to the 24-bit bus (a real even bus address).
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: RTS_LO_ADDR_SLOT,
    });
    // LOW word @ SP + 2.
    buf.push(MicroOp::Read {
        addr: Operand::Scratch(RTS_LO_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: RTS_LO_SLOT,
    });
    // Post-increment the stack pointer by 4 (the long pop). A7-aware (routes through ssp/usp).
    buf.push(MicroOp::AdjustAddr { reg: 7, delta: 4 });
    // Assemble the UNMASKED 32-bit return address (the full PC keeps its high bits; only the bus reload masks).
    buf.push(MicroOp::Combine32 {
        hi: RTS_HI_SLOT,
        lo: Operand::Scratch(RTS_LO_SLOT),
        dst: RTS_TARGET_SLOT,
    });
    // SetPc primes the queue reload at the popped target; the two Prefetch ops read at target / target+2.
    buf.push(MicroOp::SetPc {
        value: Operand::Scratch(RTS_TARGET_SLOT),
    });
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::m68000::bus68k::{FlatBus, Transaction, TxKind};
    use crate::m68000::microop::Step;

    /// The clean SingleStepTests reference case `db50 [ADD.w D5,(A0)]` (even address, 12 cycles).
    fn setup_db50() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            pc: 0x0C00,
            sr: 0x2717,
            prefetch: [0xDB50, 0x6A3C],
        };
        regs.d[5] = 0x020D_2596;
        regs.a[0] = 0xBB4F_4F46;
        let mut bus = FlatBus::new();
        for (a, v) in [
            (3076u32, 65u8),
            (3077, 78),
            (5_197_638, 63),
            (5_197_639, 224),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x4F_4F46,
                size: Size::Word,
                value: 0x3FE0,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 0x0C04,
                size: Size::Word,
                value: 0x414E,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 0x4F_4F46,
                size: Size::Word,
                value: 0x6576,
            },
        ]
    }

    fn assert_db50_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 0x0C02, "pc advanced by one word");
        assert_eq!(cpu.regs.sr, 0x2700, "CCR cleared, system byte preserved");
        assert_eq!(cpu.regs.d[5], 0x020D_2596, "Dn unchanged (dest is memory)");
        assert_eq!(cpu.regs.a[0], 0xBB4F_4F46, "An unchanged");
        assert_eq!(cpu.regs.prefetch, [0x6A3C, 0x414E], "prefetch advanced");
        assert_eq!(bus.peek(0x4F_4F46), 0x65);
        assert_eq!(bus.peek(0x4F_4F47), 0x76);
        assert_eq!(bus.log, expected_log());
    }

    #[test]
    fn run_instruction_matches_db50() {
        let (mut cpu, mut bus) = setup_db50();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 12);
        assert_db50_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_db50() {
        // Driver 1.
        let (mut rtc, mut bus_rtc) = setup_db50();
        rtc.run_instruction(&mut bus_rtc);
        // Driver 2.
        let (mut step, mut bus_step) = setup_db50();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_db50_final(&step, &bus_step);
    }

    #[test]
    fn quiescable_and_serializable_at_every_micro_op_boundary() {
        // Reference final state from an uninterrupted run.
        let (mut rref, mut bref) = setup_db50();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 4 micro-ops → in-flight boundaries after 0..=3 of them. Snapshot/restore the CPU at each,
        // resume on the same bus, and require an identical final state + transaction stream.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_db50();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            // Snapshot + restore mid-instruction (the whole CPU, incl. the in-flight cursor).
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            // Resume to completion on the same bus.
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SingleStepTests reference case `d075 [ADD.w (d8,A5,Xn),D0]` (even EA, 14 cycles). Brief
    /// ext `0xA22E` = index A2, word size, disp8 +46; EA = A5 + sign_extend16(A2 low 16) + 46 = 0x958DFC.
    fn setup_d075() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x1933_F716,
            ssp: 0x0000_0800,
            pc: 0x0C00,
            sr: 0x2718,
            prefetch: [0xD075, 0xA22E],
        };
        regs.d[0] = 0x2A4A_F7DE;
        regs.a[5] = 0xB395_5165;
        regs.a[2] = 0x02DC_3C69;
        let mut bus = FlatBus::new();
        for (a, v) in [
            (3076u32, 97u8),
            (3077, 204),
            (3078, 120),
            (3079, 192),
            (9_801_212, 62),
            (9_801_213, 27),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_d075_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 0x0C04,
                size: Size::Word,
                value: 0x61CC,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x95_8DFC,
                size: Size::Word,
                value: 0x3E1B,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 0x0C06,
                size: Size::Word,
                value: 0x78C0,
            },
        ]
    }

    fn assert_d075_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 0x0C04, "pc advanced by one word");
        assert_eq!(cpu.regs.sr, 0x2711, "CCR set per the add");
        assert_eq!(
            cpu.regs.d[0], 0x2A4A_35F9,
            "Dn low word = 0xF7DE + 0x3E1B; high preserved"
        );
        assert_eq!(cpu.regs.a[5], 0xB395_5165, "An (base) unchanged");
        assert_eq!(cpu.regs.a[2], 0x02DC_3C69, "Xn (index) unchanged");
        assert_eq!(cpu.regs.prefetch, [0x61CC, 0x78C0], "prefetch advanced");
        assert_eq!(bus.log, expected_d075_log());
    }

    #[test]
    fn run_instruction_matches_d075() {
        let (mut cpu, mut bus) = setup_d075();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 14,
            "EaCalc/Alu 0-cyc + Internal(2) + 3 word accesses"
        );
        assert_d075_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_d075() {
        let (mut rtc, mut bus_rtc) = setup_d075();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_d075();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 14);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_d075_final(&step, &bus_step);
    }

    #[test]
    fn d075_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The no-divergence guarantee for the indexed d8(An,Xn) mode: snapshot/restore the whole CPU (incl.
        // the in-flight cursor and its scratch slots — the materialized EA) at every micro-op boundary, then
        // resume on the same bus to an identical final state + transaction stream.
        let (mut rref, mut bref) = setup_d075();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 6 micro-ops (EaCalc, Internal(2), Prefetch, Read, Prefetch, Alu) → boundaries after 0..=5.
        for pause_after in 0..=5 {
            let (mut cpu, mut bus) = setup_d075();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SingleStepTests reference case `de11 [ADD.b (A1),D7]` (even byte address, 8 cycles). The
    /// real on-bus operand byte is `0x45` at the EVEN address `0x97EA9E` (driven on the UDS half); `D7` low
    /// byte `0x84` + `0x45` = `0xC9`, written to D7's low byte (upper 24 bits preserved). Bus stream is the
    /// byte-granular `[READ.b, PF.w]` — exactly the word `(An)` shape, but the operand read is `.b`.
    fn setup_de11() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            pc: 0x0C00,
            sr: 0x270B,
            prefetch: [0xDE11, 0xCD4C],
        };
        regs.d[7] = 0x18B1_3584;
        regs.a[1] = 0xBE97_EA9E;
        let mut bus = FlatBus::new();
        for (a, v) in [(3076u32, 116u8), (3077, 91), (9_955_998, 69)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_de11_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x97_EA9E,
                size: Size::Byte,
                value: 0x45,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 0x0C04,
                size: Size::Word,
                value: 0x745B,
            },
        ]
    }

    fn assert_de11_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 0x0C02, "pc advanced by one word");
        assert_eq!(cpu.regs.sr, 0x2708, "CCR per the byte add");
        assert_eq!(
            cpu.regs.d[7], 0x18B1_35C9,
            "D7 low byte = 0x84 + 0x45 = 0xC9; upper 24 bits preserved"
        );
        assert_eq!(cpu.regs.a[1], 0xBE97_EA9E, "An unchanged");
        assert_eq!(cpu.regs.prefetch, [0xCD4C, 0x745B], "prefetch advanced");
        assert_eq!(bus.log, expected_de11_log());
    }

    #[test]
    fn run_instruction_matches_de11() {
        let (mut cpu, mut bus) = setup_de11();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 8, "byte (An),Dn = [READ.b, PF.w] = 8 cycles");
        assert_de11_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_de11() {
        let (mut rtc, mut bus_rtc) = setup_de11();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_de11();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_de11_final(&step, &bus_step);
    }

    #[test]
    fn de11_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The no-divergence guarantee for byte size: snapshot/restore the whole CPU at every micro-op
        // boundary, resume on the same bus, require an identical final state + byte-granular stream.
        let (mut rref, mut bref) = setup_de11();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 3 micro-ops (Read.b, Prefetch, Alu.b) → boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_de11();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SingleStepTests reference case `d491 [ADD.l (A1),D2]` (even EA, 14 cycles) — the M0 anchor
    /// for a long two-word READ. A1 = 4429638 (even); the long operand is the hi word at A1 and the lo word
    /// at A1+2 (the read order pinned against the data): 0x2026 << 16 | 0xE993 = 0x2026E993; D2 0x7F165E69 +
    /// 0x2026E993 = 0x9F3D47FC. Bus: [READ.hi @A1, READ.lo @A1+2, PF, n2] = 14 cycles.
    fn setup_addl_an_dn() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 1_592_723_716,
            ssp: 2048,
            pc: 3072,
            sr: 9998,
            prefetch: [54417, 37994],
        };
        regs.d[2] = 2_132_402_345;
        regs.a[1] = 4_014_184_262;
        let mut bus = FlatBus::new();
        for (a, v) in [
            (3076u32, 71u8),
            (3077, 7),
            (4_429_638, 32),
            (4_429_639, 38),
            (4_429_640, 233),
            (4_429_641, 147),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_addl_an_dn_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 4_429_638,
                size: Size::Word,
                value: 8230, // hi word 0x2026
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 4_429_640,
                size: Size::Word,
                value: 59795, // lo word 0xE993
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 18183,
            },
        ]
    }

    fn assert_addl_an_dn_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced by one word");
        assert_eq!(cpu.regs.sr, 9994, "CCR per the long add (N|V)");
        assert_eq!(
            cpu.regs.d[2], 2_671_823_420,
            "D2 = 0x7F165E69 + 0x2026E993 = 0x9F3D47FC (full 32)"
        );
        assert_eq!(cpu.regs.a[1], 4_014_184_262, "An unchanged");
        assert_eq!(cpu.regs.prefetch, [37994, 18183], "prefetch advanced");
        assert_eq!(bus.log, expected_addl_an_dn_log());
    }

    #[test]
    fn run_instruction_matches_addl_an_dn() {
        let (mut cpu, mut bus) = setup_addl_an_dn();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 14, "long (An),Dn = [READ.hi, READ.lo, PF, n2] = 14");
        assert_addl_an_dn_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_addl_an_dn() {
        let (mut rtc, mut bus_rtc) = setup_addl_an_dn();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_addl_an_dn();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 14);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_addl_an_dn_final(&step, &bus_step);
    }

    #[test]
    fn addl_an_dn_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The no-divergence guarantee for long size: snapshot/restore the whole CPU (incl. the in-flight
        // cursor and its scratch slots — the two-word read halves mid-assembly) at every micro-op boundary.
        let (mut rref, mut bref) = setup_addl_an_dn();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 6 micro-ops (Read.hi, EaCalc(lo addr), Read.lo, Combine32, Prefetch, Alu, Internal) → 7 ops total.
        for pause_after in 0..=6 {
            let (mut cpu, mut bus) = setup_addl_an_dn();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SingleStepTests reference case `d192 [ADD.l D0,(A2)]` (even EA, 20 cycles) — the M0 anchor
    /// for a long two-word WRITE. A2 = 2925174 (even). The long RMW reads the old value (hi @A2, lo @A2+2),
    /// adds D0, and writes the result back **lo @A2+2 FIRST, then hi @A2** (the reversed long-store order,
    /// pinned against the data). Bus: [READ.hi, READ.lo, PF, WRITE.lo, WRITE.hi] = 20 cycles, no trailing idle.
    fn setup_addl_dn_an() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 3_625_797_882,
            ssp: 2048,
            pc: 3072,
            sr: 10008,
            prefetch: [53650, 55924],
        };
        regs.d[0] = 3_813_601_016;
        regs.a[2] = 3_039_601_270;
        let mut bus = FlatBus::new();
        for (a, v) in [
            (3076u32, 202u8),
            (3077, 33),
            (2_925_174, 82),
            (2_925_175, 162),
            (2_925_176, 241),
            (2_925_177, 128),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_addl_dn_an_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2_925_174,
                size: Size::Word,
                value: 21154, // old hi word
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2_925_176,
                size: Size::Word,
                value: 61824, // old lo word
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 51745,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2_925_176, // LOW half written FIRST (addr+2)
                size: Size::Word,
                value: 57464,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2_925_174, // HIGH half written SECOND (addr)
                size: Size::Word,
                value: 13809,
            },
        ]
    }

    fn assert_addl_dn_an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced by one word");
        assert_eq!(cpu.regs.sr, 10001, "CCR per the long add");
        assert_eq!(
            cpu.regs.d[0], 3_813_601_016,
            "Dn unchanged (dest is memory)"
        );
        assert_eq!(cpu.regs.a[2], 3_039_601_270, "An unchanged");
        assert_eq!(cpu.regs.prefetch, [55924, 51745], "prefetch advanced");
        // The 32-bit result is stored big-endian across the two halves.
        assert_eq!(bus.peek(2_925_174), 53, "result hi byte 0");
        assert_eq!(bus.peek(2_925_175), 241, "result hi byte 1");
        assert_eq!(bus.peek(2_925_176), 224, "result lo byte 0");
        assert_eq!(bus.peek(2_925_177), 120, "result lo byte 1");
        assert_eq!(bus.log, expected_addl_dn_an_log());
    }

    #[test]
    fn run_instruction_matches_addl_dn_an() {
        let (mut cpu, mut bus) = setup_addl_dn_an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 20,
            "long Dn,(An) = [READ.hi, READ.lo, PF, WRITE.lo, WRITE.hi] = 20"
        );
        assert_addl_dn_an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_addl_dn_an() {
        let (mut rtc, mut bus_rtc) = setup_addl_dn_an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_addl_dn_an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 20);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_addl_dn_an_final(&step, &bus_step);
    }

    #[test]
    fn addl_dn_an_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The no-divergence guarantee for a long memory WRITE (the reversed two-word store).
        let (mut rref, mut bref) = setup_addl_dn_an();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 8 micro-ops (EaCalc(lo addr), Read.hi, Read.lo, Combine32, Prefetch, Alu, Write.lo, Write.hi).
        for pause_after in 0..=7 {
            let (mut cpu, mut bus) = setup_addl_dn_an();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    // --- M1: MOVE.w decode quirks (the SWAPPED dest mode/reg field order + the 01/11/10 size bits). These
    // pin the decode arithmetic before any recipe runs, so a field-order regression fails loudly here. ---

    #[test]
    fn move_decode_extracts_swapped_dest_mode_and_reg() {
        // MOVE layout: `00 SS RRR MMM mmm rrr` — dst_reg is bits 11-9, dst_mode is bits 8-6 (SWAPPED vs. the
        // usual mode-then-reg), src_mode bits 5-3, src_reg bits 2-0. Opcode 0x3490 = MOVE.w (A0),(A2):
        // 0011 010 010 010 000 → size 11 (word), dst_reg 010 = A2, dst_mode 010 = (An), src_mode 010 = (An),
        // src_reg 000 = A0.
        let op: u16 = 0x3490;
        assert_eq!((op >> 12) & 3, 0b11, "size field 11 = word");
        assert_eq!((op >> 9) & 7, 2, "dst_reg (bits 11-9) = A2");
        assert_eq!(
            (op >> 6) & 7,
            2,
            "dst_mode (bits 8-6) = (An) — SWAPPED order"
        );
        assert_eq!((op >> 3) & 7, 2, "src_mode (bits 5-3) = (An)");
        assert_eq!(op & 7, 0, "src_reg (bits 2-0) = A0");
    }

    #[test]
    fn move_decode_recognizes_word_size_and_not_movea() {
        // is_move_word() gates bits15-12 == 0b0011 (00 + size 11) AND dst_mode != 1 (mode 1 == MOVEA, M4).
        assert!(is_move_word(0x3490), "0x3490 MOVE.w (A0),(A2)");
        assert!(is_move_word(0x3203), "0x3203 MOVE.w D3,D1");
        assert!(is_move_word(0x3e84), "0x3e84 MOVE.w D4,(A7)");
        // dst_mode == 1 (An) is MOVEA — NOT this commit. 0x3040 = 0011 000 001 000 000 → dst_mode 001.
        assert!(!is_move_word(0x3040), "dst_mode 1 is MOVEA, not MOVE");
        // size 01 = byte (M2), 10 = long (M3), 00 = not MOVE.
        assert!(
            !is_move_word(0x1203),
            "0x1203 size 01 = MOVE.b — not this commit"
        );
        assert!(
            !is_move_word(0x2203),
            "0x2203 size 10 = MOVE.l — not this commit"
        );
        assert!(!is_move_word(0xD040), "ADD.w — not MOVE");
    }

    /// The clean SingleStepTests reference case `3490 [MOVE.w (A0),(A2)]` (even EAs, 12 cycles) — the M1
    /// EA→EA composition anchor. A0 = 0x5462_2D0A (read at 0x69_5D0A), A2 = 0xA340_2DAE (write at 0x41_9F2E).
    /// Source word 0x9F6D → bit15 set → N; X (set in SR 0x2715) survives the move. Bus: [READ @src, WRITE
    /// @dest, PF] — the write is the second-to-last bus event (final prefetch trails it).
    fn setup_move_an_an() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                4_100_208_836,
                2_877_498_823,
                227_143_989,
                3_958_790_289,
                3_906_341_485,
                3_329_882_226,
                916_304_141,
                1_979_269_223,
            ],
            a: [
                1_416_191_242,
                1_404_823_035,
                2_738_982_510,
                2_348_872_290,
                1_590_867_478,
                2_002_883_513,
                2_299_235_345,
            ],
            usp: 2_449_174_748,
            ssp: 2048,
            pc: 3072,
            sr: 10005,
            prefetch: [13456, 27716],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (3077u32, 176u8),
            (3076, 42),
            (6_905_099, 109),
            (6_905_098, 159),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_move_an_an_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 6_905_098,
                size: Size::Word,
                value: 40813, // source word 0x9F6D
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 4_296_302,
                size: Size::Word,
                value: 40813, // copied unchanged
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 10928,
            },
        ]
    }

    fn assert_move_an_an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced by one word");
        assert_eq!(
            cpu.regs.sr, 10008,
            "CCR = X|N (X preserved, N set, Z/V/C cleared)"
        );
        assert_eq!(cpu.regs.a[0], 1_416_191_242, "src An unchanged");
        assert_eq!(cpu.regs.a[2], 2_738_982_510, "dst An unchanged");
        assert_eq!(cpu.regs.prefetch, [27716, 10928], "prefetch advanced");
        assert_eq!(bus.peek(4_296_302), 0x9F, "dest hi byte = source hi");
        assert_eq!(bus.peek(4_296_303), 0x6D, "dest lo byte = source lo");
        assert_eq!(bus.log, expected_move_an_an_log());
    }

    #[test]
    fn run_instruction_matches_move_an_an() {
        let (mut cpu, mut bus) = setup_move_an_an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 12, "MOVE.w (An),(An) = [READ, WRITE, PF] = 12");
        assert_move_an_an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_move_an_an() {
        let (mut rtc, mut bus_rtc) = setup_move_an_an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_move_an_an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_move_an_an_final(&step, &bus_step);
    }

    #[test]
    fn move_an_an_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The no-divergence guarantee for the EA→EA composition: snapshot/restore the whole CPU (incl. the
        // in-flight cursor and its parked value) at every micro-op boundary, resume on the same bus.
        let (mut rref, mut bref) = setup_move_an_an();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 4 micro-ops (Read, Alu(Move→park), Write, Prefetch) → boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_move_an_an();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    // --- M2: MOVE.b decode quirks (the 01 size bits) + the byte anchor. ---

    #[test]
    fn move_b_decode_recognizes_byte_size_and_not_movea() {
        // is_move_byte() gates bits15-12 == 0b0001 (00 + size 01) AND dst_mode != 1 (mode 1 == MOVEA, M4).
        assert!(is_move_byte(0x1094), "0x1094 MOVE.b (A4),(A0)");
        assert!(is_move_byte(0x1203), "0x1203 MOVE.b D3,D1");
        // size 11 = word, 10 = long, 00 = not MOVE — none are byte.
        assert!(!is_move_byte(0x3203), "0x3203 size 11 = MOVE.w — not byte");
        assert!(!is_move_byte(0x2203), "0x2203 size 10 = MOVE.l — not byte");
        assert!(!is_move_byte(0xD040), "ADD.w — not MOVE");
        // dst_mode == 1 (An) is MOVEA — but byte MOVEA is illegal anyway; 0x1040 = 0001 000 001 000 000.
        assert!(!is_move_byte(0x1040), "dst_mode 1 is MOVEA, not MOVE.b");
    }

    /// The clean SingleStepTests reference case `1094 [MOVE.b (A4),(A0)]` (12 cycles) — the M2 byte EA→EA
    /// anchor. A4 read at 0xF58C90 (source byte 0x44), A0 write at 0xDFA6A (the byte copied unchanged). The
    /// source byte 0x44 is positive nonzero → N/Z cleared; X (set in SR 0x271D) survives the move. Bus:
    /// byte-granular [READ.b @src, WRITE.b @dest, PF.w] — the write is the second-to-last bus event.
    fn setup_move_b_an_an() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [1_329_114_320, 0, 0, 0, 0, 0, 0, 0],
            a: [1_175_321_194, 0, 0, 0, 2_834_532_752, 0, 0],
            usp: 3_631_589_744,
            ssp: 2048,
            pc: 3072,
            sr: 10013,
            prefetch: [4244, 28765],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [(3077u32, 138u8), (3076, 216), (15_960_464, 68)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_move_b_an_an_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 15_960_464,
                size: Size::Byte,
                value: 68,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 916_074,
                size: Size::Byte,
                value: 68,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 55434,
            },
        ]
    }

    fn assert_move_b_an_an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced by one word");
        assert_eq!(
            cpu.regs.sr, 10000,
            "CCR = X (X preserved, N/Z/V/C cleared for a positive nonzero byte)"
        );
        assert_eq!(cpu.regs.a[0], 1_175_321_194, "dst An unchanged");
        assert_eq!(cpu.regs.a[4], 2_834_532_752, "src An unchanged");
        assert_eq!(cpu.regs.prefetch, [28765, 55434], "prefetch advanced");
        assert_eq!(bus.peek(916_074), 68, "dest byte = source byte");
        assert_eq!(bus.log, expected_move_b_an_an_log());
    }

    #[test]
    fn run_instruction_matches_move_b_an_an() {
        let (mut cpu, mut bus) = setup_move_b_an_an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "MOVE.b (An),(An) = [READ.b, WRITE.b, PF.w] = 12"
        );
        assert_move_b_an_an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_move_b_an_an() {
        let (mut rtc, mut bus_rtc) = setup_move_b_an_an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_move_b_an_an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_move_b_an_an_final(&step, &bus_step);
    }

    #[test]
    fn move_b_an_an_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The no-divergence guarantee for the byte EA→EA composition: snapshot/restore the whole CPU at
        // every micro-op boundary, resume on the same bus, require an identical final state + byte stream.
        let (mut rref, mut bref) = setup_move_b_an_an();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 4 micro-ops (Read.b, Alu(Move→park), Write.b, Prefetch) → boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_move_b_an_an();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    // --- M3: MOVE.l decode quirks (the 10 size bits) + the long mem→mem anchor. ---

    #[test]
    fn move_l_decode_recognizes_long_size_and_not_movea() {
        // is_move_long() gates bits15-12 == 0b0010 (00 + size 10) AND dst_mode != 1 (mode 1 == MOVEA, M4).
        assert!(is_move_long(0x2a93), "0x2a93 MOVE.l (A3),(A5)");
        assert!(is_move_long(0x2203), "0x2203 MOVE.l D3,D1");
        // size 11 = word, 01 = byte, 00 = not MOVE — none are long.
        assert!(!is_move_long(0x3203), "0x3203 size 11 = MOVE.w — not long");
        assert!(!is_move_long(0x1203), "0x1203 size 01 = MOVE.b — not long");
        assert!(!is_move_long(0xD040), "ADD.w — not MOVE");
        // dst_mode == 1 (An) is MOVEA.l — NOT this commit; 0x2040 = 0010 000 001 000 000.
        assert!(!is_move_long(0x2040), "dst_mode 1 is MOVEA.l, not MOVE.l");
    }

    /// The clean SingleStepTests reference case `2a93 [MOVE.l (A3),(A5)]` (even EAs, 20 cycles) — the M3
    /// long EA→EA composition anchor. A3 = 0x441E_5150 (long read hi @0x115E50, lo @0x115E52), A5 =
    /// 0x6092_2ACA (long write hi @0x89FE4A, lo @0x89FE4C). MOVE.l is WRITE-ONLY at the dest (no RMW read);
    /// the dest long write is HI first @addr then LO @addr+2 (the NON-reversed order — distinct from the
    /// ADD.l RMW store and from MOVE.l's `-(An)` reversal). Bus: [READ.hi, READ.lo, WRITE.hi, WRITE.lo, PF]
    /// — the writes precede the final prefetch.
    fn setup_move_l_an_an() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                2_977_854_374,
                3_910_370_993,
                2_252_041_791,
                1_855_697_001,
                2_954_101_250,
                3_188_869_178,
                2_528_412_717,
                1_046_525_309,
            ],
            a: [
                658_653_233,
                2_053_276_061,
                2_009_026_726,
                1_141_988_560,
                3_179_860_166,
                4_035_575_178,
                2_744_181_842,
            ],
            usp: 466_030_178,
            ssp: 2048,
            pc: 3072,
            sr: 10006,
            prefetch: [10899, 3607],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (3077u32, 162u8),
            (3076, 148),
            (1_137_875, 132),
            (1_137_874, 220),
            (1_137_873, 242),
            (1_137_872, 56),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_move_l_an_an_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 1_137_872,
                size: Size::Word,
                value: 14578, // source hi word
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 1_137_874,
                size: Size::Word,
                value: 56452, // source lo word
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 9_043_338,
                size: Size::Word,
                value: 14578, // dest HI half written FIRST (addr)
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 9_043_340,
                size: Size::Word,
                value: 56452, // dest LO half written SECOND (addr+2)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 38050,
            },
        ]
    }

    fn assert_move_l_an_an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced by one word");
        assert_eq!(
            cpu.regs.sr, 10000,
            "CCR per the long move (N set, V/C clear, X preserved)"
        );
        assert_eq!(cpu.regs.a[3], 1_141_988_560, "src An unchanged");
        assert_eq!(cpu.regs.a[5], 4_035_575_178, "dst An unchanged");
        assert_eq!(cpu.regs.prefetch, [3607, 38050], "prefetch advanced");
        // The 32-bit value is copied unchanged, big-endian across the two halves.
        assert_eq!(bus.peek(9_043_338), 56, "dest hi byte 0");
        assert_eq!(bus.peek(9_043_339), 242, "dest hi byte 1");
        assert_eq!(bus.peek(9_043_340), 220, "dest lo byte 0");
        assert_eq!(bus.peek(9_043_341), 132, "dest lo byte 1");
        assert_eq!(bus.log, expected_move_l_an_an_log());
    }

    #[test]
    fn run_instruction_matches_move_l_an_an() {
        let (mut cpu, mut bus) = setup_move_l_an_an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 20,
            "MOVE.l (An),(An) = [READ.hi, READ.lo, WRITE.hi, WRITE.lo, PF] = 20"
        );
        assert_move_l_an_an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_move_l_an_an() {
        let (mut rtc, mut bus_rtc) = setup_move_l_an_an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_move_l_an_an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 20);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_move_l_an_an_final(&step, &bus_step);
    }

    #[test]
    fn move_l_an_an_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The no-divergence guarantee for the long EA→EA composition (the heaviest in-scope dest write):
        // snapshot/restore the whole CPU (incl. the in-flight cursor and its scratch slots — the two-word
        // read halves mid-assembly and the parked 32-bit value) at every micro-op boundary, resume on the
        // same bus, and require an identical final state + transaction stream.
        let (mut rref, mut bref) = setup_move_l_an_an();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 9 micro-ops: Read.hi, EaCalc(lo addr), Read.lo, Combine32, Alu(Move→park), EaCalc(dst lo addr),
        // Write.hi, Write.lo, Prefetch → boundaries after 0..=8.
        for pause_after in 0..=8 {
            let (mut cpu, mut bus) = setup_move_l_an_an();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SingleStepTests reference case `2914 [MOVE.l (A4),-(A4)]` (even EAs, 20 cycles) — the M3
    /// long predecrement-dest anchor, pinning the notorious `-(An)` long-store reversal. A4 = 0xB19C_1F52
    /// (read at 0x9C1F52); the `-(A4)` dest pre-decrements A4 by 4 (the long step) to 0x9C1F4E and stores
    /// the result there. Unlike a non-predec dest (HI first @addr), the predec dest writes **LO first @addr+2,
    /// then HI @addr** (the reversed long-store order), and the prefetch precedes the writes. Bus:
    /// [READ.hi, READ.lo, PF, WRITE.lo, WRITE.hi] — pinned EXACTLY against the data, not from memory.
    fn setup_move_l_predec() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0, 0, 0, 0, 2_979_798_866, 0, 0],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 9999,
            prefetch: [0x2914, 0],
        };
        let mut bus = FlatBus::new();
        // The source long at 0x9C1F52 (hi 0xA023=40995, lo 0x2CEB=11499) and the final prefetch at pc+4.
        for (a, v) in [
            (10_231_634u32, 0xA0u8),
            (10_231_635, 0x23),
            (10_231_636, 0x2C),
            (10_231_637, 0xEB),
            (3076, 0xB5),
            (3077, 0x02),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn move_l_predec_dest_reverses_the_long_store_order() {
        // The recipe — built straight from the decode — must emit the predecrement reversal: prefetch, then
        // the LOW half written FIRST (at the higher address, addr+2), then the HIGH half (at addr).
        let (mut cpu, mut bus) = setup_move_l_predec();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 20,
            "MOVE.l (An),-(An) = [READ.hi, READ.lo, PF, WRITE.lo, WRITE.hi] = 20"
        );
        // A4 ends pre-decremented by 4.
        assert_eq!(
            cpu.regs.a[4], 2_979_798_862,
            "A4 -= 4 (the long predec step)"
        );
        assert_eq!(
            bus.log,
            vec![
                Transaction {
                    kind: TxKind::Read,
                    fc: 5,
                    addr: 10_231_634,
                    size: Size::Word,
                    value: 0xA023,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 5,
                    addr: 10_231_636,
                    size: Size::Word,
                    value: 0x2CEB,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 3076,
                    size: Size::Word,
                    value: 0xB502,
                },
                Transaction {
                    kind: TxKind::Write,
                    fc: 5,
                    addr: 10_231_632, // LOW half written FIRST (addr+2 = (A4-4)+2)
                    size: Size::Word,
                    value: 0x2CEB,
                },
                Transaction {
                    kind: TxKind::Write,
                    fc: 5,
                    addr: 10_231_630, // HIGH half written SECOND (addr = A4-4)
                    size: Size::Word,
                    value: 0xA023,
                },
            ],
        );
    }

    // --- M4: MOVEA.w / MOVEA.l decode (dst_mode == 1) + the no-flags / sign-extend anchors. ---

    #[test]
    fn movea_decode_recognizes_dst_mode_1_word_and_long() {
        // MOVEA layout: `00 SS RRR 001 mmm rrr` — dst_mode (bits 8-6) == 1 (An). Size 11 = word, 10 = long;
        // byte MOVEA (size 01) is ILLEGAL (never decoded). `is_movea` gates dst_mode == 1 with a word/long
        // size; plain MOVE (dst_mode != 1) and byte MOVEA are excluded.
        assert!(
            is_movea(0x3856),
            "0x3856 MOVEA.w (A6),A4 — dst_mode 1, word"
        );
        assert!(is_movea(0x2642), "0x2642 MOVEA.l D2,A3 — dst_mode 1, long");
        assert!(
            is_movea(0x3a49),
            "0x3a49 MOVEA.w A1,A5 — An source is legal"
        );
        // dst_mode != 1 is plain MOVE, not MOVEA.
        assert!(!is_movea(0x3490), "0x3490 dst_mode 2 = MOVE.w, not MOVEA");
        assert!(!is_movea(0x2a93), "0x2a93 dst_mode 2 = MOVE.l, not MOVEA");
        // Byte MOVEA is illegal: size 01 with dst_mode 1 is NOT a MOVEA. 0x1056 = 0001 000 001 010 110.
        assert!(
            !is_movea(0x1056),
            "byte MOVEA (size 01) is illegal — not MOVEA"
        );
        assert!(!is_movea(0xD040), "ADD.w — not MOVEA");
    }

    #[test]
    fn movea_decode_extracts_dst_an_and_size() {
        // 0x3856 = 0011 100 001 010 110 → size 11 (word), dst_reg 100 = A4, dst_mode 001 = An, src_mode 010 =
        // (An), src_reg 110 = A6.
        let op: u16 = 0x3856;
        assert_eq!((op >> 12) & 3, 0b11, "size field 11 = word");
        assert_eq!((op >> 9) & 7, 4, "dst_reg (bits 11-9) = A4");
        assert_eq!((op >> 6) & 7, 1, "dst_mode (bits 8-6) = An (MOVEA)");
        assert_eq!((op >> 3) & 7, 2, "src_mode = (An)");
        assert_eq!(op & 7, 6, "src_reg = A6");
    }

    /// The clean SingleStepTests reference case `3856 [MOVEA.w (A6),A4]` (even EA, 8 cycles) — the M4 anchor
    /// proving MOVEA.w SIGN-EXTENDS the source word and changes NO flags. A6 = 0xB5...88 (read at 0xBEF88,
    /// even); the source word 0xFC7B has bit15 set → A4 becomes 0xFFFFFC7B (sign-extended full 32). SR
    /// (0x2701) survives untouched (MOVEA affects no flags). Bus: [READ @src, PF] — no destination access.
    fn setup_movea_w_an_an() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0, 0, 0, 0, 1_097_016_680, 0, 3_037_458_184],
            usp: 2_600_751_938,
            ssp: 2048,
            pc: 3072,
            sr: 9985,
            prefetch: [0x3856, 43341],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [(3077u32, 229u8), (3076, 63), (782_089, 123), (782_088, 252)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_movea_w_an_an_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 782_088,
                size: Size::Word,
                value: 64635, // source word 0xFC7B (bit15 set)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 16357,
            },
        ]
    }

    fn assert_movea_w_an_an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced by one word");
        assert_eq!(cpu.regs.sr, 9985, "SR UNCHANGED — MOVEA affects no flags");
        assert_eq!(
            cpu.regs.a[4], 4_294_966_395,
            "A4 = sign_extend16(0xFC7B) = 0xFFFFFC7B (full 32)"
        );
        assert_eq!(cpu.regs.a[6], 3_037_458_184, "src An unchanged");
        assert_eq!(cpu.regs.prefetch, [43341, 16357], "prefetch advanced");
        assert_eq!(bus.log, expected_movea_w_an_an_log());
    }

    #[test]
    fn run_instruction_matches_movea_w_an_an() {
        let (mut cpu, mut bus) = setup_movea_w_an_an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 8,
            "MOVEA.w (An),An = [READ, PF] = 8 (no trailing idle)"
        );
        assert_movea_w_an_an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_movea_w_an_an() {
        let (mut rtc, mut bus_rtc) = setup_movea_w_an_an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_movea_w_an_an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_movea_w_an_an_final(&step, &bus_step);
    }

    #[test]
    fn movea_w_an_an_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The no-divergence guarantee for MOVEA: snapshot/restore the whole CPU at every micro-op boundary,
        // resume on the same bus, require an identical final state + transaction stream.
        let (mut rref, mut bref) = setup_movea_w_an_an();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 3 micro-ops (Read, Prefetch, Alu(MoveA→An)) → boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_movea_w_an_an();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SingleStepTests reference case `2642 [MOVEA.l D2,A3]` (4 cycles) — the M4 anchor proving
    /// MOVEA.l writes the FULL 32-bit source to An and changes NO flags, with NO trailing idle (a register
    /// source MOVEA is just [PF] = 4 cycles, unlike ADD.l Dn,Dn which trails n4). D2 = 2_055_882_111 lands in
    /// A3 verbatim; SR (10007) is untouched.
    fn setup_movea_l_dn_an() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0, 0, 0, 1_288_370_626, 0, 0, 0],
            usp: 2_279_008_622,
            ssp: 2048,
            pc: 3072,
            sr: 10007,
            prefetch: [0x2642, 29169],
        };
        regs.d[2] = 2_055_882_111;
        let mut bus = FlatBus::new();
        for (a, v) in [(3077u32, 141u8), (3076, 124)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn run_instruction_matches_movea_l_dn_an() {
        let (mut cpu, mut bus) = setup_movea_l_dn_an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 4, "MOVEA.l Dn,An = [PF] = 4 (no trailing idle)");
        assert_eq!(cpu.regs.pc, 3074, "pc advanced by one word");
        assert_eq!(cpu.regs.sr, 10007, "SR UNCHANGED — MOVEA affects no flags");
        assert_eq!(
            cpu.regs.a[3], 2_055_882_111,
            "A3 = full 32-bit D2 (no sign-extension needed for .l)"
        );
        assert_eq!(cpu.regs.d[2], 2_055_882_111, "src Dn unchanged");
        assert_eq!(cpu.regs.prefetch, [29169, 31885], "prefetch advanced");
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 31885,
            }]
        );
    }

    #[test]
    fn both_drivers_match_movea_l_dn_an() {
        let (mut rtc, mut bus_rtc) = setup_movea_l_dn_an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_movea_l_dn_an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 4);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
    }

    // --- F0: Bcc/BRA decode quirks (the cc field, the byte/word disp8==0 split) + the four anchors. ---

    #[test]
    fn bcc_decode_extracts_cc_and_disp8() {
        // Bcc layout: `0110 cccc dddddddd` (0x6xxx). cc = bits 11-8, disp8 = bits 7-0. disp8 == 0 selects the
        // word form (16-bit displacement in prefetch[1]); else the byte form (disp8 is the displacement).
        // 0x62b6 → cc 2 (HI), disp8 0xB6 (byte form). 0x6700 → cc 7 (EQ), disp8 0 (word form).
        assert_eq!((0x62b6u16 >> 8) & 0xF, 2, "0x62b6 cc = 2 (HI)");
        assert_eq!(0x62b6u16 & 0xFF, 0xB6, "0x62b6 disp8 = 0xB6 (byte form)");
        assert_eq!((0x6700u16 >> 8) & 0xF, 7, "0x6700 cc = 7 (EQ)");
        assert_eq!(0x6700u16 & 0xFF, 0x00, "0x6700 disp8 = 0 (word form)");
        // cc == 0 is BRA (always taken); cc == 1 is BSR (a SEPARATE decode arm, NOT this commit).
        assert_eq!((0x6000u16 >> 8) & 0xF, 0, "0x6000 cc = 0 (BRA)");
        assert_eq!((0x6100u16 >> 8) & 0xF, 1, "0x6100 cc = 1 (BSR — excluded)");
    }

    /// The clean SST reference case `62b6 [Bcc] 1` (byte form, condition false → NOT taken, 8 cycles) — the
    /// F0 byte not-taken anchor. opcode 0x62b6 → cc 2 (HI = !C & !Z); SR 0x2714 (X|Z set) → Z set → HI false →
    /// not taken. The recipe is the sequential fall-through `[Internal(4), Prefetch]`: pc advances one word
    /// (+2) and the queue shifts once. Bus: a single FC-6 program word read at pc+4. NO SetPc (no branch).
    fn setup_bcc_byte_not_taken() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 10004,
            prefetch: [25270, 48660],
        };
        let mut bus = FlatBus::new();
        // The word at pc+4 (3076) that refills the queue tail = 39526 (0x9A66).
        bus.poke(3076, 0x9A);
        bus.poke(3077, 0x66);
        (Cpu68000::new(regs), bus)
    }

    fn expected_bcc_byte_not_taken_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 39526,
        }]
    }

    fn assert_bcc_byte_not_taken_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word (fall-through)");
        assert_eq!(cpu.regs.sr, 10004, "SR unchanged (Bcc affects no flags)");
        assert_eq!(cpu.regs.prefetch, [48660, 39526], "queue shifted once");
        assert_eq!(bus.log, expected_bcc_byte_not_taken_log());
    }

    #[test]
    fn run_instruction_matches_bcc_byte_not_taken() {
        let (mut cpu, mut bus) = setup_bcc_byte_not_taken();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 8, "byte not-taken = [Internal(4), PF] = 8");
        assert_bcc_byte_not_taken_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_bcc_byte_not_taken() {
        let (mut rtc, mut bus_rtc) = setup_bcc_byte_not_taken();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_bcc_byte_not_taken();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_bcc_byte_not_taken_final(&step, &bus_step);
    }

    #[test]
    fn bcc_byte_not_taken_quiescable_and_serializable_at_every_micro_op_boundary() {
        let (mut rref, mut bref) = setup_bcc_byte_not_taken();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 2 micro-ops (Internal(4), Prefetch) → boundaries after 0..=1.
        for pause_after in 0..=1 {
            let (mut cpu, mut bus) = setup_bcc_byte_not_taken();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SST reference case `636a [Bcc] 2` (byte form, condition true → TAKEN, 10 cycles) — the F0
    /// byte taken anchor. opcode 0x636a → cc 3 (LS = C | Z); SR 0x2713 (X|V|C set) → C set → LS true → taken.
    /// target = pc+2+sign_extend8(0x6a) = 3072+2+106 = 3180. The recipe is `[TargetCalc(BranchDisp8),
    /// Internal(2), SetPc, Prefetch, Prefetch]`: pc lands at 3180 and the queue reloads at 3180/3182. Bus: n2
    /// (not in the stream) + two FC-6 reads at the target.
    fn setup_bcc_byte_taken() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 10003,
            prefetch: [25450, 37941],
        };
        let mut bus = FlatBus::new();
        // The two target words at 3180 (25598 = 0x63FE) and 3182 (15833 = 0x3DD9).
        bus.poke(3180, 0x63);
        bus.poke(3181, 0xFE);
        bus.poke(3182, 0x3D);
        bus.poke(3183, 0xD9);
        (Cpu68000::new(regs), bus)
    }

    fn expected_bcc_byte_taken_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3180,
                size: Size::Word,
                value: 25598,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3182,
                size: Size::Word,
                value: 15833,
            },
        ]
    }

    fn assert_bcc_byte_taken_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3180, "pc landed at the branch target");
        assert_eq!(cpu.regs.sr, 10003, "SR unchanged (Bcc affects no flags)");
        assert_eq!(
            cpu.regs.prefetch,
            [25598, 15833],
            "queue reloaded at target"
        );
        assert_eq!(bus.log, expected_bcc_byte_taken_log());
    }

    #[test]
    fn run_instruction_matches_bcc_byte_taken() {
        let (mut cpu, mut bus) = setup_bcc_byte_taken();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 10,
            "byte taken = [TargetCalc, Internal(2), SetPc, PF, PF] = 10"
        );
        assert_bcc_byte_taken_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_bcc_byte_taken() {
        let (mut rtc, mut bus_rtc) = setup_bcc_byte_taken();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_bcc_byte_taken();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 10);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_bcc_byte_taken_final(&step, &bus_step);
    }

    #[test]
    fn bcc_byte_taken_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the new taken-branch shape (the SetPc + queue-reload tail).
        let (mut rref, mut bref) = setup_bcc_byte_taken();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 5 micro-ops (TargetCalc, Internal(2), SetPc, Prefetch, Prefetch) → boundaries after 0..=4.
        for pause_after in 0..=4 {
            let (mut cpu, mut bus) = setup_bcc_byte_taken();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SST reference case `6700 [Bcc] 366` (word form, condition true → TAKEN, 10 cycles) — the F0
    /// word taken anchor. opcode 0x6700 → cc 7 (EQ = Z), disp8 0 → word form (disp in prefetch[1] = 2718);
    /// SR 0x2714 (Z set) → taken. target = pc+2+sign_extend16(2718) = 3072+2+2718 = 5792. Recipe
    /// `[TargetCalc(DispWord), Internal(2), SetPc, Prefetch, Prefetch]`. Bus: two FC-6 reads at 5792/5794.
    fn setup_bcc_word_taken() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 10004,
            prefetch: [26368, 2718],
        };
        let mut bus = FlatBus::new();
        // Target words at 5792 (45182 = 0xB07E) and 5794 (35558 = 0x8AE6).
        bus.poke(5792, 0xB0);
        bus.poke(5793, 0x7E);
        bus.poke(5794, 0x8A);
        bus.poke(5795, 0xE6);
        (Cpu68000::new(regs), bus)
    }

    fn expected_bcc_word_taken_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5792,
                size: Size::Word,
                value: 45182,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5794,
                size: Size::Word,
                value: 35558,
            },
        ]
    }

    fn assert_bcc_word_taken_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 5792,
            "pc landed at the word-form branch target"
        );
        assert_eq!(cpu.regs.sr, 10004, "SR unchanged");
        assert_eq!(
            cpu.regs.prefetch,
            [45182, 35558],
            "queue reloaded at target"
        );
        assert_eq!(bus.log, expected_bcc_word_taken_log());
    }

    #[test]
    fn run_instruction_matches_bcc_word_taken() {
        let (mut cpu, mut bus) = setup_bcc_word_taken();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 10,
            "word taken = [TargetCalc, Internal(2), SetPc, PF, PF] = 10"
        );
        assert_bcc_word_taken_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_bcc_word_taken() {
        let (mut rtc, mut bus_rtc) = setup_bcc_word_taken();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_bcc_word_taken();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 10);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_bcc_word_taken_final(&step, &bus_step);
    }

    #[test]
    fn bcc_word_taken_quiescable_and_serializable_at_every_micro_op_boundary() {
        let (mut rref, mut bref) = setup_bcc_word_taken();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        for pause_after in 0..=4 {
            let (mut cpu, mut bus) = setup_bcc_word_taken();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SST reference case `6400 [Bcc] 51` (word form, condition false → NOT taken, 12 cycles) — the
    /// F0 word not-taken anchor. opcode 0x6400 → cc 4 (CC = !C), disp8 0 → word form; SR 0x271F (C set) → CC
    /// false → not taken. The fall-through advances pc by TWO words (+4, skipping the displacement word).
    /// Recipe `[Internal(4), Prefetch, Prefetch]`. Bus: two FC-6 reads at pc+4/pc+6.
    fn setup_bcc_word_not_taken() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 10015,
            prefetch: [25600, 45236],
        };
        let mut bus = FlatBus::new();
        // pc+4 (3076) = 53674 (0xD1AA), pc+6 (3078) = 22476 (0x57CC).
        bus.poke(3076, 0xD1);
        bus.poke(3077, 0xAA);
        bus.poke(3078, 0x57);
        bus.poke(3079, 0xCC);
        (Cpu68000::new(regs), bus)
    }

    fn expected_bcc_word_not_taken_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 53674,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3078,
                size: Size::Word,
                value: 22476,
            },
        ]
    }

    fn assert_bcc_word_not_taken_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 3076,
            "pc advanced two words (skip the disp word)"
        );
        assert_eq!(cpu.regs.sr, 10015, "SR unchanged");
        assert_eq!(cpu.regs.prefetch, [53674, 22476], "queue shifted twice");
        assert_eq!(bus.log, expected_bcc_word_not_taken_log());
    }

    #[test]
    fn run_instruction_matches_bcc_word_not_taken() {
        let (mut cpu, mut bus) = setup_bcc_word_not_taken();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 12, "word not-taken = [Internal(4), PF, PF] = 12");
        assert_bcc_word_not_taken_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_bcc_word_not_taken() {
        let (mut rtc, mut bus_rtc) = setup_bcc_word_not_taken();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_bcc_word_not_taken();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_bcc_word_not_taken_final(&step, &bus_step);
    }

    #[test]
    fn bcc_word_not_taken_quiescable_and_serializable_at_every_micro_op_boundary() {
        let (mut rref, mut bref) = setup_bcc_word_not_taken();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 3 micro-ops (Internal(4), Prefetch, Prefetch) → boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_bcc_word_not_taken();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    // --- F1: JMP <control ea> — the seven control modes, one clean (even-target) anchor each, plus the
    // unmasked-target abs.l shape and the SetPc-direct (An) shape (both-drivers + snapshot/restore). ---

    #[test]
    fn jmp_decode_extracts_control_mode_and_reg() {
        // JMP layout: `0100 1110 11 mmm rrr` — bits 15-6 == 0b0100_1110_11 (0x4EC0 prefix), mode bits 5-3,
        // reg bits 2-0. Pin the seven control encodings.
        assert_eq!(0x4ED6 & 0xFFC0, 0x4EC0, "0x4ED6 is a JMP");
        assert_eq!((0x4ED6u16 >> 3) & 7, 2, "0x4ED6 = JMP (A6) — mode 2");
        assert_eq!(0x4ED6u16 & 7, 6, "reg A6");
        assert_eq!((0x4EEBu16 >> 3) & 7, 5, "0x4EEB = JMP (d16,A3) — mode 5");
        assert_eq!((0x4EF5u16 >> 3) & 7, 6, "0x4EF5 = JMP (d8,A5,Xn) — mode 6");
        assert_eq!(((0x4EF8u16 >> 3) & 7, 0x4EF8 & 7), (7, 0), "abs.w 7/0");
        assert_eq!(((0x4EF9u16 >> 3) & 7, 0x4EF9 & 7), (7, 1), "abs.l 7/1");
        assert_eq!(((0x4EFAu16 >> 3) & 7, 0x4EFA & 7), (7, 2), "(d16,PC) 7/2");
        assert_eq!(((0x4EFBu16 >> 3) & 7, 0x4EFB & 7), (7, 3), "(d8,PC,Xn) 7/3");
    }

    /// The clean SST reference case `4ed6 [JMP (A6)]` (8 cycles) — the F1 `(An)` anchor (the `SetPc(AddrReg)`
    /// direct shape, no compute, no idle). A6 = 851757616 (even); the recipe is `[SetPc(A6), Prefetch,
    /// Prefetch]` reloading the queue at the target. Bus: two FC-6 reads at target / target+2.
    fn setup_jmp_an() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                4_276_774_800,
                2_448_422_008,
                3_033_641_426,
                983_606_231,
                4_289_959_026,
                1_085_040_062,
                1_453_868_004,
                39_294_032,
            ],
            a: [
                3_476_136_870,
                1_596_889_548,
                3_265_597_458,
                415_831_824,
                2_947_717_909,
                4_107_238_674,
                851_757_616,
            ],
            usp: 3_917_039_368,
            ssp: 2048,
            pc: 3072,
            sr: 10000,
            prefetch: [20182, 52296],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (12_896_816u32, 155u8),
            (12_896_817, 243),
            (12_896_818, 171),
            (12_896_819, 24),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jmp_an_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 12_896_816,
                size: Size::Word,
                value: 39923,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 12_896_818,
                size: Size::Word,
                value: 43800,
            },
        ]
    }

    fn assert_jmp_an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 851_757_616, "pc landed at A6 (the target)");
        assert_eq!(cpu.regs.sr, 10000, "SR unchanged (JMP affects no flags)");
        assert_eq!(cpu.regs.a[6], 851_757_616, "A6 unchanged");
        assert_eq!(
            cpu.regs.prefetch,
            [39923, 43800],
            "queue reloaded at target"
        );
        assert_eq!(bus.log, expected_jmp_an_log());
    }

    #[test]
    fn run_instruction_matches_jmp_an() {
        let (mut cpu, mut bus) = setup_jmp_an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 8, "(An) = [SetPc, PF, PF] = 8 (no idle)");
        assert_jmp_an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jmp_an() {
        let (mut rtc, mut bus_rtc) = setup_jmp_an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jmp_an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jmp_an_final(&step, &bus_step);
    }

    #[test]
    fn jmp_an_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the SetPc-direct (An) jump shape.
        let (mut rref, mut bref) = setup_jmp_an();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 3 micro-ops (SetPc, Prefetch, Prefetch) → boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_jmp_an();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SST reference case `4eeb [JMP (d16,A3)]` (10 cycles) — the F1 `(d16,An)` anchor (the
    /// `TargetCalc(AddrReg, ·, DispWord)` + 2-cyc idle shape). A3 = 2998333802, disp = sign_extend16(35870)
    /// → target = 2998304136 (even). Recipe `[TargetCalc, Internal(2), SetPc, PF, PF]`. Bus: n2 (not in the
    /// stream) + two FC-6 reads at target / target+2.
    fn setup_jmp_d16an() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                2_440_929_617,
                3_895_184_752,
                2_817_252_320,
                3_435_608_030,
                3_611_049_164,
                656_369_884,
                3_041_650_526,
                2_428_829_723,
            ],
            a: [
                3_083_582_436,
                3_906_768_230,
                3_638_627_363,
                2_998_333_802,
                170_266_425,
                801_912_058,
                659_845_155,
            ],
            usp: 4_225_240_530,
            ssp: 2048,
            pc: 3072,
            sr: 9989,
            prefetch: [20203, 35870],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (11_959_688u32, 29u8),
            (11_959_689, 93),
            (11_959_690, 47),
            (11_959_691, 154),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn assert_jmp_d16an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 2_998_304_136,
            "pc landed at the computed target"
        );
        assert_eq!(cpu.regs.sr, 9989, "SR unchanged");
        assert_eq!(cpu.regs.prefetch, [7517, 12186], "queue reloaded at target");
        assert_eq!(
            bus.log,
            vec![
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 11_959_688,
                    size: Size::Word,
                    value: 7517,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 11_959_690,
                    size: Size::Word,
                    value: 12186,
                },
            ]
        );
    }

    #[test]
    fn run_instruction_matches_jmp_d16an() {
        let (mut cpu, mut bus) = setup_jmp_d16an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 10,
            "(d16,An) = [TargetCalc, n2, SetPc, PF, PF] = 10"
        );
        assert_jmp_d16an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jmp_d16an() {
        let (mut rtc, mut bus_rtc) = setup_jmp_d16an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jmp_d16an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 10);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jmp_d16an_final(&step, &bus_step);
    }

    /// The clean SST reference case `4ef5 [JMP (d8,A5,Xn)]` (14 cycles) — the F1 indexed `(d8,An,Xn)` anchor
    /// (`TargetCalc(AddrReg, BriefIndex, BriefDisp8)` + 6-cyc index idle). Recipe `[TargetCalc, Internal(6),
    /// SetPc, PF, PF]`. target = 3685257944 (even).
    fn setup_jmp_d8anxn() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                2_257_626_441,
                3_975_271_630,
                886_624_459,
                943_922_647,
                3_001_437_690,
                2_124_450_771,
                3_467_541_282,
                1_346_820_915,
            ],
            a: [
                1_690_064_846,
                3_210_009_965,
                3_062_598_033,
                1_407_409_311,
                3_742_411_135,
                1_560_807_147,
                2_079_579_124,
            ],
            usp: 301_484_952,
            ssp: 2048,
            pc: 3072,
            sr: 10004,
            prefetch: [20213, 23322],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (11_047_640u32, 186u8),
            (11_047_641, 119),
            (11_047_642, 226),
            (11_047_643, 65),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn assert_jmp_d8anxn_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 3_685_257_944,
            "pc landed at the indexed target"
        );
        assert_eq!(
            cpu.regs.prefetch,
            [47735, 57921],
            "queue reloaded at target"
        );
        assert_eq!(
            bus.log,
            vec![
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 11_047_640,
                    size: Size::Word,
                    value: 47735,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 11_047_642,
                    size: Size::Word,
                    value: 57921,
                },
            ]
        );
    }

    #[test]
    fn run_instruction_matches_jmp_d8anxn() {
        let (mut cpu, mut bus) = setup_jmp_d8anxn();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 14,
            "(d8,An,Xn) = [TargetCalc, n6, SetPc, PF, PF] = 14"
        );
        assert_jmp_d8anxn_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jmp_d8anxn() {
        let (mut rtc, mut bus_rtc) = setup_jmp_d8anxn();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jmp_d8anxn();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 14);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jmp_d8anxn_final(&step, &bus_step);
    }

    /// The clean SST reference case `4ef8 [JMP (xxx).w]` (10 cycles) — the F1 `abs.w` anchor
    /// (`TargetCalc(Zero, Zero, DispWord)` + 2-cyc idle). disp 23940 → target 23940 (even, positive).
    fn setup_jmp_absw() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 3_574_960_288,
            ssp: 2048,
            pc: 3072,
            sr: 10009,
            prefetch: [20216, 23940],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [(23_940u32, 50u8), (23_941, 4), (23_942, 174), (23_943, 133)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn assert_jmp_absw_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 23940, "pc landed at abs.w target");
        assert_eq!(
            cpu.regs.prefetch,
            [12804, 44677],
            "queue reloaded at target"
        );
        assert_eq!(
            bus.log,
            vec![
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 23940,
                    size: Size::Word,
                    value: 12804,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 23942,
                    size: Size::Word,
                    value: 44677,
                },
            ]
        );
    }

    #[test]
    fn run_instruction_matches_jmp_absw() {
        let (mut cpu, mut bus) = setup_jmp_absw();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 10, "abs.w = [TargetCalc, n2, SetPc, PF, PF] = 10");
        assert_jmp_absw_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jmp_absw() {
        let (mut rtc, mut bus_rtc) = setup_jmp_absw();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jmp_absw();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 10);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jmp_absw_final(&step, &bus_step);
    }

    /// The clean SST reference case `4efa [JMP (d16,PC)]` (10 cycles) — the F1 `(d16,PC)` anchor
    /// (`TargetCalc(PcOfExt, ·, DispWord)` + 2-cyc idle; the base is the extension-word address pc+2).
    /// disp = sign_extend16(1432); target = 3072+2+1432 = 4506 (even).
    fn setup_jmp_d16pc() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 1_864_689_512,
            ssp: 2048,
            pc: 3072,
            sr: 10001,
            prefetch: [20218, 1432],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [(4506u32, 98u8), (4507, 208), (4508, 175), (4509, 191)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn assert_jmp_d16pc_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 4506, "pc landed at the PC-relative target");
        assert_eq!(
            cpu.regs.prefetch,
            [25296, 44991],
            "queue reloaded at target"
        );
        assert_eq!(
            bus.log,
            vec![
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 4506,
                    size: Size::Word,
                    value: 25296,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 4508,
                    size: Size::Word,
                    value: 44991,
                },
            ]
        );
    }

    #[test]
    fn run_instruction_matches_jmp_d16pc() {
        let (mut cpu, mut bus) = setup_jmp_d16pc();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 10,
            "(d16,PC) = [TargetCalc, n2, SetPc, PF, PF] = 10"
        );
        assert_jmp_d16pc_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jmp_d16pc() {
        let (mut rtc, mut bus_rtc) = setup_jmp_d16pc();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jmp_d16pc();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 10);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jmp_d16pc_final(&step, &bus_step);
    }

    /// The clean SST reference case `4efb [JMP (d8,PC,Xn)]` (14 cycles) — the F1 indexed `(d8,PC,Xn)` anchor
    /// (`TargetCalc(PcOfExt, BriefIndex, BriefDisp8)` + 6-cyc index idle). target = 4294939978 (even — the
    /// target wraps into the high 32-bit range, which the UNMASKED TargetCalc/SetPc preserves).
    fn setup_jmp_d8pcxn() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                830_418_064,
                28_220_045,
                1_685_006_170,
                3_829_560_031,
                4_176_373_181,
                13_688_736,
                892_763_506,
                2_237_689_572,
            ],
            a: [
                4_098_460_053,
                1_425_630_674,
                2_796_607_075,
                764_662_098,
                1_237_758_787,
                4_273_256_745,
                1_735_235_258,
            ],
            usp: 1_644_229_776,
            ssp: 2048,
            pc: 3072,
            sr: 9991,
            prefetch: [20219, 32947],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (16_749_898u32, 246u8),
            (16_749_899, 217),
            (16_749_900, 146),
            (16_749_901, 67),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn assert_jmp_d8pcxn_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 4_294_939_978,
            "pc landed at the indexed PC-relative target (high 32-bit, unmasked)"
        );
        assert_eq!(
            cpu.regs.prefetch,
            [63193, 37443],
            "queue reloaded at target"
        );
        assert_eq!(
            bus.log,
            vec![
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 16_749_898,
                    size: Size::Word,
                    value: 63193,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 16_749_900,
                    size: Size::Word,
                    value: 37443,
                },
            ]
        );
    }

    #[test]
    fn run_instruction_matches_jmp_d8pcxn() {
        let (mut cpu, mut bus) = setup_jmp_d8pcxn();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 14,
            "(d8,PC,Xn) = [TargetCalc, n6, SetPc, PF, PF] = 14"
        );
        assert_jmp_d8pcxn_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jmp_d8pcxn() {
        let (mut rtc, mut bus_rtc) = setup_jmp_d8pcxn();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jmp_d8pcxn();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 14);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jmp_d8pcxn_final(&step, &bus_step);
    }

    /// The clean SST reference case `4ef9 [JMP (xxx).l]` (12 cycles) — the F1 `abs.l` anchor: the two
    /// extension words are assembled into the **UNMASKED** 32-bit target (HIGH = prefetch[1] = 7970, LOW =
    /// the word at pc+4 = 47784) → target 522369704 = 0x1F23_BAE8 (the upper byte 0x1F survives — an EaCalc
    /// mask would wrongly clear it). Recipe `[Combine32(HI), Prefetch (reads pc+4 → the LOW word), Combine32,
    /// SetPc, Prefetch, Prefetch]` — 3 word reads = 12 cyc, no idle. Bus: [r@pc+4, r@target, r@target+2].
    fn setup_jmp_absl() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                2_776_511_107,
                130_207_282,
                3_884_304_188,
                2_647_298_550,
                2_417_870_823,
                498_709_761,
                3_578_953_833,
                2_195_871_101,
            ],
            a: [
                912_905_757,
                767_577_753,
                3_763_895_674,
                1_026_259_002,
                4_024_688_506,
                2_575_493_352,
                1_987_335_656,
            ],
            usp: 2_954_385_806,
            ssp: 2048,
            pc: 3072,
            sr: 10006,
            prefetch: [20217, 7970],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (2_276_008u32, 210u8),
            (2_276_009, 142),
            (2_276_010, 39),
            (2_276_011, 182),
            (3076, 186),
            (3077, 168),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jmp_absl_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076, // the LOW abs.l word, refilled from pc+4
                size: Size::Word,
                value: 47784,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 2_276_008, // the target (masked bus address of 0x1F23_BAE8)
                size: Size::Word,
                value: 53902,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 2_276_010,
                size: Size::Word,
                value: 10166,
            },
        ]
    }

    fn assert_jmp_absl_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 522_369_704,
            "pc landed at the UNMASKED abs.l target (0x1F23_BAE8 — upper byte preserved)"
        );
        assert_eq!(cpu.regs.sr, 10006, "SR unchanged");
        assert_eq!(
            cpu.regs.prefetch,
            [53902, 10166],
            "queue reloaded at target"
        );
        assert_eq!(bus.log, expected_jmp_absl_log());
    }

    #[test]
    fn run_instruction_matches_jmp_absl() {
        let (mut cpu, mut bus) = setup_jmp_absl();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "abs.l = [Combine32, PF, Combine32, SetPc, PF, PF] = 3 reads = 12 (no idle)"
        );
        assert_jmp_absl_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jmp_absl() {
        let (mut rtc, mut bus_rtc) = setup_jmp_absl();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jmp_absl();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jmp_absl_final(&step, &bus_step);
    }

    #[test]
    fn jmp_absl_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the new abs.l unmasked two-ext-word target shape (the HI park, the
        // interleaved LOW refill, the unmasked Combine32, the SetPc reload).
        let (mut rref, mut bref) = setup_jmp_absl();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 6 micro-ops (Combine32, Prefetch, Combine32, SetPc, Prefetch, Prefetch) → boundaries after 0..=5.
        for pause_after in 0..=5 {
            let (mut cpu, mut bus) = setup_jmp_absl();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    // --- F2: BSR — the return-address push (hi @ SP−4, lo @ SP−2) + the SetPc reload. Two anchors: a byte
    // form (617c) and a word form (6100) whose target lands pc with high bits set (proves PC stays unmasked).

    #[test]
    fn bsr_decode_recognizes_cc1_and_disp_form() {
        // BSR is the cc == 1 encoding of the branch family: `0110 0001 dddddddd` (0x61xx). `disp8 == 0` selects
        // the word form (return = pc+4); else the byte form (return = pc+2). 0x61FF is the 68020 long-disp form
        // (an address-error trap on the 68000) — excluded from the in-scope decode.
        assert_eq!(0x617cu16 & 0xFF00, 0x6100, "0x617c is a BSR (cc == 1)");
        assert_eq!(0x617cu16 & 0xFF, 0x7C, "0x617c disp8 = 0x7C (byte form)");
        assert_eq!(0x6100u16 & 0xFF, 0x00, "0x6100 disp8 = 0 (word form)");
        assert_eq!((0x6100u16 >> 8) & 0xF, 1, "0x6100 cc = 1 (BSR)");
        // Bcc excludes cc == 1; BSR is its own arm.
        assert_eq!((0x617cu16 >> 8) & 0xF, 1, "0x617c cc = 1 (BSR — not Bcc)");
    }

    /// The clean SST reference case `617c [BSR Q] 2` (byte form, even target, 18 cycles) — the F2 byte BSR
    /// anchor. opcode 0x617c → byte form, disp8 0x7C = +124; target = pc+2+124 = 3198 (even). The return
    /// address pushed is pc+2 = 3074. SP (ssp 2048, supervisor) pre-decrements by 4 to 2044; the return
    /// address is stored big-endian: hi 0x0000 @ 2044, lo 0x0C02 (3074) @ 2046. The queue reloads at 3198/3200.
    /// Bus: n2 (not in the stream) + [WRITE hi @ SP−4, WRITE lo @ SP−2, READ @ target, READ @ target+2].
    fn setup_bsr_byte() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 2_254_788_228,
            ssp: 2048,
            pc: 3072,
            sr: 9997,
            prefetch: [24956, 64240],
        };
        let mut bus = FlatBus::new();
        // The two target words at 3198 (12842 = 0x322A) and 3200 (44646 = 0xAE66).
        for (a, v) in [(3198u32, 50u8), (3199, 42), (3200, 174), (3201, 102)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_bsr_byte_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044, // hi half @ SP−4
                size: Size::Word,
                value: 0, // high word of the return address 0x0C02
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046, // lo half @ SP−2
                size: Size::Word,
                value: 3074, // low word of the return address (pc + 2)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3198, // queue reload at the target
                size: Size::Word,
                value: 12842,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3200,
                size: Size::Word,
                value: 44646,
            },
        ]
    }

    fn assert_bsr_byte_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3198, "pc landed at the branch target");
        assert_eq!(
            cpu.regs.ssp, 2044,
            "SP pre-decremented by 4 (the long push)"
        );
        assert_eq!(cpu.regs.sr, 9997, "SR unchanged (BSR affects no flags)");
        assert_eq!(
            cpu.regs.prefetch,
            [12842, 44646],
            "queue reloaded at target"
        );
        // The return address is stored big-endian across the two stack halves.
        assert_eq!(bus.peek(2044), 0x00, "return hi byte 0");
        assert_eq!(bus.peek(2045), 0x00, "return hi byte 1");
        assert_eq!(bus.peek(2046), 0x0C, "return lo byte 0 (3074 >> 8)");
        assert_eq!(bus.peek(2047), 0x02, "return lo byte 1 (3074 & 0xFF)");
        assert_eq!(bus.log, expected_bsr_byte_log());
    }

    #[test]
    fn run_instruction_matches_bsr_byte() {
        let (mut cpu, mut bus) = setup_bsr_byte();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 18,
            "byte BSR = n2 + [WRITE.hi, WRITE.lo, PF, PF] = 18"
        );
        assert_bsr_byte_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_bsr_byte() {
        let (mut rtc, mut bus_rtc) = setup_bsr_byte();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_bsr_byte();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 18);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_bsr_byte_final(&step, &bus_step);
    }

    #[test]
    fn bsr_byte_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the new push shape (the AdjustAddr SP−4, the two long-store writes,
        // the SetPc + queue-reload tail) — the whole CPU (incl. the in-flight cursor and its scratch slots: the
        // parked return address, target and lo-half address) round-trips at every micro-op boundary.
        let (mut rref, mut bref) = setup_bsr_byte();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 10 micro-ops (TargetCalc, TargetCalc, Internal(2), AdjustAddr, EaCalc, Write, Write, SetPc, Prefetch,
        // Prefetch) → boundaries after 0..=9.
        for pause_after in 0..=9 {
            let (mut cpu, mut bus) = setup_bsr_byte();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SST reference case `6100 [BSR #] 378` (word form, even target, 18 cycles) — the F2 word BSR
    /// anchor, **and the unmasked-PC anchor**: the word displacement (`prefetch[1]` = 53056 → sign_extend16 →
    /// −12480) sends the branch BACKWARD, landing `pc = 3074 − 12480 = 0xFFFF_DB42` (4294957890) with high bits
    /// set. The reload reads at the masked bus address `0xFF_DB42` (16767810) while `pc` keeps its full 32 bits
    /// — proving `SetPc`/`TargetCalc` do NOT mask (only the bus address `read16` masks). The return address is
    /// pc+4 = 3076 (word form); SP 2048 → 2044, hi 0x0000 @ 2044, lo 0x0C04 (3076) @ 2046.
    fn setup_bsr_word_unmasked() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 3_365_769_692,
            ssp: 2048,
            pc: 3072,
            sr: 10008,
            prefetch: [24832, 53056],
        };
        let mut bus = FlatBus::new();
        // The two target words at the MASKED bus address 0xFF_DB42 (16767810) / +2.
        for (a, v) in [
            (16_767_810u32, 179u8),
            (16_767_811, 251),
            (16_767_812, 65),
            (16_767_813, 153),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_bsr_word_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044,
                size: Size::Word,
                value: 0, // high word of the return address 0x0C04
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3076, // low word of the return address (pc + 4)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16_767_810, // the queue reload at the MASKED bus address (pc stays unmasked)
                size: Size::Word,
                value: 46075,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16_767_812,
                size: Size::Word,
                value: 16793,
            },
        ]
    }

    fn assert_bsr_word_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 4_294_957_890,
            "pc landed at 0xFFFF_DB42 — UNMASKED (high bits survive a backward BSR.w)"
        );
        assert_eq!(cpu.regs.ssp, 2044, "SP pre-decremented by 4");
        assert_eq!(cpu.regs.sr, 10008, "SR unchanged");
        assert_eq!(
            cpu.regs.prefetch,
            [46075, 16793],
            "queue reloaded at target"
        );
        assert_eq!(bus.peek(2044), 0x00, "return hi byte 0");
        assert_eq!(bus.peek(2046), 0x0C, "return lo byte 0 (3076 >> 8)");
        assert_eq!(bus.peek(2047), 0x04, "return lo byte 1 (3076 & 0xFF)");
        assert_eq!(bus.log, expected_bsr_word_log());
    }

    #[test]
    fn run_instruction_matches_bsr_word_unmasked() {
        let (mut cpu, mut bus) = setup_bsr_word_unmasked();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 18,
            "word BSR = n2 + [WRITE.hi, WRITE.lo, PF, PF] = 18"
        );
        assert_bsr_word_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_bsr_word_unmasked() {
        let (mut rtc, mut bus_rtc) = setup_bsr_word_unmasked();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_bsr_word_unmasked();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 18);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_bsr_word_final(&step, &bus_step);
    }

    #[test]
    fn bsr_word_unmasked_quiescable_and_serializable_at_every_micro_op_boundary() {
        let (mut rref, mut bref) = setup_bsr_word_unmasked();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        for pause_after in 0..=9 {
            let (mut cpu, mut bus) = setup_bsr_word_unmasked();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    // --- F3: JSR <control ea> — the seven control modes, the F1 per-mode target combined with the F2 push,
    // via the JSR reload-interleave (read target → push hi/lo → read target+2). One clean (even-target)
    // anchor per mode; the documented (An) anchor (4e90) carries the full both-drivers + snapshot/restore
    // trio, and abs.l carries an extra snapshot/restore anchor for its split-reload + push shape. ---

    #[test]
    fn jsr_decode_recognizes_control_mode_and_reg() {
        // JSR layout: `0100 1110 10 mmm rrr` — bits 15-6 == 0b0100_1110_10 (0x4E80 prefix), mode bits 5-3,
        // reg bits 2-0. The seven control encodings (the SAME set as JMP, only the prefix differs: JMP is
        // 0x4EC0). The opcode space 0x4E80..=0x4EBF is disjoint from JMP (0x4EC0..=0x4EFF).
        assert_eq!(0x4E90u16 & 0xFFC0, 0x4E80, "0x4E90 is a JSR");
        assert_ne!(0x4E90u16 & 0xFFC0, 0x4EC0, "0x4E90 is NOT a JMP");
        assert_eq!((0x4E90u16 >> 3) & 7, 2, "0x4E90 = JSR (A0) — mode 2");
        assert_eq!(0x4E90u16 & 7, 0, "reg A0");
        assert_eq!((0x4EAAu16 >> 3) & 7, 5, "0x4EAA = JSR (d16,A2) — mode 5");
        assert_eq!((0x4EB0u16 >> 3) & 7, 6, "0x4EB0 = JSR (d8,A0,Xn) — mode 6");
        assert_eq!(((0x4EB8u16 >> 3) & 7, 0x4EB8 & 7), (7, 0), "abs.w 7/0");
        assert_eq!(((0x4EB9u16 >> 3) & 7, 0x4EB9 & 7), (7, 1), "abs.l 7/1");
        assert_eq!(((0x4EBAu16 >> 3) & 7, 0x4EBA & 7), (7, 2), "(d16,PC) 7/2");
        assert_eq!(((0x4EBBu16 >> 3) & 7, 0x4EBB & 7), (7, 3), "(d8,PC,Xn) 7/3");
    }

    /// The clean SST reference case `4e90 [JSR (A0)]` (16 cycles) — the documented F3 `(An)` anchor. A0 =
    /// 417170032 (even); the recipe computes the return address (pc+2 = 3074), sets pc = A0, reloads the queue
    /// at the target with the push splitting the two reloads: read @ target → push hi @ SP−4, lo @ SP−2 →
    /// read @ target+2. SP (ssp 2048, supervisor) pre-decrements by 4 to 2044; the return is stored big-endian
    /// (hi 0x0000 @ 2044, lo 0x0C02 (3074) @ 2046). Bus: [r@target, w@SP−4, w@SP−2, r@target+2].
    fn setup_jsr_an() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [
                417_170_032,
                4_256_867_618,
                3_445_037_876,
                920_057_358,
                771_600_133,
                2_194_450_307,
                793_352_992,
            ],
            usp: 2_443_315_286,
            ssp: 2048,
            pc: 3072,
            sr: 9988,
            prefetch: [20112, 43436],
        };
        let mut bus = FlatBus::new();
        // The two target words at A0 (14516848): 0x676E = 26413, 0x7E53 = 32339.
        for (a, v) in [
            (14_516_848u32, 103u8),
            (14_516_849, 45),
            (14_516_850, 126),
            (14_516_851, 83),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jsr_an_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 14_516_848, // the target — read into prefetch[0] (first reload)
                size: Size::Word,
                value: 26413,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044, // hi half @ SP−4
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046, // lo half @ SP−2
                size: Size::Word,
                value: 3074, // return address pc+2
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 14_516_850, // target+2 — read into prefetch[1] (second reload, after the push)
                size: Size::Word,
                value: 32339,
            },
        ]
    }

    fn assert_jsr_an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 417_170_032, "pc landed at A0 (the target)");
        assert_eq!(
            cpu.regs.ssp, 2044,
            "SP pre-decremented by 4 (the long push)"
        );
        assert_eq!(cpu.regs.sr, 9988, "SR unchanged (JSR affects no flags)");
        assert_eq!(cpu.regs.a[0], 417_170_032, "A0 unchanged");
        assert_eq!(
            cpu.regs.prefetch,
            [26413, 32339],
            "queue reloaded at target"
        );
        // The return address is stored big-endian across the two stack halves.
        assert_eq!(bus.peek(2044), 0x00, "return hi byte 0");
        assert_eq!(bus.peek(2045), 0x00, "return hi byte 1");
        assert_eq!(bus.peek(2046), 0x0C, "return lo byte 0 (3074 >> 8)");
        assert_eq!(bus.peek(2047), 0x02, "return lo byte 1 (3074 & 0xFF)");
        assert_eq!(bus.log, expected_jsr_an_log());
    }

    #[test]
    fn run_instruction_matches_jsr_an() {
        let (mut cpu, mut bus) = setup_jsr_an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 16,
            "(An) = [TargetCalc, SetPc, PF, AdjustAddr, EaCalc, WRITE.hi, WRITE.lo, PF] = 4 word accesses = 16"
        );
        assert_jsr_an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jsr_an() {
        let (mut rtc, mut bus_rtc) = setup_jsr_an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jsr_an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 16);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jsr_an_final(&step, &bus_step);
    }

    #[test]
    fn jsr_an_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the new JSR reload-interleave shape (the first reload Prefetch, the
        // AdjustAddr SP−4, the two long-store push writes, the second reload Prefetch) — the whole CPU (incl.
        // the in-flight cursor and its scratch slots: the parked return address, target and lo-half address)
        // round-trips at every micro-op boundary.
        let (mut rref, mut bref) = setup_jsr_an();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 8 micro-ops (TargetCalc, SetPc, Prefetch, AdjustAddr, EaCalc, Write, Write, Prefetch) → 0..=7.
        for pause_after in 0..=7 {
            let (mut cpu, mut bus) = setup_jsr_an();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SST reference case `4eaa [JSR (d16,A2)]` (18 cycles) — the F3 `(d16,An)` anchor. A2 =
    /// 3964968557, disp = sign_extend16(62609) → target 3964965630 (even). Recipe `[TargetCalc(return),
    /// TargetCalc(target), Internal(2), SetPc, PF, AdjustAddr, EaCalc, WRITE.hi, WRITE.lo, PF]`. Return = pc+4.
    fn setup_jsr_d16an() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [
                556_395_796,
                3_396_999_138,
                3_964_968_557,
                1_129_291_849,
                3_672_584_957,
                364_710_320,
                3_874_072_321,
            ],
            usp: 195_702_636,
            ssp: 2048,
            pc: 3072,
            sr: 10005,
            prefetch: [20138, 62609],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (5_542_654u32, 58u8),
            (5_542_655, 156),
            (5_542_656, 111),
            (5_542_657, 51),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jsr_d16an_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5_542_654,
                size: Size::Word,
                value: 15004,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3076, // return address pc+4
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5_542_656,
                size: Size::Word,
                value: 28467,
            },
        ]
    }

    fn assert_jsr_d16an_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 3_964_965_630,
            "pc landed at the computed target"
        );
        assert_eq!(cpu.regs.ssp, 2044, "SP pre-decremented by 4");
        assert_eq!(
            cpu.regs.prefetch,
            [15004, 28467],
            "queue reloaded at target"
        );
        assert_eq!(bus.peek(2046), 0x0C, "return lo byte 0 (3076 >> 8)");
        assert_eq!(bus.peek(2047), 0x04, "return lo byte 1 (3076 & 0xFF)");
        assert_eq!(bus.log, expected_jsr_d16an_log());
    }

    #[test]
    fn run_instruction_matches_jsr_d16an() {
        let (mut cpu, mut bus) = setup_jsr_d16an();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 18, "(d16,An) = n2 + 4 word accesses = 18");
        assert_jsr_d16an_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jsr_d16an() {
        let (mut rtc, mut bus_rtc) = setup_jsr_d16an();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jsr_d16an();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 18);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jsr_d16an_final(&step, &bus_step);
    }

    /// The clean SST reference case `4eb0 [JSR (d8,A0,Xn)]` (22 cycles) — the F3 indexed `(d8,An,Xn)` anchor
    /// (`TargetCalc(AddrReg, BriefIndex, BriefDisp8)` + 6-cyc index idle). target = 1431198278 (even). Return =
    /// pc+4.
    fn setup_jsr_d8anxn() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                3_681_282_897,
                4_184_458_319,
                1_418_104_954,
                866_354_579,
                1_123_123_000,
                13_399_063,
                3_444_713_621,
                3_692_249_412,
            ],
            a: [
                1_431_187_841,
                1_407_832_233,
                3_838_823_993,
                1_485_111_627,
                395_608_082,
                137_588_292,
                2_574_677_191,
            ],
            usp: 1_776_022_178,
            ssp: 2048,
            pc: 3072,
            sr: 9996,
            prefetch: [20144, 25648],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (5_134_918u32, 101u8),
            (5_134_919, 125),
            (5_134_920, 165),
            (5_134_921, 190),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jsr_d8anxn_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5_134_918,
                size: Size::Word,
                value: 25981,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3076,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5_134_920,
                size: Size::Word,
                value: 42430,
            },
        ]
    }

    fn assert_jsr_d8anxn_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 1_431_198_278,
            "pc landed at the indexed target"
        );
        assert_eq!(cpu.regs.ssp, 2044, "SP pre-decremented by 4");
        assert_eq!(
            cpu.regs.prefetch,
            [25981, 42430],
            "queue reloaded at target"
        );
        assert_eq!(bus.log, expected_jsr_d8anxn_log());
    }

    #[test]
    fn run_instruction_matches_jsr_d8anxn() {
        let (mut cpu, mut bus) = setup_jsr_d8anxn();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 22, "(d8,An,Xn) = n6 + 4 word accesses = 22");
        assert_jsr_d8anxn_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jsr_d8anxn() {
        let (mut rtc, mut bus_rtc) = setup_jsr_d8anxn();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jsr_d8anxn();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 22);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jsr_d8anxn_final(&step, &bus_step);
    }

    /// The clean SST reference case `4eb8 [JSR (xxx).w]` (18 cycles) — the F3 `abs.w` anchor
    /// (`TargetCalc(Zero, Zero, DispWord)` + 2-cyc idle). disp 62964 → sign_extend16 → target 4294964724
    /// (even, the high 32-bit range — the UNMASKED target preserves it). Return = pc+4.
    fn setup_jsr_absw() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 1_730_221_114,
            ssp: 2048,
            pc: 3072,
            sr: 10010,
            prefetch: [20152, 62964],
        };
        let mut bus = FlatBus::new();
        // The two target words at the MASKED bus address 16774644 / +2.
        for (a, v) in [
            (16_774_644u32, 114u8),
            (16_774_645, 174),
            (16_774_646, 249),
            (16_774_647, 162),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jsr_absw_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16_774_644,
                size: Size::Word,
                value: 29358,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3076,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16_774_646,
                size: Size::Word,
                value: 63906,
            },
        ]
    }

    fn assert_jsr_absw_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 4_294_964_724,
            "pc landed at the abs.w target (UNMASKED high 32-bit)"
        );
        assert_eq!(cpu.regs.ssp, 2044, "SP pre-decremented by 4");
        assert_eq!(
            cpu.regs.prefetch,
            [29358, 63906],
            "queue reloaded at target"
        );
        assert_eq!(bus.log, expected_jsr_absw_log());
    }

    #[test]
    fn run_instruction_matches_jsr_absw() {
        let (mut cpu, mut bus) = setup_jsr_absw();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 18, "abs.w = n2 + 4 word accesses = 18");
        assert_jsr_absw_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jsr_absw() {
        let (mut rtc, mut bus_rtc) = setup_jsr_absw();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jsr_absw();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 18);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jsr_absw_final(&step, &bus_step);
    }

    /// The clean SST reference case `4eba [JSR (d16,PC)]` (18 cycles) — the F3 `(d16,PC)` anchor
    /// (`TargetCalc(PcOfExt, ·, DispWord)` + 2-cyc idle; base = the extension-word address pc+2). disp 41476 →
    /// sign_extend16 → target 4294946310 (even). Return = pc+4.
    fn setup_jsr_d16pc() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 3_647_737_790,
            ssp: 2048,
            pc: 3072,
            sr: 9988,
            prefetch: [20154, 41476],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (16_756_230u32, 253u8),
            (16_756_231, 85),
            (16_756_232, 176),
            (16_756_233, 150),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jsr_d16pc_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16_756_230,
                size: Size::Word,
                value: 64853,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3076,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16_756_232,
                size: Size::Word,
                value: 45206,
            },
        ]
    }

    fn assert_jsr_d16pc_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 4_294_946_310,
            "pc landed at the PC-relative target (UNMASKED)"
        );
        assert_eq!(cpu.regs.ssp, 2044, "SP pre-decremented by 4");
        assert_eq!(
            cpu.regs.prefetch,
            [64853, 45206],
            "queue reloaded at target"
        );
        assert_eq!(bus.log, expected_jsr_d16pc_log());
    }

    #[test]
    fn run_instruction_matches_jsr_d16pc() {
        let (mut cpu, mut bus) = setup_jsr_d16pc();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 18, "(d16,PC) = n2 + 4 word accesses = 18");
        assert_jsr_d16pc_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jsr_d16pc() {
        let (mut rtc, mut bus_rtc) = setup_jsr_d16pc();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jsr_d16pc();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 18);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jsr_d16pc_final(&step, &bus_step);
    }

    /// The clean SST reference case `4ebb [JSR (d8,PC,Xn)]` (22 cycles) — the F3 indexed `(d8,PC,Xn)` anchor
    /// (`TargetCalc(PcOfExt, BriefIndex, BriefDisp8)` + 6-cyc index idle). target = 244402988 (even). Return =
    /// pc+4.
    fn setup_jsr_d8pcxn() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                1_628_921_814,
                3_676_879_806,
                1_656_373_399,
                3_319_941_202,
                1_365_186_904,
                1_325_478_389,
                674_272_657,
                188_275_555,
            ],
            a: [
                1_984_754_996,
                2_622_389_335,
                244_399_998,
                2_066_504_890,
                2_322_948_911,
                2_073_098_832,
                3_259_026_008,
            ],
            usp: 700_838_964,
            ssp: 2048,
            pc: 3072,
            sr: 9986,
            prefetch: [20155, 43692],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (9_521_964u32, 164u8),
            (9_521_965, 72),
            (9_521_966, 77),
            (9_521_967, 253),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jsr_d8pcxn_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 9_521_964,
                size: Size::Word,
                value: 42056,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3076,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 9_521_966,
                size: Size::Word,
                value: 19965,
            },
        ]
    }

    fn assert_jsr_d8pcxn_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 244_402_988,
            "pc landed at the indexed PC-relative target"
        );
        assert_eq!(cpu.regs.ssp, 2044, "SP pre-decremented by 4");
        assert_eq!(
            cpu.regs.prefetch,
            [42056, 19965],
            "queue reloaded at target"
        );
        assert_eq!(bus.log, expected_jsr_d8pcxn_log());
    }

    #[test]
    fn run_instruction_matches_jsr_d8pcxn() {
        let (mut cpu, mut bus) = setup_jsr_d8pcxn();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 22, "(d8,PC,Xn) = n6 + 4 word accesses = 22");
        assert_jsr_d8pcxn_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jsr_d8pcxn() {
        let (mut rtc, mut bus_rtc) = setup_jsr_d8pcxn();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jsr_d8pcxn();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 22);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jsr_d8pcxn_final(&step, &bus_step);
    }

    /// The clean SST reference case `4eb9 [JSR (xxx).l]` (20 cycles) — the F3 `abs.l` anchor: the two
    /// extension words are assembled into the UNMASKED 32-bit target (HIGH = prefetch[1] = 58874, LOW = the
    /// word at pc+4 = 39930) → target 3858406394 = 0xE5FB_963A (high byte set — an EaCalc mask would wrongly
    /// clear it). **Return = pc+6** (VERIFIED against the pushed value 3078 in the data — the recon prose said
    /// pc+4; the DATA WINS). Recipe `[TargetCalc(return), Combine32(HI), Prefetch (LOW word @pc+4), Combine32,
    /// SetPc, Prefetch (target), AdjustAddr, EaCalc, WRITE.hi, WRITE.lo, Prefetch (target+2)]`. Bus:
    /// [r@pc+4, r@target, w@SP−4, w@SP−2, r@target+2] = 5 word accesses = 20 cyc, no idle.
    fn setup_jsr_absl() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 3_372_930_788,
            ssp: 2048,
            pc: 3072,
            sr: 9987,
            prefetch: [20153, 58874],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (16_423_930u32, 150u8),
            (16_423_931, 130),
            (16_423_932, 31),
            (16_423_933, 171),
            (3076, 155),
            (3077, 250),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_jsr_absl_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076, // the LOW abs.l word, refilled from pc+4
                size: Size::Word,
                value: 39930,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16_423_930, // the target (masked bus address of 0xE5FB_963A)
                size: Size::Word,
                value: 38530,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044,
                size: Size::Word,
                value: 0, // high word of the return address pc+6 = 3078
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3078, // low word of the return address (pc + 6 — abs.l is 3 words)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16_423_932,
                size: Size::Word,
                value: 8107,
            },
        ]
    }

    fn assert_jsr_absl_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 3_858_406_394,
            "pc landed at the UNMASKED abs.l target (0xE5FB_963A — high byte preserved)"
        );
        assert_eq!(cpu.regs.ssp, 2044, "SP pre-decremented by 4");
        assert_eq!(cpu.regs.sr, 9987, "SR unchanged");
        assert_eq!(cpu.regs.prefetch, [38530, 8107], "queue reloaded at target");
        // The return address pc+6 = 3078 is stored big-endian.
        assert_eq!(bus.peek(2046), 0x0C, "return lo byte 0 (3078 >> 8)");
        assert_eq!(bus.peek(2047), 0x06, "return lo byte 1 (3078 & 0xFF)");
        assert_eq!(bus.log, expected_jsr_absl_log());
    }

    #[test]
    fn run_instruction_matches_jsr_absl() {
        let (mut cpu, mut bus) = setup_jsr_absl();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 20,
            "abs.l = [r@pc+4, r@target, w@SP−4, w@SP−2, r@target+2] = 5 word accesses = 20 (no idle)"
        );
        assert_jsr_absl_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_jsr_absl() {
        let (mut rtc, mut bus_rtc) = setup_jsr_absl();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_jsr_absl();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 20);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_jsr_absl_final(&step, &bus_step);
    }

    #[test]
    fn jsr_absl_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the JSR abs.l shape — the two-ext-word UNMASKED target assembly
        // (HI park, the interleaved LOW refill, the unmasked Combine32) interleaved with the split-reload push
        // (the first reload Prefetch, the AdjustAddr SP−4, the two push writes, the second reload Prefetch).
        let (mut rref, mut bref) = setup_jsr_absl();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 11 micro-ops (TargetCalc, Combine32, Prefetch, Combine32, SetPc, Prefetch, AdjustAddr, EaCalc,
        // Write, Write, Prefetch) → boundaries after 0..=10.
        for pause_after in 0..=10 {
            let (mut cpu, mut bus) = setup_jsr_absl();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }

    /// The clean SST reference case `4e75 [RTS] 1` (16 cycles) — the documented F4 anchor. SP (ssp 2048,
    /// supervisor) holds the 32-bit return address big-endian: hi 0x7576 @ 2048, lo 0xAC32 @ 2050. The recipe
    /// pops it (r@SP, r@SP+2), post-increments SP by 4 (→ 2052), assembles the UNMASKED target 0x7576AC32
    /// (= 1970711602), sets pc, and reloads the queue at the target — the bus reload masks to 0x76AC32
    /// (= 7777330): r@7777330, r@7777332. Bus: [r@SP, r@SP+2, r@target, r@target+2]; final.pc is the full
    /// unmasked 0x7576AC32. SR is unchanged (RTS affects no flags).
    fn setup_rts() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 3_185_716_490,
            ssp: 2048,
            pc: 3072,
            sr: 10013,
            prefetch: [20085, 24192], // prefetch[0] = 0x4E75 (RTS)
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The return address on the stack: hi 0x7576 @ SP=2048, lo 0xAC32 @ SP+2=2050.
            (2048u32, 117u8),
            (2049, 118),
            (2050, 172),
            (2051, 50),
            // The two target words at 0x76AC32 (7777330) / +2: 0xE2AB = 58027, 0xA564 = 42340.
            (7_777_330, 226),
            (7_777_331, 171),
            (7_777_332, 165),
            (7_777_333, 100),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_rts_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2048, // hi word of the return address @ SP
                size: Size::Word,
                value: 30070, // 0x7576
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2050, // lo word @ SP+2
                size: Size::Word,
                value: 44082, // 0xAC32
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 7_777_330, // target (masked) — read into prefetch[0]
                size: Size::Word,
                value: 58027,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 7_777_332, // target+2 (masked) — read into prefetch[1]
                size: Size::Word,
                value: 42340,
            },
        ]
    }

    fn assert_rts_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 1_970_711_602,
            "pc landed at the FULL unmasked popped target 0x7576AC32"
        );
        assert_eq!(
            cpu.regs.ssp, 2052,
            "SP post-incremented by 4 (the long pop)"
        );
        assert_eq!(cpu.regs.sr, 10013, "SR unchanged (RTS affects no flags)");
        assert_eq!(cpu.regs.usp, 3_185_716_490, "usp untouched");
        assert_eq!(
            cpu.regs.prefetch,
            [58027, 42340],
            "queue reloaded at the (masked) target"
        );
        assert_eq!(bus.log, expected_rts_log());
    }

    #[test]
    fn run_instruction_matches_rts() {
        let (mut cpu, mut bus) = setup_rts();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 16,
            "RTS = [Read.hi, EaCalc, Read.lo, AdjustAddr, Combine32, SetPc, PF, PF] = 4 word reads = 16"
        );
        assert_rts_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_rts() {
        let (mut rtc, mut bus_rtc) = setup_rts();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_rts();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 16);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_rts_final(&step, &bus_step);
    }

    #[test]
    fn rts_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the RTS pop shape — the long pop (two stack reads + Combine32), the
        // SP post-increment, and the SetPc + two-Prefetch queue reload — the whole CPU (incl. the in-flight
        // cursor and its scratch slots: the popped hi/lo words, the lo-half address, the assembled target)
        // round-trips at every micro-op boundary.
        let (mut rref, mut bref) = setup_rts();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 8 micro-ops (Read, EaCalc, Read, AdjustAddr, Combine32, SetPc, Prefetch, Prefetch) → 0..=7.
        for pause_after in 0..=7 {
            let (mut cpu, mut bus) = setup_rts();
            cpu.start_instruction();
            for _ in 0..pause_after {
                assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
            }
            let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
            let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
            loop {
                if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                    break;
                }
            }
            assert_eq!(
                cpu2.regs, rref.regs,
                "resume from boundary {pause_after} diverged"
            );
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from boundary {pause_after} diverged"
            );
        }
    }
}
